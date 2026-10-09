"""Request and response models."""

from __future__ import annotations

import datetime as dt
from typing import Any, Literal

from pydantic import BaseModel, Field

Criticality = Literal["low", "medium", "high", "critical"]
DataClass = Literal["public", "internal", "confidential", "restricted"]
ThreatMode = Literal["confidentiality", "authenticity", "both"]


class InventoryCreate(BaseModel):
    """Create an inventory."""

    name: str
    organisation: str = ""


class InventoryOut(BaseModel):
    """An inventory."""

    id: str
    name: str
    organisation: str
    asset_count: int = 0


class AssetOut(BaseModel):
    """An asset as returned by the API."""

    id: str
    fingerprint: str
    kind: str
    algorithm_family: str
    key_bits: int
    curve: str | None = None
    purpose: str
    threat_mode: str
    subject: str | None = None
    issuer: str | None = None
    not_after: dt.datetime | None = None
    is_ca: bool = False
    secrecy_lifetime_years: float | None = None
    criticality: Criticality | None = None
    data_class: DataClass | None = None
    indeterminate_reason: str | None = None


class AssetPatch(BaseModel):
    """Fields a user may set on an asset.

    ``secrecy_lifetime_years`` has no default on purpose. Mosca's inequality cannot be
    resolved without it, and inventing a value would be a silent substitution (Law 9).
    """

    secrecy_lifetime_years: float | None = Field(default=None, ge=0)
    migration_years_override: float | None = Field(default=None, ge=0)
    criticality: Criticality | None = None
    data_class: DataClass | None = None
    threat_mode: ThreatMode | None = None
    owner: str | None = None
    environment: str | None = None


class IngestResult(BaseModel):
    """Outcome of an ingestion."""

    ingested: int
    duplicates: int
    indeterminate: int
    rejected: list[str] = Field(default_factory=list)
    asset_ids: list[str] = Field(default_factory=list)


class BreakYear(BaseModel):
    """Quantiles of the break-year distribution."""

    p05: int | None = None
    p10: int | None = None
    p25: int | None = None
    p50: int | None = None
    p75: int | None = None
    p90: int | None = None
    p95: int | None = None


class ExposureOut(BaseModel):
    """A computed exposure."""

    asset_id: str
    kind: str
    algorithm: str
    pipeline: str
    logical_qubits: int
    break_year: BreakYear
    break_year_by_architecture: dict[str, int | None] = Field(default_factory=dict)
    deadline_year: float
    probability_exposed: float
    harvest_now_decrypt_later_year: int | None = None
    exposure_score: float
    exposure_lower: float
    exposure_upper: float
    exposure_formula: str
    warnings: list[str] = Field(default_factory=list)
    provenance: dict[str, Any] = Field(default_factory=dict)


class EstimationRequest(BaseModel):
    """Run an estimation over an inventory."""

    samples: int | None = Field(default=None, ge=100, le=100_000)
    attacker: Literal["nation_state", "opportunistic"] = "nation_state"


class EstimationSummary(BaseModel):
    """Outcome of an estimation run."""

    assessed: int
    skipped_indeterminate: int
    skipped_missing_inputs: list[str] = Field(default_factory=list)
    duration_seconds: float


class DivergenceOut(BaseModel):
    """Criticality ranking against computed-exposure ranking.

    This is the empirical content of the project's novelty claim: prioritisations derived
    from asset criticality alone diverge from those derived from computed attack cost.
    """

    kendall_tau: float
    spearman_rho: float
    n: int
    biggest_movers: list[dict[str, Any]] = Field(default_factory=list)
    interpretation: str


class TimelinePoint(BaseModel):
    """One asset's row in the exposure timeline."""

    asset_id: str
    label: str
    exposure_score: float
    break_year: BreakYear
    deadline_year: float
    not_after_year: int | None = None
    threat_mode: str
