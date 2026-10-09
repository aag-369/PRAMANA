"""Regenerate the paper figure from committed data.

Run with ``make figure``. Deterministic: the seed is fixed, so the figure in the paper and
the figure this produces are the same figure.
"""

from __future__ import annotations

import datetime as dt
import os
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
_tmp = tempfile.mkdtemp(prefix="pramana-figure-")
os.environ["PRAMANA_DATABASE_URL"] = f"sqlite:///{_tmp}/figure.db"
os.environ.setdefault("PRAMANA_CORE_PATH", str(ROOT / "build"))
os.environ.setdefault("PRAMANA_ROADMAP_DIR", str(ROOT / "data" / "roadmaps"))
sys.path.insert(0, str(ROOT / "backend"))
sys.path.insert(0, os.environ["PRAMANA_CORE_PATH"])

from cryptography import x509  # noqa: E402
from cryptography.hazmat.primitives import hashes, serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import ec, rsa  # noqa: E402
from cryptography.x509.oid import NameOID  # noqa: E402
from fastapi.testclient import TestClient  # noqa: E402

from app.db import init_db  # noqa: E402
from app.main import app  # noqa: E402

#: A representative estate. Deliberately mixed: different algorithms, threat modes and
#: retention requirements, because a figure showing only TLS certificates would not
#: demonstrate the finding.
FLEET = [
    ("archive.acme.test", "rsa4096", 3650, False, 30.0, "critical", "restricted", "confidentiality"),
    ("payments.acme.test", "rsa2048", 397, False, 15.0, "critical", "restricted", "confidentiality"),
    ("vpn.acme.test", "rsa2048", 730, False, 12.0, "high", "confidential", "confidentiality"),
    ("legacy-edi.acme.test", "rsa2048", 1095, False, 8.0, "medium", "internal", "both"),
    ("www.acme.test", "p256", 90, False, 5.0, "high", "confidential", "confidentiality"),
    ("Acme Code Signing Root", "p384", 3650, True, 20.0, "critical", "restricted", "authenticity"),
]


def _key(kind: str):
    if kind == "rsa4096":
        return rsa.generate_private_key(public_exponent=65537, key_size=4096)
    if kind == "rsa2048":
        return rsa.generate_private_key(public_exponent=65537, key_size=2048)
    if kind == "p384":
        return ec.generate_private_key(ec.SECP384R1())
    return ec.generate_private_key(ec.SECP256R1())


def _cert(cn: str, key, days: int, ca: bool) -> bytes:
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, cn)])
    now = dt.datetime.now(dt.timezone.utc)
    builder = (
        x509.CertificateBuilder()
        .subject_name(name)
        .issuer_name(name)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - dt.timedelta(days=1))
        .not_valid_after(now + dt.timedelta(days=days))
        .add_extension(x509.BasicConstraints(ca=ca, path_length=None), critical=True)
    )
    return builder.sign(key, hashes.SHA256()).public_bytes(serialization.Encoding.PEM)


def main() -> None:
    """Build the estate, assess it, and write the figure."""
    init_db()
    client = TestClient(app)
    inv = client.post(
        "/api/v1/inventories", json={"name": "figure", "organisation": "Acme"}
    ).json()["id"]

    for cn, keykind, days, ca, secrecy, crit, dc, threat in FLEET:
        pem = _cert(cn, _key(keykind), days, ca)
        client.post(
            f"/api/v1/inventories/{inv}/ingest",
            files={"files": (f"{cn}.pem", pem, "application/x-pem-file")},
        )
        asset = client.get(f"/api/v1/inventories/{inv}/assets").json()[-1]
        client.patch(
            f"/api/v1/assets/{asset['id']}",
            json={
                "secrecy_lifetime_years": secrecy,
                "criticality": crit,
                "data_class": dc,
                "threat_mode": threat,
            },
        )

    summary = client.post(
        f"/api/v1/inventories/{inv}/estimate", json={"samples": 8000}
    ).json()
    print(f"assessed {summary['assessed']} assets in {summary['duration_seconds']}s")

    svg = client.get(
        f"/api/v1/inventories/{inv}/timeline.svg",
        params={"title": "Computed quantum exposure by asset"},
    ).text

    out_dir = ROOT / "docs" / "paper"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "figure1-exposure-timeline.svg").write_text(svg)
    print(f"wrote {out_dir / 'figure1-exposure-timeline.svg'}")

    div = client.get(f"/api/v1/inventories/{inv}/divergence").json()
    print(
        f"prioritisation divergence: Kendall tau {div['kendall_tau']}, "
        f"Spearman rho {div['spearman_rho']} over {div['n']} assets"
    )


if __name__ == "__main__":
    main()
