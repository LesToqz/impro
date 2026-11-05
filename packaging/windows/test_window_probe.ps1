# Plain-assert tests for the pure helpers of window-probe.ps1 and screenshot-log.ps1 (PowerShell 7, any OS):
#   pwsh -NoProfile -File packaging/windows/test_window_probe.ps1
# The expected values are in probe-vectors.json, shared with test_window_probe.py, which checks them against an
# independent implementation. Nothing here needs a window: the Win32 and COM parts are not exercised.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'window-probe.ps1')
. (Join-Path $PSScriptRoot 'screenshot-log.ps1')

$script:failures = 0
function Assert-Equal($Actual, $Expected, [string]$What) {
    if ($Actual -ceq $Expected) { Write-Host "ok   $What" }
    else { Write-Host "FAIL $What`n  expected: $Expected`n  actual:   $Actual"; $script:failures++ }
}

$vectors = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'probe-vectors.json') -Raw | ConvertFrom-Json

foreach ($case in @($vectors.colorref)) {
    Assert-Equal (ConvertFrom-ColorRef -Value ([long]$case.value)) ([string]$case.expect) "COLORREF $($case.value)"
}
foreach ($case in @($vectors.hex32)) {
    Assert-Equal (ConvertTo-Hex32 -Value ([long]$case.value)) ([string]$case.expect) "hex32 $($case.value)"
}
foreach ($case in @($vectors.pixel_color)) {
    Assert-Equal (Format-PixelColor -R ([int]$case.r) -G ([int]$case.g) -B ([int]$case.b)) ([string]$case.expect) "pixel colour $($case.r),$($case.g),$($case.b)"
}

foreach ($case in @($vectors.style_flags)) {
    $flags = Get-WindowStyleFlags -Style ([long]$case.style)
    $expected = $case.expect
    $names = @($expected.PSObject.Properties | ForEach-Object { $_.Name })
    $flagCount = @($flags.Keys).Count
    Assert-Equal $flagCount $names.Count "style $($case.style): number of flags"
    foreach ($name in $names) {
        Assert-Equal ([bool]$flags[$name]) ([bool]$expected.$name) "style $($case.style) $name"
    }
}

foreach ($case in @($vectors.scaled_offset)) {
    Assert-Equal (Get-ScaledOffset -Offset ([double]$case.offset) -Dpi ([int]$case.dpi)) ([int]$case.expect) "offset $($case.offset) at $($case.dpi) DPI"
}

foreach ($case in @($vectors.caption_height)) {
    $got = Get-CaptionHeight -NonClientTop ([int]$case.nonclient_top) -WindowHeight ([int]$case.window_height) -ClientHeight ([int]$case.client_height) -Dpi ([int]$case.dpi) -HasCaption ([bool]$case.has_caption)
    $label = "caption (caption=$($case.has_caption) nc=$($case.nonclient_top) win=$($case.window_height) client=$($case.client_height) dpi=$($case.dpi))"
    Assert-Equal $got.Height ([int]$case.height) "$label height"
    Assert-Equal $got.Source ([string]$case.source) "$label source"
}

foreach ($case in @($vectors.probe_points)) {
    $got = @(Get-ProbePoints -Width ([int]$case.width) -Height ([int]$case.height) -CaptionHeight ([int]$case.caption) -Dpi ([int]$case.dpi))
    $want = @($case.points)
    $label = "probe points $($case.width)x$($case.height) caption $($case.caption) dpi $($case.dpi)"
    Assert-Equal $got.Count $want.Count "$label count"
    for ($i = 0; $i -lt $want.Count; $i++) {
        $g = $got[$i]
        $w = $want[$i]
        Assert-Equal $g.Name ([string]$w.name) "$label point $i name"
        Assert-Equal $g.X ([int]$w.x) "$label $($w.name) x"
        Assert-Equal $g.Y ([int]$w.y) "$label $($w.name) y"
        Assert-Equal $g.InRange ([bool]$w.inrange) "$label $($w.name) in range"
    }
}

foreach ($case in @($vectors.dpi_awareness_name)) {
    Assert-Equal (Get-DpiAwarenessName -Value ([long]$case.value)) ([string]$case.expect) "DPI awareness $($case.value)"
}
foreach ($case in @($vectors.corner_preference_name)) {
    Assert-Equal (Get-CornerPreferenceName -Value ([long]$case.value)) ([string]$case.expect) "corner preference $($case.value)"
}
foreach ($case in @($vectors.dwm_value)) {
    Assert-Equal (Format-DwmValue -Kind ([string]$case.kind) -Value ([long]$case.value)) ([string]$case.expect) "DWM $($case.kind) $($case.value)"
}
foreach ($case in @($vectors.one_line)) {
    $shown = ([string]$case.text) -replace '[\r\n]', '~'
    Assert-Equal (Get-OneLine -Text ([string]$case.text)) ([string]$case.expect) "one line of '$shown'"
}
foreach ($case in @($vectors.format_line)) {
    Assert-Equal (Format-ProbeLine -Key ([string]$case.key) -Value ([string]$case.value)) ([string]$case.expect) "probe line $($case.key)"
}
Assert-Equal (Format-ProbeLine -Key 'k') '[window-probe] k=' 'a missing value prints an empty value'
Assert-Equal (ConvertTo-BoolText $true) 'true' 'true is lower case'
Assert-Equal (ConvertTo-BoolText $false) 'false' 'false is lower case'

# The printed line goes to the information stream: Write-ProbeLine must emit exactly the formatted line.
$printed = @(Write-ProbeLine -Key 'pid' -Value '42' 6>&1 | ForEach-Object { "$_" })
Assert-Equal $printed.Count 1 'Write-ProbeLine prints one line'
$firstPrinted = $printed[0]
Assert-Equal $firstPrinted '[window-probe] pid=42' 'Write-ProbeLine prints the formatted line'

# The probe's fact runners: a failing block becomes key=error(...), a good one key=value, and nothing throws.
$failing = @(Invoke-ProbeFact -Key 'boom' -Get { throw 'it broke' } 6>&1 | ForEach-Object { "$_" })
$firstFailing = $failing[0]
Assert-Equal $firstFailing '[window-probe] boom=error(it broke)' 'a throwing fact is printed as error(...)'
$fine = @(Invoke-ProbeFact -Key 'two' -Get { 1 + 1 } 6>&1 | ForEach-Object { "$_" })
$firstFine = $fine[0]
Assert-Equal $firstFine '[window-probe] two=2' 'a fact prints its value'
Assert-Equal (Get-ProbeSilently -Get { throw 'nope' }) $null 'a silent probe that throws gives $null'
Assert-Equal (Get-ProbeSilently -Get { 7 }) 7 'a silent probe gives the value'
$array = Get-ProbeSilently -Get { [int[]](1, 2, 3, 4) }
Assert-Equal $array.Count 4 'a silent probe keeps an array an array'

# --- helpers of print-screenshots.ps1 (screenshot-log.ps1)
foreach ($case in @($vectors.base64_length)) {
    Assert-Equal (Get-Base64Length -ByteCount ([long]$case.bytes)) ([long]$case.expect) "base64 length of $($case.bytes) bytes"
    if ($case.bytes -lt 2000) {
        Assert-Equal (Get-Base64Length -ByteCount ([long]$case.bytes)) ([Convert]::ToBase64String([byte[]]::new([int]$case.bytes)).Length) "base64 length of $($case.bytes) bytes matches .NET"
    }
}
foreach ($case in @($vectors.select_screenshots)) {
    $got = @(Select-ScreenshotNames -Names ([string[]]@($case.names)))
    $want = @($case.expect)
    Assert-Equal ($got -join '|') ($want -join '|') "screenshots selected from: $(@($case.names) -join ', ')"
    Assert-Equal $got.Count $want.Count "screenshots selected: count for $(@($case.names).Count) names"
}

if ($script:failures) { Write-Host "$script:failures failure(s)"; exit 1 }
Write-Host 'all passed'
