//! # pramana-qec
//!
//! Pluggable fault-tolerant architecture cost models.
//!
//! Each architecture takes a logical-layer demand ([`model::QecInput`]) and hardware
//! assumptions ([`model::HardwareParams`]) and returns physical qubits, wall clock, and
//! spacetime volume ([`model::QecEstimate`]). Adding an architecture means implementing
//! [`model::QecArchitecture`] and registering it; nothing else in the pipeline changes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cat;
pub mod gidney2025;
pub mod magic;
pub mod model;
pub mod neutral_atom;
pub mod qldpc;
pub mod surface;
pub mod yoked;

pub use model::{
    Citation, Connectivity, HardwareParams, LimitingFactor, QecArchitecture, QecEstimate, QecError,
    QecInput,
};
pub use surface::{Layout, SurfaceCode};
pub use cat::RepetitionCat;
pub use neutral_atom::NeutralAtomTransversal;
pub use qldpc::{BbCode, BivariateBicycle, ComputationScheme};
pub use gidney2025::{Gidney2025Layout, LogicalErrorModel};
pub use yoked::YokedSurfaceCode;
