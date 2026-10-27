[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$v1 = Join-Path $projectRoot 'v1'

try {
    & pwsh -NoProfile -File (Join-Path $v1 'tests\comparison-tests.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'v1 synthetic comparison suite failed.'
    }

    & pwsh -NoProfile -File (Join-Path $v1 'tests\run-tests.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'v1 regression suite failed.'
    }
    Write-Host 'PASS v1 synthetic regression suite'
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
