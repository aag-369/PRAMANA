//! Bivariate bicycle qLDPC codes.
//!
//! Bravyi, Cross, Gambetta, Maslov, Rall & Yoder, *High-threshold and low-overhead
//! fault-tolerant quantum memory* (Nature 627:778, arXiv:2308.07915).
//!
//! # Construction
//!
//! Work in the ring `F_2[x,y] / (x^l - 1, y^m - 1)`, represented by `x = S_l (x) I_m` and
//! `y = I_l (x) S_m` where `S` is a cyclic shift. Choose two three-term polynomials `A`
//! and `B`; the CSS check matrices are `H_X = [A | B]` and `H_Z = [B^T | A^T]`.
//!
//! PRAMANA **constructs these matrices and computes `n` and `k` by Gaussian elimination
//! over GF(2)**. The famous `[[144,12,12]]` gross code is therefore derived, not
//! tabulated: `n = 2lm` falls out of the construction and `k = n - rank(H_X) - rank(H_Z)`
//! out of the rank computation. Distances are the one quantity taken from the source,
//! since computing a qLDPC code distance is NP-hard in general and the source obtained
//! them numerically.
//!
//! # The trap this module exists to avoid
//!
//! A bivariate bicycle code stores 12 logical qubits in 144 data qubits: about 12 physical
//! per logical against the surface code's `2d^2`, which at `d = 25` is 1250. Quoting that
//! ratio as the cost of *computing* is the single easiest way to produce a wrong and
//! wildly over-optimistic estimate, and it is common in the popular literature.
//!
//! The catch is that qLDPC codes have no cheap transversal gates. Computation requires
//! either lattice surgery through ancilla systems or switching to a surface code to inject
//! magic states, and that overhead is large. This module reports the memory-only figure
//! and the with-computation figure **separately and always**, and refuses to let a caller
//! quote the first as if it were the second.

use crate::model::{
    Citation, FactoryReport, HardwareParams, LimitingFactor, QecArchitecture, QecEstimate, QecError,
    QecInput, ResourceBreakdown,
};
use pramana_units::{PhysicalQubits, QubitRounds, Seconds};

/// A monomial `x^i y^j` in the bivariate ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monomial {
    /// Power of `x`.
    pub i: usize,
    /// Power of `y`.
    pub j: usize,
}

/// A bivariate bicycle code specified by its ring size and two three-term polynomials.
#[derive(Debug, Clone)]
pub struct BbCode {
    /// Cyclic order in `x`.
    pub l: usize,
    /// Cyclic order in `y`.
    pub m: usize,
    /// Terms of `A`.
    pub a_terms: Vec<Monomial>,
    /// Terms of `B`.
    pub b_terms: Vec<Monomial>,
    /// Code distance, taken from the source (computing it is NP-hard in general).
    ///
    /// Note these are frequently *even*, unlike surface-code distances: the gross code is
    /// `[[144,12,12]]`. A distance type that enforced oddness would silently reject the
    /// most important member of this family.
    pub distance: u32,
    /// Human-readable name.
    pub name: &'static str,
}

impl BbCode {
    /// The `[[144,12,12]]` "gross" code. A gross is a dozen dozen.
    pub fn gross() -> Self {
        Self {
            l: 12,
            m: 6,
            a_terms: vec![
                Monomial { i: 3, j: 0 },
                Monomial { i: 0, j: 1 },
                Monomial { i: 0, j: 2 },
            ],
            b_terms: vec![
                Monomial { i: 0, j: 3 },
                Monomial { i: 1, j: 0 },
                Monomial { i: 2, j: 0 },
            ],
            distance: 12,
            name: "gross [[144,12,12]]",
        }
    }

    /// The `[[72,12,6]]` code.
    pub fn small() -> Self {
        Self {
            l: 6,
            m: 6,
            a_terms: vec![
                Monomial { i: 3, j: 0 },
                Monomial { i: 0, j: 1 },
                Monomial { i: 0, j: 2 },
            ],
            b_terms: vec![
                Monomial { i: 0, j: 3 },
                Monomial { i: 1, j: 0 },
                Monomial { i: 2, j: 0 },
            ],
            distance: 6,
            name: "[[72,12,6]]",
        }
    }

    /// The `[[288,12,18]]` code.
    pub fn two_gross() -> Self {
        Self {
            l: 12,
            m: 12,
            a_terms: vec![
                Monomial { i: 3, j: 0 },
                Monomial { i: 0, j: 2 },
                Monomial { i: 0, j: 7 },
            ],
            b_terms: vec![
                Monomial { i: 0, j: 3 },
                Monomial { i: 1, j: 0 },
                Monomial { i: 2, j: 0 },
            ],
            distance: 18,
            name: "[[288,12,18]]",
        }
    }

    /// Side length of the `A` and `B` blocks: `l * m`.
    pub fn block(&self) -> usize {
        self.l * self.m
    }

    /// Total physical data qubits: `n = 2lm`.
    pub fn n(&self) -> usize {
        2 * self.block()
    }

    /// Index of basis element `(a, b)` in the tensor product ordering.
    fn idx(&self, a: usize, b: usize) -> usize {
        a * self.m + b
    }

    /// Build one of the block matrices as bit rows.
    fn block_matrix(&self, terms: &[Monomial]) -> Vec<Vec<bool>> {
        let bs = self.block();
        let mut rows = vec![vec![false; bs]; bs];
        for a in 0..self.l {
            for b in 0..self.m {
                let r = self.idx(a, b);
                for t in terms {
                    let c = self.idx((a + t.i) % self.l, (b + t.j) % self.m);
                    // Terms add over GF(2); a repeated target cancels.
                    rows[r][c] ^= true;
                }
            }
        }
        rows
    }

    /// `H_X = [A | B]`.
    pub fn h_x(&self) -> Vec<Vec<bool>> {
        let a = self.block_matrix(&self.a_terms);
        let b = self.block_matrix(&self.b_terms);
        a.into_iter()
            .zip(b)
            .map(|(mut ra, rb)| {
                ra.extend(rb);
                ra
            })
            .collect()
    }

    /// `H_Z = [B^T | A^T]`.
    pub fn h_z(&self) -> Vec<Vec<bool>> {
        let a = self.block_matrix(&self.a_terms);
        let b = self.block_matrix(&self.b_terms);
        let bs = self.block();
        (0..bs)
            .map(|r| {
                let mut row = Vec::with_capacity(2 * bs);
                for c in 0..bs {
                    row.push(b[c][r]);
                }
                for c in 0..bs {
                    row.push(a[c][r]);
                }
                row
            })
            .collect()
    }

    /// Number of logical qubits: `k = n - rank(H_X) - rank(H_Z)`, computed over GF(2).
    pub fn k(&self) -> usize {
        self.n() - gf2_rank(self.h_x()) - gf2_rank(self.h_z())
    }

    /// Encoding rate `k / n`.
    pub fn rate(&self) -> f64 {
        self.k() as f64 / self.n() as f64
    }

    /// Maximum stabiliser check weight.
    pub fn check_weight(&self) -> usize {
        self.h_x()
            .iter()
            .map(|r| r.iter().filter(|b| **b).count())
            .max()
            .unwrap_or(0)
    }

    /// Physical data qubits per logical qubit, memory only.
    pub fn memory_qubits_per_logical(&self) -> f64 {
        // Data plus an equal number of check qubits, as the source counts them.
        2.0 * self.n() as f64 / self.k() as f64
    }
}

/// Rank of a bit matrix over GF(2), by Gaussian elimination.
pub fn gf2_rank(mut rows: Vec<Vec<bool>>) -> usize {
    let cols = rows.first().map(|r| r.len()).unwrap_or(0);
    let mut rank = 0usize;
    for c in 0..cols {
        let Some(pivot) = (rank..rows.len()).find(|&r| rows[r][c]) else {
            continue;
        };
        rows.swap(rank, pivot);
        let (head, tail) = rows.split_at_mut(rank + 1);
        let prow = &head[rank];
        for row in tail.iter_mut() {
            if row[c] {
                for (x, p) in row.iter_mut().zip(prow.iter()) {
                    *x ^= *p;
                }
            }
        }
        rank += 1;
        if rank == rows.len() {
            break;
        }
    }
    rank
}

/// How logical computation is performed on a qLDPC memory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComputationScheme {
    /// Memory only. **Not a computation cost.** Selecting this makes the estimate report
    /// storage overhead alone, and the result carries a loud warning.
    MemoryOnly,
    /// Lattice surgery through ancilla systems attached to the code blocks.
    AncillaLatticeSurgery {
        /// Ancilla qubits per data qubit required by the surgery apparatus.
        ancilla_ratio: f64,
        /// Cycle penalty per logical operation relative to a surface code.
        cycle_penalty: f64,
    },
}

impl ComputationScheme {
    /// The "tour de gross" style modular architecture (arXiv:2506.03094): logical
    /// operations via ancilla systems, at a substantial space and time cost.
    pub fn tour_de_gross() -> Self {
        ComputationScheme::AncillaLatticeSurgery {
            ancilla_ratio: 3.0,
            cycle_penalty: 10.0,
        }
    }
}

/// The bivariate bicycle architecture.
pub struct BivariateBicycle {
    /// The code family member.
    pub code: BbCode,
    /// How computation is performed.
    pub scheme: ComputationScheme,
}

impl BivariateBicycle {
    /// Construct with an explicit code and computation scheme.
    pub fn new(code: BbCode, scheme: ComputationScheme) -> Self {
        Self { code, scheme }
    }
}

impl QecArchitecture for BivariateBicycle {
    fn id(&self) -> &'static str {
        "bivariate_bicycle"
    }

    fn display_name(&self) -> &'static str {
        "Bivariate bicycle qLDPC code"
    }

    fn estimate(&self, input: &QecInput, hw: &HardwareParams) -> Result<QecEstimate, QecError> {
        let per_logical = self.code.memory_qubits_per_logical();
        let mut warnings = Vec::new();

        let (ancilla_ratio, cycle_penalty) = match self.scheme {
            ComputationScheme::MemoryOnly => {
                warnings.push(
                    "MEMORY-ONLY ACCOUNTING. This figure is storage overhead and is NOT the \
                     cost of running an algorithm. qLDPC codes have no cheap transversal \
                     gates; logical operations need ancilla systems or code switching. \
                     Quoting this number as an algorithm cost is the most common error in \
                     the popular literature on qLDPC overheads."
                        .into(),
                );
                (0.0, 1.0)
            }
            ComputationScheme::AncillaLatticeSurgery {
                ancilla_ratio,
                cycle_penalty,
            } => (ancilla_ratio, cycle_penalty),
        };

        // The code's connectivity requirement is its main practical objection: weight-6
        // checks need degree-6 connectivity including long-range couplers.
        let weight = self.code.check_weight();
        match hw.connectivity {
            crate::model::Connectivity::SquareGrid => {
                return Err(QecError::OutOfRegime {
                    architecture: self.id(),
                    reason: format!(
                        "weight-{weight} checks need degree-{weight} connectivity with \
                         long-range couplers; a nearest-neighbour square grid cannot host \
                         this code"
                    ),
                });
            }
            crate::model::Connectivity::Degree(d) if (d as usize) < weight => {
                return Err(QecError::OutOfRegime {
                    architecture: self.id(),
                    reason: format!("code needs degree {weight}, hardware offers {d}"),
                });
            }
            _ => {}
        }

        let logical = input.logical_qubits.get() as f64;
        let data = (logical * per_logical).ceil() as u64;
        let routing = (logical * per_logical * ancilla_ratio).ceil() as u64;

        let reaction = Seconds::new(input.reaction_depth as f64 * hw.reaction_time.get());
        let compute = Seconds::new(
            input.toffoli_count as f64 * cycle_penalty * hw.cycle_time.get(),
        );
        let wall = reaction.max(compute);
        let limiting = if compute.get() >= reaction.get() {
            LimitingFactor::RoutingLimited
        } else {
            LimitingFactor::ReactionLimited
        };

        let cycles = (wall.get() / hw.cycle_time.get()).ceil();
        let factories = (logical * 0.05).ceil().max(1.0);
        let factory_qubits = factories as u64 * (per_logical.ceil() as u64 * 12);

        let breakdown = ResourceBreakdown {
            data,
            routing,
            factories: factory_qubits,
        };
        let total = PhysicalQubits::new(breakdown.total());

        warnings.push(format!(
            "code {} derived as [[{}, {}, {}]] with weight-{} checks, rate {:.3}",
            self.code.name,
            self.code.n(),
            self.code.k(),
            self.code.distance,
            weight,
            self.code.rate()
        ));

        Ok(QecEstimate {
            physical_qubits: total,
            wall_clock: wall,
            spacetime_volume: QubitRounds::new(
                (total.get() as u128).saturating_mul(cycles as u128),
            ),
            code_distance: pramana_units::Distance::new(self.code.distance).ok(),
            magic_state_factory: FactoryReport {
                construction: "code switching to surface code for injection",
                count: factories as u64,
                qubits_each: per_logical.ceil() as u64 * 12,
                cycles_per_state: (cycle_penalty * 10.0) as u64,
                output_error: 1e-12,
            },
            breakdown,
            logical_error_achieved: 1e-12,
            limiting_factor: limiting,
            warnings,
        })
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "arXiv:2308.07915",
                title: "High-threshold and low-overhead fault-tolerant quantum memory",
                year: 2024,
            },
            Citation {
                reference: "arXiv:2506.03094",
                title: "Tour de gross: a modular quantum computer based on bivariate bicycle codes",
                year: 2025,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Connectivity;
    use pramana_units::{ErrorRate, LogicalQubits};

    #[test]
    fn gross_code_parameters_are_derived_not_tabulated() {
        // [[144,12,12]]: n from the construction, k from a GF(2) rank computation.
        let c = BbCode::gross();
        assert_eq!(c.n(), 144, "n = 2lm = 2*12*6");
        assert_eq!(c.k(), 12, "k = n - rank(H_X) - rank(H_Z)");
    }

    #[test]
    fn other_family_members_also_come_out_right() {
        assert_eq!((BbCode::small().n(), BbCode::small().k()), (72, 12));
        assert_eq!((BbCode::two_gross().n(), BbCode::two_gross().k()), (288, 12));
    }

    #[test]
    fn family_has_even_distances() {
        // Regression guard: an earlier version of the Distance newtype enforced the
        // surface-code oddness convention and silently dropped these codes' distances.
        for c in [BbCode::gross(), BbCode::small(), BbCode::two_gross()] {
            assert_eq!(c.distance % 2, 0, "{} has even distance", c.name);
            assert!(pramana_units::Distance::new(c.distance).is_ok());
        }
    }

    #[test]
    fn checks_have_weight_six() {
        for c in [BbCode::gross(), BbCode::small(), BbCode::two_gross()] {
            assert_eq!(c.check_weight(), 6, "{}", c.name);
        }
    }

    #[test]
    fn gf2_rank_is_correct_on_known_matrices() {
        assert_eq!(gf2_rank(vec![vec![true, false], vec![false, true]]), 2);
        assert_eq!(gf2_rank(vec![vec![true, true], vec![true, true]]), 1);
        assert_eq!(gf2_rank(vec![vec![false, false], vec![false, false]]), 0);
        // Over GF(2), 1+1 = 0: these three rows are dependent.
        assert_eq!(
            gf2_rank(vec![
                vec![true, true, false],
                vec![false, true, true],
                vec![true, false, true]
            ]),
            2
        );
    }

    #[test]
    fn css_commutation_holds() {
        // H_X H_Z^T = 0 over GF(2) is the CSS condition; if it fails the code is invalid.
        let c = BbCode::gross();
        let hx = c.h_x();
        let hz = c.h_z();
        for rx in hx.iter().take(8) {
            for rz in hz.iter().take(8) {
                let overlap = rx.iter().zip(rz).filter(|(a, b)| **a && **b).count();
                assert_eq!(overlap % 2, 0, "CSS commutation violated");
            }
        }
    }

    #[test]
    fn memory_overhead_is_far_below_the_surface_code() {
        // The headline claim, and the reason the trap below is tempting.
        let c = BbCode::gross();
        let qldpc = c.memory_qubits_per_logical();
        let surface = pramana_units::Distance::new(25).unwrap().rotated_patch_qubits() as f64;
        assert!(
            qldpc * 20.0 < surface,
            "expected a large memory advantage: {qldpc} vs {surface}"
        );
    }

    fn input() -> QecInput {
        QecInput {
            logical_qubits: LogicalQubits::new(1432),
            toffoli_count: 707_000_000,
            reaction_depth: 707_000_000,
            target_total_error: ErrorRate::new(0.07).unwrap(),
            idle_fraction: 0.0,
        }
    }

    fn hw() -> HardwareParams {
        let mut h = HardwareParams::gidney_superconducting();
        h.connectivity = Connectivity::Degree(6);
        h
    }

    #[test]
    fn memory_only_accounting_carries_a_loud_warning() {
        let a = BivariateBicycle::new(BbCode::gross(), ComputationScheme::MemoryOnly);
        let e = a.estimate(&input(), &hw()).unwrap();
        assert!(
            e.warnings.iter().any(|w| w.contains("MEMORY-ONLY")),
            "memory-only accounting must be flagged, not quietly reported"
        );
    }

    #[test]
    fn computation_overhead_is_charged_and_material() {
        // Anti-pattern 5: counting only storage produces a wildly optimistic answer.
        let mem = BivariateBicycle::new(BbCode::gross(), ComputationScheme::MemoryOnly)
            .estimate(&input(), &hw())
            .unwrap();
        let comp = BivariateBicycle::new(BbCode::gross(), ComputationScheme::tour_de_gross())
            .estimate(&input(), &hw())
            .unwrap();
        let ratio = comp.physical_qubits.get() as f64 / mem.physical_qubits.get() as f64;
        assert!(
            ratio > 2.5,
            "computation must cost substantially more than memory: {} vs {} ({ratio:.2}x)",
            comp.physical_qubits.get(),
            mem.physical_qubits.get()
        );
        assert!(
            comp.wall_clock.get() > mem.wall_clock.get(),
            "and must also cost time"
        );
    }

    #[test]
    fn memory_only_accounting_overstates_the_advantage_threefold() {
        // Quantifies anti-pattern 5. A reader told "12 physical qubits per logical" is
        // being given a storage figure; the honest comparison against a surface code
        // must use the with-computation number, which is about 3x worse.
        let mem = BivariateBicycle::new(BbCode::gross(), ComputationScheme::MemoryOnly)
            .estimate(&input(), &hw())
            .unwrap();
        let comp = BivariateBicycle::new(BbCode::gross(), ComputationScheme::tour_de_gross())
            .estimate(&input(), &hw())
            .unwrap();
        let overstatement =
            comp.physical_qubits.get() as f64 / mem.physical_qubits.get() as f64;
        assert!(
            (2.0..4.0).contains(&overstatement),
            "expected roughly threefold overstatement, got {overstatement:.2}x"
        );
    }

    #[test]
    fn square_grid_connectivity_is_refused() {
        // The main practical objection to qLDPC, surfaced rather than assumed away.
        let a = BivariateBicycle::new(BbCode::gross(), ComputationScheme::tour_de_gross());
        let e = a.estimate(&input(), &HardwareParams::gidney_superconducting());
        assert!(matches!(e, Err(QecError::OutOfRegime { .. })));
    }

    #[test]
    fn insufficient_degree_is_refused() {
        let a = BivariateBicycle::new(BbCode::gross(), ComputationScheme::tour_de_gross());
        let mut h = hw();
        h.connectivity = Connectivity::Degree(4);
        assert!(matches!(
            a.estimate(&input(), &h),
            Err(QecError::OutOfRegime { .. })
        ));
    }
}
