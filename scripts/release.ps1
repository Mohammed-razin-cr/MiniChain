param([switch]$SkipInstall)

$ErrorActionPreference = 'Stop'
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $projectRoot
try {
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed.' }
    cargo clippy --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust lint failed.' }
    cargo test
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed.' }

    Push-Location (Join-Path $projectRoot 'frontend')
    try {
        if (-not $SkipInstall) { npm ci }
        if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed.' }
        npm test
        if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed.' }
        npm run build
        if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
        npm run lint
        if ($LASTEXITCODE -ne 0) { throw 'Frontend lint failed.' }
    } finally {
        Pop-Location
    }

    & (Join-Path $projectRoot 'target/release/minichain.exe') --json demo run
    if ($LASTEXITCODE -ne 0) { throw 'Release demo failed.' }
    & (Join-Path $PSScriptRoot 'package-release.ps1')
} finally {
    Pop-Location
}
