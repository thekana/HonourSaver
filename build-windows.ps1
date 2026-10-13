# Builds ReclaimHonour.exe on Windows.
# Usage:  powershell -ExecutionPolicy Bypass -File build-windows.ps1
$ErrorActionPreference = "Stop"

Push-Location $PSScriptRoot\rust
try {
    rustup show active-toolchain | Out-Null
    cargo build --release -p reclaimhonour-app
    Write-Host ""
    Write-Host "Built: $PSScriptRoot\rust\target\release\ReclaimHonour.exe"
}
finally {
    Pop-Location
}