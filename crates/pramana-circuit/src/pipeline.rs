//! The attack-circuit pipeline abstraction.
//!
//! Every attack is synthesised behind one trait, so that adding a construction requires
//! implementing it and registering it, with no special-casing elsewhere.

use crate::ir::CircuitIR;
use crate::target::{
    CryptoTarget, RepetitionModel, SynthesisError, SynthesisOptions, ValidityRegime,
};
use serde::{Deserialize, Serialize};

/// A literature citation attached to a construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// arXiv identifier or DOI.
    pub reference: &'static str,
    /// Short title.
    pub title: &'static str,
    /// Publication year.
    pub year: u32,
}

/// A synthesised attack, including how many times it must be run.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthesisResult {
    /// The measured circuit.
    pub circuit: CircuitIR,
    /// Expected repetitions.
    pub repetitions: RepetitionModel,
    /// Identifier of the pipeline that produced this.
    pub pipeline: &'static str,
}

impl SynthesisResult {
    /// Total Toffoli gates across all expected runs.
    ///
    /// This, not the single-run count, is what a QEC model should be handed when the
    /// question is "how long does the attack take".
    pub fn total_toffoli_all_runs(&self) -> f64 {
        self.circuit.resources.total_toffoli().get() as f64 * self.repetitions.expected_runs
    }
}

/// A construction that synthesises an attack circuit for a cryptographic target.
pub trait AttackCircuit: Send + Sync {
    /// Stable identifier.
    fn id(&self) -> &'static str;

    /// Human-readable name.
    fn display_name(&self) -> &'static str;

    /// Synthesise the circuit for `target`.
    fn synthesise(
        &self,
        target: &CryptoTarget,
        opts: &SynthesisOptions,
    ) -> Result<SynthesisResult, SynthesisError>;

    /// The size range this construction is validated for.
    fn validity(&self) -> ValidityRegime;

    /// Sources for the construction.
    fn citations(&self) -> &'static [Citation];
}
