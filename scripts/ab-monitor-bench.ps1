# Run headless A/B between egui and GPUI Studio Monitor present paths.
param(
    [Parameter(Mandatory = $true)][string]$Url,
    [int]$Seconds = 10,
    [int]$ConnectTimeout = 15
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

Write-Host "== building release monitors =="
cargo build --release -p omt-studio-monitor -p omt-studio-monitor-gpui

Write-Host "== egui headless =="
& "$Root\target\release\omt-studio-monitor.exe" --headless --url $Url --seconds $Seconds --connect-timeout $ConnectTimeout
$eguiExit = $LASTEXITCODE

Write-Host "== gpui headless =="
& "$Root\target\release\omt-studio-monitor-gpui.exe" --headless --url $Url --seconds $Seconds --connect-timeout $ConnectTimeout
$gpuiExit = $LASTEXITCODE

if ($eguiExit -ne 0 -or $gpuiExit -ne 0) {
    throw "A/B run failed (egui=$eguiExit gpui=$gpuiExit)"
}
