# Build the simple.pdf portable distribution.
# Produces dist\simple.pdf-portable.zip (<= 20 MB target).
#
# Prerequisites: Rust toolchain, Node/pnpm, pdfium.dll in resources\pdfium\
#
# Usage (from repo root or scripts\):
#   .\scripts\build-portable.ps1

param(
    [string]$CertificateThumbprint = $env:SIMPLE_PDF_CERTIFICATE_THUMBPRINT,
    [string]$TimestampUrl = $env:SIMPLE_PDF_TIMESTAMP_URL
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

Write-Host "=== simple.pdf - portable build ===" -ForegroundColor Cyan

# 1. Build (Tauri handles frontend build via beforeBuildCommand)
Push-Location (Join-Path $Root "crates\app")
try {
    Write-Host "[1/4] Building release binary (no bundle)..."
    $TauriArguments = @("build", "--no-bundle")
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
        Write-Warning "No trusted certificate configured; the portable executable will be unsigned"
    }
    node ../../frontend/node_modules/@tauri-apps/cli/tauri.js @TauriArguments
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed" }
} finally {
    Pop-Location
}

# 2. Locate artifacts
$ExeSrc  = Join-Path $Root "target\x86_64-pc-windows-msvc\release\simple-pdf.exe"
$DllSrc  = Join-Path $Root "resources\pdfium\pdfium.dll"

if (-not (Test-Path $ExeSrc)) { throw "Exe not found: $ExeSrc" }
if (-not (Test-Path $DllSrc)) { throw "pdfium.dll not found: $DllSrc" }

# 3. Stage portable directory
$DistDir = Join-Path $Root "dist\portable"
if (Test-Path $DistDir) { Remove-Item -Recurse -Force $DistDir }
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

Write-Host "[2/4] Staging files in dist\portable..."
Copy-Item $ExeSrc (Join-Path $DistDir "simple.pdf.exe")
Copy-Item $DllSrc (Join-Path $DistDir "pdfium.dll")

# portable.txt signals the app to use ./data/ instead of %APPDATA%\simple.pdf
Set-Content -Path (Join-Path $DistDir "portable.txt") -Value "" -Encoding ascii

# 4. Create zip
$ZipPath = Join-Path $Root "dist\simple.pdf-portable.zip"
if (Test-Path $ZipPath) { Remove-Item -Force $ZipPath }

Write-Host "[3/4] Compressing..."
Compress-Archive -Path "$DistDir\*" -DestinationPath $ZipPath

# 5. Report
$SizeMB = [math]::Round((Get-Item $ZipPath).Length / 1MB, 2)
Write-Host "[4/4] Done!" -ForegroundColor Green
Write-Host "  Output : $ZipPath"
Write-Host "  Size   : ${SizeMB} MB  (target <= 20 MB)"

if ($SizeMB -gt 20) {
    Write-Warning "Size exceeds 20 MB target (NFR-SIZE-01)."
}
