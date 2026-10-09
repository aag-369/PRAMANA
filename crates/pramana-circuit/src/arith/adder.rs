//! Reversible adders.
//!
//! The workhorse is Gidney's temporary-AND ripple-carry construction: an `n`-bit
//! addition emits `n - 1` AND computations, each costing 4 T gates, with measurement-
//! based uncomputation that costs no T gates at all. This is why windowed arithmetic is
//! affordable, and modelling it explicitly (rather than charging 7 T per Toffoli) is
//! worth roughly a factor of three in the final estimate.

use crate::ir::CircuitBuilder;

/// Which adder construction a cost model charges for.
///
/// This is a first-class modelling choice, not an implementation detail, because the
/// two constructions differ by roughly a factor of two and published estimates do not
/// all use the same one. Gidney-Ekera 2019 states plainly (section 2.5): "Using Cuccaro
/// et al.'s adder, each n-bit addition has a Toffoli count and measurement depth of 2n."
/// Reproducing that paper therefore requires charging `2n`, even though the
/// temporary-AND construction available today is about twice as cheap.
///
/// Making the choice explicit is the point: it lets PRAMANA show how much of a published
/// figure is algorithmic and how much is the adder the authors happened to assume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdderKind {
    /// Gidney's temporary-AND ripple-carry adder: `n - 1` AND computations for an
    /// `n`-bit addition, with measurement-based uncomputation costing no T gates.
    GidneyTemporaryAnd,
    /// Cuccaro et al.'s ripple-carry adder as charged by Gidney-Ekera 2019: `2n` Toffoli
    /// gates for an `n`-bit addition.
    CuccaroAsChargedByGe19,
}

impl AdderKind {
    /// Emit an `n`-bit addition under this construction, returning Toffoli-equivalents.
    pub fn emit(self, b: &mut CircuitBuilder, n: u32) -> u64 {
        match self {
            AdderKind::GidneyTemporaryAnd => ripple_carry_add(b, n),
            AdderKind::CuccaroAsChargedByGe19 => cuccaro_add_ge19(b, n),
        }
    }

    /// Analytic Toffoli cost of an `n`-bit addition, for oracle cross-checks.
    pub fn analytic_cost(self, n: u32) -> u64 {
        match self {
            AdderKind::GidneyTemporaryAnd => (n as u64).saturating_sub(1),
            AdderKind::CuccaroAsChargedByGe19 => 2 * n as u64,
        }
    }
}

/// Emit an `n`-bit addition costed as Cuccaro et al.'s adder is charged in
/// Gidney-Ekera 2019: `2n` Toffoli gates, with no free measurement-based uncomputation.
pub fn cuccaro_add_ge19(b: &mut CircuitBuilder, n: u32) -> u64 {
    let toffoli = 2 * n as u64;
    b.scope(format!("cuccaro_add_ge19[{n}]"), |b| {
        b.alloc(1);
        b.repeat("carry_chain", toffoli, |b| {
            b.toffoli();
            b.clifford();
        });
        b.free(1);
    });
    toffoli
}

/// Emit an in-place `n`-bit ripple-carry addition `a += b`.
///
/// Structure: one AND per carry position (`n - 1` of them), a CNOT chain for the sum
/// bits, and measurement-based uncomputation of the carry ancilla.
///
/// Returns the number of AND computations emitted, for use by callers that need to
/// reason about depth.
pub fn ripple_carry_add(b: &mut CircuitBuilder, n: u32) -> u64 {
    if n == 0 {
        return 0;
    }
    let ands = (n as u64).saturating_sub(1);
    b.scope(format!("ripple_carry_add[{n}]"), |b| {
        if ands == 0 {
            b.clifford();
            return;
        }
        b.alloc(ands);
        // Forward carry chain: each position computes one AND and fixes up with CNOTs.
        b.repeat("carry_chain", ands, |b| {
            b.and_compute();
            b.cliffords(2);
        });
        // Sum bits.
        b.repeat("sum_bits", n as u64, |b| {
            b.clifford();
        });
        // Reverse chain: measurement-based uncomputation, free of T gates.
        b.repeat("carry_uncompute", ands, |b| {
            b.and_uncompute();
        });
        b.free(ands);
    });
    ands
}

/// Emit a singly-controlled `n`-bit addition.
///
/// The control costs one additional AND relative to the uncontrolled adder.
pub fn controlled_add(b: &mut CircuitBuilder, n: u32) -> u64 {
    let mut ands = 0;
    b.scope(format!("controlled_add[{n}]"), |b| {
        b.alloc(1);
        b.and_compute();
        ands += 1;
        ands += ripple_carry_add(b, n);
        b.and_uncompute();
        b.free(1);
    });
    ands
}

/// Emit an `n`-bit comparison `a < b`, leaving the result in a borrowed output qubit.
///
/// Structurally a carry chain without the sum write-back.
pub fn compare(b: &mut CircuitBuilder, n: u32) -> u64 {
    let ands = (n as u64).saturating_sub(1);
    b.scope(format!("compare[{n}]"), |b| {
        if ands == 0 {
            b.clifford();
            return;
        }
        b.alloc(ands);
        b.repeat("borrow_chain", ands, |b| {
            b.and_compute();
            b.cliffords(2);
        });
        b.clifford();
        b.repeat("borrow_uncompute", ands, |b| {
            b.and_uncompute();
        });
        b.free(ands);
    });
    ands
}

/// Emit a modular addition `a = (a + b) mod N` in the textbook representation.
///
/// Costs roughly three carry chains: the addition, the comparison against the modulus,
/// and the conditional correction. Compare with [`padded_add`], which trades qubits for
/// a threefold reduction in gate count.
pub fn modular_add_textbook(b: &mut CircuitBuilder, n: u32) -> u64 {
    let mut ands = 0;
    b.scope(format!("modular_add_textbook[{n}]"), |b| {
        ands += ripple_carry_add(b, n);
        ands += compare(b, n);
        ands += controlled_add(b, n);
    });
    ands
}

/// Emit an addition in the coset (padded) representation of Zalka.
///
/// Modular reduction becomes implicit: the register carries `padding` extra high bits
/// so that values may exceed the modulus without wrapping, and reduction is deferred.
/// The cost is one plain carry chain over the widened register, rather than three
/// chains over the narrow one.
///
/// `padding` is conventionally `ceil(log2(n)) + O(1)`; the caller supplies it so the
/// choice is visible in the assumption set rather than buried here.
pub fn padded_add(b: &mut CircuitBuilder, n: u32, padding: u32) -> u64 {
    ripple_carry_add(b, n + padding)
}

/// The conventional coset padding width for an `n`-bit modulus: `ceil(log2 n) + slack`.
///
/// Note this is `ceil(log2 n)`, not the bit-length of `n`. The two differ by one at
/// every power of two, which is precisely where RSA and ECC key sizes live.
pub fn coset_padding(n: u32, slack: u32) -> u32 {
    let ceil_log2 = if n <= 1 { 0 } else { n.next_power_of_two().trailing_zeros() };
    ceil_log2 + slack
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::CircuitBuilder;

    fn ands_of<F: FnOnce(&mut CircuitBuilder)>(f: F) -> u128 {
        let mut b = CircuitBuilder::new("test");
        f(&mut b);
        b.finish().fold().unwrap().resources.total_toffoli().get()
    }

    #[test]
    fn ripple_carry_matches_the_n_minus_one_oracle() {
        // Oracle: Gidney's construction uses n-1 AND gates for an n-bit addition.
        for n in [8u32, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096] {
            let got = ands_of(|b| {
                ripple_carry_add(b, n);
            });
            assert_eq!(got, (n - 1) as u128, "n={n}");
        }
    }

    #[test]
    fn uncomputation_is_free_of_t_gates() {
        let mut b = CircuitBuilder::new("t");
        ripple_carry_add(&mut b, 64);
        let r = b.finish().fold().unwrap().resources;
        assert_eq!(r.and_compute.get(), 63);
        assert_eq!(r.and_uncompute.get(), 63);
        // Uncompute contributes measurements, never T gates.
        assert_eq!(r.bare_t.get(), 0);
    }

    #[test]
    fn controlled_add_costs_exactly_one_more_and() {
        for n in [16u32, 64, 256, 1024] {
            let plain = ands_of(|b| {
                ripple_carry_add(b, n);
            });
            let ctrl = ands_of(|b| {
                controlled_add(b, n);
            });
            assert_eq!(ctrl, plain + 1, "n={n}");
        }
    }

    #[test]
    fn padded_add_is_cheaper_than_textbook_modular_add() {
        let n = 2048u32;
        let pad = coset_padding(n, 2);
        let padded = ands_of(|b| {
            padded_add(b, n, pad);
        });
        let textbook = ands_of(|b| {
            modular_add_textbook(b, n);
        });
        assert!(
            padded * 2 < textbook,
            "coset representation should be well over 2x cheaper: padded={padded}, textbook={textbook}"
        );
    }

    #[test]
    fn cuccaro_is_charged_at_two_n_as_ge19_states() {
        for n in [64u32, 256, 1024, 2048] {
            let got = ands_of(|b| {
                cuccaro_add_ge19(b, n);
            });
            assert_eq!(got, 2 * n as u128, "GE19 section 2.5 charges 2n per n-bit addition");
        }
    }

    #[test]
    fn the_two_adders_differ_by_about_a_factor_of_two() {
        let n = 2048u32;
        let gidney = AdderKind::GidneyTemporaryAnd.analytic_cost(n) as f64;
        let cuccaro = AdderKind::CuccaroAsChargedByGe19.analytic_cost(n) as f64;
        let ratio = cuccaro / gidney;
        assert!(
            ratio > 1.9 && ratio < 2.1,
            "adder choice is worth ~2x, got {ratio:.3}"
        );
    }

    #[test]
    fn adder_kind_emit_matches_its_analytic_cost() {
        for kind in [AdderKind::GidneyTemporaryAnd, AdderKind::CuccaroAsChargedByGe19] {
            for n in [32u32, 128, 1024] {
                let got = ands_of(|b| {
                    kind.emit(b, n);
                });
                assert_eq!(got, kind.analytic_cost(n) as u128, "{kind:?} n={n}");
            }
        }
    }

    #[test]
    fn coset_padding_is_about_log2_n() {
        assert_eq!(coset_padding(2048, 0), 11, "ceil(log2 2048) = 11, not bit-length 12");
        assert_eq!(coset_padding(256, 0), 8);
        assert_eq!(coset_padding(1000, 0), 10, "ceil(log2 1000) = 10");
        assert_eq!(coset_padding(1024, 0), 10);
        assert_eq!(coset_padding(1025, 0), 11);
    }

    #[test]
    fn adder_ancilla_is_released() {
        let mut b = CircuitBuilder::new("t");
        ripple_carry_add(&mut b, 128);
        let f = b.finish().fold().unwrap();
        assert_eq!(f.net_alloc, 0, "adder must not leak ancilla");
        assert_eq!(f.peak_alloc, 127);
    }

    #[test]
    fn cost_is_monotone_in_width() {
        // Law 7: increasing the problem size must never decrease cost.
        let mut prev = 0u128;
        for n in (8u32..=1024).step_by(8) {
            let c = ands_of(|b| {
                ripple_carry_add(b, n);
            });
            assert!(c >= prev, "non-monotone at n={n}");
            prev = c;
        }
    }
}
