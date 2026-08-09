# OMT Tools

Open Media Transport production utilities inspired by NDI Tools.

## Suite contents

| Tool | Status | Description |
|------|--------|-------------|
| **Launcher** (Tauri) | MVP | Starts bundled tools, language/theme settings, suite version |
| **Studio Monitor** (egui) | MVP | Discover and view OMT sources on the LAN |
| **Studio Monitor** (GPUI) | Prototype | Temporary GPUI alternate for A/B comparison |
| **Test Patterns** (egui) | MVP | Send SMPTE-style patterns + tone over OMT |
| **Screen Capture** | Spike | Windows Graphics Capture / ScreenCaptureKit probe |

## Workspace layout

```text
omt-tools/
  apps/launcher/             Tauri launcher (frontend + src-tauri)
  apps/studio-monitor/       egui viewer (product default)
  apps/studio-monitor-gpui/  GPUI viewer (temporary A/B)
  apps/test-patterns/        egui sender
  crates/suite-core/         settings, i18n, versions
  crates/omt-media/          discovery / receive / send helpers
  crates/pattern-generator/
  crates/monitor-bench/      headless A/B harness
  crates/capture-spike/
```

Runtime media stack: [`openmediatransport-rs`](../openmediatransport-rs) + [`vmx-rs`](../vmx-rs).
Official `libomt` / `libomtnet` / `libvmx` are reference-only and are not modified.

## Prerequisites

- Rust **1.88+** (edition 2024)
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
cargo run --release -p omt-studio-monitor-gpui

# Headless A/B (receive + present-path simulation)
cargo run --release -p omt-studio-monitor -- --headless --url omt://127.0.0.1:6400/Test --seconds 10
cargo run --release -p omt-studio-monitor-gpui -- --headless --url omt://127.0.0.1:6400/Test --seconds 10
# or: ./scripts/ab-monitor-bench.ps1 -Url omt://...

# Screen capture spike
cargo run -p capture-spike -- --smoke
```

### Launcher

```bash
cd apps/launcher
bun install
bun run tauri dev
```

In development the launcher looks for `omt-studio-monitor` / `omt-test-patterns` under `target/debug` or `target/release`.

## Package (suite install)

```bash
cargo build --release -p omt-studio-monitor -p omt-test-patterns
# Windows
./scripts/prepare-sidecars.ps1
# macOS/Linux
./scripts/prepare-sidecars.sh

cd apps/launcher
bun install
bun run tauri build
```

### Auto-update / signing checklist

1. `bun run tauri signer generate -w <private-key-path>`
2. Set `plugins.updater.pubkey` in `apps/launcher/src-tauri/tauri.conf.json`
3. Point `plugins.updater.endpoints` at your HTTPS release feed
4. Enable `bundle.createUpdaterArtifacts`
5. Configure Windows Authenticode + macOS Developer ID / notarization
6. Before applying an update, the launcher can query `list_running_tools` so open sidecars are closed first

## Settings

Only the launcher edits preferences:

- Language: Japanese / English
- Theme: Light / Dark / System
- Suite version + per-tool versions (settings page)

Tools receive `--language` / `--theme` (and matching env vars) from the launcher.

## Targets

- Windows x64 (`x86_64-pc-windows-msvc`)
- Windows Arm64 (`aarch64-pc-windows-msvc`)
- macOS Intel (`x86_64-apple-darwin`)
- macOS Apple Silicon (`aarch64-apple-darwin`)

## License

MIT
