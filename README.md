# OMT Tools

[![CI](https://github.com/MikanseiLaboratory/omt-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/MikanseiLaboratory/omt-tools/actions/workflows/ci.yml)

Open Media Transport production utilities inspired by NDI Tools.

<img width="864" height="571" alt="image" src="https://github.com/user-attachments/assets/80605b83-effc-4733-b96a-80b34c46e3ce" />


## Suite contents

| Tool | Description |
|------|-------------|
| Studio Monitor | Discover and view OMT sources on the LAN |
| Test Patterns | Send SMPTE-style patterns + tone over OMT |
| Config Manager | View and edit the global OMT `settings.xml` |
| Discovery Server | GUI + CLI TCP discovery server (port 6399) |
| Screen Capture | Windows Graphics Capture / ScreenCaptureKit (WIP) |

Runtime media stack: [`MikanseiLaboratory/openmediatransport-rs`](https://github.com/MikanseiLaboratory/openmediatransport-rs) + [`MikanseiLaboratory/vmx-rs`](https://github.com/MikanseiLaboratory/vmx-rs)
(VMX SIMD path reporting via `simd_path()`: `avx2` / `sse128` / `neon` / `scalar`).

## Prerequisites

- Rust **1.97+** (edition 2024)
- Bun 1.2+ (launcher frontend)

## Build Targets

- Windows x64 (`x86_64-pc-windows-msvc`)
- Windows Arm64 (`aarch64-pc-windows-msvc`)
- macOS Intel (`x86_64-apple-darwin`)
- macOS Apple Silicon (`aarch64-apple-darwin`)

## License

MIT
