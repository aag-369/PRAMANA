//! PRAMANA reproduction harness.
//!
//! Runs every published figure in `verification/golden/` through the *production*
//! estimation path and reports how PRAMANA compares. Failures are accompanied by a
//! discrepancy diagnosis, because a failure with a diagnosis is a research finding
//! while a failure without one is only a bug report (spec §9.3).
//!
//! The harness never tunes anything. Its job is to state, precisely and citably, where
//! PRAMANA agrees with the literature and where it does not.

mod diagnose;
mod golden;
mod published;
mod report;

use golden::{Layer, Target};
use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_ecdlp::ShorEcdlpRNSL;
use pramana_circuit::shor_ecdlp_2026::{Optimisation, ShorEcdlp2026};
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::shor_factoring_g25::{toffoli_per_factoring, ShorFactoringG25};
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};
use pramana_qec::cat::RepetitionCat;
use pramana_qec::magic::{Cultivation, FifteenToOneTwoLevel};
use pramana_qec::model::{HardwareParams, QecArchitecture, QecInput};
use pramana_qec::gidney2025::{Gidney2025Layout, LogicalErrorModel};
use pramana_qec::surface::{Layout, SurfaceCode};
use pramana_units::ErrorRate;
use report::{Comparison, Outcome, TargetReport};
use std::path::PathBuf;

/// Targets that may never be waived: a waiver here fails the build unconditionally.
const UNWAIVABLE: &[&str] = &["gidney_2025_rsa2048"];

fn main() -> std::process::ExitCode {
    let root = std::env::var("PRAMANA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let golden_dir = root.join("verification/golden");
    let report_dir = root.join("verification/reports");

    let targets = match golden::load_dir(&golden_dir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error loading golden files: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let reports: Vec<TargetReport> = targets.iter().map(evaluate).collect();
    let hard_failures = reports
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Fail))
        .count();
    let moved_probes: Vec<&TargetReport> = reports
        .iter()
        .filter(|r| r.outcome == Outcome::ProbeUnexpectedlyPassed)
        .collect();
    let illegal_waivers: Vec<&TargetReport> = reports
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Waived) && UNWAIVABLE.contains(&r.id.as_str()))
        .collect();

    let md = report::render_markdown(&reports);
    print!("{md}");

    let _ = std::fs::create_dir_all(&report_dir);
    let _ = std::fs::write(report_dir.join("report.md"), &md);
    if let Ok(j) = serde_json::to_string_pretty(&reports) {
        let _ = std::fs::write(report_dir.join("report.json"), j);
    }

    for r in &illegal_waivers {
        eprintln!("FATAL: target '{}' may not be waived", r.id);
    }
    for r in &moved_probes {
        eprintln!(
            "NOTE: sensitivity probe '{}' now passes; what it probes has moved and the \
             probe should be re-examined",
            r.id
        );
    }

    if !illegal_waivers.is_empty() || hard_failures > 0 {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

/// Run one golden target through the production estimation path.
fn evaluate(t: &Target) -> TargetReport {
    let mut report = TargetReport::new(t);

    if t.waived {
        report.outcome = Outcome::Waived;
        report.diagnosis = t
            .waiver_reason
            .clone()
            .unwrap_or_else(|| "waived without a stated reason".into());
        return report;
    }

    let opts = SynthesisOptions::default();

    // Physical-layer targets run the full chain: synthesise the circuit, then cost its
    // error correction. Architectures beyond the plain surface code are not built yet,
    // so those targets report as pending rather than passing or failing.
    if t.layer == Layer::Physical {
        // Gidney 2025 three-region layout, under either error curve.
        if let Some(model) = match t.architecture.as_deref() {
            Some("gidney_2025_source_calibration") => Some(LogicalErrorModel::Gidney2025Simulated),
            Some("gidney_2025_generic_fit") => Some(LogicalErrorModel::default()),
            _ => None,
        } {
            let Some(n) = t.problem.modulus_bits else {
                report.outcome = Outcome::Error;
                report.diagnosis = "target declares no modulus_bits".into();
                return report;
            };
            let r = match ShorFactoringG25.synthesise(&CryptoTarget::Rsa { modulus_bits: n }, &opts)
            {
                Ok(r) => r,
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("synthesis failed: {e}");
                    return report;
                }
            };
            let p = ShorFactoringG25.params(n).expect("params exist for a synthesised size");
            let per_shot = r.circuit.resources.total_toffoli().get() / 2;
            let cold = p.input_qubits() as f64 / r.circuit.logical_qubits.get() as f64;
            let input = QecInput {
                logical_qubits: r.circuit.logical_qubits,
                toffoli_count: per_shot,
                reaction_depth: per_shot as u64,
                target_total_error: ErrorRate::new(0.07).expect("valid"),
                idle_fraction: cold,
            };
            let hw = HardwareParams::gidney_superconducting();
            match Gidney2025Layout::new(model).estimate(&input, &hw) {
                Ok(e) => {
                    if let Some(q) = &t.expected.physical_qubits {
                        report.comparisons.push(Comparison::evaluate(
                            "physical_qubits",
                            e.physical_qubits.get() as f64,
                            q,
                        ));
                    }
                    if let Some(q) = &t.expected.wall_clock_days {
                        let total_days =
                            e.wall_clock.as_days() * r.repetitions.expected_runs;
                        report.comparisons.push(Comparison::evaluate(
                            "wall_clock_days",
                            total_days,
                            q,
                        ));
                    }
                    if t.expected_failure {
                        report.finalise_probe();
                    } else {
                        report.finalise();
                    }
                    report.diagnosis = format!(
                        "Residue-arithmetic circuit: {} logical qubits ({:.0}% of them the idle \
                         input register), {:.3e} Toffoli per shot over {:.1} expected shots. \
                         Costed under the {} giving d={}. Machine: {:.4e} cold + {:.4e} hot + \
                         {:.4e} compute = {:.4e} physical qubits, {} per shot, {:?}.",
                        r.circuit.logical_qubits.get(),
                        cold * 100.0,
                        per_shot as f64,
                        r.repetitions.expected_runs,
                        model.name(),
                        e.code_distance.map(|d| d.get()).unwrap_or(0),
                        e.breakdown.data as f64,
                        e.breakdown.routing as f64,
                        e.breakdown.factories as f64,
                        e.physical_qubits.get() as f64,
                        e.wall_clock.humanise(),
                        e.limiting_factor,
                    );
                }
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("QEC estimation failed: {e}");
                }
            }
            return report;
        }

        // Repetition-cat physical target: synthesise the ECDLP circuit, then cost it
        // under the cat architecture with the source's own noise parameters.
        if t.architecture.as_deref() == Some("repetition_cat") {
            let Some(n) = t.problem.curve_bits else {
                report.outcome = Outcome::Error;
                report.diagnosis = "cat target declares no curve_bits".into();
                return report;
            };
            let r = match ShorEcdlpRNSL.synthesise(&CryptoTarget::Ecdlp { curve_bits: n }, &opts) {
                Ok(r) => r,
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("synthesis failed: {e}");
                    return report;
                }
            };
            let input = QecInput {
                logical_qubits: r.circuit.logical_qubits,
                toffoli_count: r.circuit.resources.total_toffoli().get(),
                reaction_depth: r.circuit.resources.reaction_depth,
                target_total_error: ErrorRate::new(0.01).expect("valid"),
                idle_fraction: 0.0,
            };
            let hw = HardwareParams::gouzien_cat();
            match RepetitionCat::new(2.0, Cultivation).estimate(&input, &hw) {
                Ok(e) => {
                    if let Some(q) = &t.expected.physical_qubits {
                        report.comparisons.push(Comparison::evaluate(
                            "physical_qubits",
                            e.physical_qubits.get() as f64,
                            q,
                        ));
                    }
                    if let Some(q) = &t.expected.wall_clock_hours {
                        report.comparisons.push(Comparison::evaluate(
                            "wall_clock_hours",
                            e.wall_clock.as_hours(),
                            q,
                        ));
                    }
                    report.finalise();
                    report.diagnosis = format!(
                        "Repetition distance d={} solved from the source's own noise parameters \
                         (loss ratio 1e-5, n_bar 19), giving {} physical cat qubits per logical \
                         qubit against the surface code's {}. The advantage is structural: a \
                         repetition code is linear in distance where a surface code is quadratic. \
                         Note PRAMANA's ECDLP Toffoli count omits a sub-leading term (see the \
                         roetteler_2017_p256_absolute diagnosis), so this runtime is correspondingly \
                         optimistic.",
                        e.code_distance.map(|d| d.get()).unwrap_or(0),
                        e.code_distance
                            .map(|d| pramana_qec::cat::cat_qubits_per_logical(d.get()))
                            .unwrap_or(0),
                        e.code_distance.map(|d| d.rotated_patch_qubits()).unwrap_or(0),
                    );
                }
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("QEC estimation failed: {e}");
                }
            }
            return report;
        }
        if t.architecture.as_deref() != Some("surface_code_intermediate_distillation") {
            report.outcome = Outcome::NotImplemented;
            report.diagnosis = format!(
                "architecture '{}' is not built yet; requires yoked surface codes, \
                 cultivation and the residue-arithmetic circuit (phases P4-P5)",
                t.architecture.as_deref().unwrap_or("unspecified")
            );
            return report;
        }
        let Some(n) = t.problem.modulus_bits else {
            report.outcome = Outcome::Error;
            report.diagnosis = "physical target declares no modulus_bits".into();
            return report;
        };
        let r = match ShorFactoringGE19.synthesise(&CryptoTarget::Rsa { modulus_bits: n }, &opts) {
            Ok(r) => r,
            Err(e) => {
                report.outcome = Outcome::Error;
                report.diagnosis = format!("synthesis failed: {e}");
                return report;
            }
        };
        let input = QecInput {
            logical_qubits: r.circuit.logical_qubits,
            toffoli_count: r.circuit.resources.total_toffoli().get(),
            reaction_depth: r.circuit.resources.reaction_depth,
            target_total_error: ErrorRate::new(0.01).expect("valid"),
            idle_fraction: 0.0,
        };
        let hw = HardwareParams::gidney_superconducting();
        let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        match arch.estimate(&input, &hw) {
            Ok(e) => {
                if let Some(q) = &t.expected.physical_qubits {
                    report.comparisons.push(Comparison::evaluate(
                        "physical_qubits",
                        e.physical_qubits.get() as f64,
                        q,
                    ));
                }
                if let Some(q) = &t.expected.wall_clock_hours {
                    report.comparisons.push(Comparison::evaluate(
                        "wall_clock_hours",
                        e.wall_clock.as_hours(),
                        q,
                    ));
                }
                report.finalise();
                report.diagnosis = format!(
                    "Full chain, no published figure consulted: modulus size -> circuit \
                     ({} logical qubits, {:.3e} Toffoli) -> surface code at d={} -> {:.4e} \
                     physical qubits in {}. Limiting factor: {:?}. Qubits split {:.1}% data, \
                     {:.1}% routing, {:.1}% magic state factories.",
                    r.circuit.logical_qubits.get(),
                    input.toffoli_count as f64,
                    e.code_distance.map(|d| d.get()).unwrap_or(0),
                    e.physical_qubits.get() as f64,
                    e.wall_clock.humanise(),
                    e.limiting_factor,
                    100.0 * e.breakdown.data as f64 / e.physical_qubits.get() as f64,
                    100.0 * e.breakdown.routing as f64 / e.physical_qubits.get() as f64,
                    100.0 * e.breakdown.factories as f64 / e.physical_qubits.get() as f64,
                );
            }
            Err(e) => {
                report.outcome = Outcome::Error;
                report.diagnosis = format!("QEC estimation failed: {e}");
            }
        }
        return report;
    }
    match t.circuit.as_str() {
        "shor_factoring_ge19" => {
            let n = match t.problem.modulus_bits {
                Some(n) => n,
                None => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = "target declares no modulus_bits".into();
                    return report;
                }
            };
            match ShorFactoringGE19.synthesise(&CryptoTarget::Rsa { modulus_bits: n }, &opts) {
                Ok(r) => {
                    if let Some(q) = &t.expected.logical_qubits {
                        report.comparisons.push(Comparison::evaluate(
                            "logical_qubits",
                            r.circuit.logical_qubits.get() as f64,
                            q,
                        ));
                    }
                    if let Some(q) = &t.expected.toffoli_count {
                        report.comparisons.push(Comparison::evaluate(
                            "toffoli_count",
                            r.circuit.resources.total_toffoli().get() as f64,
                            q,
                        ));
                    }
                    report.finalise();
                    if !report.comparisons.iter().all(|c| c.passed) {
                        report.diagnosis = diagnose::diagnose_ge19(n);
                    }
                }
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("synthesis failed: {e}");
                }
            }
        }
        "shor_factoring_g25" => {
            let n = t.problem.modulus_bits.unwrap_or(0);
            match ShorFactoringG25.synthesise(&CryptoTarget::Rsa { modulus_bits: n }, &opts) {
                Ok(r) => {
                    if let Some(q) = &t.expected.logical_qubits {
                        report.comparisons.push(Comparison::evaluate(
                            "logical_qubits",
                            r.circuit.logical_qubits.get() as f64,
                            q,
                        ));
                    }
                    if let Some(q) = &t.expected.toffoli_count {
                        let p = ShorFactoringG25.params(n).expect("params");
                        report.comparisons.push(Comparison::evaluate(
                            "toffoli_count",
                            toffoli_per_factoring(&p),
                            q,
                        ));
                    }
                    report.finalise();
                }
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("synthesis failed: {e}");
                }
            }
        }
        "shor_ecdlp_schrottenloher_2026" | "shor_ecdlp_schrottenloher_2026_space" => {
            let opt = if t.circuit.ends_with("_space") {
                Optimisation::Space
            } else {
                Optimisation::Gate
            };
            let arch = ShorEcdlp2026::secp256k1(opt);
            let n = t.problem.curve_bits.unwrap_or(0);
            match arch.synthesise(&CryptoTarget::Ecdlp { curve_bits: n }, &opts) {
                Ok(r) => {
                    if let Some(q) = &t.expected.logical_qubits {
                        report.comparisons.push(Comparison::evaluate(
                            "logical_qubits",
                            r.circuit.logical_qubits.get() as f64,
                            q,
                        ));
                    }
                    if let Some(q) = &t.expected.toffoli_count {
                        report.comparisons.push(Comparison::evaluate(
                            "toffoli_count",
                            r.circuit.resources.total_toffoli().get() as f64,
                            q,
                        ));
                    }
                    report.finalise();
                    let p = arch.params(n);
                    report.diagnosis = format!(
                        "Built from the split extended Euclidean algorithm: {} iterations per \
                         in-place multiplication over a {}-bit compressed garbage vector, two \
                         multiplications per point addition, {} windowed point additions. \
                         The 2017 Roetteler construction costs roughly {:.0}x more gates for \
                         the same curve.",
                        p.eea().iterations(),
                        p.eea().garbage_bits(),
                        p.point_additions(),
                        6.013e10 / r.circuit.resources.total_toffoli().get() as f64,
                    );
                }
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("synthesis failed: {e}");
                }
            }
        }
        "shor_ecdlp_rnsl" => {
            let n = match t.problem.curve_bits {
                Some(n) => n,
                None => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = "target declares no curve_bits".into();
                    return report;
                }
            };
            match ShorEcdlpRNSL.synthesise(&CryptoTarget::Ecdlp { curve_bits: n }, &opts) {
                Ok(r) => {
                    let toffoli = r.circuit.resources.total_toffoli().get() as f64;
                    if let Some(q) = &t.expected.logical_qubits {
                        report.comparisons.push(Comparison::evaluate(
                            "logical_qubits",
                            r.circuit.logical_qubits.get() as f64,
                            q,
                        ));
                    }
                    if let Some(q) = &t.expected.leading_coefficient {
                        let nf = n as f64;
                        let coeff = toffoli / (nf.powi(3) * nf.log2());
                        report
                            .comparisons
                            .push(Comparison::evaluate("leading_coefficient", coeff, q));
                    }
                    if let Some(q) = &t.expected.toffoli_count {
                        report
                            .comparisons
                            .push(Comparison::evaluate("toffoli_count", toffoli, q));
                    }
                    report.finalise();
                    if !report.comparisons.iter().all(|c| c.passed) {
                        report.diagnosis = diagnose::diagnose_ecdlp(n);
                    }
                }
                Err(e) => {
                    report.outcome = Outcome::Error;
                    report.diagnosis = format!("synthesis failed: {e}");
                }
            }
        }
        other => {
            report.outcome = Outcome::NotImplemented;
            report.diagnosis = format!("no pipeline registered for '{other}'");
        }
    }
    report
}
