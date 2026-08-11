# OMT Tools

[![CI](https://github.com/MikanseiLaboratory/omt-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/MikanseiLaboratory/omt-tools/actions/workflows/ci.yml)

Open Media Transport production utilities inspired by NDI Tools.

<img width="864" height="571" alt="image" src="https://github.com/user-attachments/assets/80605b83-effc-4733-b96a-80b34c46e3ce" />


## Suite contents

| Tool | Description |
|------|-------------|
| Studio Monitor |  Discover and view OMT sources on the LAN |
| Test Patterns |  Send SMPTE-style patterns + tone over OMT |
| Screen Capture |  Windows Graphics Capture / ScreenCaptureKit (WIP) |

Runtime media stack: [`MikanseiLaboratory/openmediatransport-rs`](https://github.com/MikanseiLaboratory/openmediatransport-rs) + [`MikanseiLaboratory/vmx-rs`](https://github.com/MikanseiLaboratory/vmx-rs).

## Docs & Guides

The launcher **Docs & Guides** button opens this section.

- [Studio Monitor](#studio-monitor) — browse LAN OMT sources, fullscreen preview
- [Test Patterns](#test-patterns) — send SMPTE-style patterns + tone
- [Windows install path](#install-destination-windows)

A [project wiki](https://github.com/MikanseiLaboratory/omt-tools/wiki) can host longer guides once pages are added.

### Studio Monitor

- Discover and view OMT sources on the LAN
- Fullscreen: toolbar button, **F11**, or double-click the preview (Esc / click / F11 to exit)
- Preferences cover language, theme, audio device, buffer delay, and quality

### Test Patterns

- Send SMPTE-style patterns and tone over OMT from the companion tool launched by the suite

## Install destination (Windows)

Windows installers place the suite under a MikanseiLaboratory folder (same layout as [vmix-utility](https://github.com/MikanseiLaboratory/vmix-utility)):

- Per-machine: `C:\Program Files\MikanseiLaboratory\OMT Tools`
- Per-user: `%LOCALAPPDATA%\MikanseiLaboratory\OMT Tools`

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
