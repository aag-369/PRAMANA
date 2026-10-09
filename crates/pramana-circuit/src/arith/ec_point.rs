//! Reversible elliptic-curve arithmetic over a prime field.
//!
//! Follows Roetteler, Naehrig, Svore & Lauter, *Quantum resource estimates for computing
//! elliptic curve discrete logarithms* (arXiv:1706.06752, ASIACRYPT 2017).
//!
//! # What is derived here and what is taken from the source
//!
//! The source reports that the asymptotically dominant cost of every modular routine
//! comes from constant incrementers, and gives per-routine leading coefficients in its
//! Table 1. PRAMANA takes **one** calibrated primitive from that table — a modular
//! constant addition costing `16 n log2(n)` Toffoli gates — and derives everything above
//! it by composition:
//!
//! | Routine | Composition | Emergent leading cost |
//! |---|---|---|
//! | Montgomery multiplication | `n` modular additions | `16 n^2 log2 n` |
//! | Montgomery squaring | `n` modular additions | `16 n^2 log2 n` |
//! | Kaliski modular inversion | `2n` modular additions | `32 n^2 log2 n` |
//! | Point addition | 4 inversions + 2 squarings + 4 multiplications | `224 n^2 log2 n` |
//! | Full ECDLP | `2n` controlled point additions | `448 n^3 log2 n` |
//!
//! Each of those four emergent figures matches the source. In particular the famous
//! `224 = 4*32 + 2*16 + 4*16` and the resulting `448` are **never written down** in
//! PRAMANA: they fall out of assembling the circuit, which is the whole point of
//! synthesising rather than tabulating.
//!
//! # Known limitation
//!
//! The source's sub-leading `+2045 n^2` term for a point addition is an empirical
//! regression over its own simulated circuits, not something derivable from the
//! published subroutine table. PRAMANA therefore reproduces the leading term exactly and
//! under-reports the absolute count at cryptographic sizes. This is recorded as an open
//! divergence rather than closed by fitting a constant to the target.

use crate::ir::CircuitBuilder;

/// Toffoli cost of a constant incrementer on `n` bits.
///
/// The dominant cost of every modular routine in the source construction is the constant
/// incrementer, whose recursive carry structure gives `log2 n` levels of `n` gates.
pub fn incrementer_cost(n: u32) -> u64 {
    let logn = ceil_log2(n).max(1) as u64;
    n as u64 * logn
}

/// Incrementers per modular constant addition.
///
/// Calibrated from Table 1 of arXiv:1706.06752, which gives `add_const_modp` a Toffoli
/// count of `16 n log2(n) - 26.9 n`. This is the single primitive PRAMANA imports from
/// the source; every higher-level cost is composed from it.
pub const INCREMENTERS_PER_MODULAR_ADDITION: u64 = 16;

/// `ceil(log2 n)`.
pub fn ceil_log2(n: u32) -> u32 {
    if n <= 1 {
        0
    } else {
        n.next_power_of_two().trailing_zeros()
    }
}

/// Emit a modular constant addition on `n`-bit operands.
pub fn modular_add_const(b: &mut CircuitBuilder, n: u32) -> u64 {
    let per_inc = incrementer_cost(n);
    let total = per_inc * INCREMENTERS_PER_MODULAR_ADDITION;
    b.scope(format!("add_const_modp[{n}]"), |b| {
        b.repeat("incrementers", INCREMENTERS_PER_MODULAR_ADDITION, |b| {
            b.repeat("incrementer_gates", per_inc, |b| {
                b.toffoli();
            });
        });
    });
    total
}

/// Emit a Montgomery modular multiplication: `n` rounds of modular constant addition.
pub fn mul_modp_montgomery(b: &mut CircuitBuilder, n: u32) -> u64 {
    let mut total = 0;
    b.scope(format!("mul_modp_montgomery[{n}]"), |b| {
        b.alloc(2 * n as u64 + 4);
        b.repeat("montgomery_round", n as u64, |b| {
            modular_add_const(b, n);
        });
        total = n as u64 * modular_add_const_cost(n);
        b.free(2 * n as u64 + 4);
    });
    total
}

/// Emit a Montgomery modular squaring. Same round structure as multiplication.
pub fn squ_modp_montgomery(b: &mut CircuitBuilder, n: u32) -> u64 {
    let mut total = 0;
    b.scope(format!("squ_modp_montgomery[{n}]"), |b| {
        b.alloc(2 * n as u64 + 5);
        b.repeat("montgomery_round", n as u64, |b| {
            modular_add_const(b, n);
        });
        total = n as u64 * modular_add_const_cost(n);
        b.free(2 * n as u64 + 5);
    });
    total
}

/// Ancilla held by a modular inversion, from Table 1 of arXiv:1706.06752.
///
/// The source states the inversion needs `7n + 2 ceil(log2 n) + 9` qubits in total, and
/// that this is what sets the peak qubit count of the whole point-addition circuit.
pub fn inversion_qubits(n: u32) -> u64 {
    7 * n as u64 + 2 * ceil_log2(n) as u64 + 9
}

/// Emit a Kaliski almost-inverse modular inversion.
///
/// The source applies its round circuit "precisely 2n times"; each round costs one
/// modular constant addition, giving `32 n^2 log2 n` by composition.
pub fn inv_modp(b: &mut CircuitBuilder, n: u32) -> u64 {
    let mut total = 0;
    let anc = inversion_qubits(n);
    b.scope(format!("inv_modp[{n}]"), |b| {
        b.alloc(anc);
        b.repeat("kaliski_round", 2 * n as u64, |b| {
            modular_add_const(b, n);
        });
        total = 2 * n as u64 * modular_add_const_cost(n);
        b.free(anc);
    });
    total
}

/// Analytic cost of one modular constant addition, for oracle cross-checks.
pub fn modular_add_const_cost(n: u32) -> u64 {
    incrementer_cost(n) * INCREMENTERS_PER_MODULAR_ADDITION
}

/// Subroutine multiplicities in one controlled point addition.
///
/// From arXiv:1706.06752: "there are a total of 4 inverters, 2 squarers, and 4
/// multipliers". These three integers are the entire specification of the point-addition
/// group law as far as leading-order cost is concerned; the coefficient 224 is *not*
/// written anywhere in PRAMANA, it emerges from this composition.
pub const INVERSIONS_PER_POINT_ADDITION: u64 = 4;
/// Squarings per controlled point addition.
pub const SQUARINGS_PER_POINT_ADDITION: u64 = 2;
/// Multiplications per controlled point addition.
pub const MULTIPLICATIONS_PER_POINT_ADDITION: u64 = 4;

/// Emit one controlled elliptic-curve point addition.
pub fn point_addition(b: &mut CircuitBuilder, n: u32) -> u64 {
    let mut total = 0;
    b.scope(format!("point_addition[{n}]"), |b| {
        for _ in 0..INVERSIONS_PER_POINT_ADDITION {
            total += inv_modp(b, n);
        }
        for _ in 0..SQUARINGS_PER_POINT_ADDITION {
            total += squ_modp_montgomery(b, n);
        }
        for _ in 0..MULTIPLICATIONS_PER_POINT_ADDITION {
            total += mul_modp_montgomery(b, n);
        }
    });
    total
}

/// Persistent data qubits of the point-addition circuit, excluding inversion ancilla.
///
/// arXiv:1706.06752: "an additional qubit is needed for the control qubit and 2n more
/// qubits are needed since two n-qubit registers need to hold intermediate results".
pub fn point_addition_data_qubits(n: u32) -> u64 {
    2 * n as u64 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toffoli_of<F: FnOnce(&mut CircuitBuilder)>(f: F) -> u128 {
        let mut b = CircuitBuilder::new("t");
        f(&mut b);
        b.finish().fold().unwrap().resources.total_toffoli().get()
    }

    /// Extract the leading coefficient `a` from a cost of the form `a * n^k * log2(n)`.
    ///
    /// Uses `ceil(log2 n)`, the number of levels in the incrementer's carry tree, which
    /// is what a circuit actually has. The source's closed forms use a continuous
    /// `log2(n)`; the two coincide exactly at powers of two and diverge by up to 11% at
    /// the worst case just above one (n=521 gives ceil=10 against 9.03). That is a real
    /// property of the model, not an error, so the oracle is stated in the model's own
    /// basis and a separate test pins agreement with the continuous form.
    fn leading_coeff(cost: u128, n: u32, power: u32) -> f64 {
        let nf = n as f64;
        cost as f64 / (nf.powi(power as i32) * ceil_log2(n) as f64)
    }

    #[test]
    fn modular_multiplication_leading_coefficient_is_sixteen() {
        for n in [128u32, 256, 384, 521] {
            let c = toffoli_of(|b| {
                mul_modp_montgomery(b, n);
            });
            let a = leading_coeff(c, n, 2);
            assert!(
                (a - 16.0).abs() < 0.6,
                "n={n}: expected 16 n^2 log2 n, got coefficient {a:.3}"
            );
        }
    }

    #[test]
    fn modular_inversion_leading_coefficient_is_thirty_two() {
        for n in [128u32, 256, 512] {
            let c = toffoli_of(|b| {
                inv_modp(b, n);
            });
            let a = leading_coeff(c, n, 2);
            assert!(
                (a - 32.0).abs() < 1.2,
                "n={n}: expected 32 n^2 log2 n, got coefficient {a:.3}"
            );
        }
    }

    #[test]
    fn point_addition_leading_coefficient_emerges_as_224() {
        // 224 = 4*32 + 2*16 + 4*16. This number appears nowhere in PRAMANA's source;
        // it is produced by assembling four inversions, two squarings and four
        // multiplications and measuring the result.
        for n in [128u32, 256, 512] {
            let c = toffoli_of(|b| {
                point_addition(b, n);
            });
            let a = leading_coeff(c, n, 2);
            assert!(
                (a - 224.0).abs() < 8.0,
                "n={n}: expected 224 n^2 log2 n, got coefficient {a:.3}"
            );
        }
    }

    #[test]
    fn point_addition_peak_qubits_match_the_published_nine_n() {
        // 9n + 2 ceil(log2 n) + 10, composed from the inversion's qubit budget plus the
        // control qubit and the two intermediate registers.
        for n in [128u32, 256, 384, 521] {
            let mut b = CircuitBuilder::new("t");
            point_addition(&mut b, n);
            let peak = b.finish().fold().unwrap().peak_alloc;
            let total = point_addition_data_qubits(n) + peak;
            let published = 9 * n as u64 + 2 * ceil_log2(n) as u64 + 10;
            assert_eq!(total, published, "n={n}");
        }
    }

    #[test]
    fn discrete_and_continuous_logs_agree_at_powers_of_two() {
        // Where ceil(log2 n) == log2(n), PRAMANA's discrete carry-tree model must match
        // the source's continuous closed form exactly.
        for n in [128u32, 256, 512, 1024] {
            let c = toffoli_of(|b| {
                point_addition(b, n);
            });
            let continuous = c as f64 / ((n as f64).powi(2) * (n as f64).log2());
            assert!(
                (continuous - 224.0).abs() < 1e-6,
                "n={n}: expected exactly 224, got {continuous}"
            );
        }
    }

    #[test]
    fn non_power_of_two_sizes_cost_more_than_the_continuous_form_predicts() {
        // A circuit cannot have a fractional number of carry-tree levels, so real
        // curves whose bit length is not a power of two pay a rounding penalty that the
        // published closed form smooths over. P-521 is the extreme case.
        let n = 521u32;
        let c = toffoli_of(|b| {
            point_addition(b, n);
        }) as f64;
        let continuous = 224.0 * (n as f64).powi(2) * (n as f64).log2();
        let ratio = c / continuous;
        assert!(
            ratio > 1.05 && ratio < 1.15,
            "P-521 rounding penalty should be ~11%, got {ratio:.4}"
        );
    }

    #[test]
    fn inversion_dominates_the_qubit_budget() {
        // The source notes the peak is set by the inversion, not by the multipliers.
        let n = 256;
        let mut b = CircuitBuilder::new("t");
        mul_modp_montgomery(&mut b, n);
        let mul_peak = b.finish().fold().unwrap().peak_alloc;
        assert!(mul_peak < inversion_qubits(n));
    }

    #[test]
    fn ancilla_are_released_by_every_routine() {
        for n in [64u32, 256] {
            let mut b = CircuitBuilder::new("t");
            point_addition(&mut b, n);
            assert_eq!(b.finish().fold().unwrap().net_alloc, 0, "n={n}");
        }
    }

    #[test]
    fn ceil_log2_is_correct() {
        assert_eq!(ceil_log2(256), 8);
        assert_eq!(ceil_log2(255), 8);
        assert_eq!(ceil_log2(257), 9);
        assert_eq!(ceil_log2(521), 10);
    }
}
