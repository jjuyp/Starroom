param(
    [Parameter(Mandatory = $true)][ValidateRange(1, 2147483647)][int]$ApplicationId,
    [Parameter(Mandatory = $true)][string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or $env:STARROOM_WINDOWS_UI_GATE -ne '1') {
    throw 'UiGateInvalid: startup diagnostics are restricted to isolated Windows CI'
}
$qualificationRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../output/qualification'))
$captureRoot = [IO.Path]::GetFullPath($OutputRoot)
if ([IO.Path]::GetDirectoryName($captureRoot) -ne $qualificationRoot -or
    [IO.Path]::GetFileName($captureRoot) -notmatch '^windows-ui-[A-Za-z0-9]+$' -or
    -not (Test-Path -LiteralPath $captureRoot -PathType Container)) {
    throw 'UiGateInvalid: diagnostics require an existing owned qualification directory'
}
# Collect only this application's descendants, not the runner's other processes.
# No environment variables, source pixels, model files or user metadata are read.
$pending = [Collections.Generic.Queue[int]]::new()
$pending.Enqueue($ApplicationId)
$seen = [Collections.Generic.HashSet[int]]::new()
$processes = [Collections.Generic.List[object]]::new()
while ($pending.Count -gt 0 -and $seen.Count -lt 32) {
    $ownedId = $pending.Dequeue()
    if (-not $seen.Add($ownedId)) { continue }
    $owned = Get-CimInstance Win32_Process -Filter "ProcessId = $ownedId"
    if ($null -ne $owned) {
        $window = Get-Process -Id $ownedId -ErrorAction SilentlyContinue
        $processes.Add([ordered]@{
            id = $owned.ProcessId; parentId = $owned.ParentProcessId; name = $owned.Name
            commandLine = $owned.CommandLine; sessionId = $owned.SessionId
            windowTitle = $window.MainWindowTitle; responding = $window.Responding
        })
        foreach ($descendant in @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $ownedId")) {
            $pending.Enqueue([int]$descendant.ProcessId)
        }
    }
}
$listeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
    Where-Object { $_.LocalPort -in 9223, 9515 } |
    Select-Object LocalAddress, LocalPort, OwningProcess)
[ordered]@{
    scope = 'Isolated CI public fixture only'; applicationId = $ApplicationId
    processes = @($processes.ToArray()); listeners = $listeners
    truncated = $pending.Count -gt 0
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $captureRoot 'startup-processes.json') -Encoding UTF8
