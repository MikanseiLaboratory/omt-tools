# Headless Studio Monitor present-path bench.
param(
    [Parameter(Mandatory = $true)][string]$Url,
    [int]$Seconds = 10,
    [int]$ConnectTimeout = 15
)

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
cargo build --release -p omt-studio-monitor
& "$Root\target\release\omt-studio-monitor.exe" --headless --url $Url --seconds $Seconds --connect-timeout $ConnectTimeout
