<#
.SYNOPSIS
Builds Skald for Windows in release mode and packs the zip that Scoop and winget install.

.DESCRIPTION
The zip holds skald.exe, transcribe.cpp's and ggml's DLLs beside it (the CPU modules and the Vulkan module,
loaded at run time; D-43), the user's README, the licence and, once it exists, THIRD-PARTY-NOTICES. Its files
sit at the top of the zip, with no folder around them. The script checks that the exe starts from the unpacked
zip alone and answers with the workspace's version, then writes the zip's SHA-256 beside it.

Needs Windows with the Visual Studio C++ build tools and the Windows SDK, CMake, Rust through rustup, and
LunarG's Vulkan SDK (packaging/windows/README.md).

.PARAMETER NoBuild
Packs what target\release already holds instead of building it.

.EXAMPLE
pwsh packaging/windows/package.ps1
#>
param(
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$release = Join-Path $root 'target\release'

$metadata = cargo metadata --manifest-path (Join-Path $root 'Cargo.toml') --no-deps --format-version 1 |
    ConvertFrom-Json
$version = ($metadata.packages | Where-Object name -EQ 'skald').version
$name = "skald-$version-windows-x86_64"

if (-not $NoBuild) {
    cargo build --manifest-path (Join-Path $root 'Cargo.toml') --release --locked -p skald
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with $LASTEXITCODE." }
}

$exe = Join-Path $release 'skald.exe'
if (-not (Test-Path $exe)) { throw "No $exe; build first or leave out -NoBuild." }
# The build stages every DLL transcribe.cpp installs next to the exe: transcribe, ggml and its base, one CPU
# module per instruction-set tier, and the Vulkan module.
$dlls = @(Get-ChildItem $release -Filter '*.dll' | Where-Object { $_.Name -match '^(transcribe|ggml)' })
foreach ($wanted in 'transcribe.dll', 'ggml.dll', 'ggml-base.dll', 'ggml-vulkan.dll') {
    if (-not ($dlls | Where-Object Name -EQ $wanted)) { throw "$wanted is missing from $release." }
}
if (-not ($dlls | Where-Object Name -Like 'ggml-cpu*.dll')) { throw "No CPU module in $release." }

$package = Join-Path $root "target\package\$name"
if (Test-Path $package) { Remove-Item $package -Recurse -Force }
New-Item -ItemType Directory -Path $package | Out-Null

Copy-Item $exe $package
$dlls | Copy-Item -Destination $package
Copy-Item (Join-Path $PSScriptRoot 'README.txt') $package
Copy-Item (Join-Path $root 'LICENSE') (Join-Path $package 'LICENSE.txt')
$notices = Join-Path $root 'THIRD-PARTY-NOTICES'
if (Test-Path $notices) {
    Copy-Item $notices (Join-Path $package 'THIRD-PARTY-NOTICES.txt')
} else {
    Write-Warning 'THIRD-PARTY-NOTICES does not exist yet; the zip goes without it.'
}

$zip = Join-Path $root "target\package\$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $package '*') -DestinationPath $zip

# The exe must start from the unpacked zip with nothing else to lean on: every DLL it needs is there.
$check = Join-Path ([System.IO.Path]::GetTempPath()) "$name-check"
if (Test-Path $check) { Remove-Item $check -Recurse -Force }
Expand-Archive $zip -DestinationPath $check
# skald.exe is a windowed program; piping its output makes PowerShell wait for it.
$answer = (& (Join-Path $check 'skald.exe') --version | Out-String).Trim()
Remove-Item $check -Recurse -Force
if ($answer -ne "skald $version") { throw "The unpacked skald.exe answered «$answer», not «skald $version»." }

$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -Path "$zip.sha256" -Value "$hash  $name.zip" -NoNewline
Write-Output "$zip"
Write-Output "SHA-256 $hash"
