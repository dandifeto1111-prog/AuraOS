# Historial de Versiones — AuraOS

## v5.5.0 (2026-11-26)
- **Motor Hardware D3D11 HLSL 3D para `Gradient Waves` aislado en `AuraWallpaper.exe` — Cero Lag + Interactividad Mouse/Picos React Bits (`src/waves_gpu.rs`, `src/bin/aura_wallpaper.rs`, `src/wallpaper.rs`):**
  - **100 % GPU nativo (Direct3D 11 + DirectComposition + Z-Buffer) en proceso aislado:** `AuraShell.exe` ya **no** dibuja ni ejecuta timers para el fondo animado (`0.00 %` de carga en el hilo de UI), eliminando por completo las caídas de FPS en Dock, barra superior, Overview y animaciones de Windows.
  - **Fidelidad 1:1 a React Bits (`GradientWaves`):** malla 3D desplazada `240×156` con ruido **3D Simplex FBM** orgánico, normales por píxel a resolución nativa, sombras proyectadas suaves, oclusión ambiental en pliegues, brillo aterciopelado Fresnel y horizonte oscuro obsidiana.
  - **Interactividad real con el mouse en el escritorio:** al mover el cursor se eleva suavemente la tela líquida y al hacer **clic izquierdo en el escritorio emergen picos 3D esculpidos y ondas concéntricas** (hasta 8 picos simultáneos), sin hooks de teclado/mouse (100 % seguro con anti-cheats).
  - **Transición suave en vivo + Auto-Pausa 0 % GPU:** cambiar entre *Ocean, Dune, Dusk, Silk, Aurora* o cualquiera de los 8 colores adaptativos hace un *crossfade* fluido en GPU sin reiniciar el proceso, y se pausa automáticamente al abrir apps maximizadas o juegos.

## v5.4.0 (2026-11-26)
- **Nuevo Fondo Animado Principal por Defecto: `Gradient Waves` estilo React Bits (`src/gradient_waves.rs`, `src/wallpaper.rs`):**
  - Motor procedural 100 % nativo en Rust + reescalado bilineal Direct2D (~1 ms por frame, sin WebView ni Chromium, 100 % sin conexión y con auto-pausa a 0 % GPU).
  - **5 presets oficiales estilo React Bits** con iluminación direccional y pliegues sedosos en movimiento: **Ocean** (`#38BDF8`, nuevo fondo por defecto de AuraOS), **Dune** (`#FB923C`), **Dusk** (`#8B5CF6`), **Silk** (`#D4D4DC`) y **Aurora** (`#34D399`).
  - **Controles en vivo:** selector de presets, **8 colores adaptativos en 1 clic** (*Ocean, Aurora, Dusk, Dune, Rosa, Cian, Oro, Seda*) y sliders de **Velocidad** (`0×` pausado hasta `2×`) y **Relieve** (`0.4×–1.6×`) en **`AuraSettings → Fondos y AuraLock`** y en el panel de escritorio **`Overview → Fondos`**.
- **Adaptación Total de Color en Tiempo Real (Material You 3 Global en toda la Suite):**
  - Al cambiar el preset o el color de `Gradient Waves`, **todo AuraOS se adapta al instante** sin reiniciar apps: barra superior, Dock, Centro de Control, widgets del escritorio, Overview/Fondos, barras de título de ventanas de Windows, **`AuraSettings`**, **`AuraFiles`**, **`AuraTasks`** y **`AuraLock`** (mediante evento nativo `WM_AURAOS_PALETTE_CHANGED`).
  - Paleta `ThemePalette::from_seed_hsl` afinada con contraste AA automático en tonos cian/esmeralda/ámbar y modo monocromático seda automático para `Silk`.
- **`AuraLock` sincronizado con `Gradient Waves` (`src/bin/aura_lock.rs`):** la pantalla de bloqueo ahora renderiza las mismas ondas animadas en vivo y muestra miniaturas reales de cada preset.

## v5.3.0 (2026-11-25)
- **Corrección Total de Bloqueos del Sistema (Launchpad → Configuración), Multi-Monitor y Crash SxS / PDF:**
  - **Fix crítico de manifiesto PE `.rsrc` (`build.rs`):** Alineación de recursos a 8/16 bytes (`IMAGE_SCN_ALIGN_8BYTES`) para que el enlazador GNU `ld` nunca trunque los últimos bytes del `RT_MANIFEST` XML en `AuraSettings.exe`, `AuraPhotos.exe` y `AuraLock.exe` (`ERROR_SXS_CANT_GEN_ACTCTX`). Los 11 binarios ahora arrancan y validan su XML SxS al 100%.
  - **Cero bloqueos al abrir apps desde Launchpad (`Tecla Win`) u Overview (`src/main.rs`, `src/win32_shell.rs`):**
    - `hide_all_overlays_immediately()` oculta instantáneamente (`SW_HIDE`) Launchpad/Overview antes de lanzar cualquier programa o UAC.
    - `launch_target()` ahora es **100% asíncrono** en hilo desacoplado con `ShellExecuteExW(SEE_MASK_FLAG_NO_UI | SEE_MASK_ASYNCOK)` y `Command::new` directo para `.exe`, evitando que un acceso directo roto o UAC congele el hilo UI de `AuraShell` o el hook `WH_KEYBOARD_LL`.
    - `wake_anim_timer()` garantizado en todos los `open_*` / `close_*` + **Watchdog automático de overlays** en `tick_fast_120ms()` que oculta cualquier panel cerrado residual.
    - `bring_to_front()` protegido contra ventanas colgadas (`IsHungAppWindow`) evitando deadlocks con `AttachThreadInput`.
    - **Soporte Multi-Monitor corregido:** `is_fullscreen_app_active()` y `is_any_maximized_window_visible()` ahora verifican `MONITORINFOF_PRIMARY`, evitando que ventanas maximizadas o juegos en el Monitor 2 oculten o bloqueen AuraOS en el Monitor 1.
    - Manejo limpio de `WM_CAPTURECHANGED` y `WM_CANCELMODE` en `AuraShell`, `AuraSettings`, `AuraFiles`, `AuraPhotos`, `AuraPlayer` y `AuraShot`.
  - **Fix crash `KERNELBASE.dll` (`0x0000087a`) en `AuraPDF.exe` y `AuraFiles.exe` (`src/pdf_engine.rs`):** Todas las llamadas WinRT `Windows.Data.Pdf` ahora corren en un hilo MTA dedicado (`COINIT_MULTITHREADED`), eliminando violaciones de apartamento STA/OLE.
- **Menú Contextual Clic Derecho en el Dock / Barra de Tareas con «Finalizar tarea (Forzar cierre)» (`src/ui_dock.rs`, `src/main.rs`):**
  - Clic derecho en cualquier app abierta del Dock abre un menú flotante estilo AuraOS con: **Enfocar/Restaurar**, **Minimizar**, **Nueva instancia**, **Abrir ubicación en AuraFiles**, **Abrir en AuraTasks**, **Cerrar ventana** y **Finalizar tarea (Forzar cierre)** (mata el árbol completo de procesos con elevación UAC automática si se requieren permisos).
  - En apps ancladas no abiertas ofrece **Abrir**, **Ejecutar como Administrador** y **Abrir ubicación**.
- **`AuraTasks.exe` — Elevación Automática UAC, `SeDebugPrivilege`, Modo Admin y Clic Derecho (`src/bin/aura_tasks.rs`):**
  - Habilita `SeDebugPrivilege` al arrancar e incluye botón **`Modo Admin`** / insignia **`Admin Activo`** en la barra superior.
  - Al finalizar procesos o árboles (`Finalizar`, `Finalizar árbol`, `Supr`) o controlar servicios protegidos, si los permisos estándar fallan con `ERROR_ACCESS_DENIED`, ofrece automáticamente elevación UAC (`taskkill /F /T /PID` o `sc` con `SW_HIDE` en segundo plano) e informa con claridad si un proceso está protegido por el kernel (PPL / Anti-Cheat).
  - Nuevo **menú contextual clic derecho** en cualquier fila de proceso.
- **`AuraFiles.exe` — Paridad Completa con el Explorador de Archivos de Windows (`src/bin/aura_files.rs`, `src/ole_dnd.rs`):**
  - **Navegación completa corregida:** Botones visuales **`←` Atrás**, **`→` Adelante**, **`↑` Subir** y **`↻` Actualizar (`F5`)**, botones laterales del mouse (**`XBUTTON1` / `XBUTTON2`**) y atajos **`Alt+←` / `Alt+→` / `Alt+↑`**.
  - **Barra de dirección editable + Migas de pan interactivas (`Ctrl+L` / `Alt+D` / `F4`):** Haz clic en cualquier carpeta de la ruta para saltar, haz clic en el espacio libre para escribir/pegar rutas directamente (`C:\Users\...\Downloads`, `%USERPROFILE%`, rutas entre comillas), botón 1-clic **Copiar ruta** (`⧉`) y clic derecho en la barra para *Copiar* o *Pegar e ir*.
  - **Menú contextual clic derecho completo** en **Carpetas**, **Archivos**, **Multiselección** y **Espacio vacío**: *Abrir*, *Abrir en nueva pestaña*, *Abrir con...* (diálogo nativo Windows), *Abrir en Terminal* (con fallback PowerShell), *Cortar / Copiar / Pegar* (ahora lee también el portapapeles nativo `CF_HDROP` del Explorador de Windows!), *Copiar ruta (`Ctrl+Shift+C`)*, *Renombrar (`F2` con `Ctrl+C/V/A`)*, *Duplicar archivos y carpetas*, *Comprimir / Extraer ZIP sin ventana azul*, *Anclar a Favoritos en barra lateral*, *Mostrar archivos ocultos (`Ctrl+H`)*, *Ordenar por Nombre / Tipo / Tamaño / Fecha*, *Nuevo documento `.txt`*, *Nueva carpeta (`Ctrl+Shift+N` con renombrado inmediato)*, *Mostrar en Explorador de Windows*, *Propiedades nativas de Windows (`Alt+Enter`)* y *Eliminar a Papelera / Permanente (`Shift+Supr`)*.
- **Suite Completa de Redes de Windows + Compartir Internet (Mobile Hotspot WinRT) (`src/radios.rs`, `src/ui_control_center.rs`, `src/bin/aura_settings.rs`):**
  - **Compartir Internet (Zona con cobertura inalámbrica móvil):** Implementado nativamente con WinRT `NetworkOperatorTetheringManager` (0 UAC): activa/apaga Hotspot, muestra dispositivos conectados (`N/8`), permite editar **Nombre de red (SSID)** y **Contraseña** en `AuraSettings → Red y Bluetooth`, y se controla en 1 clic desde el Centro de Control.
  - **Controles Rápidos de Red (2×4 + Cajón de Red en vivo en el Centro de Control):** Tarjetas de **Red/Wi-Fi** (despliega escáner de redes Wi-Fi cercanas, conectar/desconectar, IPv4, velocidad y VPN), **Compartir Red (Hotspot)**, **Bluetooth** y **Modo Avión**.
  - **`AuraSettings → Red y Bluetooth` renovado:** Telemetría real de adaptador (**IPv4, Puerta de enlace, DNS, MAC, Velocidad Mbps, Proxy**), **Test de Latencia (Ping)** en vivo, **Zona Wi-Fi móvil editable**, **Escáner y conexión Wi-Fi con contraseña/Olvidar red**, **Perfiles VPN de Windows**, **DNS presets (Cloudflare / Google / Quad9 / DHCP)** y **Renovar IP / Flush DNS**.

## v5.2.2 (2026-11-25)
- **Carpeta y Paquete Oficial Reorganizados (`AuraShell-Setup.exe` único en raíz + subcarpeta `app/`):**
  - El paquete de lanzamiento (`AuraOS-v5.2.2-Windows-x64.zip`) y el repositorio ahora mantienen limpia la carpeta principal: **solo `AuraShell-Setup.exe`** queda fuera de subcarpetas; todos los binarios (`.exe`), iconos HD (`assets/icons/`), scripts de PowerShell (`scripts/`) y manifiesto quedan ordenados dentro de `app/`, `assets/icons/` y `scripts/`.
- **Instalador Inteligente v5.2.2 con Modo Rápido Siempre Visible y Auto-Descarga desde GitHub (`installer/Installer.cs`):**
  - Botón rápido verde **`Actualización rápida (sin APB)`** o **`Instalación rápida (sin APB)`** disponible desde el Paso 1 tanto si ya tenías AuraOS instalado como si es tu primera vez.
  - Si un usuario descarga únicamente `AuraShell-Setup.exe` suelto desde GitHub Releases (sin bajar el `.zip` completo), el instalador detecta la ausencia de `app/AuraShell.exe`, descarga automáticamente la última versión desde GitHub Releases y la instala sin errores.
- **Arreglos de Estabilidad y Productividad (`db12031`):**
  - **`AuraLock.exe`:** Guard de instancia única (mutex) y salida limpia sin procesos zombies al desbloquear.
  - **Widget de Música a 60 FPS en el Escritorio:** Barra ondulada ultra-fluida sincronizada con SMTC mientras reproduces.
  - **`AuraFiles.exe` Multiselección + Drag & Drop Nativo hacia Windows (`ole_dnd.rs`):** Selecciona varios archivos/carpetas (`Ctrl+Clic`, `Shift+Clic` o marquee) y arrástralos directamente hacia programas externos de Windows mediante `CF_HDROP` OLE (`DoDragDrop`).

## v5.2.1 (2026-11-24)
- **Nuevo Hub Creativo `Aura Studio` + Integración Nativa con `AuraBeats DAW`:**
  - Nuevo modo **`Studio`** en la barra superior (`PC | Dev | Gamer | Studio | Fondos`) con lanzador de 1 clic para `AuraBeats DAW`, `AuraPhotos Studio`, herramientas creativas y espacio reservado para el próximo Editor de Video.
  - Asociaciones automáticas para `.flp` y `.lsp`, iconos HD `AuraBeats` y registro automático en Launchpad, Spotlight y Mezclador de Audio.

## v5.2.0 (2026-11-24)
- **Motor de Video Wallpaper 100% Aislado (`AuraWallpaper.exe`) — Resolución Nativa 1080p/1440p a 60 FPS + Decodificación Hardware DXVA:**
  - `AuraShell.exe` ya **no carga ni importa** librerías de Windows Media Foundation (`mfplat.dll`, `mfcore.dll`, `mfreadwrite.dll`, `mfmp4srcsnk.dll`), eliminando definitivamente los picos de `574 MB – 1.4 GB` y `21.3% CPU` provocados por el reciclado de decodificadores en proceso.
  - **Cero pixelado y FPS reales:** Eliminado el reescalado CPU *nearest-neighbor* a `1280×720 / 30 FPS`; ahora `AuraWallpaper.exe` decodifica a **resolución nativa de monitor** (hasta `1920×1080` / `2560×1440`) y **hasta 60 FPS reales** mediante **DXVA/D3D11 Hardware Video Decoding** (`IMFDXGIDeviceManager` + `MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS`), ruta rápida `memcpy` sin bucles por píxel y reescalado bilineal 2×2 suave para fuentes 4K.
  - **Auto-pausa real sin fugas (`Condvar` + IPC `HWND_MESSAGE`):** Al maximizar ventanas o jugar, `AuraShell` envía `WM_AURA_WP_PAUSE` a `AuraWallpaperIPC_v52`, estacionando el decodificador en `0.00% CPU` sin destruir/reabrir Media Foundation (0 fugas de memoria virtual) y ejecutando `IDXGIDevice3::Trim()`.
- **Optimización extrema de CPU y RAM en `AuraShell.exe` (`~20–30 MB Working Set`, `~0.0% CPU` en reposo):**
  - Brocha `ID2D1SolidColorBrush` reutilizable por `WindowSurface` (`SetColor` en lugar de crear 80+ objetos COM por frame).
  - Eliminado `CreateLayer` offscreen en el widget de Música (reemplazado por `push_clip` sin asignaciones + ritmo eficiente de 12 FPS solo al reproducir).
  - `TelemetryWorker` optimizado: caché persistente de `GlobalSystemMediaTransportControlsSessionManager` (0 fugas WinRT cada 1.4s), **0 polling de CoreAudio/Toolhelp32** mientras el Centro de Control está cerrado y `IDXGIDevice3::Trim()` automático al cerrar paneles.
- **Línea de Tiempo Musical Real Sincronizada (SMTC) + Salto con Clic/Arrastre:**
  - Consulta real de `GetTimelineProperties()` (`StartTime`, `EndTime`, `Position` y compensación exacta de `LastUpdatedTime`) en Spotify, YouTube (Chrome/Edge/Brave), VLC y reproductores nativos.
  - Muestra marcas **`M:SS` / `M:SS`** en tiempo real y permite **hacer clic o arrastrar** sobre la barra ondulada Material 3 (tanto en el widget de escritorio como en el Centro de Control) para adelantar o retroceder vía `TryChangePlaybackPositionAsync`.
- **Semáforos macOS estrictamente en ventanas nativas de AuraOS:** Eliminado el overlay experimental de semáforos sobre aplicaciones externas de Windows (`traffic_lights.rs`), conservando semáforos unificados impecables en toda la suite nativa de AuraOS.
- **Nuevo Instalador Inteligente Profundo de 4 Pasos (`AuraShell-Setup.exe` v5.2):**
  - **Escáner real de hardware al inicio:** Detecta CPU, hilos/GHz, RAM total y disponible, GPU dedicada/VRAM, resolución y Hz reales del monitor, tipo de equipo (Laptop con batería vs. PC de escritorio), disco libre, versión de Windows y conflictos activos (como Wallpaper Engine externo).
  - **Adaptación automática por perfil (`Ultra / Gamer`, `Equilibrado / Laptop`, `Ultra-Ligero`)** y **selección modular de los 11 binarios**.
  - **Memoria inteligente de `APBOptimizer` (`install_state.json`):** Nunca vuelve a pedir ni pre-seleccionar la optimización de Windows en actualizaciones ni si ya fue aplicada, además de incluir botón de **Actualización rápida (sin APB)**.

## v5.1.0 (2026-11-24)
- **Motor Dinámico Material You 3 (*illogical-impulse* en Rust + Direct2D):** Extrae automáticamente una paleta oscura armónica del fondo de pantalla activo (imagen, preset o miniatura de video) y sincroniza `%LocalAppData%\AuraShell\active_palette.json` en tiempo real en toda la suite. Incluye 5 temas seleccionables en `AuraSettings → Personalización`: **`Auto (Fondo)`**, **`Obsidian Violet`**, **`Sage Forest`**, **`Warm Apricot`** y **`Monochrome`**.
- **Correcciones críticas de UX en el Dock y Animación Genio:**
  - **Orden fijo de apps en el Dock:** Registro monotónico por `HWND` en `win32_shell::get_open_user_windows()` para que las aplicaciones **nunca salten a la primera posición** al enfocarlas, abrirlas o cerrarlas.
  - **Auto-ocultado inteligente consciente del escritorio:** Con `dock_auto_hide` activo, el Dock permanece **siempre visible en el escritorio** y solo se oculta suavemente al abrir o enfocar un programa.
  - **Efecto Genio a 120 FPS sin doble animación de Windows:** Desactiva `DWMWA_TRANSITIONS_FORCEDISABLED` en la ventana objetivo durante el minimizado/restaurado (eliminando la animación clásica hacia abajo de Windows), pre-sube la textura una sola vez a GPU (`ID2D1Bitmap`, <= 960 px) y aplica recorte lazy `2×2` del swapchain al finalizar.
- **Botones Semáforo macOS Unificados en Ventanas de Terceros (`traffic_lights.rs`):** Cápsula Direct2D nativa (`DWMWA_CAPTION_BUTTON_BOUNDS`, 0% inyección DLL, 100% compatible con Riot Vanguard) que reemplaza los controles tradicionales `— □ ×` de la ventana activa de Windows por semáforos interactivos (**Rojo** cerrar, **Amarillo** minimizar con efecto Genio al Dock, **Verde** maximizar / clic derecho ciclo Aero Snap). Desactivable en vivo desde `AuraSettings → Personalización`.
- **Developer Hub Multiusuario en `Overview → Dev & AI` (`dev_hub.rs`):**
  - **Cuotas y Estado de tus IAs (por cuenta de cada usuario):** Monitor interactivo para **Antigravity**, **Gemini**, **Claude**, **Ollama Local** (`127.0.0.1:11434`) y **OpenRouter** (si el usuario configura su clave oficial), con barras Material 3, ajustes rápidos (`-5%` / `+5%` / `100%`) y enlaces a los paneles oficiales.
  - **Monitor de GitHub y Commits Git:** Detecta automáticamente el usuario de GitHub de cada desarrollador (`ai_quotas.json`, `gh`, autor de commits / `origin` o `git config`), muestra gráfica de commits de los últimos 7 días vía API pública oficial de GitHub y el estado en vivo de repositorios `.git` locales (rama, archivos modificados y último commit).
- **Barra Superior `Hug` / `Flotante` / `Recta` + Batería Real de Laptop (`GetSystemPowerStatus`):** Indicador automático de porcentaje y carga en portátiles y cálculo anti-solapamiento del reloj central.
- **Panel Derecho Unificado *illogical-impulse* (`ui_control_center.rs`):** Uptime del sistema, **Deslizador de Brillo real** (`SetDeviceGammaRamp`), **Luz Nocturna 3800K**, **Café / Mantener Despierto** (`SetThreadExecutionState`), **barra musical ondulada Material 3** y **dock inferior con pestañas (`To-Do` persistente | `Pomodoro` | `Mezclador` | `Apps 2º Plano`)**.
- **Widgets de Escritorio Expresivos:** 3 estilos seleccionables de Reloj (`Vertical`, `Cookie` festoneado, `Clásico`) + barra de progreso ondulada en el widget de Música.
- **Productividad nativa extra:** **OCR nativo de Windows 11 (`Windows.Media.Ocr`)** en `AuraShot.exe` (`Texto OCR`), renombrado rápido con **`F2`** y copiar/cortar/pegar real de archivos y carpetas (**`Ctrl+C` / `Ctrl+X` / `Ctrl+V`**) en `AuraFiles.exe`.
- **Optimización drástica de RAM (`198.8 MB` → `~112 MB` incluso con fondo de video 1080p en vivo):** Swapchains lazy `2×2` para todos los paneles y widgets ocultos, eliminación del precalentamiento masivo de miniaturas al arranque y purga limpia de caché BGRA al cerrar paneles.
- **Separación Dev vs. Usuario Final:** `AuraAgent.exe` fue retirado de la suite de usuario (10 binarios limpios); `agent-desktop` permanece exclusivamente como herramienta de QA y pruebas en el entorno de desarrollo.

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
