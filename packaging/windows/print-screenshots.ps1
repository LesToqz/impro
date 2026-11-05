<#
.SYNOPSIS
  Print the PNG files of a directory into the job log (PowerShell 7).

.DESCRIPTION
  The dev machine can read a GitHub Actions job log but cannot download artifacts, so every screenshot is written to
  the log as base64 between two exact marker lines:

    -----BEGIN IMPRO-SCREENSHOT <file name>-----
    <base64, wrapped at 100 columns>
    -----END IMPRO-SCREENSHOT <file name>-----

  scripts/ci/extract_log_screenshots.py turns a saved log back into PNG files. A file larger than -ScaleAboveKB is
  first downscaled to at most -MaxWidth pixels wide (saved as PNG again). Best effort: a file that cannot be read or
  scaled is a warning and is skipped, and the script always exits 0.

.EXAMPLE
  ./packaging/windows/print-screenshots.ps1 -Directory "$env:RUNNER_TEMP\smoke"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Directory,   # searched for *.png (not recursively unless -Recurse)
    [int]$MaxWidth = 1100,
    [int]$ScaleAboveKB = 250,
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
$printed = 0
foreach ($f in $files) {
    try {
        $bytes = Get-ScreenshotLogBytes -Path $f.FullName -MaxWidth $MaxWidth -ScaleAboveBytes ([long]$ScaleAboveKB * 1024)
        $block = Format-ScreenshotLogBlock -Name $f.Name -Bytes $bytes
        Write-Host "[screenshots] $($f.Name): $($f.Length) bytes on disk, $($bytes.Length) bytes printed"
        Write-Host ($block -join "`n")
        $printed++
    } catch {
        Write-Host "::warning::print-screenshots: skipped $($f.Name): $($_.Exception.Message)"
    }
}
Write-Host "[screenshots] printed $printed of $($files.Count) file(s)"
exit 0
