"""Database models.

Written to be portable between SQLite (development, tests) and PostgreSQL (production):
no server-specific column types, JSON rather than JSONB, and UUIDs stored as strings.
"""

from __future__ import annotations

import datetime as dt
import uuid

from sqlalchemy import (
    JSON,
    Boolean,
    DateTime,
    Float,
    ForeignKey,
    Integer,
    String,
    Text,
)
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


def _uuid() -> str:
    return str(uuid.uuid4())


def _now() -> dt.datetime:
    return dt.datetime.now(dt.timezone.utc)


class Base(DeclarativeBase):
    """Declarative base."""


class Inventory(Base):
    """A collection of cryptographic assets belonging to one organisation."""

    __tablename__ = "inventories"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=_uuid)
    name: Mapped[str] = mapped_column(String(255))
    organisation: Mapped[str] = mapped_column(String(255), default="")
    created_at: Mapped[dt.datetime] = mapped_column(DateTime, default=_now)

    assets: Mapped[list["Asset"]] = relationship(
        back_populates="inventory", cascade="all, delete-orphan"
    )


class Asset(Base):
    """One cryptographic asset.

    ``fingerprint`` is the deduplication key: the same public key deployed in two places is
    two assets, because their deployment context and therefore their migration cost differ.
    """

    __tablename__ = "assets"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=_uuid)
    inventory_id: Mapped[str] = mapped_column(ForeignKey("inventories.id"), index=True)
    fingerprint: Mapped[str] = mapped_column(String(128), index=True)

    kind: Mapped[str] = mapped_column(String(48))
    source: Mapped[str] = mapped_column(String(64), default="upload")

    algorithm_family: Mapped[str] = mapped_column(String(32), index=True)
    key_bits: Mapped[int] = mapped_column(Integer)
    curve: Mapped[str | None] = mapped_column(String(48), nullable=True)
    signature_algorithm: Mapped[str | None] = mapped_column(String(96), nullable=True)

    purpose: Mapped[str] = mapped_column(String(32), default="signature")
    threat_mode: Mapped[str] = mapped_column(String(24), default="confidentiality")

    subject: Mapped[str | None] = mapped_column(Text, nullable=True)
    issuer: Mapped[str | None] = mapped_column(Text, nullable=True)
    not_before: Mapped[dt.datetime | None] = mapped_column(DateTime, nullable=True)
    not_after: Mapped[dt.datetime | None] = mapped_column(DateTime, nullable=True)
    is_ca: Mapped[bool] = mapped_column(Boolean, default=False)

    #: Set by a human or a policy. Absent means the asset is INDETERMINATE for Mosca
    #: purposes and PRAMANA refuses to invent a value (Law 9).
    secrecy_lifetime_years: Mapped[float | None] = mapped_column(Float, nullable=True)
    migration_years_override: Mapped[float | None] = mapped_column(Float, nullable=True)
    criticality: Mapped[str | None] = mapped_column(String(16), nullable=True)
    data_class: Mapped[str | None] = mapped_column(String(16), nullable=True)

    environment: Mapped[str | None] = mapped_column(String(48), nullable=True)
    host: Mapped[str | None] = mapped_column(String(255), nullable=True)
    owner: Mapped[str | None] = mapped_column(String(255), nullable=True)

    raw: Mapped[dict] = mapped_column(JSON, default=dict)
    created_at: Mapped[dt.datetime] = mapped_column(DateTime, default=_now)

    inventory: Mapped[Inventory] = relationship(back_populates="assets")
    exposure: Mapped["Exposure | None"] = relationship(
        back_populates="asset", cascade="all, delete-orphan", uselist=False
    )
    agility: Mapped["AgilityAssessment | None"] = relationship(
        back_populates="asset", cascade="all, delete-orphan", uselist=False
    )


class AssetEdge(Base):
    """A certificate chain edge: ``child`` was issued by ``parent``.

    Kept as an explicit table rather than a parent pointer so the graph can be queried in
    both directions, which is what "show me everything depending on this root" needs.
    """

    __tablename__ = "asset_edges"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=_uuid)
    parent_id: Mapped[str] = mapped_column(ForeignKey("assets.id"), index=True)
    child_id: Mapped[str] = mapped_column(ForeignKey("assets.id"), index=True)


class AgilityAssessment(Base):
    """Crypto-agility score for one asset."""

    __tablename__ = "agility_assessments"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=_uuid)
    asset_id: Mapped[str] = mapped_column(ForeignKey("assets.id"), index=True, unique=True)
    score: Mapped[float] = mapped_column(Float)
    migration_years: Mapped[float] = mapped_column(Float)
    dimensions: Mapped[dict] = mapped_column(JSON, default=dict)
    cheapest_improvements: Mapped[list] = mapped_column(JSON, default=list)

    asset: Mapped[Asset] = relationship(back_populates="agility")


class Exposure(Base):
    """A computed exposure result.

    Every row carries the seed, sample count and core version so a result can be reproduced
    exactly (Law 5) and its provenance chain read back (Law 2).
    """

    __tablename__ = "exposures"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=_uuid)
    asset_id: Mapped[str] = mapped_column(ForeignKey("assets.id"), index=True, unique=True)

    pipeline: Mapped[str] = mapped_column(String(64))
    logical_qubits: Mapped[int] = mapped_column(Integer)
    toffoli_all_runs: Mapped[float] = mapped_column(Float)

    break_year_p05: Mapped[int | None] = mapped_column(Integer, nullable=True)
    break_year_p50: Mapped[int | None] = mapped_column(Integer, nullable=True)
    break_year_p95: Mapped[int | None] = mapped_column(Integer, nullable=True)
    break_year_by_architecture: Mapped[dict] = mapped_column(JSON, default=dict)
    #: Counts per break year. The timeline renders a density, not an error bar; quantiles
    #: alone would collapse the distribution back into a point estimate.
    break_year_histogram: Mapped[dict] = mapped_column(JSON, default=dict)

    deadline_year: Mapped[float] = mapped_column(Float)
    probability_exposed: Mapped[float] = mapped_column(Float)
    harvest_now_decrypt_later_year: Mapped[int | None] = mapped_column(Integer, nullable=True)

    exposure_score: Mapped[float] = mapped_column(Float, index=True)
    exposure_lower: Mapped[float] = mapped_column(Float)
    exposure_upper: Mapped[float] = mapped_column(Float)
    exposure_formula: Mapped[str] = mapped_column(Text)

    warnings: Mapped[list] = mapped_column(JSON, default=list)
    provenance: Mapped[dict] = mapped_column(JSON, default=dict)
    core_version: Mapped[str] = mapped_column(String(32), default="")
    computed_at: Mapped[dt.datetime] = mapped_column(DateTime, default=_now)

    asset: Mapped[Asset] = relationship(back_populates="exposure")


class Recommendation(Base):
    """A migration recommendation for one asset."""

    __tablename__ = "recommendations"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=_uuid)
    asset_id: Mapped[str] = mapped_column(ForeignKey("assets.id"), index=True, unique=True)
    target_algorithm: Mapped[str] = mapped_column(String(64))
    parameter_set: Mapped[str] = mapped_column(String(64))
    hybrid: Mapped[bool] = mapped_column(Boolean, default=False)
    status: Mapped[str] = mapped_column(String(24), default="available")
    rationale: Mapped[str] = mapped_column(Text)
    citations: Mapped[list] = mapped_column(JSON, default=list)
    deadlines: Mapped[list] = mapped_column(JSON, default=list)
    size_impact: Mapped[dict] = mapped_column(JSON, default=dict)
