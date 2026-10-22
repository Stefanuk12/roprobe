Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Runtime.InteropServices;
public class WinCap {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern int GetWindowTextLength(IntPtr h);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    public static IntPtr Find(uint[] pids) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint p;
            GetWindowThreadProcessId(h, out p);
            if (Array.IndexOf(pids, p) < 0 || !IsWindowVisible(h) || GetWindowTextLength(h) == 0) return true;
            found = h;
            return false;
        }, IntPtr.Zero);
        return found;
    }
}
"@

$pids = if ($env:ROPROBE_PID) {
    @([uint32]$env:ROPROBE_PID)
} else {
    @(Get-Process -Name RobloxPlayerBeta -ErrorAction SilentlyContinue | ForEach-Object { [uint32]$_.Id })
}
if ($pids.Count -eq 0) { [Console]::Error.WriteLine('no RobloxPlayerBeta process'); exit 1 }

$h = [WinCap]::Find([uint32[]]$pids)
if ($h -eq [IntPtr]::Zero) { [Console]::Error.WriteLine('no visible roblox window'); exit 1 }
if ([WinCap]::IsIconic($h)) { [WinCap]::ShowWindow($h, 9) | Out-Null; Start-Sleep -Milliseconds 200 }

$r = New-Object WinCap+RECT
[WinCap]::GetClientRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L
$hgt = $r.B - $r.T
if ($w -le 0 -or $hgt -le 0) { [Console]::Error.WriteLine('window has zero size'); exit 1 }

$bmp = New-Object System.Drawing.Bitmap($w, $hgt)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
[WinCap]::PrintWindow($h, $dc, 2) | Out-Null # 2 = render full content, dont ask
$g.ReleaseHdc($dc)
$g.Dispose()

$scale = [Math]::Min(1.0, 1280.0 / $w)
$nw = [int]($w * $scale)
$nh = [int]($hgt * $scale)
$out = New-Object System.Drawing.Bitmap($nw, $nh)
$og = [System.Drawing.Graphics]::FromImage($out)
$og.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$og.DrawImage($bmp, 0, 0, $nw, $nh)
$og.Dispose()
$bmp.Dispose()

$jpg = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' } | Select-Object -First 1
$enc = New-Object System.Drawing.Imaging.EncoderParameters(1)
$enc.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter([System.Drawing.Imaging.Encoder]::Quality, [long]70)
$ms = New-Object System.IO.MemoryStream
$out.Save($ms, $jpg, $enc)
$out.Dispose()
[Convert]::ToBase64String($ms.ToArray())
