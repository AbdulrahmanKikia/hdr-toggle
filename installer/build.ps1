<#
.SYNOPSIS
    Builds the release exe and the Inno Setup installer.

.DESCRIPTION
    Runs `cargo build --release --locked`, reads the version from Cargo.toml, and compiles
    installer\hdr_toggle.iss with Inno Setup 6. The installer is written to
    installer\Output\hdr_toggle-<version>-setup.exe.

    Used both locally and by .github/workflows/release.yml.
#>
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit code $LASTEXITCODE)" }

    # The [package] version is the first top-level `version = "..."` line in Cargo.toml.
    $match = Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $match) { throw 'could not find the package version in Cargo.toml' }
    $version = $match.Matches[0].Groups[1].Value

    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    if (-not [IO.Path]::IsPathRooted($targetDir)) { $targetDir = Join-Path $root $targetDir }
    $exe = Join-Path $targetDir 'release\hdr_toggle.exe'
    if (-not (Test-Path $exe)) { throw "built exe not found at $exe" }

    $iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
    if (-not $iscc) {
        $iscc = @(
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
            "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
        ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $iscc) {
        throw 'Inno Setup 6 (ISCC.exe) not found. Install it with: winget install JRSoftware.InnoSetup'
    }

    & $iscc "/DAppVersion=$version" "/DExePath=$exe" 'installer\hdr_toggle.iss'
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed (exit code $LASTEXITCODE)" }

    $setup = Join-Path $root "installer\Output\hdr_toggle-$version-setup.exe"
    Write-Host "Installer: $setup"
}
finally {
    Pop-Location
}
