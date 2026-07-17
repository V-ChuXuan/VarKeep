[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Tag
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot

try {
    $cargo = Get-Content -Raw (Join-Path $projectRoot 'v2\Cargo.toml')
    $match = [regex]::Match($cargo, '(?m)^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"\s*$')
    if (-not $match.Success) {
        throw 'Could not read the v2 package version.'
    }
    $expected = "v$($match.Groups['version'].Value)"
    if ($Tag -cne $expected) {
        throw "Release tag must match the v2 package version: expected $expected, found $Tag"
    }
    Write-Host "PASS release tag matches package version ($Tag)"
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
