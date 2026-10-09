//! Hypothesis test for the GE19 Toffoli-count divergence.
//!
//! The verification harness diagnoses the gap as a *scaling* mismatch and proposes that
//! the cause is window-width freedom: PRAMANA's optimiser may widen its lookup windows
//! as `n` grows, whereas the published construction holds them near 5-6 bits because
//! ancilla space binds.
//!
//! This example tests that hypothesis by capping the window width and re-measuring the
//! ratio curve. If the hypothesis is right, capping should flatten the curve.

use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};

fn main() {
    let sizes = [1024u32, 2048, 3072, 4096, 8192];
    println!("Hypothesis: capping window width reproduces the published scaling.\n");
    print!("{:>10}", "max_window");
    for n in sizes {
        print!("{:>10}", n);
    }
    println!("{:>10}", "drift");
    println!("{}", "-".repeat(72));

    for max_window in [4u32, 5, 6, 7, 8, 12] {
        let opts = SynthesisOptions {
            max_window,
            ..Default::default()
        };
        print!("{:>10}", max_window);
        let mut ratios = Vec::new();
        for n in sizes {
            let r = ShorFactoringGE19
                .synthesise(&CryptoTarget::Rsa { modulus_bits: n }, &opts)
                .unwrap();
            let ratio = r.circuit.resources.total_toffoli().get() as f64
                / published_toffoli(n);
            ratios.push(ratio);
            print!("{:>10.3}", ratio);
        }
        let drift = (ratios[ratios.len() - 1] - ratios[0]).abs() / ratios[0] * 100.0;
        println!("{:>9.0}%", drift);
    }

    println!("\nWindow widths actually chosen at max_window=12:");
    for n in sizes {
        let opts = SynthesisOptions { max_window: 12, ..Default::default() };
        let p = ShorFactoringGE19.params(n, &opts);
        println!(
            "  n={:<6} w_e={} w_m={}  (table 2^{})",
            n,
            p.exponent_window,
            p.multiplication_window,
            p.exponent_window + p.multiplication_window
        );
    }
}

/// Gidney-Ekera 2019's closed forms. Local to this demonstration harness rather than the
/// library, because Law 1 keeps published results out of the estimation crates.
fn published_logical_qubits(n: u32) -> f64 {
    let nf = n as f64;
    3.0 * nf + 0.002 * nf * nf.log2()
}

fn published_toffoli(n: u32) -> f64 {
    let nf = n as f64;
    0.3 * nf.powi(3) + 0.0005 * nf.powi(3) * nf.log2()
}
