"""Application entry point."""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from fastapi import FastAPI

from .api import router
from .db import init_db


@asynccontextmanager
async def lifespan(_: FastAPI) -> AsyncIterator[None]:
    """Create tables on startup. Alembic owns migrations in production."""
    init_db()
    yield


app = FastAPI(
    lifespan=lifespan,
    title="PRAMANA",
    version="0.1.0",
    description=(
        "Cryptographic resource estimation and migration triage. Computes, per asset, the "
        "year that asset becomes quantum-breakable, by synthesising the attack circuit, "
        "costing its error correction across fault-tolerant architectures, and resolving "
        "Mosca's inequality against the asset's data-retention requirement. No fixed Q-Day "
        "is assumed anywhere."
    ),
)
app.include_router(router)


@app.get("/", tags=["ops"])
def root() -> dict[str, str]:
    """Service banner."""
    return {"service": "pramana", "docs": "/docs", "api": "/api/v1"}
