//! Qubit quantities. Logical and physical qubits are deliberately incommensurable:
//! the ratio between them is the entire output of a QEC cost model, and must never
//! be assumed implicitly.

use crate::count_newtype;

count_newtype!(
    /// A count of **logical** (error-corrected) qubits, as consumed by an algorithm.
    ///
    /// This is what circuit synthesis reports. It says nothing about hardware cost
    /// until a [`QecArchitecture`](https://docs.rs/pramana-qec) converts it.
    LogicalQubits, u64, "logical qubits"
);

count_newtype!(
    /// A count of **physical** qubits, as provided by hardware.
    ///
    /// Physical qubits from different modalities are not interchangeable (a cat qubit
    /// is not a transmon is not a neutral atom). Cross-architecture comparison must go
    /// through an explicit normalisation, never a bare comparison of this value.
    PhysicalQubits, u64, "physical qubits"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addition_is_within_unit() {
        assert_eq!((LogicalQubits::new(3) + LogicalQubits::new(4)).get(), 7);
        assert_eq!((PhysicalQubits::new(10) - PhysicalQubits::new(4)).get(), 6);
    }

    #[test]
    fn scaling_by_bare_scalar_works() {
        assert_eq!((LogicalQubits::new(6) * 7).get(), 42);
    }

    #[test]
    fn sum_over_iterator() {
        let total: LogicalQubits = (1..=4).map(LogicalQubits::new).sum();
        assert_eq!(total.get(), 10);
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn addition_overflow_panics_rather_than_wrapping() {
        let _ = LogicalQubits::new(u64::MAX) + LogicalQubits::new(1);
    }

    #[test]
    fn saturating_add_does_not_panic() {
        let v = LogicalQubits::new(u64::MAX).saturating_add(LogicalQubits::new(1));
        assert_eq!(v.get(), u64::MAX);
    }
}
