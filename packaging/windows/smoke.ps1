<#
.SYNOPSIS
  Smoke test of the packaged impro for Windows (PowerShell 7).

.DESCRIPTION
  -Mode installer : silently install the setup exe, assert files/registry, test the INSTALLED app,
                    silently uninstall and assert everything is gone.
  -Mode portable  : unzip the portable zip and test the app from there.

  The app test mirrors scripts/ci/smoke.sh: --version, --help, invalid folder (exit 2), then a GUI
  launch that writes a first-paint PNG (IMPRO_QA_SCREENSHOT) and exits 0, time-boxed.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('installer', 'portable')][string]$Mode,
    [Parameter(Mandatory)][string]$Artifacts,   # directory with the setup exe, the zip and imports-report.txt
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$OutDir,      # PNG, logs (uploaded as artifact)
    [int]$LaunchTimeoutSec = 90,
    [int]$CommandTimeoutSec = 30,
    [string[]]$RequireFiles = @()               # extra files that must sit next to impro.exe (for example WebView2Loader.dll)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$AppId = '{084215CE-9F22-4B49-9F5F-65835332B838}'
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\${AppId}_is1"
$ExpectedUsage = 'usage: impro [-h] [--version] [folder]'

$Artifacts = (Resolve-Path -LiteralPath $Artifacts).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path -LiteralPath $OutDir).Path
$Work = Join-Path ([IO.Path]::GetTempPath()) ("impro-smoke-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$script:AppDir = $null
$script:LastLogs = @()

function Step([string]$msg) { Write-Host "[smoke] $msg" }
function Pass([string]$msg) { Write-Host "[smoke] PASS $msg" }
function Fail([string]$msg) { throw "FAIL $msg" }

function Invoke-Capture {
    param([string]$Exe, [string[]]$Arguments = @(), [int]$TimeoutSec = 30, [hashtable]$EnvVars = @{}, [string]$Tag = 'cmd')
    $psi = [System.Diagnostics.ProcessStartInfo]::new($Exe)
    foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    foreach ($k in $EnvVars.Keys) { $psi.Environment[$k] = [string]$EnvVars[$k] }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = [Diagnostics.Process]::Start($psi)
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    $timedOut = -not $p.WaitForExit($TimeoutSec * 1000)
    if ($timedOut) {
        Write-Host "[smoke] $Tag timed out after ${TimeoutSec}s, killing process tree"
        try { $p.Kill($true) } catch { Write-Host "[smoke] kill failed: $_" }
        $p.WaitForExit(5000) | Out-Null
    } else {
        $p.WaitForExit()
    }
    [void]$out.Wait(5000)
    [void]$err.Wait(5000)
    $stdout = if ($out.IsCompleted) { $out.Result } else { '' }
    $stderr = if ($err.IsCompleted) { $err.Result } else { '' }
    $outFile = Join-Path $OutDir "$Tag.stdout.txt"
    $errFile = Join-Path $OutDir "$Tag.stderr.txt"
    Set-Content -LiteralPath $outFile -Value $stdout -NoNewline
    Set-Content -LiteralPath $errFile -Value $stderr -NoNewline
    $script:LastLogs += @($outFile, $errFile)
    $code = if ($timedOut) { -1 } else { $p.ExitCode }
    Step ("{0}: exit={1} elapsed={2:N1}s stdout={3}B stderr={4}B" -f $Tag, $code, $sw.Elapsed.TotalSeconds, $stdout.Length, $stderr.Length)
    [pscustomobject]@{ ExitCode = $code; TimedOut = $timedOut; Stdout = $stdout; Stderr = $stderr }
}

function Assert-Path([string]$Path, [string]$What) {
    if (-not (Test-Path -LiteralPath $Path)) { Fail "missing $What : $Path" }
    Pass "$What present"
}

function Assert-Imports([string]$AppDir) {
    $report = Join-Path $Artifacts 'imports-report.txt'
    if (-not (Test-Path -LiteralPath $report)) { Step "no imports-report.txt, skipping import check"; return }
    $missing = @()
    foreach ($line in Get-Content -LiteralPath $report) {
        $parts = $line -split ' ', 2
        if ($parts.Count -ne 2) { continue }
        $dll = $parts[1].Trim()
        if ($dll -like 'api-ms-win-*' -or $dll -like 'ext-ms-win-*') { continue }
        if (Test-Path -LiteralPath (Join-Path $AppDir $dll)) { continue }
        if (Test-Path -LiteralPath (Join-Path $env:windir "System32\$dll")) { continue }
        $missing += "$($parts[0]) -> $dll"
    }
    if ($missing.Count -gt 0) { Fail ("unresolved DLL imports:`n  " + (($missing | Sort-Object -Unique) -join "`n  ")) }
    Pass "every imported DLL resolves in the app dir or System32"
}

function Test-App([string]$AppDir) {
    $exe = Join-Path $AppDir 'impro.exe'
    $script:AppDir = $AppDir
    Assert-Path $exe 'impro.exe'
    Assert-Imports $AppDir

    $home_dir = Join-Path $Work 'home'
    New-Item -ItemType Directory -Force -Path "$home_dir\AppData\Roaming", "$home_dir\AppData\Local" | Out-Null
    $folder = Join-Path $Work 'project'
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    Set-Content -LiteralPath (Join-Path $folder 'hello.txt') -Value 'hello'
    $envs = @{ USERPROFILE = $home_dir; HOME = $home_dir; APPDATA = "$home_dir\AppData\Roaming"; LOCALAPPDATA = "$home_dir\AppData\Local" }

    $r = Invoke-Capture -Exe $exe -Arguments @('--version') -TimeoutSec $CommandTimeoutSec -EnvVars $envs -Tag 'version'
    if ($r.TimedOut -or $r.ExitCode -ne 0) { Fail "--version exit code $($r.ExitCode)" }
    if (-not $r.Stdout.Trim()) { Fail "--version printed nothing on stdout (stderr: $($r.Stderr))" }
    if ($r.Stdout -notmatch [regex]::Escape($Version)) { Step "note: --version output '$($r.Stdout.Trim())' does not contain $Version" }
    Pass "--version: $($r.Stdout.Trim())"

    $r = Invoke-Capture -Exe $exe -Arguments @('--help') -TimeoutSec $CommandTimeoutSec -EnvVars $envs -Tag 'help'
    if ($r.TimedOut -or $r.ExitCode -ne 0) { Fail "--help exit code $($r.ExitCode)" }
    $first = ($r.Stdout -split "`r?`n")[0]
    if ($first -ne $ExpectedUsage) { Fail "--help first line was: '$first'" }
    Pass '--help usage line'

    $r = Invoke-Capture -Exe $exe -Arguments @((Join-Path $Work 'does-not-exist')) -TimeoutSec $CommandTimeoutSec -EnvVars $envs -Tag 'badfolder'
    if ($r.ExitCode -ne 2) { Fail "invalid folder exited $($r.ExitCode), expected 2" }
    Pass 'invalid folder exits 2'

    $png = Join-Path $OutDir "first-paint-$Mode.png"
    Remove-Item -LiteralPath $png -ErrorAction SilentlyContinue
    $launchEnv = $envs + @{ IMPRO_QA_SCREENSHOT = $png; IMPRO_QA_TIMING = '1' }
    $r = Invoke-Capture -Exe $exe -Arguments @($folder) -TimeoutSec $LaunchTimeoutSec -EnvVars $launchEnv -Tag "launch-$Mode"
    if ($r.TimedOut) { Fail "launch did not exit within ${LaunchTimeoutSec}s" }
    if ($r.ExitCode -ne 0) { Fail "launch exited $($r.ExitCode)" }
    Pass 'launch exits 0'
    if (-not (Test-Path -LiteralPath $png) -or (Get-Item -LiteralPath $png).Length -eq 0) { Fail "no screenshot written to $png" }
    $bytes = [IO.File]::ReadAllBytes($png)
    $sig = ($bytes[0..7] | ForEach-Object { $_.ToString('x2') }) -join ''
    if ($sig -ne '89504e470d0a1a0a') { Fail "screenshot is not a PNG (magic $sig)" }
    if ($bytes.Length -lt 5000) { Fail "screenshot is only $($bytes.Length) bytes, looks blank" }
    Pass "screenshot is a PNG of $($bytes.Length) bytes"

    $all = $r.Stdout + "`n" + $r.Stderr
    $timing = ($all -split "`r?`n") | Where-Object { $_ -match 'first-paint-ms' } | Select-Object -First 1
    if ($timing) { Pass "first paint reported: $timing" } else { Write-Host '::warning::no first-paint-ms line in the app output (GUI-subsystem stdio may not reach the pipe)' }
    foreach ($bad in 'Unrecognized image file format', 'Settings schema .* is not installed', 'Failed to load module') {
        if ($all -match $bad) { Fail "runtime data problem in app output: matched '$bad'" }
    }
    if ($all -match 'WARNING') { Write-Host '::warning::app printed warnings, see the uploaded launch logs' }
}

function Assert-InstalledLayout([string]$AppDir) {
    foreach ($rel in 'impro.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'bundle-manifest.txt', 'libgtk-3-0.dll',
                     'share\glib-2.0\schemas\gschemas.compiled', 'lib\gdk-pixbuf-2.0\2.10.0\loaders.cache',
                     'share\icons\Adwaita\index.theme', 'share\icons\hicolor\index.theme', 'licenses') {
        Assert-Path (Join-Path $AppDir $rel) "installed $rel"
    }
    foreach ($rel in $RequireFiles) { Assert-Path (Join-Path $AppDir $rel) "installed $rel (next to impro.exe)" }
}

function Show-Tree([string]$Dir) {
    $files = @(Get-ChildItem -LiteralPath $Dir -Recurse -File -Force)
    $bytes = ($files | Measure-Object Length -Sum).Sum
    Step ("tree {0}: {1} files, {2:N1} MB, {3} DLLs next to the exe" -f $Dir, $files.Count, ($bytes / 1MB),
          @(Get-ChildItem -LiteralPath $Dir -Filter *.dll -File).Count)
    Get-ChildItem -LiteralPath $Dir -Force | Format-Table Mode, Length, Name -AutoSize | Out-String -Width 200 | Write-Host
}

function Show-Registry([string]$When) {
    Write-Host "::group::registry $When"
    foreach ($key in "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\${AppId}_is1",
                     'HKCU\Software\Classes\Directory\shell\impro',
                     'HKCU\Software\Classes\Directory\Background\shell\impro') {
        Write-Host "> reg query $key /s"
        $text = (& reg.exe query $key /s 2>&1 | Out-String)
        Write-Host "  reg exit code $LASTEXITCODE"
        Write-Host $text
    }
    Write-Host ('> user PATH: ' + ((Get-ItemProperty -Path 'HKCU:\Environment' -Name Path -ErrorAction SilentlyContinue).Path))
    Write-Host '::endgroup::'
}

function Wait-Gone([scriptblock]$Gone, [string]$What, [int]$TimeoutSec = 60) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if (& $Gone) { Pass "$What removed"; return }
        Start-Sleep -Seconds 1
    }
    Fail "$What still present after ${TimeoutSec}s"
}

function Collect-Evidence {
    Write-Host '::group::crash evidence'
    foreach ($f in $script:LastLogs) {
        if (Test-Path -LiteralPath $f) {
            $c = Get-Content -LiteralPath $f -Raw
            if ($c) { Write-Host "--- $f"; Write-Host $c }
        }
    }
    try {
        Write-Host '--- Application event log (last 5 min, IDs 1000/1001)'
        Get-WinEvent -FilterHashtable @{ LogName = 'Application'; Id = 1000, 1001; StartTime = (Get-Date).AddMinutes(-5) } -ErrorAction Stop |
            Format-List TimeCreated, Id, ProviderName, Message | Out-String | Write-Host
    } catch { Write-Host "no matching events ($($_.Exception.Message))" }
    foreach ($wer in "$env:LOCALAPPDATA\Microsoft\Windows\WER", "$env:ProgramData\Microsoft\Windows\WER") {
        if (Test-Path -LiteralPath $wer) {
            Write-Host "--- WER $wer"
            Get-ChildItem -LiteralPath $wer -Recurse -Filter '*.wer' -ErrorAction SilentlyContinue |
                Where-Object { $_.LastWriteTime -gt (Get-Date).AddMinutes(-10) } |
                ForEach-Object { Write-Host "# $($_.FullName)"; Get-Content -LiteralPath $_.FullName -ErrorAction SilentlyContinue | Select-Object -First 40 }
        }
    }
    if ($script:AppDir -and (Test-Path -LiteralPath $script:AppDir)) {
        Write-Host "--- DLL lookup for impro.exe imports (app dir first, then PATH)"
        $report = Join-Path $Artifacts 'imports-report.txt'
        if (Test-Path -LiteralPath $report) {
            $dlls = Get-Content -LiteralPath $report | Where-Object { $_ -like 'impro.exe *' } | ForEach-Object { ($_ -split ' ', 2)[1] }
            foreach ($d in $dlls) {
                $here = Test-Path -LiteralPath (Join-Path $script:AppDir $d)
                $sys = (& where.exe $d 2>$null | Select-Object -First 1)
                Write-Host ("{0,-40} app-dir={1} where={2}" -f $d, $here, $sys)
            }
        }
        Get-ChildItem -LiteralPath $script:AppDir -Filter *.dll | Measure-Object | ForEach-Object { Write-Host "app dir holds $($_.Count) DLLs" }
    }
    Write-Host '::endgroup::'
    if ($Mode -eq 'installer') { Show-Registry 'at failure' }
}

$failed = $false
try {
    Step "mode=$Mode version=$Version work=$Work"
    if ($Mode -eq 'installer') {
        $setup = Join-Path $Artifacts "impro-$Version-x64-setup.exe"
        Assert-Path $setup 'setup exe'
        $dir = Join-Path $Work 'install dir'
        $log = Join-Path $OutDir 'install.log'
        $setupArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', "/DIR=$dir", "/LOG=$log",
                  '/TASKS=desktopicon,contextmenu,addtopath')
        Step "installing: $setup $($setupArgs -join ' ')"
        $p = Start-Process -FilePath $setup -ArgumentList $setupArgs -PassThru -Wait
        Step "installer exit code $($p.ExitCode)"
        if ($p.ExitCode -ne 0) { Fail "installer exited $($p.ExitCode), see install.log" }
        $script:AppDir = $dir
        Show-Tree $dir
        Show-Registry 'after install'
        Assert-InstalledLayout $dir
        Assert-Path $UninstallKey 'uninstall registry key'
        Assert-Path (Join-Path ([Environment]::GetFolderPath('Programs')) 'impro.lnk') 'Start menu shortcut'
        Assert-Path (Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'impro.lnk') 'desktop shortcut'
        Assert-Path 'HKCU:\Software\Classes\Directory\shell\impro\command' 'folder context-menu entry'
        Assert-Path 'HKCU:\Software\Classes\Directory\Background\shell\impro\command' 'background context-menu entry'
        $userPath = (Get-ItemProperty -Path 'HKCU:\Environment' -Name Path).Path
        if (($userPath -split ';') -notcontains $dir) { Fail "install dir missing from the user PATH" }
        Pass 'install dir is on the user PATH'

        Test-App $dir

        $uninstaller = Get-ChildItem -LiteralPath $dir -Filter 'unins*.exe' | Select-Object -First 1
        if (-not $uninstaller) { Fail 'uninstaller not found in install dir' }
        Step "uninstalling with $($uninstaller.Name)"
        $ulog = Join-Path $OutDir 'uninstall.log'
        $p = Start-Process -FilePath $uninstaller.FullName -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=$ulog") -PassThru -Wait
        Step "uninstaller exit code $($p.ExitCode)"
        Wait-Gone { -not (Test-Path -LiteralPath $dir) } 'install dir'
        Show-Registry 'after uninstall'
        Wait-Gone { -not (Test-Path -LiteralPath $UninstallKey) } 'uninstall registry key'
        Wait-Gone { -not (Test-Path 'HKCU:\Software\Classes\Directory\shell\impro') } 'folder context-menu key'
        Wait-Gone { -not (Test-Path 'HKCU:\Software\Classes\Directory\Background\shell\impro') } 'background context-menu key'
        Wait-Gone { -not (Test-Path (Join-Path ([Environment]::GetFolderPath('Programs')) 'impro.lnk')) } 'Start menu shortcut'
        Wait-Gone { -not (Test-Path (Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'impro.lnk')) } 'desktop shortcut'
        $userPath = (Get-ItemProperty -Path 'HKCU:\Environment' -Name Path).Path
        if (($userPath -split ';') -contains $dir) { Fail 'install dir still on the user PATH after uninstall' }
        Pass 'install dir removed from the user PATH'
    } else {
        $zip = Join-Path $Artifacts "impro-$Version-x64-portable.zip"
        Assert-Path $zip 'portable zip'
        $dest = Join-Path $Work 'portable'
        Expand-Archive -LiteralPath $zip -DestinationPath $dest -Force
        $exe = Get-ChildItem -LiteralPath $dest -Recurse -Filter 'impro.exe' | Select-Object -First 1
        if (-not $exe) { Fail 'impro.exe not found in the portable zip' }
        Assert-InstalledLayout $exe.DirectoryName
        Test-App $exe.DirectoryName
    }
    Step 'ALL PASSED'
} catch {
    $failed = $true
    Write-Host "::error::smoke ($Mode) failed: $($_.Exception.Message)"
    Write-Host ($_ | Out-String)
    Collect-Evidence
}
if ($failed) { exit 1 }
exit 0
