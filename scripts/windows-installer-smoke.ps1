param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$releaseExe = Join-Path $root 'target\release\starroom-desktop.exe'
$bundleRoot = Join-Path $root 'target\release\bundle\nsis'
if (-not (Test-Path -LiteralPath $releaseExe -PathType Leaf)) { throw "Release executable missing: $releaseExe" }
$installer = Get-ChildItem -LiteralPath $bundleRoot -Filter '*.exe' -File | Select-Object -First 1
if (-not $installer) { throw "NSIS installer missing below: $bundleRoot" }

function Assert-Launch([string]$Executable, [string]$Label, [string]$ProfileRoot) {
  if (Test-Path -LiteralPath $ProfileRoot) { throw "$Label profile target is not clean: $ProfileRoot" }
  $roaming = Join-Path $ProfileRoot 'AppData\Roaming'
  $local = Join-Path $ProfileRoot 'AppData\Local'
  $webview = Join-Path $ProfileRoot 'WebView2'
  New-Item -ItemType Directory -Path $roaming, $local, $webview -Force | Out-Null
  $previous = @{
    APPDATA = $env:APPDATA
    LOCALAPPDATA = $env:LOCALAPPDATA
    WEBVIEW2_USER_DATA_FOLDER = $env:WEBVIEW2_USER_DATA_FOLDER
  }
  $env:APPDATA = $roaming
  $env:LOCALAPPDATA = $local
  $env:WEBVIEW2_USER_DATA_FOLDER = $webview
  $process = $null
  try {
    $process = Start-Process -FilePath $Executable -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 8
    if ($process.HasExited) { throw "$Label exited during runtime smoke with code $($process.ExitCode)" }
  } finally {
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
    $env:APPDATA = $previous.APPDATA
    $env:LOCALAPPDATA = $previous.LOCALAPPDATA
    $env:WEBVIEW2_USER_DATA_FOLDER = $previous.WEBVIEW2_USER_DATA_FOLDER
  }
}

function Assert-ReleaseSelfTest([string]$Executable, [string]$TestRoot) {
  if (Test-Path -LiteralPath $TestRoot) { throw "Release self-test target is not clean: $TestRoot" }
  $output = & $Executable '--release-self-test' $TestRoot
  if ($LASTEXITCODE -ne 0) { throw "Packaged release self-test failed with code $LASTEXITCODE" }
  $report = $output | ConvertFrom-Json
  if ($report.library -ne 'ok' -or $report.history -ne 'ok' -or $report.session -ne 'ok' -or
      $report.nativeExport -ne 'ok' -or -not $report.deterministicExport -or -not $report.sourceImmutable) {
    throw "Packaged release self-test returned an invalid core report: $output"
  }
  foreach ($model in @('portraitModels', 'aiMaskModels', 'aiDenoiseModel')) {
    if ($report.$model -notin @('available', 'typed-unavailable')) {
      throw "Packaged release self-test returned an invalid $model state: $($report.$model)"
    }
  }
  Write-Output "M30_RELEASE_SELF_TEST $output"
}

$runnerTemp = (Resolve-Path -LiteralPath $env:RUNNER_TEMP).Path
Assert-Launch $releaseExe 'unpacked release executable' (Join-Path $runnerTemp 'starroom-rc-unpacked-profile')

$installRoot = Join-Path $runnerTemp 'starroom-rc-clean-install'
if (Test-Path -LiteralPath $installRoot) { throw "Clean-install target already exists: $installRoot" }
if (-not $installRoot.StartsWith($runnerTemp, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe install target: $installRoot" }

$arguments = @('/S', "/D=$installRoot")
$install = Start-Process -FilePath $installer.FullName -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "NSIS install failed with code $($install.ExitCode)" }
$installedExe = Get-ChildItem -LiteralPath $installRoot -Filter '*.exe' -File -Recurse |
  Where-Object { $_.Name -notmatch '^uninstall' } | Select-Object -First 1
if (-not $installedExe) { throw "Installed application executable missing below: $installRoot" }

function Assert-BundledResource([string]$SourceRelativePath) {
  $source = Join-Path $root $SourceRelativePath
  if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Release resource source missing: $source" }
  $name = Split-Path -Leaf $SourceRelativePath
  $matches = @(Get-ChildItem -LiteralPath $installRoot -Filter $name -File -Recurse)
  if ($matches.Count -ne 1) { throw "Expected one installed $name resource, found $($matches.Count)" }
  $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
  $installedHash = (Get-FileHash -LiteralPath $matches[0].FullName -Algorithm SHA256).Hash
  if ($sourceHash -ne $installedHash) { throw "Installed resource hash mismatch: $name" }
  return $matches[0].FullName
}

$installedResources = @(
  Assert-BundledResource 'LICENSE'
  Assert-BundledResource 'THIRD_PARTY_NOTICES.md'
  Assert-BundledResource 'THIRD_PARTY_LICENSES.txt'
  Assert-BundledResource 'NOTICE.md'
  Assert-BundledResource 'MODEL_PROVENANCE.md'
  Assert-BundledResource 'docs\17_THIRD_PARTY_PROVENANCE.md'
  Assert-BundledResource 'docs\36_M30_DEPENDENCY_LICENSE_REPORT.json'
)
Assert-Launch $installedExe.FullName 'clean-installed executable' (Join-Path $runnerTemp 'starroom-rc-installed-profile')
Assert-ReleaseSelfTest $installedExe.FullName (Join-Path $runnerTemp 'starroom-rc-production-self-test')

$uninstaller = Get-ChildItem -LiteralPath $installRoot -Filter 'uninstall*.exe' -File -Recurse | Select-Object -First 1
if (-not $uninstaller) { throw "Uninstaller missing below: $installRoot" }
$uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "NSIS uninstall failed with code $($uninstall.ExitCode)" }
Start-Sleep -Seconds 2
if (Test-Path -LiteralPath $installedExe.FullName) { throw 'Uninstall left the application executable behind' }
foreach ($resource in $installedResources) {
  if (Test-Path -LiteralPath $resource) { throw "Uninstall left a legal resource behind: $resource" }
}

$exeHash = (Get-FileHash -LiteralPath $releaseExe -Algorithm SHA256).Hash.ToLowerInvariant()
$installerHash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "M30_RELEASE_EXE sha256=$exeHash"
Write-Output "M30_NSIS_INSTALLER file=$($installer.Name) sha256=$installerHash"
Write-Output 'M30_INSTALLER_RUNTIME install=ok launch=ok self_test=ok legal_resources=ok uninstall=ok'
