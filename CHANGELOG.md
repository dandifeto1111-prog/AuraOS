# Historial de Versiones — AuraOS

## v4.3.0 (2026-11-23)
- **Fix pantalla completa / maximizado en toda la suite:** Corregido el bug re-entrante de `RefCell` en `AuraPhotos`, `AuraPlayer` y `AuraNotes` que bloqueaba `WM_SIZE` al maximizar (antes renderizaba en 1/4 de pantalla). Ahora el maximizado y restaurado son instantáneos y fluidos a resolución completa.
- **Coherencia visual total con botones semáforo macOS (`×`, `−`, `+`):** Integrados de forma uniforme en los 4 widgets de escritorio (`Reloj`, `Música`, `Clima`, `Monitor`), `Centro de Control`, `Menú de Energía`, modales, `AuraShell-Setup`, `AuraSettings`, `AuraLock` y todas las apps nativas.
- **Nueva app `AuraSettings.exe`:** Reemplazo nativo ultra-ligero de Configuración de Windows en 100% Rust Direct2D con 8 categorías funcionales reales (Sistema y Energía `powercfg`, Pantalla y cambio real de Hz, Sonido y Mezclador por app, Red/Wi-Fi/Bluetooth/DNS, Personalización en vivo sin reiniciar, Fondos y AuraLock, Apps e Inicio `HKCU\Run`, Actualizaciones GitHub).
- **Nueva pantalla de bloqueo `AuraLock.exe`:** Pantalla de bloqueo GPU a pantalla completa estilo Sonoma/Drool con fondos personalizados (imágenes HD, videos en vivo o Steam Workshop), reloj, clima, mini reproductor SMTC, PIN cifrado o contraseña real de Windows (`LogonUserW`), y atajo global `Ctrl + Alt + L`.
- **Pipeline de GitHub Releases:** Añadidos `.gitignore`, `.github/workflows/release.yml` y `release.ps1` para versionar y compilar lanzamientos automáticos en GitHub.

## v4.2.0
- Fix definitivo del crash al pausar o cambiar música sin canción activa (transporte SMTC 100% MTA fuera del hilo UI).
- Buscadores interactivos corregidos en `Launchpad`, `Overview` y búsqueda recursiva en subcarpetas en `AuraFiles`.
- Delimitación exacta dorada de carpetas en el Cushion Treemap estilo WinDirStat de `AuraFiles`.
- Arranque directo al encender Windows (`HKCU\Run` + `StartupDelayInMSec = 0` + primer frame en `<120 ms`).

## v4.1.0
- Telemetría 100% asíncrona en hilo MTA (`0 ms` de bloqueo al abrir paneles durante juegos o Spotify).
- Fix cierre limpio (`0` crashes `0x0000087a`) y actualización automática silenciosa.
- Restauración de animaciones suaves de arrastre de ventanas en Windows.

## v4.0.0
- Suite nativa Pure-Rust: `AuraPhotos`, `AuraPlayer` y `AuraNotes` + indicador dinámico Ethernet / Wi-Fi / Sin conexión.
