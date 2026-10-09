//! Spacetime volume.
//!
//! Qubit-rounds are PRAMANA's architecture-neutral currency. Qubit counts alone are
//! not comparable across modalities and wall-clock alone ignores hardware cost, so
//! every architecture reports volume as well.

use crate::count_newtype;
use crate::qubits::PhysicalQubits;
use crate::time::CodeCycles;

count_newtype!(
    /// Spacetime volume in physical-qubit-rounds.
    QubitRounds, u128, "qubit-rounds"
);

impl core::ops::Mul<CodeCycles> for PhysicalQubits {
    type Output = QubitRounds;

    /// Physical qubits held for a number of code cycles is a spacetime volume.
    ///
    /// This is a deliberate, meaningful dimensional composition, in contrast to the
    /// additions between unlike units that the type system forbids.
    #[inline]
    fn mul(self, cycles: CodeCycles) -> QubitRounds {
        QubitRounds::new((self.get() as u128).saturating_mul(cycles.get() as u128))
    }
}

impl core::ops::Mul<PhysicalQubits> for CodeCycles {
    type Output = QubitRounds;
    #[inline]
    fn mul(self, qubits: PhysicalQubits) -> QubitRounds {
        qubits * self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_composes_from_qubits_and_cycles() {
        let v = PhysicalQubits::new(1_000_000) * CodeCycles::new(600_000_000);
        assert_eq!(v.get(), 600_000_000_000_000u128);
    }

    #[test]
    fn multiplication_commutes() {
        let q = PhysicalQubits::new(317);
        let c = CodeCycles::new(991);
        assert_eq!((q * c).get(), (c * q).get());
    }
}
