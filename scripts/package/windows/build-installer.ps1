<#
.SYNOPSIS
  Build the Inno Setup installer from a bundle tree (PowerShell 7, Windows runner).

.EXAMPLE
  scripts/package/windows/build-installer.ps1 -Version 0.1.0 -BundleDir dist\impro -OutDir dist\out
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$BundleDir,
    [Parameter(Mandatory)][string]$OutDir,
    [string]$InnoChocoVersion = '6.7.1'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Find-Iscc {
    $candidates = @(
        (Get-Command ISCC.exe -ErrorAction SilentlyContinue | ForEach-Object Source),
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )
    $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
}

$iscc = Find-Iscc
if (-not $iscc) {
    Write-Host "ISCC.exe not on the image, installing innosetup $InnoChocoVersion with Chocolatey"
    choco install innosetup --version $InnoChocoVersion --no-progress -y
    Write-Host "choco exit code: $LASTEXITCODE"
    $iscc = Find-Iscc
}
if (-not $iscc) { throw 'ISCC.exe not found after installation' }
Write-Host "ISCC: $iscc"
Write-Host "Inno Setup version: $((Get-Item -LiteralPath $iscc).VersionInfo.ProductVersion)"

$root = (Get-Location).Path
$bundle = (Resolve-Path -LiteralPath $BundleDir).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$out = (Resolve-Path -LiteralPath $OutDir).Path

$log = Join-Path $out 'iscc-output.txt'
& $iscc "/DMyAppVersion=$Version" "/DMyBundleDir=$bundle" "/DMyOutputDir=$out" (Join-Path $root 'packaging\windows\impro.iss') 2>&1 |
    Tee-Object -FilePath $log | Out-Null
$code = $LASTEXITCODE
Write-Host "ISCC exit code: $code"
Write-Host '--- ISCC output (last 40 lines)'
Get-Content -LiteralPath $log -Tail 40 | Write-Host
Remove-Item -LiteralPath $log
if ($code -ne 0) { throw "ISCC exited with $code" }

$setup = Join-Path $out "impro-$Version-x64-setup.exe"
if (-not (Test-Path -LiteralPath $setup)) { throw "installer not produced: $setup" }
Get-ChildItem -LiteralPath $out | Format-Table Name, Length | Out-String | Write-Host
