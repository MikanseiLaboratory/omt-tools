# OMT Tools

[![CI](https://github.com/MikanseiLaboratory/omt-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/MikanseiLaboratory/omt-tools/actions/workflows/ci.yml)

Open Media Transport production utilities inspired by NDI Tools.

## Suite contents

| Tool | Description |
|------|-------------|
| Studio Monitor |  Discover and view OMT sources on the LAN |
| Test Patterns |  Send SMPTE-style patterns + tone over OMT |
| Screen Capture |  Windows Graphics Capture / ScreenCaptureKit (WIP) |

Runtime media stack: [`MikanseiLaboratory/openmediatransport-rs`](https://github.com/MikanseiLaboratory/openmediatransport-rs) + [`MikanseiLaboratory/vmx-rs`](https://github.com/MikanseiLaboratory/vmx-rs).

## Prerequisites

- Rust **1.97+** (edition 2024)
- Bun 1.2+ (launcher frontend)
- Sibling checkouts:
  - `../openmediatransport-rs`
  - `../vmx-rs`

## Develop

```bash
# Shared crates + tools
cargo test -p suite-core -p omt-media -p pattern-generator -p capture-spike
cargo run -p omt-studio-monitor
cargo run -p omt-test-patterns

# Prefer release for realtime encode/view
cargo run --release -p omt-test-patterns
cargo run --release -p omt-studio-monitor

# Headless present-path bench
cargo run --release -p omt-studio-monitor -- --headless --url omt://127.0.0.1:6400/Test --seconds 10
# or: ./scripts/monitor-bench.ps1 -Url omt://...

# Screen capture spike
cargo run -p capture-spike -- --smoke
```

### Launcher

```bash
# From repo root (recommended)
bun install --cwd apps/launcher
bun run tools:build   # once: so Studio Monitor / Test Patterns resolve
bun run dev           # tauri dev + Vite HMR for the launcher UI

# Or from apps/launcher
cd apps/launcher
bun install
bun run tools:build
bun run dev
```

## Targets

- Windows x64 (`x86_64-pc-windows-msvc`)
- Windows Arm64 (`aarch64-pc-windows-msvc`)
- macOS Intel (`x86_64-apple-darwin`)
- macOS Apple Silicon (`aarch64-apple-darwin`)

## License

MIT
