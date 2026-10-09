//! Shor's algorithm for the elliptic-curve discrete logarithm problem.
//!
//! Follows Roetteler, Naehrig, Svore & Lauter (arXiv:1706.06752, ASIACRYPT 2017), which
//! reports for an `n`-bit curve:
//!
//! - `9n + 2 ceil(log2 n) + 10` logical qubits
//! - `448 n^3 log2(n) + 4090 n^3` Toffoli gates, from `2n` controlled point additions
//!   each costing `224 n^2 log2(n) + 2045 n^2`
//!
//! PRAMANA derives the leading terms by composition (see [`crate::arith::ec_point`]);
//! the sub-leading regression terms are not derivable from the published subroutine
//! table and are reported as an open divergence.

use crate::arith::ec_point;
use crate::ir::{CircuitBuilder, CircuitIR};
use crate::pipeline::{AttackCircuit, Citation, SynthesisResult};
use crate::target::{
    CryptoTarget, RepetitionModel, SynthesisError, SynthesisOptions, ValidityRegime,
};
use pramana_units::LogicalQubits;

/// The Roetteler-Naehrig-Svore-Lauter ECDLP construction.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShorEcdlpRNSL;

/// Controlled point additions in the full algorithm, as a multiple of `n`.
///
/// The source: "one simply multiplies by 2n, since the controlled point addition is
/// iterated 2n times".
pub const POINT_ADDITIONS_PER_BIT: u64 = 2;

impl ShorEcdlpRNSL {
}

impl AttackCircuit for ShorEcdlpRNSL {
    fn id(&self) -> &'static str {
        "shor_ecdlp_rnsl"
    }

    fn display_name(&self) -> &'static str {
        "Shor ECDLP (Roetteler-Naehrig-Svore-Lauter 2017)"
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

        let mut b = CircuitBuilder::new(format!("ecdlp_{n}"));
        b.repeat(
            "controlled_point_addition",
            POINT_ADDITIONS_PER_BIT * n as u64,
            |b| {
                ec_point::point_addition(b, n);
            },
        );
        let circuit = CircuitIR::from_node(
            format!("ecdlp_{n}"),
            b.finish(),
            LogicalQubits::new(ec_point::point_addition_data_qubits(n)),
        )?;

        Ok(SynthesisResult {
            circuit,
            repetitions: RepetitionModel::independent(
                0.9,
                1.2,
                "Shor's ECDLP succeeds with high probability per run; a small number of \
                 repetitions covers order-finding failures",
            ),
            pipeline: self.id(),
        })
    }

    fn validity(&self) -> ValidityRegime {
        ValidityRegime {
            min_bits: 64,
            max_bits: 1024,
        }
    }

    fn citations(&self) -> &'static [Citation] {
        &[Citation {
            reference: "arXiv:1706.06752",
            title: "Quantum resource estimates for computing elliptic curve discrete logarithms",
            year: 2017,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Roetteler et al.'s closed forms, local to the tests (Law 1).
    fn published_logical_qubits(n: u32) -> f64 {
        9.0 * n as f64 + 2.0 * ec_point::ceil_log2(n) as f64 + 10.0
    }

    fn published_toffoli(n: u32) -> f64 {
        let nf = n as f64;
        448.0 * nf.powi(3) * nf.log2() + 4090.0 * nf.powi(3)
    }

    const PUBLISHED_LEADING_COEFF: f64 = 448.0;

    fn synth(n: u32) -> SynthesisResult {
        ShorEcdlpRNSL
            .synthesise(
                &CryptoTarget::Ecdlp { curve_bits: n },
                &SynthesisOptions::default(),
            )
            .expect("synthesis succeeds")
    }

    #[test]
    fn reproduces_published_logical_qubit_count_exactly() {
        for n in [160u32, 192, 224, 256, 384, 521] {
            let got = synth(n).circuit.logical_qubits.get() as f64;
            let want = published_logical_qubits(n);
            assert_eq!(got, want, "n={n}");
        }
    }

    #[test]
    fn leading_coefficient_emerges_as_448() {
        // 448 = 2 * 224, and 224 itself emerges from 4 inversions + 2 squarings +
        // 4 multiplications. Neither number is written anywhere in PRAMANA.
        for n in [128u32, 256, 512] {
            let c = synth(n).circuit.resources.total_toffoli().get() as f64;
            let nf = n as f64;
            let coeff = c / (nf.powi(3) * nf.log2());
            assert!(
                (coeff - PUBLISHED_LEADING_COEFF).abs() < 1.0,
                "n={n}: expected 448, got {coeff:.3}"
            );
        }
    }

    #[test]
    fn p256_is_the_expected_order_of_magnitude() {
        let c = synth(256).circuit.resources.total_toffoli().get() as f64;
        assert!(
            c > 1e10 && c < 1e11,
            "P-256 leading-order cost should be ~6e10, got {c:.4e}"
        );
    }

    #[test]
    fn sub_leading_term_gap_is_the_known_divergence() {
        // PRAMANA derives 448 n^3 log2 n but not the empirical +4090 n^3. At P-256 the
        // omitted term is comparable to the leading one, so the ratio sits near 0.5.
        // Pinned as a regression guard on a *documented* gap, not an accepted result.
        let got = synth(256).circuit.resources.total_toffoli().get() as f64;
        let published = published_toffoli(256);
        let ratio = got / published;
        assert!(
            ratio > 0.4 && ratio < 0.6,
            "expected the documented ~0.47 ratio, got {ratio:.3}"
        );
    }

    #[test]
    fn wrong_target_is_rejected() {
        let e = ShorEcdlpRNSL
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: 2048 },
                &SynthesisOptions::default(),
            )
            .unwrap_err();
        assert!(matches!(e, SynthesisError::WrongTarget { .. }));
    }

    #[test]
    fn ecdlp_is_far_cheaper_than_rsa_at_equivalent_classical_security() {
        // P-256 and RSA-3072 are both nominally 128-bit classical security, yet the
        // elliptic curve falls to a much smaller quantum attack. This asymmetry is the
        // single most important triage fact PRAMANA exists to surface.
        use crate::shor_factoring::ShorFactoringGE19;
        let ec = synth(256).circuit.logical_qubits.get();
        let rsa = ShorFactoringGE19
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: 3072 },
                &SynthesisOptions::default(),
            )
            .unwrap()
            .circuit
            .logical_qubits
            .get();
        assert!(
            ec * 3 < rsa,
            "P-256 should need far fewer logical qubits than RSA-3072: {ec} vs {rsa}"
        );
    }
}
