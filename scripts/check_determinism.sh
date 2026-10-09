#!/usr/bin/env bash
# Law 5: re-running with the same manifest must reproduce byte-identical results.
#
# Checked at the artefact level rather than by unit test, because the property that matters
# is that a *report* is reproducible, not merely that a function is pure.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Running the verification harness twice..."
PRAMANA_ROOT=. cargo run --quiet --release -p pramana-verify > "$tmp/run1.txt" 2>/dev/null || true
cp verification/reports/report.json "$tmp/report1.json"
PRAMANA_ROOT=. cargo run --quiet --release -p pramana-verify > "$tmp/run2.txt" 2>/dev/null || true
cp verification/reports/report.json "$tmp/report2.json"

if ! diff -q "$tmp/report1.json" "$tmp/report2.json" >/dev/null; then
  echo "FAIL: the verification report differs between identical runs"
  diff "$tmp/report1.json" "$tmp/report2.json" | head -20
  exit 1
fi
echo "  PASS  verification report is byte-identical across runs"

echo "Running the exposure assessment twice..."
PRAMANA_ROOT=. cargo run --quiet --release --example exposure -p pramana-risk > "$tmp/exp1.txt" 2>/dev/null
PRAMANA_ROOT=. cargo run --quiet --release --example exposure -p pramana-risk > "$tmp/exp2.txt" 2>/dev/null
if ! diff -q "$tmp/exp1.txt" "$tmp/exp2.txt" >/dev/null; then
  echo "FAIL: the exposure assessment differs between identical runs"
  diff "$tmp/exp1.txt" "$tmp/exp2.txt" | head -20
  exit 1
fi
echo "  PASS  exposure assessment is byte-identical across runs"

echo
echo "Determinism holds."
