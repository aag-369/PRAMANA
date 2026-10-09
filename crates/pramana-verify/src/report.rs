//! Report rendering for the reproduction harness.

use crate::golden::{Quantity, Target};
use serde::Serialize;

/// Outcome of evaluating one golden target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every declared quantity was reproduced within tolerance.
    Pass,
    /// At least one quantity was outside tolerance.
    Fail,
    /// The machinery needed for this target is not built yet.
    NotImplemented,
    /// Explicitly waived with a stated reason.
    Waived,
    /// A sensitivity probe that failed as designed. Reported, but not a regression.
    ProbeConfirmed,
    /// A sensitivity probe that unexpectedly passed, meaning what it probes has moved.
    ProbeUnexpectedlyPassed,
    /// The harness could not evaluate the target at all.
    Error,
}

impl Outcome {
    fn symbol(&self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Fail => "FAIL",
            Outcome::NotImplemented => "PENDING",
            Outcome::Waived => "WAIVED",
            Outcome::ProbeConfirmed => "PROBE",
            Outcome::ProbeUnexpectedlyPassed => "PROBE-MOVED",
            Outcome::Error => "ERROR",
        }
    }
}

/// Comparison of one computed quantity against its published value.
#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    /// Quantity name.
    pub name: String,
    /// What PRAMANA computed.
    pub computed: f64,
    /// What the source published.
    pub published: f64,
    /// computed / published.
    pub ratio: f64,
    /// Whether the comparison passed.
    pub passed: bool,
    /// How the comparison was made.
    pub criterion: String,
}

impl Comparison {
    /// Evaluate a computed value against a published quantity.
    pub fn evaluate(name: &str, computed: f64, q: &Quantity) -> Self {
        let published = q.value;
        let ratio = if published != 0.0 {
            computed / published
        } else {
            f64::NAN
        };
        let (passed, criterion) = match q.comparator.as_deref() {
            Some("less_than") => (computed < published, "computed < published".to_string()),
            Some("greater_than") => (computed > published, "computed > published".to_string()),
            _ => {
                let tol = q.tolerance_pct.unwrap_or(10.0);
                let rel = ((computed - published) / published).abs() * 100.0;
                (rel <= tol, format!("within {tol:.0}% (actual {rel:.1}%)"))
            }
        };
        Comparison {
            name: name.to_string(),
            computed,
            published,
            ratio,
            passed,
            criterion,
        }
    }
}

/// Report for one golden target.
#[derive(Debug, Clone, Serialize)]
pub struct TargetReport {
    /// Target identifier.
    pub id: String,
    /// Pipeline that should reproduce it.
    pub circuit: String,
    /// Source citation, rendered.
    pub source: String,
    /// Outcome.
    pub outcome: Outcome,
    /// Per-quantity comparisons.
    pub comparisons: Vec<Comparison>,
    /// Diagnosis, for anything that is not a clean pass.
    pub diagnosis: String,
}

impl TargetReport {
    /// Start a report for a target.
    pub fn new(t: &Target) -> Self {
        let cite = t
            .source
            .arxiv
            .as_ref()
            .map(|a| format!("arXiv:{a}"))
            .or_else(|| t.source.eprint.as_ref().map(|e| format!("ePrint {e}")))
            .unwrap_or_else(|| "no identifier".into());
        TargetReport {
            id: t.id.clone(),
            circuit: t.circuit.clone(),
            source: format!("{} ({}, {})", t.source.title, cite, t.source.year),
            outcome: Outcome::Error,
            comparisons: Vec::new(),
            diagnosis: String::new(),
        }
    }

    /// Set the outcome from the accumulated comparisons.
    pub fn finalise(&mut self) {
        self.outcome = if self.comparisons.iter().all(|c| c.passed) {
            Outcome::Pass
        } else {
            Outcome::Fail
        };
    }

    /// Reinterpret the outcome for a target declared as a sensitivity probe.
    pub fn finalise_probe(&mut self) {
        self.outcome = if self.comparisons.iter().all(|c| c.passed) {
            Outcome::ProbeUnexpectedlyPassed
        } else {
            Outcome::ProbeConfirmed
        };
    }
}

/// Render the full report as Markdown.
pub fn render_markdown(reports: &[TargetReport]) -> String {
    let mut s = String::new();
    s.push_str("# PRAMANA reproduction report\n\n");
    s.push_str(
        "Every figure below was produced by running PRAMANA's production estimation path.\n\
         Published values live only in `verification/golden/` and are never consulted by\n\
         that path (Law 1). Divergences are diagnosed, not tuned away (spec §25).\n\n",
    );

    let n_pass = reports.iter().filter(|r| r.outcome == Outcome::Pass).count();
    let n_fail = reports.iter().filter(|r| r.outcome == Outcome::Fail).count();
    let n_pend = reports
        .iter()
        .filter(|r| r.outcome == Outcome::NotImplemented)
        .count();
    let n_probe = reports
        .iter()
        .filter(|r| {
            matches!(
                r.outcome,
                Outcome::ProbeConfirmed | Outcome::ProbeUnexpectedlyPassed
            )
        })
        .count();
    s.push_str(&format!(
        "**{} passed, {} failed, {} probes, {} pending, {} targets**\n\n",
        n_pass,
        n_fail,
        n_probe,
        n_pend,
        reports.len()
    ));

    s.push_str("| Target | Quantity | Computed | Published | Ratio | Criterion | Result |\n");
    s.push_str("|---|---|---:|---:|---:|---|---|\n");
    for r in reports {
        if r.comparisons.is_empty() {
            s.push_str(&format!(
                "| `{}` | - | - | - | - | - | {} |\n",
                r.id,
                r.outcome.symbol()
            ));
        }
        for c in &r.comparisons {
            s.push_str(&format!(
                "| `{}` | {} | {:.4e} | {:.4e} | {:.3} | {} | {} |\n",
                r.id,
                c.name,
                c.computed,
                c.published,
                c.ratio,
                c.criterion,
                if c.passed { "PASS" } else { "FAIL" }
            ));
        }
    }

    s.push_str("\n## Sources\n\n");
    for r in reports {
        s.push_str(&format!("- `{}` - {}\n", r.id, r.source));
    }

    s.push_str("\n## Diagnoses\n\n");
    for r in reports {
        if !r.diagnosis.is_empty() {
            s.push_str(&format!(
                "### `{}` ({})\n\n{}\n\n",
                r.id,
                r.outcome.symbol(),
                r.diagnosis
            ));
        }
    }
    s
}
