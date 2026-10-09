//! The full causal chain, end to end.
//!
//! asset key parameters -> attack circuit -> fault-tolerant cost -> hardware trajectory
//! -> break-year distribution -> Mosca resolution -> exposure score
//!
//! No step consults a fixed Q-Day.

use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_ecdlp::ShorEcdlpRNSL;
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::shor_factoring_g25::ShorFactoringG25;
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};
use pramana_hardware::fit::{fit_capability, fit_capability_blended, fit_slip};
use pramana_hardware::roadmap::{Modality, Roadmaps};
use pramana_hardware::trajectory::Trajectory;
use pramana_qec::cat::RepetitionCat;
use pramana_qec::magic::{Cultivation, FifteenToOneTwoLevel};
use pramana_qec::model::HardwareParams;
use pramana_qec::surface::{Layout, SurfaceCode};
use pramana_risk::exposure::{score, Criticality, DataClass};
use pramana_risk::improvement::{Floors, ImprovementModel, ProblemFamily};
use pramana_risk::montecarlo::{
    break_year_distribution, ArchitectureOption, AttackProfile, AttackerBudget, MonteCarloConfig,
};
use pramana_risk::mosca::{resolve, MoscaInputs, ThreatMode};

struct Asset {
    name: &'static str,
    target: CryptoTarget,
    threat: ThreatMode,
    migration_years: f64,
    secrecy_years: f64,
    criticality: Criticality,
    data_class: DataClass,
}

fn main() {
    let root = std::env::var("PRAMANA_ROOT").unwrap_or_else(|_| ".".into());
    let roadmaps = Roadmaps::load_dir(std::path::Path::new(&format!("{root}/data/roadmaps")))
        .expect("roadmap data");

    let slip = fit_slip(&roadmaps);
    let history_only = fit_capability(&roadmaps, None).expect("enough verified demonstrations");
    let fit = fit_capability_blended(&roadmaps, &slip).expect("blended fit");
    let trajectory = Trajectory::new(fit, slip.clone());

    println!(
        "History-only fit predicts {:.0} logical qubits in 2033, against IBM's own target of\n\
         2000 for that year. Three years of verified demonstrations cannot support a ten-year\n\
         extrapolation, so the curve below blends those demonstrations with roadmap targets\n\
         shifted later by the slip prior.\n",
        history_only.median_logical_qubits(2033.0)
    );

    println!("PRAMANA end-to-end exposure assessment\n");
    println!("Hardware capability curve (verified demonstrations + slip-corrected roadmap targets):");
    println!(
        "  {} points, {}-{}, growth {:.2}x per year, residual sd {:.3} dex",
        fit.points,
        fit.first_year,
        fit.last_year,
        fit.growth_per_year(),
        fit.residual_sd
    );
    println!(
        "  median capability: {:.0} logical qubits in 2030, {:.0} in 2035",
        fit.median_logical_qubits(2030.0),
        fit.median_logical_qubits(2035.0)
    );
    println!("  slip prior: mean {:.2} yr, sd {:.2} yr, {} observed, {} censored",
        slip.mean_years, slip.sd_years, slip.observed, slip.censored);
    for w in &slip.warnings {
        println!("    ! {w}");
    }

    println!("\nPhysical-to-logical ratios, by modality:");
    for m in [
        Modality::Superconducting,
        Modality::TrappedIon,
        Modality::NeutralAtom,
        Modality::Bosonic,
    ] {
        if let Some(s) = pramana_hardware::fit::ratio_summary(&roadmaps, m) {
            println!(
                "  {:<18} geometric mean {:>6.1}:1  (range {:.1}-{:.1}, n={})",
                m.name(),
                s.geometric_mean,
                s.min,
                s.max,
                s.count
            );
        }
    }

    // Roadmap targets against the fitted history.
    println!("\nAnnounced targets versus the fitted curve:");
    for m in roadmaps.announced() {
        if let Some(lq) = m.logical_qubits {
            let ratio = trajectory.disagreement_with_target(m.year, lq);
            let verdict = if ratio > 2.0 {
                "history outruns the roadmap"
            } else if ratio < 0.5 {
                "roadmap is ahead of history"
            } else {
                "consistent"
            };
            println!(
                "  {:<12} {:<28} {} target {:>5} LQ   fit predicts {:>8.0}  ({verdict})",
                m.vendor, m.name, m.year, lq,
                trajectory.fit.median_logical_qubits(m.year as f64)
            );
        }
    }

    let opts = SynthesisOptions::default();
    let architectures = || -> Vec<ArchitectureOption> {
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
    };

    let assets = vec![
        Asset {
            name: "TLS RSA-2048",
            target: CryptoTarget::Rsa { modulus_bits: 2048 },
            threat: ThreatMode::Confidentiality,
            migration_years: 2.0,
            secrecy_years: 10.0,
            criticality: Criticality::High,
            data_class: DataClass::Confidential,
        },
        Asset {
            name: "TLS P-256",
            target: CryptoTarget::Ecdlp { curve_bits: 256 },
            threat: ThreatMode::Confidentiality,
            migration_years: 1.5,
            secrecy_years: 10.0,
            criticality: Criticality::High,
            data_class: DataClass::Confidential,
        },
        Asset {
            name: "Code-sign P-384",
            target: CryptoTarget::Ecdlp { curve_bits: 384 },
            threat: ThreatMode::Authenticity,
            migration_years: 4.0,
            secrecy_years: 20.0,
            criticality: Criticality::Critical,
            data_class: DataClass::Restricted,
        },
        Asset {
            name: "Archive RSA-4096",
            target: CryptoTarget::Rsa { modulus_bits: 4096 },
            threat: ThreatMode::Confidentiality,
            migration_years: 5.0,
            secrecy_years: 30.0,
            criticality: Criticality::Critical,
            data_class: DataClass::Restricted,
        },
    ];

    let config = MonteCarloConfig {
        samples: 3000,
        ..Default::default()
    };

    println!("\n{}", "=".repeat(96));
    println!(
        "{:<18} {:>7} {:>8} {:>8} {:>8} {:>9} {:>10} {:>12}",
        "asset", "logical", "p05", "median", "p95", "deadline", "P(exposed)", "exposure"
    );
    println!("{}", "-".repeat(96));

    for a in &assets {
        let synth = match a.target {
            CryptoTarget::Rsa { modulus_bits } if modulus_bits == 2048 => ShorFactoringG25
                .synthesise(&a.target, &opts)
                .or_else(|_| ShorFactoringGE19.synthesise(&a.target, &opts)),
            CryptoTarget::Rsa { .. } => ShorFactoringGE19.synthesise(&a.target, &opts),
            CryptoTarget::Ecdlp { .. } => ShorEcdlpRNSL.synthesise(&a.target, &opts),
            _ => continue,
        }
        .expect("synthesis");

        let (family, floors, gate_imp, width_imp) = match a.target {
            CryptoTarget::Rsa { modulus_bits } => (
                ProblemFamily::Factoring,
                Floors::factoring(modulus_bits),
                ImprovementModel::factoring_gates(),
                ImprovementModel::factoring_width(),
            ),
            CryptoTarget::Ecdlp { curve_bits } => (
                ProblemFamily::Ecdlp,
                Floors::ecdlp(curve_bits),
                ImprovementModel::ecdlp_gates(),
                ImprovementModel::ecdlp_width(),
            ),
            _ => continue,
        };
        let _ = family;

        let profile = AttackProfile {
            logical_qubits: synth.circuit.logical_qubits.get(),
            toffoli: synth.total_toffoli_all_runs(),
            family,
            floors,
            gate_improvement: gate_imp,
            width_improvement: width_imp,
        };

        let dist = break_year_distribution(
            &profile,
            &architectures(),
            &trajectory,
            AttackerBudget::nation_state(),
            config,
        );

        let inputs = MoscaInputs {
            migration_years: a.migration_years,
            secrecy_lifetime_years: a.secrecy_years,
            threat_mode: a.threat,
            reference_year: 2026,
        };
        let r = resolve(&inputs, &dist);
        let s = score(&r, a.criticality, a.data_class);

        println!(
            "{:<18} {:>7} {:>8} {:>8} {:>8} {:>9.0} {:>9.3} {:>7.1} [{:.0}-{:.0}]",
            a.name,
            profile.logical_qubits,
            dist.quantile(0.05).map(|y| y.to_string()).unwrap_or("-".into()),
            dist.median().map(|y| y.to_string()).unwrap_or("-".into()),
            dist.quantile(0.95).map(|y| y.to_string()).unwrap_or("-".into()),
            r.deadline_year,
            r.probability_exposed,
            s.score,
            s.lower,
            s.upper
        );
    }

    println!("\nPer-architecture median break year, TLS P-256:");
    let synth = ShorEcdlpRNSL
        .synthesise(&CryptoTarget::Ecdlp { curve_bits: 256 }, &opts)
        .unwrap();
    let profile = AttackProfile {
        logical_qubits: synth.circuit.logical_qubits.get(),
        toffoli: synth.total_toffoli_all_runs(),
        family: ProblemFamily::Ecdlp,
        floors: Floors::ecdlp(256),
        gate_improvement: ImprovementModel::ecdlp_gates(),
        width_improvement: ImprovementModel::ecdlp_width(),
    };
    let dist = break_year_distribution(
        &profile,
        &architectures(),
        &trajectory,
        AttackerBudget::nation_state(),
        config,
    );
    for (name, median) in &dist.by_architecture {
        println!(
            "  {:<24} {}",
            name,
            median.map(|y| y.to_string()).unwrap_or("never in window".into())
        );
    }
    let spread: Vec<u32> = dist.by_architecture.iter().filter_map(|(_, m)| *m).collect();
    if spread.len() > 1 {
        let lo = spread.iter().min().unwrap();
        let hi = spread.iter().max().unwrap();
        println!(
            "  architecture choice moves the median break year by {} years",
            hi - lo
        );
    }
    for w in &dist.warnings {
        println!("  ! {w}");
    }
}
