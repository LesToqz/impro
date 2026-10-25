# Pure helpers shared by smoke.ps1 and startup-lab.ps1 (dot-sourced, tested by test_lab_helpers.ps1).

if (-not ('ImproPixels' -as [type])) {
    Add-Type -TypeDefinition @'
public static class ImproPixels {
    // Fraction of pixels whose 4 bytes differ from the top-left pixel. `data` holds rows of `stride` bytes.
    public static double DiffFraction(byte[] data, int stride, int width, int height) {
        if (width <= 0 || height <= 0) return 0.0;
        byte b = data[0], g = data[1], r = data[2], a = data[3];
        long diff = 0;
        for (int y = 0; y < height; y++) {
            int row = y * stride;
            for (int x = 0; x < width; x++) {
                int i = row + x * 4;
                if (data[i] != b || data[i + 1] != g || data[i + 2] != r || data[i + 3] != a) diff++;
            }
        }
        return (double)diff / ((double)width * height);
    }
}
'@
}

function Get-TimingValue {
    # Value of the first `name=<number>` in the text (for example first-paint-ms=812.345), or $null.
    param([AllowEmptyString()][string]$Text, [Parameter(Mandatory)][string]$Name)
    $m = [regex]::Match($Text, '(?m)(?:^|[\s,;])' + [regex]::Escape($Name) + '=(\d+(?:\.\d+)?)')
    if ($m.Success) { [double]::Parse($m.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture) } else { $null }
}

function Format-Millis {
    param($Value)
    if ($null -eq $Value) { return 'n/a' }
    ([double]$Value).ToString('N0', [Globalization.CultureInfo]::InvariantCulture)
}

function Format-LabTable {
    # GitHub-flavoured Markdown table. Rows are objects with Label, FirstPaint, GtkStartup, ExitCode.
    param([Parameter(Mandatory)][object[]]$Rows)
    $lines = @('| # | run | first-paint-ms | app-gtk-startup-ms | exit |', '|---|-----|---------------:|-------------------:|-----:|')
    $i = 0
    foreach ($r in $Rows) {
        $i++
        $lines += ('| {0} | {1} | {2} | {3} | {4} |' -f $i, $r.Label, (Format-Millis $r.FirstPaint), (Format-Millis $r.GtkStartup), $r.ExitCode)
    }
    $lines -join "`n"
}

function Get-ImageStats {
    # Decode a PNG with System.Drawing (Windows) and report size and the fraction of pixels that differ
    # from the top-left pixel colour.
    param([Parameter(Mandatory)][string]$Path)
    Add-Type -AssemblyName System.Drawing
    $bmp = [System.Drawing.Bitmap]::new($Path)
    try {
        $rect = [System.Drawing.Rectangle]::new(0, 0, $bmp.Width, $bmp.Height)
        $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $stride = [Math]::Abs($data.Stride)
            $bytes = [byte[]]::new($stride * $bmp.Height)
            [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
        } finally { $bmp.UnlockBits($data) }
        [pscustomobject]@{ Width = $bmp.Width; Height = $bmp.Height; DiffFraction = [ImproPixels]::DiffFraction($bytes, $stride, $bmp.Width, $bmp.Height) }
    } finally { $bmp.Dispose() }
}

function Test-ScreenshotProblems {
    # Emits one string per problem (nothing = fine) and logs the numbers; callers wrap the call in @(...).
    # Never `return , array`: wrapped in @() that nests the array and an empty one then looks like one problem.
    # A decoded-pixel check is the verdict; the byte-size floor is only a hint (a warning, never a failure).
    param([Parameter(Mandatory)][string]$Path, [double]$MinDiffFraction = 0.01, [int]$SizeHint = 5000)
    $problems = [System.Collections.Generic.List[string]]::new()
    if (-not (Test-Path -LiteralPath $Path) -or (Get-Item -LiteralPath $Path).Length -eq 0) {
        $problems.Add("no screenshot written to $Path"); return $problems.ToArray()
    }
    $bytes = [IO.File]::ReadAllBytes($Path)
    $sig = ($bytes[0..([Math]::Min(7, $bytes.Length - 1))] | ForEach-Object { $_.ToString('x2') }) -join ''
    if ($sig -ne '89504e470d0a1a0a') { $problems.Add("screenshot is not a PNG (magic $sig)"); return $problems.ToArray() }
    try {
        $s = Get-ImageStats -Path $Path
        Write-Host ("[smoke] screenshot {0}: {1} bytes, {2}x{3}, {4:P2} of pixels differ from the top-left pixel" -f (Split-Path $Path -Leaf), $bytes.Length, $s.Width, $s.Height, $s.DiffFraction)
        if ($s.DiffFraction -lt $MinDiffFraction) {
            $problems.Add(("screenshot looks blank: only {0:P2} of pixels differ from the top-left pixel (need >= {1:P0})" -f $s.DiffFraction, $MinDiffFraction))
        }
    } catch {
        $problems.Add("cannot decode screenshot with System.Drawing: $($_.Exception.Message)")
    }
    if ($bytes.Length -lt $SizeHint) { Write-Host "::warning::screenshot is only $($bytes.Length) bytes (hint: may be blank)" }
    $problems.ToArray()
}

function Get-ByteSum {
    # Total of the Length property over the items; 0 for no items or items without a Length (strict-mode safe).
    param([AllowNull()][object[]]$Items)
    $sum = [long]0
    foreach ($i in @($Items)) { if ($null -ne $i -and $i.PSObject.Properties['Length']) { $sum += [long]$i.Length } }
    $sum
}

function Limit-Lines {
    # The first $Count lines of the text (all of it when shorter), joined with newlines.
    param([AllowEmptyString()][string]$Text, [Parameter(Mandatory)][int]$Count)
    (($Text -split "`r?`n") | Select-Object -First $Count) -join "`n"
}
