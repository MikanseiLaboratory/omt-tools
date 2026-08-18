# Ensure Tauri externalBin placeholders exist for the host triple.
# Real binaries should be produced via prepare-sidecars after a release build.

param(
    [string]$Target = (rustc -vV | Select-String '^host:').ToString().Split(' ')[1]
)

$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root "apps\launcher\src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $Out | Out-Null

function Ensure-One([string]$Name) {
    $dst = Join-Path $Out "$Name-$Target.exe"
    if (Test-Path $dst) { return }
    $release = Join-Path $Root "target\release\$Name.exe"
    $debug = Join-Path $Root "target\debug\$Name.exe"
    if (Test-Path $release) {
        Copy-Item $release $dst -Force
    } elseif (Test-Path $debug) {
        Copy-Item $debug $dst -Force
    } else {
        Copy-Item "$env:WINDIR\System32\cmd.exe" $dst -Force
        Write-Warning "placeholder sidecar for $Name (build the real tool before packaging)"
    }
}

Ensure-One omt-studio-monitor
Ensure-One omt-test-patterns
Ensure-One omt-config-manager
Ensure-One omt-discovery-server-gui
Ensure-One omt-discovery-server
