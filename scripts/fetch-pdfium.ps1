# Downloads pdfium.dll for Windows x64 from bblanchon/pdfium-binaries
# Run once after cloning the repo. Produces resources/pdfium/pdfium.dll.

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$DestDir  = Join-Path $RepoRoot "resources\pdfium"
$DestDll  = Join-Path $DestDir  "pdfium.dll"

if (Test-Path $DestDll) {
    Write-Host "pdfium.dll already present at $DestDll"
    exit 0
}

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null

# Pin to a known-good release from bblanchon/pdfium-binaries.
# Update this tag/version as needed.
$ReleaseTag = "chromium/6666"
$AssetName  = "pdfium-win-x64.tgz"
$Url        = "https://github.com/bblanchon/pdfium-binaries/releases/download/$ReleaseTag/$AssetName"

$TempDir  = Join-Path ([System.IO.Path]::GetTempPath()) ("pdfium-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
$Archive  = Join-Path $TempDir $AssetName

Write-Host "Downloading $Url"
Invoke-WebRequest -Uri $Url -OutFile $Archive -UseBasicParsing

Write-Host "Extracting $AssetName"
tar -xzf $Archive -C $TempDir

$Found = Get-ChildItem -Path $TempDir -Recurse -Filter "pdfium.dll" | Select-Object -First 1
if (-not $Found) {
    throw "pdfium.dll not found in archive"
}
Copy-Item $Found.FullName $DestDll -Force
Remove-Item -Recurse -Force $TempDir

Write-Host "pdfium.dll installed at $DestDll"
