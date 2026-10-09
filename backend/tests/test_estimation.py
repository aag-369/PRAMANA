"""End-to-end estimation, exposure and recommendation behaviour."""

from __future__ import annotations

import pytest

pytest.importorskip("pramana", reason="the compiled estimation core is not built")

SAMPLES = 400


def _ingest_and_configure(client, inventory, cert, name, **fields):
    client.post(
        f"/api/v1/inventories/{inventory}/ingest",
        files={"files": (name, cert, "application/x-pem-file")},
    )
    assets = client.get(f"/api/v1/inventories/{inventory}/assets").json()
    asset = assets[-1]
    if fields:
        r = client.patch(f"/api/v1/assets/{asset['id']}", json=fields)
        assert r.status_code == 200, r.text
    return asset["id"]


def test_the_core_is_loaded(client):
    body = client.get("/api/v1/healthz").json()
    assert body["status"] == "ok", body


def test_trajectory_is_fitted_on_verified_demonstrations(client):
    t = client.get("/api/v1/trajectory").json()
    assert t["points"] >= 3
    assert 1.0 < t["growth_per_year"] < 3.0, (
        "a growth rate outside this band means the fit has come unanchored from the "
        f"roadmap data; got {t['growth_per_year']}"
    )
    # The finding that motivates fitting logical rather than physical qubits.
    ratios = t["physical_per_logical"]
    assert ratios["superconducting"]["geometric_mean"] > 10 * ratios["trapped ion"]["geometric_mean"]


def test_architectures_are_registered(client):
    names = [a["name"] for a in client.get("/api/v1/architectures").json()]
    assert len(names) >= 3
    assert any("cat" in n for n in names)


def test_an_asset_without_a_secrecy_lifetime_is_skipped_not_guessed(
    client, inventory, rsa_cert
):
    """Law 9: PRAMANA does not invent the input Mosca's inequality needs."""
    asset_id = _ingest_and_configure(client, inventory, rsa_cert, "rsa.pem")
    r = client.post(
        f"/api/v1/inventories/{inventory}/estimate",
        json={"samples": SAMPLES},
    ).json()
    assert r["assessed"] == 0
    assert asset_id in r["skipped_missing_inputs"]


def test_full_chain_produces_an_exposure(client, inventory, rsa_cert):
    _ingest_and_configure(
        client,
        inventory,
        rsa_cert,
        "rsa.pem",
        secrecy_lifetime_years=10.0,
        criticality="high",
        data_class="confidential",
        threat_mode="confidentiality",
    )
    summary = client.post(
        f"/api/v1/inventories/{inventory}/estimate", json={"samples": SAMPLES}
    ).json()
    assert summary["assessed"] == 1

    exposures = client.get(f"/api/v1/inventories/{inventory}/exposure").json()
    assert len(exposures) == 1
    e = exposures[0]
    assert e["logical_qubits"] > 0
    assert e["break_year"]["p05"] <= e["break_year"]["p50"] <= e["break_year"]["p95"]
    assert 0.0 <= e["probability_exposed"] <= 1.0
    assert 0.0 <= e["exposure_score"] <= 100.0
    # Law 2: the provenance chain must be present, not reconstructed later.
    assert e["provenance"]["monte_carlo_samples"] == SAMPLES
    assert e["provenance"]["circuit_pipeline"]
    assert e["provenance"]["trajectory_points"] >= 3
    # Every architecture must report a median.
    assert len(e["break_year_by_architecture"]) >= 3


def test_results_are_reproducible(client, inventory, rsa_cert):
    """Law 5: the same manifest reproduces the same result."""
    _ingest_and_configure(
        client, inventory, rsa_cert, "rsa.pem", secrecy_lifetime_years=10.0
    )
    url = f"/api/v1/inventories/{inventory}/estimate"
    client.post(url, json={"samples": SAMPLES})
    first = client.get(f"/api/v1/inventories/{inventory}/exposure").json()[0]
    client.post(url, json={"samples": SAMPLES})
    second = client.get(f"/api/v1/inventories/{inventory}/exposure").json()[0]
    assert first["exposure_score"] == second["exposure_score"]
    assert first["break_year"] == second["break_year"]


def test_a_longer_secrecy_lifetime_raises_exposure(client, inventory, rsa_cert, ec_cert):
    short = _ingest_and_configure(
        client, inventory, rsa_cert, "short.pem",
        secrecy_lifetime_years=1.0, criticality="high", data_class="confidential",
    )
    long_ = _ingest_and_configure(
        client, inventory, ec_cert, "long.pem",
        secrecy_lifetime_years=40.0, criticality="high", data_class="confidential",
    )
    client.post(f"/api/v1/inventories/{inventory}/estimate", json={"samples": SAMPLES})
    scores = {
        e["asset_id"]: e["exposure_score"]
        for e in client.get(f"/api/v1/inventories/{inventory}/exposure").json()
    }
    assert scores[long_] > scores[short], (
        "a forty-year secret must be more exposed than a one-year secret"
    )


def test_signature_assets_score_below_confidentiality_assets(
    client, inventory, rsa_cert, ec_cert
):
    """A forged signature is not retroactive; captured ciphertext is."""
    conf = _ingest_and_configure(
        client, inventory, rsa_cert, "conf.pem",
        secrecy_lifetime_years=25.0, criticality="critical",
        data_class="restricted", threat_mode="confidentiality",
    )
    auth = _ingest_and_configure(
        client, inventory, ec_cert, "auth.pem",
        secrecy_lifetime_years=25.0, criticality="critical",
        data_class="restricted", threat_mode="authenticity",
    )
    client.post(f"/api/v1/inventories/{inventory}/estimate", json={"samples": SAMPLES})
    scores = {
        e["asset_id"]: e["exposure_score"]
        for e in client.get(f"/api/v1/inventories/{inventory}/exposure").json()
    }
    assert scores[conf] > scores[auth]


def test_timeline_is_ordered_by_exposure(client, inventory, rsa_cert, ec_cert):
    _ingest_and_configure(
        client, inventory, rsa_cert, "a.pem",
        secrecy_lifetime_years=2.0, criticality="low", data_class="public",
    )
    _ingest_and_configure(
        client, inventory, ec_cert, "b.pem",
        secrecy_lifetime_years=40.0, criticality="critical", data_class="restricted",
    )
    client.post(f"/api/v1/inventories/{inventory}/estimate", json={"samples": SAMPLES})
    rows = client.get(f"/api/v1/inventories/{inventory}/timeline").json()
    assert len(rows) == 2
    assert rows[0]["exposure_score"] >= rows[1]["exposure_score"]


def test_divergence_is_reported(client, inventory, rsa_cert, ec_cert):
    """The novelty claim, operationalised: criticality ranking against computed ranking."""
    _ingest_and_configure(
        client, inventory, rsa_cert, "a.pem",
        secrecy_lifetime_years=40.0, criticality="low", data_class="public",
    )
    _ingest_and_configure(
        client, inventory, ec_cert, "b.pem",
        secrecy_lifetime_years=1.0, criticality="critical", data_class="restricted",
    )
    client.post(f"/api/v1/inventories/{inventory}/estimate", json={"samples": SAMPLES})
    d = client.get(f"/api/v1/inventories/{inventory}/divergence").json()
    assert d["n"] == 2
    assert -1.0 <= d["kendall_tau"] <= 1.0
    assert d["interpretation"]
    assert d["biggest_movers"]


def test_recommendations_never_propose_a_pending_standard(client, inventory, ec_cert):
    asset_id = _ingest_and_configure(client, inventory, ec_cert, "ec.pem")
    rec = client.get(f"/api/v1/assets/{asset_id}/recommendation").json()
    assert rec["status"] == "final", "a draft parameter set must not be the primary target"
    assert rec["citations"]
    assert rec["deadlines"]
    for alt in rec["alternatives"]:
        assert "NOT YET AVAILABLE" in alt["why"] or "state reuse" in alt["why"]


def test_recommendation_computes_size_impact(client, inventory, rsa_cert):
    asset_id = _ingest_and_configure(client, inventory, rsa_cert, "rsa.pem")
    rec = client.get(f"/api/v1/assets/{asset_id}/recommendation").json()
    impact = rec["size_impact"]
    assert impact["target_public_key_bytes"] > 0
    assert "public_key_growth" in impact
