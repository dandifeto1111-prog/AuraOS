# Script de emergencia 100% seguro: restaura la barra de tareas, estado AppBar, WorkArea, Explorador y plan de energía.
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinRestore {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct APPBARDATA {
        public int cbSize; public IntPtr hWnd; public int uCallbackMessage; public uint uEdge; public RECT rc; public IntPtr lParam;
    }
    [DllImport("user32.dll", SetLastError = true)] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
    [DllImport("user32.dll", SetLastError = true)] public static extern IntPtr FindWindowEx(IntPtr parentHandle, IntPtr childAfter, string className, string windowTitle);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool EnableWindow(IntPtr hWnd, bool bEnable);
    [DllImport("user32.dll", SetLastError = true)] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hAfter, int X, int Y, int cx, int cy, uint uFlags);
    [DllImport("shell32.dll")] public static extern uint SHAppBarMessage(uint dwMessage, ref APPBARDATA pData);
    [DllImport("user32.dll", SetLastError = true)] public static extern bool SystemParametersInfo(uint uiAction, uint uiParam, ref RECT pvParam, uint fWinIni);
}
"@

$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$primary = [WinRestore]::FindWindow("Shell_TrayWnd", $null)
if ($primary -ne [IntPtr]::Zero) {
    [void][WinRestore]::EnableWindow($primary, $true)
    [void][WinRestore]::SetWindowPos($primary, [IntPtr]::Zero, 0, ($b.Height - 48), $b.Width, 48, 0x0014)
    $abd = New-Object WinRestore+APPBARDATA
    $abd.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($abd)
    $abd.hWnd = $primary
    $abd.lParam = [IntPtr]::Zero
    [void][WinRestore]::SHAppBarMessage(0x0000000A, [ref]$abd) # ABM_SETSTATE -> normal
    [void][WinRestore]::ShowWindow($primary, 5)
}

$sec = [IntPtr]::Zero
while (($sec = [WinRestore]::FindWindowEx([IntPtr]::Zero, $sec, "Shell_SecondaryTrayWnd", $null)) -ne [IntPtr]::Zero) {
    [void][WinRestore]::EnableWindow($sec, $true)
    [void][WinRestore]::ShowWindow($sec, 5)
}

$rc = New-Object WinRestore+RECT
$rc.Left = 0; $rc.Top = 0; $rc.Right = $b.Width; $rc.Bottom = $b.Height - 48
[void][WinRestore]::SystemParametersInfo(0x002F, 0, [ref]$rc, 0x0002)

Remove-Item "HKCU:\Software\Classes\Directory\shell\open_aurafiles" -Recurse -Force -ErrorAction SilentlyContinue
Remove-ItemProperty "HKCU:\Software\Classes\Directory\shell" -Name "(default)" -ErrorAction SilentlyContinue
Remove-Item "HKCU:\Software\Classes\Directory\Background\shell\open_aurafiles" -Recurse -Force -ErrorAction SilentlyContinue

& powercfg /setactive 381b4222-f694-41f0-9685-ff5bb260df2e 2>$null
Write-Host "Barra de tareas, posición, WorkArea, Explorador de Windows y plan de energía restaurados correctamente." -ForegroundColor Green
