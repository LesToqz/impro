# Plain-assert tests for lab-helpers.ps1 (PowerShell 7, any OS): pwsh -NoProfile -File packaging/windows/test_lab_helpers.ps1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lab-helpers.ps1')

$script:failures = 0
function Assert-Equal($Actual, $Expected, [string]$What) {
    if ($Actual -ceq $Expected) { Write-Host "ok   $What" }
    else { Write-Host "FAIL $What`n  expected: $Expected`n  actual:   $Actual"; $script:failures++ }
}

$text = "STANDIN-CHECK pango-font ok`napp-gtk-startup-ms=8118.250`nfirst-paint-ms=8712.6`nprefix-first-paint-ms=1"
Assert-Equal (Get-TimingValue $text 'first-paint-ms') 8712.6 'first-paint-ms is found'
Assert-Equal (Get-TimingValue $text 'app-gtk-startup-ms') 8118.25 'app-gtk-startup-ms is found'
Assert-Equal (Get-TimingValue $text 'missing-ms') $null 'absent name gives $null'
Assert-Equal (Get-TimingValue '' 'first-paint-ms') $null 'empty text gives $null'
Assert-Equal (Get-TimingValue 'xfirst-paint-ms=5' 'first-paint-ms') $null 'name must start at a boundary'

Assert-Equal (Format-Millis 8118.4) '8,118' 'millis are rounded with a separator'
Assert-Equal (Format-Millis $null) 'n/a' 'missing millis'

$rows = @(
    [pscustomobject]@{ Label = 'default (cold)'; FirstPaint = 8712.6; GtkStartup = 8118.25; ExitCode = 0 },
    [pscustomobject]@{ Label = 'FC_DEBUG=1'; FirstPaint = $null; GtkStartup = $null; ExitCode = 1 })
$table = (Format-LabTable $rows) -split "`n"
Assert-Equal $table.Count 4 'table has header, separator and one line per row'
Assert-Equal $table[2] '| 1 | default (cold) | 8,713 | 8,118 | 0 |' 'table row'
Assert-Equal $table[3] '| 2 | FC_DEBUG=1 | n/a | n/a | 1 |' 'table row with missing numbers'

Assert-Equal (Limit-Lines "a`r`nb`nc`nd" 2) "a`nb" 'Limit-Lines keeps the first lines'
Assert-Equal (Limit-Lines 'a' 40) 'a' 'Limit-Lines keeps short text'

$flat = [byte[]]::new(4 * 10 * 10)  # all zero: every pixel equals the top-left one
Assert-Equal ([ImproPixels]::DiffFraction($flat, 40, 10, 10)) 0.0 'uniform image: 0 % differ'
$flat[4 * 5 + 0] = 255; $flat[4 * 15 + 1] = 255   # two pixels differ
Assert-Equal ([ImproPixels]::DiffFraction($flat, 40, 10, 10)) 0.02 'two of 100 pixels differ'
$padded = [byte[]]::new(48 * 2)                    # 10 px wide, stride 48: padding bytes are ignored
$padded[44] = 9
Assert-Equal ([ImproPixels]::DiffFraction($padded, 48, 10, 2)) 0.0 'row padding is ignored'

# Verdict plumbing: the caller wraps the result in @(...), exactly as smoke.ps1 does.
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("impro-lab-test-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $none = @(Test-ScreenshotProblems -Path (Join-Path $tmp 'missing.png'))
    Assert-Equal $none.Count 1 'missing screenshot: exactly one problem'
    Assert-Equal ($none[0] -is [string]) $true 'the problem is a string, not a nested array'
    Assert-Equal (($none -join '; ') -like 'no screenshot written to *') $true 'message lists the problem text'
    $bad = Join-Path $tmp 'bad.png'; Set-Content -LiteralPath $bad -Value 'not a png'
    $notPng = @(Test-ScreenshotProblems -Path $bad)
    Assert-Equal $notPng.Count 1 'non-PNG: exactly one problem'
    Assert-Equal (($notPng -join '; ') -like 'screenshot is not a PNG*') $true 'non-PNG message'
} finally { Remove-Item -LiteralPath $tmp -Recurse -Force }

# A verdict function that finds nothing wrong must yield an empty array, one or two problems must stay flat.
function Get-Mock([int]$N) { for ($i = 0; $i -lt $N; $i++) { "p$i" } }
Assert-Equal @(Get-Mock 0).Count 0 'zero problems pass'
Assert-Equal @(Get-Mock 1).Count 1 'one problem counts once'
Assert-Equal (@(Get-Mock 2) -join '; ') 'p0; p1' 'two problems are listed by text'

Assert-Equal (Get-ByteSum @()) 0 'byte sum of an empty list is 0'
Assert-Equal (Get-ByteSum $null) 0 'byte sum of $null is 0'
Assert-Equal (Get-ByteSum @([pscustomobject]@{ Name = 'x' })) 0 'byte sum ignores items without Length'
Assert-Equal (Get-ByteSum @([pscustomobject]@{ Length = 5 }, [pscustomobject]@{ Length = 7 })) 12 'byte sum adds Length'
Assert-Equal (Get-ByteSum ([pscustomobject]@{ Length = 5 })) 5 'byte sum of one scalar item'

if ($script:failures) { Write-Host "$script:failures failure(s)"; exit 1 }
Write-Host 'all passed'
