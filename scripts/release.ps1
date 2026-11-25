param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$Changelog = "",
    [switch]$Push
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $scriptDir
Set-Location $root

$cleanVer = $Version.TrimStart('v', 'V')
$tag = "v$cleanVer"

if ([string]::IsNullOrWhiteSpace($Changelog)) {
    $Changelog = "AuraOS $tag — Actualizacion oficial de la suite nativa 100% Rust."
}

Write-Host "=== Preparando lanzamiento AuraOS $tag ===" -ForegroundColor Cyan

# 1. Sync Cargo.toml [package] version safely (only the package section!)
$cargoPath = Join-Path $root "Cargo.toml"
$cargoTxt = Get-Content $cargoPath -Raw
$rxPkg = [regex]::new('(?ms)(\[package\].*?^version\s*=\s*)"[^"]+"')
$cargoTxt = $rxPkg.Replace($cargoTxt, "`${1}`"$cleanVer`"", 1)
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($cargoPath, $cargoTxt, $utf8NoBom)

# 2. Build all 11 Rust binaries into app/ + AuraShell-Setup.exe in root
& (Join-Path $scriptDir "build.ps1") -Version $cleanVer -Changelog $Changelog

# 3. Package organized ZIP in dist/ (ONLY AuraShell-Setup.exe outside + everything inside app/)
$distDir = Join-Path $root "dist"
$pkgDir = Join-Path $distDir "pkg"
$pkgAppDir = Join-Path $pkgDir "app"
if (Test-Path $pkgDir) { Remove-Item $pkgDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $pkgAppDir | Out-Null

Copy-Item (Join-Path $root "AuraShell-Setup.exe") (Join-Path $pkgDir "AuraShell-Setup.exe") -Force
Copy-Item (Join-Path $root "app\*") $pkgAppDir -Recurse -Force

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
