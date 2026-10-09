//! # pramana-risk
//!
//! Break-year distributions, Mosca resolution and exposure scoring.
//!
//! This crate closes the causal chain the project exists to establish: an asset's key
//! parameters determine a circuit, the circuit determines a fault-tolerant cost, the cost
//! meets a hardware trajectory, and the crossing is the break year. No step consults a
//! fixed Q-Day.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod exposure;
pub mod improvement;
pub mod montecarlo;
pub mod mosca;

pub use exposure::{score, Criticality, DataClass, ExposureScore};
pub use improvement::{Floors, ImprovementModel, ProblemFamily};
pub use montecarlo::{
    break_year_distribution, ArchitectureOption, AttackProfile, AttackerBudget,
    BreakYearDistribution, MonteCarloConfig,
};
pub use mosca::{resolve, MoscaInputs, MoscaResolution, ThreatMode};
