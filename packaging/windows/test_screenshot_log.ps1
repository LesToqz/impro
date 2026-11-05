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
    # Only desktop-window.png and ONE first-paint*.png are printed; the others get a "skipped" line.
    [IO.File]::WriteAllBytes((Join-Path $tmp 'desktop-window.png'), $png)
    [IO.File]::WriteAllBytes((Join-Path $tmp 'first-paint-installer-warm.png'), $png)
    [IO.File]::WriteAllBytes((Join-Path $tmp 'first-paint-installer-cold.png'), $png)
    [IO.File]::WriteAllBytes((Join-Path $tmp 'b-other.png'), $png)
    Set-Content -LiteralPath (Join-Path $tmp 'notes.txt') -Value 'not a screenshot'
    $printScript = Join-Path $PSScriptRoot 'print-screenshots.ps1'
    $out = @(& $printScript -Directory $tmp *>&1 | ForEach-Object { "$_" })
    $text = $out -join "`n"
    $logLines = @($text -split "`r?`n")
    $begins = @($logLines | Where-Object { $_ -like '-----BEGIN IMPRO-SCREENSHOT *-----' })
    $ends = @($logLines | Where-Object { $_ -like '-----END IMPRO-SCREENSHOT *-----' })
    Assert-Equal $begins.Count 2 'two blocks: the window and one first-paint, the text file is ignored'
    Assert-Equal $ends.Count 2 'one END line per BEGIN line'
    if ($begins.Count -ge 2 -and $ends.Count -ge 1) {
        $firstBegin = $begins[0]
        $secondBegin = $begins[1]
        Assert-Equal $firstBegin '-----BEGIN IMPRO-SCREENSHOT desktop-window.png-----' 'the real window comes first (name order)'
        Assert-Equal $secondBegin '-----BEGIN IMPRO-SCREENSHOT first-paint-installer-cold.png-----' 'the first first-paint file in name order'
        $start = [Array]::IndexOf($logLines, $firstBegin)
        $stop = [Array]::IndexOf($logLines, $ends[0])
        $body = ($logLines[($start + 1)..($stop - 1)]) -join ''
        Assert-Equal ([Convert]::ToBase64String($png)) $body 'the printed body decodes to the file bytes'
    } else {
        # No block: show what the script printed (status lines, warnings, errors) so that the cause is in the log.
        Write-Host "--- output of print-screenshots.ps1:`n$text`n---"
    }
    Assert-Equal ($text -match '\[screenshots\] skipped b-other\.png \(70 bytes\)') $true 'an unselected file gets a skipped line with its size'
    Assert-Equal ($text -match '\[screenshots\] skipped first-paint-installer-warm\.png \(70 bytes\)') $true 'the second first-paint file is skipped'
    Assert-Equal ($text -match 'printed 2 of 4 file') $true 'summary line'

    # The [screenshots] status lines (not the base64) are also appended to window-probe.txt in the same directory.
    $statusFile = Join-Path $tmp 'window-probe.txt'
    Assert-Equal (Test-Path -LiteralPath $statusFile) $true 'the status lines are also written to window-probe.txt'
    $statusText = Get-Content -LiteralPath $statusFile -Raw
    Assert-Equal ($statusText -match '\[screenshots\] printed 2 of 4 file') $true 'window-probe.txt has the summary line'
    Assert-Equal ($statusText -match '\[screenshots\] skipped b-other\.png') $true 'window-probe.txt has the skipped lines'
    Assert-Equal ($statusText -match 'BEGIN IMPRO-SCREENSHOT') $false 'the base64 blocks are not copied into window-probe.txt'
    # The log file is a global: print-screenshots.ps1 must put the previous value back, because the next test script of the
    # CI step runs in this same process after this directory is deleted (a stale path printed an extra warning line).
    $leftoverLog = Get-Variable -Name ImproProbeLogFile -Scope Global -ValueOnly -ErrorAction SilentlyContinue
    Assert-Equal ([bool]$leftoverLog) $false 'print-screenshots.ps1 leaves no log file target behind'

    # A file whose base64 would exceed the cap is skipped, not printed (random bytes: not a decodable image, so the
    # original is considered, and 100000 bytes are about 133 KB of base64).
    $oversizeDir = Join-Path $tmp 'oversize'
    New-Item -ItemType Directory -Path $oversizeDir | Out-Null
    $noise = [byte[]]::new(100000)
    [Random]::new(3).NextBytes($noise)
    [IO.File]::WriteAllBytes((Join-Path $oversizeDir 'desktop-window.png'), $noise)
    $capText = @(& $printScript -Directory $oversizeDir *>&1 | ForEach-Object { "$_" }) -join "`n"
    Assert-Equal ($capText -match '-----BEGIN IMPRO-SCREENSHOT') $false 'an oversized file is not printed'
    Assert-Equal ($capText -match '\[screenshots\] skipped desktop-window\.png \(100000 bytes on disk') $true 'an oversized file gets a skipped line'
    Assert-Equal ($capText -match 'printed 0 of 1 file') $true 'summary line of the oversize case'

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
        $same = Get-ScreenshotLogBytes -Path (Join-Path $tmp 'desktop-window.png') -MaxWidth 1100 -ScaleAboveBytes (250KB)
        Assert-Equal $same.Length $png.Length 'a small file is printed unchanged'
        # print-screenshots.ps1 always downscales first (-ScaleAboveBytes 0): a 1-pixel image is not wider than the limit.
        $always = Get-ScreenshotLogBytes -Path (Join-Path $tmp 'desktop-window.png') -MaxWidth 700 -ScaleAboveBytes 0
        Assert-Equal $always.Length $png.Length 'an image narrower than the limit keeps its bytes'
        $wide700 = Get-ScreenshotLogBytes -Path $wide -MaxWidth 700 -ScaleAboveBytes 0
        $ms7 = [System.IO.MemoryStream]::new($wide700)
        try {
            $img7 = [System.Drawing.Image]::FromStream($ms7)
            try { Assert-Equal $img7.Width 700 'the default limit of print-screenshots.ps1 is 700 px' } finally { $img7.Dispose() }
        } finally { $ms7.Dispose() }
    }
} finally { Remove-Item -LiteralPath $tmp -Recurse -Force }

if ($script:failures) { Write-Host "$script:failures failure(s)"; exit 1 }
Write-Host 'all passed'
