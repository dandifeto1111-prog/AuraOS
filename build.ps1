param(
    [string]$Version = "4.4.0",
    [string]$Changelog = "v4.4.0: Redimensionado nativo en bordes/esquinas + Aero Snap Windows 11 (mitad izquierda/derecha, 4 esquinas 25%, Win+Flechas, Alt+Flechas, clic derecho en +) en todas las apps AuraOS, cuadricula magnetica inteligente con auto-acoplamiento para widgets de escritorio, y renderizado sin estiramiento (proporcion original exacta) para imagenes verticales 9:16 en AuraFiles y todos los fondos de pantalla."
)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$legacy = Join-Path (Split-Path -Parent $root) "AuraShell"

Write-Host "Compilando AuraOS Suite v$Version (7 binarios 100% Rust + AuraShell-Setup)..." -ForegroundColor Cyan
Set-Location $root
cargo build --release --bin AuraShell --bin AuraFiles --bin AuraPhotos --bin AuraPlayer --bin AuraNotes --bin AuraLock --bin AuraSettings
if ($LASTEXITCODE -ne 0) { Write-Error "Falló cargo build --release" }

$bins = @("AuraShell.exe", "AuraFiles.exe", "AuraPhotos.exe", "AuraPlayer.exe", "AuraNotes.exe", "AuraLock.exe", "AuraSettings.exe")
foreach ($b in $bins) {
    Copy-Item (Join-Path $root "target\release\$b") (Join-Path $root $b) -Force
}

# Sync HD icons and safe restore script if missing
foreach ($asset in @("AuraShell.ico", "AuraFiles.ico", "AuraShell_256.png", "AuraFiles_256.png", "restore-windows.ps1")) {
    $src = Join-Path $legacy $asset
    $dst = Join-Path $root $asset
    if ((Test-Path $src) -and (-not (Test-Path $dst))) {
        Copy-Item $src $dst -Force
    }
}

# Build upgraded AuraShell-Setup.exe (v4.0 Suite + APBOptimizer + HKCU file associations!)
$instSrc = Join-Path $root "installer\Installer.cs"
$outSetup = Join-Path $root "AuraShell-Setup.exe"
$shellIco = Join-Path $root "AuraShell.ico"
$fxDir = "C:\Windows\Microsoft.NET\Framework64\v4.0.30319"
$csc = Join-Path $fxDir "csc.exe"
$wpfDir = Join-Path $fxDir "WPF"
if ((Test-Path $instSrc) -and (Test-Path $csc)) {
    Write-Host "Compilando AuraShell-Setup.exe v$Version (Suite AuraOS + APBOptimizer)..." -ForegroundColor Cyan
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
