$ErrorActionPreference = 'Stop'

$installDirectory = $PSScriptRoot
$applicationDirectory = Split-Path -Parent $installDirectory
$executable = Join-Path $installDirectory 'audio-selector.exe'
$logDirectory = Join-Path $applicationDirectory 'logs'
$currentLog = Join-Path $logDirectory 'audio-selector.log'

New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
for ($generation = 4; $generation -ge 1; $generation--) {
    $source = "$currentLog.$generation"
    $destination = "$currentLog.$($generation + 1)"
    if (Test-Path -LiteralPath $source) {
        Move-Item -LiteralPath $source -Destination $destination -Force
    }
}
if (Test-Path -LiteralPath $currentLog) {
    Move-Item -LiteralPath $currentLog -Destination "$currentLog.1" -Force
}

Push-Location $installDirectory
try {
    & $env:ComSpec /d /s /c "`"$executable`" --run >> `"$currentLog`" 2>&1"
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
