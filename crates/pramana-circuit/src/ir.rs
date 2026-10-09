//! Circuit intermediate representation and resource accumulator.
//!
//! # Synthesis, not formula evaluation
//!
//! PRAMANA's central methodological commitment (spec §7.1) is that resource counts
//! *emerge from assembling a circuit*, rather than being read off a closed-form
//! formula. Every primitive in [`crate::arith`] is built by emitting gates in loops
//! whose trip counts are determined by the structure of the algorithm.
//!
//! # The repetition node, and why it is not a cheat
//!
//! A naive reading of "assemble every gate" is intractable: a windowed modular
//! exponentiation for RSA-2048 contains on the order of `10^9` Toffoli gates, which
//! cannot be materialised as individual objects.
//!
//! The IR therefore supports [`Node::Repeat`], which represents a subcircuit executed
//! a structurally-determined number of times. The subcircuit itself is *fully
//! assembled, gate by gate*; only its repetition is folded. This preserves the property
//! that no cost is asserted by formula — the per-iteration cost is measured from real
//! construction, and the iteration count comes from loop structure, not from a fitted
//! constant.
//!
//! The distinction that matters is this: a lookup table of published results would let
//! you change `n` and get an answer with no circuit involved. Here, changing `n`
//! changes the assembled body, the window optimiser's choice, the allocation profile,
//! and hence the count. Nothing is memorised.
//!
//! [`Node::Repeat`] requires its body to be allocation-balanced, so that peak qubit
//! usage remains well-defined under folding.

use pramana_units::{
    CliffordCount, DecompositionStrategy, LogicalQubits, MeasurementCount, TCount, ToffoliCount,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can arise while assembling or folding a circuit.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CircuitError {
    /// A [`Node::Repeat`] body allocated or freed a net non-zero number of qubits.
    #[error(
        "repeat body '{scope}' is not allocation-balanced (net {net} qubits); \
         peak usage would be undefined under folding"
    )]
    UnbalancedRepeatBody {
        /// Name of the offending scope.
        scope: String,
        /// The net allocation, which must be zero.
        net: i64,
    },

    /// More qubits were freed than were allocated.
    #[error("scope '{scope}' freed more qubits than it allocated")]
    NegativeAllocation {
        /// Name of the offending scope.
        scope: String,
    },
}

/// A single quantum gate, at the granularity PRAMANA costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum GateKind {
    /// A Clifford gate. Free under surface-code-like architectures (Pauli frame
    /// tracking), but counted because some architectures do charge for them.
    Clifford,
    /// An explicit Toffoli (CCX) not expressed as a temporary-AND pair.
    Toffoli,
    /// A bare T gate.
    TGate,
    /// Gidney's temporary-AND computation: 4 T gates, one ancilla held until uncompute.
    AndCompute,
    /// Measurement-based uncomputation of a temporary AND. Costs no T gates; consumes
    /// a measurement and a classically-controlled Clifford fixup.
    AndUncompute,
    /// A measurement in the given basis.
    Measure,
    /// A z-rotation synthesised to `precision_bits` of accuracy via Ross-Selinger.
    Rotation {
        /// Bits of precision requested.
        precision_bits: u32,
    },
}

/// A node in the circuit tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "node")]
pub enum Node {
    /// A single gate.
    Gate(GateKind),
    /// Allocation of `n` logical ancilla qubits.
    Alloc(u64),
    /// Release of `n` logical ancilla qubits.
    Free(u64),
    /// A named group of child nodes, executed in sequence.
    Scope {
        /// Human-readable name, used for the provenance breakdown in the UI.
        name: String,
        /// Child nodes.
        children: Vec<Node>,
    },
    /// A subcircuit executed `times` times. The body must be allocation-balanced.
    Repeat {
        /// Number of repetitions, determined by algorithm structure.
        times: u64,
        /// The repeated body.
        body: Box<Node>,
    },
}

/// Measured resource counts for a circuit or subcircuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResourceCount {
    /// Explicit Toffoli gates.
    pub toffoli: ToffoliCount,
    /// Temporary-AND computations (each a Toffoli-equivalent with cheap uncompute).
    pub and_compute: ToffoliCount,
    /// Measurement-based AND uncomputations.
    pub and_uncompute: ToffoliCount,
    /// Bare T gates emitted directly (e.g. from rotation synthesis).
    pub bare_t: TCount,
    /// Clifford gates.
    pub clifford: CliffordCount,
    /// Measurements.
    pub measurements: MeasurementCount,
    /// Sequential layers requiring a classical feedback round.
    ///
    /// For Shor-like circuits this, not the gate count, sets the wall clock: every
    /// non-Clifford operation consumes a magic state whose correction depends on a
    /// measurement outcome, and the control system cannot issue the next layer until
    /// that outcome comes back. A model that counts only magic states will get the
    /// runtime badly wrong (spec anti-pattern 4).
    pub reaction_depth: u64,
}

impl ResourceCount {
    /// The total count of Toffoli-equivalent non-Clifford operations.
    ///
    /// This is the quantity most published estimates report as "Toffoli count".
    pub fn total_toffoli(&self) -> ToffoliCount {
        self.toffoli.saturating_add(self.and_compute)
    }

    /// Total T count under a given decomposition strategy.
    ///
    /// Temporary-AND pairs are costed as fully paired (2 T amortised per AND under the
    /// default strategy); explicit Toffolis are costed at the strategy's unpaired rate.
    pub fn total_t(&self, strategy: DecompositionStrategy) -> TCount {
        let paired = DecompositionStrategy::temporary_and(1.0).t_count(self.and_compute);
        let explicit = strategy.t_count(self.toffoli);
        TCount::new(
            paired
                .get()
                .saturating_add(explicit.get())
                .saturating_add(self.bare_t.get()),
        )
    }

    /// Accumulate another count into this one.
    pub fn add_assign(&mut self, other: &ResourceCount) {
        self.toffoli = self.toffoli.saturating_add(other.toffoli);
        self.and_compute = self.and_compute.saturating_add(other.and_compute);
        self.and_uncompute = self.and_uncompute.saturating_add(other.and_uncompute);
        self.bare_t = TCount::new(self.bare_t.get().saturating_add(other.bare_t.get()));
        self.clifford =
            CliffordCount::new(self.clifford.get().saturating_add(other.clifford.get()));
        self.measurements =
            MeasurementCount::new(self.measurements.get().saturating_add(other.measurements.get()));
        // Scopes compose sequentially, so reaction depths add.
        self.reaction_depth = self.reaction_depth.saturating_add(other.reaction_depth);
    }

    /// Multiply every count by a repetition factor.
    pub fn scale(&self, k: u64) -> ResourceCount {
        let k = k as u128;
        ResourceCount {
            toffoli: ToffoliCount::new(self.toffoli.get().saturating_mul(k)),
            and_compute: ToffoliCount::new(self.and_compute.get().saturating_mul(k)),
            and_uncompute: ToffoliCount::new(self.and_uncompute.get().saturating_mul(k)),
            bare_t: TCount::new(self.bare_t.get().saturating_mul(k)),
            clifford: CliffordCount::new(self.clifford.get().saturating_mul(k)),
            measurements: MeasurementCount::new(self.measurements.get().saturating_mul(k)),
            reaction_depth: self.reaction_depth.saturating_mul(k as u64),
        }
    }

    /// Cost of a single gate.
    fn of_gate(g: GateKind) -> ResourceCount {
        let mut r = ResourceCount::default();
        match g {
            GateKind::Clifford => r.clifford = CliffordCount::new(1),
            GateKind::Toffoli => {
                r.toffoli = ToffoliCount::new(1);
                r.reaction_depth = 1;
            }
            GateKind::TGate => r.bare_t = TCount::new(1),
            GateKind::AndCompute => {
                r.and_compute = ToffoliCount::new(1);
                r.reaction_depth = 1;
            }
            GateKind::AndUncompute => {
                r.and_uncompute = ToffoliCount::new(1);
                r.measurements = MeasurementCount::new(1);
                r.clifford = CliffordCount::new(1);
            }
            GateKind::Measure => r.measurements = MeasurementCount::new(1),
            GateKind::Rotation { precision_bits } => {
                // Ross-Selinger: a z-rotation to accuracy epsilon costs approximately
                // 3*log2(1/epsilon) + constant T gates. With `precision_bits` bits,
                // log2(1/epsilon) = precision_bits.
                let t = ROTATION_T_SLOPE * precision_bits as f64 + ROTATION_T_INTERCEPT;
                r.bare_t = TCount::new(t.max(0.0).ceil() as u128);
            }
        }
        r
    }
}

/// Ross-Selinger slope: T gates per bit of rotation precision.
pub const ROTATION_T_SLOPE: f64 = 3.0;
/// Ross-Selinger additive constant.
pub const ROTATION_T_INTERCEPT: f64 = 0.0;

/// The result of folding a circuit tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Folded {
    /// Accumulated resource counts.
    pub resources: ResourceCount,
    /// Net change in allocated ancilla (zero for a well-formed top-level circuit).
    pub net_alloc: i64,
    /// Peak simultaneous ancilla allocation within this node.
    pub peak_alloc: u64,
}

impl Node {
    /// Fold the tree, measuring resources and peak ancilla allocation.
    pub fn fold(&self) -> Result<Folded, CircuitError> {
        self.fold_named("<root>")
    }

    fn fold_named(&self, scope_name: &str) -> Result<Folded, CircuitError> {
        match self {
            Node::Gate(g) => Ok(Folded {
                resources: ResourceCount::of_gate(*g),
                net_alloc: 0,
                peak_alloc: 0,
            }),
            Node::Alloc(n) => Ok(Folded {
                resources: ResourceCount::default(),
                net_alloc: *n as i64,
                peak_alloc: *n,
            }),
            Node::Free(n) => Ok(Folded {
                resources: ResourceCount::default(),
                net_alloc: -(*n as i64),
                peak_alloc: 0,
            }),
            Node::Scope { name, children } => {
                let mut resources = ResourceCount::default();
                let mut running: i64 = 0;
                let mut peak: i64 = 0;
                for child in children {
                    let f = child.fold_named(name)?;
                    resources.add_assign(&f.resources);
                    // Peak during the child is the running level plus the child's own peak.
                    let during = running + f.peak_alloc as i64;
                    if during > peak {
                        peak = during;
                    }
                    running += f.net_alloc;
                    if running < 0 {
                        return Err(CircuitError::NegativeAllocation { scope: name.clone() });
                    }
                    if running > peak {
                        peak = running;
                    }
                }
                Ok(Folded {
                    resources,
                    net_alloc: running,
                    peak_alloc: peak.max(0) as u64,
                })
            }
            Node::Repeat { times, body } => {
                let f = body.fold_named(scope_name)?;
                if f.net_alloc != 0 {
                    return Err(CircuitError::UnbalancedRepeatBody {
                        scope: scope_name.to_string(),
                        net: f.net_alloc,
                    });
                }
                Ok(Folded {
                    resources: f.resources.scale(*times),
                    net_alloc: 0,
                    // Iterations are sequential and balanced, so peak is per-iteration.
                    peak_alloc: f.peak_alloc,
                })
            }
        }
    }

    /// Per-scope resource breakdown, one entry per named scope at any depth.
    ///
    /// This is what the `CircuitResourcePanel` UI view renders, and it is the evidence
    /// that synthesis actually happened: it shows which subroutine contributed which
    /// gates.
    pub fn breakdown(&self) -> Vec<(String, ResourceCount)> {
        let mut out = Vec::new();
        self.breakdown_into(1, &mut out);
        out
    }

    fn breakdown_into(&self, multiplier: u64, out: &mut Vec<(String, ResourceCount)>) {
        match self {
            Node::Scope { name, children } => {
                if let Ok(f) = self.fold_named(name) {
                    out.push((name.clone(), f.resources.scale(multiplier)));
                }
                for c in children {
                    c.breakdown_into(multiplier, out);
                }
            }
            Node::Repeat { times, body } => {
                body.breakdown_into(multiplier.saturating_mul(*times), out);
            }
            _ => {}
        }
    }
}

/// Builder for assembling a circuit by emitting gates.
#[derive(Debug, Clone)]
pub struct CircuitBuilder {
    stack: Vec<(String, Vec<Node>)>,
}

impl CircuitBuilder {
    /// Start a new circuit with the given top-level name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            stack: vec![(name.into(), Vec::new())],
        }
    }

    fn top(&mut self) -> &mut Vec<Node> {
        &mut self.stack.last_mut().expect("builder stack never empty").1
    }

    /// Open a named scope. Must be matched by [`CircuitBuilder::end_scope`].
    pub fn begin_scope(&mut self, name: impl Into<String>) {
        self.stack.push((name.into(), Vec::new()));
    }

    /// Close the innermost scope.
    pub fn end_scope(&mut self) {
        let (name, children) = self.stack.pop().expect("unbalanced end_scope");
        assert!(!self.stack.is_empty(), "cannot end the root scope");
        self.top().push(Node::Scope { name, children });
    }

    /// Run `f` inside a named scope.
    pub fn scope<F: FnOnce(&mut Self)>(&mut self, name: impl Into<String>, f: F) {
        self.begin_scope(name);
        f(self);
        self.end_scope();
    }

    /// Assemble `f` once and record it as executed `times` times.
    ///
    /// The body is genuinely built; only its repetition is folded. See the module docs
    /// for why this preserves the "no formula" property.
    pub fn repeat<F: FnOnce(&mut Self)>(&mut self, name: impl Into<String>, times: u64, f: F) {
        self.begin_scope(name);
        f(self);
        let (name, children) = self.stack.pop().expect("unbalanced repeat");
        let body = Node::Scope { name, children };
        self.top().push(Node::Repeat {
            times,
            body: Box::new(body),
        });
    }

    /// Emit a Clifford gate.
    pub fn clifford(&mut self) -> &mut Self {
        self.top().push(Node::Gate(GateKind::Clifford));
        self
    }

    /// Emit `n` Clifford gates.
    pub fn cliffords(&mut self, n: u64) -> &mut Self {
        for _ in 0..n {
            self.clifford();
        }
        self
    }

    /// Emit an explicit Toffoli.
    pub fn toffoli(&mut self) -> &mut Self {
        self.top().push(Node::Gate(GateKind::Toffoli));
        self
    }

    /// Emit a temporary-AND computation.
    pub fn and_compute(&mut self) -> &mut Self {
        self.top().push(Node::Gate(GateKind::AndCompute));
        self
    }

    /// Emit a measurement-based AND uncomputation.
    pub fn and_uncompute(&mut self) -> &mut Self {
        self.top().push(Node::Gate(GateKind::AndUncompute));
        self
    }

    /// Emit a measurement.
    pub fn measure(&mut self) -> &mut Self {
        self.top().push(Node::Gate(GateKind::Measure));
        self
    }

    /// Emit a z-rotation synthesised to `precision_bits`.
    pub fn rotation(&mut self, precision_bits: u32) -> &mut Self {
        self.top()
            .push(Node::Gate(GateKind::Rotation { precision_bits }));
        self
    }

    /// Allocate `n` ancilla qubits.
    pub fn alloc(&mut self, n: u64) -> &mut Self {
        self.top().push(Node::Alloc(n));
        self
    }

    /// Free `n` ancilla qubits.
    pub fn free(&mut self, n: u64) -> &mut Self {
        self.top().push(Node::Free(n));
        self
    }

    /// Finish building, returning the root node.
    pub fn finish(mut self) -> Node {
        assert_eq!(self.stack.len(), 1, "unbalanced scopes at finish()");
        let (name, children) = self.stack.pop().unwrap();
        Node::Scope { name, children }
    }
}

/// A fully synthesised circuit with measured resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CircuitIR {
    /// Circuit name.
    pub name: String,
    /// The assembled tree.
    pub root: Node,
    /// Measured resource counts.
    pub resources: ResourceCount,
    /// Peak simultaneous logical qubits, including data registers and ancilla.
    pub logical_qubits: LogicalQubits,
    /// Data-register width, excluding ancilla, for reporting.
    pub data_qubits: LogicalQubits,
}

impl CircuitIR {
    /// Fold a built tree into a measured circuit.
    ///
    /// `data_qubits` is the width of the persistent data registers, which the builder
    /// does not model as allocations; peak ancilla is measured and added to it.
    pub fn from_node(
        name: impl Into<String>,
        root: Node,
        data_qubits: LogicalQubits,
    ) -> Result<Self, CircuitError> {
        let folded = root.fold()?;
        Ok(CircuitIR {
            name: name.into(),
            resources: folded.resources,
            logical_qubits: data_qubits + LogicalQubits::new(folded.peak_alloc),
            data_qubits,
            root,
        })
    }

    /// Per-scope resource breakdown.
    pub fn breakdown(&self) -> Vec<(String, ResourceCount)> {
        self.root.breakdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_accumulate_by_construction() {
        let mut b = CircuitBuilder::new("t");
        for _ in 0..5 {
            b.and_compute();
        }
        let n = b.finish();
        let f = n.fold().unwrap();
        assert_eq!(f.resources.and_compute.get(), 5);
        assert_eq!(f.resources.total_toffoli().get(), 5);
    }

    #[test]
    fn repeat_multiplies_a_genuinely_assembled_body() {
        let mut b = CircuitBuilder::new("t");
        b.repeat("inner", 1000, |b| {
            for _ in 0..7 {
                b.and_compute();
            }
        });
        let f = b.finish().fold().unwrap();
        assert_eq!(f.resources.and_compute.get(), 7000);
    }

    #[test]
    fn repeat_rejects_unbalanced_bodies() {
        let mut b = CircuitBuilder::new("t");
        b.repeat("leaky", 10, |b| {
            b.alloc(4);
            b.and_compute();
            // deliberately no free()
        });
        let err = b.finish().fold().unwrap_err();
        assert!(matches!(err, CircuitError::UnbalancedRepeatBody { .. }));
    }

    #[test]
    fn peak_allocation_is_measured_not_assumed() {
        let mut b = CircuitBuilder::new("t");
        b.scope("a", |b| {
            b.alloc(10);
            b.scope("nested", |b| {
                b.alloc(5);
                b.free(5);
            });
            b.free(10);
        });
        b.scope("b", |b| {
            b.alloc(3);
            b.free(3);
        });
        let f = b.finish().fold().unwrap();
        assert_eq!(f.peak_alloc, 15, "peak is 10 + 5 held simultaneously");
        assert_eq!(f.net_alloc, 0);
    }

    #[test]
    fn sequential_scopes_reuse_freed_ancilla() {
        let mut b = CircuitBuilder::new("t");
        for _ in 0..100 {
            b.scope("step", |b| {
                b.alloc(8);
                b.and_compute();
                b.free(8);
            });
        }
        let f = b.finish().fold().unwrap();
        assert_eq!(f.peak_alloc, 8, "freed ancilla must be reused, not accumulated");
    }

    #[test]
    fn negative_allocation_is_an_error() {
        let mut b = CircuitBuilder::new("t");
        b.alloc(2);
        b.free(5);
        let err = b.finish().fold().unwrap_err();
        assert!(matches!(err, CircuitError::NegativeAllocation { .. }));
    }

    #[test]
    fn reaction_depth_accumulates_sequentially_and_through_repeats() {
        let mut b = CircuitBuilder::new("t");
        b.repeat("outer", 100, |b| {
            for _ in 0..5 {
                b.and_compute();
            }
        });
        let f = b.finish().fold().unwrap();
        assert_eq!(f.resources.reaction_depth, 500);
    }

    #[test]
    fn clifford_gates_do_not_add_reaction_depth() {
        let mut b = CircuitBuilder::new("t");
        b.cliffords(1000);
        assert_eq!(b.finish().fold().unwrap().resources.reaction_depth, 0);
    }

    #[test]
    fn rotation_costs_scale_with_precision() {
        let mut b = CircuitBuilder::new("t");
        b.rotation(20);
        let f = b.finish().fold().unwrap();
        assert_eq!(f.resources.bare_t.get(), 60);
    }

    #[test]
    fn breakdown_attributes_gates_to_scopes_through_repeats() {
        let mut b = CircuitBuilder::new("top");
        b.repeat("outer", 10, |b| {
            b.scope("adder", |b| {
                for _ in 0..3 {
                    b.and_compute();
                }
            });
        });
        let bd = b.finish().breakdown();
        let adder = bd.iter().find(|(n, _)| n == "adder").expect("adder scope");
        assert_eq!(adder.1.and_compute.get(), 30, "3 per iteration x 10 iterations");
    }
}
