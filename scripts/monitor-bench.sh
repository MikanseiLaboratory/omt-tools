# Headless Studio Monitor present-path bench.
# Usage: ./scripts/monitor-bench.sh omt://host:port/Name [seconds]

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
URL="${1:?url required}"
SECONDS_N="${2:-10}"

cd "$ROOT"
cargo build --release -p omt-studio-monitor
./target/release/omt-studio-monitor --headless --url "$URL" --seconds "$SECONDS_N"
