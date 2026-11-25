using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Reflection;
using System.Runtime.InteropServices;
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

    public class InstallStateRecord
    {
        public bool ApbCompleted { get; set; }
        public string ApbCompletedAt { get; set; }
        public string HardwareTier { get; set; }
        public string LastInstalledVersion { get; set; }
        public List<string> InstalledModules { get; set; }
    }

    public enum HardwareTier
    {
        UltraGamer,
        BalancedLaptop,
        UltraLight
    }

    public class HardwareProfile
    {
        public string CpuName { get; set; }
        public int LogicalCores { get; set; }
        public int CpuMhz { get; set; }
        public double TotalRamGb { get; set; }
        public double AvailableRamGb { get; set; }
        public string GpuName { get; set; }
        public double GpuVramGb { get; set; }
        public bool HasDedicatedGpu { get; set; }
        public bool IsLaptop { get; set; }
        public int BatteryPct { get; set; }
        public int ScreenW { get; set; }
        public int ScreenH { get; set; }
        public int RefreshHz { get; set; }
        public int MonitorCount { get; set; }
        public double FreeDiskGb { get; set; }
        public string WindowsLabel { get; set; }
        public bool ExternalWallpaperEngineActive { get; set; }
        public HardwareTier RecommendedTier { get; set; }
        public string RecommendationSummary { get; set; }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
        private struct MEMORYSTATUSEX
        {
            public uint dwLength;
            public uint dwMemoryLoad;
            public ulong ullTotalPhys;
            public ulong ullAvailPhys;
            public ulong ullTotalPageFile;
            public ulong ullAvailPageFile;
            public ulong ullTotalVirtual;
            public ulong ullAvailVirtual;
            public ulong ullAvailExtendedVirtual;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Auto, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GlobalMemoryStatusEx(ref MEMORYSTATUSEX lpBuffer);

        [StructLayout(LayoutKind.Sequential)]
        private struct SYSTEM_POWER_STATUS
        {
            public byte ACLineStatus;
            public byte BatteryFlag;
            public byte BatteryLifePercent;
            public byte SystemStatusFlag;
            public int BatteryLifeTime;
            public int BatteryFullLifeTime;
        }

        [DllImport("kernel32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetSystemPowerStatus(out SYSTEM_POWER_STATUS sps);

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct DEVMODEW
        {
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string dmDeviceName;
            public short dmSpecVersion;
            public short dmDriverVersion;
            public short dmSize;
            public short dmDriverExtra;
            public int dmFields;
            public int dmPositionX;
            public int dmPositionY;
            public int dmDisplayOrientation;
            public int dmDisplayFixedOutput;
            public short dmColor;
            public short dmDuplex;
            public short dmYResolution;
            public short dmTTOption;
            public short dmCollate;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
            public string dmFormName;
            public short dmLogPixels;
            public int dmBitsPerPel;
            public int dmPelsWidth;
            public int dmPelsHeight;
            public int dmDisplayFlags;
            public int dmDisplayFrequency;
            public int dmICMMethod;
            public int dmICMIntent;
            public int dmMediaType;
            public int dmDitherType;
            public int dmReserved1;
            public int dmReserved2;
            public int dmPanningWidth;
            public int dmPanningHeight;
        }

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool EnumDisplaySettingsW(string lpszDeviceName, int iModeNum, ref DEVMODEW lpDevMode);

        public static HardwareProfile Scan()
        {
            var hp = new HardwareProfile
            {
                CpuName = "Procesador x64",
                LogicalCores = Math.Max(1, Environment.ProcessorCount),
                CpuMhz = 0,
                TotalRamGb = 8.0,
                AvailableRamGb = 4.0,
                GpuName = "GPU DirectX 11",
                GpuVramGb = 0.0,
                HasDedicatedGpu = false,
                IsLaptop = false,
                BatteryPct = 100,
                ScreenW = 1920,
                ScreenH = 1080,
                RefreshHz = 60,
                MonitorCount = 1,
                FreeDiskGb = 50.0,
                WindowsLabel = "Windows 10/11 x64"
            };

            // 1. CPU from Registry (instant, zero WMI lag!)
            try
            {
                using (var k = Registry.LocalMachine.OpenSubKey(@"HARDWARE\DESCRIPTION\System\CentralProcessor\0"))
                {
                    if (k != null)
                    {
                        var name = k.GetValue("ProcessorNameString") as string;
                        if (!string.IsNullOrWhiteSpace(name))
                            hp.CpuName = CollapseSpaces(name.Trim());
                        var mhz = k.GetValue("~MHz");
                        if (mhz != null) hp.CpuMhz = Convert.ToInt32(mhz);
                    }
                }
            }
            catch { }

            // 2. Physical RAM via GlobalMemoryStatusEx
            try
            {
                var ms = new MEMORYSTATUSEX { dwLength = (uint)Marshal.SizeOf(typeof(MEMORYSTATUSEX)) };
                if (GlobalMemoryStatusEx(ref ms))
                {
                    hp.TotalRamGb = Math.Round(ms.ullTotalPhys / 1073741824.0, 1);
                    hp.AvailableRamGb = Math.Round(ms.ullAvailPhys / 1073741824.0, 1);
                }
            }
            catch { }

            // 3. GPU & Dedicated VRAM from Display Class Registry
            try
            {
                string bestName = null;
                double bestVram = 0.0;
                bool bestDedicated = false;

                using (var cls = Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}"))
                {
                    if (cls != null)
                    {
                        foreach (string sub in cls.GetSubKeyNames())
                        {
                            if (sub.Length != 4) continue;
                            using (var dk = cls.OpenSubKey(sub))
                            {
                                if (dk == null) continue;
                                string desc = dk.GetValue("DriverDesc") as string;
                                if (string.IsNullOrWhiteSpace(desc)) continue;
                                string low = desc.ToLowerInvariant();
                                if (low.Contains("basic display") || low.Contains("virtual") || low.Contains("parsec") || low.Contains("remote"))
                                    continue;

                                double vramGb = 0.0;
                                object qw = dk.GetValue("HardwareInformation.qwMemorySize");
                                if (qw is long && (long)qw > 0) vramGb = Math.Round(((long)qw) / 1073741824.0, 1);
                                else if (qw is ulong && (ulong)qw > 0) vramGb = Math.Round(((ulong)qw) / 1073741824.0, 1);
                                else
                                {
                                    object msObj = dk.GetValue("HardwareInformation.MemorySize");
                                    if (msObj is int && (int)msObj > 0) vramGb = Math.Round((uint)(int)msObj / 1073741824.0, 1);
                                    else if (msObj is byte[] && ((byte[])msObj).Length >= 4)
                                    {
                                        uint uval = BitConverter.ToUInt32((byte[])msObj, 0);
                                        if (uval > 0) vramGb = Math.Round(uval / 1073741824.0, 1);
                                    }
                                }

                                bool dedicated = low.Contains("nvidia") || low.Contains("geforce") || low.Contains("rtx") || low.Contains("gtx")
                                    || low.Contains("quadro") || low.Contains("radeon rx") || low.Contains("intel arc")
                                    || vramGb >= 3.5;

                                if (bestName == null || (dedicated && !bestDedicated) || (dedicated == bestDedicated && vramGb > bestVram))
                                {
                                    bestName = desc.Trim();
                                    bestVram = vramGb;
                                    bestDedicated = dedicated;
                                }
                            }
                        }
                    }
                }
                if (!string.IsNullOrWhiteSpace(bestName))
                {
                    hp.GpuName = bestName;
                    hp.GpuVramGb = bestVram;
                    hp.HasDedicatedGpu = bestDedicated;
                }
            }
            catch { }

            // 4. Laptop Battery detection
            try
            {
                SYSTEM_POWER_STATUS sps;
                if (GetSystemPowerStatus(out sps))
                {
                    hp.IsLaptop = (sps.BatteryFlag != 128 && sps.BatteryFlag != 255);
                    if (hp.IsLaptop && sps.BatteryLifePercent <= 100)
                        hp.BatteryPct = sps.BatteryLifePercent;
                }
            }
            catch { }

            // 5. Primary Screen Resolution + Real Refresh Rate (Hz) + Monitor Count
            try
            {
                hp.MonitorCount = Math.Max(1, System.Windows.Forms.Screen.AllScreens.Length);
                var dm = new DEVMODEW { dmSize = (short)Marshal.SizeOf(typeof(DEVMODEW)) };
                if (EnumDisplaySettingsW(null, -1, ref dm))
                {
                    if (dm.dmPelsWidth > 0) hp.ScreenW = dm.dmPelsWidth;
                    if (dm.dmPelsHeight > 0) hp.ScreenH = dm.dmPelsHeight;
                    if (dm.dmDisplayFrequency >= 24) hp.RefreshHz = dm.dmDisplayFrequency;
                }
            }
            catch { }

            // 6. Free Disk Space
            try
            {
                string root = Path.GetPathRoot(InstallerProgram.InstallDir);
                if (!string.IsNullOrEmpty(root))
                {
                    var di = new DriveInfo(root);
                    hp.FreeDiskGb = Math.Round(di.AvailableFreeSpace / 1073741824.0, 1);
                }
            }
            catch { }

            // 7. Windows Build
            try
            {
                using (var cv = Registry.LocalMachine.OpenSubKey(@"SOFTWARE\Microsoft\Windows NT\CurrentVersion"))
                {
                    if (cv != null)
                    {
                        string buildStr = cv.GetValue("CurrentBuildNumber") as string ?? "";
                        string dispVer = cv.GetValue("DisplayVersion") as string ?? "";
                        int bnum = 0;
                        int.TryParse(buildStr, out bnum);
                        string winName = bnum >= 22000 ? "Windows 11" : "Windows 10";
                        hp.WindowsLabel = string.IsNullOrWhiteSpace(dispVer)
                            ? (winName + " (Build " + buildStr + ")")
                            : (winName + " " + dispVer + " (Build " + buildStr + ")");
                    }
                }
            }
            catch { }

            // 8. External Wallpaper Engine conflict check
            try
            {
                hp.ExternalWallpaperEngineActive = Process.GetProcessesByName("wallpaper64").Length > 0
                    || Process.GetProcessesByName("wallpaper32").Length > 0;
            }
            catch { }

            // 9. Classify Hardware Tier & Tailored Recommendation
            if (hp.TotalRamGb <= 8.5 || hp.LogicalCores <= 4)
            {
                hp.RecommendedTier = HardwareTier.UltraLight;
                hp.RecommendationSummary = "Detectamos un equipo modesto (" + hp.TotalRamGb.ToString("0.#") + " GB RAM · " + hp.LogicalCores + " hilos). Activaremos Modo Ultra-Ligero (<35 MB RAM, fondo estático 0% GPU y animaciones instantáneas).";
            }
            else if (hp.IsLaptop)
            {
                hp.RecommendedTier = HardwareTier.BalancedLaptop;
                hp.RecommendationSummary = "Detectamos una Laptop con batería (" + hp.TotalRamGb.ToString("0.#") + " GB RAM · " + hp.RefreshHz + " Hz). Activaremos indicador de batería, control de brillo, auto-pausa inteligente de fondos y motor aislado AuraWallpaper.";
            }
            else if (hp.TotalRamGb >= 15.0 && hp.LogicalCores >= 8 && hp.HasDedicatedGpu)
            {
                hp.RecommendedTier = HardwareTier.UltraGamer;
                hp.RecommendationSummary = "Tu PC es de Alto Rendimiento (" + hp.TotalRamGb.ToString("0.#") + " GB RAM · " + hp.GpuName + " · " + hp.RefreshHz + " Hz). Recomendamos fondos 60 FPS HD en proceso aislado AuraWallpaper.exe, Genie 120 FPS, Material You 3 y Game Boost.";
            }
            else
            {
                hp.RecommendedTier = HardwareTier.BalancedLaptop;
                hp.RecommendationSummary = "Detectamos un equipo Equilibrado (" + hp.TotalRamGb.ToString("0.#") + " GB RAM · " + hp.LogicalCores + " hilos). Configuraremos AuraOS con fluidez completa, auto-pausa de fondos y consumo mínimo de memoria.";
            }

            return hp;
        }

        private static string CollapseSpaces(string s)
        {
            var sb = new StringBuilder();
            bool prevSpace = false;
            foreach (char c in s)
            {
                if (char.IsWhiteSpace(c))
                {
                    if (!prevSpace) { sb.Append(' '); prevSpace = true; }
                }
                else { sb.Append(c); prevSpace = false; }
            }
            return sb.ToString();
        }
    }

    public class InstallOptions
    {
        public HardwareTier SelectedTier { get; set; }
        public bool RunApbOptimizerFirst { get; set; }
        public bool CreateDesktopShortcuts { get; set; }
        public bool CreateStartMenu { get; set; }
        public bool RegisterFileAssociations { get; set; }
        public bool StartWithWindows { get; set; }
        public bool HideTaskbarOnStart { get; set; }
        public bool ApplyWindowsDarkPolish { get; set; }
        public bool WallpaperAutoPause { get; set; }
        public string DockPosition { get; set; }
        public string BarStyle { get; set; }
        public int WidgetsPreset { get; set; } // 0 = Reloj + Música, 1 = Solo Reloj, 2 = Desactivados
        public bool LaunchAfter { get; set; }
        public bool PreserveExistingSettings { get; set; }
        public bool ApplyHardwareProfileToSettings { get; set; }
        public HashSet<string> SelectedModules { get; set; }

        public InstallOptions()
        {
            SelectedTier = HardwareTier.UltraGamer;
            CreateDesktopShortcuts = true;
            CreateStartMenu = true;
            RegisterFileAssociations = true;
            StartWithWindows = true;
            HideTaskbarOnStart = true;
            ApplyWindowsDarkPolish = true;
            WallpaperAutoPause = true;
            DockPosition = "Left";
            BarStyle = "Hug";
            WidgetsPreset = 0;
            LaunchAfter = true;
            PreserveExistingSettings = true;
            ApplyHardwareProfileToSettings = true;
            SelectedModules = new HashSet<string>(InstallerProgram.AllSuiteStems, StringComparer.OrdinalIgnoreCase);
        }
    }

    public static class InstallerProgram
    {
        public static readonly string InstallDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "AuraShell");

        public const string ApbScriptUrl = "https://raw.githubusercontent.com/OptiJuegos/host/refs/heads/main/APBOptimizer.ps1";
        public const string LatestReleaseApiUrl = "https://api.github.com/repos/dandifeto1111-prog/AuraOS/releases/latest";
        private static string _downloadedSourceDir = "";
        private const string UninstallRegKey = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\AuraShell";
        private static readonly UTF8Encoding Utf8NoBom = new UTF8Encoding(false);

        [DllImport("shell32.dll")]
        private static extern void SHChangeNotify(uint wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);

        public static readonly string[] CoreStems = new[]
        {
            "AuraShell", "AuraWallpaper", "AuraSettings"
        };

        public static readonly string[] AllSuiteStems = new[]
        {
            "AuraShell", "AuraWallpaper", "AuraSettings",
            "AuraFiles", "AuraTasks", "AuraPDF", "AuraPhotos",
            "AuraPlayer", "AuraNotes", "AuraShot", "AuraLock"
        };

        [STAThread]
        public static void Main(string[] args)
        {
            CleanupOldUpdaterBinary();

            if (args != null && args.Length > 0)
            {
                if (args[0].Equals("--install", StringComparison.OrdinalIgnoreCase))
                {
                    var hw = HardwareProfile.Scan();
                    var opts = BuildRecommendedOptions(hw, false);
                    opts.RunApbOptimizerFirst = false;
                    opts.LaunchAfter = false;
                    string err;
                    PerformInstall(opts, null, out err);
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
                    int step = 0;
                    if (args.Length >= 3) int.TryParse(args[2], out step);
                    CapturePreview(args[1], step);
                    return;
                }
            }

            var app = new Application();
            app.Run(new InstallerWindow());
        }

        public static InstallOptions BuildRecommendedOptions(HardwareProfile hw, bool isAlreadyInstalled)
        {
            var state = LoadInstallState();
            var opts = new InstallOptions
            {
                SelectedTier = hw.RecommendedTier,
                PreserveExistingSettings = isAlreadyInstalled,
                ApplyHardwareProfileToSettings = !isAlreadyInstalled,
                // CRITICAL: NEVER pre-check APBOptimizer when updating or if already completed once!
                RunApbOptimizerFirst = !isAlreadyInstalled && !state.ApbCompleted
            };

            if (isAlreadyInstalled)
            {
                bool hideTb, startup;
                string dock, bar;
                ReadExistingPrefs(out hideTb, out startup, out dock, out bar);
                opts.HideTaskbarOnStart = hideTb;
                opts.StartWithWindows = startup;
                opts.DockPosition = dock;
                opts.BarStyle = bar;
                if (state.InstalledModules != null && state.InstalledModules.Count > 0)
                {
                    opts.SelectedModules = new HashSet<string>(state.InstalledModules, StringComparer.OrdinalIgnoreCase);
                    foreach (string c in CoreStems) opts.SelectedModules.Add(c);
                }
            }
            else
            {
                ApplyTierToOptions(opts, hw.RecommendedTier);
            }
            return opts;
        }

        public static void ApplyTierToOptions(InstallOptions opts, HardwareTier tier)
        {
            opts.SelectedTier = tier;
            if (tier == HardwareTier.UltraLight)
            {
                opts.WidgetsPreset = 1; // Solo Reloj
                opts.WallpaperAutoPause = true;
                opts.BarStyle = "Rect";
            }
            else if (tier == HardwareTier.BalancedLaptop)
            {
                opts.WidgetsPreset = 0; // Reloj + Música
                opts.WallpaperAutoPause = true;
                opts.BarStyle = "Hug";
            }
            else
            {
                opts.WidgetsPreset = 0; // Reloj + Música
                opts.WallpaperAutoPause = true;
                opts.BarStyle = "Hug";
            }
        }

        public static InstallStateRecord LoadInstallState()
        {
            try
            {
                string path = Path.Combine(InstallDir, "install_state.json");
                if (File.Exists(path))
                {
                    var js = new JavaScriptSerializer();
                    var rec = js.Deserialize<InstallStateRecord>(File.ReadAllText(path).TrimStart('\uFEFF'));
                    if (rec != null) return rec;
                }
            }
            catch { }
            return new InstallStateRecord();
        }

        public static void SaveInstallState(InstallStateRecord rec)
        {
            try
            {
                Directory.CreateDirectory(InstallDir);
                var js = new JavaScriptSerializer();
                File.WriteAllText(Path.Combine(InstallDir, "install_state.json"), js.Serialize(rec), Utf8NoBom);
            }
            catch { }
        }

        public static void MarkApbCompleted()
        {
            var rec = LoadInstallState();
            rec.ApbCompleted = true;
            rec.ApbCompletedAt = DateTime.Now.ToString("yyyy-MM-dd HH:mm");
            SaveInstallState(rec);
        }

        private static void CleanupOldUpdaterBinary()
        {
            try
            {
                foreach (string f in new[] { "AuraShell-Setup.exe.old", "AuraFiles.exe.old", "AuraWallpaper.exe.old" })
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
            foreach (string cand in new[]
            {
                Path.Combine(srcDir, "APBOptimizer.ps1"),
                Path.Combine(srcDir, @"scripts\APBOptimizer.ps1"),
                Path.Combine(srcDir, @"..\scripts\APBOptimizer.ps1"),
                Path.Combine(InstallDir, "APBOptimizer.ps1")
            })
            {
                if (File.Exists(cand)) return cand;
            }
            return null;
        }

        public static bool EnsureApbScriptReady(Action<int, string> onProgress, out string localPath, out string errorMsg)
        {
            errorMsg = "";
            localPath = Path.Combine(InstallDir, "APBOptimizer.ps1");
            try
            {
                Directory.CreateDirectory(InstallDir);
                string existing = ResolveApbScriptPath();
                if (!string.IsNullOrEmpty(existing))
                {
                    if (!AreSamePath(existing, localPath)) File.Copy(existing, localPath, true);
                    return true;
                }

                if (onProgress != null) onProgress(8, "Descargando APBOptimizer oficial desde GitHub...");
                ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12 | SecurityProtocolType.Tls11 | SecurityProtocolType.Tls;
                using (var wc = new WebClient())
                {
                    wc.Headers.Add("User-Agent", "AuraOS-Installer/5.2");
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

        public static bool LaunchApbOptimizer(bool waitForExit, Action<int, string> onProgress, out string errorMsg)
        {
            errorMsg = "";
            string scriptPath;
            if (!EnsureApbScriptReady(onProgress, out scriptPath, out errorMsg))
                return false;

            try
            {
                if (onProgress != null)
                {
                    onProgress(12, waitForExit
                        ? "Ejecutando APBOptimizer (acepta UAC, aplica tus optimizaciones y ciérralo para continuar)..."
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

                if (waitForExit) proc.WaitForExit();
                MarkApbCompleted();
                return true;
            }
            catch (System.ComponentModel.Win32Exception ex)
            {
                errorMsg = ex.NativeErrorCode == 1223
                    ? "Permiso UAC cancelado para APBOptimizer."
                    : ("Error al iniciar APBOptimizer: " + ex.Message);
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
            catch { return null; }
        }

        public static string ResolveSourceDir()
        {
            string exeDir = NormalizeDir(AppDomain.CurrentDomain.BaseDirectory);
            string instDir = NormalizeDir(InstallDir);

            // 1. Organized package layout: Setup.exe at root + binaries cleanly inside app/
            string exeAppDir = NormalizeDir(Path.Combine(exeDir, "app"));
            if (!AreSamePath(exeAppDir, instDir) && File.Exists(Path.Combine(exeAppDir, "AuraShell.exe")))
                return exeAppDir;

            // 2. Legacy flat folder layout beside Setup.exe
            if (!AreSamePath(exeDir, instDir) && File.Exists(Path.Combine(exeDir, "AuraShell.exe")))
                return exeDir;

            // 3. Auto-downloaded payload from GitHub Releases (when running standalone Setup.exe)
            if (!string.IsNullOrEmpty(_downloadedSourceDir) && File.Exists(Path.Combine(_downloadedSourceDir, "AuraShell.exe")))
                return _downloadedSourceDir;

            // 4. Saved SourceDir from previous install
            var installedMeta = ReadManifest(instDir);
            if (installedMeta != null && !string.IsNullOrWhiteSpace(installedMeta.SourceDir))
            {
                foreach (string cand in new[] { NormalizeDir(installedMeta.SourceDir), NormalizeDir(Path.Combine(installedMeta.SourceDir, "app")) })
                {
                    if (!AreSamePath(cand, instDir) && Directory.Exists(cand) && File.Exists(Path.Combine(cand, "AuraShell.exe")))
                        return cand;
                }
            }

            // 5. Developer workspace paths
            string userProf = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            foreach (string rel in new[] { @".gemini\antigravity\scratch\AuraOS-Rust\app", @".gemini\antigravity\scratch\AuraOS-Rust", @".gemini\antigravity\scratch\AuraShell" })
            {
                string cand = Path.Combine(userProf, rel);
                if (Directory.Exists(cand) && File.Exists(Path.Combine(cand, "AuraShell.exe")))
                    return NormalizeDir(cand);
            }

            return Directory.Exists(exeAppDir) ? exeAppDir : exeDir;
        }

        public static bool EnsureSourcePayloadReady(Action<int, string> onProgress, out string sourceDir, out string errorMsg)
        {
            errorMsg = "";
            sourceDir = ResolveSourceDir();
            if (File.Exists(Path.Combine(sourceDir, "AuraShell.exe")))
                return true;

            try
            {
                if (onProgress != null) onProgress(6, "Descargando última versión de AuraOS desde GitHub...");
                ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12 | SecurityProtocolType.Tls11 | SecurityProtocolType.Tls;

                string json;
                using (var wc = new WebClient())
                {
                    wc.Headers.Add("User-Agent", "AuraOS-Installer/5.2.2");
                    json = wc.DownloadString(LatestReleaseApiUrl);
                }

                var js = new JavaScriptSerializer { MaxJsonLength = int.MaxValue };
                var rel = js.Deserialize<Dictionary<string, object>>(json);
                string zipUrl = "";
                string zipName = "AuraOS-latest.zip";
                if (rel != null && rel.ContainsKey("assets") && rel["assets"] is System.Collections.ArrayList)
                {
                    foreach (object item in (System.Collections.ArrayList)rel["assets"])
                    {
                        var asset = item as Dictionary<string, object>;
                        if (asset == null) continue;
                        string name = asset.ContainsKey("name") && asset["name"] != null ? asset["name"].ToString() : "";
                        string url = asset.ContainsKey("browser_download_url") && asset["browser_download_url"] != null ? asset["browser_download_url"].ToString() : "";
                        if (name.EndsWith(".zip", StringComparison.OrdinalIgnoreCase) && !string.IsNullOrEmpty(url))
                        {
                            zipName = name;
                            zipUrl = url;
                            break;
                        }
                    }
                }

                if (string.IsNullOrEmpty(zipUrl))
                {
                    errorMsg = "No se encontró el paquete .zip en la última Release de GitHub.";
                    return false;
                }

                string cacheRoot = Path.Combine(Path.GetTempPath(), "AuraOS-Setup-Cache");
                string extDir = Path.Combine(cacheRoot, Path.GetFileNameWithoutExtension(zipName));
                if (Directory.Exists(extDir)) { try { Directory.Delete(extDir, true); } catch { } }
                Directory.CreateDirectory(cacheRoot);

                string zipPath = Path.Combine(cacheRoot, zipName);
                if (onProgress != null) onProgress(10, "Descargando " + zipName + "...");
                using (var wc = new WebClient())
                {
                    wc.Headers.Add("User-Agent", "AuraOS-Installer/5.2.2");
                    wc.DownloadFile(zipUrl, zipPath);
                }

                if (onProgress != null) onProgress(16, "Descomprimiendo paquete oficial...");
                var psi = new ProcessStartInfo
                {
                    FileName = "powershell.exe",
                    Arguments = "-ExecutionPolicy Bypass -NoProfile -Command \"Expand-Archive -LiteralPath '" + zipPath.Replace("'", "''") + "' -DestinationPath '" + extDir.Replace("'", "''") + "' -Force\"",
                    UseShellExecute = false,
                    CreateNoWindow = true
                };
                using (var p = Process.Start(psi))
                {
                    if (p != null) p.WaitForExit(120000);
                }

                string candApp = Path.Combine(extDir, "app");
                if (File.Exists(Path.Combine(candApp, "AuraShell.exe"))) sourceDir = candApp;
                else if (File.Exists(Path.Combine(extDir, "AuraShell.exe"))) sourceDir = extDir;
                else
                {
                    errorMsg = "El paquete descargado no contiene AuraShell.exe.";
                    return false;
                }

                _downloadedSourceDir = sourceDir;
                return true;
            }
            catch (Exception ex)
            {
                errorMsg = "No se pudo descargar el paquete desde GitHub: " + ex.Message;
                return false;
            }
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
                    return File.GetLastWriteTimeUtc(srcExe) > File.GetLastWriteTimeUtc(dstExe).AddSeconds(2);
            }
            catch { }

            return false;
        }

        public static bool PerformQuickUpdate(bool relaunchShell, Action<int, string> onProgress, out string statusMessage)
        {
            bool alreadyInstalled = IsInstalled();
            var hw = HardwareProfile.Scan();
            var opts = BuildRecommendedOptions(hw, alreadyInstalled);
            opts.RunApbOptimizerFirst = false; // NEVER run or prompt APB on quick install / quick update!
            opts.LaunchAfter = relaunchShell;
            opts.PreserveExistingSettings = alreadyInstalled;
            opts.ApplyHardwareProfileToSettings = !alreadyInstalled;
            return PerformInstall(opts, onProgress, out statusMessage);
        }

        public static void ReadExistingPrefs(out bool hideTaskbar, out bool startWithWin, out string dockPos, out string barStyle)
        {
            hideTaskbar = true;
            startWithWin = true;
            dockPos = "Left";
            barStyle = "Hug";
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
                        if (d3.ContainsKey("bar_style") && d3["bar_style"] != null) barStyle = d3["bar_style"].ToString();
                        return;
                    }
                }
            }
            catch { }
        }

        public static bool RunBuildScriptInSource(Action<int, string> onProgress, out string errorMsg)
        {
            errorMsg = "";
            try
            {
                string srcDir = ResolveSourceDir();
                string buildPs1 = File.Exists(Path.Combine(srcDir, "build.ps1"))
                    ? Path.Combine(srcDir, "build.ps1")
                    : Path.Combine(srcDir, @"..\scripts\build.ps1");
                if (!File.Exists(buildPs1))
                {
                    errorMsg = "No se encontró scripts\\build.ps1 en " + srcDir;
                    return false;
                }
                string repoRoot = Path.GetFullPath(Path.Combine(Path.GetDirectoryName(buildPs1), ".."));

                if (onProgress != null) onProgress(10, "Compilando suite completa AuraOS desde código fuente...");
                var psi = new ProcessStartInfo
                {
                    FileName = "powershell.exe",
                    Arguments = "-ExecutionPolicy Bypass -NoProfile -File \"" + buildPs1 + "\"",
                    WorkingDirectory = Directory.Exists(repoRoot) ? repoRoot : srcDir,
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    CreateNoWindow = true
                };
                using (var p = Process.Start(psi))
                {
                    string stdout = p.StandardOutput.ReadToEnd();
                    string stderr = p.StandardError.ReadToEnd();
                    p.WaitForExit(300000);
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

        public static bool PerformInstall(InstallOptions opts, Action<int, string> onProgress, out string statusMessage)
        {
            statusMessage = "";
            Action<int, string> report = (pct, msg) => { if (onProgress != null) onProgress(pct, msg); };

            try
            {
                Directory.CreateDirectory(InstallDir);
                report(5, "Preparando destino: " + InstallDir);

                // Step 1 (Only if explicitly checked by user): APBOptimizer
                if (opts.RunApbOptimizerFirst)
                {
                    string apbErr;
                    bool apbOk = LaunchApbOptimizer(true, onProgress, out apbErr);
                    if (!apbOk)
                    {
                        report(15, "APBOptimizer omitido (" + apbErr + "). Continuando con AuraOS...");
                        Thread.Sleep(500);
                    }
                }

                string sourceDir;
                if (!EnsureSourcePayloadReady(onProgress, out sourceDir, out statusMessage))
                    return false;
                string srcShell = Path.Combine(sourceDir, "AuraShell.exe");

                report(20, "Deteniendo instancias previas de AuraShell y AuraWallpaper...");
                StopRunningApps();

                var chosen = new HashSet<string>(opts.SelectedModules ?? new HashSet<string>(AllSuiteStems), StringComparer.OrdinalIgnoreCase);
                foreach (string c in CoreStems) chosen.Add(c);

                int idx = 0;
                int total = chosen.Count;
                foreach (string stem in AllSuiteStems)
                {
                    if (!chosen.Contains(stem)) continue;
                    idx++;
                    int pct = 24 + (int)(idx * 40.0 / Math.Max(1, total));
                    string sExe = Path.Combine(sourceDir, stem + ".exe");
                    if (File.Exists(sExe))
                    {
                        report(pct, "Instalando " + stem + ".exe (" + idx + "/" + total + ")...");
                        SafeCopyFileWithRetry(sExe, Path.Combine(InstallDir, stem + ".exe"));
                    }
                    foreach (string extAsset in new[] { ".ico", "_256.png" })
                    {
                        string assetName = stem + extAsset;
                        string sAsset = Path.Combine(sourceDir, assetName);
                        if (!File.Exists(sAsset)) sAsset = Path.Combine(Path.Combine(sourceDir, @"assets\icons"), assetName);
                        if (!File.Exists(sAsset)) sAsset = Path.Combine(Path.Combine(sourceDir, @"..\assets\icons"), assetName);
                        if (File.Exists(sAsset)) SafeCopyFileWithRetry(sAsset, Path.Combine(InstallDir, assetName));
                    }
                }

                // Copy AuraBeats Studio icon asset so Aura Studio Hub always displays the crisp HD icon
                foreach (string studioAsset in new[] { "AuraBeats_256.png", "AuraBeats.ico" })
                {
                    string sAsset = Path.Combine(sourceDir, studioAsset);
                    if (!File.Exists(sAsset)) sAsset = Path.Combine(Path.Combine(sourceDir, @"assets\icons"), studioAsset);
                    if (!File.Exists(sAsset)) sAsset = Path.Combine(Path.Combine(sourceDir, @"..\assets\icons"), studioAsset);
                    if (File.Exists(sAsset)) SafeCopyFileWithRetry(sAsset, Path.Combine(InstallDir, studioAsset));
                }

                report(68, "Copiando scripts de restauración, manifiesto y actualizador...");
                string dstRestore = Path.Combine(InstallDir, "restore-windows.ps1");
                string dstApb = Path.Combine(InstallDir, "APBOptimizer.ps1");
                string dstSetup = Path.Combine(InstallDir, "AuraShell-Setup.exe");
                string dstVersion = Path.Combine(InstallDir, "version.json");
                string sRestore = File.Exists(Path.Combine(sourceDir, "restore-windows.ps1")) ? Path.Combine(sourceDir, "restore-windows.ps1") : Path.Combine(sourceDir, @"..\scripts\restore-windows.ps1");
                string sApb = File.Exists(Path.Combine(sourceDir, "APBOptimizer.ps1")) ? Path.Combine(sourceDir, "APBOptimizer.ps1") : Path.Combine(sourceDir, @"..\scripts\APBOptimizer.ps1");
                if (File.Exists(sRestore)) SafeCopyFileWithRetry(sRestore, dstRestore);
                if (File.Exists(sApb)) SafeCopyFileWithRetry(sApb, dstApb);
                if (File.Exists(Path.Combine(sourceDir, "version.json"))) SafeCopyFileWithRetry(Path.Combine(sourceDir, "version.json"), dstVersion);

                string srcSetup = Path.Combine(sourceDir, "AuraShell-Setup.exe");
                if (!File.Exists(srcSetup)) srcSetup = Path.Combine(sourceDir, @"..\AuraShell-Setup.exe");
                if (!File.Exists(srcSetup)) srcSetup = Assembly.GetExecutingAssembly().Location;
                SafeCopySelfUpdater(srcSetup, dstSetup);

                // Apply hardware-adapted settings cleanly (preserving existing user customizations on update!)
                report(74, "Adaptando perfil de hardware (" + TierLabel(opts.SelectedTier) + ") a settings_v3.json...");
                var js = new JavaScriptSerializer();
                string v3Path = Path.Combine(InstallDir, "settings_v3.json");
                Dictionary<string, object> v3Obj = null;
                if (opts.PreserveExistingSettings && File.Exists(v3Path))
                {
                    try { v3Obj = js.Deserialize<Dictionary<string, object>>(File.ReadAllText(v3Path).TrimStart('\uFEFF')); } catch { }
                }
                if (v3Obj == null)
                {
                    bool isLight = opts.SelectedTier == HardwareTier.UltraLight;
                    v3Obj = new Dictionary<string, object>
                    {
                        { "glass_opacity", isLight ? 0.90 : 0.78 },
                        { "auto_game_boost", opts.SelectedTier == HardwareTier.UltraGamer },
                        { "dock_auto_hide", true },
                        { "show_desktop_widgets", opts.WidgetsPreset != 2 },
                        { "last_mode", "PC" },
                        { "style_windows_apps", true },
                        { "wallpaper_id", isLight ? "preset:matte_grid" : "preset:aura_obsidian_flow" },
                        { "wallpaper_auto_pause", opts.WallpaperAutoPause },
                        { "performance_mode", isLight },
                        { "theme_scheme", "Auto" },
                        { "clock_widget_style", "Vertical" }
                    };
                }
                else if (opts.ApplyHardwareProfileToSettings)
                {
                    bool isLight = opts.SelectedTier == HardwareTier.UltraLight;
                    v3Obj["performance_mode"] = isLight;
                    v3Obj["wallpaper_auto_pause"] = opts.WallpaperAutoPause;
                    v3Obj["show_desktop_widgets"] = opts.WidgetsPreset != 2;
                    v3Obj["auto_game_boost"] = opts.SelectedTier == HardwareTier.UltraGamer;
                }
                v3Obj["hide_windows_taskbar"] = opts.HideTaskbarOnStart;
                v3Obj["start_with_windows"] = opts.StartWithWindows;
                v3Obj["dock_position"] = opts.DockPosition ?? "Left";
                v3Obj["bar_style"] = opts.BarStyle ?? "Hug";
                File.WriteAllText(v3Path, js.Serialize(v3Obj), Utf8NoBom);

                if (opts.ApplyWindowsDarkPolish)
                {
                    report(80, "Aplicando modo oscuro limpio y ocultando widgets de Windows...");
                    ApplySafeWindowsPolish();
                }

                string dstShell = Path.Combine(InstallDir, "AuraShell.exe");
                string dstFiles = Path.Combine(InstallDir, "AuraFiles.exe");
                string dstPhotos = Path.Combine(InstallDir, "AuraPhotos.exe");
                string dstPlayer = Path.Combine(InstallDir, "AuraPlayer.exe");
                string dstNotes = Path.Combine(InstallDir, "AuraNotes.exe");
                string dstSettings = Path.Combine(InstallDir, "AuraSettings.exe");
                string dstLock = Path.Combine(InstallDir, "AuraLock.exe");
                string dstTasks = Path.Combine(InstallDir, "AuraTasks.exe");
                string dstPdf = Path.Combine(InstallDir, "AuraPDF.exe");
                string dstShot = Path.Combine(InstallDir, "AuraShot.exe");
                string shellIconRef = IconFor("AuraShell", dstShell);
                string filesIconRef = IconFor("AuraFiles", dstFiles);

                report(84, "Creando accesos directos y menú Inicio...");
                if (opts.CreateDesktopShortcuts)
                {
                    string desktop = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                    CreateShortcut(Path.Combine(desktop, "AuraShell.lnk"), dstShell, "", InstallDir, shellIconRef, "AuraOS — Escritorio Nativo Rust");
                    if (File.Exists(dstFiles))
                        CreateShortcut(Path.Combine(desktop, "AuraFiles.lnk"), dstFiles, "", InstallDir, filesIconRef, "AuraFiles — Gestor de Archivos & Analizador de Disco");
                }

                if (opts.CreateStartMenu)
                {
                    string startFolder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), "AuraShell");
                    Directory.CreateDirectory(startFolder);
                    CreateShortcut(Path.Combine(startFolder, "AuraShell.lnk"), dstShell, "", InstallDir, shellIconRef, "Iniciar entorno AuraOS");
                    if (File.Exists(dstFiles)) CreateShortcut(Path.Combine(startFolder, "AuraFiles.lnk"), dstFiles, "", InstallDir, IconFor("AuraFiles", dstFiles), "Gestor de Archivos AuraFiles");
                    if (File.Exists(dstPdf)) CreateShortcut(Path.Combine(startFolder, "AuraPDF Studio.lnk"), dstPdf, "", InstallDir, IconFor("AuraPDF", dstPdf), "Visor y Editor PDF AuraPDF");
                    if (File.Exists(dstPhotos)) CreateShortcut(Path.Combine(startFolder, "AuraPhotos Studio.lnk"), dstPhotos, "", InstallDir, IconFor("AuraPhotos", dstPhotos), "Visor y Editor GPU AuraPhotos");
                    if (File.Exists(dstShot)) CreateShortcut(Path.Combine(startFolder, "AuraShot Capturas.lnk"), dstShot, "", InstallDir, IconFor("AuraShot", dstShot), "Captura y OCR WinRT AuraShot");
                    if (File.Exists(dstPlayer)) CreateShortcut(Path.Combine(startFolder, "AuraPlayer.lnk"), dstPlayer, "", InstallDir, IconFor("AuraPlayer", dstPlayer), "Reproductor Multimedia AuraPlayer");
                    if (File.Exists(dstNotes)) CreateShortcut(Path.Combine(startFolder, "AuraNotes.lnk"), dstNotes, "", InstallDir, IconFor("AuraNotes", dstNotes), "Editor instantáneo AuraNotes");
                    if (File.Exists(dstSettings)) CreateShortcut(Path.Combine(startFolder, "Configuración AuraOS.lnk"), dstSettings, "", InstallDir, IconFor("AuraSettings", dstSettings), "Configuración nativa AuraOS");
                    if (File.Exists(dstTasks)) CreateShortcut(Path.Combine(startFolder, "AuraTasks (Tareas).lnk"), dstTasks, "", InstallDir, IconFor("AuraTasks", dstTasks), "Administrador de Tareas AuraTasks");
                    if (File.Exists(dstLock)) CreateShortcut(Path.Combine(startFolder, "AuraLock (Bloquear).lnk"), dstLock, "", InstallDir, IconFor("AuraLock", dstLock), "Pantalla de bloqueo GPU AuraLock");
                    CreateShortcut(Path.Combine(startFolder, "Actualizar o Configurar AuraOS.lnk"), dstSetup, "", InstallDir, shellIconRef, "Actualizar, adaptar hardware o desinstalar AuraOS");
                    if (File.Exists(dstApb)) CreateShortcut(Path.Combine(startFolder, "Optimizar Windows (APBOptimizer).lnk"), "powershell.exe", "-ExecutionPolicy Bypass -NoProfile -File \"" + dstApb + "\"", InstallDir, shellIconRef, "Abrir APBOptimizer (OptiJuegos)");
                    if (File.Exists(dstRestore)) CreateShortcut(Path.Combine(startFolder, "Restaurar Barra de Windows.lnk"), "powershell.exe", "-ExecutionPolicy Bypass -File \"" + dstRestore + "\"", InstallDir, shellIconRef, "Restaurar barra clásica de Windows");
                }

                // Zero-delay direct logon boot (HKCU\Run + StartupDelayInMSec = 0, 100% Vanguard-safe!)
                report(88, "Configurando arranque instantáneo y asociaciones de archivos...");
                string startupLnk = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Startup), "AuraShell.lnk");
                try { if (File.Exists(startupLnk)) File.Delete(startupLnk); } catch { }
                try
                {
                    using (var runKey = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run"))
                    {
                        if (runKey != null)
                        {
                            if (opts.StartWithWindows) runKey.SetValue("AuraShell", "\"" + dstShell + "\"");
                            else runKey.DeleteValue("AuraShell", false);
                        }
                    }
                    if (opts.StartWithWindows)
                    {
                        using (var serKey = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize"))
                        {
                            if (serKey != null) serKey.SetValue("StartupDelayInMSec", 0, RegistryValueKind.DWord);
                        }
                    }
                }
                catch { }

                if (opts.RegisterFileAssociations)
                {
                    if (File.Exists(dstFiles))
                    {
                        try
                        {
                            using (var k = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\shell\open_aurafiles"))
                            { k.SetValue("", "Abrir en AuraFiles"); k.SetValue("Icon", filesIconRef); }
                            using (var cmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\shell\open_aurafiles\command"))
                            { cmd.SetValue("", "\"" + dstFiles + "\" \"%1\""); }
                            using (var root = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\shell"))
                            { root.SetValue("", "open_aurafiles"); }
                            using (var bg = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\Background\shell\open_aurafiles"))
                            { bg.SetValue("", "Abrir carpeta en AuraFiles"); bg.SetValue("Icon", filesIconRef); }
                            using (var bgCmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\Directory\Background\shell\open_aurafiles\command"))
                            { bgCmd.SetValue("", "\"" + dstFiles + "\" \"%V\""); }
                        }
                        catch { }
                    }
                    RegisterSuiteFileAssociations(dstPdf, dstPhotos, dstPlayer, dstNotes);
                    try { SHChangeNotify(0x08000000, 0, IntPtr.Zero, IntPtr.Zero); } catch { }
                }

                var verMeta = ReadManifest(InstallDir) ?? ReadManifest(sourceDir);
                string verLabel = verMeta != null && !string.IsNullOrEmpty(verMeta.Version) ? verMeta.Version : "5.2.2";

                using (RegistryKey key = Registry.CurrentUser.CreateSubKey(UninstallRegKey))
                {
                    if (key != null)
                    {
                        key.SetValue("DisplayName", "AuraOS (Suite Nativa Rust + AuraWallpaper)");
                        key.SetValue("DisplayVersion", verLabel);
                        key.SetValue("Publisher", "AuraOS");
                        key.SetValue("DisplayIcon", shellIconRef);
                        key.SetValue("InstallLocation", InstallDir);
                        key.SetValue("UninstallString", "\"" + dstSetup + "\" --uninstall");
                        key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                        key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                    }
                }

                // Persist install_state.json so future updates remember chosen modules & APB completion!
                var stRec = LoadInstallState();
                stRec.HardwareTier = opts.SelectedTier.ToString();
                stRec.LastInstalledVersion = verLabel;
                stRec.InstalledModules = new List<string>(chosen);
                SaveInstallState(stRec);

                if (opts.LaunchAfter)
                {
                    report(96, "Iniciando AuraOS v" + verLabel + "...");
                    Process.Start(new ProcessStartInfo { FileName = dstShell, WorkingDirectory = InstallDir, UseShellExecute = true });
                }

                report(100, "¡AuraOS v" + verLabel + " listo y optimizado para tu PC!");
                statusMessage = "AuraOS v" + verLabel + " instalado correctamente (" + chosen.Count + " módulos · perfil " + TierLabel(opts.SelectedTier) + ").";
                return true;
            }
            catch (Exception ex)
            {
                statusMessage = "Error al instalar: " + ex.Message;
                return false;
            }
        }

        public static string TierLabel(HardwareTier t)
        {
            if (t == HardwareTier.UltraGamer) return "Ultra / Gamer";
            if (t == HardwareTier.BalancedLaptop) return "Equilibrado / Laptop";
            return "Ultra-Ligero";
        }

        private static void SafeCopyFileWithRetry(string src, string dst)
        {
            if (string.IsNullOrWhiteSpace(src) || string.IsNullOrWhiteSpace(dst)) return;
            if (!File.Exists(src) || AreSamePath(src, dst)) return;

            Exception lastEx = null;
            for (int attempt = 0; attempt < 8; attempt++)
            {
                try { File.Copy(src, dst, true); return; }
                catch (IOException ex)
                {
                    lastEx = ex;
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
                else SafeCopyFileWithRetry(srcSetup, dstSetup);
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
            try { return string.Equals(Path.GetFullPath(a).TrimEnd('\\', '/'), Path.GetFullPath(b).TrimEnd('\\', '/'), StringComparison.OrdinalIgnoreCase); }
            catch { return string.Equals(a, b, StringComparison.OrdinalIgnoreCase); }
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

                foreach (string stem in AllSuiteStems)
                {
                    foreach (string ext in new[] { ".exe", ".ico", "_256.png" })
                    {
                        string p = Path.Combine(InstallDir, stem + ext);
                        try { if (File.Exists(p)) File.Delete(p); } catch { }
                    }
                }
                foreach (string f in new[] { "settings.json", "settings_v3.json", "install_state.json", "version.json", "AuraShell.log", "restore-windows.ps1", "APBOptimizer.ps1" })
                {
                    string p = Path.Combine(InstallDir, f);
                    try { if (File.Exists(p)) File.Delete(p); } catch { }
                }
                UnregisterSuiteFileAssociations();
            }
            catch { }
        }

        private static string IconFor(string stem, string fallbackExe)
        {
            string ico = Path.Combine(InstallDir, stem + ".ico");
            return File.Exists(ico) ? ico : (fallbackExe + ",0");
        }

        private static void RegisterSuiteFileAssociations(string dstPdf, string dstPhotos, string dstPlayer, string dstNotes)
        {
            try
            {
                if (File.Exists(dstPdf))
                {
                    string pdfIco = IconFor("AuraPDF", dstPdf);
                    RegisterProgId("AuraOS.PDF", "Documento PDF (AuraPDF)", dstPdf, pdfIco);
                    RegisterExtAssociation(".pdf", "AuraOS.PDF", "Abrir y editar en AuraPDF", dstPdf, pdfIco);
                }
                if (File.Exists(dstPhotos))
                {
                    string phIco = IconFor("AuraPhotos", dstPhotos);
                    RegisterProgId("AuraOS.Photos", "Imagen (AuraPhotos Studio)", dstPhotos, phIco);
                    foreach (string ext in new[] { ".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".ico" })
                        RegisterExtAssociation(ext, "AuraOS.Photos", "Abrir con AuraPhotos Studio", dstPhotos, phIco);
                }
                if (File.Exists(dstPlayer))
                {
                    string plIco = IconFor("AuraPlayer", dstPlayer);
                    RegisterProgId("AuraOS.Player", "Multimedia (AuraPlayer)", dstPlayer, plIco);
                    foreach (string ext in new[] { ".mp4", ".mkv", ".webm", ".mov", ".avi", ".wmv", ".mp3", ".wav", ".flac", ".ogg" })
                        RegisterExtAssociation(ext, "AuraOS.Player", "Reproducir en AuraPlayer", dstPlayer, plIco);
                }
                if (File.Exists(dstNotes))
                {
                    string ntIco = IconFor("AuraNotes", dstNotes);
                    RegisterProgId("AuraOS.Notes", "Documento de texto (AuraNotes)", dstNotes, ntIco);
                    foreach (string ext in new[] { ".txt", ".md", ".json", ".log", ".ini", ".cfg", ".toml", ".yaml", ".ps1" })
                        RegisterExtAssociation(ext, "AuraOS.Notes", "Editar en AuraNotes", dstNotes, ntIco);
                }
            }
            catch { }
        }

        private static void RegisterProgId(string progId, string friendlyName, string exePath, string iconRef)
        {
            using (var k = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId)) k.SetValue("", friendlyName);
            using (var ic = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId + @"\DefaultIcon")) ic.SetValue("", iconRef);
            using (var cmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + progId + @"\shell\open\command")) cmd.SetValue("", "\"" + exePath + "\" \"%1\"");
        }

        private static void RegisterExtAssociation(string ext, string progId, string verbLabel, string exePath, string iconRef)
        {
            using (var openWith = Registry.CurrentUser.CreateSubKey(@"Software\Classes\" + ext + @"\OpenWithProgids")) openWith.SetValue(progId, "", RegistryValueKind.String);
            using (var menu = Registry.CurrentUser.CreateSubKey(@"Software\Classes\SystemFileAssociations\" + ext + @"\shell\open_auraos"))
            { menu.SetValue("", verbLabel); menu.SetValue("Icon", iconRef); }
            using (var mcmd = Registry.CurrentUser.CreateSubKey(@"Software\Classes\SystemFileAssociations\" + ext + @"\shell\open_auraos\command"))
            { mcmd.SetValue("", "\"" + exePath + "\" \"%1\""); }
        }

        private static void UnregisterSuiteFileAssociations()
        {
            foreach (string pid in new[] { "AuraOS.PDF", "AuraOS.Photos", "AuraOS.Player", "AuraOS.Notes" })
                try { Registry.CurrentUser.DeleteSubKeyTree(@"Software\Classes\" + pid, false); } catch { }
            foreach (string ext in new[] { ".pdf", ".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".ico", ".mp4", ".mkv", ".webm", ".mov", ".avi", ".wmv", ".mp3", ".wav", ".flac", ".ogg", ".txt", ".md", ".json", ".log", ".ini", ".cfg", ".toml", ".yaml", ".ps1" })
                try { Registry.CurrentUser.DeleteSubKeyTree(@"Software\Classes\SystemFileAssociations\" + ext + @"\shell\open_auraos", false); } catch { }
        }

        public static void StopRunningApps()
        {
            foreach (string procName in new[] { "AuraWallpaper", "AuraShell" })
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
                                if (!p.WaitForExit(1000))
                                {
                                    p.Kill();
                                    p.WaitForExit(3000);
                                }
                            }
                        }
                        catch { }
                        finally { try { p.Dispose(); } catch { } }
                    }
                }
                catch { }
            }
            Thread.Sleep(300);
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

        private static void CapturePreview(string outPath, int step)
        {
            try
            {
                var win = new InstallerWindow(step);
                var visual = (UIElement)win.Content;
                win.Content = null;
                var host = new Border { Width = 756, Background = new SolidColorBrush(Color.FromRgb(11, 11, 16)), Child = visual };
                host.Measure(new Size(756, 1100));
                host.Arrange(new Rect(0, 0, 756, host.DesiredSize.Height));
                host.UpdateLayout();
                int h = Math.Max(580, (int)Math.Ceiling(host.DesiredSize.Height));
                var rtb = new RenderTargetBitmap(756, h, 96, 96, PixelFormats.Pbgra32);
                rtb.Render(host);
                var enc = new PngBitmapEncoder();
                enc.Frames.Add(BitmapFrame.Create(rtb));
                Directory.CreateDirectory(Path.GetDirectoryName(outPath));
                using (var fs = File.Create(outPath)) enc.Save(fs);
            }
            catch { }
        }
    }

    // ==================== FLAGSHIP 4-STEP HARDWARE-AWARE INSTALLER WIZARD ====================
    public class InstallerWindow : Window
    {
        private static readonly Color BgCanvas = Color.FromRgb(18, 18, 25);
        private static readonly Color CardBg = Color.FromRgb(26, 26, 36);
        private static readonly Color CardSubtle = Color.FromRgb(22, 22, 31);
        private static readonly Color CardBorder = Color.FromRgb(46, 46, 64);
        private static readonly Color Violet = Color.FromRgb(124, 92, 255);
        private static readonly Color VioletHover = Color.FromRgb(145, 116, 255);
        private static readonly Color Emerald = Color.FromRgb(34, 197, 94);
        private static readonly Color Amber = Color.FromRgb(245, 158, 11);
        private static readonly Color Coral = Color.FromRgb(242, 109, 125);
        private static readonly SolidColorBrush TextPrimary = new SolidColorBrush(Color.FromRgb(244, 245, 252));
        private static readonly SolidColorBrush TextMuted = new SolidColorBrush(Color.FromRgb(154, 158, 182));

        private readonly HardwareProfile _hw;
        private readonly InstallOptions _opts;
        private readonly InstallStateRecord _installState;
        private readonly bool _isInstalled;
        private int _step = 0; // 0 = Sistema & Hardware, 1 = Módulos, 2 = Integración & APB, 3 = Instalación
        private bool _busy = false;
        private bool _finishedOk = false;

        private Border _versionBadge;
        private TextBlock _versionBadgeTxt;
        private Grid _stepperGrid;
        private ContentControl _stepBody;
        private StackPanel _footerLeft;
        private StackPanel _footerRight;

        // Live Step 4 UI refs
        private Border _progressFill;
        private TextBlock _progressPctTxt;
        private TextBlock _progressHeadlineTxt;
        private TextBox _installLogBox;
        private Border _finishOpenBtn;

        public InstallerWindow() : this(0) { }

        public InstallerWindow(int initialStep)
        {
            Title = "AuraOS v5.2 — Instalador Inteligente";
            Width = 748;
            SizeToContent = SizeToContent.Height;
            WindowStartupLocation = WindowStartupLocation.CenterScreen;
            WindowStyle = WindowStyle.None;
            AllowsTransparency = true;
            Background = Brushes.Transparent;
            ResizeMode = ResizeMode.NoResize;
            FontFamily = new FontFamily("Segoe UI Variable Text, Segoe UI, Arial");

            _isInstalled = InstallerProgram.IsInstalled();
            _installState = InstallerProgram.LoadInstallState();
            _hw = HardwareProfile.Scan();
            _opts = InstallerProgram.BuildRecommendedOptions(_hw, _isInstalled);
            _step = Math.Max(0, Math.Min(3, initialStep));

            BuildShell();
            RenderStep();
        }

        private void BuildShell()
        {
            var outer = new Border
            {
                Margin = new Thickness(14),
                CornerRadius = new CornerRadius(24),
                BorderThickness = new Thickness(1),
                BorderBrush = new SolidColorBrush(CardBorder),
                Background = new SolidColorBrush(BgCanvas),
                Effect = new System.Windows.Media.Effects.DropShadowEffect { Color = Colors.Black, BlurRadius = 32, ShadowDepth = 8, Opacity = 0.62 }
            };

            var root = new StackPanel { Margin = new Thickness(24, 20, 24, 20) };

            // 1. HEADER + UNIFIED TRAFFIC LIGHTS
            var header = new Grid { Background = Brushes.Transparent, Cursor = Cursors.SizeAll, Margin = new Thickness(0, 0, 0, 14) };
            header.MouseLeftButtonDown += (s, e) => { try { DragMove(); } catch { } };

            var titleStack = new StackPanel();
            var titleRow = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
            titleRow.Children.Add(new TextBlock { Text = "AuraOS", Foreground = Brushes.White, FontSize = 23, FontWeight = FontWeights.Bold });
            _versionBadge = new Border
            {
                CornerRadius = new CornerRadius(8),
                Background = new SolidColorBrush(Violet),
                Padding = new Thickness(9, 3, 9, 3),
                Margin = new Thickness(10, 0, 0, 0),
                VerticalAlignment = VerticalAlignment.Center
            };
            _versionBadgeTxt = new TextBlock { Text = "v5.2.2", Foreground = Brushes.White, FontSize = 10.6, FontWeight = FontWeights.SemiBold };
            _versionBadge.Child = _versionBadgeTxt;
            titleRow.Children.Add(_versionBadge);
            titleStack.Children.Add(titleRow);
            titleStack.Children.Add(new TextBlock
            {
                Text = "Asistente Inteligente de Instalación, Adaptación de Hardware y Actualizaciones",
                Foreground = TextMuted,
                FontSize = 11.6,
                Margin = new Thickness(0, 3, 0, 0)
            });
            header.Children.Add(titleStack);

            // macOS Traffic Lights (Native AuraOS Window!)
            var tlPanel = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, VerticalAlignment = VerticalAlignment.Top };
            var tlGlyphs = new List<TextBlock>();
            Action<Color, string, Action> addLight = (col, glyph, onClick) =>
            {
                var gTxt = new TextBlock
                {
                    Text = glyph, Foreground = new SolidColorBrush(Color.FromRgb(22, 16, 20)), FontSize = 9.5,
                    FontWeight = FontWeights.Bold, HorizontalAlignment = HorizontalAlignment.Center,
                    VerticalAlignment = VerticalAlignment.Center, Opacity = 0.0, Margin = new Thickness(0, -1, 0, 0)
                };
                tlGlyphs.Add(gTxt);
                var dot = new Border
                {
                    Width = 14, Height = 14, CornerRadius = new CornerRadius(7),
                    Background = new SolidColorBrush(col), Margin = new Thickness(6, 2, 0, 0),
                    Cursor = Cursors.Hand, Child = gTxt
                };
                dot.MouseLeftButtonDown += (s, e) => { e.Handled = true; onClick(); };
                tlPanel.Children.Add(dot);
            };
            addLight(Coral, "×", () => { if (!_busy) Close(); });
            addLight(Color.FromRgb(251, 191, 36), "−", () => WindowState = WindowState.Minimized);
            addLight(Emerald, "+", () => { });
            tlPanel.MouseEnter += (s, e) => { foreach (var g in tlGlyphs) g.Opacity = 1.0; };
            tlPanel.MouseLeave += (s, e) => { foreach (var g in tlGlyphs) g.Opacity = 0.0; };
            header.Children.Add(tlPanel);
            root.Children.Add(header);

            // 2. 4-STEP VISUAL STEPPER BAR
            _stepperGrid = new Grid { Margin = new Thickness(0, 0, 0, 14) };
            for (int i = 0; i < 4; i++) _stepperGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            root.Children.Add(_stepperGrid);

            // 3. DYNAMIC STEP BODY
            _stepBody = new ContentControl { Margin = new Thickness(0, 0, 0, 14) };
            root.Children.Add(_stepBody);

            // 4. FOOTER NAVIGATION
            var footer = new Grid();
            _footerLeft = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Left };
            _footerRight = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
            footer.Children.Add(_footerLeft);
            footer.Children.Add(_footerRight);
            root.Children.Add(footer);

            outer.Child = root;
            Content = outer;
            RefreshBadge();
        }

        private void RefreshBadge()
        {
            VersionManifest inst, avail;
            bool hasNewer = InstallerProgram.HasNewerVersionAvailable(out inst, out avail);
            string verStr = avail != null && !string.IsNullOrEmpty(avail.Version) ? avail.Version : "5.2.2";
            if (!_isInstalled)
            {
                _versionBadge.Background = new SolidColorBrush(Violet);
                _versionBadgeTxt.Text = "Nueva instalación · v" + verStr;
            }
            else if (hasNewer)
            {
                _versionBadge.Background = new SolidColorBrush(Violet);
                _versionBadgeTxt.Text = "Actualización disponible · v" + verStr;
            }
            else
            {
                _versionBadge.Background = new SolidColorBrush(Color.FromRgb(26, 128, 72));
                _versionBadgeTxt.Text = "Instalado · v" + (inst != null ? inst.Version : verStr);
            }
        }

        private void RenderStepper()
        {
            _stepperGrid.Children.Clear();
            string[] labels = { "1. Diagnóstico PC", "2. Suite y Módulos", "3. Integración", "4. Instalación" };
            for (int i = 0; i < 4; i++)
            {
                int idx = i;
                bool active = idx == _step;
                bool done = idx < _step || (_step == 3 && _finishedOk);
                var bg = active
                    ? new SolidColorBrush(Color.FromArgb(55, Violet.R, Violet.G, Violet.B))
                    : (done ? new SolidColorBrush(Color.FromArgb(32, Emerald.R, Emerald.G, Emerald.B)) : new SolidColorBrush(CardSubtle));
                var border = active
                    ? new SolidColorBrush(Violet)
                    : (done ? new SolidColorBrush(Color.FromArgb(120, Emerald.R, Emerald.G, Emerald.B)) : new SolidColorBrush(CardBorder));
                var pill = new Border
                {
                    CornerRadius = new CornerRadius(10),
                    Background = bg,
                    BorderBrush = border,
                    BorderThickness = new Thickness(1),
                    Padding = new Thickness(10, 7, 10, 7),
                    Margin = new Thickness(i == 0 ? 0 : 6, 0, 0, 0),
                    Cursor = (!_busy && idx < 3) ? Cursors.Hand : Cursors.Arrow,
                    Child = new TextBlock
                    {
                        Text = (done ? "✓ " : "") + labels[i],
                        Foreground = active ? Brushes.White : (done ? new SolidColorBrush(Color.FromRgb(134, 239, 172)) : TextMuted),
                        FontSize = 11.2,
                        FontWeight = active ? FontWeights.SemiBold : FontWeights.Medium,
                        HorizontalAlignment = HorizontalAlignment.Center
                    }
                };
                pill.MouseLeftButtonDown += (s, e) =>
                {
                    if (!_busy && idx < 3) { _step = idx; RenderStep(); }
                };
                Grid.SetColumn(pill, i);
                _stepperGrid.Children.Add(pill);
            }
        }

        private void RenderStep()
        {
            RenderStepper();
            _footerLeft.Children.Clear();
            _footerRight.Children.Clear();

            if (_step == 0) _stepBody.Content = BuildStep1Hardware();
            else if (_step == 1) _stepBody.Content = BuildStep2Modules();
            else if (_step == 2) _stepBody.Content = BuildStep3Integration();
            else _stepBody.Content = BuildStep4Install();

            BuildFooter();
        }

        // ==================== STEP 1: DEEP HARDWARE DIAGNOSTICS & AUTO-ADAPTATION ====================
        private UIElement BuildStep1Hardware()
        {
            var stack = new StackPanel();
            VersionManifest inst, avail;
            InstallerProgram.HasNewerVersionAvailable(out inst, out avail);

            // Version & Path Card
            var verCard = MakeCard(10);
            var vcGrid = new Grid();
            vcGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            vcGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            vcGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1.2, GridUnitType.Star) });

            vcGrid.Children.Add(MakeStatCell("INSTALADA ACTUALMENTE", inst != null ? ("v" + inst.Version) : (_isInstalled ? "Instalada" : "Sin instalar"), Brushes.White));
            var c2 = MakeStatCell("PAQUETE DISPONIBLE", avail != null ? ("v" + avail.Version) : "v5.2.2", new SolidColorBrush(VioletHover));
            Grid.SetColumn(c2, 1); vcGrid.Children.Add(c2);
            var c3 = MakeStatCell("SISTEMA DETECTADO", _hw.WindowsLabel, TextPrimary);
            Grid.SetColumn(c3, 2); vcGrid.Children.Add(c3);
            verCard.Child = vcGrid;
            stack.Children.Add(verCard);

            // 6-Tile Live Hardware Diagnostic Grid
            var hwCard = MakeCard(10);
            var hwStack = new StackPanel();
            var hwHdr = new DockPanel { Margin = new Thickness(0, 0, 0, 8) };
            hwHdr.Children.Add(new TextBlock { Text = "ESCÁNER PROFUNDO DE TU PC", Foreground = new SolidColorBrush(VioletHover), FontSize = 10.2, FontWeight = FontWeights.Bold });
            var formTag = new Border
            {
                CornerRadius = new CornerRadius(6),
                Background = new SolidColorBrush(Color.FromArgb(40, Emerald.R, Emerald.G, Emerald.B)),
                Padding = new Thickness(7, 2, 7, 2),
                HorizontalAlignment = HorizontalAlignment.Right,
                Child = new TextBlock
                {
                    Text = _hw.IsLaptop ? ("Laptop · Batería " + _hw.BatteryPct + "%") : "PC de Escritorio",
                    Foreground = new SolidColorBrush(Color.FromRgb(134, 239, 172)),
                    FontSize = 9.8, FontWeight = FontWeights.SemiBold
                }
            };
            DockPanel.SetDock(formTag, Dock.Right);
            hwHdr.Children.Add(formTag);
            hwStack.Children.Add(hwHdr);

            var g = new Grid();
            for (int c = 0; c < 3; c++) g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            g.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            g.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            AddHwTile(g, 0, 0, "PROCESADOR (CPU)", _hw.CpuName, _hw.LogicalCores + " hilos lógicos" + (_hw.CpuMhz > 0 ? (" · " + (_hw.CpuMhz / 1000.0).ToString("0.00") + " GHz") : ""));
            AddHwTile(g, 0, 1, "MEMORIA RAM", _hw.TotalRamGb.ToString("0.#") + " GB Totales", _hw.AvailableRamGb.ToString("0.#") + " GB libres ahora");
            AddHwTile(g, 0, 2, "TARJETA GRÁFICA (GPU)", _hw.GpuName, _hw.HasDedicatedGpu ? ("Dedicada" + (_hw.GpuVramGb > 0 ? (" · " + _hw.GpuVramGb.ToString("0.#") + " GB VRAM") : "")) : "Gráficos Integrados");
            AddHwTile(g, 1, 0, "PANTALLA PRINCIPAL", _hw.ScreenW + "×" + _hw.ScreenH + " @ " + _hw.RefreshHz + " Hz", _hw.MonitorCount + (_hw.MonitorCount == 1 ? " monitor activo" : " monitores activos"));
            AddHwTile(g, 1, 1, "ALMACENAMIENTO", _hw.FreeDiskGb.ToString("0.#") + " GB libres", "AuraOS ocupa solo ~28 MB");
            AddHwTile(g, 1, 2, "CONFLICTOS EXTERNOS", _hw.ExternalWallpaperEngineActive ? "Wallpaper Engine activo" : "Sin conflictos", _hw.ExternalWallpaperEngineActive ? "Sugerencia: evita doble fondo" : "Listo para AuraWallpaper 60 FPS");

            hwStack.Children.Add(g);
            hwCard.Child = hwStack;
            stack.Children.Add(hwCard);

            // Smart Hardware Adaptation Recommendation Card
            var recCard = new Border
            {
                CornerRadius = new CornerRadius(16),
                Background = new SolidColorBrush(Color.FromRgb(25, 23, 42)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(140, Violet.R, Violet.G, Violet.B)),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(14, 12, 14, 12)
            };
            var recStack = new StackPanel();
            recStack.Children.Add(new TextBlock
            {
                Text = "PERFIL ADAPTADO RECOMENDADO PARA TU HARDWARE",
                Foreground = new SolidColorBrush(VioletHover),
                FontSize = 10.2,
                FontWeight = FontWeights.Bold
            });
            recStack.Children.Add(new TextBlock
            {
                Text = _hw.RecommendationSummary,
                Foreground = TextPrimary,
                FontSize = 11.4,
                TextWrapping = TextWrapping.Wrap,
                Margin = new Thickness(0, 4, 0, 10)
            });

            var tierGrid = new Grid();
            for (int i = 0; i < 3; i++) tierGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

            AddTierButton(tierGrid, 0, HardwareTier.UltraGamer, "Ultra / Gamer", "60 FPS HD aislado · 120Hz Genie · Game Boost");
            AddTierButton(tierGrid, 1, HardwareTier.BalancedLaptop, "Equilibrado / Laptop", "Auto-pausa fondos · Batería + Brillo · Ligero");
            AddTierButton(tierGrid, 2, HardwareTier.UltraLight, "Ultra-Ligero (PC Modesto)", "0% GPU estático · <35 MB RAM · Instantáneo");
            recStack.Children.Add(tierGrid);

            recCard.Child = recStack;
            stack.Children.Add(recCard);

            return stack;
        }

        private void AddHwTile(Grid g, int row, int col, string label, string val, string sub)
        {
            var tile = new Border
            {
                CornerRadius = new CornerRadius(11),
                Background = new SolidColorBrush(CardSubtle),
                BorderBrush = new SolidColorBrush(Color.FromRgb(38, 38, 54)),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(10, 8, 10, 8),
                Margin = new Thickness(col == 0 ? 0 : 6, row == 0 ? 0 : 6, 0, 0)
            };
            var sp = new StackPanel();
            sp.Children.Add(new TextBlock { Text = label, Foreground = TextMuted, FontSize = 9.0, FontWeight = FontWeights.Bold });
            sp.Children.Add(new TextBlock { Text = val, Foreground = Brushes.White, FontSize = 11.6, FontWeight = FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis, Margin = new Thickness(0, 2, 0, 1) });
            sp.Children.Add(new TextBlock { Text = sub, Foreground = TextMuted, FontSize = 9.8, TextTrimming = TextTrimming.CharacterEllipsis });
            tile.Child = sp;
            Grid.SetRow(tile, row);
            Grid.SetColumn(tile, col);
            g.Children.Add(tile);
        }

        private void AddTierButton(Grid g, int col, HardwareTier tier, string title, string desc)
        {
            bool selected = _opts.SelectedTier == tier;
            bool isRec = _hw.RecommendedTier == tier;
            var card = new Border
            {
                CornerRadius = new CornerRadius(11),
                Background = selected ? new SolidColorBrush(Color.FromArgb(65, Violet.R, Violet.G, Violet.B)) : new SolidColorBrush(CardSubtle),
                BorderBrush = selected ? new SolidColorBrush(Violet) : new SolidColorBrush(CardBorder),
                BorderThickness = new Thickness(selected ? 1.5 : 1),
                Padding = new Thickness(10, 8, 10, 8),
                Margin = new Thickness(col == 0 ? 0 : 6, 0, 0, 0),
                Cursor = Cursors.Hand
            };
            var sp = new StackPanel();
            var top = new StackPanel { Orientation = Orientation.Horizontal };
            top.Children.Add(new TextBlock { Text = title, Foreground = Brushes.White, FontSize = 11.4, FontWeight = FontWeights.SemiBold });
            if (isRec)
            {
                top.Children.Add(new Border
                {
                    CornerRadius = new CornerRadius(5),
                    Background = new SolidColorBrush(Emerald),
                    Padding = new Thickness(5, 1, 5, 1),
                    Margin = new Thickness(6, 0, 0, 0),
                    Child = new TextBlock { Text = "Ideal", Foreground = new SolidColorBrush(Color.FromRgb(10, 20, 12)), FontSize = 8.8, FontWeight = FontWeights.Bold }
                });
            }
            sp.Children.Add(top);
            sp.Children.Add(new TextBlock { Text = desc, Foreground = TextMuted, FontSize = 9.8, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 3, 0, 0) });
            card.Child = sp;
            card.MouseLeftButtonDown += (s, e) =>
            {
                InstallerProgram.ApplyTierToOptions(_opts, tier);
                _opts.ApplyHardwareProfileToSettings = true;
                RenderStep();
            };
            Grid.SetColumn(card, col);
            g.Children.Add(card);
        }

        // ==================== STEP 2: MODULAR SUITE COMPONENT PICKER ====================
        private UIElement BuildStep2Modules()
        {
            var stack = new StackPanel();
            var card = MakeCard(0);
            var sp = new StackPanel();

            var topBar = new DockPanel { Margin = new Thickness(0, 0, 0, 10) };
            topBar.Children.Add(new TextBlock { Text = "SELECCIÓN MODULAR DE COMPONENTES (SUITE 100% RUST)", Foreground = new SolidColorBrush(VioletHover), FontSize = 10.4, FontWeight = FontWeights.Bold, VerticalAlignment = VerticalAlignment.Center });

            var quickRow = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
            quickRow.Children.Add(MakeChipButton("Todo (11)", false, () =>
            {
                foreach (string m in InstallerProgram.AllSuiteStems) _opts.SelectedModules.Add(m);
                RenderStep();
            }));
            var minBtn = MakeChipButton("Solo Núcleo Ligero", false, () =>
            {
                _opts.SelectedModules.Clear();
                foreach (string c in InstallerProgram.CoreStems) _opts.SelectedModules.Add(c);
                RenderStep();
            });
            minBtn.Margin = new Thickness(6, 0, 0, 0);
            quickRow.Children.Add(minBtn);
            DockPanel.SetDock(quickRow, Dock.Right);
            topBar.Children.Add(quickRow);
            sp.Children.Add(topBar);

            var modules = new[]
            {
                Tuple.Create("AuraShell", "Escritorio, TopBar, Dock Inteligente y Centro de Control", true),
                Tuple.Create("AuraWallpaper", "Motor de Video Aislado 60 FPS HD (DXVA Hardware, 0 fugas)", true),
                Tuple.Create("AuraSettings", "Panel de Configuración nativa (Audio, Pantalla, Temas)", true),
                Tuple.Create("AuraFiles", "Gestor de Archivos GPU + Analizador de Disco + F2/Ctrl+C/V", false),
                Tuple.Create("AuraTasks", "Administrador de Tareas nativo con Ctrl+Shift+Esc", false),
                Tuple.Create("AuraPDF", "Visor, notas y firma de documentos PDF", false),
                Tuple.Create("AuraPhotos", "Visor y editor rápido de imágenes GPU", false),
                Tuple.Create("AuraPlayer", "Reproductor ligero de video y audio local", false),
                Tuple.Create("AuraNotes", "Bloc de notas y editor instantáneo de código", false),
                Tuple.Create("AuraShot", "Capturas de pantalla con anotaciones y OCR nativo WinRT", false),
                Tuple.Create("AuraLock", "Pantalla de bloqueo GPU con PIN y fondo animado", false)
            };

            var grid = new Grid();
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            int rows = (modules.Length + 1) / 2;
            for (int r = 0; r < rows; r++) grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            for (int i = 0; i < modules.Length; i++)
            {
                var m = modules[i];
                string stem = m.Item1;
                string desc = m.Item2;
                bool required = m.Item3;
                bool on = required || _opts.SelectedModules.Contains(stem);

                var rowCard = new Border
                {
                    CornerRadius = new CornerRadius(11),
                    Background = on ? new SolidColorBrush(Color.FromRgb(30, 30, 44)) : new SolidColorBrush(CardSubtle),
                    BorderBrush = on ? new SolidColorBrush(required ? Color.FromArgb(140, Emerald.R, Emerald.G, Emerald.B) : Color.FromArgb(130, Violet.R, Violet.G, Violet.B)) : new SolidColorBrush(Color.FromRgb(40, 40, 56)),
                    BorderThickness = new Thickness(1),
                    Padding = new Thickness(10, 7, 10, 7),
                    Margin = new Thickness(i % 2 == 1 ? 8 : 0, i >= 2 ? 7 : 0, 0, 0),
                    Cursor = required ? Cursors.Arrow : Cursors.Hand
                };
                var rg = new Grid();
                rg.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                rg.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var txtSp = new StackPanel();
                var nameRow = new StackPanel { Orientation = Orientation.Horizontal };
                nameRow.Children.Add(new TextBlock { Text = stem, Foreground = Brushes.White, FontSize = 11.6, FontWeight = FontWeights.SemiBold });
                if (required)
                {
                    nameRow.Children.Add(new Border
                    {
                        CornerRadius = new CornerRadius(5),
                        Background = new SolidColorBrush(Color.FromArgb(40, Emerald.R, Emerald.G, Emerald.B)),
                        Padding = new Thickness(5, 1, 5, 1),
                        Margin = new Thickness(6, 0, 0, 0),
                        Child = new TextBlock { Text = "Núcleo", Foreground = new SolidColorBrush(Color.FromRgb(134, 239, 172)), FontSize = 8.8, FontWeight = FontWeights.SemiBold }
                    });
                }
                txtSp.Children.Add(nameRow);
                txtSp.Children.Add(new TextBlock { Text = desc, Foreground = TextMuted, FontSize = 9.8, TextTrimming = TextTrimming.CharacterEllipsis, Margin = new Thickness(0, 2, 6, 0) });
                rg.Children.Add(txtSp);

                var check = new Border
                {
                    Width = 20, Height = 20, CornerRadius = new CornerRadius(6),
                    Background = on ? new SolidColorBrush(required ? Emerald : Violet) : new SolidColorBrush(Color.FromRgb(38, 38, 52)),
                    BorderBrush = new SolidColorBrush(Color.FromRgb(74, 74, 96)),
                    BorderThickness = new Thickness(on ? 0 : 1),
                    VerticalAlignment = VerticalAlignment.Center,
                    Child = new TextBlock { Text = on ? "✓" : "", Foreground = Brushes.White, FontSize = 11, FontWeight = FontWeights.Bold, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center }
                };
                Grid.SetColumn(check, 1);
                rg.Children.Add(check);

                rowCard.Child = rg;
                if (!required)
                {
                    rowCard.MouseLeftButtonDown += (s, e) =>
                    {
                        if (_opts.SelectedModules.Contains(stem)) _opts.SelectedModules.Remove(stem);
                        else _opts.SelectedModules.Add(stem);
                        RenderStep();
                    };
                }
                Grid.SetRow(rowCard, i / 2);
                Grid.SetColumn(rowCard, i % 2);
                grid.Children.Add(rowCard);
            }

            sp.Children.Add(grid);
            card.Child = sp;
            stack.Children.Add(card);
            return stack;
        }

        // ==================== STEP 3: WINDOWS INTEGRATION + SMART APBOPTIMIZER ====================
        private UIElement BuildStep3Integration()
        {
            var stack = new StackPanel();

            // Layout Preferences Strip (Dock + TopBar + Widgets)
            var styleCard = MakeCard(10);
            var sg = new Grid();
            for (int c = 0; c < 3; c++) sg.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

            var dockCol = new StackPanel();
            dockCol.Children.Add(new TextBlock { Text = "POSICIÓN DEL DOCK", Foreground = TextMuted, FontSize = 9.5, FontWeight = FontWeights.Bold, Margin = new Thickness(0, 0, 0, 6) });
            var dockBtns = new StackPanel { Orientation = Orientation.Horizontal };
            dockBtns.Children.Add(MakeChipButton("Izquierda", _opts.DockPosition != "Bottom", () => { _opts.DockPosition = "Left"; RenderStep(); }));
            var db2 = MakeChipButton("Abajo", _opts.DockPosition == "Bottom", () => { _opts.DockPosition = "Bottom"; RenderStep(); });
            db2.Margin = new Thickness(6, 0, 0, 0);
            dockBtns.Children.Add(db2);
            dockCol.Children.Add(dockBtns);
            sg.Children.Add(dockCol);

            var barCol = new StackPanel();
            barCol.Children.Add(new TextBlock { Text = "ESTILO TOPBAR", Foreground = TextMuted, FontSize = 9.5, FontWeight = FontWeights.Bold, Margin = new Thickness(0, 0, 0, 6) });
            var barBtns = new StackPanel { Orientation = Orientation.Horizontal };
            foreach (string bs in new[] { "Hug", "Float", "Rect" })
            {
                string val = bs;
                var b = MakeChipButton(val, string.Equals(_opts.BarStyle, val, StringComparison.OrdinalIgnoreCase), () => { _opts.BarStyle = val; RenderStep(); });
                if (barBtns.Children.Count > 0) b.Margin = new Thickness(5, 0, 0, 0);
                barBtns.Children.Add(b);
            }
            barCol.Children.Add(barBtns);
            Grid.SetColumn(barCol, 1);
            sg.Children.Add(barCol);

            var widCol = new StackPanel();
            widCol.Children.Add(new TextBlock { Text = "WIDGETS ESCRITORIO", Foreground = TextMuted, FontSize = 9.5, FontWeight = FontWeights.Bold, Margin = new Thickness(0, 0, 0, 6) });
            var widBtns = new StackPanel { Orientation = Orientation.Horizontal };
            string[] wLabels = { "Todos", "Reloj", "Off" };
            for (int wi = 0; wi < 3; wi++)
            {
                int wIdx = wi;
                var b = MakeChipButton(wLabels[wi], _opts.WidgetsPreset == wIdx, () => { _opts.WidgetsPreset = wIdx; _opts.ApplyHardwareProfileToSettings = true; RenderStep(); });
                if (wi > 0) b.Margin = new Thickness(5, 0, 0, 0);
                widBtns.Children.Add(b);
            }
            widCol.Children.Add(widBtns);
            Grid.SetColumn(widCol, 2);
            sg.Children.Add(widCol);

            styleCard.Child = sg;
            stack.Children.Add(styleCard);

            // Integration Toggles
            var optCard = MakeCard(10);
            var optSp = new StackPanel();
            optSp.Children.Add(CreateToggle("Iniciar AuraOS automáticamente al encender Windows (arranque instantáneo)", _opts.StartWithWindows, v => _opts.StartWithWindows = v));
            optSp.Children.Add(CreateToggle("Ocultar barra clásica de Windows (Ctrl+Alt+W la restaura al instante)", _opts.HideTaskbarOnStart, v => _opts.HideTaskbarOnStart = v));
            optSp.Children.Add(CreateToggle("Auto-pausar video wallpaper al maximizar o jugar (0.0% CPU en juegos)", _opts.WallpaperAutoPause, v => _opts.WallpaperAutoPause = v));
            optSp.Children.Add(CreateToggle("Crear accesos directos en Escritorio, Menú Inicio y Asociaciones de Archivos", _opts.RegisterFileAssociations, v => { _opts.RegisterFileAssociations = v; _opts.CreateDesktopShortcuts = v; _opts.CreateStartMenu = v; }));
            optSp.Children.Add(CreateToggle("Aplicar modo oscuro nativo de Windows y quitar panel Widgets", _opts.ApplyWindowsDarkPolish, v => _opts.ApplyWindowsDarkPolish = v));
            optSp.Children.Add(CreateToggle("Abrir AuraOS automáticamente al finalizar la instalación", _opts.LaunchAfter, v => _opts.LaunchAfter = v));
            optCard.Child = optSp;
            stack.Children.Add(optCard);

            // SMART APBOPTIMIZER CARD (Auto-skipped & remembers when already done!)
            bool skipRecommended = _isInstalled || _installState.ApbCompleted;
            var apbCard = new Border
            {
                CornerRadius = new CornerRadius(14),
                Background = new SolidColorBrush(skipRecommended ? Color.FromRgb(22, 30, 28) : Color.FromRgb(26, 24, 42)),
                BorderBrush = new SolidColorBrush(skipRecommended ? Color.FromArgb(115, Emerald.R, Emerald.G, Emerald.B) : Color.FromArgb(130, Violet.R, Violet.G, Violet.B)),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(14, 10, 14, 10)
            };
            var apbStack = new StackPanel();
            var apbHead = new Grid();
            apbHead.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            apbHead.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            var apbTitle = new StackPanel();
            var r1 = new StackPanel { Orientation = Orientation.Horizontal };
            r1.Children.Add(new TextBlock { Text = "OPTIMIZADOR WINDOWS · APBOptimizer", Foreground = skipRecommended ? new SolidColorBrush(Color.FromRgb(134, 239, 172)) : new SolidColorBrush(VioletHover), FontSize = 10.2, FontWeight = FontWeights.Bold });
            r1.Children.Add(new Border
            {
                CornerRadius = new CornerRadius(6),
                Background = new SolidColorBrush(Color.FromArgb(45, Emerald.R, Emerald.G, Emerald.B)),
                Padding = new Thickness(6, 1, 6, 1),
                Margin = new Thickness(8, 0, 0, 0),
                Child = new TextBlock
                {
                    Text = _installState.ApbCompleted
                        ? ("✓ Ya aplicado (" + (_installState.ApbCompletedAt ?? "previo") + ")")
                        : (_isInstalled ? "✓ Omitido en actualizaciones" : "Opcional · 1ra vez"),
                    Foreground = new SolidColorBrush(Color.FromRgb(134, 239, 172)),
                    FontSize = 9.2, FontWeight = FontWeights.SemiBold
                }
            });
            apbTitle.Children.Add(r1);
            apbTitle.Children.Add(new TextBlock
            {
                Text = skipRecommended
                    ? "No volverá a pedirse al actualizar AuraOS. Solo actívalo si deseas repetir la optimización de Windows."
                    : "Optimiza servicios, telemetría y rendimiento gaming de Windows antes de instalar AuraOS (se recuerda para no repetirlo).",
                Foreground = TextMuted,
                FontSize = 10.6,
                TextWrapping = TextWrapping.Wrap,
                Margin = new Thickness(0, 3, 10, 0)
            });
            apbHead.Children.Add(apbTitle);

            var runApbBtn = MakeChipButton("Abrir APB manual", false, () => RunApbStandaloneAsync());
            runApbBtn.VerticalAlignment = VerticalAlignment.Center;
            Grid.SetColumn(runApbBtn, 1);
            apbHead.Children.Add(runApbBtn);

            apbStack.Children.Add(apbHead);
            apbStack.Children.Add(CreateToggle("Ejecutar APBOptimizer antes de instalar AuraOS", _opts.RunApbOptimizerFirst, v => _opts.RunApbOptimizerFirst = v));
            apbCard.Child = apbStack;
            stack.Children.Add(apbCard);

            return stack;
        }

        // ==================== STEP 4: LIVE INSTALLATION PROGRESS & LOG ====================
        private UIElement BuildStep4Install()
        {
            var card = MakeCard(0);
            var sp = new StackPanel();

            var hdr = new DockPanel { Margin = new Thickness(0, 0, 0, 8) };
            _progressHeadlineTxt = new TextBlock
            {
                Text = _finishedOk ? "Instalación completada con éxito" : "Instalando AuraOS v5.2...",
                Foreground = Brushes.White,
                FontSize = 14,
                FontWeight = FontWeights.SemiBold
            };
            hdr.Children.Add(_progressHeadlineTxt);
            _progressPctTxt = new TextBlock
            {
                Text = _finishedOk ? "100%" : "0%",
                Foreground = new SolidColorBrush(VioletHover),
                FontSize = 13,
                FontWeight = FontWeights.Bold,
                HorizontalAlignment = HorizontalAlignment.Right
            };
            DockPanel.SetDock(_progressPctTxt, Dock.Right);
            hdr.Children.Add(_progressPctTxt);
            sp.Children.Add(hdr);

            var track = new Border
            {
                Height = 12,
                CornerRadius = new CornerRadius(6),
                Background = new SolidColorBrush(Color.FromRgb(36, 36, 50)),
                Margin = new Thickness(0, 0, 0, 12),
                ClipToBounds = true
            };
            _progressFill = new Border
            {
                Height = 12,
                CornerRadius = new CornerRadius(6),
                Background = new SolidColorBrush(_finishedOk ? Emerald : Violet),
                HorizontalAlignment = HorizontalAlignment.Left,
                Width = _finishedOk ? 640 : 18
            };
            track.Child = _progressFill;
            sp.Children.Add(track);

            _installLogBox = new TextBox
            {
                Height = 200,
                IsReadOnly = true,
                TextWrapping = TextWrapping.Wrap,
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
                Background = new SolidColorBrush(Color.FromRgb(14, 14, 20)),
                Foreground = new SolidColorBrush(Color.FromRgb(196, 204, 224)),
                BorderBrush = new SolidColorBrush(Color.FromRgb(42, 42, 58)),
                BorderThickness = new Thickness(1),
                FontFamily = new FontFamily("Consolas"),
                FontSize = 10.6,
                Padding = new Thickness(10, 8, 10, 8),
                Text = _finishedOk ? "[100%] AuraOS v5.2 listo.\r\n" : "[0%] Iniciando asistente de instalación profunda...\r\n"
            };
            sp.Children.Add(_installLogBox);
            card.Child = sp;
            return card;
        }

        private void BuildFooter()
        {
            // Left Footer Controls
            if (_step == 0 && _isInstalled)
            {
                var unBtn = MakeActionBtn("Desinstalar", Color.FromArgb(40, Coral.R, Coral.G, Coral.B), Color.FromArgb(100, Coral.R, Coral.G, Coral.B), Color.FromRgb(253, 164, 175), () => RunUninstallAsync());
                _footerLeft.Children.Add(unBtn);

                string srcDir = InstallerProgram.ResolveSourceDir();
                if (File.Exists(Path.Combine(srcDir, "build.ps1")) || File.Exists(Path.Combine(srcDir, @"..\scripts\build.ps1")))
                {
                    var bBtn = MakeActionBtn("Compilar + Actualizar", CardBg, CardBorder, Colors.White, () => StartInstallFlow(true, false));
                    bBtn.Margin = new Thickness(8, 0, 0, 0);
                    _footerLeft.Children.Add(bBtn);
                }
            }
            else if (_step > 0 && _step < 3 && !_busy)
            {
                _footerLeft.Children.Add(MakeActionBtn("← Atrás", CardBg, CardBorder, Colors.White, () => { _step--; RenderStep(); }));
            }

            // Right Footer Controls
            if (_step == 0)
            {
                string quickLbl = _isInstalled ? "Actualización rápida (sin APB)" : "Instalación rápida (sin APB)";
                var quickBtn = MakeActionBtn(quickLbl, Color.FromRgb(24, 52, 38), Emerald, Color.FromRgb(167, 243, 208), () => StartInstallFlow(false, true));
                quickBtn.Margin = new Thickness(0, 0, 8, 0);
                _footerRight.Children.Add(quickBtn);
                _footerRight.Children.Add(MakeActionBtn("Personalizar instalación →", Violet, VioletHover, Colors.White, () => { _step = 1; RenderStep(); }));
            }
            else if (_step == 1)
            {
                _footerRight.Children.Add(MakeActionBtn("Siguiente: Integración →", Violet, VioletHover, Colors.White, () => { _step = 2; RenderStep(); }));
            }
            else if (_step == 2)
            {
                string label = _isInstalled ? "Aplicar y Actualizar AuraOS" : "Instalar AuraOS v5.2";
                _footerRight.Children.Add(MakeActionBtn(label, Violet, VioletHover, Colors.White, () => StartInstallFlow(false, false)));
            }
            else
            {
                _finishOpenBtn = MakeActionBtn("Abrir AuraOS", Color.FromRgb(24, 62, 42), Emerald, Colors.White, () =>
                {
                    try
                    {
                        string exe = Path.Combine(InstallerProgram.InstallDir, "AuraShell.exe");
                        if (File.Exists(exe)) Process.Start(new ProcessStartInfo { FileName = exe, WorkingDirectory = InstallerProgram.InstallDir, UseShellExecute = true });
                    }
                    catch { }
                });
                _finishOpenBtn.Visibility = _finishedOk ? Visibility.Visible : Visibility.Collapsed;
                _finishOpenBtn.Margin = new Thickness(0, 0, 8, 0);
                _footerRight.Children.Add(_finishOpenBtn);

                var closeBtn = MakeActionBtn(_busy ? "Instalando..." : "Cerrar", CardBg, CardBorder, Colors.White, () => { if (!_busy) Close(); });
                _footerRight.Children.Add(closeBtn);
            }
        }

        private async void StartInstallFlow(bool rebuildFirst, bool quickUpdate)
        {
            if (_busy) return;
            _busy = true;
            _finishedOk = false;
            _step = 3;
            RenderStep();

            Action<int, string> pushProgress = (pct, msg) =>
            {
                Dispatcher.Invoke(() =>
                {
                    if (_progressPctTxt != null) _progressPctTxt.Text = pct + "%";
                    if (_progressHeadlineTxt != null) _progressHeadlineTxt.Text = msg;
                    if (_progressFill != null) _progressFill.Width = Math.Max(14, (pct / 100.0) * 644.0);
                    if (_installLogBox != null)
                    {
                        _installLogBox.AppendText("[" + pct + "%] " + msg + "\r\n");
                        _installLogBox.ScrollToEnd();
                    }
                });
            };

            string resultMsg = "";
            bool ok = await Task.Run(() =>
            {
                if (rebuildFirst)
                {
                    string buildErr;
                    if (!InstallerProgram.RunBuildScriptInSource(pushProgress, out buildErr))
                    {
                        resultMsg = "Error al compilar: " + buildErr;
                        return false;
                    }
                }
                if (quickUpdate)
                    return InstallerProgram.PerformQuickUpdate(true, pushProgress, out resultMsg);
                return InstallerProgram.PerformInstall(_opts, pushProgress, out resultMsg);
            });

            _busy = false;
            _finishedOk = ok;
            RefreshBadge();
            RenderStepper();
            _footerLeft.Children.Clear();
            _footerRight.Children.Clear();
            BuildFooter();

            if (_progressFill != null) _progressFill.Background = new SolidColorBrush(ok ? Emerald : Coral);
            if (_progressHeadlineTxt != null) _progressHeadlineTxt.Text = resultMsg;
            if (_installLogBox != null)
            {
                _installLogBox.AppendText((ok ? "[OK] " : "[ERROR] ") + resultMsg + "\r\n");
                _installLogBox.ScrollToEnd();
            }
        }

        private async void RunApbStandaloneAsync()
        {
            if (_busy) return;
            _busy = true;
            string err = "";
            bool ok = await Task.Run(() => InstallerProgram.LaunchApbOptimizer(false, null, out err));
            _busy = false;
            if (ok)
            {
                _installState.ApbCompleted = true;
                _installState.ApbCompletedAt = DateTime.Now.ToString("yyyy-MM-dd HH:mm");
                _opts.RunApbOptimizerFirst = false;
                RenderStep();
            }
        }

        private async void RunUninstallAsync()
        {
            if (_busy) return;
            _busy = true;
            _step = 3;
            RenderStep();
            if (_progressHeadlineTxt != null) _progressHeadlineTxt.Text = "Desinstalando AuraOS y restaurando barra clásica de Windows...";
            await Task.Run(() => InstallerProgram.PerformUninstall());
            _busy = false;
            _finishedOk = false;
            if (_progressPctTxt != null) _progressPctTxt.Text = "100%";
            if (_progressFill != null) { _progressFill.Width = 644; _progressFill.Background = new SolidColorBrush(Emerald); }
            if (_progressHeadlineTxt != null) _progressHeadlineTxt.Text = "AuraOS desinstalado y Windows restaurado correctamente.";
            if (_installLogBox != null) _installLogBox.AppendText("[OK] Desinstalación limpia completada.\r\n");
            RefreshBadge();
            _footerLeft.Children.Clear();
            _footerRight.Children.Clear();
            BuildFooter();
        }

        // ==================== UI HELPERS ====================
        private Border MakeCard(double marginBottom)
        {
            return new Border
            {
                CornerRadius = new CornerRadius(16),
                Background = new SolidColorBrush(CardBg),
                BorderBrush = new SolidColorBrush(CardBorder),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(14, 11, 14, 11),
                Margin = new Thickness(0, 0, 0, marginBottom)
            };
        }

        private StackPanel MakeStatCell(string label, string value, Brush valBrush)
        {
            var sp = new StackPanel();
            sp.Children.Add(new TextBlock { Text = label, Foreground = TextMuted, FontSize = 9.2, FontWeight = FontWeights.Bold });
            sp.Children.Add(new TextBlock { Text = value, Foreground = valBrush, FontSize = 12.2, FontWeight = FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis, Margin = new Thickness(0, 2, 6, 0) });
            return sp;
        }

        private Border MakeChipButton(string text, bool active, Action onClick)
        {
            var b = new Border
            {
                CornerRadius = new CornerRadius(8),
                Background = active ? new SolidColorBrush(Violet) : new SolidColorBrush(CardSubtle),
                BorderBrush = active ? new SolidColorBrush(VioletHover) : new SolidColorBrush(CardBorder),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(10, 5, 10, 5),
                Cursor = Cursors.Hand,
                Child = new TextBlock { Text = text, Foreground = active ? Brushes.White : TextMuted, FontSize = 10.6, FontWeight = active ? FontWeights.SemiBold : FontWeights.Medium }
            };
            b.MouseLeftButtonDown += (s, e) => onClick();
            return b;
        }

        private Border MakeActionBtn(string text, Color bg, Color border, Color fg, Action onClick)
        {
            var b = new Border
            {
                CornerRadius = new CornerRadius(12),
                Background = new SolidColorBrush(bg),
                BorderBrush = new SolidColorBrush(border),
                BorderThickness = new Thickness(1),
                Padding = new Thickness(15, 9, 15, 9),
                Cursor = Cursors.Hand,
                Child = new TextBlock { Text = text, Foreground = new SolidColorBrush(fg), FontSize = 12.0, FontWeight = FontWeights.SemiBold }
            };
            b.MouseLeftButtonDown += (s, e) => onClick();
            return b;
        }

        private Grid CreateToggle(string label, bool initial, Action<bool> onChanged)
        {
            bool state = initial;
            var row = new Grid { Margin = new Thickness(0, 6, 0, 0), Cursor = Cursors.Hand, Background = Brushes.Transparent };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.Children.Add(new TextBlock { Text = label, Foreground = TextPrimary, FontSize = 11.4, VerticalAlignment = VerticalAlignment.Center });

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
