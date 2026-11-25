param(
    [string]$Version = "5.2.2",
    [string]$Changelog = "v5.2.2: Estructura limpia (solo AuraShell-Setup.exe fuera de carpetas + app/), instalacion rapida (sin APB) tambien para nuevos usuarios, descarga automatica desde GitHub si se ejecuta Setup suelto, AuraLock limpio, widget de musica 60 FPS y AuraFiles multiseleccion + OLE drag-and-drop."
)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $scriptDir
$appDir = Join-Path $root "app"

Write-Host "Compilando AuraOS Suite v$Version (11 binarios 100% Rust + paquete limpio en app\ + AuraShell-Setup.exe)..." -ForegroundColor Cyan
Set-Location $root
cargo build --release --bin AuraShell --bin AuraWallpaper --bin AuraFiles --bin AuraPhotos --bin AuraPlayer --bin AuraNotes --bin AuraLock --bin AuraSettings --bin AuraTasks --bin AuraPDF --bin AuraShot
if ($LASTEXITCODE -ne 0) { Write-Error "Falló cargo build --release" }

# Place ALL runtime binaries, icons, scripts, and manifest inside clean `app/` subfolder
New-Item -ItemType Directory -Force -Path $appDir | Out-Null

$bins = @(
    "AuraShell.exe", "AuraWallpaper.exe", "AuraFiles.exe", "AuraPhotos.exe", "AuraPlayer.exe",
    "AuraNotes.exe", "AuraLock.exe", "AuraSettings.exe", "AuraTasks.exe",
    "AuraPDF.exe", "AuraShot.exe"
)
foreach ($b in $bins) {
    Copy-Item (Join-Path $root "target\release\$b") (Join-Path $appDir $b) -Force
}

# Copy ALL `.ico` and `*_256.png` companion icons from assets/icons into app/
$iconsDir = Join-Path $root "assets\icons"
if (Test-Path $iconsDir) {
    Get-ChildItem $iconsDir -File | Where-Object { $_.Extension -in ".ico", ".png" } | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $appDir $_.Name) -Force
    }
}

foreach ($asset in @("restore-windows.ps1", "APBOptimizer.ps1")) {
    $src = Join-Path $scriptDir $asset
    if (Test-Path $src) {
        Copy-Item $src (Join-Path $appDir $asset) -Force
    }
}

# Clean any stray binaries/manifests previously left in repo root (only AuraShell-Setup.exe sits outside folders)
Get-ChildItem $root -File | Where-Object {
    ($_.Extension -in ".exe", ".old", ".pdb", ".log" -and $_.Name -ne "AuraShell-Setup.exe") -or $_.Name -eq "version.json"
} | Remove-Item -Force -ErrorAction SilentlyContinue

# Build AuraShell-Setup.exe cleanly in root
$instSrc = Join-Path $root "installer\Installer.cs"
$outSetup = Join-Path $root "AuraShell-Setup.exe"
$shellIco = Join-Path $iconsDir "AuraShell.ico"
$fxDir = "C:\Windows\Microsoft.NET\Framework64\v4.0.30319"
$csc = Join-Path $fxDir "csc.exe"
$wpfDir = Join-Path $fxDir "WPF"
if ((Test-Path $instSrc) -and (Test-Path $csc)) {
    Write-Host "Compilando AuraShell-Setup.exe v$Version..." -ForegroundColor Cyan
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
    SourceDir   = $appDir
    Changelog   = $Changelog
}
[System.IO.File]::WriteAllText((Join-Path $appDir "version.json"), ($meta | ConvertTo-Json -Depth 3), (New-Object System.Text.UTF8Encoding($false)))

$summary = ($bins | ForEach-Object { "{0} ({1} KB)" -f $_, [math]::Round((Get-Item (Join-Path $appDir $_)).Length / 1KB, 1) }) + @(
    "AuraShell-Setup.exe ({0} KB)" -f [math]::Round((Get-Item $outSetup).Length / 1KB, 1)
)
Write-Host ("Compilación v{0} lista (root limpio -> solo AuraShell-Setup.exe + app\): {1}" -f $Version, ($summary -join " | ")) -ForegroundColor Green
