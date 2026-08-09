#!/usr/bin/env bash
set -euo pipefail

# Copy release tool binaries into the Tauri sidecar folder with target-triple suffixes.
# Usage: ./scripts/prepare-sidecars.sh [target-triple]

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${1:-$(rustc -vV | awk '/^host:/{print $2}')}"
OUT="$ROOT/apps/launcher/src-tauri/binaries"
PROFILE_DIR="$ROOT/target/release"
mkdir -p "$OUT"

copy_one() {
  local name="$1"
  local src
  if [[ "$TARGET" == *-windows-* ]]; then
    src="$PROFILE_DIR/${name}.exe"
    cp "$src" "$OUT/${name}-${TARGET}.exe"
  else
    src="$PROFILE_DIR/${name}"
    cp "$src" "$OUT/${name}-${TARGET}"
    chmod +x "$OUT/${name}-${TARGET}"
  fi
  echo "prepared $name for $TARGET"
}

copy_one omt-studio-monitor
copy_one omt-test-patterns
