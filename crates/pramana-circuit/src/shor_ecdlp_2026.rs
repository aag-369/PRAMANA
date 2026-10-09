//! Shor's ECDLP algorithm with the 2026 point-addition circuits.
//!
//! Implements Schrottenloher, *Optimized Point Addition Circuits for Elliptic Curve
//! Discrete Logarithms* (arXiv:2606.02235), which reconstructs in the open the circuits
//! Babbush et al. (arXiv:2603.28846) reported but withheld behind a zero-knowledge proof.
//!
//! # Why this supersedes the Roetteler pipeline
//!
//! `shor_ecdlp.rs` implements Roetteler et al. 2017: 2,330 logical qubits and 1.29e11
//! Toffoli gates for a 256-bit curve. This construction needs about **1,200 qubits and
//! 5.8e7 Toffoli gates** — roughly 2,200 times fewer gates.
//!
//! PRAMANA keeps both. The 2017 figure is the anchor point of the algorithmic-improvement
//! series the risk engine fits; this one is what an attacker would actually build.
//!
//! # Structure
//!
//! Shor's algorithm with a semiclassical Fourier transform reduces to windowed point
//! addition. With window `w`, the two `n`-bit scalar registers become `2n/w` windowed
//! additions, and four of those are removable (the first becomes a lookup, the last three
//! are absorbed into classical post-processing), so secp256k1 needs 28.
//!
//! Each windowed point addition (Algorithm 1 of the source) performs three table lookups,
//! two **in-place modular multiplications**, one modular squaring, and `O(n)` linear work.
//! The in-place multiplication dominates at about 90% of the cost and is built from the
//! split extended Euclidean algorithm in [`crate::arith::eea`].

use crate::arith::eea::{
    EeaParams, COMPRESSION_GROUP, COMPRESSION_TOFFOLI, MSB_COMPARE_BITS,
    PSEUDO_MERSENNE_LSB_BITS,
};
use crate::ir::{CircuitBuilder, CircuitIR};
use crate::pipeline::{AttackCircuit, Citation, SynthesisResult};
use crate::target::{
    CryptoTarget, RepetitionModel, SynthesisError, SynthesisOptions, ValidityRegime,
};
use pramana_units::LogicalQubits;
use serde::{Deserialize, Serialize};

/// Window size over the scalar registers. The source uses 16, following Babbush et al.
pub const WINDOW_BITS: u32 = 16;

/// Point additions removed by classical post-processing.
///
/// The first is replaced by a lookup of its result; the last three are absorbed into
/// post-processing (Ekera). For secp256k1 this takes `2n/w = 32` down to 28.
pub const REMOVABLE_ADDITIONS: u64 = 4;

/// Table lookups per point addition: the point, then `3x`, then the point again.
pub const LOOKUPS_PER_ADDITION: u64 = 3;

/// In-place modular multiplications per point addition.
pub const INPLACE_MULTIPLICATIONS_PER_ADDITION: u64 = 2;

/// Share of the point-addition cost taken by the two in-place multiplications.
///
/// Table 3 of the source: 90%, with the modular squaring taking 9% and linear work the
/// remainder. PRAMANA builds the multiplications explicitly and scales up by this share
/// rather than modelling the squaring separately, because the source reports the split but
/// not the squaring's internal structure.
pub const INPLACE_MULTIPLICATION_SHARE: f64 = 0.90;

/// Which point on the space-time frontier to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Optimisation {
    /// Minimise qubits. Bezout replay uses ancilla-free CDKM arithmetic.
    Space,
    /// Minimise gates. Bezout replay uses Gidney adders, costing about `n` extra ancillas.
    Gate,
}

impl Optimisation {
    /// Toffoli cost of a controlled `n`-bit addition under this choice.
    ///
    /// CDKM controlled addition costs `3n`; Gidney's costs `2n` but needs about `n`
    /// ancillas, which are unavailable during the Bezout replay unless space is spent.
    pub fn controlled_add(self, bits: u32) -> u64 {
        match self {
            Optimisation::Space => 3 * bits as u64,
            Optimisation::Gate => 2 * bits as u64,
        }
    }

    /// Extra ancilla qubits this choice requires.
    pub fn extra_ancilla(self, modulus_bits: u32) -> u64 {
        match self {
            Optimisation::Space => 0,
            Optimisation::Gate => modulus_bits as u64,
        }
    }
}

/// Parameters of a 2026 ECDLP circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ecdlp2026Params {
    /// Bit length of the field prime.
    pub curve_bits: u32,
    /// Window over the scalar registers.
    pub window_bits: u32,
    /// Whether the prime is pseudo-Mersenne, `2^u - f` with `f` small.
    ///
    /// secp256k1's prime is `2^256 - 4294968273`, which collapses the modular reductions
    /// into small constant additions. NIST P-256 is not of this form, so the two curves
    /// are genuinely different targets and PRAMANA does not transfer one's estimate to the
    /// other.
    pub pseudo_mersenne: bool,
    /// Which frontier point to build.
    pub optimisation: Optimisation,
}

impl Ecdlp2026Params {
    /// secp256k1, the Bitcoin curve, whose prime is pseudo-Mersenne.
    pub fn secp256k1(optimisation: Optimisation) -> Self {
        Self {
            curve_bits: 256,
            window_bits: WINDOW_BITS,
            pseudo_mersenne: true,
            optimisation,
        }
    }

    /// A generic prime-field curve of the given size, such as NIST P-256.
    pub fn generic(curve_bits: u32, optimisation: Optimisation) -> Self {
        Self {
            curve_bits,
            window_bits: WINDOW_BITS,
            pseudo_mersenne: false,
            optimisation,
        }
    }

    /// The split-EEA parameters this circuit uses.
    pub fn eea(&self) -> EeaParams {
        EeaParams::new(self.curve_bits)
    }

    /// Windowed point additions in the whole algorithm.
    pub fn point_additions(&self) -> u64 {
        let raw = (2 * self.curve_bits as u64).div_ceil(self.window_bits as u64);
        raw.saturating_sub(REMOVABLE_ADDITIONS).max(1)
    }

    /// Toffoli cost of one table lookup: `2^w` by unary iteration. Unlookup is nearly free
    /// through measurement-based uncomputation.
    pub fn lookup_toffoli(&self) -> u64 {
        1u64 << self.window_bits.min(62)
    }

    /// Toffoli cost of one Euclidean-pass iteration at register width `bits`.
    fn gcd_iteration_toffoli(&self, bits: u32) -> u64 {
        // Comparison `u > v` approximated on the most significant bits (CDKM, 2 per bit).
        let compare = 2 * MSB_COMPARE_BITS as u64;
        // Conditional swap of the two working registers.
        let swap = bits as u64;
        // Conditional subtraction. Ancillas are available during this pass, so Gidney's
        // adder applies at one Toffoli per bit.
        let subtract = bits as u64;
        compare + swap + subtract
    }

    /// Toffoli cost of one Bezout-replay iteration, over full-width registers.
    fn bezout_iteration_toffoli(&self) -> u64 {
        let n = self.curve_bits;
        let lsb = PSEUDO_MERSENNE_LSB_BITS.min(n);
        let msb = MSB_COMPARE_BITS.min(n);
        // Gidney's constant adder with dirty ancillas costs 3 Toffoli per bit.
        let constant_add = |bits: u32| 3 * bits as u64;

        let doubling = if self.pseudo_mersenne {
            // Shift, then add the small `f` into the low bits (Algorithm 7).
            constant_add(lsb)
        } else {
            // Shift, compare on the high bits, then subtract the full modulus (Algorithm 6).
            2 * msb as u64 + constant_add(n)
        };

        let modular_add = if self.pseudo_mersenne {
            // Controlled addition, then add `f` on overflow, then clear the overflow bit
            // by comparison (Algorithm 10).
            self.optimisation.controlled_add(n) + constant_add(lsb) + 2 * msb as u64
        } else {
            // Controlled addition, compare, subtract the modulus, compare again
            // (Algorithm 9).
            self.optimisation.controlled_add(n) + 2 * msb as u64 + constant_add(n)
                + 2 * msb as u64
        };

        let swap = n as u64;
        doubling + modular_add + swap
    }

    /// Toffoli cost of one in-place modular multiplication.
    ///
    /// Two Euclidean passes (forward to build the garbage vector, backward to clear it)
    /// and one Bezout replay.
    pub fn inplace_multiplication_toffoli(&self) -> u64 {
        let eea = self.eea();
        let iterations = eea.iterations();

        let gcd_pass: u64 = (0..iterations)
            .map(|i| self.gcd_iteration_toffoli(eea.register_bits_at(i)))
            .sum();
        let compression = iterations.div_ceil(COMPRESSION_GROUP) * COMPRESSION_TOFFOLI;
        let bezout = iterations * self.bezout_iteration_toffoli();

        2 * (gcd_pass + compression) + bezout
    }

    /// Toffoli cost of one windowed point addition, excluding the lookups.
    pub fn point_addition_arithmetic_toffoli(&self) -> u64 {
        let mults =
            INPLACE_MULTIPLICATIONS_PER_ADDITION * self.inplace_multiplication_toffoli();
        // Scale up for the modular squaring and linear work, which the source reports as a
        // share rather than a structure.
        (mults as f64 / INPLACE_MULTIPLICATION_SHARE) as u64
    }

    /// Toffoli cost of one windowed point addition, matching Table 1 of the source.
    pub fn point_addition_toffoli(&self) -> u64 {
        LOOKUPS_PER_ADDITION * self.lookup_toffoli() + self.point_addition_arithmetic_toffoli()
    }

    /// Peak logical qubits, matching Table 2 of the source.
    ///
    /// The Bezout replay sets the peak. The scalar registers do not count, because the
    /// semiclassical Fourier transform recycles a single control qubit.
    pub fn logical_qubits(&self) -> u64 {
        self.eea().peak_qubits()
            + self.optimisation.extra_ancilla(self.curve_bits)
            + self.window_bits as u64
    }

    /// Total Toffoli gates for the whole algorithm.
    pub fn total_toffoli(&self) -> u64 {
        self.point_additions() * self.point_addition_toffoli()
    }
}

/// The Schrottenloher 2026 ECDLP construction.
#[derive(Debug, Clone, Copy)]
pub struct ShorEcdlp2026 {
    /// Which frontier point to build.
    pub optimisation: Optimisation,
    /// Whether to assume a pseudo-Mersenne prime.
    pub pseudo_mersenne: bool,
}

impl Default for ShorEcdlp2026 {
    fn default() -> Self {
        Self {
            optimisation: Optimisation::Gate,
            pseudo_mersenne: false,
        }
    }
}

impl ShorEcdlp2026 {
    /// The secp256k1 configuration, which exploits the pseudo-Mersenne prime.
    pub fn secp256k1(optimisation: Optimisation) -> Self {
        Self {
            optimisation,
            pseudo_mersenne: true,
        }
    }

    /// A generic prime-field configuration, valid for NIST curves.
    pub fn generic(optimisation: Optimisation) -> Self {
        Self {
            optimisation,
            pseudo_mersenne: false,
        }
    }

    /// Parameters for a curve size.
    pub fn params(&self, curve_bits: u32) -> Ecdlp2026Params {
        Ecdlp2026Params {
            curve_bits,
            window_bits: WINDOW_BITS,
            pseudo_mersenne: self.pseudo_mersenne,
            optimisation: self.optimisation,
        }
    }
}

impl AttackCircuit for ShorEcdlp2026 {
    fn id(&self) -> &'static str {
        "shor_ecdlp_schrottenloher_2026"
    }

    fn display_name(&self) -> &'static str {
        "Shor ECDLP (Schrottenloher 2026, split-EEA point addition)"
    }

    fn synthesise(
        &self,
        target: &CryptoTarget,
        _opts: &SynthesisOptions,
    ) -> Result<SynthesisResult, SynthesisError> {
        let n = match target {
            CryptoTarget::Ecdlp { curve_bits } => *curve_bits,
            other => {
                return Err(SynthesisError::WrongTarget {
                    pipeline: self.id(),
                    target: other.id(),
                })
            }
        };
        self.validity().check(self.id(), n)?;
        let p = self.params(n);
        let eea = p.eea();

        let mut b = CircuitBuilder::new(format!("ecdlp2026_{n}"));
        b.repeat("windowed_point_addition", p.point_additions(), |b| {
            b.repeat("table_lookup", LOOKUPS_PER_ADDITION, |b| {
                b.repeat("unary_iteration", p.lookup_toffoli(), |b| {
                    b.and_compute();
                });
            });
            b.repeat(
                "inplace_modular_multiplication",
                INPLACE_MULTIPLICATIONS_PER_ADDITION,
                |b| {
                    b.scope("euclidean_forward", |b| {
                        for i in 0..eea.iterations() {
                            let w = eea.register_bits_at(i);
                            b.repeat("gcd_iteration", p.gcd_iteration_toffoli(w), |b| {
                                b.and_compute();
                            });
                        }
                    });
                    b.repeat(
                        "garbage_compression",
                        eea.iterations().div_ceil(COMPRESSION_GROUP) * COMPRESSION_TOFFOLI,
                        |b| {
                            b.and_compute();
                        },
                    );
                    b.scope("bezout_replay", |b| {
                        b.repeat(
                            "replay_iteration",
                            eea.iterations() * p.bezout_iteration_toffoli(),
                            |b| {
                                b.and_compute();
                            },
                        );
                    });
                    b.scope("euclidean_backward", |b| {
                        for i in 0..eea.iterations() {
                            let w = eea.register_bits_at(i);
                            b.repeat("gcd_iteration", p.gcd_iteration_toffoli(w), |b| {
                                b.and_compute();
                            });
                        }
                        b.repeat(
                            "garbage_compression",
                            eea.iterations().div_ceil(COMPRESSION_GROUP) * COMPRESSION_TOFFOLI,
                            |b| {
                                b.and_compute();
                            },
                        );
                    });
                },
            );
            // Modular squaring and the linear work around it, which the source reports as
            // a share of the total rather than as a structure.
            let overhead = p
                .point_addition_arithmetic_toffoli()
                .saturating_sub(
                    INPLACE_MULTIPLICATIONS_PER_ADDITION * p.inplace_multiplication_toffoli(),
                );
            b.repeat("modular_squaring_and_linear_work", overhead, |b| {
                b.and_compute();
            });
        });

        let circuit = CircuitIR::from_node(
            format!("ecdlp2026_{n}"),
            b.finish(),
            LogicalQubits::new(p.logical_qubits()),
        )?;

        Ok(SynthesisResult {
            circuit,
            repetitions: RepetitionModel::certain(
                "a single run suffices: the discrete logarithm is recovered from one \
                 measurement with probability above one half, and unlike the RSA \
                 space-time trade-offs there is no repetition penalty",
            ),
            pipeline: self.id(),
        })
    }

    fn validity(&self) -> ValidityRegime {
        ValidityRegime {
            min_bits: 128,
            max_bits: 1024,
        }
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "arXiv:2606.02235",
                title: "Optimized Point Addition Circuits for Elliptic Curve Discrete Logarithms",
                year: 2026,
            },
            Citation {
                reference: "arXiv:2603.28846",
                title: "Securing elliptic curve cryptocurrencies against quantum vulnerabilities",
                year: 2026,
            },
            Citation {
                reference: "arXiv:2510.10967",
                title: "Verifiable quantum advantage via optimized DQI circuits (dialog split)",
                year: 2025,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth(arch: ShorEcdlp2026, n: u32) -> SynthesisResult {
        arch.synthesise(
            &CryptoTarget::Ecdlp { curve_bits: n },
            &SynthesisOptions::default(),
        )
        .expect("synthesis succeeds")
    }

    #[test]
    fn secp256k1_needs_twenty_eight_point_additions() {
        // 2n/w = 32, less the four removable by classical post-processing.
        let p = Ecdlp2026Params::secp256k1(Optimisation::Gate);
        assert_eq!(p.point_additions(), 28);
    }

    #[test]
    fn reproduces_published_qubit_counts() {
        // Table 2 of arXiv:2606.02235: 1208 space-optimised, 1462 gate-optimised.
        for (opt, want) in [(Optimisation::Space, 1208.0), (Optimisation::Gate, 1462.0)] {
            let got = Ecdlp2026Params::secp256k1(opt).logical_qubits() as f64;
            let rel = (got - want).abs() / want;
            assert!(rel < 0.05, "{opt:?}: got {got}, published {want} (rel {rel:.4})");
        }
    }

    #[test]
    fn reproduces_published_toffoli_counts() {
        // Table 2: 2^26.11 space-optimised, 2^25.78 gate-optimised, for secp256k1.
        for (opt, exponent) in [(Optimisation::Space, 26.11), (Optimisation::Gate, 25.78)] {
            let got = Ecdlp2026Params::secp256k1(opt).total_toffoli() as f64;
            let want = 2f64.powf(exponent);
            let ratio = got / want;
            assert!(
                (0.7..1.4).contains(&ratio),
                "{opt:?}: got 2^{:.2}, published 2^{exponent} (ratio {ratio:.3})",
                got.log2()
            );
        }
    }

    #[test]
    fn the_pseudo_mersenne_prime_is_a_real_advantage() {
        // Table 1 reports a materially higher gate count for a generic prime, entirely
        // from the modular reduction circuits. secp256k1 and P-256 are therefore not
        // interchangeable targets.
        let special = Ecdlp2026Params::secp256k1(Optimisation::Gate).total_toffoli() as f64;
        let generic = Ecdlp2026Params::generic(256, Optimisation::Gate).total_toffoli() as f64;
        assert!(
            generic > special * 1.2,
            "a generic prime should cost noticeably more: {generic:.3e} vs {special:.3e}"
        );
    }

    #[test]
    fn gate_optimisation_trades_qubits_for_gates() {
        let space = Ecdlp2026Params::secp256k1(Optimisation::Space);
        let gate = Ecdlp2026Params::secp256k1(Optimisation::Gate);
        assert!(gate.logical_qubits() > space.logical_qubits());
        assert!(gate.total_toffoli() < space.total_toffoli());
    }

    #[test]
    fn supersedes_the_2017_construction_by_orders_of_magnitude() {
        // The headline finding of the research dossier, asserted as a test so a regression
        // in either pipeline is caught.
        use crate::shor_ecdlp::ShorEcdlpRNSL;
        let modern = synth(ShorEcdlp2026::secp256k1(Optimisation::Gate), 256);
        let legacy = ShorEcdlpRNSL
            .synthesise(
                &CryptoTarget::Ecdlp { curve_bits: 256 },
                &SynthesisOptions::default(),
            )
            .unwrap();

        let gate_ratio = legacy.circuit.resources.total_toffoli().get() as f64
            / modern.circuit.resources.total_toffoli().get() as f64;
        assert!(
            gate_ratio > 500.0,
            "2017 should cost hundreds of times more gates, got {gate_ratio:.0}x"
        );
        assert!(
            modern.circuit.logical_qubits.get() < legacy.circuit.logical_qubits.get(),
            "and fewer qubits too"
        );
    }

    #[test]
    fn a_single_run_suffices_unlike_the_rsa_trade_offs() {
        let r = synth(ShorEcdlp2026::secp256k1(Optimisation::Gate), 256);
        assert_eq!(r.repetitions.expected_runs, 1.0);
    }

    #[test]
    fn assembled_circuit_matches_the_analytic_model() {
        for n in [192u32, 256, 384] {
            let arch = ShorEcdlp2026::generic(Optimisation::Gate);
            let got = synth(arch, n).circuit.resources.total_toffoli().get() as f64;
            let want = arch.params(n).total_toffoli() as f64;
            let rel = (got - want).abs() / want;
            assert!(rel < 0.01, "n={n}: assembled {got:.4e} vs model {want:.4e}");
        }
    }

    #[test]
    fn cost_is_monotone_in_curve_size() {
        let arch = ShorEcdlp2026::generic(Optimisation::Gate);
        let mut prev = 0u64;
        for n in [128u32, 192, 256, 384, 521] {
            let c = arch.params(n).total_toffoli();
            assert!(c > prev, "non-monotone at n={n}");
            prev = c;
        }
    }

    #[test]
    fn wrong_target_is_rejected() {
        let e = ShorEcdlp2026::default()
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: 2048 },
                &SynthesisOptions::default(),
            )
            .unwrap_err();
        assert!(matches!(e, SynthesisError::WrongTarget { .. }));
    }
}
