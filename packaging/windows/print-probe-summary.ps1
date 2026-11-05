<#
.SYNOPSIS
  Print the window probe results and the smoke output files at the END of the job log (PowerShell 7).

.DESCRIPTION
  The job log of the smoke job is long and can only be read from its end. This is the last step of the job. It prints
  the names and sizes of the files in the smoke output directory, then the text of window-probe.txt (the
  `[window-probe]` lines and capture status lines that smoke.ps1 collected) framed by

    ===== WINDOW PROBE SUMMARY BEGIN =====
    ...
    ===== WINDOW PROBE SUMMARY END =====

  or, when the file does not exist, a clear line between the same markers. The framed text is the last thing printed.
  Best effort: the script always exits 0.

.EXAMPLE
  ./packaging/windows/print-probe-summary.ps1 -Directory "$env:RUNNER_TEMP\smoke"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Directory,
    [string]$Name = 'window-probe.txt'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$out = [System.Collections.Generic.List[string]]::new()
try {
    $out.Add('===== SMOKE OUTPUT FILES =====')
    if (Test-Path -LiteralPath $Directory -PathType Container) {
        $root = (Resolve-Path -LiteralPath $Directory).Path
        $files = @(Get-ChildItem -LiteralPath $Directory -Recurse -File -Force -ErrorAction SilentlyContinue | Sort-Object -Property FullName)
        $total = [long]0
        foreach ($f in $files) {
            $relative = [IO.Path]::GetRelativePath($root, $f.FullName)
            $out.Add(('{0,10}  {1}' -f $f.Length, $relative))
            $total += [long]$f.Length
        }
        $out.Add(('{0} file(s), {1} bytes in {2}' -f $files.Count, $total, $Directory))
    } else {
        $out.Add("the directory does not exist: $Directory")
    }

    $out.Add('===== WINDOW PROBE SUMMARY BEGIN =====')
    $file = Join-Path $Directory $Name
    if (Test-Path -LiteralPath $file -PathType Leaf) {
        $lines = @(Get-Content -LiteralPath $file)
        if ($lines.Count -eq 0) { $out.Add("($Name exists but is empty: the window probe printed nothing)") }
        foreach ($line in $lines) { $out.Add([string]$line) }
    } else {
        $out.Add("$Name is missing in ${Directory}: the installer smoke test did not reach the window probe")
    }
    $out.Add('===== WINDOW PROBE SUMMARY END =====')
} catch {
    $out.Add("the summary stopped early: $($_.Exception.Message)")
    $out.Add('===== WINDOW PROBE SUMMARY END =====')
}
Write-Host ($out -join "`n")
exit 0
