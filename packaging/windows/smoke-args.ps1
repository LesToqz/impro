# Pure helpers for building native command lines (dot-sourced by smoke.ps1, tested by test_smoke_args.ps1).
#
# Start-Process -ArgumentList joins its elements with single spaces and adds NO quoting, so a value that
# contains a space (for example the deliberate "install dir" of the smoke test) must be quoted by hand.

function ConvertTo-QuotedArgument {
    # Quote one argument following the CommandLineToArgvW rules when it needs it.
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Value)
    if ($Value -ne '' -and $Value -notmatch '[\s"]') { return $Value }
    $sb = [System.Text.StringBuilder]::new()
    [void]$sb.Append('"')
    $backslashes = 0
    foreach ($c in $Value.ToCharArray()) {
        if ($c -eq '\') { $backslashes++; continue }
        if ($c -eq '"') { [void]$sb.Append('\', 2 * $backslashes + 1); [void]$sb.Append('"') }
        else { [void]$sb.Append('\', $backslashes); [void]$sb.Append($c) }
        $backslashes = 0
    }
    [void]$sb.Append('\', 2 * $backslashes)
    [void]$sb.Append('"')
    $sb.ToString()
}

function New-InnoArgumentList {
    # Arguments for the Inno Setup installer or uninstaller. Path values are passed as /NAME="path".
    param([string]$Dir, [string]$Log, [string]$Tasks)
    $list = [System.Collections.Generic.List[string]]::new()
    foreach ($f in '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') { $list.Add($f) }
    if ($Dir) {
        $list.Add('/CURRENTUSER')
        # Inno rejects a trailing backslash inside quotes (it reads \" as an escaped quote).
        $list.Add('/DIR="' + $Dir.TrimEnd('\') + '"')
    }
    if ($Log) { $list.Add('/LOG="' + $Log + '"') }
    if ($Tasks) { $list.Add("/TASKS=$Tasks") }
    , $list.ToArray()
}
