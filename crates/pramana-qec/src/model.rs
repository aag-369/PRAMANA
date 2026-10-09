//! The pluggable architecture interface.
//!
//! Spec Law 4: adding an architecture must require implementing one trait and
//! registering it, with no special-casing anywhere in the pipeline. Everything an
//! architecture needs to know arrives in [`QecInput`] and [`HardwareParams`]; everything
//! it must report leaves in [`QecEstimate`].

use pramana_units::{Distance, ErrorRate, LogicalQubits, PhysicalQubits, QubitRounds, Seconds};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The logical-layer demand placed on an architecture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QecInput {
    /// Peak simultaneous logical qubits.
    pub logical_qubits: LogicalQubits,
    /// Toffoli-equivalent non-Clifford operations.
    pub toffoli_count: u128,
    /// Sequential layers requiring classical feedback.
    pub reaction_depth: u64,
    /// Total tolerable failure probability for the whole run.
    pub target_total_error: ErrorRate,
    /// Fraction of logical qubits idle at any given moment.
    ///
    /// Windowed modular exponentiation touches a narrow working set while the bulk of
    /// its registers sit idle, and idle qubits can be stored far more cheaply than
    /// active ones. Architectures that exploit this (yoked surface codes) need to know
    /// the fraction; architectures that do not may ignore it.
    pub idle_fraction: f64,
}

/// Physical connectivity available on the hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Connectivity {
    /// Nearest-neighbour on a square grid, as assumed by superconducting estimates.
    SquareGrid,
    /// Fixed-degree graph with long-range couplers, as qLDPC codes require.
    Degree(u32),
    /// Physically reconfigurable, as in neutral-atom arrays.
    Reconfigurable,
    /// All-to-all, as approximately available in small trapped-ion registers.
    AllToAll,
}

/// Physical hardware assumptions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HardwareParams {
    /// Uniform physical gate and measurement error rate.
    pub physical_error_rate: ErrorRate,
    /// Duration of one syndrome-extraction round.
    pub cycle_time: Seconds,
    /// Classical control feedback latency.
    pub reaction_time: Seconds,
    /// Connectivity model.
    pub connectivity: Connectivity,
    /// Noise bias (phase to bit flip), for biased-noise architectures such as cat qubits.
    pub error_bias: Option<f64>,
    /// Mean photon number of a cat state, which sets the bit-flip suppression.
    pub mean_photon_number: Option<f64>,
    /// Ratio of single-photon to two-photon loss, the dominant cat-qubit noise figure.
    pub loss_ratio: Option<f64>,
    /// Time to physically relocate an atom, for reconfigurable neutral-atom arrays.
    ///
    /// Typically the dominant time cost in that modality, and orders of magnitude slower
    /// than a superconducting syndrome round.
    pub atom_move_time: Option<Seconds>,
}

impl HardwareParams {
    /// The assumptions shared by Gidney-Ekera 2019 and Gidney 2025: a square grid of
    /// qubits with nearest-neighbour connections, a uniform gate error rate of 0.1%, a
    /// surface code cycle time of 1 microsecond, and a reaction time of 10 microseconds.
    pub fn gidney_superconducting() -> Self {
        Self {
            physical_error_rate: ErrorRate::new_unchecked(1e-3),
            cycle_time: Seconds::from_micros(1.0),
            reaction_time: Seconds::from_micros(10.0),
            connectivity: Connectivity::SquareGrid,
            error_bias: None,
            mean_photon_number: None,
            loss_ratio: None,
            atom_move_time: None,
        }
    }

    /// A reconfigurable neutral-atom array: slower syndrome extraction than
    /// superconducting hardware, but physically movable qubits and therefore
    /// effectively free long-range connectivity.
    pub fn neutral_atom_array() -> Self {
        Self {
            physical_error_rate: ErrorRate::new_unchecked(1e-3),
            cycle_time: Seconds::from_micros(200.0),
            reaction_time: Seconds::from_micros(200.0),
            connectivity: Connectivity::Reconfigurable,
            error_bias: None,
            mean_photon_number: None,
            loss_ratio: None,
            atom_move_time: Some(Seconds::from_micros(200.0)),
        }
    }

    /// The assumptions of Gouzien et al. (arXiv:2302.06639) for a repetition-cat
    /// architecture: a single-to-two-photon loss ratio of 1e-5, a cycle time of 500 ns,
    /// and a mean photon number of 19.
    pub fn gouzien_cat() -> Self {
        Self {
            // Placeholder; the cat model derives its own phase-error rate from the
            // loss ratio and photon number below.
            physical_error_rate: ErrorRate::new_unchecked(1e-3),
            cycle_time: Seconds::from_nanos(500.0),
            // The source pipelines its computation rather than stalling on classical
            // feedback, so the reaction time is taken as one cycle.
            reaction_time: Seconds::from_nanos(500.0),
            connectivity: Connectivity::SquareGrid,
            error_bias: None,
            mean_photon_number: Some(19.0),
            loss_ratio: Some(1e-5),
            atom_move_time: None,
        }
    }
}

/// What binds the wall-clock estimate.
///
/// Not cosmetic: this drives the explanation of *why* an asset has the exposure it has,
/// and it is what makes the cross-architecture comparison interpretable rather than a
/// bare ranking of numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitingFactor {
    /// Classical feedback latency dominates. Adding hardware does not help.
    ReactionLimited,
    /// Magic state supply dominates. More factories would help.
    FactoryLimited,
    /// Moving data around the layout dominates.
    RoutingLimited,
    /// Idle storage error forces a larger code than computation does.
    MemoryLimited,
}

/// A breakdown of where the physical qubits went.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ResourceBreakdown {
    /// Qubits in data patches.
    pub data: u64,
    /// Qubits in routing and ancilla space.
    pub routing: u64,
    /// Qubits in magic state factories.
    pub factories: u64,
}

impl ResourceBreakdown {
    /// Total physical qubits.
    pub fn total(&self) -> u64 {
        self.data + self.routing + self.factories
    }
}

/// A report on the magic state supply.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FactoryReport {
    /// Name of the construction chosen.
    pub construction: &'static str,
    /// Number of factories instantiated in parallel.
    pub count: u64,
    /// Physical qubits per factory.
    pub qubits_each: u64,
    /// Code cycles to produce one state, per factory.
    pub cycles_per_state: u64,
    /// Output error rate per state.
    pub output_error: f64,
}

/// The architecture's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct QecEstimate {
    /// Total physical qubits.
    pub physical_qubits: PhysicalQubits,
    /// Wall-clock runtime.
    pub wall_clock: Seconds,
    /// Spacetime volume, the architecture-neutral currency for comparison.
    pub spacetime_volume: QubitRounds,
    /// Code distance chosen for computation.
    pub code_distance: Option<Distance>,
    /// Magic state supply.
    pub magic_state_factory: FactoryReport,
    /// Where the qubits went.
    pub breakdown: ResourceBreakdown,
    /// Logical error rate actually achieved per patch per round.
    pub logical_error_achieved: f64,
    /// What bound the runtime.
    pub limiting_factor: LimitingFactor,
    /// Warnings raised while estimating.
    pub warnings: Vec<String>,
}

/// Errors from architecture cost models.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum QecError {
    /// The physical error rate is at or above the code threshold, so no distance helps.
    #[error(
        "physical error rate {p:.2e} is at or above the {code} threshold {threshold:.2e}; \
         error correction cannot succeed at any distance"
    )]
    AboveThreshold {
        /// The physical error rate supplied.
        p: f64,
        /// The code's threshold.
        threshold: f64,
        /// Name of the code.
        code: &'static str,
    },

    /// No distance within the search bound met the error target.
    #[error("no code distance up to {max} achieves the target logical error rate")]
    DistanceSearchExhausted {
        /// Largest distance tried.
        max: u32,
    },

    /// The architecture is outside its validated regime (Law 9).
    #[error("architecture '{architecture}' is out of regime: {reason}")]
    OutOfRegime {
        /// Architecture identifier.
        architecture: &'static str,
        /// Why.
        reason: String,
    },
}

/// A literature citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// arXiv identifier or DOI.
    pub reference: &'static str,
    /// Short title.
    pub title: &'static str,
    /// Year.
    pub year: u32,
}

/// A fault-tolerant architecture cost model.
pub trait QecArchitecture: Send + Sync {
    /// Stable identifier.
    fn id(&self) -> &'static str;

    /// Human-readable name.
    fn display_name(&self) -> &'static str;

    /// Cost the given logical demand on the given hardware.
    fn estimate(&self, input: &QecInput, hw: &HardwareParams) -> Result<QecEstimate, QecError>;

    /// Sources for the construction.
    fn citations(&self) -> &'static [Citation];
}
