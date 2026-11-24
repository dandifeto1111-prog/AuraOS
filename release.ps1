param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$Changelog = "",
    [switch]$Push
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

$cleanVer = $Version.TrimStart('v', 'V')
$tag = "v$cleanVer"

if ([string]::IsNullOrWhiteSpace($Changelog)) {
    $Changelog = "AuraOS $tag — Actualizacion oficial de la suite nativa 100% Rust."
}

Write-Host "=== Preparando lanzamiento AuraOS $tag ===" -ForegroundColor Cyan

# 1. Sync Cargo.toml version
$cargoPath = Join-Path $root "Cargo.toml"
$cargoTxt = Get-Content $cargoPath -Raw
$cargoTxt = [regex]::Replace($cargoTxt, '(?m)^version\s*=\s*"[^"]+"', "version = `"$cleanVer`"", 1)
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($cargoPath, $cargoTxt, $utf8NoBom)

# 2. Build all 7 Rust binaries + AuraShell-Setup.exe
.\build.ps1 -Version $cleanVer -Changelog $Changelog

# 3. Package ZIP + latest.json in dist/
$distDir = Join-Path $root "dist"
$pkgDir = Join-Path $distDir "pkg"
if (Test-Path $pkgDir) { Remove-Item $pkgDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $pkgDir | Out-Null

$bundle = @(
    "AuraShell.exe", "AuraFiles.exe", "AuraPhotos.exe", "AuraPlayer.exe",
    "AuraNotes.exe", "AuraLock.exe", "AuraSettings.exe", "AuraTasks.exe",
    "AuraPDF.exe", "AuraShot.exe", "AuraShell-Setup.exe",
    "restore-windows.ps1", "APBOptimizer.ps1", "version.json"
)
foreach ($f in $bundle) {
    $fp = Join-Path $root $f
    if (Test-Path $fp) { Copy-Item $fp (Join-Path $pkgDir $f) -Force }
}
Get-ChildItem $root -File | Where-Object { $_.Extension -in ".ico", ".png" -and $_.Name -like "Aura*" } | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $pkgDir $_.Name) -Force
}

$zipName = "AuraOS-$tag-Windows-x64.zip"
$zipPath = Join-Path $distDir $zipName
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Compress-Archive -Path "$pkgDir\*" -DestinationPath $zipPath -Force
$hash = (Get-FileHash $zipPath -Algorithm SHA256).Hash

$manifest = [ordered]@{
    Version = $cleanVer
    BuildNumber = [int64](Get-Date -Format "yyyyMMddHHmm")
    ZipFile = $zipName
    Sha256 = $hash
    Changelog = $Changelog
}
[System.IO.File]::WriteAllText((Join-Path $distDir "latest.json"), ($manifest | ConvertTo-Json -Depth 4), $utf8NoBom)
Write-Host "Paquete listo: $zipPath (SHA256: $hash)" -ForegroundColor Green

# 4. Optional git commit, tag & push to trigger GitHub Actions release!
if ($Push) {
    git add -A
    git commit -m "release: AuraOS $tag"
    git tag -a $tag -m "AuraOS $tag"
    git push origin main --tags
    Write-Host "Tag $tag enviado a GitHub — GitHub Actions publicara la release automaticamente." -ForegroundColor Green
}
