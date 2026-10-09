//! Parsing of the golden files: published figures and their provenance.
//!
//! These files are the *only* place in the repository where published results appear,
//! and they are read exclusively by this harness (spec Law 1).

use serde::Deserialize;

/// One published figure to reproduce.
#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    /// Stable identifier.
    pub id: String,
    /// Identifier of the synthesis pipeline that should reproduce it.
    pub circuit: String,
    /// Which layer of the stack this figure belongs to.
    pub layer: Layer,
    /// Where the figure comes from.
    pub source: Source,
    /// The problem instance.
    pub problem: Problem,
    /// Hardware assumptions, for physical-layer targets.
    #[serde(default)]
    pub hardware: Option<Hardware>,
    /// The figures themselves.
    pub expected: Expected,
    /// Architecture identifier, for physical-layer targets.
    #[serde(default)]
    pub architecture: Option<String>,
    /// Free-text notes.
    #[serde(default)]
    pub notes: Option<Notes>,
    /// This target is a sensitivity probe that is *designed* to fail.
    ///
    /// Distinct from a waiver. A waiver says "we know this is broken and are ignoring
    /// it"; an expected failure says "the failure is the measurement". Probes are
    /// reported prominently but do not break the build, and a probe that unexpectedly
    /// *passes* is itself flagged, since that means the thing it was probing has moved.
    #[serde(default)]
    pub expected_failure: bool,
    /// Explicit waiver.
    #[serde(default)]
    pub waived: bool,
    /// Reason for the waiver.
    #[serde(default)]
    pub waiver_reason: Option<String>,
}

/// Which layer of the estimation stack a figure belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// Abstract circuit model: logical qubits and gate counts.
    Logical,
    /// Physical resources: qubits, wall clock, spacetime volume.
    Physical,
}

/// Provenance of a published figure.
#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    /// arXiv identifier, if any.
    #[serde(default)]
    pub arxiv: Option<String>,
    /// IACR ePrint identifier, if any.
    #[serde(default)]
    pub eprint: Option<String>,
    /// Title.
    pub title: String,
    /// Journal reference, if any.
    #[serde(default)]
    pub journal: Option<String>,
    /// Year.
    pub year: u32,
    /// Where in the source the figure appears.
    pub locus: String,
    /// Date the figure was checked against the source.
    pub retrieved: String,
}

/// The problem instance a figure refers to.
#[derive(Debug, Clone, Deserialize)]
pub struct Problem {
    /// Problem family.
    pub kind: String,
    /// RSA modulus width.
    #[serde(default)]
    pub modulus_bits: Option<u32>,
    /// Elliptic curve width.
    #[serde(default)]
    pub curve_bits: Option<u32>,
    /// Symmetric key width.
    #[serde(default)]
    pub key_bits: Option<u32>,
}

/// Hardware assumptions attached to a physical-layer figure.
#[derive(Debug, Clone, Deserialize)]
pub struct Hardware {
    /// Physical gate error rate.
    pub physical_error_rate: f64,
    /// Surface code cycle time in microseconds.
    pub cycle_time_us: f64,
    /// Control system reaction time in microseconds.
    pub reaction_time_us: f64,
    /// Connectivity model.
    pub connectivity: String,
}

/// A single expected quantity.
#[derive(Debug, Clone, Deserialize)]
pub struct Quantity {
    /// The published value.
    pub value: f64,
    /// Allowed relative deviation, in percent.
    #[serde(default)]
    pub tolerance_pct: Option<f64>,
    /// Comparator, for one-sided bounds such as "less_than".
    #[serde(default)]
    pub comparator: Option<String>,
    /// The closed form the paper gives, if any.
    #[serde(default)]
    pub formula: Option<String>,
}

/// The set of expected quantities for a target.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Expected {
    /// Logical qubits.
    #[serde(default)]
    pub logical_qubits: Option<Quantity>,
    /// Toffoli gate count.
    #[serde(default)]
    pub toffoli_count: Option<Quantity>,
    /// Physical qubits.
    #[serde(default)]
    pub physical_qubits: Option<Quantity>,
    /// Wall clock in days.
    #[serde(default)]
    pub wall_clock_days: Option<Quantity>,
    /// Wall clock in hours.
    #[serde(default)]
    pub wall_clock_hours: Option<Quantity>,
    /// Leading coefficient of the dominant asymptotic term.
    ///
    /// Comparing leading coefficients separates a structurally correct construction from
    /// one that merely lands near the right absolute number.
    #[serde(default)]
    pub leading_coefficient: Option<Quantity>,
}

/// Free-text notes on a target.
#[derive(Debug, Clone, Deserialize)]
pub struct Notes {
    /// The note.
    pub text: String,
}

/// A parsed golden file.
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenFile {
    /// Targets declared in the file.
    #[serde(default, rename = "target")]
    pub targets: Vec<Target>,
}

/// Load every golden file in a directory, sorted by filename for determinism.
pub fn load_dir(dir: &std::path::Path) -> Result<Vec<Target>, String> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
        .collect();
    paths.sort();

    let mut out = Vec::new();
    for p in paths {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let f: GoldenFile =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        out.extend(f.targets);
    }
    Ok(out)
}
