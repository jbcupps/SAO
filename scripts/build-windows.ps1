# Local Windows dev build helper for SAO.
#
# Sets the OpenSSL environment variables required by `webauthn-rs` -> `openssl-sys`
# on x86_64-pc-windows-msvc with the workspace's `+crt-static` rustflags, then
# invokes cargo. See `.cargo/config.toml` for the full motivation.
#
# Usage examples:
#   pwsh -File scripts/build-windows.ps1                            # cargo build --workspace --all-targets
#   pwsh -File scripts/build-windows.ps1 -Cargo 'test --workspace'  # cargo test --workspace
#   pwsh -File scripts/build-windows.ps1 -Cargo 'clippy --workspace --all-targets -- -D warnings'

[CmdletBinding()]
param(
    [string]$Cargo = 'build --workspace --all-targets',
    [string]$OpenSslRoot = 'C:\Program Files\OpenSSL-Win64'
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $OpenSslRoot)) {
    Write-Error "OpenSSL not found at '$OpenSslRoot'. Install OpenSSL-Win64 (https://slproweb.com/products/Win32OpenSSL.html) or pass -OpenSslRoot."
    exit 1
}

$libDir = Join-Path -Path $OpenSslRoot -ChildPath 'lib\VC\x64\MT'
if (-not (Test-Path -LiteralPath $libDir)) {
    Write-Error "OpenSSL static-CRT libs not found at '$libDir'. The MSI's MT (static CRT) libs are required because .cargo/config.toml pins +crt-static."
    exit 1
}

$includeDir = Join-Path -Path $OpenSslRoot -ChildPath 'include'

$env:OPENSSL_DIR = $OpenSslRoot
$env:OPENSSL_LIB_DIR = $libDir
$env:OPENSSL_INCLUDE_DIR = $includeDir
$env:OPENSSL_STATIC = '1'

Write-Host "OPENSSL_DIR     = $($env:OPENSSL_DIR)"
Write-Host "OPENSSL_LIB_DIR = $($env:OPENSSL_LIB_DIR)"
Write-Host "OPENSSL_STATIC  = $($env:OPENSSL_STATIC)"
Write-Host ''
Write-Host "cargo $Cargo"

$cargoArgs = $Cargo -split '\s+' | Where-Object { $_ -ne '' }
& cargo @cargoArgs
exit $LASTEXITCODE
