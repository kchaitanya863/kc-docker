$ErrorActionPreference = 'Stop'

$toolsDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
$binPath = Join-Path $toolsDir "bin"
if (Test-Path $binPath) {
  Uninstall-ChocolateyPath $binPath 'Machine'
}
