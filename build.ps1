param(
    [string]$Version = "5.2.1",
    [string]$Changelog = "v5.2.1: Hub creativo nativo Aura Studio (PC | Dev | Gamer | Studio | Fondos) integrado con AuraBeats DAW (fork GPL-3.0 de Lemon-Studio en Rust con tema Aura Obsidian Violet, semaforos macOS, 28 plugins DSP, VST3/VST2/CLAP, WASAPI+ASIO y apertura directa de proyectos .flp/.lsp desde AuraFiles), registro automatico en Launchpad/Spotlight/Mezclador de Audio y espacio reservado para el proximo Editor de Video."
)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$legacy = Join-Path (Split-Path -Parent $root) "AuraShell"

Write-Host "Compilando AuraOS Suite v$Version (11 binarios 100% Rust con motor aislado AuraWallpaper + AuraShell-Setup)..." -ForegroundColor Cyan
Set-Location $root
cargo build --release --bin AuraShell --bin AuraWallpaper --bin AuraFiles --bin AuraPhotos --bin AuraPlayer --bin AuraNotes --bin AuraLock --bin AuraSettings --bin AuraTasks --bin AuraPDF --bin AuraShot
if ($LASTEXITCODE -ne 0) { Write-Error "Falló cargo build --release" }

$bins = @(
    "AuraShell.exe", "AuraWallpaper.exe", "AuraFiles.exe", "AuraPhotos.exe", "AuraPlayer.exe",
    "AuraNotes.exe", "AuraLock.exe", "AuraSettings.exe", "AuraTasks.exe",
    "AuraPDF.exe", "AuraShot.exe"
)
foreach ($b in $bins) {
    Copy-Item (Join-Path $root "target\release\$b") (Join-Path $root $b) -Force
}

# Copy ALL 11 `.ico` and `*_256.png` companion icons from assets/icons into root so Setup/Update ships 100% of icons!
$iconsDir = Join-Path $root "assets\icons"
if (Test-Path $iconsDir) {
    Get-ChildItem $iconsDir -File | Where-Object { $_.Extension -in ".ico", ".png" } | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $root $_.Name) -Force
    }
}

foreach ($asset in @("restore-windows.ps1")) {
    $src = Join-Path $legacy $asset
    $dst = Join-Path $root $asset
    if ((Test-Path $src) -and (-not (Test-Path $dst))) {
        Copy-Item $src $dst -Force
    }
}

# Build upgraded AuraShell-Setup.exe (v5.0 Complete Suite + Icons + Associations + APBOptimizer!)
$instSrc = Join-Path $root "installer\Installer.cs"
$outSetup = Join-Path $root "AuraShell-Setup.exe"
$shellIco = Join-Path $root "AuraShell.ico"
$fxDir = "C:\Windows\Microsoft.NET\Framework64\v4.0.30319"
$csc = Join-Path $fxDir "csc.exe"
$wpfDir = Join-Path $fxDir "WPF"
if ((Test-Path $instSrc) -and (Test-Path $csc)) {
    Write-Host "Compilando AuraShell-Setup.exe v$Version (Suite AuraOS v5.0)..." -ForegroundColor Cyan
    $refs = @(
        "/r:$wpfDir\PresentationCore.dll",
        "/r:$wpfDir\PresentationFramework.dll",
        "/r:$wpfDir\WindowsBase.dll",
        "/r:$fxDir\System.Xaml.dll",
        "/r:$fxDir\System.Drawing.dll",
        "/r:$fxDir\System.Windows.Forms.dll",
        "/r:$fxDir\System.Web.Extensions.dll",
        "/r:$fxDir\Microsoft.CSharp.dll"
    )
    $argsSetup = @("/target:winexe", "/optimize+", "/nologo", "/utf8output", "/out:$outSetup", "/win32icon:$shellIco") + $refs + @($instSrc)
    & $csc @argsSetup
    if ($LASTEXITCODE -ne 0) { Write-Error "Falló la compilación de AuraShell-Setup.exe" }
}

$now = Get-Date
$meta = [ordered]@{
    Version     = $Version
    BuildNumber = [long]$now.ToString("yyyyMMddHHmmss")
    BuildTime   = $now.ToString("yyyy-MM-dd HH:mm")
    SourceDir   = $root
    Changelog   = $Changelog
}
[System.IO.File]::WriteAllText((Join-Path $root "version.json"), ($meta | ConvertTo-Json -Depth 3), (New-Object System.Text.UTF8Encoding($false)))

$summary = ($bins + @("AuraShell-Setup.exe")) | ForEach-Object {
    "{0} ({1} KB)" -f $_, [math]::Round((Get-Item (Join-Path $root $_)).Length / 1KB, 1)
}
Write-Host ("Compilación v{0} lista: {1}" -f $Version, ($summary -join " | ")) -ForegroundColor Green
