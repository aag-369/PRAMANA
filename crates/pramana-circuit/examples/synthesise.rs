//! Synthesise windowed modular exponentiation circuits and report measured resources.
//!
//! Run with: `cargo run --release --example synthesise`

use pramana_circuit::arith::modexp::{emit_modexp, optimise_windows, ModExpParams};
use pramana_circuit::ir::CircuitBuilder;
use pramana_units::{DecompositionStrategy, LogicalQubits};

fn main() {
    println!("PRAMANA - windowed modular exponentiation, synthesised and measured");
    println!("(counts emerge from circuit assembly; no published value is consulted)\n");
    println!(
        "{:>6} {:>4} {:>4} {:>9} {:>13} {:>13} {:>10}",
        "n", "w_e", "w_m", "table", "Toffoli", "T count", "ancilla"
    );
    println!("{}", "-".repeat(68));

    for n in [512u32, 1024, 2048, 3072, 4096] {
        let p = optimise_windows(&ModExpParams::textbook(n), 12);
        let mut b = CircuitBuilder::new(format!("rsa{n}"));
        emit_modexp(&mut b, &p);
        let node = b.finish();
        let f = node.fold().expect("well-formed circuit");
        let toff = f.resources.total_toffoli().get();
        let t = f.resources.total_t(DecompositionStrategy::default()).get();
        println!(
            "{:>6} {:>4} {:>4} {:>9} {:>13.4e} {:>13.4e} {:>10}",
            n,
            p.exponent_window,
            p.multiplication_window,
            format!("2^{}", p.exponent_window + p.multiplication_window),
            toff as f64,
            t as f64,
            f.peak_alloc
        );
    }

    println!("\nPer-scope breakdown for n=2048 (this is the provenance the UI renders):");
    let p = optimise_windows(&ModExpParams::textbook(2048), 12);
    let mut b = CircuitBuilder::new("rsa2048");
    emit_modexp(&mut b, &p);
    let node = b.finish();
    let total = node.fold().unwrap().resources.total_toffoli().get() as f64;
    let mut seen: Vec<(String, u128)> = Vec::new();
    for (name, r) in node.breakdown() {
        let key = name.split('[').next().unwrap_or(&name).to_string();
        let c = r.total_toffoli().get();
        if let Some(e) = seen.iter_mut().find(|(k, _)| *k == key) {
            e.1 = e.1.max(c);
        } else {
            seen.push((key, c));
        }
    }
    seen.sort_by(|a, b| b.1.cmp(&a.1));
    for (name, c) in seen.iter().take(8) {
        println!("  {:<28} {:>12.4e}  ({:>5.1}%)", name, *c as f64, 100.0 * *c as f64 / total);
    }

    let ir = pramana_circuit::ir::CircuitIR::from_node(
        "rsa2048",
        node,
        LogicalQubits::new((p.register_bits() * 3) as u64),
    )
    .unwrap();
    println!("\nPeak logical qubits (3 registers + measured ancilla): {}", ir.logical_qubits);

    println!("\nDeclared structural omissions still to be charged (Phase 2):");
    for o in ModExpParams::known_omissions() {
        println!("  - {o}");
    }
}
