# Ensure Tauri externalBin paths exist for the host triple.
# Prefer a real debug/release tool. Never copy cmd.exe — that used to make
# `tauri dev` launch a console instead of Config Manager / Discovery Server.

param(
    [string]$Target = (rustc -vV | Select-String '^host:').ToString().Split(' ')[1]
)

$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root "apps\launcher\src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $Out | Out-Null

$CmdExe = Join-Path $env:WINDIR "System32\cmd.exe"
$CmdLen = if (Test-Path $CmdExe) { (Get-Item $CmdExe).Length } else { 0 }

function Test-CmdClone([string]$Path) {
    if (-not (Test-Path $Path)) { return $false }
    if ($CmdLen -le 0) { return $false }
    return ((Get-Item $Path).Length -eq $CmdLen)
}

function Write-Stub([string]$Path) {
    # Minimal MZ bytes so tauri-build can resolve externalBin during `cargo check`.
    [System.IO.File]::WriteAllBytes($Path, [byte[]](0x4D, 0x5A))
}

function Ensure-One([string]$Name) {
    $dst = Join-Path $Out "$Name-$Target.exe"
    $release = Join-Path $Root "target\release\$Name.exe"
    $debug = Join-Path $Root "target\debug\$Name.exe"

    if ((Test-Path $release) -and -not (Test-CmdClone $release)) {
        Copy-Item $release $dst -Force
        return
    }
    if ((Test-Path $debug) -and -not (Test-CmdClone $debug)) {
        Copy-Item $debug $dst -Force
        return
    }

    if (-not (Test-Path $dst) -or (Test-CmdClone $dst)) {
        Write-Stub $dst
        Write-Warning "sidecar stub for $Name (build the real tool before tauri dev/packaging)"
    }
}

Ensure-One omt-studio-monitor
Ensure-One omt-test-patterns
Ensure-One omt-config-manager
Ensure-One omt-discovery-server-gui
Ensure-One omt-discovery-server
