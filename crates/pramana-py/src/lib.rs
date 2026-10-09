//! Python bindings for the PRAMANA estimation core.
//!
//! Deliberately thin. Every function here marshals arguments, calls into the Rust crates
//! and marshals results back; none of them decide anything. Business logic living in a
//! binding layer is a bug, because it would be invisible to the Rust test suite and to the
//! verification harness.
//!
//! The GIL is released around the expensive calls so that a prefork worker pool behaves.

use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_ecdlp::ShorEcdlpRNSL;
use pramana_circuit::shor_ecdlp_2026::{Optimisation, ShorEcdlp2026};
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::shor_factoring_g25::ShorFactoringG25;
use pramana_circuit::grover_symmetric::GroverSymmetric;
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};
use pramana_hardware::fit::{fit_capability_blended, fit_slip, ratio_summary};
use pramana_hardware::roadmap::{Modality, Roadmaps};
use pramana_hardware::trajectory::Trajectory;
use pramana_qec::cat::RepetitionCat;
use pramana_qec::magic::{Cultivation, FifteenToOneTwoLevel};
use pramana_qec::model::{HardwareParams, QecArchitecture, QecInput};
use pramana_qec::surface::{Layout, SurfaceCode};
use pramana_risk::exposure::{score, Criticality, DataClass};
use pramana_risk::improvement::{Floors, ImprovementModel, ProblemFamily};
use pramana_risk::montecarlo::{
    break_year_distribution, ArchitectureOption, AttackProfile, AttackerBudget, MonteCarloConfig,
};
use pramana_risk::mosca::{resolve, MoscaInputs, ThreatMode};
use pramana_units::{ErrorRate, LogicalQubits};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Parse a target description into a [`CryptoTarget`].
fn parse_target(kind: &str, size: u32) -> PyResult<CryptoTarget> {
    match kind {
        "rsa" => Ok(CryptoTarget::Rsa { modulus_bits: size }),
        "ecdlp" | "ecc" => Ok(CryptoTarget::Ecdlp { curve_bits: size }),
        "ffdlp" | "dh" | "dsa" => Ok(CryptoTarget::FiniteFieldDlp { prime_bits: size }),
        "symmetric" | "aes" => Ok(CryptoTarget::Symmetric { key_bits: size }),
        other => Err(PyValueError::new_err(format!(
            "unknown target kind '{other}'; expected rsa, ecdlp, ffdlp or symmetric"
        ))),
    }
}

/// Synthesise an attack circuit and return its measured logical resources.
///
/// `pipeline` may be `auto`, `ge19`, `g25`, `rnsl` or `grover`.
#[pyfunction]
#[pyo3(signature = (kind, size, pipeline = "auto"))]
fn synthesise(py: Python<'_>, kind: &str, size: u32, pipeline: &str) -> PyResult<PyObject> {
    let target = parse_target(kind, size)?;
    let opts = SynthesisOptions::default();

    let result = py.allow_threads(|| -> Result<_, String> {
        let attempt = |p: &str| -> Result<pramana_circuit::pipeline::SynthesisResult, String> {
            match p {
                "ge19" => ShorFactoringGE19.synthesise(&target, &opts).map_err(|e| e.to_string()),
                "g25" => ShorFactoringG25.synthesise(&target, &opts).map_err(|e| e.to_string()),
                "rnsl" => ShorEcdlpRNSL.synthesise(&target, &opts).map_err(|e| e.to_string()),
                "ecdlp2026" => ShorEcdlp2026::generic(Optimisation::Gate)
                    .synthesise(&target, &opts)
                    .map_err(|e| e.to_string()),
                "secp256k1" => ShorEcdlp2026::secp256k1(Optimisation::Gate)
                    .synthesise(&target, &opts)
                    .map_err(|e| e.to_string()),
                "grover" => GroverSymmetric.synthesise(&target, &opts).map_err(|e| e.to_string()),
                other => Err(format!("unknown pipeline '{other}'")),
            }
        };
        if pipeline != "auto" {
            return attempt(pipeline);
        }
        // Auto: prefer the most recent construction that handles this target.
        match target {
            CryptoTarget::Rsa { .. } => attempt("g25").or_else(|_| attempt("ge19")),
            CryptoTarget::FiniteFieldDlp { .. } => attempt("ge19"),
            // Prefer the 2026 construction. The 2017 one costs roughly 1000x more gates
            // and using it would understate the attacker, which for a triage tool is the
            // dangerous direction of error.
            CryptoTarget::Ecdlp { .. } => attempt("ecdlp2026").or_else(|_| attempt("rnsl")),
            CryptoTarget::Symmetric { .. } => attempt("grover"),
        }
    });

    let r = result.map_err(PyRuntimeError::new_err)?;
    let d = PyDict::new(py);
    d.set_item("pipeline", r.pipeline)?;
    d.set_item("logical_qubits", r.circuit.logical_qubits.get())?;
    d.set_item("data_qubits", r.circuit.data_qubits.get())?;
    d.set_item("toffoli", r.circuit.resources.total_toffoli().get() as f64)?;
    d.set_item("reaction_depth", r.circuit.resources.reaction_depth)?;
    d.set_item("expected_runs", r.repetitions.expected_runs)?;
    d.set_item("repetition_rationale", r.repetitions.rationale)?;
    d.set_item("toffoli_all_runs", r.total_toffoli_all_runs())?;
    Ok(d.into())
}

/// Build the architecture set the risk engine samples from.
fn architecture_options() -> Vec<ArchitectureOption> {
    vec![
        ArchitectureOption {
            name: "surface + distillation".into(),
            architecture: Box::new(SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)),
            hardware: HardwareParams::gidney_superconducting(),
            physical_per_logical: 105.0,
            weight: 1.0,
        },
        ArchitectureOption {
            name: "surface + cultivation".into(),
            architecture: Box::new(SurfaceCode::new(Layout::Intermediate, Cultivation)),
            hardware: HardwareParams::gidney_superconducting(),
            physical_per_logical: 105.0,
            weight: 1.0,
        },
        ArchitectureOption {
            name: "repetition cat".into(),
            architecture: Box::new(RepetitionCat::new(2.0, Cultivation)),
            hardware: HardwareParams::gouzien_cat(),
            physical_per_logical: 4.7,
            weight: 1.0,
        },
    ]
}

/// Cost a logical demand under one architecture.
#[pyfunction]
#[pyo3(signature = (logical_qubits, toffoli, architecture = "surface", target_total_error = 0.05))]
fn estimate_qec(
    py: Python<'_>,
    logical_qubits: u64,
    toffoli: f64,
    architecture: &str,
    target_total_error: f64,
) -> PyResult<PyObject> {
    let opts = architecture_options();
    let opt = opts
        .iter()
        .find(|o| o.name.starts_with(architecture) || o.name == architecture)
        .ok_or_else(|| {
            PyValueError::new_err(format!(
                "unknown architecture '{architecture}'; available: {}",
                opts.iter().map(|o| o.name.as_str()).collect::<Vec<_>>().join(", ")
            ))
        })?;

    let input = QecInput {
        logical_qubits: LogicalQubits::new(logical_qubits),
        toffoli_count: toffoli as u128,
        reaction_depth: toffoli as u64,
        target_total_error: ErrorRate::new(target_total_error)
            .map_err(|e| PyValueError::new_err(e.to_string()))?,
        idle_fraction: 0.0,
    };

    let est = py
        .allow_threads(|| opt.architecture.estimate(&input, &opt.hardware))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let d = PyDict::new(py);
    d.set_item("architecture", opt.name.as_str())?;
    d.set_item("physical_qubits", est.physical_qubits.get())?;
    d.set_item("wall_clock_seconds", est.wall_clock.get())?;
    d.set_item("wall_clock_human", est.wall_clock.humanise())?;
    d.set_item("code_distance", est.code_distance.map(|x| x.get()))?;
    d.set_item("limiting_factor", format!("{:?}", est.limiting_factor))?;
    d.set_item("data_qubits", est.breakdown.data)?;
    d.set_item("routing_qubits", est.breakdown.routing)?;
    d.set_item("factory_qubits", est.breakdown.factories)?;
    d.set_item("warnings", est.warnings)?;
    Ok(d.into())
}

/// List the registered architectures.
#[pyfunction]
fn list_architectures(py: Python<'_>) -> PyResult<Vec<PyObject>> {
    architecture_options()
        .iter()
        .map(|o| {
            let d = PyDict::new(py);
            d.set_item("name", o.name.as_str())?;
            d.set_item("id", o.architecture.id())?;
            d.set_item("display_name", o.architecture.display_name())?;
            d.set_item("physical_per_logical", o.physical_per_logical)?;
            Ok(d.into())
        })
        .collect()
}

fn load_trajectory(roadmap_dir: &str) -> Result<Trajectory, String> {
    let roadmaps =
        Roadmaps::load_dir(std::path::Path::new(roadmap_dir)).map_err(|e| e.to_string())?;
    let slip = fit_slip(&roadmaps);
    let fit = fit_capability_blended(&roadmaps, &slip).map_err(|e| e.to_string())?;
    Ok(Trajectory::new(fit, slip))
}

/// Summarise the fitted hardware trajectory.
#[pyfunction]
#[pyo3(signature = (roadmap_dir = "data/roadmaps"))]
fn trajectory_summary(py: Python<'_>, roadmap_dir: &str) -> PyResult<PyObject> {
    let t = load_trajectory(roadmap_dir).map_err(PyRuntimeError::new_err)?;
    let roadmaps = Roadmaps::load_dir(std::path::Path::new(roadmap_dir))
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let d = PyDict::new(py);
    d.set_item("growth_per_year", t.fit.growth_per_year())?;
    d.set_item("residual_sd_dex", t.fit.residual_sd)?;
    d.set_item("points", t.fit.points)?;
    d.set_item("first_year", t.fit.first_year)?;
    d.set_item("last_year", t.fit.last_year)?;
    d.set_item("slip_mean_years", t.slip.mean_years)?;
    d.set_item("slip_observed", t.slip.observed)?;
    d.set_item("slip_censored", t.slip.censored)?;
    d.set_item("slip_warnings", t.slip.warnings.clone())?;

    let ratios = PyDict::new(py);
    for m in [
        Modality::Superconducting,
        Modality::TrappedIon,
        Modality::NeutralAtom,
        Modality::Photonic,
        Modality::Bosonic,
        Modality::Silicon,
    ] {
        if let Some(s) = ratio_summary(&roadmaps, m) {
            let e = PyDict::new(py);
            e.set_item("geometric_mean", s.geometric_mean)?;
            e.set_item("min", s.min)?;
            e.set_item("max", s.max)?;
            e.set_item("count", s.count)?;
            ratios.set_item(m.name(), e)?;
        }
    }
    d.set_item("physical_per_logical", ratios)?;

    let projection = PyDict::new(py);
    for year in [2030u32, 2035, 2040, 2045] {
        projection.set_item(year, t.fit.median_logical_qubits(year as f64))?;
    }
    d.set_item("median_logical_qubits", projection)?;
    Ok(d.into())
}

#[allow(clippy::too_many_arguments)]
fn family_for(target: &CryptoTarget) -> (ProblemFamily, Floors, ImprovementModel, ImprovementModel) {
    match target {
        CryptoTarget::Rsa { modulus_bits } | CryptoTarget::FiniteFieldDlp { prime_bits: modulus_bits } => (
            ProblemFamily::Factoring,
            Floors::factoring(*modulus_bits),
            ImprovementModel::factoring_gates(),
            ImprovementModel::factoring_width(),
        ),
        CryptoTarget::Ecdlp { curve_bits } => (
            ProblemFamily::Ecdlp,
            Floors::ecdlp(*curve_bits),
            ImprovementModel::ecdlp_gates(),
            ImprovementModel::ecdlp_width(),
        ),
        CryptoTarget::Symmetric { .. } => (
            ProblemFamily::Symmetric,
            Floors::none(),
            ImprovementModel::symmetric(),
            ImprovementModel::symmetric(),
        ),
    }
}

/// Run the whole chain for one asset: synthesis, costing, trajectory, break year, Mosca,
/// exposure score.
///
/// This is the call the backend makes per asset.
#[pyfunction]
#[pyo3(signature = (
    kind, size, migration_years, secrecy_years,
    threat_mode = "confidentiality", criticality = "medium", data_class = "internal",
    samples = 4000, seed = 0x5052414D414E41u64, roadmap_dir = "data/roadmaps",
    attacker = "nation_state", pipeline = "auto"
))]
fn assess_asset(
    py: Python<'_>,
    kind: &str,
    size: u32,
    migration_years: f64,
    secrecy_years: f64,
    threat_mode: &str,
    criticality: &str,
    data_class: &str,
    samples: usize,
    seed: u64,
    roadmap_dir: &str,
    attacker: &str,
    pipeline: &str,
) -> PyResult<PyObject> {
    let target = parse_target(kind, size)?;
    let mode = match threat_mode {
        "confidentiality" => ThreatMode::Confidentiality,
        "authenticity" => ThreatMode::Authenticity,
        "both" => ThreatMode::Both,
        o => return Err(PyValueError::new_err(format!("unknown threat mode '{o}'"))),
    };
    let crit = match criticality {
        "low" => Criticality::Low,
        "medium" => Criticality::Medium,
        "high" => Criticality::High,
        "critical" => Criticality::Critical,
        o => return Err(PyValueError::new_err(format!("unknown criticality '{o}'"))),
    };
    let dc = match data_class {
        "public" => DataClass::Public,
        "internal" => DataClass::Internal,
        "confidential" => DataClass::Confidential,
        "restricted" => DataClass::Restricted,
        o => return Err(PyValueError::new_err(format!("unknown data class '{o}'"))),
    };
    let budget = match attacker {
        "nation_state" => AttackerBudget::nation_state(),
        "opportunistic" => AttackerBudget::opportunistic(),
        o => return Err(PyValueError::new_err(format!("unknown attacker profile '{o}'"))),
    };

    let opts = SynthesisOptions::default();
    let trajectory = load_trajectory(roadmap_dir).map_err(PyRuntimeError::new_err)?;

    let outcome = py.allow_threads(|| -> Result<_, String> {
        let synth = match (pipeline, &target) {
            ("auto", CryptoTarget::Rsa { .. }) => ShorFactoringG25
                .synthesise(&target, &opts)
                .or_else(|_| ShorFactoringGE19.synthesise(&target, &opts)),
            ("auto", CryptoTarget::Ecdlp { .. }) => ShorEcdlp2026::generic(Optimisation::Gate)
                .synthesise(&target, &opts)
                .or_else(|_| ShorEcdlpRNSL.synthesise(&target, &opts)),
            ("auto", CryptoTarget::Symmetric { .. }) => GroverSymmetric.synthesise(&target, &opts),
            ("auto", CryptoTarget::FiniteFieldDlp { .. }) => {
                ShorFactoringGE19.synthesise(&target, &opts)
            }
            ("ge19", _) => ShorFactoringGE19.synthesise(&target, &opts),
            ("g25", _) => ShorFactoringG25.synthesise(&target, &opts),
            ("rnsl", _) => ShorEcdlpRNSL.synthesise(&target, &opts),
            ("ecdlp2026", _) => ShorEcdlp2026::generic(Optimisation::Gate).synthesise(&target, &opts),
            ("secp256k1", _) => ShorEcdlp2026::secp256k1(Optimisation::Gate).synthesise(&target, &opts),
            ("grover", _) => GroverSymmetric.synthesise(&target, &opts),
            (o, _) => return Err(format!("unknown pipeline '{o}'")),
        }
        .map_err(|e| e.to_string())?;

        let (family, floors, gate_imp, width_imp) = family_for(&target);
        let profile = AttackProfile {
            logical_qubits: synth.circuit.logical_qubits.get(),
            toffoli: synth.total_toffoli_all_runs(),
            family,
            floors,
            gate_improvement: gate_imp,
            width_improvement: width_imp,
        };
        let config = MonteCarloConfig {
            samples,
            seed,
            ..Default::default()
        };
        let dist = break_year_distribution(
            &profile,
            &architecture_options(),
            &trajectory,
            budget,
            config,
        );
        let inputs = MoscaInputs {
            migration_years,
            secrecy_lifetime_years: secrecy_years,
            threat_mode: mode,
            reference_year: config.first_year,
        };
        let res = resolve(&inputs, &dist);
        let s = score(&res, crit, dc);
        Ok((synth, dist, res, s))
    });

    let (synth, dist, res, s) = outcome.map_err(PyRuntimeError::new_err)?;

    let d = PyDict::new(py);
    d.set_item("target", format!("{kind}-{size}"))?;
    d.set_item("pipeline", synth.pipeline)?;
    d.set_item("logical_qubits", synth.circuit.logical_qubits.get())?;
    d.set_item("toffoli_all_runs", synth.total_toffoli_all_runs())?;

    let q = PyDict::new(py);
    for (label, quant) in [
        ("p05", 0.05),
        ("p10", 0.10),
        ("p25", 0.25),
        ("p50", 0.50),
        ("p75", 0.75),
        ("p90", 0.90),
        ("p95", 0.95),
    ] {
        q.set_item(label, dist.quantile(quant))?;
    }
    d.set_item("break_year", q)?;
    d.set_item("never_fraction", dist.never_fraction())?;
    let hist = PyDict::new(py);
    for (year, count) in dist.histogram() {
        hist.set_item(year, count)?;
    }
    d.set_item("break_year_histogram", hist)?;
    d.set_item("median_standard_error", dist.median_standard_error())?;

    let by_arch = PyDict::new(py);
    for (name, median) in &dist.by_architecture {
        by_arch.set_item(name, *median)?;
    }
    d.set_item("break_year_by_architecture", by_arch)?;

    d.set_item("deadline_year", res.deadline_year)?;
    d.set_item("probability_exposed", res.probability_exposed)?;
    d.set_item("probability_standard_error", res.probability_standard_error)?;
    d.set_item("harvest_now_decrypt_later_year", res.harvest_now_decrypt_later_year)?;
    d.set_item("already_exposed", res.already_exposed)?;

    d.set_item("exposure_score", s.score)?;
    d.set_item("exposure_lower", s.lower)?;
    d.set_item("exposure_upper", s.upper)?;
    d.set_item("exposure_formula", s.formula)?;

    let mut warnings = res.warnings.clone();
    warnings.extend(dist.warnings.clone());
    d.set_item("warnings", warnings)?;

    // Provenance, so the API can satisfy Law 2 without reconstructing anything.
    let prov = PyDict::new(py);
    prov.set_item("circuit_pipeline", synth.pipeline)?;
    prov.set_item("expected_runs", synth.repetitions.expected_runs)?;
    prov.set_item("trajectory_growth_per_year", trajectory.fit.growth_per_year())?;
    prov.set_item("trajectory_points", trajectory.fit.points)?;
    prov.set_item("slip_mean_years", trajectory.slip.mean_years)?;
    prov.set_item("monte_carlo_samples", samples)?;
    prov.set_item("seed", seed)?;
    d.set_item("provenance", prov)?;

    Ok(d.into())
}

/// The PRAMANA estimation core.
#[pymodule]
fn pramana(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(synthesise, m)?)?;
    m.add_function(wrap_pyfunction!(estimate_qec, m)?)?;
    m.add_function(wrap_pyfunction!(list_architectures, m)?)?;
    m.add_function(wrap_pyfunction!(trajectory_summary, m)?)?;
    m.add_function(wrap_pyfunction!(assess_asset, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
