//! Reversible extended Euclidean algorithm via the dialog / garbage-vector split.
//!
//! Follows Schrottenloher, *Optimized Point Addition Circuits for Elliptic Curve Discrete
//! Logarithms* (arXiv:2606.02235), which in turn takes the split from Khattar et al.
//! (arXiv:2510.10967).
//!
//! # The idea, and why it saves so much
//!
//! A textbook extended Euclidean algorithm maintains the Bezout coefficients `(r, s)`
//! alongside `(u, v)`, so a reversible implementation must hold both. The split runs the
//! plain Euclidean algorithm on `(u, v)` first, recording only the *branch decisions* it
//! took into a compressed bit-vector, then replays those decisions onto `(r, s)` in a
//! second pass.
//!
//! Two consequences, both large:
//!
//! 1. **Space.** `(u, v)` and `(r, s)` never coexist. The registers freed as `u` and `v`
//!    shrink are reused to store the garbage bits.
//! 2. **A free multiplication.** The replay updates are linear in the recorded bits, so
//!    starting the replay from `(y, 0)` rather than `(1, 0)` yields `y * x^-1 mod q`
//!    instead of `x^-1`. The inversion and an in-place modular multiplication come out of
//!    one pass, where previous constructions needed a separate multiplication.
//!
//! Running the circuit in reverse multiplies by `x` instead of `x^-1`, so a single
//! construction serves both in-place multiplications in a point addition.

use serde::{Deserialize, Serialize};

/// `log2(8/3)`: the expected number of bits removed from `(u, v)` per Euclidean iteration.
///
/// Each iteration halves `v` (one bit), and with probability one half also subtracts `u`
/// from `v` (another bit), so the product `uv` shrinks by a factor of
/// `1/2 * 1/2 + 1/2 * 1/4 = 3/8` per step.
pub const BITS_PER_ITERATION: f64 = 1.4150374992788437;

/// Iterations needed to reduce `2n` bits: `2n / log2(8/3)`, about `1.413n`.
pub const ITERATION_COEFF: f64 = 2.0 / BITS_PER_ITERATION;

/// Standard deviation of the iteration count, as a multiple of `sqrt(n)`.
pub const ITERATION_SD_COEFF: f64 = 0.6;

/// Slack on the iteration count, in standard deviations.
///
/// The source uses four, which makes the point-addition circuit succeed on ten thousand
/// random inputs.
pub const ITERATION_SLACK_SIGMAS: f64 = 4.0;

/// Padding on the working registers, as a multiple of `sqrt(n)`.
pub const PADDING_COEFF: f64 = 2.3;

/// Garbage bits emitted per iteration before compression.
///
/// An iteration records `b0`, and records `b0 & b1` only when `b0` is set, so it emits
/// 1.5 bits on average.
pub const RAW_GARBAGE_BITS_PER_ITERATION: f64 = 1.5;

/// Iterations packed by one compression block.
pub const COMPRESSION_GROUP: u64 = 3;

/// Bits produced by one compression block.
///
/// Three iterations produce three pairs, each of which can only be `(0,0)`, `(1,0)` or
/// `(1,1)`. That is `3^3 = 27 < 2^5` possibilities, so five bits suffice and the block
/// costs five Toffoli gates. Fixing the block size also fixes the garbage register size,
/// which a queue-with-shifts encoding would leave variable.
pub const COMPRESSION_BITS: u64 = 5;

/// Toffoli cost of one compression block.
pub const COMPRESSION_TOFFOLI: u64 = 5;

/// Bits compared when a comparison is approximated by its most significant bits.
///
/// The source uses 40 to 50; comparisons are only required to be right with high
/// probability, and the count controls that probability.
pub const MSB_COMPARE_BITS: u32 = 45;

/// Low-order bits touched when reducing modulo a pseudo-Mersenne prime.
pub const PSEUDO_MERSENNE_LSB_BITS: u32 = 40;

/// Parameters of the split extended Euclidean algorithm for an `n`-bit modulus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EeaParams {
    /// Bit length of the modulus.
    pub modulus_bits: u32,
}

impl EeaParams {
    /// Construct for an `n`-bit modulus.
    pub fn new(modulus_bits: u32) -> Self {
        Self { modulus_bits }
    }

    fn n(&self) -> f64 {
        self.modulus_bits as f64
    }

    /// Fixed iteration count: `1.413n + 4 * 0.6 * sqrt(n)`.
    pub fn iterations(&self) -> u64 {
        let n = self.n();
        (ITERATION_COEFF * n + ITERATION_SLACK_SIGMAS * ITERATION_SD_COEFF * n.sqrt()).ceil()
            as u64
    }

    /// Padding carried on the working registers, `2.3 * sqrt(n)` bits.
    pub fn padding_bits(&self) -> u32 {
        (PADDING_COEFF * self.n().sqrt()).ceil() as u32
    }

    /// Compressed garbage-vector width: `iterations / 3 * 5`, about `2.355n + O(sqrt n)`.
    pub fn garbage_bits(&self) -> u64 {
        self.iterations().div_ceil(COMPRESSION_GROUP) * COMPRESSION_BITS
    }

    /// Register width at iteration `i`.
    ///
    /// `u` and `v` shrink by `0.5 * log2(8/3)` bits per iteration; the padding absorbs the
    /// variance so the schedule holds with high probability.
    pub fn register_bits_at(&self, iteration: u64) -> u32 {
        let shrunk = self.n() + self.padding_bits() as f64
            - 0.5 * BITS_PER_ITERATION * iteration as f64;
        shrunk.max(1.0) as u32
    }

    /// Mean register width across the run, for costing the Euclidean pass.
    pub fn mean_register_bits(&self) -> f64 {
        let it = self.iterations();
        if it == 0 {
            return self.n();
        }
        let total: f64 = (0..it).map(|i| self.register_bits_at(i) as f64).sum();
        total / it as f64
    }

    /// Peak logical qubits held by the whole in-place multiplication.
    ///
    /// The Bezout replay dominates: two `n`-bit registers plus the garbage vector, giving
    /// `2n + 2.355n = 4.355n + O(sqrt n)`.
    pub fn peak_qubits(&self) -> u64 {
        2 * self.modulus_bits as u64 + self.garbage_bits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iteration_count_matches_the_published_asymptotic() {
        // The source states 1.413n + c*sqrt(n) with c about 2.4.
        let p = EeaParams::new(256);
        let it = p.iterations() as f64;
        let expected = 1.413 * 256.0 + 2.4 * 16.0;
        assert!(
            (it - expected).abs() / expected < 0.02,
            "expected about {expected:.0} iterations, got {it}"
        );
    }

    #[test]
    fn garbage_width_approaches_its_asymptotic_coefficient() {
        // The published 2.355n is asymptotic: 1.413/3*5 = 2.355. At cryptographic sizes
        // the O(sqrt n) slack still contributes visibly, so the coefficient is only
        // approached at large n. Testing the asymptote directly, rather than asserting it
        // holds at n=256, is what the source actually claims.
        let far = EeaParams::new(1_000_000);
        let per_n = far.garbage_bits() as f64 / 1_000_000.0;
        assert!(
            (2.35..2.37).contains(&per_n),
            "expected the coefficient to approach 2.355, got {per_n:.4}"
        );
    }

    #[test]
    fn peak_space_approaches_four_point_three_five_five_n() {
        let far = EeaParams::new(1_000_000);
        let per_n = far.peak_qubits() as f64 / 1_000_000.0;
        assert!(
            (4.35..4.37).contains(&per_n),
            "expected the coefficient to approach 4.355, got {per_n:.4}"
        );
    }

    #[test]
    fn reproduces_the_published_qubit_count_for_secp256k1() {
        // Table 1 of arXiv:2606.02235 reports 1192 logical qubits for the space-optimised
        // circuit. This is the concrete claim, and it is stronger than the asymptote.
        let p = EeaParams::new(256);
        let got = p.peak_qubits() as f64;
        let rel = (got - 1192.0).abs() / 1192.0;
        assert!(rel < 0.03, "expected about 1192 qubits, got {got} (rel {rel:.4})");
    }

    #[test]
    fn the_sqrt_slack_is_material_at_cryptographic_sizes() {
        // Worth pinning: reading 2.355n as if it held at n=256 understates the register
        // by about 11%, which at these sizes is the difference between two published
        // qubit counts.
        let p = EeaParams::new(256);
        let asymptotic = 2.355 * 256.0;
        let actual = p.garbage_bits() as f64;
        assert!(
            actual > asymptotic * 1.05,
            "slack should add materially at n=256: {actual} vs {asymptotic:.0}"
        );
    }

    #[test]
    fn registers_shrink_monotonically_and_never_vanish() {
        let p = EeaParams::new(256);
        let mut prev = u32::MAX;
        for i in 0..p.iterations() {
            let w = p.register_bits_at(i);
            assert!(w <= prev, "register grew at iteration {i}");
            assert!(w >= 1);
            prev = w;
        }
    }

    #[test]
    fn mean_register_width_is_about_half_the_modulus() {
        // Registers start above n and end near zero, so the mean sits near n/2.
        let p = EeaParams::new(256);
        let mean = p.mean_register_bits();
        assert!(
            (100.0..190.0).contains(&mean),
            "unexpected mean register width {mean:.1}"
        );
    }

    #[test]
    fn space_scales_linearly_in_the_modulus() {
        let a = EeaParams::new(256).peak_qubits() as f64 / 256.0;
        let b = EeaParams::new(521).peak_qubits() as f64 / 521.0;
        assert!((a - b).abs() < 0.15, "coefficient drifted: {a:.3} vs {b:.3}");
    }
}
