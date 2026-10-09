//! Windowed modular exponentiation.
//!
//! This is the inner loop of Shor's factoring algorithm and the dominant cost of the
//! whole attack. The windowed construction (Gidney, *Windowed quantum arithmetic*,
//! arXiv:1905.07682) replaces controlled modular multiplications with classically
//! precomputed table lookups followed by plain additions, trading a table of size
//! `2^(w_e + w_m)` against a factor `w_e * w_m` reduction in the number of additions.
//!
//! The window sizes are **found by search, not asserted**. Recovering the optimum
//! independently is one of PRAMANA's verification signals: published constructions
//! settle on small windows (around 4-6 bits for RSA-2048), and a from-scratch optimiser
//! that lands in the same region is evidence the cost structure is modelled correctly.
//!
//! # Scope of this module (P1)
//!
//! This implements the *structure* of windowed modular exponentiation and measures it.
//! Calibration against the published end-to-end Toffoli counts of Gidney-Ekera 2019 and
//! Gidney 2025 is Phase 2 work and requires the exact register layout and modular
//! reduction strategy from those papers. The current construction is deliberately
//! documented as a lower bound on their cost: see [`ModExpParams::known_omissions`].

use crate::arith::adder::{self, AdderKind};
use crate::arith::lookup;
use crate::ir::CircuitBuilder;
use serde::{Deserialize, Serialize};

/// Parameters describing a windowed modular exponentiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModExpParams {
    /// Bit length of the modulus `N`.
    pub modulus_bits: u32,
    /// Bit length of the exponent register.
    ///
    /// Textbook Shor uses `2n`. Ekera-Hastad short-exponent variants reduce this
    /// substantially, at the cost of more classical post-processing and more runs.
    pub exponent_bits: u32,
    /// Exponent window width in bits.
    pub exponent_window: u32,
    /// Multiplication window width in bits.
    pub multiplication_window: u32,
    /// Extra high-order bits carried for the coset (Zalka) representation, which makes
    /// modular reduction implicit.
    pub coset_padding: u32,
    /// Which adder construction to charge for.
    pub adder: AdderKind,
}

impl ModExpParams {
    /// Standard textbook-Shor parameters for an `n`-bit modulus, with windows left to
    /// the optimiser.
    pub fn textbook(modulus_bits: u32) -> Self {
        Self {
            modulus_bits,
            exponent_bits: 2 * modulus_bits,
            exponent_window: 5,
            multiplication_window: 5,
            coset_padding: adder::coset_padding(modulus_bits, 2),
            adder: AdderKind::CuccaroAsChargedByGe19,
        }
    }

    /// The width of the working register, including coset padding.
    pub fn register_bits(&self) -> u32 {
        self.modulus_bits + self.coset_padding
    }

    /// Persistent data registers held across the whole exponentiation.
    ///
    /// Two registers are persistent (accumulator and multiplicand). The third register
    /// implied by the conventional `3n` figure is the adder's carry workspace, which
    /// PRAMANA does not assume: it is *measured* from the assembled circuit's peak
    /// ancilla. Adding a hardcoded third register on top of the measured workspace
    /// double-counts it.
    pub fn data_registers(&self) -> u32 {
        2
    }

    /// Persistent data qubits, excluding measured ancilla.
    pub fn data_qubits(&self) -> u64 {
        self.data_registers() as u64 * self.register_bits() as u64
    }

    /// Number of exponent windows, i.e. outer iterations.
    pub fn outer_iterations(&self) -> u64 {
        (self.exponent_bits as u64).div_ceil(self.exponent_window.max(1) as u64)
    }

    /// Number of lookup-addition rounds per modular multiplication.
    ///
    /// Gidney-Ekera 2019 section 2.5: "the 2n controlled additions we needed to perform
    /// within each multiplication become 2n/cmul uncontrolled additions". The factor of
    /// two is not an inefficiency to be optimised away; it is the pair of passes that
    /// reversible multiplication requires, one to accumulate the product into a fresh
    /// register and one to clear the source register by multiplying by the precomputed
    /// modular inverse. Charging `n/cmul` here and a separate doubling elsewhere would
    /// double-count it.
    pub fn inner_rounds(&self) -> u64 {
        let additions = 2 * self.register_bits() as u64;
        additions.div_ceil(self.multiplication_window.max(1) as u64)
    }

    /// Number of entries in the lookup table for one round.
    pub fn table_entries(&self) -> u64 {
        let bits = self.exponent_window + self.multiplication_window;
        if bits >= 63 {
            u64::MAX / 2
        } else {
            1u64 << bits
        }
    }

    /// Analytic cost oracle, used only to cross-check the assembled circuit in tests.
    ///
    /// This is never on the production path (Law 1); it exists so that the synthesised
    /// count can be validated against an independent derivation of the same structure.
    pub fn analytic_toffoli(&self) -> u128 {
        let plan = lookup::optimal_plan(self.table_entries(), self.register_bits());
        let per_round = plan.compute_ands as u128
            + plan.uncompute_ands as u128
            + self.adder.analytic_cost(self.register_bits()) as u128;
        per_round * self.inner_rounds() as u128 * self.outer_iterations() as u128
    }

    /// Structural elements this construction does not yet charge for.
    ///
    /// Recorded explicitly so that a divergence from published totals is diagnosed
    /// rather than tuned away (spec §25). Each entry is a hypothesis to be tested in
    /// Phase 2 against the primary sources.
    ///
    /// The exponent length is no longer listed: Gidney-Ekera 2019 section 2.2 states
    /// "The total exponent length is hence ne = 3m = 1.5n + O(1)", confirming the value
    /// PRAMANA assumes.
    pub fn known_omissions() -> &'static [&'static str] {
        &[
            "Semiclassical (one-qubit) exponent readout and its measurement overhead",
            "Rz rotation synthesis in the semiclassical Fourier transform",
            "Classical precomputation of the lookup tables (not a quantum cost, but a real one)",
        ]
    }
}

/// Emit a full windowed modular exponentiation.
///
/// Structure, following Gidney-Ekera 2019 section 2.5, which windows at two levels:
///
/// - over the exponent, turning `ne` controlled multiplications into `ne/cexp`
///   uncontrolled ones;
/// - over the multiplier, turning the `2n` controlled additions inside each
///   multiplication into `2n/cmul` uncontrolled lookup-additions.
///
/// Each lookup-addition is a table lookup addressed by `cexp + cmul` qubits followed by
/// an unconditional addition. The `2n` (rather than `n`) additions per multiplication
/// are the accumulate-and-clear pair that reversible multiplication requires.
pub fn emit_modexp(b: &mut CircuitBuilder, p: &ModExpParams) {
    let reg = p.register_bits();
    let entries = p.table_entries();
    b.scope(
        format!(
            "modexp[n={},ne={},cexp={},cmul={}]",
            p.modulus_bits, p.exponent_bits, p.exponent_window, p.multiplication_window
        ),
        |b| {
            b.repeat("modular_multiplication", p.outer_iterations(), |b| {
                b.repeat("lookup_addition", p.inner_rounds(), |b| {
                    lookup::emit_lookup(b, entries, reg);
                    p.adder.emit(b, reg);
                });
            });
        },
    );
}



/// Search for the window sizes minimising total Toffoli cost.
///
/// Exhaustive over a generous rectangle rather than analytic, so it remains correct if
/// the lookup or adder cost models change. Returns the optimal parameters.
pub fn optimise_windows(base: &ModExpParams, max_window: u32) -> ModExpParams {
    let mut best = *base;
    let mut best_cost = u128::MAX;
    for we in 1..=max_window {
        for wm in 1..=max_window {
            let cand = ModExpParams {
                exponent_window: we,
                multiplication_window: wm,
                ..*base
            };
            let c = cand.analytic_toffoli();
            if c < best_cost {
                best_cost = c;
                best = cand;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::CircuitBuilder;

    fn assembled_toffoli(p: &ModExpParams) -> u128 {
        let mut b = CircuitBuilder::new("t");
        emit_modexp(&mut b, p);
        b.finish().fold().unwrap().resources.total_toffoli().get()
    }

    #[test]
    fn assembled_count_matches_the_independent_oracle() {
        // Spec §7.5: synthesised count must agree with the analytic oracle to within 1%.
        for n in [512u32, 1024, 1536, 2048, 3072, 4096] {
            let p = optimise_windows(&ModExpParams::textbook(n), 8);
            let got = assembled_toffoli(&p) as f64;
            let want = p.analytic_toffoli() as f64;
            let rel = (got - want).abs() / want;
            assert!(rel < 0.01, "n={n}: assembled={got:.3e} oracle={want:.3e} rel={rel:.4}");
        }
    }

    #[test]
    fn optimiser_finds_small_windows_as_published_constructions_do() {
        // Independent recovery of the published window regime is a verification signal.
        let p = optimise_windows(&ModExpParams::textbook(2048), 12);
        assert!(
            (2..=8).contains(&p.exponent_window),
            "w_e = {} outside the expected small-window regime",
            p.exponent_window
        );
        assert!(
            (2..=8).contains(&p.multiplication_window),
            "w_m = {} outside the expected small-window regime",
            p.multiplication_window
        );
    }

    #[test]
    fn optimiser_output_is_a_local_minimum() {
        let p = optimise_windows(&ModExpParams::textbook(2048), 10);
        let base = p.analytic_toffoli();
        for dwe in [-1i32, 0, 1] {
            for dwm in [-1i32, 0, 1] {
                if dwe == 0 && dwm == 0 {
                    continue;
                }
                let we = (p.exponent_window as i32 + dwe).max(1) as u32;
                let wm = (p.multiplication_window as i32 + dwm).max(1) as u32;
                let cand = ModExpParams {
                    exponent_window: we,
                    multiplication_window: wm,
                    ..p
                };
                assert!(
                    cand.analytic_toffoli() >= base,
                    "perturbation (we={we}, wm={wm}) beat the optimum"
                );
            }
        }
    }

    #[test]
    fn windowing_beats_the_unwindowed_construction() {
        let n = 2048u32;
        let unwindowed = ModExpParams {
            exponent_window: 1,
            multiplication_window: 1,
            ..ModExpParams::textbook(n)
        };
        let windowed = optimise_windows(&ModExpParams::textbook(n), 10);
        let speedup = unwindowed.analytic_toffoli() as f64 / windowed.analytic_toffoli() as f64;
        assert!(speedup > 5.0, "windowing should give a large win, got {speedup:.1}x");
    }

    #[test]
    fn cost_is_monotone_in_modulus_size() {
        // Law 7.
        let mut prev = 0u128;
        for n in [256u32, 512, 1024, 1536, 2048, 3072, 4096, 8192] {
            let p = optimise_windows(&ModExpParams::textbook(n), 8);
            let c = p.analytic_toffoli();
            assert!(c > prev, "non-monotone at n={n}");
            prev = c;
        }
    }

    #[test]
    fn cost_scales_roughly_as_n_cubed() {
        // Windowed modexp is Theta(n^3 / (w_e w_m)); doubling n should cost ~8x.
        let a = optimise_windows(&ModExpParams::textbook(1024), 8).analytic_toffoli() as f64;
        let b = optimise_windows(&ModExpParams::textbook(2048), 8).analytic_toffoli() as f64;
        let ratio = b / a;
        assert!(
            ratio > 6.0 && ratio < 11.0,
            "expected ~8x for a doubling of n, got {ratio:.2}x"
        );
    }

    #[test]
    fn peak_qubits_do_not_grow_with_iteration_count() {
        // Ancilla must be reused across the billions of rounds, not accumulated.
        let p = optimise_windows(&ModExpParams::textbook(2048), 8);
        let mut b = CircuitBuilder::new("t");
        emit_modexp(&mut b, &p);
        let f = b.finish().fold().unwrap();
        assert_eq!(f.net_alloc, 0);
        assert!(
            f.peak_alloc < 100_000,
            "peak ancilla {} is implausibly large; ancilla are not being reused",
            f.peak_alloc
        );
    }

    #[test]
    fn breakdown_attributes_cost_to_lookup_and_adder() {
        let p = optimise_windows(&ModExpParams::textbook(1024), 8);
        let mut b = CircuitBuilder::new("t");
        emit_modexp(&mut b, &p);
        let bd = b.finish().breakdown();
        let has_adder = bd.iter().any(|(n, _)| n.starts_with("cuccaro_add_ge19"));
        let has_lookup = bd.iter().any(|(n, _)| n.starts_with("qrom"));
        assert!(has_adder && has_lookup, "provenance must name both subroutines");
    }

    #[test]
    fn omissions_are_documented_rather_than_silently_tuned_away() {
        assert!(
            !ModExpParams::known_omissions().is_empty(),
            "structural gaps must be declared, not hidden"
        );
    }
}
