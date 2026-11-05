<#
.SYNOPSIS
  Smoke test of the packaged impro for Windows (PowerShell 7).

.DESCRIPTION
  -Mode installer : two rounds.
                    1. "Next, Next, Finish": a silent install WITHOUT /TASKS (the task defaults of impro.iss), the
                       integration is asserted, the same setup runs again over it (upgrade in place: still one PATH
                       entry), then one silent uninstall must remove everything. A failure here is reported at the
                       end so that round 2 still produces its evidence.
                    2. A silent install with every task (/TASKS=desktopicon,contextmenu,filecontextmenu,addtopath),
                       the same assertions plus the desktop shortcut, the tests of the INSTALLED app, then a silent
                       uninstall: all files, shortcuts, registry keys and the PATH entry are gone, while a marker in
                       %APPDATA%\impro survives (the uninstaller never touches user settings).
                    Integration asserted after an install: Start menu shortcut (target = installed impro.exe,
                    working directory = user profile, AppUserModelID), App Paths key, Software\Classes\Applications
                    (open command and SupportedTypes), the folder / folder-background / file context-menu keys, the
                    user PATH, and the uninstall entry (name, version, publisher, help link).
  -Mode portable  : unzip the portable zip and test the app from there.

  The app test mirrors scripts/ci/smoke.sh: --version, --help, invalid folder (exit 2), then a GUI
  launch that writes a first-paint PNG (IMPRO_QA_SCREENSHOT) and exits 0, time-boxed. In installer mode a last
  launch WITHOUT the QA variables stays open until the real top-level window exists and is captured with its native
  frame into <OutDir>\desktop-window.png (best effort, never fails the run; see window-capture.ps1).

  Silent installer flags used (see smoke-args.ps1): /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER /DIR="..."
  /LOG="..." and, for round 2 only, /TASKS=...; the uninstaller gets /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LOG="...".
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

. (Join-Path $PSScriptRoot 'smoke-args.ps1')
. (Join-Path $PSScriptRoot 'lab-helpers.ps1')
. (Join-Path $PSScriptRoot 'window-capture.ps1')

$AppId = '{084215CE-9F22-4B49-9F5F-65835332B838}'
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\${AppId}_is1"
$UninstallSubKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $AppId + '_is1'
$ExpectedUsage = 'usage: impro [-h] [--version] [folder]'

# What impro.iss registers (all under HKCU for the per-user install the smoke test performs). packaging/windows/
# test_iss.py checks that every key named here is written by the script.
$FolderMenuSubKey = 'Software\Classes\Directory\shell\impro'
$BackgroundMenuSubKey = 'Software\Classes\Directory\Background\shell\impro'
$FileMenuSubKey = 'Software\Classes\*\shell\impro'
$AppPathsSubKey = 'Software\Microsoft\Windows\CurrentVersion\App Paths\impro.exe'
$ApplicationsSubKey = 'Software\Classes\Applications\impro.exe'
$ExpectedAppUserModelId = 'dev.impro.impro'   # impro_core::identity::APP_ID, set on the process by the app
$StartMenuLnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'impro.lnk'
$DesktopLnk = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'impro.lnk'
$script:Deferred = @()

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

    # Two launches in fresh processes: the first one pays the one-time first-run costs (font cache and so on),
    # the second one is warm. Both numbers are logged; every launch is finished and logged before anything fails.
    $launches = @()
    foreach ($run in 'cold', 'warm') {
        $png = Join-Path $OutDir "first-paint-$Mode-$run.png"
        Remove-Item -LiteralPath $png -ErrorAction SilentlyContinue
        $launchEnv = $envs + @{ IMPRO_QA_SCREENSHOT = $png; IMPRO_QA_TIMING = '1' }
        $r = Invoke-Capture -Exe $exe -Arguments @($folder) -TimeoutSec $LaunchTimeoutSec -EnvVars $launchEnv -Tag "launch-$Mode-$run"
        $text = $r.Stdout + "`n" + $r.Stderr
        $paint = Get-TimingValue $text 'first-paint-ms'
        $gtk = Get-TimingValue $text 'app-gtk-startup-ms'
        Step ("launch {0}: first-paint-ms={1} app-gtk-startup-ms={2} exit={3}" -f $run, (Format-Millis $paint), (Format-Millis $gtk), $r.ExitCode)
        $launches += [pscustomobject]@{ Run = $run; Result = $r; Png = $png; Text = $text }
    }
    foreach ($l in $launches) {
        if ($l.Result.TimedOut) { Fail "$($l.Run) launch did not exit within ${LaunchTimeoutSec}s" }
        if ($l.Result.ExitCode -ne 0) { Fail "$($l.Run) launch exited $($l.Result.ExitCode)" }
    }
    Pass 'both launches exit 0'
    # The screenshot verdict is the cold launch; the warm capture is only judged when the cold one fails,
    # so that one broken capture is reported together with the state of the other.
    $coldProblems = @(Test-ScreenshotProblems -Path $launches[0].Png)
    if ($coldProblems.Count -gt 0) {
        $warmProblems = @(Test-ScreenshotProblems -Path $launches[1].Png)
        $warmNote = if ($warmProblems.Count -gt 0) { 'warm launch: ' + ($warmProblems -join '; ') } else { 'warm launch screenshot is fine' }
        Fail ("cold launch: " + ($coldProblems -join '; ') + " | $warmNote")
    }
    Pass 'cold launch screenshot is a non-blank PNG'
    $all = ($launches | ForEach-Object { $_.Text }) -join "`n"
    $timing = ($launches[0].Text -split "`r?`n") | Where-Object { $_ -match 'first-paint-ms' } | Select-Object -First 1
    if ($timing) { Pass "first paint reported: $timing" } else { Write-Host '::warning::no first-paint-ms line in the app output (GUI-subsystem stdio may not reach the pipe)' }
    foreach ($bad in 'Unrecognized image file format', 'Settings schema .* is not installed', 'Failed to load module') {
        if ($all -match $bad) { Fail "runtime data problem in app output: matched '$bad'" }
    }
    if ($all -match 'WARNING') { Write-Host '::warning::app printed warnings, see the uploaded launch logs' }

    # Best effort, after every verdict above: the real desktop window with its native frame (the first-paint PNG
    # holds only the GTK content). Installer mode only; never fails the run.
    if ($Mode -eq 'installer') {
        $null = Save-DesktopWindowScreenshot -Exe $exe -Folder $folder -Path (Join-Path $OutDir 'desktop-window.png') -EnvVars $envs
    }
}

function Assert-InstalledLayout([string]$AppDir) {
    foreach ($rel in 'impro.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'bundle-manifest.txt', 'libgtk-3-0.dll',
                     'share\glib-2.0\schemas\gschemas.compiled', 'lib\gdk-pixbuf-2.0\2.10.0\loaders.cache',
                     'share\icons\Adwaita\index.theme', 'share\icons\hicolor\index.theme', 'licenses') {
        Assert-Path (Join-Path $AppDir $rel) "installed $rel"
    }
    foreach ($rel in $RequireFiles) { Assert-Path (Join-Path $AppDir $rel) "installed $rel (next to impro.exe)" }
}

# --- the integration impro.iss registers ------------------------------------------------------------------
# Registry values are read through .NET: the registry provider of PowerShell would treat the * in
# Software\Classes\*\shell as a wildcard.

function Test-UserRegKey([string]$SubKey) {
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey)
    if ($null -eq $k) { return $false }
    $k.Dispose()
    $true
}

function Get-UserRegValue([string]$SubKey, [string]$Name) {
    # The value ('' = the default value), or $null when the key or the value is missing. %VARS% stay unexpanded.
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey)
    if ($null -eq $k) { return $null }
    try { return $k.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $k.Dispose() }
}

function Assert-UserKey([string]$SubKey, [string]$What) {
    if (-not (Test-UserRegKey $SubKey)) { Fail "missing $What : HKCU\$SubKey" }
    Pass "$What present"
}

function Assert-Missing([string]$Path, [string]$What) {
    if (Test-Path -LiteralPath $Path) { Fail "unexpected $What : $Path" }
    Pass "$What absent"
}

function Assert-OpenCommand([string]$SubKey, [string]$ExpectedArg, [string]$What, [string]$Exe) {
    # The default value of the key must be: "<installed impro.exe>" "<ExpectedArg>"
    $cmd = [string](Get-UserRegValue $SubKey '')
    $m = [regex]::Match($cmd, '^"(?<exe>[^"]+)" "(?<arg>[^"]+)"$')
    if (-not $m.Success) { Fail "$What has an unexpected command line : $cmd" }
    $cmdExe = $m.Groups['exe'].Value
    $cmdArg = $m.Groups['arg'].Value
    if (-not (Test-SamePath $cmdExe $Exe)) { Fail "$What runs '$cmdExe', expected '$Exe'" }
    if ($cmdArg -cne $ExpectedArg) { Fail "$What passes '$cmdArg', expected '$ExpectedArg'" }
    Pass "$What runs the installed impro.exe with $ExpectedArg"
}

function Get-ShortcutInfo([string]$LnkPath) {
    $shell = New-Object -ComObject WScript.Shell
    try {
        $lnk = $shell.CreateShortcut($LnkPath)
        [pscustomobject]@{
            Target = [string]$lnk.TargetPath
            WorkingDirectory = [string]$lnk.WorkingDirectory
            Icon = [string]$lnk.IconLocation
        }
    } finally { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($shell) }
}

function Assert-Shortcut([string]$LnkPath, [string]$Exe, [string]$What) {
    Assert-Path $LnkPath $What
    $info = Get-ShortcutInfo $LnkPath
    $target = $info.Target
    $startIn = $info.WorkingDirectory
    $profileDir = $env:USERPROFILE
    Step ("{0} : target='{1}' workingDirectory='{2}' icon='{3}'" -f $What, $target, $startIn, $info.Icon)
    if (-not (Test-SamePath $target $Exe)) { Fail "$What targets '$target', expected the installed '$Exe'" }
    Pass "$What targets the installed impro.exe"
    if (-not (Test-SamePath $startIn $profileDir)) {
        Fail "$What starts in '$startIn', expected the user profile '$profileDir'"
    }
    Pass "$What starts in the user profile"
    # The AppUserModelID sits in the property store of the .lnk as UTF-16 text. This is a plausibility check on
    # the file bytes (no public API reads it back without a COM interop), so a miss is only a warning.
    $bytes = [IO.File]::ReadAllBytes($LnkPath)
    if (Test-ByteSequence $bytes ([Text.Encoding]::Unicode.GetBytes($ExpectedAppUserModelId))) {
        Pass "$What carries the AppUserModelID $ExpectedAppUserModelId"
    } else {
        Write-Host "::warning::$What : AppUserModelID $ExpectedAppUserModelId not found in the .lnk bytes"
    }
}

function Get-UserPathEntries {
    # The entries of the user PATH as stored (unexpanded), without empty items. Callers wrap the call in @(...).
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    if ($null -eq $k) { return @() }
    try { $raw = [string]$k.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $k.Dispose() }
    @($raw -split ';' | Where-Object { $_ -ne '' })
}

function Assert-UserPathCount([string]$Dir, [int]$Expected) {
    $hits = @(@(Get-UserPathEntries) | Where-Object { Test-SamePath $_ $Dir })
    if ($hits.Count -ne $Expected) { Fail "install dir is on the user PATH $($hits.Count) time(s), expected $Expected" }
    if ($Expected -eq 0) { Pass 'install dir is not on the user PATH' } else { Pass "install dir is on the user PATH exactly $Expected time(s)" }
}

function Assert-UninstallEntry([string]$Dir) {
    # What Settings > Apps shows: name, version, publisher, links, install location.
    $name = [string](Get-UserRegValue $UninstallSubKey 'DisplayName')
    $ver = [string](Get-UserRegValue $UninstallSubKey 'DisplayVersion')
    $pub = [string](Get-UserRegValue $UninstallSubKey 'Publisher')
    $help = [string](Get-UserRegValue $UninstallSubKey 'HelpLink')
    $loc = [string](Get-UserRegValue $UninstallSubKey 'InstallLocation')
    Step "uninstall entry: DisplayName='$name' DisplayVersion='$ver' Publisher='$pub' HelpLink='$help' InstallLocation='$loc'"
    if ($name -notlike 'impro*') { Fail "uninstall entry DisplayName is '$name'" }
    if ($ver -ne $Version) { Fail "uninstall entry DisplayVersion is '$ver', expected $Version" }
    if (-not $pub.Trim()) { Fail 'uninstall entry has no Publisher' }
    if ($help -notlike 'https://*') { Fail "uninstall entry HelpLink is '$help'" }
    if (-not (Test-SamePath $loc $Dir)) { Fail "uninstall entry InstallLocation is '$loc', expected '$Dir'" }
    Pass 'uninstall entry shows name, version, publisher, help link and install location'
}

function Assert-Integration {
    # Everything an install must have registered with the tasks that are on by default (folder and file context
    # menus, PATH) plus the desktop shortcut when $Desktop.
    param([Parameter(Mandatory)][string]$Dir, [Parameter(Mandatory)][bool]$Desktop)
    $exe = Join-Path $Dir 'impro.exe'
    Assert-Path $UninstallKey 'uninstall registry key'
    Assert-UninstallEntry $Dir
    Assert-Shortcut $StartMenuLnk $exe 'Start menu shortcut'
    if ($Desktop) {
        Assert-Shortcut $DesktopLnk $exe 'desktop shortcut'
    } else {
        Assert-Missing $DesktopLnk 'desktop shortcut (the task is off by default)'
    }

    Assert-UserKey $AppPathsSubKey 'App Paths key'
    if (-not (Test-SamePath ([string](Get-UserRegValue $AppPathsSubKey '')) $exe)) { Fail 'App Paths default value is not the installed impro.exe' }
    if (-not (Test-SamePath ([string](Get-UserRegValue $AppPathsSubKey 'Path')) $Dir)) { Fail 'App Paths "Path" value is not the install directory' }
    Pass 'App Paths points at the installed impro.exe and its directory'

    Assert-UserKey $ApplicationsSubKey 'Applications\impro.exe key (Open with)'
    if ([string](Get-UserRegValue $ApplicationsSubKey 'FriendlyAppName') -ne 'impro') { Fail 'Applications\impro.exe has no FriendlyAppName "impro"' }
    Assert-OpenCommand "$ApplicationsSubKey\shell\open\command" '%1' 'Open with command' $exe
    foreach ($ext in '.txt', '.md', '.rs', '.json', '.py') {
        if ($null -eq (Get-UserRegValue "$ApplicationsSubKey\SupportedTypes" $ext)) { Fail "SupportedTypes lacks $ext" }
    }
    Pass 'Open with lists impro for common text and source files (SupportedTypes)'

    Assert-OpenCommand "$FolderMenuSubKey\command" '%1' 'folder context menu' $exe
    Assert-OpenCommand "$BackgroundMenuSubKey\command" '%V' 'folder background context menu' $exe
    Assert-OpenCommand "$FileMenuSubKey\command" '%1' 'file context menu' $exe
    if ([string](Get-UserRegValue $FileMenuSubKey '') -ne 'Open with impro') { Fail 'file context menu entry is not "Open with impro"' }
    Assert-UserPathCount $Dir 1
}

function Assert-IntegrationGone {
    # After an uninstall: the directory, the uninstall entry, every key, both shortcuts and the PATH entry are gone.
    param([Parameter(Mandatory)][string]$Dir)
    Wait-Gone { -not (Test-Path -LiteralPath $Dir) } 'install dir'
    Show-Registry 'after uninstall'
    Wait-Gone { -not (Test-Path -LiteralPath $UninstallKey) } 'uninstall registry key'
    foreach ($sub in $FolderMenuSubKey, $BackgroundMenuSubKey, $FileMenuSubKey, $AppPathsSubKey, $ApplicationsSubKey) {
        Wait-Gone { -not (Test-UserRegKey $sub) } "registry key HKCU\$sub"
    }
    Wait-Gone { -not (Test-Path -LiteralPath $StartMenuLnk) } 'Start menu shortcut'
    Wait-Gone { -not (Test-Path -LiteralPath $DesktopLnk) } 'desktop shortcut'
    # The PATH entry is removed in the very last uninstall step, after the directory: poll for it.
    Wait-Gone { @(@(Get-UserPathEntries) | Where-Object { Test-SamePath $_ $Dir }).Count -eq 0 } 'install dir entry on the user PATH'
}

function Invoke-Install {
    param([Parameter(Mandatory)][string]$Setup, [Parameter(Mandatory)][string]$Dir, [string]$Tasks, [Parameter(Mandatory)][string]$LogName)
    $setupArgs = New-InnoArgumentList -Dir $Dir -Log (Join-Path $OutDir $LogName) -Tasks $Tasks
    Step "installing: $Setup $($setupArgs -join ' ')"
    $p = Start-Process -FilePath $Setup -ArgumentList $setupArgs -PassThru -Wait
    Step "installer exit code $($p.ExitCode)"
    if ($p.ExitCode -ne 0) { Fail "installer exited $($p.ExitCode), see $LogName" }
}

function Invoke-Uninstall {
    # Runs the uninstaller found in an install directory and waits until the directory is gone (the uninstaller
    # relaunches itself from %TEMP%, so -Wait returns early). $false when there is no uninstaller to run.
    param([Parameter(Mandatory)][string]$Dir, [Parameter(Mandatory)][string]$LogName, [int]$TimeoutSec = 60)
    $uninstaller = $null
    if (Test-Path -LiteralPath $Dir) {
        $uninstaller = Get-ChildItem -LiteralPath $Dir -Filter 'unins*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
    }
    if (-not $uninstaller) { return $false }
    Step "uninstalling with $($uninstaller.Name)"
    $p = Start-Process -FilePath $uninstaller.FullName -ArgumentList (New-InnoArgumentList -Log (Join-Path $OutDir $LogName)) -PassThru -Wait
    Step "uninstaller exit code $($p.ExitCode)"
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Test-Path -LiteralPath $Dir) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 1 }
    $true
}

function Invoke-Deferred([string]$Name, [scriptblock]$Body) {
    # Runs a group of checks whose failure must not hide the checks after it: the message is kept and the run
    # fails at the end.
    try { & $Body }
    catch {
        $msg = "$Name : $($_.Exception.Message)"
        Write-Host "::error::smoke ($Mode) deferred failure, $msg"
        Write-Host ($_ | Out-String)
        $script:Deferred += $msg
    }
}

function Test-DefaultInstallRoundTrip([string]$Setup) {
    # What a user gets by clicking through the wizard: no /TASKS, so the task defaults of impro.iss apply.
    $dir = Join-Path $Work 'default install'
    $script:AppDir = $dir
    try {
        Invoke-Install -Setup $Setup -Dir $dir -Tasks '' -LogName 'install-default.log'
        Show-Registry 'after default install'
        Assert-InstalledLayout $dir
        Assert-Integration -Dir $dir -Desktop $false
        # Same version over itself, same directory: an upgrade in place must not add a second PATH entry.
        Invoke-Install -Setup $Setup -Dir $dir -Tasks '' -LogName 'install-default-again.log'
        Assert-Integration -Dir $dir -Desktop $false
        Pass 'upgrade in place keeps a single PATH entry and all registrations'
    } finally {
        # Whatever happened above, leave nothing behind for round 2.
        try { [void](Invoke-Uninstall -Dir $dir -LogName 'uninstall-default.log') }
        catch { Write-Host "::warning::cleanup of the default install failed: $($_.Exception.Message)" }
    }
    # Only reached when the try block succeeded (otherwise its exception propagates after the finally block).
    Assert-IntegrationGone -Dir $dir
}

function Show-Tree([string]$Dir) {
    $files = @(Get-ChildItem -LiteralPath $Dir -Recurse -File -Force)
    $bytes = Get-ByteSum $files
    Step ("tree {0}: {1} files, {2:N1} MB, {3} DLLs next to the exe" -f $Dir, $files.Count, ($bytes / 1MB),
          @(Get-ChildItem -LiteralPath $Dir -Filter *.dll -File).Count)
    Get-ChildItem -LiteralPath $Dir -Force | Format-Table Mode, Length, Name -AutoSize | Out-String -Width 200 | Write-Host
}

function Show-Registry([string]$When) {
    Write-Host "::group::registry $When"
    foreach ($key in "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\${AppId}_is1",
                     'HKCU\Software\Classes\Directory\shell\impro',
                     'HKCU\Software\Classes\Directory\Background\shell\impro',
                     'HKCU\Software\Classes\*\shell\impro',
                     'HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\impro.exe',
                     'HKCU\Software\Classes\Applications\impro.exe') {
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

        # Round 1: the defaults of the wizard, installed twice (upgrade in place), removed once. Deferred, so that a
        # problem here is reported at the end and round 2 (the installed-app tests) still runs.
        Invoke-Deferred 'default tasks, upgrade in place, uninstall' { Test-DefaultInstallRoundTrip $setup }

        # Round 2: every task, then the tests of the installed app.
        $dir = Join-Path $Work 'install dir'
        Invoke-Install -Setup $setup -Dir $dir -Tasks 'desktopicon,contextmenu,filecontextmenu,addtopath' -LogName 'install.log'
        $script:AppDir = $dir
        Show-Tree $dir
        Show-Registry 'after install'
        Assert-InstalledLayout $dir
        Assert-Integration -Dir $dir -Desktop $true

        Test-App $dir

        # Settings under %APPDATA%\impro belong to the user: the uninstaller must leave them alone.
        $userData = Join-Path $env:APPDATA 'impro'
        $marker = Join-Path $userData 'uninstall-smoke-marker.txt'
        New-Item -ItemType Directory -Force -Path $userData | Out-Null
        Set-Content -LiteralPath $marker -Value 'the uninstaller must not delete this'

        if (-not (Invoke-Uninstall -Dir $dir -LogName 'uninstall.log')) { Fail 'uninstaller not found in install dir' }
        Assert-IntegrationGone -Dir $dir
        Assert-Path $marker 'settings folder %APPDATA%\impro (left alone by the uninstaller)'
        Remove-Item -LiteralPath $marker -Force
        if (@(Get-ChildItem -LiteralPath $userData -Force).Count -eq 0) { Remove-Item -LiteralPath $userData -Force }
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
    if ($script:Deferred.Count -gt 0) { Fail ("deferred check(s) failed: " + ($script:Deferred -join ' | ')) }
    Step 'ALL PASSED'
} catch {
    $failed = $true
    Write-Host "::error::smoke ($Mode) failed: $($_.Exception.Message)"
    Write-Host ($_ | Out-String)
    Collect-Evidence
}
if ($failed) { exit 1 }
exit 0
