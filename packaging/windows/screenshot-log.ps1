# Helpers behind print-screenshots.ps1 (dot-sourced; tested by test_screenshot_log.ps1).
#
# A screenshot goes into the job log as
#
#   -----BEGIN IMPRO-SCREENSHOT <file name>-----
#   <base64, 100 columns>
#   -----END IMPRO-SCREENSHOT <file name>-----
#
# and scripts/ci/extract_log_screenshots.py turns a saved log back into PNG files. Keep the two in step.
#
# Functions that return a byte[] or a string[] emit it with a leading comma: without it PowerShell unrolls the array
# into the pipeline, which is slow for megabytes and turns a one-element array into a scalar.

function ConvertTo-WrappedBase64 {
    # The base64 text of the bytes, cut into lines of $Width characters (the last one may be shorter).
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][byte[]]$Bytes,
        [int]$Width = 100
    )
    $text = [Convert]::ToBase64String($Bytes)
    $lines = [System.Collections.Generic.List[string]]::new()
    for ($i = 0; $i -lt $text.Length; $i += $Width) {
        $lines.Add($text.Substring($i, [Math]::Min($Width, $text.Length - $i)))
    }
    , $lines.ToArray()
}

function Format-ScreenshotLogBlock {
    # The BEGIN line, the base64 lines and the END line of one file.
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][AllowEmptyCollection()][byte[]]$Bytes,
        [int]$Width = 100
    )
    $body = ConvertTo-WrappedBase64 -Bytes $Bytes -Width $Width
    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add("-----BEGIN IMPRO-SCREENSHOT ${Name}-----")
    foreach ($line in $body) { $lines.Add($line) }
    $lines.Add("-----END IMPRO-SCREENSHOT ${Name}-----")
    , $lines.ToArray()
}

function Get-DownscaledPngBytes {
    # PNG bytes of the image scaled to $MaxWidth pixels wide, keeping the aspect ratio. An image that is not wider
    # keeps its file bytes. Needs System.Drawing (Windows).
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][int]$MaxWidth)
    Add-Type -AssemblyName System.Drawing
    $src = [System.Drawing.Bitmap]::new($Path)
    try {
        if ($src.Width -le $MaxWidth) { return , [IO.File]::ReadAllBytes($Path) }
        $height = [Math]::Max(1, [int][Math]::Round($src.Height * $MaxWidth / $src.Width))
        $dst = [System.Drawing.Bitmap]::new($MaxWidth, $height, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
        try {
            $g = [System.Drawing.Graphics]::FromImage($dst)
            try {
                $g.Clear([System.Drawing.Color]::White)
                $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $g.DrawImage($src, 0, 0, $MaxWidth, $height)
            } finally { $g.Dispose() }
            $ms = [System.IO.MemoryStream]::new()
            try {
                $dst.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
                return , $ms.ToArray()
            } finally { $ms.Dispose() }
        } finally { $dst.Dispose() }
    } finally { $src.Dispose() }
}

function Get-ScreenshotLogBytes {
    # The bytes to print for one PNG: the file itself up to $ScaleAboveBytes, a downscaled copy above that. A file
    # that cannot be downscaled is printed as it is; one that is still larger than $MaxPrintBytes is refused.
    param(
        [Parameter(Mandatory)][string]$Path,
        [int]$MaxWidth = 1100,
        [long]$ScaleAboveBytes = 256000,
        [long]$MaxPrintBytes = 4MB
    )
    $length = (Get-Item -LiteralPath $Path).Length
    if ($length -le $ScaleAboveBytes) { return , [IO.File]::ReadAllBytes($Path) }
    try {
        $bytes = Get-DownscaledPngBytes -Path $Path -MaxWidth $MaxWidth
    } catch {
        Write-Host "::warning::could not downscale ${Path}, printing the original: $($_.Exception.Message)"
        $bytes = [IO.File]::ReadAllBytes($Path)
    }
    if ($bytes.Length -gt $MaxPrintBytes) { throw "$($bytes.Length) bytes is more than the $MaxPrintBytes bytes allowed in a log" }
    , $bytes
}
