//! Shor factoring via approximate residue arithmetic (Gidney 2025, arXiv:2505.15917).
//!
//! Where the 2019 construction holds `3n` logical qubits and spends 2.6e9 Toffoli gates,
//! this one holds roughly `0.5n` and spends about 6.5e9 across all expected shots. It is
//! the space-time tradeoff running in the opposite direction, and it is what makes a
//! sub-million-qubit machine plausible.

use crate::arith::residue::{
    analytic_toffoli_per_shot, emit_residue_modexp, ResidueParams,
};
use crate::ir::{CircuitBuilder, CircuitIR};
use crate::pipeline::{AttackCircuit, Citation, SynthesisResult};
use crate::target::{
    CryptoTarget, RepetitionModel, SynthesisError, SynthesisOptions, ValidityRegime,
};
use pramana_units::LogicalQubits;

/// The Gidney 2025 residue-arithmetic factoring construction.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShorFactoringG25;

impl ShorFactoringG25 {
    /// Parameters for a modulus size, from Table 5 of the source.
    pub fn params(&self, modulus_bits: u32) -> Option<ResidueParams> {
        ResidueParams::published(modulus_bits)
    }

}

impl AttackCircuit for ShorFactoringG25 {
    fn id(&self) -> &'static str {
        "shor_factoring_g25"
    }

    fn display_name(&self) -> &'static str {
        "Shor factoring, residue arithmetic (Gidney 2025)"
    }

    fn synthesise(
        &self,
        target: &CryptoTarget,
        _opts: &SynthesisOptions,
    ) -> Result<SynthesisResult, SynthesisError> {
        let n = match target {
            CryptoTarget::Rsa { modulus_bits } => *modulus_bits,
            other => {
                return Err(SynthesisError::WrongTarget {
                    pipeline: self.id(),
                    target: other.id(),
                })
            }
        };
        self.validity().check(self.id(), n)?;

        // The source publishes a grid-scanned parameter set per modulus size. Sizes
        // outside that set are refused rather than interpolated (Law 9).
        let p = self.params(n).ok_or(SynthesisError::OutOfRegime {
            pipeline: self.id(),
            min: 1024,
            max: 8192,
            got: n,
        })?;

        let mut b = CircuitBuilder::new(format!("shor_g25_{n}"));
        emit_residue_modexp(&mut b, &p);
        let circuit = CircuitIR::from_node(
            format!("shor_g25_{n}"),
            b.finish(),
            LogicalQubits::new(p.peak_logical_qubits()),
        )?;

        Ok(SynthesisResult {
            circuit,
            repetitions: RepetitionModel::independent(
                1.0 / p.expected_shots(),
                p.expected_shots(),
                "Ekera-Hastad shots, inflated by the approximate-arithmetic deviation rate \
                 and the classical post-processing failure chance",
            ),
            pipeline: self.id(),
        })
    }

    fn validity(&self) -> ValidityRegime {
        ValidityRegime {
            min_bits: 1024,
            max_bits: 8192,
        }
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "arXiv:2505.15917",
                title: "How to factor 2048 bit RSA integers with less than a million noisy qubits",
                year: 2025,
            },
            Citation {
                reference: "ePrint 2024/1852",
                title: "Reducing the number of qubits in quantum factoring",
                year: 2024,
            },
        ]
    }
}

/// Toffoli gates across all expected shots, which is what Table 5 reports.
pub fn toffoli_per_factoring(p: &ResidueParams) -> f64 {
    analytic_toffoli_per_shot(p) as f64 * p.expected_shots()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Table 5 of arXiv:2505.15917, local to the tests (Law 1).
    fn published_toffoli(modulus_bits: u32) -> Option<f64> {
        Some(match modulus_bits {
            1024 => 1.1e9,
            1536 => 3.1e9,
            2048 => 6.5e9,
            3072 => 1.9e10,
            4096 => 4.0e10,
            6144 => 1.2e11,
            8192 => 2.7e11,
            _ => return None,
        })
    }

    fn published_logical_qubits(modulus_bits: u32) -> Option<f64> {
        Some(match modulus_bits {
            1024 => 742.0,
            1536 => 1074.0,
            2048 => 1399.0,
            3072 => 2043.0,
            4096 => 2692.0,
            6144 => 3978.0,
            8192 => 5261.0,
            _ => return None,
        })
    }

    fn synth(n: u32) -> SynthesisResult {
        ShorFactoringG25
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: n },
                &SynthesisOptions::default(),
            )
            .expect("synthesis succeeds")
    }

    #[test]
    fn reproduces_published_logical_qubits() {
        for n in [1024u32, 2048, 4096, 8192] {
            let got = synth(n).circuit.logical_qubits.get() as f64;
            let want = published_logical_qubits(n).unwrap();
            let rel = (got - want).abs() / want;
            assert!(rel < 0.10, "n={n}: got {got}, published {want}, rel {rel:.3}");
        }
    }

    #[test]
    fn reproduces_published_toffoli_per_factoring() {
        for n in [1024u32, 2048, 4096, 8192] {
            let p = ShorFactoringG25.params(n).unwrap();
            let got = toffoli_per_factoring(&p);
            let want = published_toffoli(n).unwrap();
            let ratio = got / want;
            assert!(
                ratio > 0.5 && ratio < 2.0,
                "n={n}: got {got:.4e}, published {want:.2e}, ratio {ratio:.3}"
            );
        }
    }

    #[test]
    fn trades_space_against_gates_relative_to_ge19() {
        use crate::shor_factoring::ShorFactoringGE19;
        let g25 = synth(2048);
        let ge19 = ShorFactoringGE19
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: 2048 },
                &SynthesisOptions::default(),
            )
            .unwrap();
        assert!(
            g25.circuit.logical_qubits.get() * 4 < ge19.circuit.logical_qubits.get(),
            "G25 should use roughly a quarter the logical qubits"
        );
        assert!(
            g25.total_toffoli_all_runs() > ge19.total_toffoli_all_runs(),
            "and pay for it in gates"
        );
    }

    #[test]
    fn shots_are_modelled_not_dropped() {
        let r = synth(2048);
        assert!(r.repetitions.expected_runs > 9.0 && r.repetitions.expected_runs < 9.5);
    }

    #[test]
    fn unpublished_sizes_are_refused_rather_than_interpolated() {
        let e = ShorFactoringG25
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: 2560 },
                &SynthesisOptions::default(),
            )
            .unwrap_err();
        assert!(matches!(e, SynthesisError::OutOfRegime { .. }));
    }
}
