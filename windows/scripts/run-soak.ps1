param(
    [ValidateRange(1, 604800)]
    [int]$DurationSeconds = 28800,

    [ValidateRange(1, 3600)]
    [int]$SampleSeconds = 60
)

$ErrorActionPreference = 'Stop'

$windowsDirectory = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $windowsDirectory 'target\release\audio-selector.exe'
if (-not (Test-Path -LiteralPath $executable)) {
    throw "Release executable not found: $executable`nRun: cargo +1.94.0 build --release --locked"
}

$logDirectory = Join-Path $windowsDirectory 'soak-logs'
New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$applicationLog = Join-Path $logDirectory "spp-$timestamp.log"
$memoryLog = Join-Path $logDirectory "memory-$timestamp.csv"

$startInfo = New-Object System.Diagnostics.ProcessStartInfo
$startInfo.FileName = $executable
$startInfo.Arguments = "--run-spp $DurationSeconds"
$startInfo.WorkingDirectory = $windowsDirectory
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$startInfo.EnvironmentVariables['RUST_LOG'] = 'audio_selector=debug'

$process = New-Object System.Diagnostics.Process
$process.StartInfo = $startInfo
if (-not $process.Start()) {
    throw 'Failed to start audio-selector.exe'
}

$stdout = $process.StandardOutput.ReadToEndAsync()
$stderr = $process.StandardError.ReadToEndAsync()
'timestamp,working_set_bytes,private_memory_bytes' |
    Set-Content -LiteralPath $memoryLog -Encoding UTF8

do {
    $process.Refresh()
    '{0},{1},{2}' -f (
        (Get-Date).ToString('o'),
        $process.WorkingSet64,
        $process.PrivateMemorySize64
    ) | Add-Content -LiteralPath $memoryLog -Encoding UTF8
} while (-not $process.WaitForExit($SampleSeconds * 1000))

$process.WaitForExit()
$utf8 = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText(
    $applicationLog,
    $stdout.Result + $stderr.Result,
    $utf8
)

$samples = Import-Csv -LiteralPath $memoryLog
$first = $samples | Select-Object -First 1
$last = $samples | Select-Object -Last 1
$maximum = ($samples | Measure-Object -Property private_memory_bytes -Maximum).Maximum

Write-Host "Exit code: $($process.ExitCode)"
Write-Host "Application log: $applicationLog"
Write-Host "Memory log: $memoryLog"
Write-Host "Private memory: $($first.private_memory_bytes) -> $($last.private_memory_bytes) bytes (max $maximum)"
