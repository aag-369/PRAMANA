//! Shor's factoring algorithm: the Gidney-Ekera 2019 construction.
//!
//! # Published anchors
//!
//! Gidney & Ekera, *How to factor 2048 bit RSA integers in 8 hours using 20 million
//! noisy qubits* (arXiv:1905.09749, Quantum 5:433) report, in the abstract circuit
//! model that excludes distillation, routing and error correction:
//!
//! - `3n + 0.002 n lg n` logical qubits
//! - `0.3 n^3 + 0.0005 n^3 lg n` Toffoli gates
//! - `500 n^2 + n^2 lg n` measurement depth
//!
//! For `n = 2048` that is roughly 6200 logical qubits and 3 billion Toffoli gates.
//!
//! These figures are reproduction *targets*. They are stated here in documentation and
//! in `verification/golden/`, and are never consulted by the estimation path (Law 1).

use crate::arith::adder::{self, AdderKind};
use crate::arith::modexp::{emit_modexp, optimise_windows, ModExpParams};
use crate::ir::{CircuitBuilder, CircuitIR};
use crate::pipeline::{AttackCircuit, Citation, SynthesisResult};
use crate::target::{
    CryptoTarget, RepetitionModel, SynthesisError, SynthesisOptions, ValidityRegime,
};
use pramana_units::LogicalQubits;

/// The Gidney-Ekera 2019 factoring construction.
///
/// Windowed modular exponentiation in Zalka's coset representation, with Gidney
/// temporary-AND adders and a semiclassical (Griffiths-Niu) exponent readout.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShorFactoringGE19;

/// Exponent length used by the Ekera-Hastad variant, as a multiple of `n`.
///
/// Confirmed against the source: Gidney-Ekera 2019 section 2.2 states "The total
/// exponent length is hence ne = 3m = 1.5n + O(1)". Textbook Shor would use `2n`.
/// The shorter exponent costs more classical post-processing and a success probability
/// below one, both modelled in [`ShorFactoringGE19`]'s repetition model rather than
/// silently dropped.
pub const EKERA_HASTAD_EXPONENT_MULTIPLE: f64 = 1.5;

/// Expected number of runs for the Ekera-Hastad variant.
///
/// The short-exponent variant does not succeed with certainty from a single run; the
/// lattice-based post-processing requires a small number of independent runs.
pub const EKERA_HASTAD_EXPECTED_RUNS: f64 = 3.0;

impl ShorFactoringGE19 {
    /// Build the modular-exponentiation parameters for an `n`-bit modulus.
    pub fn params(&self, modulus_bits: u32, opts: &SynthesisOptions) -> ModExpParams {
        let exponent_bits =
            (modulus_bits as f64 * EKERA_HASTAD_EXPONENT_MULTIPLE).ceil() as u32;
        let base = ModExpParams {
            modulus_bits,
            exponent_bits,
            exponent_window: 5,
            multiplication_window: 5,
            coset_padding: adder::coset_padding(modulus_bits, opts.coset_slack),
            // GE19 section 2.5 charges Cuccaro's adder at 2n Toffoli per n-bit addition.
            // Reproducing the paper means charging what the paper charged.
            adder: AdderKind::CuccaroAsChargedByGe19,
        };
        optimise_windows(&base, opts.max_window)
    }

}

impl AttackCircuit for ShorFactoringGE19 {
    fn id(&self) -> &'static str {
        "shor_factoring_ge19"
    }

    fn display_name(&self) -> &'static str {
        "Shor factoring (Gidney-Ekera 2019)"
    }

    fn synthesise(
        &self,
        target: &CryptoTarget,
        opts: &SynthesisOptions,
    ) -> Result<SynthesisResult, SynthesisError> {
        let n = match target {
            CryptoTarget::Rsa { modulus_bits } => *modulus_bits,
            // The same construction costs finite-field DLP, per the source paper.
            CryptoTarget::FiniteFieldDlp { prime_bits } => *prime_bits,
            other => {
                return Err(SynthesisError::WrongTarget {
                    pipeline: self.id(),
                    target: other.id(),
                })
            }
        };
        self.validity().check(self.id(), n)?;

        let p = self.params(n, opts);
        let mut b = CircuitBuilder::new(format!("shor_ge19_{n}"));
        emit_modexp(&mut b, &p);
        let circuit = CircuitIR::from_node(
            format!("shor_ge19_{n}"),
            b.finish(),
            LogicalQubits::new(p.data_qubits()),
        )?;

        Ok(SynthesisResult {
            circuit,
            repetitions: RepetitionModel::independent(
                1.0 / EKERA_HASTAD_EXPECTED_RUNS,
                EKERA_HASTAD_EXPECTED_RUNS,
                "Ekera-Hastad short-exponent post-processing requires several independent runs",
            ),
            pipeline: self.id(),
        })
    }

    fn validity(&self) -> ValidityRegime {
        ValidityRegime {
            min_bits: 128,
            max_bits: 16384,
        }
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "arXiv:1905.09749",
                title: "How to factor 2048 bit RSA integers in 8 hours using 20 million noisy qubits",
                year: 2019,
            },
            Citation {
                reference: "arXiv:1905.07682",
                title: "Windowed quantum arithmetic",
                year: 2019,
            },
            Citation {
                reference: "quant-ph/0601097",
                title: "Shor's algorithm with fewer (pure) qubits (coset representation)",
                year: 2006,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gidney-Ekera 2019's closed forms, local to the tests.
    ///
    /// Law 1 keeps published results out of production code. A test asserting that this
    /// construction reproduces its source is exactly where such a figure belongs, and a
    /// `#[cfg(test)]` block cannot be reached at runtime.
    fn published_logical_qubits(n: u32) -> f64 {
        let nf = n as f64;
        3.0 * nf + 0.002 * nf * nf.log2()
    }

    fn published_toffoli(n: u32) -> f64 {
        let nf = n as f64;
        0.3 * nf.powi(3) + 0.0005 * nf.powi(3) * nf.log2()
    }

    fn synth(n: u32) -> SynthesisResult {
        ShorFactoringGE19
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: n },
                &SynthesisOptions::default(),
            )
            .expect("synthesis succeeds")
    }

    #[test]
    fn reproduces_published_logical_qubit_count_for_rsa2048() {
        // GE19 abstract circuit model: 3n + 0.002 n lg n = 6189 for n = 2048.
        let r = synth(2048);
        let got = r.circuit.logical_qubits.get() as f64;
        let want = published_logical_qubits(2048);
        let rel = (got - want).abs() / want;
        assert!(
            rel < 0.05,
            "logical qubits: got {got}, published {want:.0}, rel error {rel:.4}"
        );
    }

    #[test]
    fn logical_qubit_count_tracks_three_n_across_sizes() {
        for n in [1024u32, 2048, 3072, 4096] {
            let got = synth(n).circuit.logical_qubits.get() as f64;
            let want = published_logical_qubits(n);
            let rel = (got - want).abs() / want;
            assert!(rel < 0.06, "n={n}: got {got}, published {want:.0}, rel {rel:.4}");
        }
    }

    #[test]
    fn reproduces_published_toffoli_count_for_rsa2048() {
        // Reproduction, not calibration. Every structural element behind this number is
        // traced to an explicit statement in GE19 section 2.5: 2n additions per
        // multiplication, 2n/cmul of them after windowing, and Cuccaro's adder charged
        // at 2n Toffoli per n-bit addition. No constant was fitted.
        let r = synth(2048);
        let got = r.circuit.resources.total_toffoli().get() as f64;
        let want = published_toffoli(2048);
        let rel = (got - want).abs() / want;
        assert!(
            rel < 0.05,
            "Toffoli: got {got:.4e}, published {want:.4e}, rel error {rel:.4}"
        );
    }

    #[test]
    fn adder_choice_is_visible_and_material() {
        // The adder is a modelling choice worth ~2x. PRAMANA must not hide it: swapping
        // GE19's Cuccaro charge for the modern temporary-AND adder should roughly halve
        // the count, which is exactly the kind of thing a resource estimate should be
        // able to attribute.
        use crate::arith::adder::AdderKind;
        use crate::arith::modexp::emit_modexp;
        use crate::ir::CircuitBuilder;

        let base = ShorFactoringGE19.params(2048, &SynthesisOptions::default());
        let mut cheap = base;
        cheap.adder = AdderKind::GidneyTemporaryAnd;

        let count = |p: &crate::arith::modexp::ModExpParams| {
            let mut b = CircuitBuilder::new("t");
            emit_modexp(&mut b, p);
            b.finish().fold().unwrap().resources.total_toffoli().get() as f64
        };
        let ratio = count(&base) / count(&cheap);
        assert!(
            ratio > 1.5 && ratio < 2.5,
            "adder choice should be worth ~2x, got {ratio:.3}"
        );
    }

    #[test]
    fn repetitions_are_modelled_not_dropped() {
        let r = synth(2048);
        assert!(r.repetitions.expected_runs > 1.0);
        assert!(
            r.total_toffoli_all_runs() > r.circuit.resources.total_toffoli().get() as f64,
            "total cost across runs must exceed single-run cost"
        );
    }

    #[test]
    fn out_of_regime_is_a_typed_error_not_a_guess() {
        let e = ShorFactoringGE19
            .synthesise(
                &CryptoTarget::Rsa { modulus_bits: 32 },
                &SynthesisOptions::default(),
            )
            .unwrap_err();
        assert!(matches!(e, SynthesisError::OutOfRegime { .. }));
    }

    #[test]
    fn wrong_target_is_rejected_rather_than_coerced() {
        let e = ShorFactoringGE19
            .synthesise(
                &CryptoTarget::Symmetric { key_bits: 256 },
                &SynthesisOptions::default(),
            )
            .unwrap_err();
        assert!(matches!(e, SynthesisError::WrongTarget { .. }));
    }

    #[test]
    fn finite_field_dlp_is_costed_by_the_same_construction() {
        let r = ShorFactoringGE19
            .synthesise(
                &CryptoTarget::FiniteFieldDlp { prime_bits: 2048 },
                &SynthesisOptions::default(),
            )
            .expect("ffdlp is in scope for GE19");
        assert!(r.circuit.resources.total_toffoli().get() > 0);
    }

    #[test]
    fn citations_are_attached() {
        assert!(!ShorFactoringGE19.citations().is_empty());
    }
}
