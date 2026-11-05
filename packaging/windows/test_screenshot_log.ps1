# Plain-assert tests for screenshot-log.ps1 and print-screenshots.ps1 (PowerShell 7):
#   pwsh -NoProfile -File packaging/windows/test_screenshot_log.ps1
# The downscaling case needs System.Drawing and only runs on Windows (the `check` job); the rest runs anywhere.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'screenshot-log.ps1')

$script:failures = 0
function Assert-Equal($Actual, $Expected, [string]$What) {
    if ($Actual -ceq $Expected) { Write-Host "ok   $What" }
    else { Write-Host "FAIL $What`n  expected: $Expected`n  actual:   $Actual"; $script:failures++ }
}

# --- base64 wrapping
$all = [byte[]](0..255)
$lines = ConvertTo-WrappedBase64 -Bytes $all -Width 100
Assert-Equal $lines.Count 4 '256 bytes are 344 base64 characters: four lines at width 100'
Assert-Equal $lines[0].Length 100 'first line is full width'
Assert-Equal $lines[3].Length 44 'last line holds the rest'
Assert-Equal ($lines -join '') ([Convert]::ToBase64String($all)) 'the lines join back to the base64 text'
$one = ConvertTo-WrappedBase64 -Bytes ([byte[]](1, 2, 3))
Assert-Equal $one.Count 1 'a short input stays an array of one line'
Assert-Equal $one[0] 'AQID' 'base64 of 01 02 03'
$none = ConvertTo-WrappedBase64 -Bytes ([byte[]]@())
Assert-Equal @($none).Count 0 'no bytes, no lines'

# --- marker lines
$block = Format-ScreenshotLogBlock -Name 'desktop-window.png' -Bytes ([byte[]](1, 2, 3))
Assert-Equal $block.Count 3 'marker, one body line, marker'
Assert-Equal $block[0] '-----BEGIN IMPRO-SCREENSHOT desktop-window.png-----' 'BEGIN marker line'
Assert-Equal $block[1] 'AQID' 'body line'
Assert-Equal $block[2] '-----END IMPRO-SCREENSHOT desktop-window.png-----' 'END marker line'
$big = Format-ScreenshotLogBlock -Name 'x.png' -Bytes $all
Assert-Equal $big.Count 6 'four body lines plus two markers'

# --- the real script: print a directory, decode the log text and compare with the files
$png = [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==')
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("impro-shot-test-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    [IO.File]::WriteAllBytes((Join-Path $tmp 'b-second.png'), $png)
    [IO.File]::WriteAllBytes((Join-Path $tmp 'a-first.png'), $png)
    Set-Content -LiteralPath (Join-Path $tmp 'notes.txt') -Value 'not a screenshot'
    $printScript = Join-Path $PSScriptRoot 'print-screenshots.ps1'
    $out = @(& $printScript -Directory $tmp *>&1 | ForEach-Object { "$_" })
    $text = $out -join "`n"
    $logLines = @($text -split "`r?`n")
    $begins = @($logLines | Where-Object { $_ -like '-----BEGIN IMPRO-SCREENSHOT *-----' })
    $ends = @($logLines | Where-Object { $_ -like '-----END IMPRO-SCREENSHOT *-----' })
    Assert-Equal $begins.Count 2 'one BEGIN line per PNG, the text file is ignored'
    Assert-Equal $ends.Count 2 'one END line per PNG'
    Assert-Equal $begins[0] '-----BEGIN IMPRO-SCREENSHOT a-first.png-----' 'files are printed in name order'
    $start = [Array]::IndexOf($logLines, $begins[0])
    $stop = [Array]::IndexOf($logLines, $ends[0])
    $body = ($logLines[($start + 1)..($stop - 1)]) -join ''
    Assert-Equal ([Convert]::ToBase64String($png)) $body 'the printed body decodes to the file bytes'
    Assert-Equal ($text -match 'printed 2 of 2 file') $true 'summary line'

    $missing = @(& $printScript -Directory (Join-Path $tmp 'nope') *>&1 | ForEach-Object { "$_" }) -join "`n"
    Assert-Equal ($missing -match 'no such directory') $true 'a missing directory is a warning, not a failure'

    # --- downscaling (Windows only: System.Drawing)
    if ($IsWindows) {
        Add-Type -AssemblyName System.Drawing
        $wide = Join-Path $tmp 'wide.png'
        $bmp = [System.Drawing.Bitmap]::new(1600, 800, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
        try {
            # Random bytes do not compress, so the file is far above 250 KB.
            $rect = [System.Drawing.Rectangle]::new(0, 0, 1600, 800)
            $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::WriteOnly, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
            try {
                $noise = [byte[]]::new([Math]::Abs($data.Stride) * 800)
                [Random]::new(7).NextBytes($noise)
                [System.Runtime.InteropServices.Marshal]::Copy($noise, 0, $data.Scan0, $noise.Length)
            } finally { $bmp.UnlockBits($data) }
            $bmp.Save($wide, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $bmp.Dispose() }
        Assert-Equal ((Get-Item -LiteralPath $wide).Length -gt 250KB) $true 'the test image is larger than 250 KB'
        $scaled = Get-ScreenshotLogBytes -Path $wide -MaxWidth 1100 -ScaleAboveBytes (250KB)
        $ms = [System.IO.MemoryStream]::new($scaled)
        try {
            $img = [System.Drawing.Image]::FromStream($ms)
            try {
                Assert-Equal $img.Width 1100 'a large image is downscaled to the maximum width'
                Assert-Equal $img.Height 550 'the aspect ratio is kept'
            } finally { $img.Dispose() }
        } finally { $ms.Dispose() }
        $same = Get-ScreenshotLogBytes -Path (Join-Path $tmp 'a-first.png') -MaxWidth 1100 -ScaleAboveBytes (250KB)
        Assert-Equal $same.Length $png.Length 'a small file is printed unchanged'
    }
} finally { Remove-Item -LiteralPath $tmp -Recurse -Force }

if ($script:failures) { Write-Host "$script:failures failure(s)"; exit 1 }
Write-Host 'all passed'
