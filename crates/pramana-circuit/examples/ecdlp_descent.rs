//! The elliptic-curve resource descent, 2017 to 2026.
//!
//! The series this prints is the empirical backing for the risk engine's
//! algorithmic-improvement term. No other estimator models it.

use pramana_circuit::pipeline::AttackCircuit;
use pramana_circuit::shor_ecdlp::ShorEcdlpRNSL;
use pramana_circuit::shor_ecdlp_2026::{Ecdlp2026Params, Optimisation, ShorEcdlp2026};
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};

fn main() {
    let opts = SynthesisOptions::default();
    let target = CryptoTarget::Ecdlp { curve_bits: 256 };

    println!("ECDLP over a 256-bit curve: what PRAMANA computes\n");
    println!(
        "{:<44} {:>9} {:>12} {:>10}",
        "construction", "qubits", "Toffoli", "vs 2017"
    );
    println!("{}", "-".repeat(80));

    let legacy = ShorEcdlpRNSL.synthesise(&target, &opts).unwrap();
    let base = legacy.circuit.resources.total_toffoli().get() as f64;
    println!(
        "{:<44} {:>9} {:>12.3e} {:>10}",
        "Roetteler et al. 2017 (leading order)",
        legacy.circuit.logical_qubits.get(),
        base,
        "1x"
    );

    for (label, arch) in [
        (
            "Schrottenloher 2026, space-optimised",
            ShorEcdlp2026::secp256k1(Optimisation::Space),
        ),
        (
            "Schrottenloher 2026, gate-optimised",
            ShorEcdlp2026::secp256k1(Optimisation::Gate),
        ),
        (
            "Schrottenloher 2026, generic prime, gate-opt",
            ShorEcdlp2026::generic(Optimisation::Gate),
        ),
    ] {
        let r = arch.synthesise(&target, &opts).unwrap();
        let t = r.circuit.resources.total_toffoli().get() as f64;
        println!(
            "{:<44} {:>9} {:>12.3e} {:>9.0}x",
            label,
            r.circuit.logical_qubits.get(),
            t,
            base / t
        );
    }

    println!("\nPublished figures for comparison (Table 2, arXiv:2606.02235):");
    println!("  Babbush et al. space-optimised   1191 qubits   2^26.27 = {:.3e}", 2f64.powf(26.27));
    println!("  Babbush et al. gate-optimised    1441 qubits   2^25.94 = {:.3e}", 2f64.powf(25.94));
    println!("  Schrottenloher space-optimised   1208 qubits   2^26.11 = {:.3e}", 2f64.powf(26.11));
    println!("  Schrottenloher gate-optimised    1462 qubits   2^25.78 = {:.3e}", 2f64.powf(25.78));

    println!("\nWhere the cost sits, secp256k1 gate-optimised:");
    let p = Ecdlp2026Params::secp256k1(Optimisation::Gate);
    let per_add = p.point_addition_toffoli() as f64;
    let lookups = (3 * p.lookup_toffoli()) as f64;
    let mults = (2 * p.inplace_multiplication_toffoli()) as f64;
    println!("  point additions                {}", p.point_additions());
    println!("  per addition                   {per_add:.3e} Toffoli");
    println!(
        "    table lookups (3 x 2^16)     {lookups:.3e}  ({:.1}%)",
        100.0 * lookups / per_add
    );
    println!(
        "    in-place multiplications     {mults:.3e}  ({:.1}%)",
        100.0 * mults / per_add
    );
    println!("  EEA iterations per multiply    {}", p.eea().iterations());
    println!("  garbage vector                 {} bits", p.eea().garbage_bits());

    println!("\nWhy this matters for triage: PRAMANA's Roetteler pipeline understates the");
    println!("attacker by roughly {:.0}x on gate count. For a migration tool that is the", base / (Ecdlp2026Params::secp256k1(Optimisation::Gate).total_toffoli() as f64));
    println!("dangerous direction of error.");
}
