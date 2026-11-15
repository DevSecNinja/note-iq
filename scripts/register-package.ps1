#Requires -Version 5.1
<#
.SYNOPSIS
Registers a Windows identity for a local NoteIQ build, so it can use on-device AI.

.DESCRIPTION
Windows only lets apps with a package identity use its on-device AI model. This script
registers an unsigned package with external location for the folder containing noteiq.exe.
Unsigned registration requires Developer Mode. NoteIQ itself stays where it is; rebuilding
it in place keeps the registration. Run with -Unregister to remove it.

.PARAMETER Executable
The noteiq.exe to register. Defaults to the native release build under target\.

.PARAMETER Unregister
Removes the registration. NoteIQ keeps working without on-device AI.
#>
[CmdletBinding()]
param(
    [string]$Executable,
    [switch]$Unregister
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$packageName = 'DevSecNinja.NoteIQ'
if ($env:OS -ne 'Windows_NT') {
    throw 'This script registers the Windows desktop app and must run on Windows.'
}
$root = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).ProviderPath

$existing = @(Get-AppxPackage -Name $packageName)
if ($Unregister) {
    if ($existing.Count -eq 0) {
        Write-Host 'NoteIQ has no Windows registration. Nothing to remove.'
        return
    }
    $existing | Remove-AppxPackage
    Write-Host 'Removed the NoteIQ Windows registration. Restart NoteIQ (Quit from the tray menu) to apply it.'
    return
}

$unlock = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue
if (-not $unlock -or $unlock.PSObject.Properties['AllowDevelopmentWithoutDevLicense'] -eq $null -or
    $unlock.AllowDevelopmentWithoutDevLicense -ne 1) {
    throw 'Registering an unsigned build requires Developer Mode. Turn it on in Settings > System > For developers, then retry.'
}

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($architecture) {
    'Arm64' { $target = 'aarch64-pc-windows-msvc' }
    'X64' { $target = 'x86_64-pc-windows-msvc' }
    default { throw "Unsupported Windows architecture: $architecture" }
}
if (-not $Executable) {
    $Executable = Join-Path $root "target\$target\release\noteiq.exe"
}
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    throw "NoteIQ was not found at $Executable. Build it first, for example with scripts\rebuild.ps1 -NoLaunch."
}
$Executable = (Resolve-Path -LiteralPath $Executable).ProviderPath
$folder = Split-Path -Parent $Executable
if ((Split-Path -Leaf $Executable) -ne 'noteiq.exe') {
    throw 'The executable must be named noteiq.exe, as the package manifest expects.'
}

$bytes = [IO.File]::ReadAllBytes($Executable)
$offset = [BitConverter]::ToInt32($bytes, 0x3c)
$machine = [BitConverter]::ToUInt16($bytes, $offset + 4)
$packageArchitecture = switch ($machine) {
    0xAA64 { 'arm64' }
    0x8664 { 'x64' }
    default { throw ('Unsupported executable architecture: 0x{0:X4}' -f $machine) }
}
$runtime = @(Get-AppxPackage -Name 'Microsoft.WindowsAppRuntime.2' |
        Where-Object { $_.Architecture.ToString() -eq $packageArchitecture })
if ($runtime.Count -eq 0) {
    throw "The Windows App Runtime 2 ($packageArchitecture) is not installed. Install it with: winget install Microsoft.WindowsAppRuntime.2"
}

$version = (Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version = "(\d+)\.(\d+)\.(\d+)"' |
        Select-Object -First 1).Matches[0].Groups
$packageVersion = '{0}.{1}.{2}.0' -f $version[1].Value, $version[2].Value, $version[3].Value

# The build script renders the app icon; the newest copy belongs to this build.
$logo = Get-ChildItem -Path (Join-Path $folder 'build') -Filter 'noteiq-64.png' -Recurse -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $logo) {
    throw "The NoteIQ icon was not found under $folder\build. Rebuild NoteIQ, then retry."
}

$staging = Join-Path $folder 'noteiq-package'
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'Assets') | Out-Null
Copy-Item -LiteralPath $logo.FullName -Destination (Join-Path $staging 'Assets\noteiq.png') -Force
$template = Get-Content -Raw -LiteralPath (Join-Path $root 'apps\noteiq-windows\package\AppxManifest.xml')
$manifest = $template.Replace('{{ARCH}}', $packageArchitecture).Replace('{{VERSION}}', $packageVersion)
$manifestPath = Join-Path $staging 'AppxManifest.xml'
[IO.File]::WriteAllText($manifestPath, $manifest, [Text.UTF8Encoding]::new($false))

if ($existing.Count -gt 0) {
    $existing | Remove-AppxPackage
}
Add-AppxPackage -Register $manifestPath -ExternalLocation $folder
$registered = Get-AppxPackage -Name $packageName
Write-Host "Registered $($registered.PackageFullName) for $folder."
Write-Host "Keep ${staging}: deleting it, for example with cargo clean, breaks the registration until you rerun this script."
$running = @(Get-Process -Name noteiq -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Executable })
if ($running.Count -gt 0) {
    Write-Host 'Restart NoteIQ (Quit from the tray menu, then start it again) so it runs with its new identity.'
}
