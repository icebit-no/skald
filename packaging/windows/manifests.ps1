<#
.SYNOPSIS
Fills the Scoop and winget manifests for a release from its version and the zip's SHA-256.

.DESCRIPTION
Writes target\package\manifests\scoop\skald.json, for icebit-no/scoop-bucket (D-14), and the three winget
manifests under target\package\manifests\winget\manifests\i\Icebit\Skald\<version>, laid out as winget-pkgs
keeps them. Nothing is published; the release pipeline (M6) takes them from here.

.PARAMETER Version
The release's version, as in the workspace's Cargo.toml.

.PARAMETER Sha256
The SHA-256 of skald-<version>-windows-x86_64.zip, as package.ps1 prints it.

.PARAMETER Url
Where the zip is downloaded from; by default the release's asset on GitHub.

.PARAMETER ReleaseDate
The release's date, yyyy-MM-dd; by default today.

.PARAMETER OutDir
Where the manifests go.

.EXAMPLE
pwsh packaging/windows/manifests.ps1 -Version 0.1.0 -Sha256 (Get-Content target\package\skald-0.1.0-windows-x86_64.zip.sha256).Split()[0]
#>
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Sha256,
    [string]$Url = "https://github.com/icebit-no/skald/releases/download/v$Version/skald-$Version-windows-x86_64.zip",
    [string]$ReleaseDate = (Get-Date -Format 'yyyy-MM-dd'),
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') { throw "«$Version» is not a version." }
if ($Sha256 -notmatch '^[0-9A-Fa-f]{64}$') { throw "«$Sha256» is not a SHA-256." }
if ($ReleaseDate -notmatch '^\d{4}-\d{2}-\d{2}$') { throw "«$ReleaseDate» is not a date (yyyy-MM-dd)." }
if (-not $Url.StartsWith('https://')) { throw "The zip's address must use https: $Url" }
if (-not $OutDir) {
    $OutDir = Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path 'target/package/manifests'
}

$values = [ordered]@{
    '{{version}}'          = $Version
    '{{url}}'              = $Url
    '{{sha256}}'           = $Sha256.ToLowerInvariant()
    '{{sha256_uppercase}}' = $Sha256.ToUpperInvariant()
    '{{release_date}}'     = $ReleaseDate
}

function Fill([string]$template, [string]$destination) {
    $text = Get-Content -Path $template -Raw -Encoding utf8
    foreach ($placeholder in $values.Keys) {
        $text = $text.Replace($placeholder, $values[$placeholder])
    }
    if ($text.Contains('{{')) { throw "$template has a placeholder this script does not fill." }
    New-Item -ItemType Directory -Force -Path (Split-Path $destination) | Out-Null
    # UTF-8 without a byte-order mark, as Scoop and winget-pkgs keep their manifests.
    [System.IO.File]::WriteAllText($destination, $text, [System.Text.UTF8Encoding]::new($false))
    Write-Output $destination
}

$scoop = Join-Path $OutDir 'scoop/skald.json'
Fill (Join-Path $PSScriptRoot 'scoop/skald.json') $scoop
Get-Content $scoop -Raw | ConvertFrom-Json | Out-Null

$winget = Join-Path $OutDir "winget/manifests/i/Icebit/Skald/$Version"
foreach ($template in Get-ChildItem (Join-Path $PSScriptRoot 'winget') -Filter 'Icebit.Skald*.yaml') {
    Fill $template.FullName (Join-Path $winget $template.Name)
}
