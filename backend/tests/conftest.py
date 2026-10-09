"""Test fixtures.

Each test run gets a fresh SQLite database in a temporary directory, so tests never share
state and never touch a developer's working database.
"""

from __future__ import annotations

import datetime as dt
import os
import sys
import tempfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "backend"))

_tmp = tempfile.mkdtemp(prefix="pramana-test-")
os.environ["PRAMANA_DATABASE_URL"] = f"sqlite:///{_tmp}/test.db"
os.environ.setdefault("PRAMANA_CORE_PATH", str(ROOT / "build"))
# The compiled core must be importable at collection time so that tests guarded by
# importorskip run rather than silently skipping.
sys.path.insert(0, os.environ["PRAMANA_CORE_PATH"])
os.environ.setdefault("PRAMANA_ROADMAP_DIR", str(ROOT / "data" / "roadmaps"))

from cryptography import x509  # noqa: E402
from cryptography.hazmat.primitives import hashes, serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import ec, rsa  # noqa: E402
from cryptography.x509.oid import NameOID  # noqa: E402
from fastapi.testclient import TestClient  # noqa: E402

from app.db import init_db  # noqa: E402
from app.main import app  # noqa: E402


def _make_cert(
    subject_cn: str,
    key,
    *,
    issuer_cn: str | None = None,
    issuer_key=None,
    is_ca: bool = False,
    days: int = 90,
) -> bytes:
    """Build a self-signed or issued certificate and return it as PEM."""
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, subject_cn)])
    issuer = x509.Name(
        [x509.NameAttribute(NameOID.COMMON_NAME, issuer_cn or subject_cn)]
    )
    now = dt.datetime.now(dt.timezone.utc)
    builder = (
        x509.CertificateBuilder()
        .subject_name(subject)
        .issuer_name(issuer)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - dt.timedelta(days=1))
        .not_valid_after(now + dt.timedelta(days=days))
        .add_extension(x509.BasicConstraints(ca=is_ca, path_length=None), critical=True)
    )
    signer = issuer_key or key
    cert = builder.sign(signer, hashes.SHA256())
    return cert.public_bytes(serialization.Encoding.PEM)


@pytest.fixture(scope="session")
def rsa_key():
    """A 2048-bit RSA key."""
    return rsa.generate_private_key(public_exponent=65537, key_size=2048)


@pytest.fixture(scope="session")
def ec_key():
    """A P-256 key."""
    return ec.generate_private_key(ec.SECP256R1())


@pytest.fixture(scope="session")
def rsa_cert(rsa_key) -> bytes:
    """A self-signed RSA-2048 leaf certificate."""
    return _make_cert("rsa.example.test", rsa_key)


@pytest.fixture(scope="session")
def ec_cert(ec_key) -> bytes:
    """A self-signed P-256 leaf certificate."""
    return _make_cert("ec.example.test", ec_key)


@pytest.fixture(scope="session")
def ca_chain(rsa_key, ec_key) -> bytes:
    """A CA certificate and a leaf it issued, as one PEM bundle."""
    ca = _make_cert("Test Root CA", rsa_key, is_ca=True, days=3650)
    leaf = _make_cert(
        "leaf.example.test", ec_key, issuer_cn="Test Root CA", issuer_key=rsa_key, days=90
    )
    return ca + leaf


@pytest.fixture(scope="session")
def private_key_pem(rsa_key) -> bytes:
    """A PEM private key, which the API must refuse."""
    return rsa_key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )


@pytest.fixture(scope="session", autouse=True)
def _db():
    init_db()


@pytest.fixture()
def client() -> TestClient:
    """A test client."""
    return TestClient(app)


@pytest.fixture()
def inventory(client: TestClient) -> str:
    """A fresh inventory id."""
    r = client.post("/api/v1/inventories", json={"name": "test", "organisation": "acme"})
    assert r.status_code == 200, r.text
    return r.json()["id"]
