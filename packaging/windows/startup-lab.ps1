<#
.SYNOPSIS
  Startup experiment for the installed app (PowerShell 7): which environment variant removes the one-time
  first-launch cost? TEMPORARY, report-only; see packaging/windows/README.md ("Startup lab").

.DESCRIPTION
  Installs the setup exe silently, then launches impro.exe several times, each in a fresh process, and prints
  a table (also appended to $GITHUB_STEP_SUMMARY) with first-paint-ms and app-gtk-startup-ms per variant.
  Run it BEFORE any other launch on the machine so that run 1 is the true cold start. The real user profile is
  used on purpose (fontconfig and friends cache there), except for the last row.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Artifacts,   # directory with impro-<version>-x64-setup.exe
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$OutDir,      # logs and cache listings (uploaded as artifact)
    [int]$LaunchTimeoutSec = 90
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'smoke-args.ps1')
. (Join-Path $PSScriptRoot 'lab-helpers.ps1')

$Artifacts = (Resolve-Path -LiteralPath $Artifacts).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path -LiteralPath $OutDir).Path
$Work = Join-Path ([IO.Path]::GetTempPath()) ("impro-lab-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force -Path $Work | Out-Null

function Step([string]$msg) { Write-Host "[lab] $msg" }

function Get-DirSnapshot {
    # One line per top-level entry: size and file count. Missing directory => one line saying so.
    param([string]$Dir)
    if (-not (Test-Path -LiteralPath $Dir)) { return @("$Dir : (does not exist)") }
    $lines = @("$Dir :")
    foreach ($e in Get-ChildItem -LiteralPath $Dir -Force -ErrorAction SilentlyContinue | Sort-Object Name) {
        $files = if ($e.PSIsContainer) { @(Get-ChildItem -LiteralPath $e.FullName -Recurse -File -Force -ErrorAction SilentlyContinue) } else { @($e) }
        $bytes = ($files | Measure-Object Length -Sum).Sum
        $lines += ('  {0,-40} {1,10:N0} bytes {2,6} files' -f $e.Name, $(if ($bytes) { $bytes } else { 0 }), $files.Count)
    }
    $lines
}

function Save-CacheSnapshot {
    param([string]$When, [string]$AppDir)
    $lines = @("# $When $(Get-Date -Format o)")
    $lines += Get-DirSnapshot $env:LOCALAPPDATA
    $lines += Get-DirSnapshot (Join-Path $env:LOCALAPPDATA 'fontconfig')
    $lines += Get-DirSnapshot $env:TEMP
    foreach ($rel in 'var\cache', 'etc\fonts', 'share\fontconfig') { $lines += Get-DirSnapshot (Join-Path $AppDir $rel) }
    $fcCache = Join-Path $AppDir 'fc-cache.exe'
    if (Test-Path -LiteralPath $fcCache) {
        $lines += '> fc-cache -v (cache directories fontconfig would use, first 40 lines)'
        $lines += Limit-Lines ((& $fcCache -v 2>&1 | Out-String)) 40
    } else {
        $lines += 'fc-cache.exe is not in the bundle: only %LOCALAPPDATA% and %TEMP% growth is listed'
    }
    $file = Join-Path $OutDir ("cache-" + ($When -replace '\W+', '-') + '.txt')
    Set-Content -LiteralPath $file -Value $lines
    Step "cache snapshot '$When' written to $file"
    $lines
}

function Invoke-Launch {
    param([string]$Exe, [string]$Folder, [string]$Label, [hashtable]$EnvVars = @{}, [int]$KeepLines = 0)
    $slug = ($Label -replace '\W+', '-').Trim('-').ToLowerInvariant()
    $png = Join-Path $Work "$slug.png"
    $psi = [System.Diagnostics.ProcessStartInfo]::new($Exe)
    $psi.ArgumentList.Add($Folder)
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.Environment['IMPRO_QA_SCREENSHOT'] = $png
    $psi.Environment['IMPRO_QA_TIMING'] = '1'
    foreach ($k in $EnvVars.Keys) { $psi.Environment[$k] = [string]$EnvVars[$k] }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = [Diagnostics.Process]::Start($psi)
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    $timedOut = -not $p.WaitForExit($LaunchTimeoutSec * 1000)
    if ($timedOut) { try { $p.Kill($true) } catch { Write-Host "[lab] kill failed: $_" }; $p.WaitForExit(5000) | Out-Null } else { $p.WaitForExit() }
    [void]$out.Wait(5000); [void]$err.Wait(5000)
    $stdout = if ($out.IsCompleted) { $out.Result } else { '' }
    $stderr = if ($err.IsCompleted) { $err.Result } else { '' }
    $wall = $sw.Elapsed.TotalMilliseconds
    $text = $stdout + "`n" + $stderr
    Set-Content -LiteralPath (Join-Path $OutDir "lab-$slug.stdout.txt") -Value $stdout -NoNewline
    Set-Content -LiteralPath (Join-Path $OutDir "lab-$slug.stderr.txt") -Value $stderr -NoNewline
    if ($KeepLines -gt 0) {
        Write-Host "::group::$Label (first $KeepLines lines of stdout+stderr)"
        Write-Host (Limit-Lines $text $KeepLines)
        Write-Host '::endgroup::'
        Set-Content -LiteralPath (Join-Path $OutDir "lab-$slug.first-lines.txt") -Value (Limit-Lines $text $KeepLines)
    }
    $code = if ($timedOut) { -1 } else { $p.ExitCode }
    $row = [pscustomobject]@{
        Label = $Label; FirstPaint = Get-TimingValue $text 'first-paint-ms'; GtkStartup = Get-TimingValue $text 'app-gtk-startup-ms'
        ExitCode = $code; WallMs = $wall
    }
    Step ("{0}: first-paint-ms={1} app-gtk-startup-ms={2} exit={3} wall={4:N0}ms" -f $Label, (Format-Millis $row.FirstPaint), (Format-Millis $row.GtkStartup), $code, $wall)
    $row
}

$rows = @()
$setupExit = $null
$dir = Join-Path $Work 'install dir'
try {
    $setup = Join-Path $Artifacts "impro-$Version-x64-setup.exe"
    if (-not (Test-Path -LiteralPath $setup)) { throw "missing setup exe: $setup" }
    $p = Start-Process -FilePath $setup -PassThru -Wait -ArgumentList (New-InnoArgumentList -Dir $dir -Log (Join-Path $OutDir 'lab-install.log') -Tasks '')
    $setupExit = $p.ExitCode
    if ($setupExit -ne 0) { throw "installer exited $setupExit" }
    $exe = Join-Path $dir 'impro.exe'
    $folder = Join-Path $Work 'project'
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    Set-Content -LiteralPath (Join-Path $folder 'hello.txt') -Value 'hello'

    $before = Save-CacheSnapshot 'before first launch' $dir

    # 1 and 2: the very first launch on the machine, then the same again.
    $rows += Invoke-Launch $exe $folder 'default (cold)'
    $after = Save-CacheSnapshot 'after first launch' $dir
    $rows += Invoke-Launch $exe $folder 'default (warm)'

    # 3 to 6: one variable per run, always after the warm run so none of them pays first-run costs.
    $rows += Invoke-Launch $exe $folder 'PANGOCAIRO_BACKEND=win32' @{ PANGOCAIRO_BACKEND = 'win32' }
    $rows += Invoke-Launch $exe $folder 'DBUS_SESSION_BUS_ADDRESS=disabled:' @{ DBUS_SESSION_BUS_ADDRESS = 'disabled:' }
    $rows += Invoke-Launch $exe $folder 'GSETTINGS_BACKEND=memory' @{ GSETTINGS_BACKEND = 'memory' }
    $rows += Invoke-Launch $exe $folder 'FC_DEBUG=1 (first 40 lines kept)' @{ FC_DEBUG = '1' } -KeepLines 40

    # 7 (cache listings above) is complemented by a fresh profile: smoke.ps1 gives every launch one, so this
    # shows whether a per-profile cache (under LOCALAPPDATA/APPDATA) is what the real smoke job pays for.
    $fresh = Join-Path $Work 'fresh-profile'
    New-Item -ItemType Directory -Force -Path "$fresh\AppData\Roaming", "$fresh\AppData\Local" | Out-Null
    $rows += Invoke-Launch $exe $folder 'default, fresh profile dirs' @{
        USERPROFILE = $fresh; HOME = $fresh; APPDATA = "$fresh\AppData\Roaming"; LOCALAPPDATA = "$fresh\AppData\Local" }
    # Last: the default again, to see whether the machine drifted while the variants ran.
    $rows += Invoke-Launch $exe $folder 'default (warm, repeated at the end)'
} catch {
    Write-Host "::warning::startup lab stopped early: $($_.Exception.Message)"
} finally {
    $uninstaller = if (Test-Path -LiteralPath $dir) { Get-ChildItem -LiteralPath $dir -Filter 'unins*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1 }
    if ($uninstaller) {
        $p = Start-Process -FilePath $uninstaller.FullName -ArgumentList (New-InnoArgumentList -Log (Join-Path $OutDir 'lab-uninstall.log')) -PassThru -Wait
        Step "uninstaller exit code $($p.ExitCode)"
    }
}

if ($rows.Count -gt 0) {
    $table = Format-LabTable $rows
    $summary = "### Startup lab (installed app, $Version)`n`n$table`n`n" +
        "Wall time per launch (process start to exit, ms): " + (($rows | ForEach-Object { '{0:N0}' -f $_.WallMs }) -join ' / ') + "`n"
    Write-Host $summary
    Set-Content -LiteralPath (Join-Path $OutDir 'startup-lab.md') -Value $summary
    if ($env:GITHUB_STEP_SUMMARY) { Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value $summary }
    if ($before -and $after) {
        Write-Host '::group::cache listing difference around the first launch (< before, > after)'
        Compare-Object $before $after | Where-Object { $_.InputObject -notmatch '^# ' } | ForEach-Object { Write-Host ("{0} {1}" -f $_.SideIndicator.Replace('<=', '<').Replace('=>', '>'), $_.InputObject) }
        Write-Host '::endgroup::'
    }
}
# Report-only: a lab that could not run is a warning above, never a failed job.
exit 0
