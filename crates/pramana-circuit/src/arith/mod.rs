//! Reversible arithmetic primitives.
//!
//! Every routine here assembles gates in loops whose trip counts follow from the
//! algorithm's structure. Closed-form cost formulas appear only in `#[cfg(test)]`
//! oracles, where they cross-check the assembled count (spec §7.5).

pub mod adder;
pub mod ec_point;
pub mod eea;
pub mod lookup;
pub mod modexp;
pub mod residue;
