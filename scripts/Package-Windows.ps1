param([string]$RuntimeDirectory)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$release = Join-Path $repoRoot 'src/build/windows/x64/runner/Release'
$versionMatch = Select-String -LiteralPath (Join-Path $repoRoot 'src/pubspec.yaml') -Pattern '^version:\s*([^+\s]+)'
if (-not $versionMatch) { throw 'Version missing from pubspec.yaml' }
$version = $versionMatch.Matches[0].Groups[1].Value
$stem = 'soyam-windows-x64-portable'
$dist = Join-Path $repoRoot 'dist'
$portable = Join-Path $dist $stem
$archive = Join-Path $dist "$stem.zip"
foreach ($required in @('soyam.exe', 'yayma.dll', 'flutter_windows.dll', 'data/flutter_assets', 'data/icudtl.dat')) {
    if (-not (Test-Path -LiteralPath (Join-Path $release $required))) { throw "Missing release file: $required" }
}
if ((Test-Path -LiteralPath $portable) -or (Test-Path -LiteralPath $archive)) {
    throw "Release package already exists: $stem. Move the previous package before packaging again."
}
if (-not $RuntimeDirectory) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (Test-Path -LiteralPath $vswhere) {
        $installations = & $vswhere -all -products '*' -property installationPath
        $RuntimeDirectory = $installations | ForEach-Object {
            Get-ChildItem -Path (Join-Path $_ 'VC/Redist/MSVC/*/x64/Microsoft.VC143.CRT') -Directory -ErrorAction SilentlyContinue
        } | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
}
if (-not $RuntimeDirectory -or -not (Test-Path -LiteralPath (Join-Path $RuntimeDirectory 'msvcp140.dll'))) {
    throw 'MSVC x64 redistributable runtime not found. Specify -RuntimeDirectory.'
}
New-Item -ItemType Directory -Path $portable -Force | Out-Null
# Exclude an obsolete runner left by a build made before the executable rename.
Get-ChildItem -LiteralPath $release | Where-Object { $_.Name -notin @('yayma.exe', 'yayma.pdb', 'soyam.pdb') } | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $portable -Recurse -Force
}
Get-ChildItem -LiteralPath $RuntimeDirectory -Filter '*.dll' | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $portable
}
Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination (Join-Path $portable 'LICENSE.txt')
@"
SOYAM $version - Windows x64 portable

Extract the entire archive into a writable folder and run soyam.exe.
Keep all DLL files and the data folder next to soyam.exe.
No installation is required. The MSVC x64 runtime is included.
Account data, settings and downloaded tracks are stored in your Windows profile.
Existing YAYMA data is reused when present.
Close the application before replacing files with a newer release.

Project and updates: https://github.com/wlaster/SOYAM
License: GPL-3.0 (LICENSE.txt)
"@ | Set-Content -LiteralPath (Join-Path $portable 'PORTABLE-README.txt') -Encoding utf8
Compress-Archive -Path (Join-Path $portable '*') -DestinationPath $archive -CompressionLevel Optimal
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    $files = @(Get-ChildItem -LiteralPath $portable -File -Recurse)
    $entries = @($zip.Entries | Where-Object { $_.Name })
    if ($entries.Count -ne $files.Count) { throw 'Archive file count mismatch' }
    foreach ($entry in $entries) {
        $file = Join-Path $portable ($entry.FullName.Replace('/', [IO.Path]::DirectorySeparatorChar))
        $stream = $entry.Open()
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $actual = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '') }
        finally { $sha.Dispose(); $stream.Dispose() }
        if ($actual -ne (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash) { throw "Archive mismatch: $file" }
    }
} finally { $zip.Dispose() }
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "Executable: $portable\soyam.exe"
Write-Output "Archive: $archive"
Write-Output "SHA256: $hash"
Write-Output "Verified files: $($files.Count)"
