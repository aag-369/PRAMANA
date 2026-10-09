//! Vendor roadmaps and verified demonstrations.
//!
//! # Why this fits logical qubits, not physical
//!
//! The obvious model regresses physical qubit count against year. That is wrong, and the
//! 2026 data shows why: the physical-to-logical ratio spans more than an order of
//! magnitude across modalities. Quantinuum reached 48 logical qubits from 98 physical
//! (2:1); Atom Computing needed 1,180 physical for 24 logical (49:1); Google's surface-code
//! demonstration used 105 physical for one logical.
//!
//! A physical-qubit fit would rank Atom Computing's 1,180-qubit machine above QuEra's
//! 448-qubit machine, while QuEra delivers four times the logical qubits. PRAMANA therefore
//! fits **verified logical qubits** and carries the physical-to-logical ratio as a separate
//! modality-specific quantity.
//!
//! # Announced is not delivered
//!
//! Milestones carry a [`MilestoneStatus`]. Only `Delivered` entries with `verified = true`
//! enter the capability fit. `Announced` entries feed the roadmap-target comparison, and
//! `Abandoned` entries exist so the slip prior can treat them as right-censored rather than
//! silently dropping them, which would bias the prior optimistic.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Physical qubit technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    /// Superconducting transmons.
    Superconducting,
    /// Trapped ions.
    TrappedIon,
    /// Neutral atoms in optical tweezers.
    NeutralAtom,
    /// Photonic.
    Photonic,
    /// Bosonic / cat qubits.
    Bosonic,
    /// Spin qubits in silicon.
    Silicon,
}

impl Modality {
    /// Display name.
    pub fn name(&self) -> &'static str {
        match self {
            Modality::Superconducting => "superconducting",
            Modality::TrappedIon => "trapped ion",
            Modality::NeutralAtom => "neutral atom",
            Modality::Photonic => "photonic",
            Modality::Bosonic => "bosonic",
            Modality::Silicon => "silicon spin",
        }
    }
}

/// Whether a milestone happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneStatus {
    /// Shipped, with the year recorded in `year`.
    Delivered,
    /// Publicly targeted for `year`, not yet shipped.
    Announced,
    /// Publicly targeted and then dropped or silently abandoned.
    ///
    /// These must be retained. A slip prior fitted only on delivered milestones sees no
    /// failures at all and is therefore biased optimistic; abandoned milestones are
    /// right-censored observations of unbounded slip.
    Abandoned,
}

/// A single roadmap entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// Organisation.
    pub vendor: String,
    /// Technology.
    pub modality: Modality,
    /// System or milestone name.
    pub name: String,
    /// Delivery year, or target year if not yet delivered.
    pub year: u32,
    /// Year this milestone was first publicly announced, for the slip fit.
    #[serde(default)]
    pub announced_in: Option<u32>,
    /// Logical qubits, where the milestone reports them.
    #[serde(default)]
    pub logical_qubits: Option<u64>,
    /// Physical qubits, where the milestone reports them.
    #[serde(default)]
    pub physical_qubits: Option<u64>,
    /// Whether the logical qubit count was independently verified rather than announced.
    #[serde(default)]
    pub verified: bool,
    /// Status.
    pub status: MilestoneStatus,
    /// Source for the claim.
    pub citation: String,
    /// Free-text note.
    #[serde(default)]
    pub note: Option<String>,
}

impl Milestone {
    /// Physical qubits consumed per logical qubit, where both are known.
    pub fn physical_per_logical(&self) -> Option<f64> {
        match (self.physical_qubits, self.logical_qubits) {
            (Some(p), Some(l)) if l > 0 => Some(p as f64 / l as f64),
            _ => None,
        }
    }

    /// Observed slip in years, where the milestone was delivered and its announcement year
    /// is known.
    pub fn slip_years(&self) -> Option<i64> {
        match (self.status, self.announced_in) {
            (MilestoneStatus::Delivered, Some(a)) => Some(self.year as i64 - a as i64),
            _ => None,
        }
    }

    /// Whether this entry is admissible into the capability fit.
    ///
    /// Requires a delivered, independently verified logical qubit count. Announced targets
    /// and unverified press-release counts are excluded (Law 9: no silent substitution).
    pub fn is_fit_datum(&self) -> bool {
        self.status == MilestoneStatus::Delivered && self.verified && self.logical_qubits.is_some()
    }
}

/// A collection of milestones.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Roadmaps {
    /// All entries.
    #[serde(default, rename = "milestone")]
    pub milestones: Vec<Milestone>,
}

/// Errors loading roadmap data.
#[derive(Debug, Error)]
pub enum RoadmapError {
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// Path attempted.
        path: String,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The file could not be parsed.
    #[error("cannot parse {path}: {message}")]
    Parse {
        /// Path attempted.
        path: String,
        /// Parser message.
        message: String,
    },
}

impl Roadmaps {
    /// Load every `.toml` file in a directory, sorted for determinism.
    pub fn load_dir(dir: &std::path::Path) -> Result<Self, RoadmapError> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| RoadmapError::Io {
                path: dir.display().to_string(),
                source: e,
            })?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
            .collect();
        paths.sort();

        let mut all = Vec::new();
        for p in paths {
            let text = std::fs::read_to_string(&p).map_err(|e| RoadmapError::Io {
                path: p.display().to_string(),
                source: e,
            })?;
            let r: Roadmaps = toml::from_str(&text).map_err(|e| RoadmapError::Parse {
                path: p.display().to_string(),
                message: e.to_string(),
            })?;
            all.extend(r.milestones);
        }
        Ok(Roadmaps { milestones: all })
    }

    /// Entries admissible into the capability fit.
    pub fn fit_data(&self) -> Vec<&Milestone> {
        self.milestones.iter().filter(|m| m.is_fit_datum()).collect()
    }

    /// Announced but undelivered targets, for the roadmap-versus-history comparison.
    pub fn announced(&self) -> Vec<&Milestone> {
        self.milestones
            .iter()
            .filter(|m| m.status == MilestoneStatus::Announced)
            .collect()
    }

    /// Observed slips, plus a count of abandoned milestones as right-censored data.
    pub fn slips(&self) -> (Vec<i64>, usize) {
        let observed: Vec<i64> = self.milestones.iter().filter_map(|m| m.slip_years()).collect();
        let censored = self
            .milestones
            .iter()
            .filter(|m| m.status == MilestoneStatus::Abandoned)
            .count();
        (observed, censored)
    }

    /// Physical-to-logical ratios observed for a modality.
    pub fn ratios_for(&self, modality: Modality) -> Vec<f64> {
        self.milestones
            .iter()
            .filter(|m| m.modality == modality && m.is_fit_datum())
            .filter_map(|m| m.physical_per_logical())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(verified: bool, status: MilestoneStatus, lq: Option<u64>) -> Milestone {
        Milestone {
            vendor: "v".into(),
            modality: Modality::NeutralAtom,
            name: "n".into(),
            year: 2026,
            announced_in: Some(2024),
            logical_qubits: lq,
            physical_qubits: Some(448),
            verified,
            status,
            citation: "c".into(),
            note: None,
        }
    }

    #[test]
    fn only_verified_delivered_entries_enter_the_fit() {
        assert!(m(true, MilestoneStatus::Delivered, Some(96)).is_fit_datum());
        assert!(!m(false, MilestoneStatus::Delivered, Some(96)).is_fit_datum());
        assert!(!m(true, MilestoneStatus::Announced, Some(96)).is_fit_datum());
        assert!(!m(true, MilestoneStatus::Delivered, None).is_fit_datum());
    }

    #[test]
    fn physical_to_logical_ratio_is_computed_where_both_are_known() {
        let x = m(true, MilestoneStatus::Delivered, Some(96));
        let r = x.physical_per_logical().unwrap();
        assert!((r - 448.0 / 96.0).abs() < 1e-9);
    }

    #[test]
    fn slip_is_only_defined_for_delivered_milestones() {
        assert_eq!(m(true, MilestoneStatus::Delivered, Some(1)).slip_years(), Some(2));
        assert_eq!(m(true, MilestoneStatus::Announced, Some(1)).slip_years(), None);
        assert_eq!(m(true, MilestoneStatus::Abandoned, Some(1)).slip_years(), None);
    }

    #[test]
    fn abandoned_milestones_are_counted_as_censored_not_dropped() {
        let r = Roadmaps {
            milestones: vec![
                m(true, MilestoneStatus::Delivered, Some(1)),
                m(true, MilestoneStatus::Abandoned, Some(1)),
                m(true, MilestoneStatus::Abandoned, Some(1)),
            ],
        };
        let (observed, censored) = r.slips();
        assert_eq!(observed.len(), 1);
        assert_eq!(censored, 2, "abandoned milestones must survive as censored data");
    }
}
