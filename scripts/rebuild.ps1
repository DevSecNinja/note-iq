#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$BuildDirectory,
    [switch]$Update,
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-NoteIqStopped {
    if (@(Get-Process -Name noteiq -ErrorAction SilentlyContinue).Count -gt 0) {
        throw 'NoteIQ is still running. Save your note and choose Quit from the tray menu, then retry. Closing the window only hides it.'
    }
}

if ($env:OS -ne 'Windows_NT') {
    throw 'This script builds the Windows desktop app and must run on Windows.'
}

$scriptRoot = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).ProviderPath
if ((Get-Location).Provider.Name -ne 'FileSystem') {
    throw 'Run this script from a filesystem directory.'
}
$currentDirectory = (Get-Location).ProviderPath
if (-not $BuildDirectory) {
    if ($currentDirectory -eq $scriptRoot) {
        $BuildDirectory = $scriptRoot
    } else {
        Write-Host "Current directory: $currentDirectory"
        Write-Host "Script checkout:   $scriptRoot"
        $choice = Read-Host 'Where should NoteIQ be built? Enter 1 for the current directory, 2 for the script checkout, or a directory path (blank cancels)'
        switch ($choice.Trim()) {
            '' { throw 'Build cancelled. No checkout was changed.' }
            '1' { $BuildDirectory = $currentDirectory }
            '2' { $BuildDirectory = $scriptRoot }
            default { $BuildDirectory = $choice.Trim() }
        }
    }
}
$resolvedDirectory = Resolve-Path -LiteralPath $BuildDirectory -ErrorAction Stop
if ($resolvedDirectory.Provider.Name -ne 'FileSystem' -or
    -not (Test-Path -LiteralPath $resolvedDirectory.ProviderPath -PathType Container)) {
    throw 'The build directory must be a filesystem directory.'
}
$root = $resolvedDirectory.ProviderPath
foreach ($manifest in @('Cargo.toml', 'apps\noteiq-windows\Cargo.toml', 'crates\noteiq-core\Cargo.toml')) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $manifest) -PathType Leaf)) {
        throw "Not a NoteIQ checkout root: $root (missing $manifest)."
    }
}
Write-Host "Build checkout: $root"
Push-Location -LiteralPath $root
try {
    Assert-NoteIqStopped
    $null = Get-Command cargo -CommandType Application -ErrorAction Stop
    if ($Update) {
        $null = Get-Command git -CommandType Application -ErrorAction Stop
        $branch = & git symbolic-ref --quiet --short HEAD
        if ($LASTEXITCODE -ne 0 -or $branch -ne 'main') {
            throw '-Update requires a checkout already on main. No branch switch was performed.'
        }
        $changes = & git status --porcelain
        if ($LASTEXITCODE -ne 0) {
            throw 'Could not check the working tree.'
        }
        if ($changes) {
            throw '-Update requires a clean working tree. No changes were stashed or discarded.'
        }
    }

    $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($architecture) {
        'Arm64' { $vcArchitecture = 'arm64'; $target = 'aarch64-pc-windows-msvc'; $machine = 0xAA64 }
        'X64' { $vcArchitecture = 'x64'; $target = 'x86_64-pc-windows-msvc'; $machine = 0x8664 }
        default { throw "Unsupported Windows architecture: $architecture" }
    }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
        throw 'Visual Studio Installer was not found. Install Visual Studio Build Tools with the matching C++ tools and Windows SDK.'
    }
    $component = if ($vcArchitecture -eq 'arm64') {
        'Microsoft.VisualStudio.Component.VC.Tools.ARM64'
    } else {
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64'
    }
    $installation = & $vswhere -latest -products '*' -requires $component -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $installation) {
        throw "Visual Studio C++ build tools for $vcArchitecture were not found."
    }
    $vcvars = Join-Path $installation 'VC\Auxiliary\Build\vcvarsall.bat'
    if (-not (Test-Path -LiteralPath $vcvars -PathType Leaf)) {
        throw "Visual Studio environment setup was not found: $vcvars"
    }

    if ($Update) {
        & git pull --ff-only origin main
        if ($LASTEXITCODE -ne 0) {
            throw 'Updating main failed. No build or launch was attempted.'
        }
    }
    Assert-NoteIqStopped
    $targetDirectory = Join-Path $root 'target'
    Write-Host "Building NoteIQ for $target..."
    # Keep environment setup and Cargo in one process; /d disables cmd AutoRun.
    & $env:ComSpec /d /s /c "call `"$vcvars`" $vcArchitecture >nul && cargo build --release --locked --features desktop -p noteiq-windows --target $target --target-dir `"$targetDirectory`""
    if ($LASTEXITCODE -ne 0) {
        throw 'NoteIQ build failed. The executable was not launched.'
    }
    $executable = Join-Path $targetDirectory "$target\release\noteiq.exe"
    $bytes = [IO.File]::ReadAllBytes($executable)
    if ($bytes.Length -lt 64 -or [BitConverter]::ToUInt16($bytes, 0) -ne 0x5A4D) {
        throw 'The build output is not a Windows executable.'
    }
    $offset = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($offset -lt 0 -or $offset -gt $bytes.Length - 94 -or
        [BitConverter]::ToUInt32($bytes, $offset) -ne 0x4550 -or
        [BitConverter]::ToUInt16($bytes, $offset + 4) -ne $machine -or
        [BitConverter]::ToUInt16($bytes, $offset + 92) -ne 2) {
        throw 'The build output must be a native-architecture Windows GUI executable, not a console executable.'
    }
    Write-Host "Built: $executable"
    if (-not $NoLaunch) {
        Assert-NoteIqStopped
        $process = Start-Process -FilePath $executable -WorkingDirectory $root -PassThru
        if ($process.WaitForExit(3000)) {
            throw "NoteIQ exited immediately (exit code $($process.ExitCode))."
        }
        $process.Refresh()
        if (-not $process.Responding) {
            throw 'NoteIQ started but is not responding. No process was terminated.'
        }
        Write-Host "NoteIQ started (PID $($process.Id))."
    }
} finally {
    Pop-Location
}
