# Status lines that go to the job log AND to a text file (dot-sourced by smoke.ps1, print-screenshots.ps1 and the tests,
# before window-capture.ps1 and window-probe.ps1, which call Write-LogLine).
#
# Why: the job log of the `smoke` job is long and can only be read from its end, so smoke.ps1 keeps every
# `[window-probe] ...` line and the desktop capture status lines in <OutDir>\window-probe.txt, and the last step of the
# job (print-probe-summary.ps1) prints that file again. The file holds exactly the text that was written to the host.
#
# The target is a GLOBAL variable (the functions of several dot-sourced files and a separate script all see it) and is
# empty by default: without Set-ProbeLogFile, Write-LogLine is just Write-Host. A global outlives the script that set it
# when scripts run in one PowerShell process (the test step of the `check` job runs several), so whoever sets it for
# a limited time must put the previous value back (print-screenshots.ps1 does, in a finally block), and tests start
# with Set-ProbeLogFile ''.
#
# Output streams: none of these functions writes to the success stream except Get-ProbeLogFile, which returns the path.

function Get-ProbeLogFile {
    # The current target ('' when none).
    $value = Get-Variable -Name ImproProbeLogFile -Scope Global -ValueOnly -ErrorAction SilentlyContinue
    if ($value) { [string]$value } else { '' }
}

function Set-ProbeLogFile {
    # Where Write-LogLine also appends its lines. An empty path switches the file off.
    param([AllowEmptyString()][string]$Path = '')
    $global:ImproProbeLogFile = $Path
}

function Write-LogLine {
    # Writes the text to the host and appends the same text to the log file, if one is set. Never throws and returns
    # nothing. A file that cannot be written (a stale path, a deleted directory) is reported ONCE and then switched off,
    # so that it cannot add a warning to every later line.
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Text)
    Write-Host $Text
    $target = Get-ProbeLogFile
    if ($target) {
        try {
            Add-Content -LiteralPath $target -Value $Text -Encoding utf8 -ErrorAction Stop
        } catch {
            $global:ImproProbeLogFile = ''
            Write-Host ('::warning::cannot append to ' + $target + ', no longer copying the lines to it: ' + $_.Exception.Message)
        }
    }
}
