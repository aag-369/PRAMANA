"""Crypto-agility scoring.

NIST CSWP 39 asks organisations to *measure* crypto agility, and essentially nobody has
tooled it. The score feeds the migration-time term of Mosca's inequality, so it is not
decoration: a low-agility asset takes longer to migrate and is therefore exposed for longer.

Each dimension is scored 0-100 from evidence, with the reasoning recorded, so a user can
see why an asset scored what it did and what would most cheaply improve it.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

#: Dimension weights. They sum to 1.0.
WEIGHTS: dict[str, float] = {
    "algorithm_negotiability": 0.16,
    "configuration_surface": 0.14,
    "lifecycle_automation": 0.16,
    "dependency_depth": 0.12,
    "library_support": 0.14,
    "hardware_binding": 0.12,
    "vendor_dependency": 0.08,
    "size_headroom": 0.08,
}

#: Score band to migration time in years. Published so a user can override it rather than
#: having to reverse-engineer it from results.
SCORE_TO_MIGRATION_YEARS: list[tuple[int, float]] = [
    (90, 0.25),
    (75, 0.5),
    (60, 1.0),
    (40, 2.0),
    (20, 3.0),
    (0, 5.0),
]


@dataclass
class Dimension:
    """One scored dimension."""

    name: str
    score: float
    weight: float
    evidence: str
    improvement: str | None = None


@dataclass
class AgilityResult:
    """A scored asset."""

    score: float
    migration_years: float
    dimensions: list[Dimension] = field(default_factory=list)

    def cheapest_improvements(self, limit: int = 3) -> list[dict[str, Any]]:
        """Improvements ranked by weighted points recoverable."""
        candidates = [
            {
                "dimension": d.name,
                "current_score": d.score,
                "points_recoverable": round((100.0 - d.score) * d.weight, 2),
                "action": d.improvement,
            }
            for d in self.dimensions
            if d.improvement and d.score < 100
        ]
        candidates.sort(key=lambda c: c["points_recoverable"], reverse=True)
        return candidates[:limit]

    def as_dict(self) -> dict[str, Any]:
        """Serialisable form."""
        return {
            "score": self.score,
            "migration_years": self.migration_years,
            "dimensions": [
                {
                    "name": d.name,
                    "score": d.score,
                    "weight": d.weight,
                    "evidence": d.evidence,
                    "improvement": d.improvement,
                }
                for d in self.dimensions
            ],
        }


def migration_years_for(score: float) -> float:
    """Map an agility score to a migration time in years."""
    for threshold, years in SCORE_TO_MIGRATION_YEARS:
        if score >= threshold:
            return years
    return SCORE_TO_MIGRATION_YEARS[-1][1]


def assess(
    *,
    kind: str,
    algorithm_family: str,
    validity_days: int | None,
    is_ca: bool,
    dependents: int,
    hsm_backed: bool = False,
    third_party_product: bool = False,
    library_supports_pqc: bool | None = None,
    protocol_negotiated: bool | None = None,
    automated_rotation: bool | None = None,
) -> AgilityResult:
    """Score an asset's crypto agility from observable evidence."""
    dims: list[Dimension] = []

    negotiated = protocol_negotiated
    if negotiated is None:
        negotiated = kind in {"tls_cert", "ssh_host_key", "vpn_tunnel"}
    dims.append(
        Dimension(
            "algorithm_negotiability",
            85.0 if negotiated else 25.0,
            WEIGHTS["algorithm_negotiability"],
            "algorithm negotiated at runtime" if negotiated
            else "algorithm fixed at build or deployment time",
            None if negotiated else "move the algorithm choice behind a negotiated protocol "
            "or a configuration value",
        )
    )

    config_score = 30.0 if kind in {"code_signing_cert", "db_tde_key"} else 70.0
    dims.append(
        Dimension(
            "configuration_surface", config_score, WEIGHTS["configuration_surface"],
            f"{kind} typically changes by {'re-architecture' if config_score < 50 else 'configuration'}",
            "externalise the algorithm choice into configuration" if config_score < 50 else None,
        )
    )

    if automated_rotation is None:
        # A short certificate lifetime is evidence that automation exists and works; a
        # 397-day lifetime is evidence of a manual process.
        automated_rotation = validity_days is not None and validity_days <= 100
    lifecycle = 90.0 if automated_rotation else (40.0 if (validity_days or 0) <= 400 else 15.0)
    dims.append(
        Dimension(
            "lifecycle_automation", lifecycle, WEIGHTS["lifecycle_automation"],
            f"validity {validity_days} days" if validity_days else "validity unknown",
            "adopt ACME or an equivalent so rotation is automatic" if lifecycle < 90 else None,
        )
    )

    depth = max(0.0, 100.0 - min(dependents, 20) * 5.0)
    dims.append(
        Dimension(
            "dependency_depth", depth, WEIGHTS["dependency_depth"],
            f"{dependents} dependent asset(s) in the chain graph",
            "reduce the blast radius by re-issuing dependents under a shorter-lived "
            "intermediate" if depth < 60 else None,
        )
    )

    lib = 80.0 if library_supports_pqc else (20.0 if library_supports_pqc is False else 50.0)
    dims.append(
        Dimension(
            "library_support", lib, WEIGHTS["library_support"],
            "deployed library PQC support: "
            + ("yes" if library_supports_pqc else "no" if library_supports_pqc is False else "unknown"),
            "upgrade to a library version with ML-KEM and ML-DSA support" if lib < 80 else None,
        )
    )

    hw = 15.0 if hsm_backed else 85.0
    dims.append(
        Dimension(
            "hardware_binding", hw, WEIGHTS["hardware_binding"],
            "key bound to an HSM or secure element" if hsm_backed else "software-held key",
            "confirm the HSM firmware roadmap supports ML-DSA and ML-KEM; firmware refresh "
            "cycles are the long pole in most enterprises" if hsm_backed else None,
        )
    )

    vendor = 25.0 if third_party_product else 80.0
    dims.append(
        Dimension(
            "vendor_dependency", vendor, WEIGHTS["vendor_dependency"],
            "algorithm controlled by a third-party product" if third_party_product
            else "algorithm under the organisation's control",
            "obtain the vendor's PQC roadmap and a contractual date" if third_party_product else None,
        )
    )

    # ML-DSA-65 signatures are ~3.3 KB against ECDSA P-256's 64 bytes. For some protocols
    # that is fatal, and it is a migration blocker rather than an inconvenience.
    tight = algorithm_family in {"ecdsa", "eddsa"} and kind in {"dnssec_key", "code_signing_cert"}
    size = 35.0 if tight else 75.0
    dims.append(
        Dimension(
            "size_headroom", size, WEIGHTS["size_headroom"],
            "post-quantum signatures are 50x larger than the current ECDSA signature and "
            "this asset class is size-sensitive" if tight
            else "no known size constraint",
            "check MTU, packet size and storage budgets against a 3.3 KB signature; "
            "consider FN-DSA when it is finalised" if tight else None,
        )
    )

    total = sum(d.score * d.weight for d in dims)
    return AgilityResult(
        score=round(total, 1),
        migration_years=migration_years_for(total),
        dimensions=dims,
    )
