param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$modelRoot = Join-Path $root 'release-models'
$modelName = 'BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx'
$modelPath = Join-Path $modelRoot $modelName
$expectedHash = '5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333'
$expectedSize = 224005088
$source = 'https://github.com/ZhengPeng7/BiRefNet/releases/download/v1/BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx'

New-Item -ItemType Directory -Path $modelRoot -Force | Out-Null
if (Test-Path -LiteralPath $modelPath -PathType Leaf) {
  $existingHash = (Get-FileHash -LiteralPath $modelPath -Algorithm SHA256).Hash.ToLowerInvariant()
  $existingSize = (Get-Item -LiteralPath $modelPath).Length
  if ($existingHash -ne $expectedHash -or $existingSize -ne $expectedSize) {
    Remove-Item -LiteralPath $modelPath -Force
  }
}
if (-not (Test-Path -LiteralPath $modelPath -PathType Leaf)) {
  Invoke-WebRequest -Uri $source -OutFile $modelPath
}
$actualHash = (Get-FileHash -LiteralPath $modelPath -Algorithm SHA256).Hash.ToLowerInvariant()
$actualSize = (Get-Item -LiteralPath $modelPath).Length
if ($actualHash -ne $expectedHash -or $actualSize -ne $expectedSize) {
  throw "BiRefNet release model identity mismatch: sha256=$actualHash size=$actualSize"
}

# Tauri's restored Cargo target cache can contain release resources copied by an older commit.
# Refresh every legal/runtime document explicitly before `tauri build`; this keeps the speed of the
# compiled-artifact cache without allowing an old notice or provenance file into a new installer.
$releaseStage = Join-Path $root 'target\release'
$releaseResources = @(
  'LICENSE',
  'THIRD_PARTY_NOTICES.md',
  'THIRD_PARTY_LICENSES.txt',
  'NOTICE.md',
  'MODEL_PROVENANCE.md',
  'licenses\models\BiRefNet-LICENSE.txt',
  'docs\17_THIRD_PARTY_PROVENANCE.md',
  'docs\36_M30_DEPENDENCY_LICENSE_REPORT.json'
)
foreach ($relative in $releaseResources) {
  $sourcePath = Join-Path $root $relative
  if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) { throw "Release resource missing: $sourcePath" }
  $stagedPath = Join-Path $releaseStage $relative
  $stagedParent = Split-Path -Parent $stagedPath
  New-Item -ItemType Directory -Path $stagedParent -Force | Out-Null
  Copy-Item -LiteralPath $sourcePath -Destination $stagedPath -Force
}
Write-Output "RC2_RELEASE_MODEL name=$modelName sha256=$actualHash size=$actualSize source=$source"
Write-Output "RC_RELEASE_RESOURCES refreshed=$($releaseResources.Count) staging=$releaseStage"
