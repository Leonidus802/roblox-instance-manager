param([switch]$Check)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    $localCargo = Join-Path $PSScriptRoot '.tools\cargo\bin\cargo.exe'
    if (Test-Path -LiteralPath $localCargo) {
        $env:RUSTUP_HOME = Join-Path $PSScriptRoot '.tools\rustup'
        $env:CARGO_HOME = Join-Path $PSScriptRoot '.tools\cargo'
        $cargo = $localCargo
    } else {
        $cargo = (Get-Command cargo -ErrorAction Stop).Source
    }
    if ($Check) {
        & $cargo fmt -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed.' }
        & $cargo test --locked
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
        & $cargo clippy --locked --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    }
    & $cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    New-Item -ItemType Directory -Force -Path dist | Out-Null
    Copy-Item -LiteralPath target\release\roblox-instance-manager.exe -Destination dist\RobloxInstanceManager.exe
    Copy-Item -LiteralPath README.md -Destination dist\README.md
    Copy-Item -LiteralPath LICENSE -Destination dist\LICENSE
    Get-FileHash -LiteralPath dist\RobloxInstanceManager.exe -Algorithm SHA256 |
        Format-List | Out-File -LiteralPath dist\SHA256.txt -Encoding utf8
    Compress-Archive -LiteralPath dist\RobloxInstanceManager.exe,dist\README.md,dist\LICENSE,dist\SHA256.txt -DestinationPath dist\RobloxInstanceManager-windows-x64.zip -Force
    Write-Output (Join-Path $PSScriptRoot 'dist\RobloxInstanceManager.exe')
} finally { Pop-Location }
