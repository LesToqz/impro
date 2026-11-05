<#
.SYNOPSIS
  Print the key screenshots of a directory into the job log (PowerShell 7).

.DESCRIPTION
  The dev machine can read a GitHub Actions job log but cannot download artifacts, so a screenshot is written to the
  log as base64 between two exact marker lines:

    -----BEGIN IMPRO-SCREENSHOT <file name>-----
    <base64, wrapped at 100 columns>
    -----END IMPRO-SCREENSHOT <file name>-----

  scripts/ci/extract_log_screenshots.py turns a saved log back into PNG files.

  The log stays small: only desktop-window.png (the real window) and ONE first-paint*.png (the first in name order)
  are considered. Each is downscaled to at most -MaxWidth pixels wide (saved as PNG again) and skipped when its base64
  text would still be longer than -MaxBase64KB kilobytes. Every file that is not printed gets one line,
  `[screenshots] skipped <name> (<size>)`. The structured facts of window-probe.ps1 are the primary evidence.

  Best effort: a file that cannot be read or scaled is a warning and is skipped, and the script always exits 0.

.EXAMPLE
  ./packaging/windows/print-screenshots.ps1 -Directory "$env:RUNNER_TEMP\smoke"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Directory,   # searched for *.png (not recursively unless -Recurse)
    [int]$MaxWidth = 700,
    [int]$MaxBase64KB = 60,
    [switch]$Recurse
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'screenshot-log.ps1')

if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
    Write-Host "::warning::print-screenshots: no such directory: $Directory"
    exit 0
}

$files = @(Get-ChildItem -LiteralPath $Directory -Filter '*.png' -File -Recurse:$Recurse | Sort-Object -Property FullName)
Write-Host "[screenshots] $($files.Count) PNG file(s) in $Directory"
$wanted = @(Select-ScreenshotNames -Names @($files | ForEach-Object { $_.Name }))
$capChars = [long]$MaxBase64KB * 1024
$printedNames = @{}
$printed = 0
foreach ($f in $files) {
    $nameKey = $f.Name.ToLowerInvariant()
    if (($wanted -notcontains $f.Name) -or $printedNames.ContainsKey($nameKey)) {
        Write-Host ('[screenshots] skipped {0} ({1} bytes)' -f $f.Name, $f.Length)
        continue
    }
    try {
        $bytes = Get-ScreenshotLogBytes -Path $f.FullName -MaxWidth $MaxWidth -ScaleAboveBytes 0
        $chars = Get-Base64Length -ByteCount $bytes.Length
        if ($chars -gt $capChars) {
            Write-Host ('[screenshots] skipped {0} ({1} bytes on disk, {2} bytes at most {3} px wide, base64 {4} chars is over the {5} limit)' -f $f.Name, $f.Length, $bytes.Length, $MaxWidth, $chars, $capChars)
            continue
        }
        $block = Format-ScreenshotLogBlock -Name $f.Name -Bytes $bytes
        Write-Host ('[screenshots] {0}: {1} bytes on disk, {2} bytes printed' -f $f.Name, $f.Length, $bytes.Length)
        Write-Host ($block -join "`n")
        $printedNames[$nameKey] = $true
        $printed++
    } catch {
        Write-Host "::warning::print-screenshots: skipped $($f.Name): $($_.Exception.Message)"
    }
}
Write-Host "[screenshots] printed $printed of $($files.Count) file(s)"
exit 0
