# PRAMANA developer entry points.
SHELL := /bin/bash

.PHONY: all test build fmt clippy doc synthesise verify hypothesis spread exposure ecdlp-descent \
	laws determinism ci \
	core backend-test serve frontend frontend-dev figure clean bootstrap

all: test

bootstrap:
	@bash scripts/bootstrap.sh

build:
	cargo build --workspace

test:
	cargo test --workspace

# Mechanical enforcement of the specification's laws.
laws:
	@python3 scripts/check_laws.py

# Law 5: the same manifest reproduces byte-identical results.
determinism:
	@bash scripts/check_determinism.sh

# Everything CI runs, locally.
ci: laws test verify determinism backend-test

# Build the Python extension module the backend imports.
core:
	cargo build --release -p pramana-py
	mkdir -p build
	cp $${CARGO_TARGET_DIR:-target}/release/libpramana.so build/pramana.so

backend-test: core
	cd backend && python3 -m pytest tests/ -q

serve: core
	cd backend && python3 -m uvicorn app.main:app --reload --port 8000

frontend:
	cd frontend && npm install --no-audit --no-fund && npm run build

frontend-dev:
	cd frontend && npm run dev

# Regenerate the paper figure from committed data.
figure: core
	cd backend && python3 -m app.figure

synthesise:
	cargo run --release --example synthesise -p pramana-circuit

# Reproduction harness. Exits non-zero if any golden target regresses.
verify:
	PRAMANA_ROOT=. cargo run --release -p pramana-verify

# Full causal chain: asset parameters through to an exposure score.
exposure:
	PRAMANA_ROOT=. cargo run --release --example exposure -p pramana-risk

# The 2017-to-2026 elliptic-curve resource descent.
ecdlp-descent:
	cargo run --release --example ecdlp_descent -p pramana-circuit

# Architecture sensitivity sweep.
spread:
	cargo run --release --example spread -p pramana-verify

# Controlled experiments testing the hypotheses raised by `make verify`.
hypothesis:
	cargo run --release --example hypothesis -p pramana-circuit
	cargo run --release --example reproduce -p pramana-circuit

doc:
	cargo doc --workspace --no-deps

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

clean:
	cargo clean
