<div align="center">
  <img src="AuraShell_256.png" alt="AuraOS Logo" width="112" />
  <h1>AuraOS</h1>
  <p><strong>Un entorno de escritorio híbrido 100% nativo en Rust + Direct2D sobre Windows 11.</strong></p>
  <p>Conserva toda la compatibilidad de juegos, DirectX, drivers y anti-cheats de Windows (Riot Vanguard, Steam, Epic), mientras reemplaza la interfaz pesada por una suite ultrarrápida estilo <em>Drool Dark Mode</em>.</p>

  <p>
    <a href="https://github.com/dandifeto1111-prog/AuraOS/releases/latest"><img src="https://img.shields.io/badge/Release-v5.0.0-7C5CFF?style=for-the-badge&logo=github" alt="Release v5.0.0" /></a>
    <img src="https://img.shields.io/badge/100%25_Rust-Direct2D_%2B_DirectComposition-F4727F?style=for-the-badge&logo=rust" alt="100% Rust" />
    <img src="https://img.shields.io/badge/OS-Windows_11_x64-34D399?style=for-the-badge&logo=windows11" alt="Windows 11" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-FBBF24?style=for-the-badge" alt="MIT License" /></a>
  </p>
</div>

---

## ✨ ¿Qué es AuraOS?

**AuraOS** nace con una idea clara: combinar la base sólida y compatibilidad total de **Windows 11** con una experiencia visual completamente nueva, fluida y ligera escrita en **Rust puro** (sin Electron, sin Chromium y sin procesos WebView pesados).

Cada aplicación del sistema está acelerada por hardware con **Direct2D y DirectComposition**, ofrece **gestión completa de ventanas estilo Windows 11** (redimensionado libre por bordes/esquinas, Aero Snap al arrastrar a bordes, `Win + Flechas` y `Alt + Flechas`), **botones semáforo unificados (`×`, `−`, `+`)**, **widgets de escritorio con cuadrícula magnética**, **imágenes y fondos siempre en su proporción real sin estirarse** y respeto total por la estabilidad de tu equipo (reversible al instante con `Ctrl + Alt + W` o desde el desinstalador).

---

## 🖼️ Galería del Sistema (Capturas Reales v4.5.0)

### 🌌 Escritorio, Barra Superior, Dock macOS (Magnificación + Vista Previa en Vivo + Efecto Genio) y Widgets Magnéticos
Barra superior de 34 px con controles activos, telemetría en vivo (CPU, RAM, clima geolocalizado automáticamente según tu ciudad, Ethernet/Wi-Fi dinámico), Dock con **magnificación parabólica estilo macOS**, **vista previa DWM 60 FPS en vivo** al pasar el cursor, **efecto Genio al minimizar/abrir ventanas** y widgets arrastrables con cuadrícula magnética.

<p align="center">
  <img src="docs/screenshots/desktop_widgets.png" alt="Escritorio y Widgets de AuraOS" width="100%" />
</p>

### 🔒 AuraLock — Pantalla de Bloqueo GPU con Fondos Personalizados
Pantalla de bloqueo a pantalla completa con reloj estilo Sonoma, fecha, clima en vivo, mini reproductor SMTC, selector instantáneo de fondos (imagen HD, video animado o Steam Workshop) y desbloqueo por PIN cifrado o contraseña real de Windows (`LogonUserW`). Atajo global: **`Ctrl + Alt + L`**.

<p align="center">
  <img src="docs/screenshots/auralock.png" alt="Pantalla de bloqueo AuraLock" width="100%" />
</p>

### ⚙️ AuraSettings — Reemplazo Nativo y Ligero de Configuración de Windows
Panel completo en Rust puro (~15 MB RAM) con 8 categorías reales: **Sistema y Energía** (`powercfg`), **Pantalla** (cambio real de Hz en vivo), **Sonido y Mezclador por App**, **Red / Wi-Fi / Bluetooth / DNS**, **Personalización en vivo sin reiniciar**, **Fondos y AuraLock**, **Gestor de Inicio (`HKCU\Run`)** y **Actualizaciones GitHub**.

<table>
  <tr>
    <td><img src="docs/screenshots/aurasettings_system.png" alt="AuraSettings Sistema y Energía" /></td>
    <td><img src="docs/screenshots/aurasettings_lock.png" alt="AuraSettings Fondos y AuraLock" /></td>
  </tr>
  <tr>
    <td align="center"><sub><strong>Sistema y Energía:</strong> CPU/RAM, planes <code>powercfg</code> reales, discos y limpieza segura de <code>%TEMP%</code></sub></td>
    <td align="center"><sub><strong>Fondos y AuraLock:</strong> Foto de perfil circular, contraseña/PIN seguro, videos nativos Media Foundation y galería compartida</sub></td>
  </tr>
</table>

### 📊 AuraTasks — Administrador de Tareas Propio (`Ctrl + Shift + Esc`)
Más completo y ~5× más ligero que el Administrador de Tareas clásico: gestión de **Procesos** (CPU% real por proceso, RAM, Finalizar árbol, Modo Eco, Suspender/Reanudar, Prioridad, Liberar RAM, Abrir ruta en `AuraFiles`), **Rendimiento** (gráficas 60s de CPU, RAM, Red Bajada/Subida, GPU DXGI y Discos), **Red y Puertos TCP nativos**, **Inicio** y **Servicios de Windows (SCM)**.

<table>
  <tr>
    <td><img src="docs/screenshots/auratasks_procs.png" alt="AuraTasks Procesos" /></td>
    <td><img src="docs/screenshots/auratasks_perf.png" alt="AuraTasks Rendimiento en Vivo" /></td>
  </tr>
  <tr>
    <td align="center"><sub><strong>Procesos:</strong> Búsqueda instantánea, filtros, Modo Eco, Suspender/Reanudar, Finalizar árbol y Liberar RAM</sub></td>
    <td align="center"><sub><strong>Rendimiento:</strong> Gráficas 60s de CPU, RAM, Red Bajada/Subida, GPU DXGI y análisis directo de discos</sub></td>
  </tr>
</table>

### 🎛️ Centro de Control & 📁 AuraFiles con Analizador de Disco tipo WinDirStat
Control de audio maestro, cambio de salida/micrófono en 1 clic, mezclador por aplicación en tiempo real y explorador de archivos con pestañas, búsqueda recursiva en subcarpetas y **mapa de calor Cushion Treemap** que delimita en dorado el área exacta de cada carpeta.

<table>
  <tr>
    <td width="35%"><img src="docs/screenshots/control_center.png" alt="Centro de Control AuraOS" /></td>
    <td width="65%"><img src="docs/screenshots/aurafiles_analyzer.png" alt="AuraFiles Analizador WinDirStat" /></td>
  </tr>
  <tr>
    <td align="center"><sub><strong>Centro de Control:</strong> Reproductor SMTC, mezclador Core Audio por app y controles rápidos</sub></td>
    <td align="center"><sub><strong>AuraFiles v4.4:</strong> Escaneo paralelo (26,000+ archivos en 0.4 s), contorno dorado por carpeta y vista previa 1:1 sin estiramiento</sub></td>
  </tr>
</table>

### 🚀 Launchpad de Aplicaciones & 🎨 Motor de Fondos (Compatible con Steam Workshop)

<table>
  <tr>
    <td><img src="docs/screenshots/launchpad.png" alt="Launchpad de Aplicaciones" /></td>
    <td><img src="docs/screenshots/wallpaper_studio.png" alt="Estudio de Fondos y Widgets" /></td>
  </tr>
  <tr>
    <td align="center"><sub><strong>Launchpad:</strong> Todas tus apps y juegos con buscador instantáneo y apertura con <code>Enter</code></sub></td>
    <td align="center"><sub><strong>Fondos y Widgets:</strong> Compatible con videos de Wallpaper Engine, auto-pausa al jugar (0% GPU)</sub></td>
  </tr>
</table>

---

## 🧩 Suite Completa Incluida (8 Binarios Rust con Iconos HD + Instalador)

| Módulo | Binario | Descripción |
|---|---|---|
| **Shell Principal** | `AuraShell.exe` | Barra superior, Dock macOS (magnificación, vista previa DWM en vivo, efecto Genio), Overview, Launchpad, Centro de Control, Widgets magnéticos y motor de fondos nativo Media Foundation |
| **Administrador de Tareas** | `AuraTasks.exe` | Reemplazo nativo ultraligero (~14 MB RAM) con Procesos (CPU%, Modo Eco, Suspender/Reanudar, Finalizar árbol), gráficas 60s CPU/RAM/Red/GPU, **Red y Puertos TCP**, Inicio y Servicios SCM |
| **Configuración** | `AuraSettings.exe` | Reemplazo nativo ligero de Configuración de Windows (energía, Hz, audio, red, DNS, inicio, Modo Rendimiento, ciudad del clima, fondos, foto de perfil y contraseña de AuraLock) |
| **Pantalla de Bloqueo** | `AuraLock.exe` | Bloqueo GPU a pantalla completa con foto de perfil circular, fondos personalizados (imagen/video nativo/Workshop), reloj, clima geolocalizado, música y contraseña/PIN seguro |
| **Explorador + Disco** | `AuraFiles.exe` | Gestor con pestañas, etiquetas de color, vista previa sin estiramiento, búsqueda recursiva y analizador Cushion Treemap tipo WinDirStat |
| **Visor de Imágenes** | `AuraPhotos.exe` | Visor GPU instantáneo con zoom fluido, rotación, tira de miniaturas y pantalla completa nativa |
| **Reproductor Video/Audio** | `AuraPlayer.exe` | Video acelerado por GPU (Media Foundation + FFmpeg) + audio `waveOut` 48 kHz + botón para convertir video a fondo animado |
| **Editor Rápido** | `AuraNotes.exe` | Editor ultraligero de texto y código con numeración de líneas y búsqueda (`Ctrl + F`) |
| **Instalador & Updater** | `AuraShell-Setup.exe` | Instalación en 1 clic, actualización silenciosa (`--silent-update`), integración con `APBOptimizer` y desinstalación 100% limpia |

---

## 📖 Cómo Instalar y Usar AuraOS

### 1. Instalación rápida (Recomendada)
1. Ve a la pestaña **[Releases](https://github.com/dandifeto1111-prog/AuraOS/releases/latest)** y descarga **`AuraOS-v4.5.0-Windows-x64.zip`**.
2. Extrae el archivo ZIP y ejecuta **`AuraShell-Setup.exe`**.
3. Elige tus opciones (inicio directo con Windows, ocultar barra clásica, posición del Dock, optimización opcional con `APBOptimizer`) y haz clic en **Instalar / Aplicar ajustes**.
4. ¡Listo! AuraOS iniciará automáticamente en tu escritorio y recibirá futuras actualizaciones desde GitHub Releases.

### 2. Atajos de Teclado Integrados con AuraOS
| Atajo | Acción |
|---|---|
| **`Tecla Win` (toque solo)** | Abre el **Launchpad de Aplicaciones** de AuraOS (conserva `Win+Flechas`, `Win+D`, `Win+Shift+S`, `Win+V`, `Win+L`) |
| **`Ctrl + Shift + Esc`** | Abre **`AuraTasks.exe`** (Administrador de Tareas propio de AuraOS) |
| **`Win + E` / `Alt + E`** | Abre **`AuraFiles.exe`** (gestor de archivos + analizador de espacio) |
| **`Win + I`** | Abre **`AuraSettings.exe`** (Configuración de AuraOS) |
| **`Win + S` / `Alt + Space`** | Abre el **Overview y Buscador instantáneo** (apps, juegos y acciones) |
| **`Win + Flechas` / `Alt + Flechas`** | Acopla cualquier ventana Aura a izquierda/derecha (50%), maximizar o restaurar |
| **Clic derecho en semáforo verde `+`** | Cicla zonas de acople rápido (izquierda, derecha, esquinas 25%, centrar) |
| **`Ctrl + Alt + L`** | Bloquea la pantalla con **AuraLock** |
| **`Ctrl + Alt + W`** | **Recuperación de emergencia:** restaura la barra clásica y atajos de Windows inmediatamente |

### 3. Cómo usar las funciones clave
- **Personalizar tu fondo de bloqueo (`AuraLock`):** Abre el engranaje en la barra superior → **Fondos y AuraLock** → selecciona cualquier fondo de tu galería o haz clic en **+ Archivo para AuraLock** (PNG, JPG, WebP o MP4/WebM). También puedes crear un PIN o activar la verificación con tu contraseña de Windows.
- **Analizar qué ocupa espacio en tu disco:** Abre **AuraFiles** → haz clic en **Mapa espacio** (o en el botón **Mapa** junto a cualquier disco `C:`, `D:`, etc.) y selecciona carpetas en el árbol para ver su contorno dorado exacto en el mapa de calor.
- **Volver a Windows clásico en cualquier momento:** Pulsa **`Ctrl + Alt + W`** o ejecuta `AuraShell-Setup.exe --uninstall`. Todo es 100% reversible y nunca reemplaza `explorer.exe` ni afecta anti-cheats como Riot Vanguard.

---

## 🛠️ Compilar desde Código Fuente

Requisitos: **Rust estable** (`rustup` / `cargo`) en Windows 10/11 x64 y `.NET Framework 4.x` (`csc.exe`, ya incluido en Windows).

```powershell
git clone https://github.com/dandifeto1111-prog/AuraOS.git
cd AuraOS
.\build.ps1 -Version "4.3.0"
.\AuraShell-Setup.exe
```

### Publicar una nueva versión (CI/CD Automático)
El workflow [.github/workflows/release.yml](.github/workflows/release.yml) compila automáticamente los 7 binarios y publica la release en GitHub cada vez que subes un tag `v*.*.*`:

```powershell
.\release.ps1 -Version "4.4.0" -Changelog "Descripción de mejoras" -Push
```

---

<div align="center">
  <sub>Hecho con 🦀 Rust, Direct2D y obsesión por el rendimiento por <a href="https://github.com/dandifeto1111-prog"><strong>DanDe (@dandifeto1111-prog)</strong></a>.</sub>
</div>
