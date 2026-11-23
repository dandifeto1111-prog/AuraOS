using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Microsoft.Win32;

namespace AuraShellInstaller
{
    public class VersionManifest
    {
        public string Version { get; set; }
        public long BuildNumber { get; set; }
        public string BuildTime { get; set; }
        public string SourceDir { get; set; }
        public string Changelog { get; set; }
    }

    public static class InstallerProgram
    {
        public static readonly string InstallDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "AuraShell");

        public const string ApbScriptUrl = "https://raw.githubusercontent.com/OptiJuegos/host/refs/heads/main/APBOptimizer.ps1";
        private const string UninstallRegKey = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\AuraShell";
        private static readonly UTF8Encoding Utf8NoBom = new UTF8Encoding(false);

        [STAThread]
        public static void Main(string[] args)
        {
            CleanupOldUpdaterBinary();

            if (args != null && args.Length > 0)
            {
                if (args[0].Equals("--install", StringComparison.OrdinalIgnoreCase))
                {
                    string err;
                    PerformInstall(false, true, true, true, true, true, "Left", false, true, null, out err);
                    return;
                }
                if (args[0].Equals("--update", StringComparison.OrdinalIgnoreCase)
                    || args[0].Equals("--silent-update", StringComparison.OrdinalIgnoreCase))
                {
                    string err;
                    PerformQuickUpdate(true, null, out err);
                    return;
                }
                if (args[0].Equals("--uninstall", StringComparison.OrdinalIgnoreCase))
                {
                    PerformUninstall();
                    return;
                }
                if (args[0].Equals("--capture", StringComparison.OrdinalIgnoreCase) && args.Length >= 2)
                {
                    CapturePreview(args[1]);
                    return;
                }
            }

            var app = new Application();
            app.Run(new InstallerWindow());
        }

        private static void CleanupOldUpdaterBinary()
        {
            try
            {
                foreach (string f in new[] { "AuraShell-Setup.exe.old", "AuraFiles.exe.old" })
                {
                    string oldFile = Path.Combine(InstallDir, f);
                    if (File.Exists(oldFile)) File.Delete(oldFile);
                }
            }
            catch { }
        }

        public static bool IsInstalled()
        {
            return File.Exists(Path.Combine(InstallDir, "AuraShell.exe"));
        }

        public static string ResolveApbScriptPath()
        {
            string srcDir = ResolveSourceDir();
            string fromSrc = Path.Combine(srcDir, "APBOptimizer.ps1");
            if (File.Exists(fromSrc)) return fromSrc;

            string fromInst = Path.Combine(InstallDir, "APBOptimizer.ps1");
            if (File.Exists(fromInst)) return fromInst;

            return null;
        }

        public static bool EnsureApbScriptReady(Action<string> onProgress, out string localPath, out string errorMsg)
        {
            errorMsg = "";
            localPath = Path.Combine(InstallDir, "APBOptimizer.ps1");
            try
            {
                Directory.CreateDirectory(InstallDir);
                string existing = ResolveApbScriptPath();
                if (!string.IsNullOrEmpty(existing))
                {
                    if (!AreSamePath(existing, localPath))
                    {
                        File.Copy(existing, localPath, true);
                    }
                    return true;
                }

                if (onProgress != null) onProgress("Descargando APBOptimizer oficial desde GitHub...");
                ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12 | SecurityProtocolType.Tls11 | SecurityProtocolType.Tls;
                using (var wc = new WebClient())
                {
                    wc.Headers.Add("User-Agent", "AuraOS-Installer/3.3");
                    wc.DownloadFile(ApbScriptUrl, localPath);
                }
                return File.Exists(localPath);
            }
            catch (Exception ex)
            {
                errorMsg = "No se pudo preparar APBOptimizer: " + ex.Message;
                return false;
            }
        }

        public static bool LaunchApbOptimizer(bool waitForExit, Action<string> onProgress, out string errorMsg)
        {
            errorMsg = "";
            string scriptPath;
            if (!EnsureApbScriptReady(onProgress, out scriptPath, out errorMsg))
            {
                return false;
            }

            try
            {
                if (onProgress != null)
                {
                    onProgress(waitForExit
                        ? "Ejecutando APBOptimizer (acepta UAC, aplica tus optimizaciones y ciérralo para continuar con AuraOS)..."
                        : "Abriendo APBOptimizer como Administrador...");
                }

                var psi = new ProcessStartInfo
                {
                    FileName = "powershell.exe",
                    Arguments = "-ExecutionPolicy Bypass -NoProfile -File \"" + scriptPath + "\"",
                    WorkingDirectory = Path.GetDirectoryName(scriptPath),
                    UseShellExecute = true,
                    Verb = "runas"
                };

                var proc = Process.Start(psi);
                if (proc == null)
                {
                    errorMsg = "No se pudo iniciar PowerShell para APBOptimizer.";
                    return false;
                }

                if (waitForExit)
                {
                    proc.WaitForExit();
                }
                return true;
            }
            catch (System.ComponentModel.Win32Exception ex)
            {
                if (ex.NativeErrorCode == 1223)
                {
                    errorMsg = "Permiso UAC cancelado para APBOptimizer.";
                }
                else
                {
                    errorMsg = "Error al iniciar APBOptimizer: " + ex.Message;
                }
                return false;
            }
            catch (Exception ex)
            {
                errorMsg = "Error al iniciar APBOptimizer: " + ex.Message;
                return false;
            }
        }

        public static VersionManifest ReadManifest(string dir)
        {
            try
            {
                if (string.IsNullOrWhiteSpace(dir)) return null;
                string path = Path.Combine(dir, "version.json");
                if (!File.Exists(path)) return null;
                var js = new JavaScriptSerializer();
                return js.Deserialize<VersionManifest>(File.ReadAllText(path).TrimStart('\uFEFF'));
            }
            catch
            {
                return null;
            }
        }

        public static string ResolveSourceDir()
        {
            string exeDir = NormalizeDir(AppDomain.CurrentDomain.BaseDirectory);
            string instDir = NormalizeDir(InstallDir);

            if (!AreSamePath(exeDir, instDir) && File.Exists(Path.Combine(exeDir, "AuraShell.exe")))
            {
                return exeDir;
            }

            var installedMeta = ReadManifest(instDir);
            if (installedMeta != null && !string.IsNullOrWhiteSpace(installedMeta.SourceDir))
            {
                string candidate = NormalizeDir(installedMeta.SourceDir);
                if (Directory.Exists(candidate) && File.Exists(Path.Combine(candidate, "AuraShell.exe")))
                {
                    return candidate;
                }
            }

            string userProf = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            foreach (string rel in new[] { @".gemini\antigravity\scratch\AuraOS-Rust", @".gemini\antigravity\scratch\AuraShell" })
            {
                string cand = Path.Combine(userProf, rel);
                if (Directory.Exists(cand) && File.Exists(Path.Combine(cand, "AuraShell.exe")))
                {
                    return NormalizeDir(cand);
                }
            }

            return exeDir;
        }

        public static bool HasNewerVersionAvailable(out VersionManifest installed, out VersionManifest available)
        {
            installed = ReadManifest(InstallDir);
            string srcDir = ResolveSourceDir();
            available = ReadManifest(srcDir);

            if (available == null) return false;
            if (installed == null) return true;
            if (available.BuildNumber > installed.BuildNumber) return true;

            try
            {
                string srcExe = Path.Combine(srcDir, "AuraShell.exe");
                string dstExe = Path.Combine(InstallDir, "AuraShell.exe");
                if (!AreSamePath(srcExe, dstExe) && File.Exists(srcExe) && File.Exists(dstExe))
                {
                    return File.GetLastWriteTimeUtc(srcExe) > File.GetLastWriteTimeUtc(dstExe).AddSeconds(2);
                }
            }
            catch { }

            return false;
        }

        public static bool PerformQuickUpdate(bool relaunchShell, Action<string> onProgress, out string statusMessage)
        {
            bool hideTaskbar = true;
            bool startWithWin = true;
            string dockPos = "Left";
            ReadExistingPrefs(out hideTaskbar, out startWithWin, out dockPos);
            return PerformInstall(false, true, true, startWithWin, hideTaskbar, true, dockPos, relaunchShell, true, onProgress, out statusMessage);
        }

        public static void ReadExistingPrefs(out bool hideTaskbar, out bool startWithWin, out string dockPos)
        {
            hideTaskbar = true;
            startWithWin = true;
            dockPos = "Left";
            try
            {
                var js = new JavaScriptSerializer();
                string v3Path = Path.Combine(InstallDir, "settings_v3.json");
                if (File.Exists(v3Path))
                {
                    var d3 = js.Deserialize<Dictionary<string, object>>(File.ReadAllText(v3Path).TrimStart('\uFEFF'));
                    if (d3 != null)
                    {
                        if (d3.ContainsKey("hide_windows_taskbar")) hideTaskbar = Convert.ToBoolean(d3["hide_windows_taskbar"]);
                        if (d3.ContainsKey("start_with_windows")) startWithWin = Convert.ToBoolean(d3["start_with_windows"]);
                        if (d3.ContainsKey("dock_position") && d3["dock_position"] != null) dockPos = d3["dock_position"].ToString();
                        return;
                    }
                }
                string legacyPath = Path.Combine(InstallDir, "settings.json");
                if (File.Exists(legacyPath))
                {
                    var d = js.Deserialize<Dictionary<string, object>>(File.ReadAllText(legacyPath).TrimStart('\uFEFF'));
                    if (d != null)
                    {
                        if (d.ContainsKey("HideWindowsTaskbar")) hideTaskbar = Convert.ToBoolean(d["HideWindowsTaskbar"]);
                        if (d.ContainsKey("StartWithWindows")) startWithWin = Convert.ToBoolean(d["StartWithWindows"]);
                        if (d.ContainsKey("DockPosition") && d["DockPosition"] != null) dockPos = d["DockPosition"].ToString();
                    }
                }
            }
            catch { }
        }

        public static bool RunBuildScriptInSource(Action<string> onProgress, out string errorMsg)
        {
            errorMsg = "";
            try
            {
                string srcDir = ResolveSourceDir();
                string buildPs1 = Path.Combine(srcDir, "build.ps1");
                if (!File.Exists(buildPs1))
                {
                    errorMsg = "No se encontró build.ps1 en " + srcDir;
                    return false;
                }

                if (onProgress != null) onProgress("Compilando última versión desde código fuente...");
                var psi = new ProcessStartInfo
                {
                    FileName = "powershell.exe",
                    Arguments = "-ExecutionPolicy Bypass -NoProfile -File \"" + buildPs1 + "\"",
                    WorkingDirectory = srcDir,
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    CreateNoWindow = true
                };
                using (var p = Process.Start(psi))
                {
                    string stdout = p.StandardOutput.ReadToEnd();
                    string stderr = p.StandardError.ReadToEnd();
                    p.WaitForExit(240000);
                    if (p.ExitCode != 0)
                    {
                        errorMsg = string.IsNullOrWhiteSpace(stderr) ? stdout : stderr;
                        return false;
                    }
                }
                return true;
            }
            catch (Exception ex)
            {
                errorMsg = ex.Message;
                return false;
            }
        }

        public static bool PerformInstall(
            bool runApbOptimizerFirst,
            bool createDesktopShortcuts,
            bool createStartMenu,
            bool startWithWindows,
            bool hideTaskbarOnStart,
            bool applyWindowsDarkPolish,
            string dockPosition,
            bool launchAfter,
            bool preserveExistingSettings,
            Action<string> onProgress,
            out string statusMessage)
        {
            statusMessage = "";
            try
            {
                Directory.CreateDirectory(InstallDir);

                // Step 1 (Optional): Optimize Windows with APBOptimizer BEFORE installing AuraOS!
                if (runApbOptimizerFirst)
                {
                    string apbErr;
                    bool apbOk = LaunchApbOptimizer(true, onProgress, out apbErr);
                    if (!apbOk)
                    {
                        // Informative message if user cancelled UAC — continue smoothly or report
                        if (onProgress != null) onProgress("APBOptimizer omitido (" + apbErr + "). Continuando instalación de AuraOS...");
                        Thread.Sleep(700);
                    }
                }

                string sourceDir = ResolveSourceDir();
                string srcShell = Path.Combine(sourceDir, "AuraShell.exe");
                string srcFiles = Path.Combine(sourceDir, "AuraFiles.exe");
                if (!File.Exists(srcShell))
                {
                    statusMessage = "No se encontró AuraShell.exe en " + sourceDir;
                    return false;
                }

                if (onProgress != null) onProgress("Cerrando instancias abiertas de AuraOS y AuraFiles...");
                StopRunningApps();

                string dstShell = Path.Combine(InstallDir, "AuraShell.exe");
                string dstFiles = Path.Combine(InstallDir, "AuraFiles.exe");
                string dstPhotos = Path.Combine(InstallDir, "AuraPhotos.exe");
                string dstPlayer = Path.Combine(InstallDir, "AuraPlayer.exe");
                string dstNotes = Path.Combine(InstallDir, "AuraNotes.exe");
                string dstSettings = Path.Combine(InstallDir, "AuraSettings.exe");
                string dstLock = Path.Combine(InstallDir, "AuraLock.exe");
                string dstShellIco = Path.Combine(InstallDir, "AuraShell.ico");
                string dstFilesIco = Path.Combine(InstallDir, "AuraFiles.ico");
                string dstRestore = Path.Combine(InstallDir, "restore-windows.ps1");
                string dstApb = Path.Combine(InstallDir, "APBOptimizer.ps1");
                string dstSetup = Path.Combine(InstallDir, "AuraShell-Setup.exe");
                string dstVersion = Path.Combine(InstallDir, "version.json");

                if (onProgress != null) onProgress("Copiando Suite AuraOS (Shell, Settings, Lock, Files, Photos, Player, Notes)...");
                SafeCopyFileWithRetry(srcShell, dstShell);
                if (File.Exists(srcFiles)) SafeCopyFileWithRetry(srcFiles, dstFiles);
                if (File.Exists(Path.Combine(sourceDir, "AuraPhotos.exe"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraPhotos.exe"), dstPhotos);
                if (File.Exists(Path.Combine(sourceDir, "AuraPlayer.exe"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraPlayer.exe"), dstPlayer);
                if (File.Exists(Path.Combine(sourceDir, "AuraNotes.exe"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraNotes.exe"), dstNotes);
                if (File.Exists(Path.Combine(sourceDir, "AuraSettings.exe"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraSettings.exe"), dstSettings);
                if (File.Exists(Path.Combine(sourceDir, "AuraLock.exe"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraLock.exe"), dstLock);
                if (File.Exists(Path.Combine(sourceDir, "AuraShell.ico"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraShell.ico"), dstShellIco);
                if (File.Exists(Path.Combine(sourceDir, "AuraFiles.ico"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraFiles.ico"), dstFilesIco);
                if (File.Exists(Path.Combine(sourceDir, "AuraShell_256.png"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraShell_256.png"), Path.Combine(InstallDir, "AuraShell_256.png"));
                if (File.Exists(Path.Combine(sourceDir, "AuraFiles_256.png"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "AuraFiles_256.png"), Path.Combine(InstallDir, "AuraFiles_256.png"));
                if (File.Exists(Path.Combine(sourceDir, "restore-windows.ps1"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "restore-windows.ps1"), dstRestore);
                if (File.Exists(Path.Combine(sourceDir, "APBOptimizer.ps1"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "APBOptimizer.ps1"), dstApb);
                if (File.Exists(Path.Combine(sourceDir, "version.json"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "version.json"), dstVersion);

                string srcSetup = Path.Combine(sourceDir, "AuraShell-Setup.exe");
                if (!File.Exists(srcSetup)) srcSetup = Assembly.GetExecutingAssembly().Location;
                SafeCopySelfUpdater(srcSetup, dstSetup);

                // Merge/write BOTH settings_v3.json (Rust v3) and legacy settings.json cleanly without BOM!
                if (onProgress != null) onProgress("Aplicando configuración e integración con Windows...");
                var js = new JavaScriptSerializer();

                string v3Path = Path.Combine(InstallDir, "settings_v3.json");
                Dictionary<string, object> v3Obj = null;
                if (preserveExistingSettings && File.Exists(v3Path))
                {
                    try { v3Obj = js.Deserialize<Dictionary<string, object>>(File.ReadAllText(v3Path).TrimStart('\uFEFF')); } catch { }
                }
                if (v3Obj == null)
                {
                    v3Obj = new Dictionary<string, object>
                    {
                        { "glass_opacity", 0.78 },
                        { "auto_game_boost", true },
                        { "dock_auto_hide", true },
                        { "show_desktop_widgets", true },
                        { "last_mode", "PC" },
                        { "style_windows_apps", true },
                        { "wallpaper_id", "external" },
                        { "wallpaper_auto_pause", true }
                    };
                }
                v3Obj["hide_windows_taskbar"] = hideTaskbarOnStart;
                v3Obj["start_with_windows"] = startWithWindows;
                v3Obj["dock_position"] = dockPosition ?? "Left";
                File.WriteAllText(v3Path, js.Serialize(v3Obj), Utf8NoBom);

                string legacyPath = Path.Combine(InstallDir, "settings.json");
                Dictionary<string, object> legObj = null;
                if (preserveExistingSettings && File.Exists(legacyPath))
                {
                    try { legObj = js.Deserialize<Dictionary<string, object>>(File.ReadAllText(legacyPath).TrimStart('\uFEFF')); } catch { }
                }
                if (legObj == null)
                {
                    legObj = new Dictionary<string, object>
                    {
                        { "GlassOpacity", 0.78 },
                        { "AutoGameBoost", true },
                        { "DockAutoHide", true },
                        { "ShowDesktopWidgets", true },
                        { "LastMode", "PC" }
                    };
                }
                legObj["HideWindowsTaskbar"] = hideTaskbarOnStart;
                legObj["StartWithWindows"] = startWithWindows;
                legObj["DockPosition"] = dockPosition ?? "Left";
                File.WriteAllText(legacyPath, js.Serialize(legObj), Utf8NoBom);

                if (applyWindowsDarkPolish)
                {
                    ApplySafeWindowsPolish();
                }

                string shellIconRef = File.Exists(dstShellIco) ? dstShellIco : dstShell;
                string filesIconRef = File.Exists(dstFilesIco) ? dstFilesIco : dstFiles;

                if (createDesktopShortcuts)
                {
                    string desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                    CreateShortcut(Path.Combine(desktop, "AuraShell.lnk"), dstShell, "", InstallDir, shellIconRef, "AuraOS — Entorno Dark Mode & Control Center");
                    if (File.Exists(dstFiles))
                        CreateShortcut(Path.Combine(desktop, "AuraFiles.lnk"), dstFiles, "", InstallDir, filesIconRef, "AuraFiles — Gestor de Archivos & Analizador de Disco");
                }

                if (createStartMenu)
                {
                    string startFolder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), "AuraShell");
                    Directory.CreateDirectory(startFolder);
                    CreateShortcut(Path.Combine(startFolder, "AuraShell.lnk"), dstShell, "", InstallDir, shellIconRef, "Iniciar entorno AuraOS");
                    if (File.Exists(dstFiles))
                        CreateShortcut(Path.Combine(startFolder, "AuraFiles.lnk"), dstFiles, "", InstallDir, filesIconRef, "Abrir Gestor de Archivos AuraFiles");
                    if (File.Exists(dstPhotos))
                        CreateShortcut(Path.Combine(startFolder, "AuraPhotos.lnk"), dstPhotos, "", InstallDir, shellIconRef, "Visor de Imágenes GPU AuraPhotos");
                    if (File.Exists(dstPlayer))
                        CreateShortcut(Path.Combine(startFolder, "AuraPlayer.lnk"), dstPlayer, "", InstallDir, shellIconRef, "Reproductor de Video y Audio AuraPlayer");
                    if (File.Exists(dstNotes))
                        CreateShortcut(Path.Combine(startFolder, "AuraNotes.lnk"), dstNotes, "", InstallDir, shellIconRef, "Editor instantáneo de Texto y Código AuraNotes");
                    if (File.Exists(dstSettings))
                        CreateShortcut(Path.Combine(startFolder, "Configuración AuraOS.lnk"), dstSettings, "", InstallDir, shellIconRef, "Configuración nativa de sistema, audio, pantalla y red AuraOS");
                    if (File.Exists(dstLock))
                        CreateShortcut(Path.Combine(startFolder, "AuraLock (Bloquear).lnk"), dstLock, "", InstallDir, shellIconRef, "Pantalla de bloqueo GPU AuraLock con fondos personalizados");
                    CreateShortcut(Path.Combine(startFolder, "Actualizar o Configurar AuraOS.lnk"), dstSetup, "", InstallDir, shellIconRef, "Actualizar versión, optimizar Windows o desinstalar AuraOS");
                    if (File.Exists(dstApb))
                        CreateShortcut(Path.Combine(startFolder, "Optimizar Windows (APBOptimizer).lnk"), "powershell.exe", "-ExecutionPolicy Bypass -NoProfile -File \"" + dstApb + "\"", InstallDir, shellIconRef, "Abrir APBOptimizer (OptiJuegos) para optimizar Windows");
                    if (File.Exists(dstRestore))
                        CreateShortcut(Path.Combine(startFolder, "Restaurar Barra de Windows.lnk"), "powershell.exe", "-ExecutionPolicy Bypass -File \"" + dstRestore + "\"", InstallDir, shellIconRef, "Restaurar barra clásica y energía de Windows");
                }

                // ZERO-DELAY DIRECT LOGON BOOT (replaces slow 10s-delayed Startup folder .lnk!):
                // Registers HKCU\Run + Explorer\Serialize\StartupDelayInMSec = 0 (100% reversible & Vanguard-safe!)
                string startupLnk = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Startup), "AuraShell.lnk");
                try { if (File.Exists(startupLnk)) File.Delete(startupLnk); } catch { }
                try
                {
                    using (var runKey = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run"))
                    {
                        if (runKey != null)
                        {
                            if (startWithWindows) runKey.SetValue("AuraShell", "\"" + dstShell + "\"");
                            else runKey.DeleteValue("AuraShell", false);
                        }
                    }
                    if (startWithWindows)
                    {
                        using (var serKey = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize"))
                        {
                            if (serKey != null) serKey.SetValue("StartupDelayInMSec", 0, RegistryValueKind.DWord);
                        }
                    }
                    else
                    {
                        using (var serKey = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", true))
                        {
                            if (serKey != null) serKey.DeleteValue("StartupDelayInMSec", false);
                        }
                    }
                }
                catch { }

                if (File.Exists(dstFiles))
                {
                    try
                    {
                        using (var k = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\shell\open_aurafiles"))
                        {
                            k.SetValue("", "Abrir en AuraFiles");
                            k.SetValue("Icon", filesIconRef);
                        }
                        using (var cmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\shell\open_aurafiles\command"))
                        {
                            cmd.SetValue("", "\"" + dstFiles + "\" \"%1\"");
                        }
                        using (var root = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\shell"))
                        {
                            root.SetValue("", "open_aurafiles");
                        }
                        using (var bg = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\Background\shell\open_aurafiles"))
                        {
                            bg.SetValue("", "Abrir carpeta en AuraFiles");
                            bg.SetValue("Icon", filesIconRef);
                        }
                        using (var bgCmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\Background\shell\open_aurafiles\command"))
                        {
                            bgCmd.SetValue("", "\"" + dstFiles + "\" \"%V\"");
                        }
                    }
                    catch { }
                }

                // 100% Reversible HKCU File Associations & Open-With Integration for AuraOS Suite!
                RegisterSuiteFileAssociations(dstPhotos, dstPlayer, dstNotes, shellIconRef);

                var verMeta = ReadManifest(InstallDir) ?? ReadManifest(sourceDir);
                string verLabel = verMeta != null && !string.IsNullOrEmpty(verMeta.Version) ? verMeta.Version : "3.3.0";

                using (RegistryKey key = Registry.CurrentUser.CreateSubKey(UninstallRegKey))
                {
                    if (key != null)
                    {
                        key.SetValue("DisplayName", "AuraOS (Shell, AuraFiles & APBOptimizer)");
                        key.SetValue("DisplayVersion", verLabel);
                        key.SetValue("Publisher", "AuraOS");
                        key.SetValue("DisplayIcon", shellIconRef);
                        key.SetValue("InstallLocation", InstallDir);
                        key.SetValue("UninstallString", "\"" + dstSetup + "\" --uninstall");
                        key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                        key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                    }
                }

                if (launchAfter)
                {
                    if (onProgress != null) onProgress("Iniciando AuraOS v" + verLabel + "...");
                    Process.Start(new ProcessStartInfo { FileName = dstShell, WorkingDirectory = InstallDir, UseShellExecute = true });
                }

                statusMessage = "AuraOS v" + verLabel + " instalado y actualizado correctamente.";
                return true;
            }
            catch (Exception ex)
            {
                statusMessage = "Error al instalar: " + ex.Message;
                return false;
            }
        }

        private static void SafeCopyFileWithRetry(string src, string dst)
        {
            if (string.IsNullOrWhiteSpace(src) || string.IsNullOrWhiteSpace(dst)) return;
            if (!File.Exists(src)) return;
            if (AreSamePath(src, dst)) return;

            Exception lastEx = null;
            for (int attempt = 0; attempt < 8; attempt++)
            {
                try
                {
                    File.Copy(src, dst, true);
                    return;
                }
                catch (IOException ex)
                {
                    lastEx = ex;
                    // Seamless zero-disruption hot-swap via .old rename if binary is currently open!
                    try
                    {
                        string oldPath = dst + ".old";
                        if (File.Exists(oldPath)) File.Delete(oldPath);
                        File.Move(dst, oldPath);
                        File.Copy(src, dst, true);
                        return;
                    }
                    catch { }
                    StopRunningApps();
                    Thread.Sleep(180);
                }
                catch (UnauthorizedAccessException ex)
                {
                    lastEx = ex;
                    StopRunningApps();
                    Thread.Sleep(180);
                }
            }
            if (lastEx != null) throw lastEx;
        }

        private static void SafeCopySelfUpdater(string srcSetup, string dstSetup)
        {
            try
            {
                if (!File.Exists(srcSetup) || AreSamePath(srcSetup, dstSetup)) return;
                string runningExe = Assembly.GetExecutingAssembly().Location;
                if (AreSamePath(runningExe, dstSetup))
                {
                    string oldPath = dstSetup + ".old";
                    try { if (File.Exists(oldPath)) File.Delete(oldPath); } catch { }
                    File.Move(dstSetup, oldPath);
                    File.Copy(srcSetup, dstSetup, true);
                }
                else
                {
                    SafeCopyFileWithRetry(srcSetup, dstSetup);
                }
            }
            catch { }
        }

        private static string NormalizeDir(string p)
        {
            if (string.IsNullOrWhiteSpace(p)) return "";
            try { return Path.GetFullPath(p).TrimEnd('\\', '/'); }
            catch { return p.TrimEnd('\\', '/'); }
        }

        private static bool AreSamePath(string a, string b)
        {
            try
            {
                string pa = Path.GetFullPath(a).TrimEnd('\\', '/');
                string pb = Path.GetFullPath(b).TrimEnd('\\', '/');
                return string.Equals(pa, pb, StringComparison.OrdinalIgnoreCase);
            }
            catch
            {
                return string.Equals(a, b, StringComparison.OrdinalIgnoreCase);
            }
        }

        private static void ApplySafeWindowsPolish()
        {
            try
            {
                using (var pers = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"))
                {
                    if (pers != null)
                    {
                        pers.SetValue("AppsUseLightTheme", 0, RegistryValueKind.DWord);
                        pers.SetValue("SystemUsesLightTheme", 0, RegistryValueKind.DWord);
                        pers.SetValue("EnableTransparency", 1, RegistryValueKind.DWord);
                    }
                }
                using (var adv = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"))
                {
                    if (adv != null)
                    {
                        adv.SetValue("TaskbarDa", 0, RegistryValueKind.DWord);
                        adv.SetValue("TaskbarMn", 0, RegistryValueKind.DWord);
                    }
                }
            }
            catch { }
        }

        public static void PerformUninstall()
        {
            try
            {
                StopRunningApps();

                string restoreScript = Path.Combine(InstallDir, "restore-windows.ps1");
                if (File.Exists(restoreScript))
                {
                    try
                    {
                        var p = Process.Start(new ProcessStartInfo
                        {
                            FileName = "powershell.exe",
                            Arguments = "-ExecutionPolicy Bypass -WindowStyle Hidden -File \"" + restoreScript + "\"",
                            UseShellExecute = false,
                            CreateNoWindow = true
                        });
                        if (p != null) p.WaitForExit(3500);
                    }
                    catch { }
                }

                string desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                foreach (string name in new[] { "AuraShell.lnk", "AuraFiles.lnk" })
                {
                    string p = Path.Combine(desktop, name);
                    if (File.Exists(p)) File.Delete(p);
                }

                string startupLnk = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Startup), "AuraShell.lnk");
                if (File.Exists(startupLnk)) File.Delete(startupLnk);
                try { using (var rk = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run", true)) { if (rk != null) rk.DeleteValue("AuraShell", false); } } catch { }
                try { using (var sk = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", true)) { if (sk != null) sk.DeleteValue("StartupDelayInMSec", false); } } catch { }

                string startFolder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), "AuraShell");
                if (Directory.Exists(startFolder)) Directory.Delete(startFolder, true);

                try { Registry.CurrentUser.DeleteSubKeyTree(@"Software\Classes\Directory\shell\open_aurafiles", false); } catch { }
                try { using (var r = Registry.CurrentUser.OpenSubKey(@"Software\Classes\Directory\shell", true)) { if (r != null) r.DeleteValue("", false); } } catch { }
                try { Registry.CurrentUser.DeleteSubKeyTree(@"Software\Classes\Directory\Background\shell\open_aurafiles", false); } catch { }
                try { Registry.CurrentUser.DeleteSubKeyTree(UninstallRegKey, false); } catch { }

                foreach (string f in new[] { "AuraShell.exe", "AuraFiles.exe", "AuraPhotos.exe", "AuraPlayer.exe", "AuraNotes.exe", "AuraSettings.exe", "AuraLock.exe", "AuraShell.ico", "AuraFiles.ico", "AuraShell_256.png", "AuraFiles_256.png", "settings.json", "settings_v3.json", "version.json", "AuraShell.log", "restore-windows.ps1", "APBOptimizer.ps1" })
                {
                    string p = Path.Combine(InstallDir, f);
                    try { if (File.Exists(p)) File.Delete(p); } catch { }
                }
                UnregisterSuiteFileAssociations();
            }
            catch { }
        }

        private static void RegisterSuiteFileAssociations(string dstPhotos, string dstPlayer, string dstNotes, string iconRef)
        {
            try
            {
                if (File.Exists(dstPhotos))
                {
                    RegisterProgId("AuraOS.Photos", "Imagen (AuraPhotos)", dstPhotos, iconRef);
                    foreach (string ext in new[] { ".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".ico" })
                        RegisterExtAssociation(ext, "AuraOS.Photos", "Abrir con AuraPhotos", dstPhotos, iconRef);
                }
                if (File.Exists(dstPlayer))
                {
                    RegisterProgId("AuraOS.Player", "Multimedia (AuraPlayer)", dstPlayer, iconRef);
                    foreach (string ext in new[] { ".mp4", ".mkv", ".webm", ".mov", ".avi", ".wmv", ".mp3", ".wav", ".flac", ".ogg" })
                        RegisterExtAssociation(ext, "AuraOS.Player", "Reproducir en AuraPlayer", dstPlayer, iconRef);
                }
                if (File.Exists(dstNotes))
                {
                    RegisterProgId("AuraOS.Notes", "Documento de texto (AuraNotes)", dstNotes, iconRef);
                    foreach (string ext in new[] { ".txt", ".md", ".json", ".log", ".ini", ".cfg", ".toml", ".yaml", ".ps1" })
                        RegisterExtAssociation(ext, "AuraOS.Notes", "Editar en AuraNotes", dstNotes, iconRef);
                }
            }
            catch { }
        }

        private static void RegisterProgId(string progId, string friendlyName, string exePath, string iconRef)
        {
            using (var k = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId))
            {
                k.SetValue("", friendlyName);
            }
            using (var ic = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId + @"\DefaultIcon"))
            {
                ic.SetValue("", iconRef);
            }
            using (var cmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId + @"\shell\open\command"))
            {
                cmd.SetValue("", "\"" + exePath + "\" \"%1\"");
            }
        }

        private static void RegisterExtAssociation(string ext, string progId, string verbLabel, string exePath, string iconRef)
        {
            using (var openWith = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + ext + @"\OpenWithProgids"))
            {
                openWith.SetValue(progId, "", RegistryValueKind.String);
            }
            using (var menu = Registry.CurrentUser.CreateSubKey(@"Software\Classes\SystemFileAssociations\" + ext + @"\shell\open_auraos"))
            {
                menu.SetValue("", verbLabel);
                menu.SetValue("Icon", iconRef);
            }
            using (var mcmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\SystemFileAssociations\" + ext + @"\shell\open_auraos\command"))
            {
                mcmd.SetValue("", "\"" + exePath + "\" \"%1\"");
            }
        }

        private static void UnregisterSuiteFileAssociations()
        {
            foreach (string pid in new[] { "AuraOS.Photos", "AuraOS.Player", "AuraOS.Notes" })
            {
                try { Registry.CurrentUser.DeleteSubKeyTree(@"Software\Classes\" + pid, false); } catch { }
            }
            foreach (string ext in new[] { ".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".ico", ".mp4", ".mkv", ".webm", ".mov", ".avi", ".wmv", ".mp3", ".wav", ".flac", ".ogg", ".txt", ".md", ".json", ".log", ".ini", ".cfg", ".toml", ".yaml", ".ps1" })
            {
                try { Registry.CurrentUser.DeleteSubKeyTree(@"Software\Classes\SystemFileAssociations\" + ext + @"\shell\open_auraos", false); } catch { }
            }
        }

        private static void StopRunningApps()
        {
            // Only stop AuraShell cleanly so open AuraFiles/Photos/Player/Notes stay undisturbed (hot-swapped via .old)!
            foreach (string procName in new[] { "AuraShell" })
            {
                try
                {
                    foreach (var p in Process.GetProcessesByName(procName))
                    {
                        try
                        {
                            if (!p.HasExited)
                            {
                                p.CloseMainWindow();
                                if (!p.WaitForExit(1200))
                                {
                                    p.Kill();
                                    p.WaitForExit(3500);
                                }
                            }
                        }
                        catch { }
                        finally { try { p.Dispose(); } catch { } }
                    }
                }
                catch { }
            }
            Thread.Sleep(350);
        }

        private static void CreateShortcut(string lnkPath, string target, string args, string workDir, string iconLoc, string desc)
        {
            Type shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType == null) return;
            dynamic shell = Activator.CreateInstance(shellType);
            dynamic shortcut = shell.CreateShortcut(lnkPath);
            shortcut.TargetPath = target;
            if (!string.IsNullOrEmpty(args)) shortcut.Arguments = args;
            if (!string.IsNullOrEmpty(workDir)) shortcut.WorkingDirectory = workDir;
            if (!string.IsNullOrEmpty(iconLoc)) shortcut.IconLocation = iconLoc;
            shortcut.Description = desc;
            shortcut.Save();
        }

        private static void CapturePreview(string outPath)
        {
            try
            {
                var win = new InstallerWindow();
                var visual = (UIElement)win.Content;
                win.Content = null;
                var host = new Border { Width = 604, Background = new SolidColorBrush(Color.FromRgb(11, 11, 15)), Child = visual };
                host.Measure(new Size(604, 1100));
                host.Arrange(new Rect(0, 0, 604, host.DesiredSize.Height));
                host.UpdateLayout();
                int h = Math.Max(560, (int)Math.Ceiling(host.DesiredSize.Height));
                var rtb = new RenderTargetBitmap(604, h, 96, 96, PixelFormats.Pbgra32);
                rtb.Render(host);
                var enc = new PngBitmapEncoder();
                enc.Frames.Add(BitmapFrame.Create(rtb));
                Directory.CreateDirectory(Path.GetDirectoryName(outPath));
                using (var fs = File.Create(outPath)) enc.Save(fs);
            }
            catch { }
        }
    }

    public class InstallerWindow : Window
    {
        private static readonly Color BgCanvas = Color.FromRgb(20, 20, 27);
        private static readonly Color CardBg = Color.FromRgb(28, 28, 38);
        private static readonly Color CardBorder = Color.FromRgb(46, 46, 62);
        private static readonly Color Violet = Color.FromRgb(124, 92, 255);
        private static readonly Color VioletHover = Color.FromRgb(140, 112, 255);
        private static readonly Color Emerald = Color.FromRgb(34, 197, 94);
        private static readonly Color Amber = Color.FromRgb(245, 158, 11);
        private static readonly Color Coral = Color.FromRgb(242, 109, 125);
        private static readonly SolidColorBrush TextMuted = new SolidColorBrush(Color.FromRgb(154, 156, 178));

        private bool _optApbBeforeInstall = true;
        private bool _optDesktop = true;
        private bool _optStartMenu = true;
        private bool _optStartup = true;
        private bool _optHideTaskbar = true;
        private bool _optDarkPolish = true;
        private bool _optDockLeft = true;
        private bool _optLaunch = true;
        private bool _busy = false;

        private TextBlock _statusText;
        private Border _statusBanner;
        private TextBlock _installedVerTxt;
        private TextBlock _availableVerTxt;
        private TextBlock _changelogTxt;
        private Border _versionBadge;
        private TextBlock _versionBadgeTxt;
        private Border _primaryBtn;
        private TextBlock _primaryBtnTxt;
        private Border _launchNowBtn;

        public InstallerWindow()
        {
            Title = "AuraOS — Instalador y Actualizador";
            Width = 596;
            SizeToContent = SizeToContent.Height;
            WindowStartupLocation = WindowStartupLocation.CenterScreen;
            WindowStyle = WindowStyle.None;
            AllowsTransparency = true;
            Background = Brushes.Transparent;
            ResizeMode = ResizeMode.NoResize;
            FontFamily = new FontFamily("Segoe UI Variable Text, Segoe UI, Arial");

            LoadExistingPreferences();
            BuildUi();
            RefreshVersionInfo();
        }

        private void LoadExistingPreferences()
        {
            bool hideTb, startup;
            string dock;
            InstallerProgram.ReadExistingPrefs(out hideTb, out startup, out dock);
            _optHideTaskbar = hideTb;
            _optStartup = startup;
            _optDockLeft = !dock.Equals("Bottom", StringComparison.OrdinalIgnoreCase);
            // Default ON for clean first install; also easy to toggle ON anytime before reinstall
            _optApbBeforeInstall = true;
        }

        private void BuildUi()
        {
            var outer = new Border
            {
                Margin = new Thickness(14),
                CornerRadius = new CornerRadius(24),
                BorderThickness = new Thickness(1),
                BorderBrush = new SolidColorBrush(CardBorder),
                Background = new SolidColorBrush(BgCanvas),
                Effect = new System.Windows.Media.Effects.DropShadowEffect { Color = Colors.Black, BlurRadius = 30, ShadowDepth = 8, Opacity = 0.60 }
            };

            var root = new StackPanel { Margin = new Thickness(24, 20, 24, 20) };

            // HEADER (draggable)
            var header = new Grid { Background = Brushes.Transparent, Cursor = Cursors.SizeAll, Margin = new Thickness(0, 0, 0, 12) };
            header.MouseLeftButtonDown += (s, e) => { try { DragMove(); } catch { } };
            var ts = new StackPanel();
            var titleRow = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
            titleRow.Children.Add(new TextBlock { Text = "AuraOS", Foreground = Brushes.White, FontSize = 22, FontWeight = FontWeights.Bold });
            _versionBadge = new Border
            {
                CornerRadius = new CornerRadius(8),
                Background = new SolidColorBrush(Violet),
                Padding = new Thickness(8, 3, 8, 3),
                Margin = new Thickness(10, 0, 0, 0),
                VerticalAlignment = VerticalAlignment.Center
            };
            _versionBadgeTxt = new TextBlock { Text = "v3.3.0", Foreground = Brushes.White, FontSize = 10.5, FontWeight = FontWeights.SemiBold };
            _versionBadge.Child = _versionBadgeTxt;
            titleRow.Children.Add(_versionBadge);
            ts.Children.Add(titleRow);
            ts.Children.Add(new TextBlock { Text = "Instalador, Optimizador Windows (APB) y Gestor de Actualizaciones", Foreground = TextMuted, FontSize = 11.6, Margin = new Thickness(0, 3, 0, 0) });
            header.Children.Add(ts);

            // Unified macOS Traffic Lights (× coral close, − amber minimize, + emerald accent)
            var tlPanel = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Top };
            var tlGlyphs = new List<TextBlock>();
            Action<Color, string, Action> addLight = (col, glyph, onClick) =>
            {
                var gTxt = new TextBlock
                {
                    Text = glyph,
                    Foreground = new SolidColorBrush(Color.FromRgb(22, 16, 20)),
                    FontSize = 9.5,
                    FontWeight = FontWeights.Bold,
                    HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center,
                    Opacity = 0.0,
                    Margin = new Thickness(0, -1, 0, 0)
                };
                tlGlyphs.Add(gTxt);
                var dot = new Border
                {
                    Width = 14, Height = 14, CornerRadius = new CornerRadius(7),
                    Background = new SolidColorBrush(col),
                    Margin = new Thickness(6, 2, 0, 0),
                    Cursor = Cursors.Hand,
                    Child = gTxt
                };
                dot.MouseLeftButtonDown += (s, e) => { e.Handled = true; onClick(); };
                tlPanel.Children.Add(dot);
            };
            addLight(Coral, "×", () => Close());
            addLight(Color.FromRgb(251, 191, 36), "−", () => WindowState = WindowState.Minimized);
            addLight(Emerald, "+", () => { });
            tlPanel.MouseEnter += (s, e) => { foreach (var g in tlGlyphs) g.Opacity = 1.0; };
            tlPanel.MouseLeave += (s, e) => { foreach (var g in tlGlyphs) g.Opacity = 0.0; };
            header.Children.Add(tlPanel);
            root.Children.Add(header);

            // VERSION COMPARISON & PATH CARD
            var verCard = new Border
            {
                CornerRadius = new CornerRadius(16),
                Background = new SolidColorBrush(CardBg),
                BorderBrush = new SolidColorBrush(CardBorder),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(14, 11, 14, 11),
                Margin = new Thickness(0, 0, 0, 10)
            };
            var vcStack = new StackPanel();
            var verCols = new Grid { Margin = new Thickness(0, 0, 0, 6) };
            verCols.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            verCols.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

            var col1 = new StackPanel();
            col1.Children.Add(new TextBlock { Text = "VERSIÓN INSTALADA", Foreground = TextMuted, FontSize = 9.5, FontWeight = FontWeights.Bold });
            _installedVerTxt = new TextBlock { Text = "Sin instalar", Foreground = Brushes.White, FontSize = 12.8, FontWeight = FontWeights.SemiBold, Margin = new Thickness(0, 2, 0, 0) };
            col1.Children.Add(_installedVerTxt);
            verCols.Children.Add(col1);

            var col2 = new StackPanel();
            col2.Children.Add(new TextBlock { Text = "DISPONIBLE PARA ACTUALIZAR", Foreground = TextMuted, FontSize = 9.5, FontWeight = FontWeights.Bold });
            _availableVerTxt = new TextBlock { Text = "v3.3.0", Foreground = new SolidColorBrush(VioletHover), FontSize = 12.8, FontWeight = FontWeights.SemiBold, Margin = new Thickness(0, 2, 0, 0) };
            col2.Children.Add(_availableVerTxt);
            Grid.SetColumn(col2, 1);
            verCols.Children.Add(col2);
            vcStack.Children.Add(verCols);

            _changelogTxt = new TextBlock { Text = "", Foreground = new SolidColorBrush(Color.FromRgb(200, 204, 222)), FontSize = 10.8, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 0, 0, 5) };
            vcStack.Children.Add(_changelogTxt);

            vcStack.Children.Add(new TextBlock
            {
                Text = "Carpeta: " + InstallerProgram.InstallDir,
                Foreground = TextMuted,
                FontFamily = new FontFamily("Consolas"),
                FontSize = 10.2
            });
            verCard.Child = vcStack;
            root.Children.Add(verCard);

            // DEDICATED APBOPTIMIZER PRE-INSTALL SUITE CARD (OptiJuegos)
            bool apbBundled = !string.IsNullOrEmpty(InstallerProgram.ResolveApbScriptPath());
            var apbCard = new Border
            {
                CornerRadius = new CornerRadius(16),
                Background = new SolidColorBrush(Color.FromRgb(26, 24, 42)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(130, Violet.R, Violet.G, Violet.B)),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(14, 10, 14, 10),
                Margin = new Thickness(0, 0, 0, 10)
            };
            var apbStack = new StackPanel();
            var apbHead = new Grid();
            apbHead.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            apbHead.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            var apbTitleBox = new StackPanel();
            var apbTitleRow = new StackPanel { Orientation = Orientation.Horizontal };
            apbTitleRow.Children.Add(new TextBlock { Text = "OPTIMIZADOR WINDOWS · APBOptimizer", Foreground = new SolidColorBrush(VioletHover), FontSize = 10, FontWeight = FontWeights.Bold, VerticalAlignment = VerticalAlignment.Center });
            apbTitleRow.Children.Add(new Border
            {
                CornerRadius = new CornerRadius(6),
                Background = new SolidColorBrush(Color.FromArgb(45, Emerald.R, Emerald.G, Emerald.B)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(110, Emerald.R, Emerald.G, Emerald.B)),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(6, 1, 6, 1),
                Margin = new Thickness(8, 0, 0, 0),
                Child = new TextBlock { Text = apbBundled ? "Incluido offline" : "GitHub oficial", Foreground = new SolidColorBrush(Color.FromRgb(134, 239, 172)), FontSize = 9.2, FontWeight = FontWeights.SemiBold }
            });
            apbTitleBox.Children.Add(apbTitleRow);
            apbTitleBox.Children.Add(new TextBlock
            {
                Text = "Optimiza servicios, telemetría y rendimiento gaming antes de instalar AuraOS.",
                Foreground = new SolidColorBrush(Color.FromRgb(198, 200, 220)),
                FontSize = 10.8,
                Margin = new Thickness(0, 3, 10, 0),
                TextWrapping = TextWrapping.Wrap
            });
            apbHead.Children.Add(apbTitleBox);

            var runApbBtn = new Border
            {
                CornerRadius = new CornerRadius(10),
                Background = new SolidColorBrush(Color.FromArgb(55, Violet.R, Violet.G, Violet.B)),
                BorderBrush = new SolidColorBrush(Violet),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(12, 7, 12, 7),
                VerticalAlignment = VerticalAlignment.Center,
                Cursor = Cursors.Hand,
                ToolTip = "Abre APBOptimizer como Administrador de inmediato",
                Child = new TextBlock { Text = "Abrir APB ahora", Foreground = Brushes.White, FontSize = 11.2, FontWeight = FontWeights.SemiBold }
            };
            runApbBtn.MouseEnter += (s, e) => runApbBtn.Background = new SolidColorBrush(Violet);
            runApbBtn.MouseLeave += (s, e) => runApbBtn.Background = new SolidColorBrush(Color.FromArgb(55, Violet.R, Violet.G, Violet.B));
            runApbBtn.MouseLeftButtonDown += (s, e) => RunApbStandaloneAsync();
            Grid.SetColumn(runApbBtn, 1);
            apbHead.Children.Add(runApbBtn);

            apbStack.Children.Add(apbHead);
            apbStack.Children.Add(CreateToggle("Ejecutar APBOptimizer automáticamente antes de instalar AuraOS", _optApbBeforeInstall, v => _optApbBeforeInstall = v));
            apbCard.Child = apbStack;
            root.Children.Add(apbCard);

            // OPTIONS CARD
            var optCard = new Border
            {
                CornerRadius = new CornerRadius(16),
                Background = new SolidColorBrush(Color.FromRgb(23, 23, 31)),
                BorderBrush = new SolidColorBrush(Color.FromRgb(38, 38, 52)),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(14, 6, 14, 10),
                Margin = new Thickness(0, 0, 0, 10)
            };
            var optStack = new StackPanel();
            optStack.Children.Add(CreateToggle("Crear accesos directos en el Escritorio (AuraShell + AuraFiles)", _optDesktop, v => _optDesktop = v));
            optStack.Children.Add(CreateToggle("Integrar en Menú Inicio, APBOptimizer y Aplicaciones de Windows", _optStartMenu, v => _optStartMenu = v));
            optStack.Children.Add(CreateToggle("Iniciar AuraOS automáticamente al encender Windows", _optStartup, v => _optStartup = v));
            optStack.Children.Add(CreateToggle("Ocultar barra clásica de Windows (Ctrl+Alt+W restaura)", _optHideTaskbar, v => _optHideTaskbar = v));
            optStack.Children.Add(CreateToggle("Dock a la izquierda estilo Ubuntu (OFF = Abajo)", _optDockLeft, v => _optDockLeft = v));
            optStack.Children.Add(CreateToggle("Aplicar modo oscuro limpio y quitar Widgets de Windows", _optDarkPolish, v => _optDarkPolish = v));
            optStack.Children.Add(CreateToggle("Iniciar AuraOS automáticamente al terminar", _optLaunch, v => _optLaunch = v));
            optCard.Child = optStack;
            root.Children.Add(optCard);

            // LIVE STATUS BANNER
            _statusBanner = new Border
            {
                CornerRadius = new CornerRadius(12),
                Background = new SolidColorBrush(Color.FromRgb(26, 26, 36)),
                BorderBrush = new SolidColorBrush(CardBorder),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(12, 8, 12, 8),
                Margin = new Thickness(0, 0, 0, 12)
            };
            _statusText = new TextBlock
            {
                Text = "Listo · Incluye APBOptimizer (OptiJuegos) integrado + AuraOS 100% reversible.",
                Foreground = TextMuted,
                FontSize = 11.3,
                TextWrapping = TextWrapping.Wrap
            };
            _statusBanner.Child = _statusText;
            root.Children.Add(_statusBanner);

            // FOOTER ACTIONS
            var footer = new Grid();
            var leftActions = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Left };
            if (InstallerProgram.IsInstalled())
            {
                var uninstBtn = new Border
                {
                    CornerRadius = new CornerRadius(12),
                    Background = new SolidColorBrush(Color.FromArgb(40, Coral.R, Coral.G, Coral.B)),
                    BorderBrush = new SolidColorBrush(Color.FromArgb(95, Coral.R, Coral.G, Coral.B)),
                    BorderThickness = new Thickness(1),
                    Padding = new Thickness(13, 9, 13, 9),
                    Margin = new Thickness(0, 0, 8, 0),
                    Cursor = Cursors.Hand,
                    Child = new TextBlock { Text = "Desinstalar", Foreground = new SolidColorBrush(Color.FromRgb(253, 164, 175)), FontSize = 11.8, FontWeight = FontWeights.SemiBold }
                };
                uninstBtn.MouseLeftButtonDown += (s, e) => RunUninstallAsync();
                leftActions.Children.Add(uninstBtn);
            }

            string srcDir = InstallerProgram.ResolveSourceDir();
            if (File.Exists(Path.Combine(srcDir, "build.ps1")))
            {
                var recompileBtn = new Border
                {
                    CornerRadius = new CornerRadius(12),
                    Background = new SolidColorBrush(CardBg),
                    BorderBrush = new SolidColorBrush(CardBorder),
                    BorderThickness = new Thickness(1),
                    Padding = new Thickness(12, 9, 12, 9),
                    Cursor = Cursors.Hand,
                    ToolTip = "Recompila cualquier cambio en el código y actualiza tu instalación directamente",
                    Child = new TextBlock { Text = "Compilar + Actualizar", Foreground = Brushes.White, FontSize = 11.8, FontWeight = FontWeights.Medium }
                };
                recompileBtn.MouseEnter += (s, e) => recompileBtn.Background = new SolidColorBrush(Color.FromRgb(42, 42, 56));
                recompileBtn.MouseLeave += (s, e) => recompileBtn.Background = new SolidColorBrush(CardBg);
                recompileBtn.MouseLeftButtonDown += (s, e) => RunInstallOrUpdateAsync(true);
                leftActions.Children.Add(recompileBtn);
            }
            footer.Children.Add(leftActions);

            var rightActions = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
            _launchNowBtn = new Border
            {
                CornerRadius = new CornerRadius(12),
                Background = new SolidColorBrush(Color.FromRgb(30, 58, 44)),
                BorderBrush = new SolidColorBrush(Emerald),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(13, 9, 13, 9),
                Margin = new Thickness(0, 0, 8, 0),
                Cursor = Cursors.Hand,
                Visibility = Visibility.Collapsed,
                Child = new TextBlock { Text = "Abrir AuraOS", Foreground = Brushes.White, FontSize = 11.8, FontWeight = FontWeights.SemiBold }
            };
            _launchNowBtn.MouseLeftButtonDown += (s, e) =>
            {
                try
                {
                    string exe = Path.Combine(InstallerProgram.InstallDir, "AuraShell.exe");
                    if (File.Exists(exe))
                        Process.Start(new ProcessStartInfo { FileName = exe, WorkingDirectory = InstallerProgram.InstallDir, UseShellExecute = true });
                }
                catch { }
            };
            rightActions.Children.Add(_launchNowBtn);

            _primaryBtn = new Border
            {
                CornerRadius = new CornerRadius(12),
                Background = new SolidColorBrush(Violet),
                Padding = new Thickness(18, 10, 18, 10),
                Cursor = Cursors.Hand
            };
            _primaryBtnTxt = new TextBlock
            {
                Text = InstallerProgram.IsInstalled() ? "Actualizar versión" : "Optimizar e Instalar AuraOS",
                Foreground = Brushes.White,
                FontSize = 12.6,
                FontWeight = FontWeights.SemiBold
            };
            _primaryBtn.Child = _primaryBtnTxt;
            _primaryBtn.MouseEnter += (s, e) => { if (!_busy) _primaryBtn.Background = new SolidColorBrush(VioletHover); };
            _primaryBtn.MouseLeave += (s, e) => { if (!_busy) _primaryBtn.Background = new SolidColorBrush(Violet); };
            _primaryBtn.MouseLeftButtonDown += (s, e) => RunInstallOrUpdateAsync(false);
            rightActions.Children.Add(_primaryBtn);

            footer.Children.Add(rightActions);
            root.Children.Add(footer);

            outer.Child = root;
            Content = outer;
        }

        private void RefreshVersionInfo()
        {
            VersionManifest installed, available;
            bool hasNewer = InstallerProgram.HasNewerVersionAvailable(out installed, out available);

            if (installed != null)
                _installedVerTxt.Text = "v" + installed.Version + "  (" + installed.BuildTime + ")";
            else if (InstallerProgram.IsInstalled())
                _installedVerTxt.Text = "v3.0 (anterior)";
            else
                _installedVerTxt.Text = "Sin instalar";

            if (available != null)
            {
                _availableVerTxt.Text = "v" + available.Version + "  (" + available.BuildTime + ")";
                _changelogTxt.Text = string.IsNullOrWhiteSpace(available.Changelog) ? "" : ("Novedades: " + available.Changelog);
                _changelogTxt.Visibility = string.IsNullOrWhiteSpace(available.Changelog) ? Visibility.Collapsed : Visibility.Visible;
            }
            else
            {
                _availableVerTxt.Text = "v3.3.0";
                _changelogTxt.Visibility = Visibility.Collapsed;
            }

            if (!InstallerProgram.IsInstalled())
            {
                _versionBadge.Background = new SolidColorBrush(Violet);
                _versionBadgeTxt.Text = available != null ? ("v" + available.Version) : "v3.3.0";
                _primaryBtnTxt.Text = "Optimizar e Instalar";
            }
            else if (hasNewer)
            {
                _versionBadge.Background = new SolidColorBrush(Violet);
                _versionBadgeTxt.Text = "Actualización disponible";
                _primaryBtnTxt.Text = "Actualizar a v" + (available != null ? available.Version : "3.3.0");
            }
            else
            {
                _versionBadge.Background = new SolidColorBrush(Color.FromRgb(28, 126, 72));
                _versionBadgeTxt.Text = "Al día · v" + (installed != null ? installed.Version : "3.3.0");
                _primaryBtnTxt.Text = "Reinstalar / Aplicar ajustes";
            }
        }

        private async void RunApbStandaloneAsync()
        {
            if (_busy) return;
            _busy = true;
            SetBanner("Iniciando APBOptimizer como Administrador (acepta el aviso UAC)...", Violet, Brushes.White);
            string err = "";
            bool ok = await Task.Run(() =>
                InstallerProgram.LaunchApbOptimizer(false, step => Dispatcher.Invoke(() => SetBanner(step, Violet, Brushes.White)), out err));
            _busy = false;
            if (ok)
            {
                SetBanner("APBOptimizer abierto. Aplica las optimizaciones que desees y luego instala o actualiza AuraOS.", Emerald, new SolidColorBrush(Color.FromRgb(134, 239, 172)));
            }
            else
            {
                SetBanner(err, Amber, new SolidColorBrush(Color.FromRgb(253, 224, 71)));
            }
        }

        private async void RunInstallOrUpdateAsync(bool rebuildFirst)
        {
            if (_busy) return;
            _busy = true;
            _primaryBtn.Opacity = 0.65;
            _launchNowBtn.Visibility = Visibility.Collapsed;
            SetBanner("Preparando instalación...", CardBorder, TextMuted);

            bool runApb = _optApbBeforeInstall;
            bool desk = _optDesktop, start = _optStartMenu, startup = _optStartup;
            bool hideTb = _optHideTaskbar, dark = _optDarkPolish, launch = _optLaunch;
            string dock = _optDockLeft ? "Left" : "Bottom";

            string resultMsg = "";
            bool ok = await Task.Run(() =>
            {
                if (rebuildFirst)
                {
                    string buildErr;
                    bool built = InstallerProgram.RunBuildScriptInSource(
                        step => Dispatcher.Invoke(() => SetBanner(step, Violet, Brushes.White)),
                        out buildErr);
                    if (!built)
                    {
                        resultMsg = "Error al compilar: " + buildErr;
                        return false;
                    }
                }

                return InstallerProgram.PerformInstall(
                    runApb, desk, start, startup, hideTb, dark, dock, launch, true,
                    step => Dispatcher.Invoke(() => SetBanner(step, Violet, Brushes.White)),
                    out resultMsg);
            });

            _busy = false;
            _primaryBtn.Opacity = 1.0;
            RefreshVersionInfo();

            if (ok)
            {
                string extra = launch ? " AuraOS se inició en segundo plano — puedes cerrar esta ventana cuando quieras." : " Haz clic en 'Abrir AuraOS' para iniciarlo.";
                SetBanner(resultMsg + extra, Emerald, new SolidColorBrush(Color.FromRgb(134, 239, 172)));
                _launchNowBtn.Visibility = Visibility.Visible;
            }
            else
            {
                SetBanner(resultMsg, Coral, new SolidColorBrush(Color.FromRgb(253, 164, 175)));
            }
        }

        private async void RunUninstallAsync()
        {
            if (_busy) return;
            _busy = true;
            SetBanner("Desinstalando AuraOS y restaurando la barra de Windows...", Coral, Brushes.White);
            await Task.Run(() => InstallerProgram.PerformUninstall());
            _busy = false;
            RefreshVersionInfo();
            _launchNowBtn.Visibility = Visibility.Collapsed;
            SetBanner("AuraOS desinstalado y Windows restaurado correctamente.", Emerald, new SolidColorBrush(Color.FromRgb(134, 239, 172)));
        }

        private void SetBanner(string msg, Color borderColor, Brush textBrush)
        {
            _statusText.Text = msg;
            _statusText.Foreground = textBrush;
            _statusBanner.BorderBrush = new SolidColorBrush(borderColor);
        }

        private Grid CreateToggle(string label, bool initial, Action<bool> onChanged)
        {
            bool state = initial;
            var row = new Grid { Margin = new Thickness(0, 7, 0, 0), Cursor = Cursors.Hand, Background = Brushes.Transparent };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.Children.Add(new TextBlock { Text = label, Foreground = new SolidColorBrush(Color.FromRgb(235, 237, 245)), FontSize = 11.8, VerticalAlignment = VerticalAlignment.Center });

            var track = new Border
            {
                Width = 40, Height = 22, CornerRadius = new CornerRadius(11),
                Background = state ? new SolidColorBrush(Violet) : new SolidColorBrush(Color.FromRgb(44, 44, 58)),
                BorderBrush = new SolidColorBrush(Color.FromRgb(64, 64, 82)),
                BorderThickness = new Thickness(1)
            };
            var thumb = new Border { Width = 16, Height = 16, CornerRadius = new CornerRadius(8), Background = Brushes.White, HorizontalAlignment = HorizontalAlignment.Left, Margin = new Thickness(state ? 20 : 2, 0, 0, 0) };
            track.Child = thumb;
            Grid.SetColumn(track, 1);
            row.Children.Add(track);

            row.MouseLeftButtonDown += (s, e) =>
            {
                state = !state;
                thumb.Margin = new Thickness(state ? 20 : 2, 0, 0, 0);
                track.Background = state ? new SolidColorBrush(Violet) : new SolidColorBrush(Color.FromRgb(44, 44, 58));
                onChanged(state);
            };
            return row;
        }
    }
}
