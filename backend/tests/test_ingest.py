"""Ingestion behaviour."""

from __future__ import annotations


def test_ingests_an_rsa_certificate(client, inventory, rsa_cert):
    r = client.post(
        f"/api/v1/inventories/{inventory}/ingest",
        files={"files": ("rsa.pem", rsa_cert, "application/x-pem-file")},
    )
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["ingested"] == 1
    assert body["rejected"] == []

    assets = client.get(f"/api/v1/inventories/{inventory}/assets").json()
    assert len(assets) == 1
    a = assets[0]
    assert a["algorithm_family"] == "rsa"
    assert a["key_bits"] == 2048


def test_ingests_an_elliptic_curve_certificate(client, inventory, ec_cert):
    client.post(
        f"/api/v1/inventories/{inventory}/ingest",
        files={"files": ("ec.pem", ec_cert, "application/x-pem-file")},
    )
    a = client.get(f"/api/v1/inventories/{inventory}/assets").json()[0]
    assert a["algorithm_family"] == "ecdsa"
    assert a["key_bits"] == 256
    assert a["curve"] == "secp256r1"


def test_private_keys_are_rejected_not_parsed(client, inventory, private_key_pem):
    """Accepting private key material at all would mean it had reached a request log."""
    r = client.post(
        f"/api/v1/inventories/{inventory}/ingest",
        files={"files": ("key.pem", private_key_pem, "application/x-pem-file")},
    )
    assert r.status_code == 200
    body = r.json()
    assert body["ingested"] == 0
    assert len(body["rejected"]) == 1
    assert "private key" in body["rejected"][0].lower()


def test_duplicate_certificates_are_deduplicated(client, inventory, rsa_cert):
    url = f"/api/v1/inventories/{inventory}/ingest"
    client.post(url, files={"files": ("a.pem", rsa_cert, "application/x-pem-file")})
    second = client.post(
        url, files={"files": ("b.pem", rsa_cert, "application/x-pem-file")}
    ).json()
    assert second["ingested"] == 0
    assert second["duplicates"] == 1


def test_a_bundle_builds_the_chain_graph(client, inventory, ca_chain):
    r = client.post(
        f"/api/v1/inventories/{inventory}/ingest",
        files={"files": ("chain.pem", ca_chain, "application/x-pem-file")},
    ).json()
    assert r["ingested"] == 2
    assets = client.get(f"/api/v1/inventories/{inventory}/assets").json()
    kinds = {a["kind"] for a in assets}
    assert "ca_root" in kinds
    # The leaf's agility score must reflect that a dependent exists on the root.
    root = next(a for a in assets if a["is_ca"])
    ag = client.get(f"/api/v1/assets/{root['id']}/agility").json()
    depth = next(d for d in ag["dimensions"] if d["name"] == "dependency_depth")
    assert depth["score"] < 100, "the root has a dependent and must score below perfect"


def test_garbage_is_rejected_with_a_reason(client, inventory):
    r = client.post(
        f"/api/v1/inventories/{inventory}/ingest",
        files={"files": ("junk.txt", b"not a certificate", "text/plain")},
    ).json()
    assert r["ingested"] == 0
    assert r["rejected"]
