param(
    [Parameter(Mandatory = $true)][ValidateSet('Enable', 'Disable')][string]$Action,
    [Parameter(Mandatory = $true)][string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or $env:STARROOM_WINDOWS_UI_GATE -ne '1') {
    throw 'UiGateInvalid: browser policy is restricted to isolated Windows CI'
}
$qualificationRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../output/qualification'))
$captureRoot = [IO.Path]::GetFullPath($OutputRoot)
if ([IO.Path]::GetDirectoryName($captureRoot) -ne $qualificationRoot -or
    [IO.Path]::GetFileName($captureRoot) -notmatch '^windows-ui-[A-Za-z0-9]+$' -or
    -not (Test-Path -LiteralPath $captureRoot -PathType Container)) {
    throw 'UiGateInvalid: policy requires an existing owned qualification directory'
}
# Elevated WebView2 hosts ignore environment/HKCU overrides. Use Microsoft's
# documented per-application HKLM override only on this disposable CI runner.
# https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security
$policyPath = 'HKLM:\SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
$applicationName = 'starroom-desktop.exe'
$arguments = '--remote-debugging-port=9223 --remote-debugging-address=127.0.0.1'
$receiptPath = Join-Path $captureRoot 'browser-policy.json'
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
$elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($Action -eq 'Enable') {
    if (Test-Path -LiteralPath $receiptPath) { throw 'UiGateInvalid: policy receipt already exists' }
    $createdKey = $false
    if ($elevated) {
        $key = Get-Item -LiteralPath $policyPath -ErrorAction SilentlyContinue
        if ($null -ne $key -and $key.GetValueNames() -contains $applicationName) {
            throw 'UiGateInvalid: refusing to replace an existing application browser policy'
        }
        if ($null -eq $key) {
            New-Item -Path $policyPath -Force | Out-Null
            $createdKey = $true
        }
        New-ItemProperty -LiteralPath $policyPath -Name $applicationName -Value $arguments -PropertyType String | Out-Null
    }
    [ordered]@{ schemaVersion = 1; elevated = $elevated; enabled = $elevated
        createdKey = $createdKey; cleaned = $false } |
        ConvertTo-Json | Set-Content -LiteralPath $receiptPath -Encoding UTF8
} else {
    if (-not (Test-Path -LiteralPath $receiptPath)) { return }
    $receipt = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json
    if ($receipt.schemaVersion -ne 1) { throw 'UiGateInvalid: invalid owned policy receipt' }
    if ($receipt.enabled -and -not $receipt.cleaned) {
        $key = Get-Item -LiteralPath $policyPath
        if ($key.GetValue($applicationName) -ne $arguments) {
            throw 'UiGateInvalid: policy ownership changed; refusing to remove it'
        }
        Remove-ItemProperty -LiteralPath $policyPath -Name $applicationName
        if ($receipt.createdKey -and $key.GetValueNames().Count -eq 0 -and $key.GetSubKeyNames().Count -eq 0) {
            Remove-Item -LiteralPath $policyPath
        }
    }
    $receipt.cleaned = $true
    $receipt | ConvertTo-Json | Set-Content -LiteralPath $receiptPath -Encoding UTF8
}
