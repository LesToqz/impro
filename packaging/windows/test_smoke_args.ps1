# Plain-assert tests for smoke-args.ps1 (PowerShell 7, any OS): pwsh -NoProfile -File packaging/windows/test_smoke_args.ps1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'smoke-args.ps1')

$script:failures = 0
function Assert-Equal($Actual, $Expected, [string]$What) {
    if ($Actual -ceq $Expected) { Write-Host "ok   $What" }
    else { Write-Host "FAIL $What`n  expected: $Expected`n  actual:   $Actual"; $script:failures++ }
}

Assert-Equal (ConvertTo-QuotedArgument 'plain') 'plain' 'plain value is untouched'
Assert-Equal (ConvertTo-QuotedArgument 'C:\a b\c') '"C:\a b\c"' 'space is quoted'
Assert-Equal (ConvertTo-QuotedArgument 'C:\a b\') '"C:\a b\\"' 'trailing backslash is doubled'
Assert-Equal (ConvertTo-QuotedArgument 'say "hi"') '"say \"hi\""' 'embedded quotes are escaped'
Assert-Equal (ConvertTo-QuotedArgument '') '""' 'empty value becomes ""'

$dir = 'C:\Users\x\Temp\impro-smoke-1\install dir'
$log = 'D:\a\_temp\smoke\install.log'
$a = New-InnoArgumentList -Dir $dir -Log $log -Tasks 'desktopicon,contextmenu,addtopath'
Assert-Equal ($a -join ' ') ('/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER /DIR="' + $dir + '" /LOG="' + $log + '" /TASKS=desktopicon,contextmenu,addtopath') 'installer command line'
Assert-Equal $a.Count 7 'installer argument count (the dir stays one element)'
$u = New-InnoArgumentList -Log 'D:\out dir\uninstall.log'
Assert-Equal ($u -join ' ') '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LOG="D:\out dir\uninstall.log"' 'uninstaller command line'
$t = New-InnoArgumentList -Dir 'C:\x y\' -Log $log
Assert-Equal $t[4] '/DIR="C:\x y"' 'trailing backslash of /DIR is dropped'

if ($script:failures) { Write-Host "$script:failures failure(s)"; exit 1 }
Write-Host 'all passed'
