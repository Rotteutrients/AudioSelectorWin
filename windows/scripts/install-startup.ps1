param(
    [switch]$StartNow
)

$ErrorActionPreference = 'Stop'
$windowsDirectory = Split-Path -Parent $PSScriptRoot
$sourceExecutable = Join-Path $windowsDirectory 'target\release\audio-selector.exe'
if (-not (Test-Path -LiteralPath $sourceExecutable)) {
    throw "Release executable not found. Run: cargo +1.94.0 build --release --locked"
}

$applicationDirectory = Join-Path $env:LOCALAPPDATA 'AudioSelector'
$installDirectory = Join-Path $applicationDirectory 'bin'
New-Item -ItemType Directory -Path $installDirectory -Force | Out-Null
Copy-Item -LiteralPath $sourceExecutable -Destination (Join-Path $installDirectory 'audio-selector.exe') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'run-background.ps1') -Destination (Join-Path $installDirectory 'run-background.ps1') -Force

$runner = Join-Path $installDirectory 'run-background.ps1'
$arguments = "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$runner`""
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments
$trigger = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
$settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero) `
    -MultipleInstances IgnoreNew `
    -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 1)
$principal = New-ScheduledTaskPrincipal `
    -UserId "$env:USERDOMAIN\$env:USERNAME" `
    -LogonType Interactive `
    -RunLevel Limited
$task = New-ScheduledTask -Action $action -Trigger $trigger -Settings $settings -Principal $principal
Register-ScheduledTask -TaskName 'AudioSelector' -InputObject $task -Force | Out-Null

if ($StartNow) {
    Start-ScheduledTask -TaskName 'AudioSelector'
}

Write-Host "Installed: $installDirectory"
Write-Host "Scheduled task: AudioSelector"
Write-Host "Logs: $(Join-Path $applicationDirectory 'logs')"
