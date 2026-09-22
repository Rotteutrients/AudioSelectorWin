$ErrorActionPreference = 'Stop'

if (Get-ScheduledTask -TaskName 'AudioSelector' -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask -TaskName 'AudioSelector' -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName 'AudioSelector' -Confirm:$false
}

Write-Host 'Scheduled task AudioSelector was removed.'
Write-Host "Installed files and logs remain under: $(Join-Path $env:LOCALAPPDATA 'AudioSelector')"
