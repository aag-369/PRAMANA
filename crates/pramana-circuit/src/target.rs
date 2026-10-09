//! The cryptographic target of an attack, and the repetition model for the attack.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What is being attacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CryptoTarget {
    /// An RSA modulus of the given bit length.
    Rsa {
        /// Bit length of the modulus `N`.
        modulus_bits: u32,
    },
    /// A discrete logarithm on an elliptic curve over a prime field.
    Ecdlp {
        /// Bit length of the field prime / curve order.
        curve_bits: u32,
    },
    /// A discrete logarithm in a finite field (classic Diffie-Hellman, DSA).
    FiniteFieldDlp {
        /// Bit length of the field prime.
        prime_bits: u32,
    },
    /// A symmetric primitive attacked by Grover search.
    Symmetric {
        /// Key length in bits.
        key_bits: u32,
    },
}

impl CryptoTarget {
    /// A short stable identifier, used in cache keys and provenance records.
    pub fn id(&self) -> String {
        match self {
            CryptoTarget::Rsa { modulus_bits } => format!("rsa-{modulus_bits}"),
            CryptoTarget::Ecdlp { curve_bits } => format!("ecdlp-{curve_bits}"),
            CryptoTarget::FiniteFieldDlp { prime_bits } => format!("ffdlp-{prime_bits}"),
            CryptoTarget::Symmetric { key_bits } => format!("sym-{key_bits}"),
        }
    }

    /// The security parameter in bits, for ordering targets by difficulty.
    pub fn size_bits(&self) -> u32 {
        match self {
            CryptoTarget::Rsa { modulus_bits } => *modulus_bits,
            CryptoTarget::Ecdlp { curve_bits } => *curve_bits,
            CryptoTarget::FiniteFieldDlp { prime_bits } => *prime_bits,
            CryptoTarget::Symmetric { key_bits } => *key_bits,
        }
    }
}

/// How many times the quantum circuit must be run to succeed.
///
/// Dropping this is a documented failure mode (spec §23.7): variants that reduce qubit
/// count often pay in success probability, and ignoring the resulting repetitions
/// understates the wall-clock cost of the attack.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RepetitionModel {
    /// Probability that a single run yields usable output.
    pub success_probability: f64,
    /// Expected number of runs, including classical post-processing requirements.
    pub expected_runs: f64,
    /// Human-readable justification, carried into the provenance chain.
    pub rationale: &'static str,
}

impl RepetitionModel {
    /// A single deterministic run.
    pub fn certain(rationale: &'static str) -> Self {
        Self {
            success_probability: 1.0,
            expected_runs: 1.0,
            rationale,
        }
    }

    /// `runs` independent runs each succeeding with probability `p`.
    pub fn independent(p: f64, runs: f64, rationale: &'static str) -> Self {
        Self {
            success_probability: p,
            expected_runs: runs,
            rationale,
        }
    }
}

/// Knobs controlling how a circuit is synthesised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SynthesisOptions {
    /// Upper bound on the window widths the optimiser will consider.
    pub max_window: u32,
    /// Extra coset padding beyond `ceil(log2 n)`.
    pub coset_slack: u32,
}

impl Default for SynthesisOptions {
    fn default() -> Self {
        Self {
            max_window: 12,
            coset_slack: 2,
        }
    }
}

/// Errors from circuit synthesis.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum SynthesisError {
    /// The pipeline does not apply to this target.
    #[error("pipeline '{pipeline}' does not handle target {target}")]
    WrongTarget {
        /// Pipeline identifier.
        pipeline: &'static str,
        /// Target identifier.
        target: String,
    },

    /// The target size falls outside the pipeline's validated range (Law 9).
    #[error(
        "pipeline '{pipeline}' is validated for {min}..={max} bits; {got} bits is out of regime"
    )]
    OutOfRegime {
        /// Pipeline identifier.
        pipeline: &'static str,
        /// Smallest validated size.
        min: u32,
        /// Largest validated size.
        max: u32,
        /// Requested size.
        got: u32,
    },

    /// The circuit could not be assembled.
    #[error("circuit assembly failed: {0}")]
    Assembly(#[from] crate::ir::CircuitError),
}

/// The validated regime of a synthesis pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidityRegime {
    /// Smallest target size the pipeline is validated for.
    pub min_bits: u32,
    /// Largest target size the pipeline is validated for.
    pub max_bits: u32,
}

impl ValidityRegime {
    /// Check a size, producing a typed error rather than a silent guess (Law 9).
    pub fn check(&self, pipeline: &'static str, got: u32) -> Result<(), SynthesisError> {
        if got < self.min_bits || got > self.max_bits {
            return Err(SynthesisError::OutOfRegime {
                pipeline,
                min: self.min_bits,
                max: self.max_bits,
                got,
            });
        }
        Ok(())
    }
}
