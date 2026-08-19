# Sidecar binaries

For `tauri dev`, run `bun run tools:sidecars:dev` (or just `bun run dev`) so debug builds are copied here. `ensure-sidecar-placeholders.ps1` only writes a tiny stub when a tool has not been built — it must not copy `cmd.exe`.

For packaging, build the tools, then run `scripts/prepare-sidecars.ps1` (Windows) or `scripts/prepare-sidecars.sh` before `bun run tauri build`.

Required names:

- `omt-studio-monitor-<triple>`
- `omt-test-patterns-<triple>`
- `omt-config-manager-<triple>`
- `omt-discovery-server-gui-<triple>`
- `omt-discovery-server-<triple>`
