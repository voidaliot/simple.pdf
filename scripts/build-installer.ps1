# Build the simple.pdf NSIS installer.
# Produces target\x86_64-pc-windows-msvc\release\bundle\nsis\simple.pdf_*_x64-setup.exe.
#
# Prerequisites: Rust toolchain, Node/pnpm, pdfium.dll in resources\pdfium\,
#                NSIS 3.x installed and on PATH.
#
# Usage (from repo root or scripts\):
#   .\scripts\build-installer.ps1

param(
    [string]$CertificateThumbprint = $env:SIMPLE_PDF_CERTIFICATE_THUMBPRINT,
    [string]$TimestampUrl = $env:SIMPLE_PDF_TIMESTAMP_URL
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

Write-Host "=== simple.pdf - NSIS installer build ===" -ForegroundColor Cyan

Push-Location (Join-Path $Root "crates\app")
try {
    Write-Host "[1/2] Building NSIS installer..."
    $TauriArguments = @("build", "--bundles", "nsis")
    if ($CertificateThumbprint) {
        if (-not $TimestampUrl) {
            throw "SIMPLE_PDF_TIMESTAMP_URL is required when signing is enabled"
        }
        $SigningConfig = @{
            bundle = @{
                windows = @{
                    certificateThumbprint = $CertificateThumbprint
                    digestAlgorithm = "sha256"
                    timestampUrl = $TimestampUrl
                }
            }
        } | ConvertTo-Json -Compress -Depth 4
        $TauriArguments += @("--config", $SigningConfig)
        Write-Host "  Authenticode signing enabled" -ForegroundColor Green
    } else {
        Write-Warning "No trusted certificate configured; the executable and installer will be unsigned"
    }
    node ../../frontend/node_modules/@tauri-apps/cli/tauri.js @TauriArguments
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed" }
} finally {
    Pop-Location
}

$Bundle = Join-Path $Root "target\x86_64-pc-windows-msvc\release\bundle\nsis"
$Version = (Get-Content (Join-Path $Root "crates\app\tauri.conf.json") -Raw | ConvertFrom-Json).version
$Installer = Get-ChildItem -Path $Bundle -Filter "*_${Version}_*-setup.exe" |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1

if ($Installer) {
    $SizeMB = [math]::Round($Installer.Length / 1MB, 2)
    Write-Host "[2/2] Done!" -ForegroundColor Green
    Write-Host "  Output : $($Installer.FullName)"
    Write-Host "  Size   : ${SizeMB} MB"
} else {
    Write-Warning "Version $Version installer produced but not found in $Bundle"
}
