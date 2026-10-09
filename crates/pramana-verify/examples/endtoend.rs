//! End-to-end: synthesise an attack circuit, then cost its error correction.
//!
//! This is the full causal chain the project exists to establish — asset parameters to
//! circuit to physical resources — with no fixed Q-Day assumption anywhere in it.

use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_ecdlp::ShorEcdlpRNSL;
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};
use pramana_qec::magic::{Cultivation, FifteenToOneTwoLevel};
use pramana_qec::model::{HardwareParams, QecArchitecture, QecInput};
use pramana_qec::surface::{Layout, SurfaceCode};
use pramana_units::ErrorRate;

fn main() {
    let opts = SynthesisOptions::default();
    let hw = HardwareParams::gidney_superconducting();

    println!("PRAMANA end-to-end: circuit synthesis -> QEC cost");
    println!("Hardware: p=1e-3, cycle 1us, reaction 10us, square grid\n");
    println!(
        "{:<14} {:>9} {:>11} {:>12} {:>5} {:>10} {:>16}",
        "asset", "logical", "Toffoli", "physical", "d", "runtime", "limited by"
    );
    println!("{}", "-".repeat(84));

    let targets: Vec<(&str, CryptoTarget)> = vec![
        ("RSA-1024", CryptoTarget::Rsa { modulus_bits: 1024 }),
        ("RSA-2048", CryptoTarget::Rsa { modulus_bits: 2048 }),
        ("RSA-3072", CryptoTarget::Rsa { modulus_bits: 3072 }),
        ("RSA-4096", CryptoTarget::Rsa { modulus_bits: 4096 }),
        ("P-256", CryptoTarget::Ecdlp { curve_bits: 256 }),
        ("P-384", CryptoTarget::Ecdlp { curve_bits: 384 }),
        ("P-521", CryptoTarget::Ecdlp { curve_bits: 521 }),
    ];

    let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);

    for (name, t) in &targets {
        let r = match t {
            CryptoTarget::Rsa { .. } => ShorFactoringGE19.synthesise(t, &opts),
            CryptoTarget::Ecdlp { .. } => ShorEcdlpRNSL.synthesise(t, &opts),
            _ => continue,
        }
        .expect("synthesis");

        let input = QecInput {
            logical_qubits: r.circuit.logical_qubits,
            toffoli_count: r.circuit.resources.total_toffoli().get(),
            reaction_depth: r.circuit.resources.reaction_depth,
            target_total_error: ErrorRate::new(0.01).unwrap(),
            idle_fraction: 0.0,
        };
        let e = arch.estimate(&input, &hw).expect("qec estimate");
        println!(
            "{:<14} {:>9} {:>11.3e} {:>12.4e} {:>5} {:>10} {:>16?}",
            name,
            r.circuit.logical_qubits.get(),
            input.toffoli_count as f64,
            e.physical_qubits.get() as f64,
            e.code_distance.unwrap().get(),
            e.wall_clock.humanise(),
            e.limiting_factor
        );
    }

    println!("\nRSA-2048 breakdown (published GE19: ~20M qubits, ~8 hours):");
    let r = ShorFactoringGE19
        .synthesise(&CryptoTarget::Rsa { modulus_bits: 2048 }, &opts)
        .unwrap();
    let input = QecInput {
        logical_qubits: r.circuit.logical_qubits,
        toffoli_count: r.circuit.resources.total_toffoli().get(),
        reaction_depth: r.circuit.resources.reaction_depth,
        target_total_error: ErrorRate::new(0.01).unwrap(),
            idle_fraction: 0.0,
    };
    let e = arch.estimate(&input, &hw).unwrap();
    println!("  data patches   : {:>12.4e}", e.breakdown.data as f64);
    println!("  routing        : {:>12.4e}", e.breakdown.routing as f64);
    println!("  factories      : {:>12.4e}  ({} x {})",
        e.breakdown.factories as f64,
        e.magic_state_factory.count,
        e.magic_state_factory.construction);
    println!("  total          : {:>12.4e}", e.physical_qubits.get() as f64);
    println!("  runtime        : {}", e.wall_clock.humanise());
    println!("  logical error  : {:.3e} per patch-round", e.logical_error_achieved);

    println!("\nSame circuit, magic state cultivation instead of distillation:");
    let cult = SurfaceCode::new(Layout::Intermediate, Cultivation);
    let ec = cult.estimate(&input, &hw).unwrap();
    println!(
        "  physical qubits: {:.4e} ({:+.1}% vs distillation)",
        ec.physical_qubits.get() as f64,
        100.0 * (ec.physical_qubits.get() as f64 / e.physical_qubits.get() as f64 - 1.0)
    );
    println!("  factory qubits : {:.4e} vs {:.4e}",
        ec.breakdown.factories as f64, e.breakdown.factories as f64);
}
