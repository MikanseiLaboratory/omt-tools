# Prefetch Tauri's NSIS 3.11 toolchain into %LOCALAPPDATA%\tauri\NSIS.
# tauri-bundler downloads this with attohttpc (no retries) and CI often fails with
# `io: Peer disconnected` while fetching nsis-3.11.zip from GitHub.

$ErrorActionPreference = "Stop"

$tools = Join-Path $env:LOCALAPPDATA "tauri"
$nsis = Join-Path $tools "NSIS"
$dllRel = "Plugins\x86-unicode\additional\nsis_tauri_utils.dll"
$dllPath = Join-Path $nsis $dllRel

# Hashes match @tauri-apps/cli 2.11.x (tauri-bundler nsis/mod.rs).
$nsisUrl = "https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip"
$nsisSha1 = "EF7FF767E5CBD9EDD22ADD3A32C9B8F4500BB10D"
$dllUrl = "https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll"
$dllSha1 = "75197FEE3C6A814FE035788D1C34EAD39349B860"

function Get-Sha1([string]$Path) {
    (Get-FileHash -Algorithm SHA1 -Path $Path).Hash
}

function Invoke-Download([string]$Url, [string]$Out, [string]$ExpectedSha1) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
    $attempt = 0
    while ($true) {
        $attempt++
        if (Test-Path $Out) { Remove-Item $Out -Force }
        Write-Host "Downloading $Url (attempt $attempt)"
        & curl.exe -L --fail --retry 5 --retry-all-errors --retry-delay 5 --connect-timeout 30 -o $Out $Url
        if ($LASTEXITCODE -eq 0 -and (Test-Path $Out)) {
            $actual = Get-Sha1 $Out
            if ($actual -eq $ExpectedSha1) { return }
            Write-Warning "SHA1 mismatch for $Url (got $actual, want $ExpectedSha1)"
        }
        if ($attempt -ge 4) {
            throw "failed to download $Url after $attempt attempts"
        }
        Start-Sleep -Seconds (10 * $attempt)
    }
}

$makensis = Join-Path $nsis "makensis.exe"
if ((Test-Path $makensis) -and (Test-Path $dllPath)) {
    Write-Host "NSIS already present at $nsis"
    exit 0
}

New-Item -ItemType Directory -Force -Path $tools | Out-Null
$zip = Join-Path $env:TEMP "nsis-3.11.zip"
Invoke-Download $nsisUrl $zip $nsisSha1

if (Test-Path $nsis) { Remove-Item $nsis -Recurse -Force }
$extract = Join-Path $env:TEMP "nsis-extract"
if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
New-Item -ItemType Directory -Force -Path $extract | Out-Null
Expand-Archive -Path $zip -DestinationPath $extract -Force
$extracted = Join-Path $extract "nsis-3.11"
if (-not (Test-Path $extracted)) {
    throw "zip did not contain nsis-3.11"
}
Move-Item $extracted $nsis

$dllTmp = Join-Path $env:TEMP "nsis_tauri_utils.dll"
Invoke-Download $dllUrl $dllTmp $dllSha1
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dllPath) | Out-Null
Copy-Item $dllTmp $dllPath -Force
Write-Host "NSIS ready at $nsis"
