param([Parameter(Mandatory)][ValidateSet('x86_64', 'arm64')][string]$Arch)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location (Split-Path $PSScriptRoot -Parent)
$version = (python scripts/release.py version).Trim()
$coreVersion = (python scripts/release.py version --core).Trim()
$target = @{ x86_64 = 'x86_64-pc-windows-msvc'; arm64 = 'aarch64-pc-windows-msvc' }[$Arch]
$binary = Join-Path $PWD "target/$target/release/redisdock.exe"
python scripts/release.py binary $binary $Arch
$imports = (& dumpbin /nologo /dependents $binary) -join "`n"
if ($imports -match '(?im)^\s*(vcruntime|msvcp|libssl|libcrypto)[\w.-]*\.dll\s*$') {
    throw "Unexpected non-system runtime dependency: $($Matches[0])"
}
$outputDir = Join-Path $PWD 'target/dist'
New-Item -ItemType Directory -Force $outputDir | Out-Null
$compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
& $compiler "/DArch=$Arch" "/DVersion=$version" "/DCoreVersion=$coreVersion" "/DSourceDir=$PWD" "/DBinary=$binary" "/DOutputDir=$outputDir" scripts/windows-installer.iss
$installer = Join-Path $outputDir "RedisDock-$version-windows-$Arch-setup.exe"
$installDir = Join-Path $env:RUNNER_TEMP "redisdock-install-$Arch"
$install = Start-Process $installer -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', "/DIR=`"$installDir`"") -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "Installer failed: $($install.ExitCode)" }
try {
    $installedBinary = Join-Path $installDir 'redisdock.exe'
    python scripts/release.py binary $installedBinary $Arch
    if ((Get-FileHash $binary).Hash -ne (Get-FileHash $installedBinary).Hash) { throw 'Installed binary differs' }
    if (-not (Test-Path (Join-Path $installDir 'LICENSE'))) { throw 'Missing application license' }
    foreach ($license in Get-ChildItem assets/licenses/*.txt) {
        $installedLicense = Join-Path $installDir "licenses/$($license.Name)"
        if ((Get-FileHash $license.FullName).Hash -ne (Get-FileHash $installedLicense).Hash) { throw "Incorrect license: $license" }
    }
} finally {
    $uninstall = Start-Process (Join-Path $installDir 'unins000.exe') -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) { throw "Uninstaller failed: $($uninstall.ExitCode)" }
}
if (Test-Path (Join-Path $installDir 'redisdock.exe')) { throw 'Uninstaller left the executable behind' }
Write-Output "INSTALLER PASS: $installer"
