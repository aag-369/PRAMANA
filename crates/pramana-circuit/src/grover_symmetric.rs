//! Grover search against symmetric primitives.
//!
//! PRAMANA needs this to say, with a computed number rather than an assertion, that
//! symmetric assets are *not* urgent. "AES is fine, just double the key length" is
//! received wisdom; a triage tool should be able to show its working.
//!
//! # What is derived and what is imported
//!
//! The Grover structure is pure arithmetic and fully derived here: the iteration count is
//! `(pi/4) * 2^(k/2)`, each iteration invokes the oracle twice (compute and uncompute),
//! and the oracle evaluates the cipher once per plaintext-ciphertext block needed to pin
//! the key uniquely.
//!
//! The single imported primitive is the **multiplicative complexity of the AES S-box: 32
//! AND gates**, the Boyar-Peralta result. Everything else about the cipher cost is
//! composed from round structure and key schedule.
//!
//! # Depth limits
//!
//! NIST's call for proposals bounds the depth of any real attack by `MAXDEPTH`, on the
//! grounds that no attacker runs a single coherent computation for geological time.
//! Grover parallelised across `S` machines yields only a `sqrt(S)` speedup, so a depth
//! bound cannot be bought off with more hardware. PRAMANA reports the depth-limited
//! machine requirement alongside the raw gate count, because the raw count alone
//! overstates how close the attack is.

use crate::ir::{CircuitBuilder, CircuitIR};
use crate::pipeline::{AttackCircuit, Citation, SynthesisResult};
use crate::target::{
    CryptoTarget, RepetitionModel, SynthesisError, SynthesisOptions, ValidityRegime,
};
use pramana_units::LogicalQubits;

/// Multiplicative complexity of the AES S-box, in AND gates.
///
/// Boyar & Peralta, *A small depth-16 circuit for the AES S-box* (2011): the AES S-box
/// has multiplicative complexity 32. Since a reversible AND is one Toffoli-equivalent,
/// this is the S-box's non-Clifford cost.
pub const AES_SBOX_AND_GATES: u64 = 32;

/// AES parameters for a given key length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AesParams {
    /// Key length in bits.
    pub key_bits: u32,
    /// Number of rounds.
    pub rounds: u32,
    /// S-box applications in the round function per round (the 16-byte state).
    pub sboxes_per_round: u32,
    /// S-box applications in the key schedule, over the whole cipher.
    pub key_schedule_sboxes: u32,
}

impl AesParams {
    /// Standard AES parameter sets (FIPS 197).
    pub fn for_key_bits(key_bits: u32) -> Option<Self> {
        let (rounds, key_schedule_sboxes) = match key_bits {
            128 => (10, 40),
            192 => (12, 32),
            256 => (14, 56),
            _ => return None,
        };
        Some(Self {
            key_bits,
            rounds,
            sboxes_per_round: 16,
            key_schedule_sboxes,
        })
    }

    /// Total S-box applications in one full encryption, including the key schedule.
    pub fn total_sboxes(&self) -> u64 {
        self.rounds as u64 * self.sboxes_per_round as u64 + self.key_schedule_sboxes as u64
    }

    /// Toffoli-equivalents for one encryption.
    pub fn encryption_toffoli(&self) -> u64 {
        self.total_sboxes() * AES_SBOX_AND_GATES
    }

    /// Plaintext-ciphertext blocks needed to determine the key uniquely.
    ///
    /// One 128-bit block leaves roughly `2^(k-128)` spurious keys, so `ceil(k/128)`
    /// blocks are required.
    pub fn blocks_needed(&self) -> u64 {
        (self.key_bits as u64).div_ceil(128)
    }
}

/// Grover key search against a symmetric primitive.
#[derive(Debug, Default, Clone, Copy)]
pub struct GroverSymmetric;

/// Oracle invocations per Grover iteration: one to compute, one to uncompute.
pub const ORACLE_CALLS_PER_ITERATION: f64 = 2.0;

/// A depth-limited attack assessment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthLimitedCost {
    /// The depth bound assumed.
    pub max_depth: f64,
    /// Parallel machines required to complete the search within that depth.
    ///
    /// Grover across `S` machines gives only a `sqrt(S)` speedup, so halving the depth
    /// costs a factor of four in hardware.
    pub machines_required: f64,
}

impl GroverSymmetric {
    /// Grover iterations for a `k`-bit key: `(pi/4) * 2^(k/2)`.
    pub fn iterations(key_bits: u32) -> f64 {
        std::f64::consts::FRAC_PI_4 * 2f64.powf(key_bits as f64 / 2.0)
    }

    /// Total Toffoli-equivalents for the unconstrained attack.
    pub fn total_toffoli(p: &AesParams) -> f64 {
        Self::iterations(p.key_bits)
            * ORACLE_CALLS_PER_ITERATION
            * p.blocks_needed() as f64
            * p.encryption_toffoli() as f64
    }

    /// Machines needed to finish inside a depth bound.
    ///
    /// With total serial depth `D_total` and a bound `D_max`, the required parallelism is
    /// `(D_total / D_max)^2` because parallel Grover buys only `sqrt`.
    pub fn depth_limited(p: &AesParams, max_depth: f64) -> DepthLimitedCost {
        let serial_depth = Self::total_toffoli(p);
        let ratio = (serial_depth / max_depth).max(1.0);
        DepthLimitedCost {
            max_depth,
            machines_required: ratio * ratio,
        }
    }

    /// Effective classical security in bits, i.e. `log2` of the attack's gate count.
    pub fn effective_security_bits(p: &AesParams) -> f64 {
        Self::total_toffoli(p).log2()
    }
}

impl AttackCircuit for GroverSymmetric {
    fn id(&self) -> &'static str {
        "grover_symmetric_aes"
    }

    fn display_name(&self) -> &'static str {
        "Grover key search (AES)"
    }

    fn synthesise(
        &self,
        target: &CryptoTarget,
        _opts: &SynthesisOptions,
    ) -> Result<SynthesisResult, SynthesisError> {
        let k = match target {
            CryptoTarget::Symmetric { key_bits } => *key_bits,
            other => {
                return Err(SynthesisError::WrongTarget {
                    pipeline: self.id(),
                    target: other.id(),
                })
            }
        };
        self.validity().check(self.id(), k)?;
        let p = AesParams::for_key_bits(k).ok_or(SynthesisError::OutOfRegime {
            pipeline: self.id(),
            min: 128,
            max: 256,
            got: k,
        })?;

        // Assemble one oracle evaluation. The Grover iteration count is astronomically
        // large and is carried as a repetition factor rather than materialised.
        let mut b = CircuitBuilder::new(format!("aes{k}_oracle"));
        b.repeat("cipher_block", p.blocks_needed(), |b| {
            b.repeat("round", p.rounds as u64, |b| {
                b.repeat("sbox", p.sboxes_per_round as u64, |b| {
                    b.repeat("sbox_and_gates", AES_SBOX_AND_GATES, |b| {
                        b.and_compute();
                    });
                });
                // MixColumns and AddRoundKey are linear, hence Clifford.
                b.repeat("linear_layer", 128, |b| {
                    b.clifford();
                });
            });
            b.repeat("key_schedule_sbox", p.key_schedule_sboxes as u64, |b| {
                b.repeat("sbox_and_gates", AES_SBOX_AND_GATES, |b| {
                    b.and_compute();
                });
            });
        });

        // State, key, and ancilla for the reversible S-box network.
        let data = 128 + k as u64 + 128;
        let circuit = CircuitIR::from_node(
            format!("aes{k}_oracle"),
            b.finish(),
            LogicalQubits::new(data),
        )?;

        Ok(SynthesisResult {
            circuit,
            repetitions: RepetitionModel::independent(
                1.0,
                Self::iterations(k) * ORACLE_CALLS_PER_ITERATION,
                "Grover iterations, each invoking the oracle to compute and uncompute",
            ),
            pipeline: self.id(),
        })
    }

    fn validity(&self) -> ValidityRegime {
        ValidityRegime {
            min_bits: 128,
            max_bits: 256,
        }
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "Boyar-Peralta 2011",
                title: "A small depth-16 circuit for the AES S-box",
                year: 2011,
            },
            Citation {
                reference: "arXiv:1910.01700",
                title: "Implementing Grover oracles for quantum key search on AES and LowMC",
                year: 2020,
            },
            Citation {
                reference: "NIST PQC call for proposals",
                title: "Security strength categories and the MAXDEPTH constraint",
                year: 2016,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_round_counts_match_fips_197() {
        assert_eq!(AesParams::for_key_bits(128).unwrap().rounds, 10);
        assert_eq!(AesParams::for_key_bits(192).unwrap().rounds, 12);
        assert_eq!(AesParams::for_key_bits(256).unwrap().rounds, 14);
        assert!(AesParams::for_key_bits(200).is_none());
    }

    #[test]
    fn encryption_cost_composes_from_sbox_multiplicative_complexity() {
        let p = AesParams::for_key_bits(128).unwrap();
        // 10 rounds x 16 S-boxes + 40 key schedule = 200 S-boxes, 32 AND each.
        assert_eq!(p.total_sboxes(), 200);
        assert_eq!(p.encryption_toffoli(), 6400);
    }

    #[test]
    fn assembled_oracle_matches_the_analytic_cost() {
        let r = GroverSymmetric
            .synthesise(&CryptoTarget::Symmetric { key_bits: 128 }, &SynthesisOptions::default())
            .unwrap();
        let p = AesParams::for_key_bits(128).unwrap();
        assert_eq!(
            r.circuit.resources.total_toffoli().get() as u64,
            p.encryption_toffoli() * p.blocks_needed()
        );
    }

    #[test]
    fn grover_gives_only_a_square_root_speedup() {
        // AES-128 should land far above 2^64, i.e. Grover does not halve the security
        // level in any operational sense once the oracle cost is charged.
        let p = AesParams::for_key_bits(128).unwrap();
        let bits = GroverSymmetric::effective_security_bits(&p);
        assert!(
            bits > 75.0 && bits < 90.0,
            "AES-128 Grover should cost ~2^78 Toffoli, got 2^{bits:.1}"
        );
    }

    #[test]
    fn aes256_is_hopeless_for_the_attacker() {
        let p = AesParams::for_key_bits(256).unwrap();
        let bits = GroverSymmetric::effective_security_bits(&p);
        assert!(bits > 140.0, "AES-256 should remain far out of reach, got 2^{bits:.1}");
    }

    #[test]
    fn depth_limits_cannot_be_bought_off_with_hardware() {
        // Halving the allowed depth must quadruple the machines required, because
        // parallel Grover buys only sqrt(S).
        let p = AesParams::for_key_bits(128).unwrap();
        let a = GroverSymmetric::depth_limited(&p, 2f64.powi(64));
        let b = GroverSymmetric::depth_limited(&p, 2f64.powi(63));
        let ratio = b.machines_required / a.machines_required;
        assert!((ratio - 4.0).abs() < 0.01, "expected 4x, got {ratio:.4}");
    }

    #[test]
    fn symmetric_is_far_less_urgent_than_asymmetric() {
        // The headline triage fact: breaking RSA-2048 costs ~2^31 Toffoli, breaking
        // AES-128 costs ~2^78. That gap of ~2^47, about fourteen decimal orders of
        // magnitude, is the difference between "migrate now" and "not a priority".
        use crate::shor_factoring::ShorFactoringGE19;
        let rsa = ShorFactoringGE19
            .synthesise(&CryptoTarget::Rsa { modulus_bits: 2048 }, &SynthesisOptions::default())
            .unwrap()
            .circuit
            .resources
            .total_toffoli()
            .get() as f64;
        let aes = GroverSymmetric::total_toffoli(&AesParams::for_key_bits(128).unwrap());
        let ratio_bits = (aes / rsa).log2();
        assert!(
            ratio_bits > 40.0,
            "AES-128 should be ~2^47 times harder than RSA-2048, got 2^{ratio_bits:.1}"
        );
    }

    #[test]
    fn wrong_target_is_rejected() {
        let e = GroverSymmetric
            .synthesise(&CryptoTarget::Rsa { modulus_bits: 2048 }, &SynthesisOptions::default())
            .unwrap_err();
        assert!(matches!(e, SynthesisError::WrongTarget { .. }));
    }
}
