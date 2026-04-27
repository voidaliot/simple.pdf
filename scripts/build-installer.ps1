# Build the simple.pdf NSIS installer.
# Produces target\release\bundle\nsis\simple.pdf_*_x64-setup.exe.
#
# Prerequisites: Rust toolchain, Node/pnpm, pdfium.dll in resources\pdfium\,
#                NSIS 3.x installed and on PATH.
#
# Usage (from repo root or scripts\):
#   .\scripts\build-installer.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

Write-Host "=== simple.pdf — NSIS installer build ===" -ForegroundColor Cyan

Push-Location (Join-Path $Root "crates\app")
try {
    Write-Host "[1/2] Building NSIS installer…"
    cargo tauri build --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "cargo tauri build failed" }
} finally {
    Pop-Location
}

$Bundle = Join-Path $Root "target\release\bundle\nsis"
$Installer = Get-ChildItem -Path $Bundle -Filter "*-setup.exe" | Select-Object -First 1

if ($Installer) {
    $SizeMB = [math]::Round($Installer.Length / 1MB, 2)
    Write-Host "[2/2] Done!" -ForegroundColor Green
    Write-Host "  Output : $($Installer.FullName)"
    Write-Host "  Size   : ${SizeMB} MB"
} else {
    Write-Warning "Installer produced but not found in $Bundle"
}
