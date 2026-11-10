param([Parameter(Mandatory = $true)][string]$RepositoryRoot)

$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') {
  throw 'UiGateSetupInvalid: this tool prepares only an isolated Windows CI runner'
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$runtimeRoot = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft\EdgeWebView\Application'
$runtimes = @(Get-ChildItem -LiteralPath $runtimeRoot -Filter 'msedgewebview2.exe' -File -Recurse |
  Where-Object { $_.VersionInfo.ProductVersion -match '^\d+\.\d+\.\d+\.\d+' } |
  Sort-Object { [version]([regex]::Match($_.VersionInfo.ProductVersion, '^\d+\.\d+\.\d+\.\d+').Value) } -Descending)
if (-not $runtimes.Count) { throw 'UiGateSetupUnavailable: WebView2 Runtime is missing' }
$runtimeVersion = [regex]::Match($runtimes[0].VersionInfo.ProductVersion, '^\d+\.\d+\.\d+\.\d+').Value
$build = ($runtimeVersion.Split('.')[0..2] -join '.')
$toolsRoot = Join-Path $repository ('output\ui-driver-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $toolsRoot | Out-Null
$driver = Join-Path $toolsRoot 'msedgedriver.exe'
$candidate = Get-Command 'msedgedriver.exe' -ErrorAction SilentlyContinue
if ($candidate) {
  $candidateSignature = Get-AuthenticodeSignature -LiteralPath $candidate.Source
  if ($candidateSignature.Status -eq 'Valid' -and $candidateSignature.SignerCertificate.Subject -match 'Microsoft Corporation') {
    $candidateVersion = (& $candidate.Source --version) -join ' '
    if ($LASTEXITCODE -eq 0 -and $candidateVersion -match "\b$([regex]::Escape($build))\.\d+\b") {
      Copy-Item -LiteralPath $candidate.Source -Destination $driver
    }
  }
}
if (-not (Test-Path -LiteralPath $driver)) {
  $releaseContent = (Invoke-WebRequest -Uri ("https://msedgedriver.microsoft.com/LATEST_RELEASE_{0}" -f $build)).Content
  $releaseText = if ($releaseContent -is [byte[]]) { [Text.Encoding]::UTF8.GetString($releaseContent) } else { [string]$releaseContent }
  $driverVersion = [regex]::Match($releaseText.Replace([string][char]0, ''), '\d+\.\d+\.\d+\.\d+').Value
  if (-not $driverVersion.StartsWith($build + '.', [StringComparison]::Ordinal)) {
    throw "UiGateSetupUnavailable: no matching signed EdgeDriver for WebView2 $runtimeVersion"
  }
  $archive = Join-Path $toolsRoot 'edgedriver_win64.zip'
  Invoke-WebRequest -Uri ("https://msedgedriver.microsoft.com/{0}/edgedriver_win64.zip" -f $driverVersion) -OutFile $archive
  if ((Get-Item -LiteralPath $archive).Length -gt 100MB) { throw 'UiGateSetupInvalid: oversized driver archive' }
  # Extract only the exact root executable to a predetermined new path. Never
  # extract archive-controlled paths, overwrite, recurse or delete the archive.
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $zip = [IO.Compression.ZipFile]::OpenRead($archive)
  try {
    $entries = @($zip.Entries | Where-Object { $_.FullName -ceq 'msedgedriver.exe' })
    if ($entries.Count -ne 1 -or $entries[0].Length -gt 150MB) {
      throw 'UiGateSetupInvalid: driver archive layout or size is invalid'
    }
    $inputStream = $entries[0].Open()
    $outputStream = [IO.File]::Open($driver, [IO.FileMode]::CreateNew)
    try { $inputStream.CopyTo($outputStream) }
    finally { $outputStream.Dispose(); $inputStream.Dispose() }
  } finally { $zip.Dispose() }
}
$signature = Get-AuthenticodeSignature -LiteralPath $driver
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') {
  throw 'UiGateSetupInvalid: EdgeDriver is not validly Microsoft-signed'
}
$actualVersion = (& $driver --version) -join ' '
if ($LASTEXITCODE -ne 0 -or $actualVersion -notmatch "\b$([regex]::Escape($build))\.\d+\b") {
  throw "UiGateSetupInvalid: EdgeDriver does not match Runtime $runtimeVersion"
}
$hash = (Get-FileHash -LiteralPath $driver -Algorithm SHA256).Hash.ToLowerInvariant()
$metadata = @{
  schemaVersion = 1
  scope = 'CI-only external Microsoft WebView2 automation tool; never bundled in Starroom'
  runtimeVersion = $runtimeVersion
  driverVersion = $actualVersion
  driverSha256 = $hash
  signer = $signature.SignerCertificate.Subject
}
$metadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $toolsRoot 'driver-identity.json') -Encoding utf8
"STARROOM_EDGE_DRIVER=$driver" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"STARROOM_EDGE_DRIVER_IDENTITY=$(Join-Path $toolsRoot 'driver-identity.json')" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
Write-Output ("UI_GATE_DRIVER runtime={0} sha256={1} signature=valid" -f $runtimeVersion, $hash)
