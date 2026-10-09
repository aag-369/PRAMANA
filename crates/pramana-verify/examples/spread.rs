//! Architecture sensitivity sweep.
//!
//! The novelty claim is that architecture choice shifts the estimated exposure window
//! materially for identical assets. That is only worth claiming if the spread is measured,
//! so this costs the same synthesised circuits under every architecture PRAMANA implements.

use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_ecdlp::ShorEcdlpRNSL;
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};
use pramana_qec::cat::RepetitionCat;
use pramana_qec::magic::{Cultivation, FifteenToOneTwoLevel};
use pramana_qec::model::{Connectivity, HardwareParams, QecArchitecture, QecInput};
use pramana_qec::neutral_atom::NeutralAtomTransversal;
use pramana_qec::qldpc::{BbCode, BivariateBicycle, ComputationScheme};
use pramana_qec::surface::{Layout, SurfaceCode};
use pramana_qec::yoked::YokedSurfaceCode;
use pramana_units::ErrorRate;

fn main() {
    let opts = SynthesisOptions::default();
    let targets: Vec<(&str, CryptoTarget)> = vec![
        ("RSA-2048", CryptoTarget::Rsa { modulus_bits: 2048 }),
        ("RSA-4096", CryptoTarget::Rsa { modulus_bits: 4096 }),
        ("P-256", CryptoTarget::Ecdlp { curve_bits: 256 }),
        ("P-384", CryptoTarget::Ecdlp { curve_bits: 384 }),
    ];

    println!("PRAMANA architecture sensitivity sweep");
    println!("Identical circuits, costed under every implemented architecture.\n");
    println!(
        "{:<10} {:>11} {:>11} {:>11} {:>11} {:>11} {:>8}",
        "asset", "surface", "yoked", "cat", "qLDPC", "atoms", "spread"
    );
    println!("{}", "-".repeat(80));

    let mut spreads = Vec::new();

    for (name, t) in &targets {
        let r = match t {
            CryptoTarget::Rsa { .. } => ShorFactoringGE19.synthesise(t, &opts),
            CryptoTarget::Ecdlp { .. } => ShorEcdlpRNSL.synthesise(t, &opts),
            _ => continue,
        }
        .expect("synthesis");

        let mk = |idle: f64| QecInput {
            logical_qubits: r.circuit.logical_qubits,
            toffoli_count: r.circuit.resources.total_toffoli().get(),
            reaction_depth: r.circuit.resources.reaction_depth,
            target_total_error: ErrorRate::new(0.01).unwrap(),
            idle_fraction: idle,
        };

        let sc = HardwareParams::gidney_superconducting();
        let mut ldpc_hw = HardwareParams::gidney_superconducting();
        ldpc_hw.connectivity = Connectivity::Degree(6);

        let vals = [
            SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)
                .estimate(&mk(0.0), &sc)
                .unwrap()
                .physical_qubits
                .get() as f64,
            YokedSurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)
                .estimate(&mk(0.8), &sc)
                .unwrap()
                .physical_qubits
                .get() as f64,
            RepetitionCat::new(2.0, Cultivation)
                .estimate(&mk(0.0), &HardwareParams::gouzien_cat())
                .unwrap()
                .physical_qubits
                .get() as f64,
            BivariateBicycle::new(BbCode::gross(), ComputationScheme::tour_de_gross())
                .estimate(&mk(0.0), &ldpc_hw)
                .unwrap()
                .physical_qubits
                .get() as f64,
            NeutralAtomTransversal::transversal()
                .estimate(&mk(0.0), &HardwareParams::neutral_atom_array())
                .unwrap()
                .physical_qubits
                .get() as f64,
        ];

        let lo = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = vals.iter().cloned().fold(0.0, f64::max);
        spreads.push(hi / lo);

        println!(
            "{:<10} {:>11.3e} {:>11.3e} {:>11.3e} {:>11.3e} {:>11.3e} {:>7.0}x",
            name, vals[0], vals[1], vals[2], vals[3], vals[4], hi / lo
        );
    }

    let worst = spreads.iter().cloned().fold(0.0, f64::max);
    println!(
        "\nPhysical qubit requirement varies by up to {worst:.0}x across architectures for an\n\
         identical asset. A single-architecture estimate is a point estimate wearing a lab coat."
    );

    println!(
        "\nNORMALISATION DISCLOSURE. This spread is not purely a property of the codes.\n\
         A cat qubit is not a transmon is not an atom, and the columns assume different\n\
         cycle times (500 ns cat, 1 us superconducting, 200 us atoms) and different\n\
         connectivity. The qLDPC column charges logical computation overhead, not memory\n\
         only; quoting its memory figure instead would overstate its advantage roughly\n\
         threefold. Any headline comparison must say which part is code structure and\n\
         which is hardware assumption."
    );

    println!("\nWhat binds each architecture, for RSA-2048:");
    let r = ShorFactoringGE19
        .synthesise(&CryptoTarget::Rsa { modulus_bits: 2048 }, &opts)
        .unwrap();
    let inp = QecInput {
        logical_qubits: r.circuit.logical_qubits,
        toffoli_count: r.circuit.resources.total_toffoli().get(),
        reaction_depth: r.circuit.resources.reaction_depth,
        target_total_error: ErrorRate::new(0.01).unwrap(),
        idle_fraction: 0.0,
    };
    let mut ldpc_hw = HardwareParams::gidney_superconducting();
    ldpc_hw.connectivity = Connectivity::Degree(6);
    let rows: Vec<(&str, _)> = vec![
        (
            "surface code",
            SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)
                .estimate(&inp, &HardwareParams::gidney_superconducting())
                .unwrap(),
        ),
        (
            "repetition cat",
            RepetitionCat::new(2.0, Cultivation)
                .estimate(&inp, &HardwareParams::gouzien_cat())
                .unwrap(),
        ),
        (
            "bivariate bicycle",
            BivariateBicycle::new(BbCode::gross(), ComputationScheme::tour_de_gross())
                .estimate(&inp, &ldpc_hw)
                .unwrap(),
        ),
        (
            "neutral atoms",
            NeutralAtomTransversal::transversal()
                .estimate(&inp, &HardwareParams::neutral_atom_array())
                .unwrap(),
        ),
    ];
    for (label, e) in rows {
        println!(
            "  {:<20} {:>13}  d={:<4} {:?}",
            label,
            e.wall_clock.humanise(),
            e.code_distance.map(|d| d.get()).unwrap_or(0),
            e.limiting_factor
        );
    }
}
