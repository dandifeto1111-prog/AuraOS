# Historial de Versiones — AuraOS

## v5.0.0 (2026-11-23)
- **`AuraPDF Studio` (`AuraPDF.exe` + Vista Previa en `AuraFiles`):** Visor, previsualizador estilo macOS Quick-Look y editor de documentos PDF 100% en Rust + `Windows.Data.Pdf` + Direct2D (cero librerías externas C/C++). Incluye miniaturas laterales, zoom fluido, Modo Lectura Nocturna, rotación 90°, reordenamiento y eliminación de páginas, combinación de PDFs (`+ Combinar`), anotaciones vectoriales en vivo (**Resaltador**, **Pluma / Firma**, **Recuadro**, **Notas de texto**) y exportación a `.pdf` estándar compatible con todos los lectores.
- **Vista Rápida Quick-Look con `Espacio` y Previsualizador PDF en `AuraFiles`:** Al seleccionar cualquier `.pdf` en `AuraFiles` se renderiza su página 1 en alta definición + conteo de páginas en el panel derecho, y al pulsar la barra espaciadora se abre un modal Quick-Look estilo macOS para imágenes, PDFs y código/texto.
- **`AuraPhotos Studio` (`AuraPhotos.exe`):** Evolucionado de visor a **Visor + Estudio de Edición GPU en tiempo real**: deslizadores no destructivos de **Brillo, Contraste, Saturación y Calidez**, filtros 1-clic (`Auto`, `B&N`, `Sepia`), **Recorte con proporciones** (`Libre`, `1:1`, `16:9`, `9:16`), **Volteo H/V**, **Pincel / Flecha / Recuadro / Resaltador**, **Pixelado / Censura de privacidad** y exportación directa a PNG/JPG o portapapeles (`CF_DIB`).
- **`AuraShot` (`AuraShot.exe` — Capturas y Anotación en `<25 ms`):** Reemplazo instantáneo de Recortes de Windows (`Impr Pant` / `Win + Shift + S` / Centro de Control) con congelamiento nativo de pantalla, recorte por región o pantalla completa, flechas, recuadros, resaltador, pixelado, copia en 1 clic al portapapeles y guardado automático en `Pictures\AuraShots\`.
- **Integración Triple `agent-desktop` + `AuraAgent.exe` (Cursor Autónomo IA):** Puente nativo JSON (`agent_bridge.rs` en `%LocalAppData%\AuraShell\ui_trees\`), integración CLI con `vercel-labs/agent-desktop` y cursor fantasma animado DirectComposition (`AuraAICursor`) capaz de inspeccionar y auditar todo el escritorio en segundo plano sin robar el ratón físico.
- **Utilidades diarias macOS/Windows integradas en `AuraShell` y `AuraSettings`:**
  - **Calendario Mensual + Temporizador Pomodoro (`25m / 5m`)** al hacer clic en el reloj superior.
  - **Historial de Portapapeles `AuraClip` (`Win + V`)** con fijado de favoritos (`★`) y auto-pegado.
  - **Calculadora Matemática, Porcentajes y Conversor de Unidades** instantáneos en `Alt + Espacio` / `Win + S` + ejecutor `> comando` + **OSD flotante de volumen**.
  - **Desinstalador Rápido de Programas de Windows** en `AuraSettings → Apps e Inicio`.
  - **11 iconos HD de 256 px (`_256.png` + `.ico` COFF `.rsrc` + `AppUserModelID`)** garantizados en todas las ventanas, Dock, Launchpad, Alt+Tab y Menú Inicio, más apertura instantánea `0 ms` por teclado (`emil-design-eng`).

## v4.5.0 (2026-11-23)
- **Fondos de pantalla de video 100% nativos en cualquier PC sin FFmpeg:** Nuevo motor `video_decoder.rs` con decodificación nativa **Windows Media Foundation (`IMFSourceReader`)** (0 dependencias externas, funciona directo en cualquier Windows 10/11 limpio con `.mp4`, `.m4v`, `.mov`, `.wmv`, `.avi`), fallback automático a `ffmpeg.exe` si existe y fallback universal HTML5 `<video>` en `WorkerW` para codecs exóticos VP9/AV1, más extracción automática de miniaturas para videos personalizados.
- **Geolocalización automática de Clima y Reloj (cero ciudades fijas):** Eliminadas las coordenadas fijas de Barranquilla; ahora detecta automáticamente la ciudad real del usuario en todo el mundo (Bogotá, Medellín, Madrid, CDMX, etc.) mediante múltiples proveedores WinHTTP + Open-Meteo, caché local y opción de ciudad manual en `AuraSettings`.
- **Nueva app `AuraTasks.exe` (Administrador de Tareas propio):** Más completo y ~5× más ligero (~14 MB RAM) que el de Windows, con 5 módulos: **Procesos** (CPU% real por proceso, RAM, Finalizar árbol, Modo Eco real, Suspender/Reanudar, Prioridad, Liberar RAM, Abrir ruta en `AuraFiles`), **Rendimiento** (gráficas 60s de CPU, RAM, Red Bajada/Subida, GPU DXGI y Discos), **Red y Puertos TCP nativos** (cierra procesos por puerto), **Inicio** y **Servicios de Windows nativos (SCM)**.
- **Atajos nativos de Windows integrados con AuraOS (`hotkey_hook.rs`):** Hook ligero `WH_KEYBOARD_LL` (0 inyección DLL, 100% seguro con anti-cheats, desactivable en `AuraSettings` y auto-pausado en juegos): tecla **`Win`** sola → abre **Launchpad de aplicaciones**, **`Ctrl + Shift + Esc`** → abre **`AuraTasks.exe`**, **`Win + E`** → **`AuraFiles.exe`**, **`Win + I`** → **`AuraSettings.exe`**, **`Win + S`** → **Overview** (conservando intactos `Win+Flechas`, `Win+D`, `Win+Shift+S`, `Win+V`, `Win+Tab`, `Win+L`).
- **Foto de perfil y contraseña de seguridad en `AuraLock`:** Avatar circular recortado en GPU (`lock_avatar_path`) y contraseña o PIN completo de seguridad configurables en 1 clic desde `AuraSettings → Fondos y AuraLock`.
- **Iconos HD nativos incrustados en los 8 ejecutables (`build.rs`):** Compilador Win32 `.res` en Rust puro + `WM_SETICON` para que `AuraShell`, `AuraFiles`, `AuraSettings`, `AuraPhotos`, `AuraPlayer`, `AuraNotes`, `AuraLock` y `AuraTasks` luzcan su icono squircle propio en Explorador, Alt+Tab, Barra, Menú Inicio y Dock.
- **Efectos visuales estilo macOS en el Dock:** Magnificación parabólica progresiva al pasar el mouse, **vista previa en vivo 60 FPS (`DwmRegisterThumbnail`)** al posar el cursor sobre apps abiertas y **efecto Genio** de succión/des-succión GPU de 36 bandas al minimizar y restaurar ventanas.
- **Optimización profunda para PCs modestos:** Frame pacing adaptativo (`16 ms` activo → `160 ms` en reposo, -90% despertares CPU), dirty-tracking inteligente de TopBar y widgets, caché DWM en `window_styler`, doble búfer sin asignaciones para video y **Modo Rendimiento** auto-detectable para equipos ligeros.

## v4.4.0 (2026-11-23)
- **Gestión completa de ventanas estilo Windows 11 en toda la suite (`AuraFiles`, `AuraPhotos`, `AuraPlayer`, `AuraNotes`, `AuraSettings`):** Nuevo módulo compartido `aura_window` con `WS_THICKFRAME` + `WM_NCCALCSIZE=0` + `DWMWA_COLOR_NONE`: redimensionado fluido desde los 4 bordes y 4 esquinas, cursores nativos, Aero Snap nativo (mitad izquierda/derecha, 4 esquinas 25%, maximizado arriba, `Win + Flechas`, `Alt + Flechas` y ciclo de zonas con clic derecho en `+`).
- **Cuadrícula magnética inteligente para Widgets de escritorio:** Al arrastrar un widget cerca de otro (26 px), se acopla automáticamente encima, debajo o al lado con espacio uniforme de 12 px, alinea bordes/centros, muestra brillo violeta `"Acoplado"`, evita superposiciones al soltar y se despega libremente al arrastrarlo más allá del imán; incluye botón **Ordenar widgets en cuadrícula**.
- **Imágenes y fondos sin deformación (0% estiramiento):** `AuraFiles` ahora renderiza la vista previa con relación de aspecto original exacta (`Contain`, perfecto para imágenes verticales 9:16), y todos los fondos de pantalla (estáticos, video y miniaturas Workshop/AuraLock/AuraSettings) usan `Cover` centrado sin estirar ningún píxel.

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
