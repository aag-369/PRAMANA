//! # pramana-hardware
//!
//! Hardware capability trajectories fitted from public roadmaps and, crucially, from
//! **independently verified logical-qubit demonstrations** rather than announced targets or
//! physical qubit counts.
//!
//! This is one half of the two-curve model PRAMANA computes. The other half, the falling
//! algorithmic requirement, comes from `pramana-circuit` and `pramana-qec`. A
//! cryptographically relevant quantum computer exists at the moment the two curves cross,
//! and the break-year distribution is the distribution of that crossing.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod fit;
pub mod roadmap;
pub mod trajectory;

pub use fit::{
    fit_capability, fit_capability_blended, fit_slip, ratio_summary, CapabilityFit, SlipPrior,
};
pub use roadmap::{Milestone, MilestoneStatus, Modality, Roadmaps};
pub use trajectory::{Trajectory, TrajectoryWarning};
