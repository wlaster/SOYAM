$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$env:RUSTUP_TOOLCHAIN = '1.97.1'
$cargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
& $cargo test --locked --manifest-path (Join-Path $repo 'src/rust/Cargo.toml') live_all_public_sources -- --ignored --nocapture
exit $LASTEXITCODE
