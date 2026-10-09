//! Reproduction report: synthesised counts against published figures.
//!
//! Run with: `cargo run --release --example reproduce`

use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};

fn main() {
    let opts = SynthesisOptions::default();
    println!("PRAMANA reproduction report - Gidney-Ekera 2019 (arXiv:1905.09749)");
    println!("Abstract circuit model: 3n + 0.002 n lg n qubits, 0.3n^3 + 0.0005 n^3 lg n Toffoli\n");
    println!(
        "{:>6} | {:>9} {:>9} {:>7} | {:>11} {:>11} {:>7}",
        "n", "qubits", "published", "ratio", "Toffoli", "published", "ratio"
    );
    println!("{}", "-".repeat(76));

    for n in [1024u32, 2048, 3072, 4096, 8192] {
        let r = ShorFactoringGE19
            .synthesise(&CryptoTarget::Rsa { modulus_bits: n }, &opts)
            .unwrap();
        let q = r.circuit.logical_qubits.get() as f64;
        let qp = published_logical_qubits(n);
        let t = r.circuit.resources.total_toffoli().get() as f64;
        let tp = published_toffoli(n);
        println!(
            "{:>6} | {:>9.0} {:>9.0} {:>7.3} | {:>11.4e} {:>11.4e} {:>7.3}",
            n, q, qp, q / qp, t, tp, t / tp
        );
    }

    let r = ShorFactoringGE19
        .synthesise(&CryptoTarget::Rsa { modulus_bits: 2048 }, &opts)
        .unwrap();
    println!("\nRSA-2048 detail:");
    println!("  expected runs         : {:.1}", r.repetitions.expected_runs);
    println!("  rationale             : {}", r.repetitions.rationale);
    println!(
        "  Toffoli, all runs     : {:.4e}",
        r.total_toffoli_all_runs()
    );
    println!("  data qubits           : {}", r.circuit.data_qubits);
    println!(
        "  measured peak ancilla : {}",
        r.circuit.logical_qubits.get() - r.circuit.data_qubits.get()
    );
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
