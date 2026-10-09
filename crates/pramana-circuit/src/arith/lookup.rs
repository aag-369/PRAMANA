//! Quantum table lookup (QROM) and the select-swap variant (QROAM).
//!
//! Table lookup is the engine behind windowed arithmetic: a windowed modular
//! exponentiation replaces most of its controlled multiplications with lookups into a
//! classically-precomputed table, and the balance between lookup cost and addition cost
//! is what the window optimiser in [`crate::arith::modexp`] resolves.
//!
//! Two constructions are implemented:
//!
//! - **Unary iteration** (Babbush et al.): reading one of `L` entries costs `L - 1` AND
//!   gates and holds `O(log L)` ancilla.
//! - **Select-swap / QROAM** (Berry et al., Low et al.): trades ancilla for gates by
//!   loading `k` entries at once and swapping the wanted one into place, costing about
//!   `L/k + k*w` AND gates while holding `k*w` ancilla.
//!
//! Uncomputation of a lookup is much cheaper than computation: the measurement-based
//! trick reduces it to `O(sqrt(L))`.

use crate::ir::CircuitBuilder;

/// A choice of lookup construction with its measured cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupPlan {
    /// Number of table entries.
    pub entries: u64,
    /// Bit width of each entry.
    pub output_bits: u32,
    /// Select-swap parameter. `1` means plain unary iteration.
    pub swap_factor: u64,
    /// AND gates required to compute the lookup.
    pub compute_ands: u64,
    /// AND gates required to uncompute it.
    pub uncompute_ands: u64,
    /// Ancilla qubits held during the lookup.
    pub ancilla: u64,
}

impl LookupPlan {
    /// Total AND cost of a compute/uncompute pair.
    pub fn total_ands(&self) -> u64 {
        self.compute_ands.saturating_add(self.uncompute_ands)
    }
}

/// Integer square root, rounded up.
fn isqrt_ceil(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut r = (n as f64).sqrt() as u64;
    while r * r < n {
        r += 1;
    }
    while r > 0 && (r - 1) * (r - 1) >= n {
        r -= 1;
    }
    r
}

/// Cost a lookup under a given select-swap factor.
///
/// `swap_factor = 1` is plain unary iteration.
pub fn plan_with_swap(entries: u64, output_bits: u32, swap_factor: u64) -> LookupPlan {
    let k = swap_factor.max(1);
    let blocks = entries.div_ceil(k);
    // Select over `blocks` addresses via unary iteration.
    let select = blocks.saturating_sub(1);
    // Swap network over k loaded entries of `output_bits` each.
    let swap = k.saturating_sub(1).saturating_mul(output_bits as u64);
    let compute = select.saturating_add(swap);
    // Measurement-based uncomputation: O(sqrt(entries)), plus the swap network again.
    let uncompute = isqrt_ceil(entries).saturating_mul(2).saturating_add(swap);
    LookupPlan {
        entries,
        output_bits,
        swap_factor: k,
        compute_ands: compute,
        uncompute_ands: uncompute,
        ancilla: k.saturating_mul(output_bits as u64),
    }
}

/// Choose the select-swap factor minimising total AND cost.
///
/// Searches powers of two, which is the standard restriction (the swap network is a
/// binary tree). The optimum sits near `sqrt(L / w)`; the search is exhaustive over the
/// legal range rather than analytic, so it stays correct if the cost model changes.
pub fn optimal_plan(entries: u64, output_bits: u32) -> LookupPlan {
    let mut best = plan_with_swap(entries, output_bits, 1);
    let mut k = 2u64;
    while k <= entries.max(2) {
        let p = plan_with_swap(entries, output_bits, k);
        if p.total_ands() < best.total_ands() {
            best = p;
        }
        if k > entries {
            break;
        }
        k = k.saturating_mul(2);
        if k > (1u64 << 40) {
            break;
        }
    }
    best
}

/// Emit a table lookup, using the cost-optimal select-swap factor.
///
/// Returns the plan that was used, so callers can record it in the provenance chain.
pub fn emit_lookup(b: &mut CircuitBuilder, entries: u64, output_bits: u32) -> LookupPlan {
    let plan = optimal_plan(entries, output_bits);
    emit_lookup_with_plan(b, &plan);
    plan
}

/// Emit a table lookup under a caller-chosen plan.
pub fn emit_lookup_with_plan(b: &mut CircuitBuilder, plan: &LookupPlan) {
    b.scope(
        format!("qrom[L={},w={},k={}]", plan.entries, plan.output_bits, plan.swap_factor),
        |b| {
            b.alloc(plan.ancilla.max(1));
            if plan.compute_ands > 0 {
                b.repeat("select_compute", plan.compute_ands, |b| {
                    b.and_compute();
                });
            }
            // Data write-out is Clifford (CNOT fan-out from the loaded entry).
            b.repeat("data_writeout", plan.output_bits as u64, |b| {
                b.clifford();
            });
            if plan.uncompute_ands > 0 {
                // Uncomputing a table lookup is NOT free. The measurement-based trick
                // (Berry et al.) reduces the cost from O(L) to O(sqrt(L)), but the
                // fixup circuit it leaves behind is built from genuine AND gates and
                // must be charged as such.
                //
                // This is distinct from the temporary-AND uncomputation inside an
                // adder, which really does cost zero T gates. Conflating the two
                // undercounts every windowed multiplication in the circuit.
                b.repeat("select_uncompute", plan.uncompute_ands, |b| {
                    b.and_compute();
                });
            }
            b.free(plan.ancilla.max(1));
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::CircuitBuilder;

    fn ands_of<F: FnOnce(&mut CircuitBuilder)>(f: F) -> u128 {
        let mut b = CircuitBuilder::new("test");
        f(&mut b);
        b.finish().fold().unwrap().resources.total_toffoli().get()
    }

    #[test]
    fn unary_iteration_costs_l_minus_one_to_compute() {
        for l in [2u64, 4, 16, 256, 1024] {
            let p = plan_with_swap(l, 64, 1);
            assert_eq!(p.compute_ands, l - 1, "L={l}");
        }
    }

    #[test]
    fn uncomputation_is_sublinear_in_table_size() {
        let p = plan_with_swap(65536, 64, 1);
        assert_eq!(p.compute_ands, 65535);
        assert_eq!(p.uncompute_ands, 512, "2*sqrt(65536) = 512");
        assert!(p.uncompute_ands * 100 < p.compute_ands);
    }

    #[test]
    fn select_swap_beats_unary_iteration_for_large_tables() {
        let l = 1u64 << 16;
        let w = 32u32;
        let unary = plan_with_swap(l, w, 1).total_ands();
        let best = optimal_plan(l, w);
        assert!(
            best.total_ands() < unary,
            "select-swap should win: best={} unary={unary}",
            best.total_ands()
        );
        assert!(best.swap_factor > 1);
    }

    #[test]
    fn optimal_swap_factor_is_a_local_minimum() {
        // Spec Law 7 / §7.5: the optimiser's output must be a genuine local minimum.
        for (l, w) in [(1u64 << 10, 64u32), (1 << 14, 32), (1 << 20, 16)] {
            let best = optimal_plan(l, w);
            let lower = plan_with_swap(l, w, (best.swap_factor / 2).max(1));
            let upper = plan_with_swap(l, w, best.swap_factor.saturating_mul(2));
            assert!(
                best.total_ands() <= lower.total_ands(),
                "L={l} w={w}: halving k improved cost"
            );
            assert!(
                best.total_ands() <= upper.total_ands(),
                "L={l} w={w}: doubling k improved cost"
            );
        }
    }

    #[test]
    fn optimum_sits_near_sqrt_l_over_w() {
        let l = 1u64 << 20;
        let w = 16u32;
        let best = optimal_plan(l, w);
        let analytic = ((l as f64) / (w as f64)).sqrt();
        let ratio = best.swap_factor as f64 / analytic;
        assert!(
            ratio > 0.25 && ratio < 4.0,
            "k={} vs analytic {analytic:.0}",
            best.swap_factor
        );
    }

    #[test]
    fn emitted_gate_count_matches_the_plan() {
        let plan = optimal_plan(4096, 64);
        let emitted = ands_of(|b| {
            emit_lookup_with_plan(b, &plan);
        });
        assert_eq!(
            emitted,
            plan.compute_ands as u128 + plan.uncompute_ands as u128,
            "lookup uncomputation is charged as real Toffolis"
        );
    }

    #[test]
    fn lookup_uncompute_is_charged_but_adder_uncompute_is_not() {
        // Regression guard for a real bug: QROM uncomputation was briefly emitted as
        // free temporary-AND uncompute, which silently undercounted every windowed
        // multiplication.
        use crate::arith::adder;
        let mut b = CircuitBuilder::new("t");
        emit_lookup(&mut b, 4096, 64);
        let lookup_r = b.finish().fold().unwrap().resources;
        assert!(
            lookup_r.and_uncompute.get() == 0,
            "lookup must not report free uncomputation"
        );

        let mut b = CircuitBuilder::new("t");
        adder::ripple_carry_add(&mut b, 64);
        let adder_r = b.finish().fold().unwrap().resources;
        assert_eq!(
            adder_r.and_uncompute.get(),
            63,
            "adder uncomputation genuinely is free and must stay so"
        );
    }

    #[test]
    fn lookup_releases_its_ancilla() {
        let mut b = CircuitBuilder::new("t");
        emit_lookup(&mut b, 1024, 64);
        let f = b.finish().fold().unwrap();
        assert_eq!(f.net_alloc, 0);
    }

    #[test]
    fn cost_is_monotone_in_table_size() {
        let mut prev = 0u64;
        for e in 1..=16u32 {
            let c = optimal_plan(1u64 << e, 32).total_ands();
            assert!(c >= prev, "non-monotone at L=2^{e}");
            prev = c;
        }
    }

    #[test]
    fn isqrt_ceil_is_correct() {
        assert_eq!(isqrt_ceil(0), 0);
        assert_eq!(isqrt_ceil(1), 1);
        assert_eq!(isqrt_ceil(15), 4);
        assert_eq!(isqrt_ceil(16), 4);
        assert_eq!(isqrt_ceil(17), 5);
    }
}
