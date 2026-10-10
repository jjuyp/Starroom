# Copies already-owned, hash-verified local model weights into this user's Starroom profile.
# This script deliberately neither downloads nor redistributes model binaries.
param(
    [string]$SourceRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) 'models\local')
)

$ErrorActionPreference = 'Stop'
function Get-ModelSha256([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try { return [System.BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-', '') }
    finally { $hasher.Dispose(); $stream.Dispose() }
}
$expected = [ordered]@{
    'BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx' = '5600024376F572A557870A5EB0AFB1E5961636BEF4E1E22132025467D0F03333'
    'face_detection_yunet_2026may.onnx' = 'EBAFCE4E3C118D6554634BE5C27AB333B4C047A9A8C3FAF1D7CF93101C22F0F0'
    'bisenet_resnet18.onnx' = '0D9BD318E46987C3BDBFACAE9E2C0F461CAE1C6AC6EA6D43BBE541A91727E33F'
    'segformer-b0-ade20k-489d5cd.onnx' = '56D255BEFACE9E9F82AB68A1292B8B03881AA45161DFFE914B7FB9657133DC58'
    'nafnet-sidd-width32-512-opset20.onnx' = '0E522D6DE607958C283C834E6459A37B2FCCBF5C19223A289393B4745F0CB633'
}

if (-not (Test-Path -LiteralPath $SourceRoot -PathType Container)) {
    throw "Local model source is missing: $SourceRoot"
}
if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA is unavailable.' }
$destinationRoot = Join-Path $env:LOCALAPPDATA 'studio.starroom.app\models\local'
$verifiedSources = @()
foreach ($name in $expected.Keys) {
    $source = Join-Path $SourceRoot $name
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing local model: $source" }
    $hash = Get-ModelSha256 $source
    if ($hash -ne $expected[$name]) { throw "SHA-256 mismatch for $name; no files were installed." }
    $verifiedSources += [pscustomobject]@{ Name = $name; Path = $source; Hash = $hash }
}

New-Item -ItemType Directory -Path $destinationRoot -Force | Out-Null
foreach ($model in $verifiedSources) {
    $destination = Join-Path $destinationRoot $model.Name
    if (Test-Path -LiteralPath $destination -PathType Leaf) {
        $currentHash = Get-ModelSha256 $destination
        if ($currentHash -eq $model.Hash) {
            Write-Output "Already verified: $($model.Name)"
            continue
        }
        throw "Existing personal model differs: $destination. It was not overwritten."
    }
    $staged = "$destination.installing"
    if (Test-Path -LiteralPath $staged) { throw "Staged model already exists: $staged" }
    try {
        Copy-Item -LiteralPath $model.Path -Destination $staged -ErrorAction Stop
        if ((Get-ModelSha256 $staged) -ne $model.Hash) {
            throw "Installed copy failed integrity check: $($model.Name)"
        }
        Move-Item -LiteralPath $staged -Destination $destination -ErrorAction Stop
    } finally {
        if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
    }
    Write-Output "Installed locally: $($model.Name)"
}
Write-Output "Private model installation complete: $destinationRoot"
