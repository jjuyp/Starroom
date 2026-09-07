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
Write-Output "RC2_RELEASE_MODEL name=$modelName sha256=$actualHash size=$actualSize source=$source"
