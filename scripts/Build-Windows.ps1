# Run from PowerShell after installing the prerequisites in docs/windows-build.md.
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$localTools = Join-Path (Split-Path -Parent $repoRoot) 'tools'
foreach ($toolDirectory in @(
    (Join-Path $env:USERPROFILE '.cargo\bin'),
    (Join-Path $localTools 'flutter\bin'),
    (Join-Path $localTools 'frb-codegen'),
    (Join-Path $localTools 'cargo-expand')
)) {
    if (Test-Path -LiteralPath $toolDirectory) {
        $env:PATH = "$toolDirectory;$env:PATH"
    }
}
foreach ($command in @('flutter', 'cargo', 'rustup')) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "Missing $command in PATH. See docs/windows-build.md."
    }
}
function Invoke-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed (exit $LASTEXITCODE)." }
}
$previousToolchain = $env:RUSTUP_TOOLCHAIN
Push-Location (Join-Path $repoRoot 'src')
try {
    # Avoid installing every mobile/desktop target from rust-toolchain.toml.
    $env:RUSTUP_TOOLCHAIN = '1.97.1'
    Invoke-Checked 'rustup' @('toolchain', 'install', '1.97.1', '--profile', 'minimal', '--component', 'rustfmt')
    Invoke-Checked 'flutter' @('doctor', '-v')
    Invoke-Checked 'flutter' @('pub', 'get')
    if (-not (Get-Command flutter_rust_bridge_codegen -ErrorAction SilentlyContinue)) {
        Invoke-Checked 'cargo' @('install', 'flutter_rust_bridge_codegen', '--version', '2.13.0', '--locked')
    }
    $bridgeVersion = & flutter_rust_bridge_codegen --version
    if ($bridgeVersion -notmatch '\b2\.13\.0\b') {
        throw "Expected flutter_rust_bridge_codegen 2.13.0, got: $bridgeVersion"
    }
    if (-not (Get-Command cargo-expand -ErrorAction SilentlyContinue)) {
        Invoke-Checked 'cargo' @('install', 'cargo-expand', '--locked')
    }
    Invoke-Checked 'flutter_rust_bridge_codegen' @('generate')
    Invoke-Checked 'cargo' @('test', '--manifest-path', 'rust/Cargo.toml', '--locked', 'lyrics::')
    Invoke-Checked 'flutter' @('build', 'windows', '--release')
    $release = Join-Path $repoRoot 'src/build/windows/x64/runner/Release'
    $dist = Join-Path $repoRoot 'dist'
    New-Item -ItemType Directory -Force $dist | Out-Null
    $portable = Join-Path $dist 'YAYMA-Windows'
    New-Item -ItemType Directory -Force $portable | Out-Null
    Copy-Item -Path (Join-Path $release '*') -Destination $portable -Recurse -Force
    Compress-Archive -Path (Join-Path $portable '*') -DestinationPath (Join-Path $dist 'yayma-windows-lyrics.zip') -Force
    Write-Output "Ready: $portable\yayma.exe"
    Write-Output "Portable archive: $dist\yayma-windows-lyrics.zip"
} finally {
    $env:RUSTUP_TOOLCHAIN = $previousToolchain
    Pop-Location
}
