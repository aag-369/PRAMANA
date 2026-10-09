//! Gate counts.
//!
//! Under a surface-code-like architecture Clifford gates are effectively free (tracked
//! in the Pauli frame) while non-Clifford gates must be supplied by magic states. The
//! type system keeps Toffoli and T counts distinct because the conversion between them
//! depends on the decomposition strategy, which is a modelling choice and not a
//! constant.

use crate::count_newtype;

count_newtype!(
    /// A count of Toffoli (CCX) gates.
    ToffoliCount, u128, "Toffoli"
);

count_newtype!(
    /// A count of T gates.
    TCount, u128, "T"
);

count_newtype!(
    /// A count of Clifford gates. Tracked for completeness and for architectures where
    /// Cliffords are *not* free (e.g. where transversal gates dominate the time cost).
    CliffordCount, u128, "Clifford"
);

count_newtype!(
    /// A count of measurements. Relevant to the reaction limit and to post-selected
    /// constructions such as magic state cultivation.
    MeasurementCount, u128, "measurement"
);

/// How a Toffoli gate is decomposed into T gates.
///
/// This is a modelling decision with a factor-of-two consequence, so it is explicit
/// rather than a hidden constant. `TemporaryAnd` is the construction that makes
/// windowed arithmetic affordable and is the default in modern estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecompositionStrategy {
    /// Textbook decomposition: 7 T gates per Toffoli.
    Textbook,
    /// Standard catalysed decomposition: 4 T gates per Toffoli.
    Standard,
    /// Gidney's temporary-AND: 4 T to compute, 0 T to uncompute (measurement-based).
    ///
    /// Carries the fraction of Toffolis in the circuit that are part of a
    /// compute/uncompute pair and therefore benefit from free uncomputation.
    ///
    /// The fraction is stored in per-mille (0..=1000) rather than as an `f64` so that
    /// the strategy remains `Eq + Hash` and can be used directly in a memoisation key.
    /// Floating-point cache keys are a correctness hazard, so the quantisation is
    /// deliberate rather than incidental.
    TemporaryAnd {
        /// Per-mille (0..=1000) of Toffolis whose uncomputation is measurement-based.
        uncomputed_permille: u16,
    },
}

impl DecompositionStrategy {
    /// Build a [`DecompositionStrategy::TemporaryAnd`] from a fraction in `[0, 1]`.
    ///
    /// The fraction is clamped and quantised to per-mille.
    pub fn temporary_and(fraction: f64) -> Self {
        let f = if fraction.is_nan() { 0.0 } else { fraction.clamp(0.0, 1.0) };
        DecompositionStrategy::TemporaryAnd {
            uncomputed_permille: (f * 1000.0).round() as u16,
        }
    }
}

impl DecompositionStrategy {
    /// Convert a Toffoli count into a T count under this strategy.
    ///
    /// This is the *only* sanctioned path from [`ToffoliCount`] to [`TCount`].
    pub fn t_count(self, toffoli: ToffoliCount) -> TCount {
        let n = toffoli.get();
        match self {
            DecompositionStrategy::Textbook => TCount::new(n.saturating_mul(7)),
            DecompositionStrategy::Standard => TCount::new(n.saturating_mul(4)),
            DecompositionStrategy::TemporaryAnd {
                uncomputed_permille,
            } => {
                let f = (uncomputed_permille.min(1000) as f64) / 1000.0;
                // Paired Toffolis: one compute (4 T) + one free uncompute, i.e. 4 T per
                // pair rather than 8. Unpaired Toffolis cost the standard 4 T.
                let paired = (n as f64 * f) as u128;
                let unpaired = n.saturating_sub(paired);
                TCount::new(paired.saturating_mul(2).saturating_add(unpaired.saturating_mul(4)))
            }
        }
    }
}

impl Default for DecompositionStrategy {
    fn default() -> Self {
        DecompositionStrategy::TemporaryAnd {
            uncomputed_permille: 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textbook_is_seven_t_per_toffoli() {
        let t = DecompositionStrategy::Textbook.t_count(ToffoliCount::new(10));
        assert_eq!(t.get(), 70);
    }

    #[test]
    fn standard_is_four_t_per_toffoli() {
        let t = DecompositionStrategy::Standard.t_count(ToffoliCount::new(10));
        assert_eq!(t.get(), 40);
    }

    #[test]
    fn temporary_and_halves_fully_paired_circuits() {
        let s = DecompositionStrategy::temporary_and(1.0);
        assert_eq!(s.t_count(ToffoliCount::new(100)).get(), 200);
    }

    #[test]
    fn temporary_and_degrades_to_standard_when_nothing_is_paired() {
        let s = DecompositionStrategy::temporary_and(0.0);
        assert_eq!(s.t_count(ToffoliCount::new(100)).get(), 400);
    }

    #[test]
    fn strategy_is_usable_as_a_cache_key() {
        use std::collections::HashMap;
        let mut m: HashMap<DecompositionStrategy, u32> = HashMap::new();
        m.insert(DecompositionStrategy::temporary_and(1.0), 1);
        m.insert(DecompositionStrategy::Standard, 2);
        assert_eq!(m.get(&DecompositionStrategy::temporary_and(1.0)), Some(&1));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn strategies_are_ordered_as_expected() {
        let n = ToffoliCount::new(1000);
        let tb = DecompositionStrategy::Textbook.t_count(n).get();
        let st = DecompositionStrategy::Standard.t_count(n).get();
        let ta = DecompositionStrategy::default().t_count(n).get();
        assert!(ta < st && st < tb, "temp-AND < standard < textbook");
    }
}
