//! # pramana-circuit
//!
//! Attack-circuit synthesis and resource measurement.
//!
//! PRAMANA never simulates a quantum state and never executes a circuit. It assembles
//! a structural representation from reversible-arithmetic primitives and measures the
//! result. See [`ir`] for the synthesis-versus-formula argument and the role of the
//! repetition node.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod arith;
pub mod grover_symmetric;
pub mod ir;
pub mod pipeline;
pub mod shor_ecdlp;
pub mod shor_ecdlp_2026;
pub mod shor_factoring;
pub mod shor_factoring_g25;
pub mod target;

pub use ir::{CircuitBuilder, CircuitError, CircuitIR, GateKind, Node, ResourceCount};
pub use pipeline::{AttackCircuit, Citation, SynthesisResult};
pub use grover_symmetric::GroverSymmetric;
pub use shor_ecdlp::ShorEcdlpRNSL;
pub use shor_ecdlp_2026::{Ecdlp2026Params, Optimisation, ShorEcdlp2026};
pub use shor_factoring::ShorFactoringGE19;
pub use shor_factoring_g25::ShorFactoringG25;
pub use target::{CryptoTarget, RepetitionModel, SynthesisError, SynthesisOptions};
