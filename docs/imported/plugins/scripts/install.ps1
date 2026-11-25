<#
.SYNOPSIS
Install the FTS plugin bundles (.clap, .vst3) in this folder into the
standard Windows plugin folders, replacing older copies, and say what went
where. Shipped in the Windows release archive beside the bundles.

.DESCRIPTION
For you (default):   %LOCALAPPDATA%\Programs\Common\CLAP and ...\VST3
For everyone -System: %COMMONPROGRAMFILES%\CLAP and %COMMONPROGRAMFILES%\VST3
                      (run PowerShell as administrator)
These are the CLAP and VST3 specifications' standard locations.

A bundle is copied beside the old one, then swapped in. Only the exact
"<name>.clap" / "<name>.vst3" it installs is touched.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\install.ps1
powershell -ExecutionPolicy Bypass -File .\install.ps1 -System
powershell -ExecutionPolicy Bypass -File .\install.ps1 -DryRun
#>
param(
    [string]$From = $PSScriptRoot,
    [switch]$System,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

if ($System) {
    $base = $env:CommonProgramFiles
} else {
    $base = Join-Path $env:LOCALAPPDATA 'Programs\Common'
}
$dirs = @{ '.clap' = (Join-Path $base 'CLAP'); '.vst3' = (Join-Path $base 'VST3') }

$bundles = @(Get-ChildItem -LiteralPath $From | Where-Object { $_.Extension -in '.clap', '.vst3' })
if ($bundles.Count -eq 0) { throw "no .clap or .vst3 bundles in $From" }

if (-not $DryRun) {
    foreach ($d in $dirs.Values) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
}
$installed = 0; $replaced = 0
foreach ($b in $bundles) {
    $dir = $dirs[$b.Extension]
    $dest = Join-Path $dir $b.Name
    $what = if (Test-Path -LiteralPath $dest) { 'replace' } else { 'install' }
    if ($DryRun) {
        '  {0,-7} {1,-22} -> {2} (dry run)' -f $what, $b.Name, $dir
        continue
    }
    $tmp = Join-Path $dir ('.' + $b.Name + '.installing')
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
    Copy-Item -LiteralPath $b.FullName -Destination $tmp -Recurse
    if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Recurse -Force }
    Move-Item -LiteralPath $tmp -Destination $dest
    '  {0,-7} {1,-22} -> {2}' -f $what, $b.Name, $dir
    $installed++
    if ($what -eq 'replace') { $replaced++ }
}
if ($DryRun) {
    "dry run: $($bundles.Count) bundle(s) from $From, nothing changed"
} else {
    "installed $installed bundle(s) ($replaced replaced an older copy)"
    "  CLAP: $($dirs['.clap'])"
    "  VST3: $($dirs['.vst3'])"
    'Rescan plugins in your host to pick them up.'
}
