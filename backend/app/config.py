"""Configuration.

Every value is overridable by environment variable so that the same image runs in dev
against SQLite and in production against PostgreSQL.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path


def _repo_root() -> Path:
    """Locate the repository root from this file's position."""
    return Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class Settings:
    """Runtime settings."""

    #: SQLAlchemy URL. SQLite by default so the stack runs with no services; set
    #: ``PRAMANA_DATABASE_URL`` to a PostgreSQL DSN in production. The schema is written to
    #: be portable between the two.
    database_url: str = field(
        default_factory=lambda: os.environ.get(
            "PRAMANA_DATABASE_URL", f"sqlite:///{_repo_root() / 'build' / 'pramana.db'}"
        )
    )

    #: Where the compiled ``pramana`` extension module lives.
    core_path: str = field(
        default_factory=lambda: os.environ.get(
            "PRAMANA_CORE_PATH", str(_repo_root() / "build")
        )
    )

    #: Directory of vendor roadmap data.
    roadmap_dir: str = field(
        default_factory=lambda: os.environ.get(
            "PRAMANA_ROADMAP_DIR", str(_repo_root() / "data" / "roadmaps")
        )
    )

    #: Monte Carlo samples per asset for interactive requests.
    default_samples: int = field(
        default_factory=lambda: int(os.environ.get("PRAMANA_SAMPLES", "4000"))
    )

    #: Seed, fixed so that identical requests reproduce identical results (Law 5).
    seed: int = field(
        default_factory=lambda: int(os.environ.get("PRAMANA_SEED", str(0x5052414D414E41)))
    )

    #: Hosts a live scan is permitted to touch. Empty means scanning is disabled, which is
    #: the default: scanning without authorisation is a legal and ethical hazard.
    scan_allowlist: tuple[str, ...] = field(
        default_factory=lambda: tuple(
            h.strip()
            for h in os.environ.get("PRAMANA_SCAN_ALLOWLIST", "").split(",")
            if h.strip()
        )
    )


SETTINGS = Settings()
