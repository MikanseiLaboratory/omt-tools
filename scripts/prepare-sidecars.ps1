# Copy release tool binaries into the Tauri sidecar folder with target-triple suffixes.
param(
    [string]$Target = (rustc -vV | Select-String '^host:').ToString().Split(' ')[1]
)

$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root "apps\launcher\src-tauri\binaries"
$ProfileDir = Join-Path $Root "target\release"
New-Item -ItemType Directory -Force -Path $Out | Out-Null

function Copy-One([string]$Name) {
    $src = Join-Path $ProfileDir "$Name.exe"
    if (-not (Test-Path $src)) { throw "missing $src — build with cargo build --release -p $Name first" }
    $dst = Join-Path $Out "$Name-$Target.exe"
    Copy-Item $src $dst -Force
    Write-Host "prepared $Name for $Target"
}

Copy-One omt-studio-monitor
Copy-One omt-test-patterns
