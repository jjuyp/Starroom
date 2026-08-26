param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$releaseExe = Join-Path $root 'target\release\starroom-desktop.exe'
$bundleRoot = Join-Path $root 'target\release\bundle\nsis'
if (-not (Test-Path -LiteralPath $releaseExe -PathType Leaf)) { throw "Release executable missing: $releaseExe" }
$installer = Get-ChildItem -LiteralPath $bundleRoot -Filter '*.exe' -File | Select-Object -First 1
if (-not $installer) { throw "NSIS installer missing below: $bundleRoot" }

function Assert-Launch([string]$Executable, [string]$Label) {
  $process = Start-Process -FilePath $Executable -WindowStyle Hidden -PassThru
  try {
    Start-Sleep -Seconds 8
    if ($process.HasExited) { throw "$Label exited during runtime smoke with code $($process.ExitCode)" }
  } finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
  }
}

Assert-Launch $releaseExe 'unpacked release executable'

$runnerTemp = (Resolve-Path -LiteralPath $env:RUNNER_TEMP).Path
$installRoot = Join-Path $runnerTemp 'starroom-rc-clean-install'
if (Test-Path -LiteralPath $installRoot) { throw "Clean-install target already exists: $installRoot" }
if (-not $installRoot.StartsWith($runnerTemp, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe install target: $installRoot" }

$arguments = @('/S', "/D=$installRoot")
$install = Start-Process -FilePath $installer.FullName -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "NSIS install failed with code $($install.ExitCode)" }
$installedExe = Get-ChildItem -LiteralPath $installRoot -Filter '*.exe' -File -Recurse |
  Where-Object { $_.Name -notmatch '^uninstall' } | Select-Object -First 1
if (-not $installedExe) { throw "Installed application executable missing below: $installRoot" }
Assert-Launch $installedExe.FullName 'clean-installed executable'

$uninstaller = Get-ChildItem -LiteralPath $installRoot -Filter 'uninstall*.exe' -File -Recurse | Select-Object -First 1
if (-not $uninstaller) { throw "Uninstaller missing below: $installRoot" }
$uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "NSIS uninstall failed with code $($uninstall.ExitCode)" }
Start-Sleep -Seconds 2
if (Test-Path -LiteralPath $installedExe.FullName) { throw 'Uninstall left the application executable behind' }

$exeHash = (Get-FileHash -LiteralPath $releaseExe -Algorithm SHA256).Hash.ToLowerInvariant()
$installerHash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "M30_RELEASE_EXE sha256=$exeHash"
Write-Output "M30_NSIS_INSTALLER file=$($installer.Name) sha256=$installerHash"
Write-Output 'M30_INSTALLER_RUNTIME install=ok launch=ok uninstall=ok'
