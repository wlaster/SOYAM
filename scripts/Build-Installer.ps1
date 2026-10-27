param([string]$Compiler, [string]$RuntimeDirectory)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$release = Join-Path $repoRoot 'src/build/windows/x64/runner/Release'
foreach ($required in @('soyam.exe', 'yayma.dll', 'flutter_windows.dll', 'data/flutter_assets', 'data/icudtl.dat')) {
    if (-not (Test-Path -LiteralPath (Join-Path $release $required))) { throw "Missing release file: $required" }
}
$versionMatch = Select-String -LiteralPath (Join-Path $repoRoot 'src/pubspec.yaml') -Pattern '^version:\s*([^+\s]+)'
if (-not $versionMatch) { throw 'Version missing from pubspec.yaml' }
$version = $versionMatch.Matches[0].Groups[1].Value
$exeVersion = (Get-Item -LiteralPath (Join-Path $release 'soyam.exe')).VersionInfo.ProductVersion
if (($exeVersion -split '\+')[0] -ne $version) { throw "Build version $exeVersion does not match $version; rebuild first." }
if (-not $Compiler) {
    $candidate = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($candidate) { $Compiler = $candidate.Source }
    foreach ($path in @(
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs/Inno Setup 6/ISCC.exe'),
        (Join-Path (Split-Path -Parent $repoRoot) 'tools/innosetup/ISCC.exe')
    )) {
        if (-not $Compiler -and (Test-Path -LiteralPath $path)) { $Compiler = $path }
    }
}
if (-not $Compiler -or -not (Test-Path -LiteralPath $Compiler)) { throw 'Inno Setup compiler not found. Specify -Compiler.' }
if (-not $RuntimeDirectory) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (Test-Path -LiteralPath $vswhere) {
        $installations = & $vswhere -all -products '*' -property installationPath
        $RuntimeDirectory = $installations | ForEach-Object {
            Get-ChildItem -Path (Join-Path $_ 'VC/Redist/MSVC/*/x64/Microsoft.VC143.CRT') -Directory -ErrorAction SilentlyContinue
        } | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
}
foreach ($dll in @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
    if (-not $RuntimeDirectory -or -not (Test-Path -LiteralPath (Join-Path $RuntimeDirectory $dll))) { throw "MSVC x64 runtime missing: $dll" }
}
$installer = Join-Path $repoRoot "dist/soyam-windows-setup-$version.exe"
if (Test-Path -LiteralPath $installer) { throw "Installer already exists: $installer" }
& $Compiler "/DAppVersion=$version" "/DReleaseDir=$release" "/DRuntimeDir=$RuntimeDirectory" (Join-Path $repoRoot 'packaging/windows/yayma.iss')
if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed: $LASTEXITCODE" }
if (-not (Test-Path -LiteralPath $installer)) { throw 'Installer was not produced' }
$installerInfo = (Get-Item -LiteralPath $installer).VersionInfo
$installerVersion = "$($installerInfo.FileMajorPart).$($installerInfo.FileMinorPart).$($installerInfo.FileBuildPart)"
if ($installerVersion -ne $version) { throw "Installer version $installerVersion does not match $version" }
$hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $([IO.Path]::GetFileName($installer))" | Set-Content -LiteralPath "$installer.sha256" -Encoding ascii
Write-Output "Installer: $installer"
Write-Output "SHA256: $hash"
