# Window probe: small structured facts about the REAL top-level window of the running app, printed to the job log
# one per line as
#
#   [window-probe] key=value
#
# so that they can be read without downloading or decoding a screenshot. Dot-sourced by smoke.ps1 (after
# lab-helpers.ps1 and window-capture.ps1); Save-DesktopWindowScreenshot calls Write-WindowProbe while the window is
# still open. Best effort: every fact is read inside its own try/catch and a failure is printed as
# `key=error(<message>)`; nothing here compiles or runs at dot-source time and Write-WindowProbe never throws.
#
# The functions up to "Window facts" are pure (strings, colours, style bits, DPI arithmetic) and are tested by
# test_window_probe.ps1 and test_window_probe.py against the shared vectors in probe-vectors.json.
#
# Strict-mode notes: functions that return an array or an object per element are called as @(...); arrays leave
# a function with a leading comma where a one-element result must stay an array; variables used by the
# script blocks of Invoke-ProbeFact / Get-ProbeSilently are never named Key, Get, value, text or result.

# --- pure helpers -------------------------------------------------------------------------------------------

function Get-OneLine {
    # The text on one line: every run of line breaks (with the blanks around it) becomes a single space.
    param([AllowNull()][AllowEmptyString()][string]$Text)
    if ($null -eq $Text) { return '' }
    ($Text -replace '\s*[\r\n]+\s*', ' ').Trim()
}

function Format-ProbeLine {
    # The exact line of the job log for one fact.
    param([Parameter(Mandatory)][string]$Key, [AllowNull()][AllowEmptyString()][string]$Value = '')
    '[window-probe] {0}={1}' -f $Key, (Get-OneLine $Value)
}

function Write-ProbeLine {
    param([Parameter(Mandatory)][string]$Key, [AllowNull()][AllowEmptyString()][string]$Value = '')
    Write-LogLine (Format-ProbeLine -Key $Key -Value $Value)
}

function ConvertTo-BoolText {
    param([Parameter(Mandatory)][bool]$Value)
    if ($Value) { 'true' } else { 'false' }
}

function ConvertTo-Hex32 {
    # 0x00000000 form of the low 32 bits (a negative HRESULT or a signed style prints as its unsigned bits).
    param([Parameter(Mandatory)][long]$Value)
    '0x{0:X8}' -f ($Value -band 0xFFFFFFFFL)
}

function ConvertFrom-ColorRef {
    # A COLORREF is 0x00BBGGRR. Values with a non-zero high byte are not colours (DWMWA_COLOR_DEFAULT is
    # 0xFFFFFFFF, DWMWA_COLOR_NONE 0xFFFFFFFE) and are returned as raw 0xXXXXXXXX text.
    param([Parameter(Mandatory)][long]$Value)
    $v = $Value -band 0xFFFFFFFFL
    if ($v -gt 0xFFFFFFL) { return ('0x{0:X8}' -f $v) }
    '#{0:X2}{1:X2}{2:X2}' -f ($v -band 0xFF), (($v -shr 8) -band 0xFF), (($v -shr 16) -band 0xFF)
}

function Format-PixelColor {
    param([Parameter(Mandatory)][int]$R, [Parameter(Mandatory)][int]$G, [Parameter(Mandatory)][int]$B)
    '#{0:X2}{1:X2}{2:X2}' -f $R, $G, $B
}

function Get-WindowStyleFlags {
    # Ordered name -> bool for the style bits the probe reports. A multi-bit style (WS_CAPTION is WS_BORDER plus
    # WS_DLGFRAME) counts only when all of its bits are set. The style may arrive signed (GetWindowLong).
    param([Parameter(Mandatory)][long]$Style)
    $bits = [ordered]@{
        WS_CAPTION     = 0x00C00000
        WS_THICKFRAME  = 0x00040000
        WS_SYSMENU     = 0x00080000
        WS_MINIMIZEBOX = 0x00020000
        WS_MAXIMIZEBOX = 0x00010000
        WS_VISIBLE     = 0x10000000
    }
    $s = $Style -band 0xFFFFFFFFL
    $flags = [ordered]@{}
    foreach ($name in $bits.Keys) {
        $mask = [long]$bits[$name]
        $flags[$name] = (($s -band $mask) -eq $mask)
    }
    $flags
}

function Get-ScaledOffset {
    # A layout offset in device pixels: Offset * Dpi / 96, rounded half away from zero. Unknown DPI counts as 96.
    param([Parameter(Mandatory)][double]$Offset, [int]$Dpi = 96)
    if ($Dpi -le 0) { $Dpi = 96 }
    [int][Math]::Round($Offset * $Dpi / 96.0, [MidpointRounding]::AwayFromZero)
}

function Get-CaptionHeight {
    # Height in device pixels of the title bar. A window with WS_CAPTION: the distance from the top of the visible
    # frame to the client origin (NonClientTop) or, failing that, the difference of the window and client heights;
    # anything under 10 px is a border, not a caption. Otherwise (client-side decorations, or unknown) 16 px at 96 DPI.
    param([int]$NonClientTop = 0, [int]$WindowHeight = 0, [int]$ClientHeight = 0, [int]$Dpi = 96, [bool]$HasCaption = $false)
    if ($HasCaption -and $NonClientTop -ge 10) {
        return [pscustomobject]@{ Height = $NonClientTop; Source = 'client_origin' }
    }
    $diff = $WindowHeight - $ClientHeight
    if ($HasCaption -and $ClientHeight -gt 0 -and $diff -ge 10) {
        return [pscustomobject]@{ Height = $diff; Source = 'rect_difference' }
    }
    [pscustomobject]@{ Height = (Get-ScaledOffset 16 $Dpi); Source = 'default' }
}

function Get-ProbePoints {
    # The pixels sampled from the capture of the window (frame included), in bitmap coordinates. Offsets are in
    # device pixels at 96 DPI and scaled by Dpi/96; the caption height is already in device pixels.
    # One object per point: Name, X, Y, InRange (inside the bitmap). Callers wrap the call in @(...).
    param(
        [Parameter(Mandatory)][int]$Width,
        [Parameter(Mandatory)][int]$Height,
        [Parameter(Mandatory)][int]$CaptionHeight,
        [int]$Dpi = 96
    )
    $half = [int][Math]::Floor([double]$Width / 2.0)
    $capHalf = [int][Math]::Floor([double]$CaptionHeight / 2.0)
    $below = $CaptionHeight + (Get-ScaledOffset 6 $Dpi)
    $editorY = $CaptionHeight + [int][Math]::Floor([double]($Height - $CaptionHeight) / 2.0)
    $inset = Get-ScaledOffset 12 $Dpi
    $points = @(
        @('caption_center', $half, $capHalf),
        @('below_caption_x60', (Get-ScaledOffset 60 $Dpi), $below),
        @('below_caption_center', $half, $below),
        @('sidebar_body', (Get-ScaledOffset 150 $Dpi), ($CaptionHeight + (Get-ScaledOffset 120 $Dpi))),
        @('editor_center', $half, $editorY),
        @('bottom_right', ($Width - $inset), ($Height - $inset))
    )
    foreach ($p in $points) {
        $x = [int]$p[1]
        $y = [int]$p[2]
        [pscustomobject]@{ Name = [string]$p[0]; X = $x; Y = $y; InRange = (($x -ge 0) -and ($x -lt $Width) -and ($y -ge 0) -and ($y -lt $Height)) }
    }
}

function Get-DpiAwarenessName {
    # DPI_AWARENESS and PROCESS_DPI_AWARENESS: 0 unaware, 1 system, 2 per monitor; -1 is DPI_AWARENESS_INVALID.
    param([Parameter(Mandatory)][long]$Value)
    if ($Value -eq 0) { return 'unaware' }
    if ($Value -eq 1) { return 'system_aware' }
    if ($Value -eq 2) { return 'per_monitor_aware' }
    if ($Value -eq -1) { return 'invalid' }
    'unknown'
}

function Get-CornerPreferenceName {
    # DWM_WINDOW_CORNER_PREFERENCE.
    param([Parameter(Mandatory)][long]$Value)
    if ($Value -eq 0) { return 'default' }
    if ($Value -eq 1) { return 'do_not_round' }
    if ($Value -eq 2) { return 'round' }
    if ($Value -eq 3) { return 'round_small' }
    'unknown'
}

function Format-DwmValue {
    # Kind: bool (0 or 1), corner (preference enum, shown as 2(round)) or color (#RRGGBB, or the raw hex of the
    # DWMWA_COLOR_DEFAULT / DWMWA_COLOR_NONE markers).
    param([Parameter(Mandatory)][string]$Kind, [Parameter(Mandatory)][long]$Value)
    if ($Kind -eq 'color') { return (ConvertFrom-ColorRef -Value $Value) }
    if ($Kind -eq 'corner') { return ('{0}({1})' -f $Value, (Get-CornerPreferenceName -Value $Value)) }
    [string]$Value
}

# --- window facts (Windows only) ------------------------------------------------------------------------------

function Initialize-WindowProbe {
    # Compiles the two P/Invoke helpers once per process: ImproProbe (plain Win32) and ImproProbeShell (the COM
    # property store). Returns which of them is usable; never throws.
    $win32 = $false
    $shell = $false
    try {
        if (-not ('ImproProbe' -as [type])) {
            Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class ImproProbe {
    [StructLayout(LayoutKind.Sequential)]
    private struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X; public int Y; }

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowTextW(IntPtr hWnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassNameW(IntPtr hWnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll")]
    private static extern int GetWindowLongW(IntPtr hWnd, int index);
    [DllImport("user32.dll")]
    private static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")]
    private static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")]
    private static extern bool ClientToScreen(IntPtr hWnd, ref POINT point);
    [DllImport("user32.dll")]
    private static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll")]
    private static extern IntPtr GetWindowDpiAwarenessContext(IntPtr hWnd);
    [DllImport("user32.dll")]
    private static extern int GetAwarenessFromDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")]
    private static extern bool AreDpiAwarenessContextsEqual(IntPtr a, IntPtr b);
    [DllImport("user32.dll")]
    private static extern IntPtr GetDC(IntPtr hWnd);
    [DllImport("user32.dll")]
    private static extern int ReleaseDC(IntPtr hWnd, IntPtr hdc);
    [DllImport("gdi32.dll")]
    private static extern int GetDeviceCaps(IntPtr hdc, int index);
    [DllImport("user32.dll", EntryPoint = "SendMessageTimeoutW")]
    private static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", EntryPoint = "GetClassLongPtrW")]
    private static extern IntPtr GetClassLongPtr(IntPtr hWnd, int index);
    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr hWnd, int attribute, out int value, int size);
    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr hWnd, int attribute, out RECT value, int size);
    [DllImport("shcore.dll")]
    private static extern int GetProcessDpiAwareness(IntPtr process, out int awareness);
    [DllImport("kernel32.dll")]
    private static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
    [DllImport("kernel32.dll")]
    private static extern bool CloseHandle(IntPtr handle);

    public static int ProcessId(IntPtr hWnd) {
        uint pid;
        GetWindowThreadProcessId(hWnd, out pid);
        return (int)pid;
    }

    public static string Title(IntPtr hWnd) {
        StringBuilder sb = new StringBuilder(512);
        GetWindowTextW(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    public static string ClassName(IntPtr hWnd) {
        StringBuilder sb = new StringBuilder(256);
        GetClassNameW(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    // GWL_STYLE (-16) and GWL_EXSTYLE (-20) as unsigned 32-bit values.
    public static long Style(IntPtr hWnd) { return (long)(uint)GetWindowLongW(hWnd, -16); }
    public static long ExStyle(IntPtr hWnd) { return (long)(uint)GetWindowLongW(hWnd, -20); }

    // left, top, right, bottom in screen pixels, or null.
    public static int[] WindowRect(IntPtr hWnd) {
        RECT r;
        if (!GetWindowRect(hWnd, out r)) return null;
        return new int[] { r.Left, r.Top, r.Right, r.Bottom };
    }

    // left, top, right, bottom in client coordinates (left and top are 0), or null.
    public static int[] ClientRect(IntPtr hWnd) {
        RECT r;
        if (!GetClientRect(hWnd, out r)) return null;
        return new int[] { r.Left, r.Top, r.Right, r.Bottom };
    }

    // x, y of the client origin in screen pixels, or null.
    public static int[] ClientOrigin(IntPtr hWnd) {
        POINT p = new POINT();
        if (!ClientToScreen(hWnd, ref p)) return null;
        return new int[] { p.X, p.Y };
    }

    // hr, left, top, right, bottom of DWMWA_EXTENDED_FRAME_BOUNDS (9).
    public static int[] DwmFrameBounds(IntPtr hWnd) {
        RECT r = new RECT();
        int hr = DwmGetWindowAttribute(hWnd, 9, out r, Marshal.SizeOf(typeof(RECT)));
        return new int[] { hr, r.Left, r.Top, r.Right, r.Bottom };
    }

    // hr, value (as unsigned 32-bit) of a 4-byte DWM window attribute.
    public static long[] DwmAttribute(IntPtr hWnd, int attribute) {
        int value = 0;
        int hr = DwmGetWindowAttribute(hWnd, attribute, out value, 4);
        return new long[] { hr, (long)(uint)value };
    }

    // GetDpiForWindow (Windows 10 1607 and later; throws EntryPointNotFoundException before that).
    public static long Dpi(IntPtr hWnd) { return (long)GetDpiForWindow(hWnd); }

    // LOGPIXELSX (88) of the window's device context.
    public static long DpiFromDeviceCaps(IntPtr hWnd) {
        IntPtr dc = GetDC(hWnd);
        if (dc == IntPtr.Zero) return 0;
        try { return (long)GetDeviceCaps(dc, 88); } finally { ReleaseDC(hWnd, dc); }
    }

    // DPI_AWARENESS of the window: -1 invalid, 0 unaware, 1 system, 2 per monitor.
    public static int WindowDpiAwareness(IntPtr hWnd) {
        return GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(hWnd));
    }

    // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 is the pseudo handle -4.
    public static bool WindowIsPerMonitorV2(IntPtr hWnd) {
        return AreDpiAwarenessContextsEqual(GetWindowDpiAwarenessContext(hWnd), new IntPtr(-4));
    }

    // hr, PROCESS_DPI_AWARENESS of a process (needs PROCESS_QUERY_LIMITED_INFORMATION, 0x1000).
    public static long[] ProcessDpiAwareness(int processId) {
        IntPtr process = OpenProcess(0x1000, false, (uint)processId);
        if (process == IntPtr.Zero) return new long[] { -1, -1 };
        try {
            int awareness;
            int hr = GetProcessDpiAwareness(process, out awareness);
            return new long[] { hr, awareness };
        } finally { CloseHandle(process); }
    }

    // WM_GETICON (0x7F) with ICON_SMALL (0), ICON_BIG (1) or ICON_SMALL2 (2): the HICON, 0 when the window has
    // none, -1 when it did not answer within a second.
    public static long Icon(IntPtr hWnd, int which) {
        IntPtr result;
        IntPtr answered = SendMessageTimeout(hWnd, 0x7F, new IntPtr(which), IntPtr.Zero, 0x2, 1000, out result);
        if (answered == IntPtr.Zero) return -1;
        return result.ToInt64();
    }

    // GetClassLongPtr: GCLP_HICON (-14) or GCLP_HICONSM (-34).
    public static long ClassIcon(IntPtr hWnd, int index) { return GetClassLongPtr(hWnd, index).ToInt64(); }
}
'@
        }
        $win32 = $true
    } catch {
        Write-ProbeLine 'win32_helper' ('error(' + $_.Exception.Message + ')')
    }
    try {
        if (-not ('ImproProbeShell' -as [type])) {
            Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class ImproProbeShell {
    [StructLayout(LayoutKind.Sequential)]
    private struct PROPERTYKEY { public Guid fmtid; public uint pid; }

    // Sized for the 64-bit PROPVARIANT; vt at offset 0, the pointer member of the union at offset 8.
    [StructLayout(LayoutKind.Explicit, Size = 24)]
    private struct PROPVARIANT {
        [FieldOffset(0)] public ushort vt;
        [FieldOffset(8)] public IntPtr ptr;
    }

    [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IPropertyStore {
        [PreserveSig] int GetCount(out uint count);
        [PreserveSig] int GetAt(uint index, out PROPERTYKEY key);
        [PreserveSig] int GetValue(ref PROPERTYKEY key, out PROPVARIANT value);
        [PreserveSig] int SetValue(ref PROPERTYKEY key, ref PROPVARIANT value);
        [PreserveSig] int Commit();
    }

    [DllImport("shell32.dll")]
    private static extern int SHGetPropertyStoreForWindow(IntPtr hwnd, ref Guid riid, out IPropertyStore store);
    [DllImport("ole32.dll")]
    private static extern int PropVariantClear(ref PROPVARIANT value);

    // System.AppUserModel.ID of the window's property store: { hr of GetValue (or of the store lookup), VARTYPE, text }.
    public static string[] AppUserModelId(IntPtr hwnd) {
        Guid iid = new Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99");
        IPropertyStore store;
        int hr = SHGetPropertyStoreForWindow(hwnd, ref iid, out store);
        if (hr != 0 || store == null) return new string[] { hr.ToString("X8"), "", "" };
        try {
            PROPERTYKEY key = new PROPERTYKEY();
            key.fmtid = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3");
            key.pid = 5;
            PROPVARIANT pv;
            int hr2 = store.GetValue(ref key, out pv);
            string text = "";
            ushort vt = 0;
            if (hr2 == 0) {
                vt = pv.vt;
                if (vt == 31 && pv.ptr != IntPtr.Zero) text = Marshal.PtrToStringUni(pv.ptr);
                PropVariantClear(ref pv);
            }
            return new string[] { hr2.ToString("X8"), vt.ToString(), text };
        } finally {
            Marshal.ReleaseComObject(store);
        }
    }
}
'@
        }
        $shell = $true
    } catch {
        Write-ProbeLine 'shell_helper' ('error(' + $_.Exception.Message + ')')
    }
    [pscustomobject]@{ Win32 = $win32; Shell = $shell }
}

function Invoke-ProbeFact {
    # Prints `key=<value>` for the output of the script block; an exception prints `key=error(<message>)`.
    param([Parameter(Mandatory)][string]$Key, [Parameter(Mandatory)][scriptblock]$Get)
    $text = ''
    try {
        $result = & $Get
        $text = [string]$result
    } catch {
        $text = 'error(' + (Get-OneLine $_.Exception.Message) + ')'
    }
    Write-ProbeLine $Key $text
}

function Get-ProbeSilently {
    # The output of the script block, or $null when it throws. An array stays an array.
    param([Parameter(Mandatory)][scriptblock]$Get)
    try { $result = & $Get } catch { return $null }
    , $result
}

function Write-PixelProbes {
    # Prints bitmap_size and the colour of every probe point of the capture.
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][int]$CaptionHeight, [int]$Dpi = 96)
    Add-Type -AssemblyName System.Drawing
    $bmp = [System.Drawing.Bitmap]::new($Path)
    try {
        Write-ProbeLine 'bitmap_size' ('{0}x{1}' -f $bmp.Width, $bmp.Height)
        foreach ($pt in @(Get-ProbePoints -Width $bmp.Width -Height $bmp.Height -CaptionHeight $CaptionHeight -Dpi $Dpi)) {
            Write-ProbeLine ('pixel_pos(' + $pt.Name + ')') ('{0},{1}' -f $pt.X, $pt.Y)
            if ($pt.InRange) {
                $c = $bmp.GetPixel($pt.X, $pt.Y)
                Write-ProbeLine ('pixel(' + $pt.Name + ')') (Format-PixelColor -R $c.R -G $c.G -B $c.B)
            } else {
                Write-ProbeLine ('pixel(' + $pt.Name + ')') 'n/a(outside the bitmap)'
            }
        }
    } finally { $bmp.Dispose() }
}

function Write-WindowProbe {
    # Prints the facts about one window. Call it while the window exists. $ScreenshotPath is the CopyFromScreen
    # capture of that window (frame included); without it the pixel probes are skipped.
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [string]$ScreenshotPath = '',
        [string]$ExpectedAppUserModelId = 'dev.impro.impro'
    )
    try {
        $h = $Hwnd
        Write-ProbeLine 'hwnd' (ConvertTo-Hex32 -Value ($h.ToInt64()))
        Invoke-ProbeFact 'os_version' { [Environment]::OSVersion.Version.ToString() }
        Invoke-ProbeFact 'os_build' { [Environment]::OSVersion.Version.Build }
        Invoke-ProbeFact 'os_build_ubr' { [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'UBR', 'n/a') }
        Invoke-ProbeFact 'os_display_version' { [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'DisplayVersion', 'n/a') }
        Invoke-ProbeFact 'os_product' { [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'ProductName', 'n/a') }

        $avail = Initialize-WindowProbe
        if (-not $avail.Win32) { Write-ProbeLine 'window' 'n/a(the Win32 helper did not compile)'; return }

        # --- identity
        $procId = Get-ProbeSilently { [ImproProbe]::ProcessId($h) }
        Invoke-ProbeFact 'pid' { [ImproProbe]::ProcessId($h) }
        Invoke-ProbeFact 'process_path' { (Get-Process -Id ([int]$procId) -ErrorAction Stop).Path }
        Invoke-ProbeFact 'title' { [ImproProbe]::Title($h) }
        Invoke-ProbeFact 'class' { [ImproProbe]::ClassName($h) }

        # --- style bits
        $style = Get-ProbeSilently { [ImproProbe]::Style($h) }
        Invoke-ProbeFact 'style' { ConvertTo-Hex32 -Value ([long][ImproProbe]::Style($h)) }
        Invoke-ProbeFact 'exstyle' { ConvertTo-Hex32 -Value ([long][ImproProbe]::ExStyle($h)) }
        $hasCaption = $false
        if ($null -ne $style) {
            $flags = Get-WindowStyleFlags -Style ([long]$style)
            foreach ($flagName in $flags.Keys) {
                $flagKey = ([string]$flagName).ToLowerInvariant()
                Write-ProbeLine $flagKey (ConvertTo-BoolText ([bool]$flags[$flagName]))
            }
            $hasCaption = [bool]$flags['WS_CAPTION']
        }

        # --- geometry
        $wr = Get-ProbeSilently { [ImproProbe]::WindowRect($h) }
        $cr = Get-ProbeSilently { [ImproProbe]::ClientRect($h) }
        $co = Get-ProbeSilently { [ImproProbe]::ClientOrigin($h) }
        $fb = Get-ProbeSilently { [ImproProbe]::DwmFrameBounds($h) }
        Invoke-ProbeFact 'window_rect' { ([ImproProbe]::WindowRect($h)) -join ',' }
        Invoke-ProbeFact 'window_size' { '{0}x{1}' -f ($wr[2] - $wr[0]), ($wr[3] - $wr[1]) }
        Invoke-ProbeFact 'client_size' { '{0}x{1}' -f ($cr[2] - $cr[0]), ($cr[3] - $cr[1]) }
        Invoke-ProbeFact 'client_origin_screen' { $co -join ',' }
        Invoke-ProbeFact 'dwm_frame_bounds' { '{0},{1},{2},{3}' -f $fb[1], $fb[2], $fb[3], $fb[4] }
        Invoke-ProbeFact 'dwm_frame_size' { '{0}x{1}' -f ($fb[3] - $fb[1]), ($fb[4] - $fb[2]) }
        Invoke-ProbeFact 'dwm_frame_hr' { ConvertTo-Hex32 -Value ([long]$fb[0]) }
        $frameTop = $null
        if ($null -ne $fb -and [int]$fb[0] -eq 0) { $frameTop = [int]$fb[2] } elseif ($null -ne $wr) { $frameTop = [int]$wr[1] }
        $ncTop = 0
        if ($null -ne $co -and $null -ne $frameTop) { $ncTop = [int]$co[1] - [int]$frameTop }
        Write-ProbeLine 'nonclient_top_px' ([string]$ncTop)

        # --- DPI
        $ctx = @{ DpiSource = 'none' }
        $dpiRaw = Get-ProbeSilently {
            $d = 0
            try { $d = [ImproProbe]::Dpi($h); $ctx.DpiSource = 'GetDpiForWindow' } catch { $d = 0 }
            if ($d -le 0) { $d = [ImproProbe]::DpiFromDeviceCaps($h); $ctx.DpiSource = 'GetDeviceCaps' }
            $d
        }
        $dpi = 96
        if ($null -ne $dpiRaw -and [int]$dpiRaw -gt 0) { $dpi = [int]$dpiRaw }
        Write-ProbeLine 'dpi_window' $(if ($null -ne $dpiRaw) { [string]$dpiRaw } else { 'n/a' })
        Write-ProbeLine 'dpi_source' $ctx.DpiSource
        Write-ProbeLine 'dpi_scale' ([string]::Format([Globalization.CultureInfo]::InvariantCulture, '{0:0.00}', ($dpi / 96.0)))
        $aw = Get-ProbeSilently { [ImproProbe]::WindowDpiAwareness($h) }
        Write-ProbeLine 'dpi_awareness_window' $(if ($null -ne $aw) { '{0}({1})' -f (Get-DpiAwarenessName -Value ([long]$aw)), $aw } else { 'n/a' })
        Invoke-ProbeFact 'dpi_per_monitor_v2_window' { ConvertTo-BoolText ([bool][ImproProbe]::WindowIsPerMonitorV2($h)) }
        $pa = Get-ProbeSilently { [ImproProbe]::ProcessDpiAwareness([int]$procId) }
        if ($null -ne $pa -and [long]$pa[0] -eq 0) {
            Write-ProbeLine 'dpi_awareness_process' ('{0}({1})' -f (Get-DpiAwarenessName -Value ([long]$pa[1])), $pa[1])
        } else {
            Write-ProbeLine 'dpi_awareness_process' 'n/a'
        }
        if ($null -ne $pa) { Write-ProbeLine 'dpi_awareness_process_hr' (ConvertTo-Hex32 -Value ([long]$pa[0])) }

        # --- DWM attributes: value and HRESULT of each read-back
        $dwmSpecs = @(
            @('dwm_immersive_dark_mode', 20, 'bool'),
            @('dwm_window_corner_preference', 33, 'corner'),
            @('dwm_border_color', 34, 'color'),
            @('dwm_caption_color', 35, 'color'),
            @('dwm_text_color', 36, 'color')
        )
        foreach ($spec in $dwmSpecs) {
            $dwmKey = [string]$spec[0]
            $dwmAttr = [int]$spec[1]
            $dwmKind = [string]$spec[2]
            $res = Get-ProbeSilently { [ImproProbe]::DwmAttribute($h, $dwmAttr) }
            if ($null -eq $res) {
                Write-ProbeLine $dwmKey 'error(DwmGetWindowAttribute failed)'
                continue
            }
            if ([long]$res[0] -eq 0) {
                Write-ProbeLine $dwmKey (Format-DwmValue -Kind $dwmKind -Value ([long]$res[1]))
            } else {
                Write-ProbeLine $dwmKey 'n/a'
            }
            Write-ProbeLine ($dwmKey + '_hr') (ConvertTo-Hex32 -Value ([long]$res[0]))
        }

        # --- AppUserModelID of the window (the per-window property store; a process-level ID may not show here)
        if ($avail.Shell) {
            $aum = Get-ProbeSilently { [ImproProbeShell]::AppUserModelId($h) }
            if ($null -eq $aum) {
                Write-ProbeLine 'app_user_model_id' 'error(SHGetPropertyStoreForWindow failed)'
            } else {
                $aumText = [string]$aum[2]
                $aumShown = $aumText
                if ([string]$aum[0] -ne '00000000') {
                    $aumShown = 'n/a'
                } elseif ($aumText -eq '') {
                    $aumShown = '(empty)'
                }
                Write-ProbeLine 'app_user_model_id' $aumShown
                Write-ProbeLine 'app_user_model_id_hr' ('0x' + [string]$aum[0])
                Write-ProbeLine 'app_user_model_id_vartype' ([string]$aum[1])
                Write-ProbeLine 'app_user_model_id_matches' (ConvertTo-BoolText ($aumText -ceq $ExpectedAppUserModelId))
            }
        } else {
            Write-ProbeLine 'app_user_model_id' 'n/a(the shell helper did not compile)'
        }

        # --- icons
        foreach ($iconSpec in @(@('small', 0), @('big', 1), @('small2', 2))) {
            $iconName = [string]$iconSpec[0]
            $iconWhich = [int]$iconSpec[1]
            $icon = Get-ProbeSilently { [ImproProbe]::Icon($h, $iconWhich) }
            if ($null -eq $icon -or [long]$icon -lt 0) {
                Write-ProbeLine ('wm_geticon_' + $iconName) 'n/a(no answer)'
            } else {
                Write-ProbeLine ('wm_geticon_' + $iconName) (ConvertTo-Hex32 -Value ([long]$icon))
                Write-ProbeLine ('wm_geticon_' + $iconName + '_nonzero') (ConvertTo-BoolText ([long]$icon -ne 0))
            }
        }
        Invoke-ProbeFact 'class_icon_big' { ConvertTo-Hex32 -Value ([ImproProbe]::ClassIcon($h, -14)) }
        Invoke-ProbeFact 'class_icon_small' { ConvertTo-Hex32 -Value ([ImproProbe]::ClassIcon($h, -34)) }

        # --- caption height and the pixels of the capture
        $wh = 0
        if ($null -ne $wr) { $wh = [int]$wr[3] - [int]$wr[1] }
        $ch = 0
        if ($null -ne $cr) { $ch = [int]$cr[3] - [int]$cr[1] }
        $cap = Get-CaptionHeight -NonClientTop $ncTop -WindowHeight $wh -ClientHeight $ch -Dpi $dpi -HasCaption $hasCaption
        Write-ProbeLine 'caption_height_px' ([string]$cap.Height)
        Write-ProbeLine 'caption_height_source' ([string]$cap.Source)
        if ($ScreenshotPath -and (Test-Path -LiteralPath $ScreenshotPath)) {
            try { Write-PixelProbes -Path $ScreenshotPath -CaptionHeight ([int]$cap.Height) -Dpi $dpi }
            catch { Write-ProbeLine 'pixel_probes' ('error(' + (Get-OneLine $_.Exception.Message) + ')') }
        } else {
            Write-ProbeLine 'bitmap_size' 'n/a(no screenshot)'
        }
        Write-ProbeLine 'done' 'true'
    } catch {
        Write-LogLine ('::warning::window probe stopped early: ' + (Get-OneLine $_.Exception.Message))
    }
}
