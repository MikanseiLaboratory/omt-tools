#!/usr/bin/env bash
set -euo pipefail
# Run headless A/B between egui and GPUI Studio Monitor present paths.
# Usage: ./scripts/ab-monitor-bench.sh omt://host:port/Name [seconds]

URL="${1:?url required}"
SECONDS_N="${2:-10}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== building release monitors =="
cargo build --release -p omt-studio-monitor -p omt-studio-monitor-gpui

echo "== egui headless =="
./target/release/omt-studio-monitor --headless --url "$URL" --seconds "$SECONDS_N"

echo "== gpui headless =="
./target/release/omt-studio-monitor-gpui --headless --url "$URL" --seconds "$SECONDS_N"
