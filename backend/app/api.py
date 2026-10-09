"""HTTP API."""

from __future__ import annotations

import datetime as dt
import time
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Response, UploadFile, status
from sqlalchemy import select
from sqlalchemy.orm import Session

from .config import SETTINGS
from .core import CoreUnavailable, core, core_version
from .db import get_session
from .models import (
    AgilityAssessment,
    Asset,
    AssetEdge,
    Exposure,
    Inventory,
    Recommendation,
)
from .schemas import (
    AssetOut,
    AssetPatch,
    DivergenceOut,
    EstimationRequest,
    EstimationSummary,
    ExposureOut,
    IngestResult,
    InventoryCreate,
    InventoryOut,
    TimelinePoint,
)
from .services import agility as agility_service
from .services import divergence as divergence_service
from .services import recommend as recommend_service
from .services import timeline_svg
from .services.x509_ingest import (
    PrivateKeyRejected,
    UnparseableCertificate,
    parse_bundle,
)

router = APIRouter(prefix="/api/v1")


@router.get("/healthz", tags=["ops"])
def healthz() -> dict[str, Any]:
    """Liveness, plus whether the estimation core loaded."""
    try:
        return {"status": "ok", "core_version": core_version()}
    except CoreUnavailable as exc:
        return {"status": "degraded", "detail": str(exc)}


@router.post("/inventories", response_model=InventoryOut, tags=["inventory"])
def create_inventory(
    body: InventoryCreate, db: Session = Depends(get_session)
) -> InventoryOut:
    """Create an inventory."""
    inv = Inventory(name=body.name, organisation=body.organisation)
    db.add(inv)
    db.commit()
    return InventoryOut(id=inv.id, name=inv.name, organisation=inv.organisation)


@router.get("/inventories", response_model=list[InventoryOut], tags=["inventory"])
def list_inventories(db: Session = Depends(get_session)) -> list[InventoryOut]:
    """List inventories."""
    out = []
    for inv in db.scalars(select(Inventory)).all():
        count = len(inv.assets)
        out.append(
            InventoryOut(
                id=inv.id, name=inv.name, organisation=inv.organisation, asset_count=count
            )
        )
    return out


@router.post(
    "/inventories/{inventory_id}/ingest",
    response_model=IngestResult,
    tags=["inventory"],
)
async def ingest(
    inventory_id: str,
    files: list[UploadFile],
    db: Session = Depends(get_session),
) -> IngestResult:
    """Ingest certificates.

    Accepts PEM bundles and DER certificates. Private key material is rejected outright.
    """
    inv = db.get(Inventory, inventory_id)
    if inv is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, "inventory not found")

    ingested = duplicates = indeterminate = 0
    rejected: list[str] = []
    asset_ids: list[str] = []
    by_subject: dict[str, str] = {}
    pending_edges: list[tuple[str, str]] = []

    for upload in files:
        data = await upload.read()
        try:
            parsed_list = parse_bundle(data)
        except PrivateKeyRejected as exc:
            rejected.append(f"{upload.filename}: {exc}")
            continue
        except UnparseableCertificate as exc:
            rejected.append(f"{upload.filename}: not a certificate ({exc})")
            continue

        for p in parsed_list:
            existing = db.scalar(
                select(Asset).where(
                    Asset.inventory_id == inventory_id,
                    Asset.fingerprint == p.fingerprint,
                )
            )
            if existing is not None:
                duplicates += 1
                by_subject[p.subject] = existing.id
                continue

            raw = dict(p.raw)
            if p.indeterminate_reason:
                raw["indeterminate_reason"] = p.indeterminate_reason
                indeterminate += 1

            asset = Asset(
                inventory_id=inventory_id,
                fingerprint=p.fingerprint,
                kind=p.kind,
                source=f"upload:{upload.filename}",
                algorithm_family=p.algorithm_family,
                key_bits=p.key_bits,
                curve=p.curve,
                signature_algorithm=p.signature_algorithm,
                purpose=p.purpose,
                threat_mode=p.threat_mode,
                subject=p.subject,
                issuer=p.issuer,
                not_before=p.not_before.replace(tzinfo=None) if p.not_before else None,
                not_after=p.not_after.replace(tzinfo=None) if p.not_after else None,
                is_ca=p.is_ca,
                raw=raw,
            )
            db.add(asset)
            db.flush()
            ingested += 1
            asset_ids.append(asset.id)
            by_subject[p.subject] = asset.id
            if p.issuer != p.subject:
                pending_edges.append((p.issuer, asset.id))

    # Build the chain graph for issuers present in this inventory.
    for issuer_subject, child_id in pending_edges:
        parent_id = by_subject.get(issuer_subject)
        if parent_id is None:
            parent = db.scalar(
                select(Asset).where(
                    Asset.inventory_id == inventory_id,
                    Asset.subject == issuer_subject,
                    Asset.is_ca.is_(True),
                )
            )
            parent_id = parent.id if parent else None
        if parent_id and parent_id != child_id:
            db.add(AssetEdge(parent_id=parent_id, child_id=child_id))

    db.commit()
    return IngestResult(
        ingested=ingested,
        duplicates=duplicates,
        indeterminate=indeterminate,
        rejected=rejected,
        asset_ids=asset_ids,
    )


def _asset_out(a: Asset) -> AssetOut:
    return AssetOut(
        id=a.id,
        fingerprint=a.fingerprint,
        kind=a.kind,
        algorithm_family=a.algorithm_family,
        key_bits=a.key_bits,
        curve=a.curve,
        purpose=a.purpose,
        threat_mode=a.threat_mode,
        subject=a.subject,
        issuer=a.issuer,
        not_after=a.not_after,
        is_ca=a.is_ca,
        secrecy_lifetime_years=a.secrecy_lifetime_years,
        criticality=a.criticality,
        data_class=a.data_class,
        indeterminate_reason=a.raw.get("indeterminate_reason"),
    )


@router.get(
    "/inventories/{inventory_id}/assets",
    response_model=list[AssetOut],
    tags=["inventory"],
)
def list_assets(
    inventory_id: str,
    kind: str | None = None,
    algorithm_family: str | None = None,
    db: Session = Depends(get_session),
) -> list[AssetOut]:
    """List assets in an inventory."""
    q = select(Asset).where(Asset.inventory_id == inventory_id)
    if kind:
        q = q.where(Asset.kind == kind)
    if algorithm_family:
        q = q.where(Asset.algorithm_family == algorithm_family)
    return [_asset_out(a) for a in db.scalars(q).all()]


@router.patch("/assets/{asset_id}", response_model=AssetOut, tags=["inventory"])
def patch_asset(
    asset_id: str, body: AssetPatch, db: Session = Depends(get_session)
) -> AssetOut:
    """Set the human-supplied fields Mosca resolution needs."""
    a = db.get(Asset, asset_id)
    if a is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, "asset not found")
    for field_name, value in body.model_dump(exclude_unset=True).items():
        if value is not None:
            setattr(a, field_name, value)
    db.commit()
    return _asset_out(a)


def _dependents(db: Session, asset_id: str) -> int:
    return len(db.scalars(select(AssetEdge).where(AssetEdge.parent_id == asset_id)).all())


@router.post(
    "/inventories/{inventory_id}/estimate",
    response_model=EstimationSummary,
    tags=["estimation"],
)
def estimate(
    inventory_id: str,
    body: EstimationRequest,
    db: Session = Depends(get_session),
) -> EstimationSummary:
    """Assess every determinate asset in an inventory.

    Synchronous. A Celery worker fronts this in production; the computation is identical.
    """
    started = time.monotonic()
    try:
        pramana = core()
    except CoreUnavailable as exc:
        raise HTTPException(status.HTTP_503_SERVICE_UNAVAILABLE, str(exc)) from exc

    assets = db.scalars(select(Asset).where(Asset.inventory_id == inventory_id)).all()
    if not assets:
        raise HTTPException(status.HTTP_404_NOT_FOUND, "inventory has no assets")

    samples = body.samples or SETTINGS.default_samples
    assessed = 0
    skipped_indeterminate = 0
    skipped_missing: list[str] = []

    family_to_kind = {"rsa": "rsa", "ecdsa": "ecdlp", "ecdh": "ecdlp", "dsa": "ffdlp"}

    for a in assets:
        if a.raw.get("indeterminate_reason"):
            skipped_indeterminate += 1
            continue
        kind = family_to_kind.get(a.algorithm_family)
        if kind is None or a.key_bits <= 0:
            skipped_indeterminate += 1
            continue
        if a.secrecy_lifetime_years is None:
            # Law 9: no silent default. The asset is surfaced, not guessed at.
            skipped_missing.append(a.id)
            continue

        # Agility drives the migration term unless a human overrode it.
        validity_days = None
        if a.not_before and a.not_after:
            validity_days = (a.not_after - a.not_before).days
        ag = agility_service.assess(
            kind=a.kind,
            algorithm_family=a.algorithm_family,
            validity_days=validity_days,
            is_ca=a.is_ca,
            dependents=_dependents(db, a.id),
        )
        migration_years = a.migration_years_override or ag.migration_years

        existing_ag = db.scalar(
            select(AgilityAssessment).where(AgilityAssessment.asset_id == a.id)
        )
        if existing_ag:
            db.delete(existing_ag)
            db.flush()
        db.add(
            AgilityAssessment(
                asset_id=a.id,
                score=ag.score,
                migration_years=ag.migration_years,
                dimensions=ag.as_dict()["dimensions"],
                cheapest_improvements=ag.cheapest_improvements(),
            )
        )

        result = pramana.assess_asset(
            kind,
            a.key_bits,
            migration_years=migration_years,
            secrecy_years=a.secrecy_lifetime_years,
            threat_mode=a.threat_mode,
            criticality=a.criticality or "medium",
            data_class=a.data_class or "internal",
            samples=samples,
            seed=SETTINGS.seed,
            roadmap_dir=SETTINGS.roadmap_dir,
            attacker=body.attacker,
        )

        old = db.scalar(select(Exposure).where(Exposure.asset_id == a.id))
        if old:
            db.delete(old)
            db.flush()
        by = result["break_year"]
        db.add(
            Exposure(
                asset_id=a.id,
                pipeline=result["pipeline"],
                logical_qubits=result["logical_qubits"],
                toffoli_all_runs=result["toffoli_all_runs"],
                break_year_p05=by.get("p05"),
                break_year_p50=by.get("p50"),
                break_year_p95=by.get("p95"),
                break_year_by_architecture=result["break_year_by_architecture"],
                break_year_histogram=result["break_year_histogram"],
                deadline_year=result["deadline_year"],
                probability_exposed=result["probability_exposed"],
                harvest_now_decrypt_later_year=result["harvest_now_decrypt_later_year"],
                exposure_score=result["exposure_score"],
                exposure_lower=result["exposure_lower"],
                exposure_upper=result["exposure_upper"],
                exposure_formula=result["exposure_formula"],
                warnings=result["warnings"],
                provenance=result["provenance"],
                core_version=core_version(),
            )
        )

        rec = recommend_service.recommend(
            kind=a.kind,
            algorithm_family=a.algorithm_family,
            key_bits=a.key_bits,
            purpose=a.purpose,
            is_ca=a.is_ca,
        )
        old_rec = db.scalar(select(Recommendation).where(Recommendation.asset_id == a.id))
        if old_rec:
            db.delete(old_rec)
            db.flush()
        d = rec.as_dict()
        db.add(
            Recommendation(
                asset_id=a.id,
                target_algorithm=d["target_algorithm"],
                parameter_set=d["parameter_set"],
                hybrid=d["hybrid"],
                status=d["status"],
                rationale=d["rationale"],
                citations=d["citations"],
                deadlines=d["deadlines"],
                size_impact=d["size_impact"],
            )
        )
        assessed += 1

    db.commit()
    return EstimationSummary(
        assessed=assessed,
        skipped_indeterminate=skipped_indeterminate,
        skipped_missing_inputs=skipped_missing,
        duration_seconds=round(time.monotonic() - started, 3),
    )


def _exposure_rows(db: Session, inventory_id: str) -> list[tuple[Asset, Exposure]]:
    rows = []
    for a in db.scalars(select(Asset).where(Asset.inventory_id == inventory_id)).all():
        if a.exposure is not None:
            rows.append((a, a.exposure))
    return rows


@router.get(
    "/inventories/{inventory_id}/exposure",
    response_model=list[ExposureOut],
    tags=["exposure"],
)
def list_exposure(
    inventory_id: str, db: Session = Depends(get_session)
) -> list[ExposureOut]:
    """Exposure results, highest score first."""
    rows = _exposure_rows(db, inventory_id)
    rows.sort(key=lambda r: r[1].exposure_score, reverse=True)
    return [
        ExposureOut(
            asset_id=a.id,
            kind=a.kind,
            algorithm=f"{a.algorithm_family}-{a.key_bits}",
            pipeline=e.pipeline,
            logical_qubits=e.logical_qubits,
            break_year={
                "p05": e.break_year_p05,
                "p50": e.break_year_p50,
                "p95": e.break_year_p95,
            },
            break_year_by_architecture=e.break_year_by_architecture,
            deadline_year=e.deadline_year,
            probability_exposed=e.probability_exposed,
            harvest_now_decrypt_later_year=e.harvest_now_decrypt_later_year,
            exposure_score=e.exposure_score,
            exposure_lower=e.exposure_lower,
            exposure_upper=e.exposure_upper,
            exposure_formula=e.exposure_formula,
            warnings=e.warnings,
            provenance=e.provenance,
        )
        for a, e in rows
    ]


@router.get(
    "/inventories/{inventory_id}/timeline",
    response_model=list[TimelinePoint],
    tags=["exposure"],
)
def timeline(inventory_id: str, db: Session = Depends(get_session)) -> list[TimelinePoint]:
    """Data for the exposure timeline view."""
    rows = _exposure_rows(db, inventory_id)
    rows.sort(key=lambda r: r[1].exposure_score, reverse=True)
    return [
        TimelinePoint(
            asset_id=a.id,
            label=(a.subject or a.fingerprint[:16]),
            exposure_score=e.exposure_score,
            break_year={
                "p05": e.break_year_p05,
                "p50": e.break_year_p50,
                "p95": e.break_year_p95,
            },
            deadline_year=e.deadline_year,
            not_after_year=a.not_after.year if a.not_after else None,
            threat_mode=a.threat_mode,
        )
        for a, e in rows
    ]


@router.get(
    "/inventories/{inventory_id}/timeline.svg",
    tags=["exposure"],
    response_class=Response,
)
def timeline_figure(
    inventory_id: str,
    title: str = "Computed quantum exposure by asset",
    db: Session = Depends(get_session),
) -> Response:
    """The exposure timeline as publication-quality SVG.

    This is the project's hero view and Figure 1 of the paper. Served as a figure rather
    than as data so that the artefact a reader sees is the artefact the tool produced.
    """
    rows = _exposure_rows(db, inventory_id)
    rows.sort(key=lambda r: r[1].exposure_score, reverse=True)
    tl_rows = [
        timeline_svg.TimelineRow(
            label=(a.subject or a.fingerprint[:16]).replace("CN=", ""),
            algorithm=f"{a.algorithm_family}-{a.key_bits}",
            exposure_score=e.exposure_score,
            histogram={int(k): int(v) for k, v in (e.break_year_histogram or {}).items()},
            deadline_year=e.deadline_year,
            not_after_year=a.not_after.year if a.not_after else None,
            threat_mode=a.threat_mode,
        )
        for a, e in rows
    ]
    return Response(
        content=timeline_svg.render(tl_rows, title=title),
        media_type="image/svg+xml",
        headers={"Cache-Control": "no-store"},
    )


@router.get(
    "/inventories/{inventory_id}/divergence",
    response_model=DivergenceOut,
    tags=["exposure"],
)
def divergence(inventory_id: str, db: Session = Depends(get_session)) -> DivergenceOut:
    """Criticality-based ranking against computed-exposure ranking."""
    rows = [
        {
            "asset_id": a.id,
            "label": (a.subject or a.fingerprint[:16]),
            "criticality": a.criticality,
            "data_class": a.data_class,
            "exposure_score": e.exposure_score,
        }
        for a, e in _exposure_rows(db, inventory_id)
    ]
    return DivergenceOut(**divergence_service.analyse(rows))


@router.get("/architectures", tags=["reference"])
def architectures() -> list[dict[str, Any]]:
    """Registered fault-tolerant architectures."""
    try:
        return core().list_architectures()
    except CoreUnavailable as exc:
        raise HTTPException(status.HTTP_503_SERVICE_UNAVAILABLE, str(exc)) from exc


@router.get("/trajectory", tags=["reference"])
def trajectory() -> dict[str, Any]:
    """The fitted hardware capability trajectory and its provenance."""
    try:
        return core().trajectory_summary(SETTINGS.roadmap_dir)
    except CoreUnavailable as exc:
        raise HTTPException(status.HTTP_503_SERVICE_UNAVAILABLE, str(exc)) from exc


@router.get("/assets/{asset_id}/recommendation", tags=["migration"])
def recommendation(asset_id: str, db: Session = Depends(get_session)) -> dict[str, Any]:
    """The migration recommendation for one asset."""
    a = db.get(Asset, asset_id)
    if a is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, "asset not found")
    rec = recommend_service.recommend(
        kind=a.kind,
        algorithm_family=a.algorithm_family,
        key_bits=a.key_bits,
        purpose=a.purpose,
        is_ca=a.is_ca,
    )
    return rec.as_dict()


@router.get("/assets/{asset_id}/agility", tags=["migration"])
def asset_agility(asset_id: str, db: Session = Depends(get_session)) -> dict[str, Any]:
    """The crypto-agility assessment for one asset."""
    a = db.get(Asset, asset_id)
    if a is None:
        raise HTTPException(status.HTTP_404_NOT_FOUND, "asset not found")
    validity_days = None
    if a.not_before and a.not_after:
        validity_days = (a.not_after - a.not_before).days
    ag = agility_service.assess(
        kind=a.kind,
        algorithm_family=a.algorithm_family,
        validity_days=validity_days,
        is_ca=a.is_ca,
        dependents=_dependents(db, a.id),
    )
    out = ag.as_dict()
    out["cheapest_improvements"] = ag.cheapest_improvements()
    return out
