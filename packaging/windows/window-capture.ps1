# Best-effort capture of the REAL top-level window of the running app, native caption and frame included.
# Dot-sourced by smoke.ps1 AFTER lab-helpers.ps1 (it uses Test-ScreenshotProblems). Nothing is compiled or
# executed at dot-source time, and Save-DesktopWindowScreenshot never throws: a machine without an interactive
# desktop only produces a warning, never a failed smoke run.

function Initialize-WindowCapture {
    # Compiles the P/Invoke helper once per process. Throws when it cannot be compiled (callers catch).
    if ('ImproWindows' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class ImproWindows {
    [StructLayout(LayoutKind.Sequential)]
    private struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

    private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")]
    private static extern IntPtr GetWindow(IntPtr hWnd, uint cmd);
    [DllImport("user32.dll")]
    private static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll")]
    private static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr hWnd, int cmd);
    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")]
    private static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")]
    private static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int index);
    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr hWnd, int attribute, out RECT rect, int size);

    // Physical pixels from here on, so window rectangles and screen coordinates agree on scaled displays.
    public static void MakeDpiAware() {
        try { SetProcessDPIAware(); } catch (Exception) { }
    }

    // The largest visible window without an owner that belongs to one of the processes; IntPtr.Zero when none.
    public static IntPtr FindMainWindow(int[] pids, int minWidth, int minHeight) {
        IntPtr best = IntPtr.Zero;
        long bestArea = 0;
        EnumWindows(delegate (IntPtr hWnd, IntPtr lParam) {
            uint pid;
            GetWindowThreadProcessId(hWnd, out pid);
            bool mine = false;
            foreach (int p in pids) {
                if ((uint)p == pid) { mine = true; break; }
            }
            if (!mine || !IsWindowVisible(hWnd) || GetWindow(hWnd, 4) != IntPtr.Zero) return true;
            RECT r;
            if (!GetWindowRect(hWnd, out r)) return true;
            long w = r.Right - r.Left;
            long h = r.Bottom - r.Top;
            if (w < minWidth || h < minHeight) return true;
            if (w * h > bestArea) { bestArea = w * h; best = hWnd; }
            return true;
        }, IntPtr.Zero);
        return best;
    }

    public static string Title(IntPtr hWnd) {
        StringBuilder sb = new StringBuilder(256);
        GetWindowText(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    // x, y, width, height of the window as drawn: the DWM frame bounds leave out the invisible resize border
    // that GetWindowRect includes. Null when the window is gone.
    public static int[] FrameBounds(IntPtr hWnd) {
        RECT r = new RECT();
        bool ok = false;
        try { ok = DwmGetWindowAttribute(hWnd, 9, out r, Marshal.SizeOf(typeof(RECT))) == 0; } catch (Exception) { ok = false; }
        if (!ok && !GetWindowRect(hWnd, out r)) return null;
        return new int[] { r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top };
    }

    // x, y, width, height from GetWindowRect (what PrintWindow renders). Null when the window is gone.
    public static int[] OuterBounds(IntPtr hWnd) {
        RECT r;
        if (!GetWindowRect(hWnd, out r)) return null;
        return new int[] { r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top };
    }

    public static int[] ScreenSize() {
        return new int[] { GetSystemMetrics(0), GetSystemMetrics(1) };
    }

    public static void BringToFront(IntPtr hWnd) {
        if (IsIconic(hWnd)) ShowWindow(hWnd, 9);
        SetForegroundWindow(hWnd);
    }

    // PW_RENDERFULLCONTENT (2): ask the window to paint itself, frame included, into the device context.
    public static bool PrintTo(IntPtr hWnd, IntPtr hdc) {
        return PrintWindow(hWnd, hdc, 2);
    }
}
'@
}

function Save-ScreenRectangle {
    # Copies a rectangle of the visible screen into a PNG. Everything on screen is included, so the window must be on top.
    param([int]$X, [int]$Y, [int]$Width, [int]$Height, [Parameter(Mandatory)][string]$Path)
    Add-Type -AssemblyName System.Drawing
    $bmp = [System.Drawing.Bitmap]::new($Width, $Height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        try { $g.CopyFromScreen($X, $Y, 0, 0, $bmp.Size) } finally { $g.Dispose() }
        $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $bmp.Dispose() }
}

function Save-WindowWithPrintWindow {
    # Asks the window to paint itself into a PNG (works for a covered window, may be black for GPU-drawn content).
    param([Parameter(Mandatory)][IntPtr]$Hwnd, [Parameter(Mandatory)][string]$Path)
    Add-Type -AssemblyName System.Drawing
    $b = [ImproWindows]::OuterBounds($Hwnd)
    if ($null -eq $b -or $b[2] -le 0 -or $b[3] -le 0) { throw 'the window has no size' }
    $bmp = [System.Drawing.Bitmap]::new($b[2], $b[3], [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $printed = $false
        try {
            $hdc = $g.GetHdc()
            try { $printed = [ImproWindows]::PrintTo($Hwnd, $hdc) } finally { $g.ReleaseHdc($hdc) }
        } finally { $g.Dispose() }
        if (-not $printed) { throw 'PrintWindow returned false' }
        $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $bmp.Dispose() }
}

function Save-DesktopWindowScreenshot {
    # Starts the app on a folder WITHOUT the QA variables (so it stays open), waits until its main window exists,
    # captures that window with its native frame into $Path and stops the app. $true when a non-blank PNG was
    # written. Never throws; problems are logged as warnings.
    param(
        [Parameter(Mandatory)][string]$Exe,
        [Parameter(Mandatory)][string]$Folder,
        [Parameter(Mandatory)][string]$Path,
        [hashtable]$EnvVars = @{},
        [int]$WindowTimeoutSec = 60
    )
    $started = $null
    $ok = $false
    try {
        Initialize-WindowCapture
        Add-Type -AssemblyName System.Drawing
        [ImproWindows]::MakeDpiAware()
        Remove-Item -LiteralPath $Path -ErrorAction SilentlyContinue
        $psi = [System.Diagnostics.ProcessStartInfo]::new($Exe)
        $psi.ArgumentList.Add($Folder)
        $psi.UseShellExecute = $false
        foreach ($k in $EnvVars.Keys) { $psi.Environment[$k] = [string]$EnvVars[$k] }
        $started = [System.Diagnostics.Process]::Start($psi)
        $screen = [ImproWindows]::ScreenSize()
        Write-Host ('[smoke] desktop capture: started pid {0}, screen {1}x{2}' -f $started.Id, $screen[0], $screen[1])

        # The window may belong to another impro process than the one started here (an app that hands over to
        # a running instance), so look at every impro process.
        $hwnd = [IntPtr]::Zero
        $deadline = (Get-Date).AddSeconds($WindowTimeoutSec)
        while ((Get-Date) -lt $deadline) {
            $pids = [int[]]@(Get-Process -Name 'impro' -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
            if ($pids.Count -eq 0) { Write-Host '::warning::desktop capture: the app exited before it showed a window'; break }
            $hwnd = [ImproWindows]::FindMainWindow($pids, 300, 200)
            if ($hwnd.ToInt64() -ne 0) { break }
            Start-Sleep -Milliseconds 500
        }
        if ($hwnd.ToInt64() -eq 0) {
            Write-Host "::warning::desktop capture: no impro window appeared within ${WindowTimeoutSec}s"
        } else {
            Write-Host ("[smoke] desktop capture: window found, title '{0}'" -f [ImproWindows]::Title($hwnd))
            Start-Sleep -Seconds 3   # let the first paint finish behind the frame
            for ($attempt = 1; $attempt -le 6; $attempt++) {
                [ImproWindows]::BringToFront($hwnd)
                Start-Sleep -Milliseconds 700
                $b = [ImproWindows]::FrameBounds($hwnd)
                if ($null -eq $b -or $b[2] -le 0 -or $b[3] -le 0) { Start-Sleep -Seconds 1; continue }
                Write-Host ('[smoke] desktop capture attempt {0}: window at {1},{2} size {3}x{4}' -f $attempt, $b[0], $b[1], $b[2], $b[3])
                Save-ScreenRectangle -X $b[0] -Y $b[1] -Width $b[2] -Height $b[3] -Path $Path
                $problems = @(Test-ScreenshotProblems -Path $Path)
                if ($problems.Count -eq 0) { $ok = $true; break }
                Write-Host ('[smoke] desktop capture attempt {0}: {1}' -f $attempt, ($problems -join '; '))
                Start-Sleep -Seconds 2
            }
            if (-not $ok) {
                # The screen copy stayed blank (no interactive desktop?): ask the window to paint itself instead.
                $alt = Join-Path ([IO.Path]::GetDirectoryName($Path)) ([IO.Path]::GetFileNameWithoutExtension($Path) + '-printwindow.png')
                try {
                    Save-WindowWithPrintWindow -Hwnd $hwnd -Path $alt
                    Write-Host "[smoke] desktop capture: PrintWindow fallback written to $alt"
                } catch {
                    Write-Host "::warning::desktop capture: PrintWindow fallback failed: $($_.Exception.Message)"
                }
            }
        }
    } catch {
        Write-Host "::warning::desktop window capture failed: $($_.Exception.Message)"
    } finally {
        if ($null -ne $started) {
            try { $started.Kill($true) } catch { Write-Host "[smoke] desktop capture: kill: $($_.Exception.Message)" }
            try { [void]$started.WaitForExit(10000) } catch { Write-Host "[smoke] desktop capture: wait: $($_.Exception.Message)" }
        }
        foreach ($leftover in @(Get-Process -Name 'impro' -ErrorAction SilentlyContinue)) {
            try { $leftover.Kill($true) } catch { Write-Host "[smoke] desktop capture: kill: $($_.Exception.Message)" }
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $ok) { Write-Host '::warning::desktop capture: no non-blank window screenshot (see the messages above)' }
    $ok
}
