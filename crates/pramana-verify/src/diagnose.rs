//! Discrepancy diagnosis.
//!
//! When a reproduction fails, the useful question is not "by how much" but "in what
//! shape". A gap that is a constant multiple across problem sizes points to a missing
//! multiplicative structure in the circuit. A gap that drifts with `n` points to a
//! scaling mismatch: the model and the source disagree about an exponent or a
//! logarithmic factor, which is a substantively different and more serious defect.
//!
//! This module measures which of the two is happening, so the verification report
//! carries a hypothesis rather than only a number.

use pramana_circuit::pipeline::AttackCircuit;
use crate::published;
use pramana_circuit::shor_ecdlp::ShorEcdlpRNSL;
use pramana_circuit::shor_factoring::ShorFactoringGE19;
use pramana_circuit::target::{CryptoTarget, SynthesisOptions};

/// Problem sizes swept when characterising a discrepancy.
const SWEEP: &[u32] = &[1024, 2048, 3072, 4096, 8192, 16384, 32768];

/// Least-squares slope of `y` on `x`.
#[allow(dead_code)]
fn slope(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let num: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let den: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

/// Fit `y = a*x^3 + b*x^2` by ordinary least squares.
///
/// A single-power fit cannot characterise this discrepancy: the table-lookup term
/// contributes a genuine `n^2` component that is half the total at `n = 1024`, which
/// drags an unconstrained exponent fit well below 3 and produces a spurious "asymptotic
/// mismatch" verdict. Fitting both terms together separates the leading coefficient,
/// which is the quantity that should be compared against the published `0.3`.
///
/// `x` is rescaled by 1000 before fitting to keep the normal equations conditioned.
fn fit_cubic_quadratic(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    const SCALE: f64 = 1000.0;
    let (mut s66, mut s65, mut s55, mut t6, mut t5) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (x, y) in xs.iter().zip(ys) {
        let u = x / SCALE;
        let (u2, u3) = (u * u, u * u * u);
        s66 += u3 * u3;
        s65 += u3 * u2;
        s55 += u2 * u2;
        t6 += y * u3;
        t5 += y * u2;
    }
    let det = s66 * s55 - s65 * s65;
    if det.abs() < f64::EPSILON {
        return (f64::NAN, f64::NAN);
    }
    let a_scaled = (t6 * s55 - t5 * s65) / det;
    let b_scaled = (s66 * t5 - s65 * t6) / det;
    // Undo the rescaling: a*u^3 = a/SCALE^3 * x^3, likewise for b.
    (a_scaled / SCALE.powi(3), b_scaled / SCALE.powi(2))
}

/// Published leading coefficient of the `n^3` term in Gidney-Ekera 2019.
const PUBLISHED_LEADING_COEFF: f64 = 0.3;

/// Characterise the Toffoli-count discrepancy for the GE19 pipeline.
///
/// Fits `computed = a*n^3 + b*n^2` and compares `a` against the published `0.3`. This
/// separates a genuine asymptotic disagreement from a missing constant multiplicative
/// structure, which raw ratio inspection cannot do.
pub fn diagnose_ge19(_n: u32) -> String {
    let opts = SynthesisOptions::default();
    let mut ns = Vec::new();
    let mut computed = Vec::new();
    let mut rows = Vec::new();

    for &n in SWEEP {
        let Ok(r) = ShorFactoringGE19.synthesise(&CryptoTarget::Rsa { modulus_bits: n }, &opts)
        else {
            continue;
        };
        let c = r.circuit.resources.total_toffoli().get() as f64;
        let p = published::ge19_toffoli(n);
        ns.push(n as f64);
        computed.push(c);
        rows.push(format!("| {n} | {c:.4e} | {p:.4e} | {:.3} |", c / p));
    }

    if ns.len() < 4 {
        return "insufficient sweep points to characterise the discrepancy".into();
    }

    let (a, b) = fit_cubic_quadratic(&ns, &computed);
    let coeff_ratio = PUBLISHED_LEADING_COEFF / a;

    // Share of the total carried by the sub-leading term at the smallest and largest n.
    let share = |n: f64| b * n * n / (a * n * n * n + b * n * n) * 100.0;
    let share_lo = share(ns[0]);
    let share_hi = share(ns[ns.len() - 1]);

    let verdict = format!(
        "**Constant-factor gap in the leading coefficient.** A two-term fit gives \
         `computed = {a:.4} n^3 + {b:.1} n^2`. The cubic term is the right shape; its \
         coefficient is {a:.4} against the published {PUBLISHED_LEADING_COEFF}, a factor of \
         {coeff_ratio:.2}x.\n\n\
         The raw ratio column drifts downward only because the `n^2` term carries {share_lo:.0}% \
         of the total at n={:.0} but just {share_hi:.0}% at n={:.0}. That is a small-`n` artefact, \
         not a scaling mismatch, and a single-power fit over this range reports a spurious \
         exponent near 2.7 for exactly that reason.",
        ns[0],
        ns[ns.len() - 1]
    );

    let hypotheses = "Candidate structures accounting for the missing factor, each testable in \
         isolation and none to be applied without justification traced to the source \
         construction:\n\n\
         1. **Exponent length.** PRAMANA assumes the Ekera-Hastad multiple of 1.5n; textbook \
            Shor uses 2n. Worth 1.33x.\n\
         2. **Accumulator width during multiplication.** If the product register is widened \
            before coset reduction, every addition in the inner loop costs proportionally more. \
            Worth up to 2x.\n\
         3. **Per-multiplication coset reduction**, declared in `ModExpParams::known_omissions` \
            and not yet emitted.\n\n\
         A prior hypothesis, that PRAMANA's window optimiser was widening lookup windows as `n` \
         grew and buying an unearned `1/(lg n)^2` saving, was **falsified** by controlled \
         experiment (`examples/hypothesis.rs`): capping the window width leaves the curve \
         unchanged, and the optimiser independently selects the published 5-6 bit regime at every \
         size. That result is retained because a rejected hypothesis is a real constraint on the \
         remaining explanations.";

    format!(
        "Toffoli-count sweep against the published closed form \
         `0.3 n^3 + 0.0005 n^3 lg n`:\n\n\
         | n | computed | published | ratio |\n|---:|---:|---:|---:|\n{}\n\n{}\n\n{}\n\n\
         Direction of error: PRAMANA is **below** the published figure, i.e. optimistic about the \
         attacker's cost. For a migration-triage tool that is the dangerous direction, and it is \
         recorded as an open defect rather than accepted.",
        rows.join("\n"),
        verdict,
        hypotheses
    )
}

/// Characterise the ECDLP discrepancy.
pub fn diagnose_ecdlp(n: u32) -> String {
    let opts = SynthesisOptions::default();
    let Ok(r) = ShorEcdlpRNSL.synthesise(&CryptoTarget::Ecdlp { curve_bits: n }, &opts) else {
        return "synthesis failed".into();
    };
    let got = r.circuit.resources.total_toffoli().get() as f64;
    let nf = n as f64;
    let coeff = got / (nf.powi(3) * nf.log2());
    let leading_only = 448.0 * nf.powi(3) * nf.log2();
    let sub_leading = 4090.0 * nf.powi(3);
    let published = leading_only + sub_leading;

    format!(
        "PRAMANA computes {got:.4e} Toffoli gates against the published {published:.4e}, a ratio \
         of {:.3}.\n\n\
         **The leading term is reproduced exactly.** The fitted coefficient is {coeff:.1} against \
         the published 448, and that 448 is never written in PRAMANA's source: it emerges from \
         composing 4 inversions, 2 squarings and 4 multiplications into a point addition \
         (`4*32 + 2*16 + 4*16 = 224`) and iterating `2n` times.\n\n\
         **The entire discrepancy is the sub-leading term.** The source's expression is \
         `448 n^3 log2 n + 4090 n^3`. At n={n} the leading term is {leading_only:.4e} and the \
         sub-leading term is {sub_leading:.4e} — the two are comparable, which is why the ratio \
         sits near one half rather than near one.\n\n\
         That `+4090 n^3` is not derivable from the published subroutine table. The source obtained \
         it by regression over its own simulated circuits (\"we then again perform a regression to \
         determine the next coefficient\"), and Table 1 gives sub-leading terms for only some \
         routines, with none for the modular inversion that dominates. Reproducing it would require \
         implementing the group law at gate level rather than at the level of subroutine \
         multiplicities.\n\n\
         It would be trivial to close this by inserting a `+511 n^2` term into the inversion cost, \
         since `4 * 511 = 2044 ~ 2045`. That is precisely the fit-to-target this harness exists to \
         prevent, and it is not done. Direction of error: PRAMANA is **below** the published \
         figure, i.e. optimistic about the attacker's cost.",
        got / published
    )
}
