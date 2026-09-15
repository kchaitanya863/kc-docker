$ErrorActionPreference = 'Stop'

$toolsDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
$url64 = 'https://github.com/kchaitanya863/homebrew-tap/releases/download/__TAG__/boxr-windows-x86_64.zip'
$checksum64 = '__WINDOWS_SHA256__'
$checksumType64 = 'sha256'

$packageArgs = @{
  packageName   = 'boxr'
  unzipLocation = $toolsDir
  url64bit      = $url64
  checksum64    = $checksum64
  checksumType64= $checksumType64
}

Install-ChocolateyZipPackage @packageArgs

# Add bin directory to PATH
$binPath = Join-Path $toolsDir "bin"
if (Test-Path $binPath) {
  Install-ChocolateyPath $binPath 'Machine'
}
