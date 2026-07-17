[CmdletBinding()]
param([switch] $Check)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$v2 = Join-Path $projectRoot 'v2'
$outputPath = Join-Path $v2 'THIRD-PARTY-LICENSES.txt'
$target = 'x86_64-pc-windows-msvc'
$utf8 = [Text.UTF8Encoding]::new($false)

function Get-TextHash {
    param([Parameter(Mandatory)][string] $Text)

    $bytes = $utf8.GetBytes($Text)
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

function Find-PackageDirectory {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string] $Version,
        [Parameter(Mandatory)][IO.DirectoryInfo[]] $SourceRoots
    )

    foreach ($sourceRoot in $SourceRoots) {
        $candidate = Join-Path $sourceRoot.FullName "$Name-$Version"
        if (Test-Path -LiteralPath $candidate -PathType Container) {
            return Get-Item -LiteralPath $candidate
        }
    }
    throw "Cargo source directory is unavailable for $Name $Version."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'cargo is required to generate third-party licenses.'
}

$cargoHome = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_HOME)) {
    $env:CARGO_HOME
}
else {
    Join-Path $env:USERPROFILE '.cargo'
}
$registryRoot = Join-Path $cargoHome 'registry\src'
$sourceRoots = @(Get-ChildItem -LiteralPath $registryRoot -Directory -ErrorAction Stop)
if ($sourceRoots.Count -eq 0) {
    throw 'Cargo registry sources are unavailable.'
}

Push-Location $v2
try {
    $treeLines = @(& cargo tree --locked --target $target --edges normal,build --prefix none --format '{p}|{l}|{r}')
    if ($LASTEXITCODE -ne 0) {
        throw 'cargo tree failed.'
    }
}
finally {
    Pop-Location
}

$packages = [ordered]@{}
foreach ($line in $treeLines) {
    $parts = $line -split '\|', 3
    if ($parts.Count -ne 3) {
        throw "Unexpected cargo tree output: $line"
    }
    $identity = $parts[0] -replace ' \(\*\)$', '' -replace ' \(proc-macro\)$', ''
    if ($identity -notmatch '^(?<name>\S+) v(?<version>\S+)') {
        throw "Unexpected Cargo package identity: $identity"
    }
    $name = $matches.name
    $version = $matches.version
    if ($name -eq 'varkeep') {
        continue
    }
    $license = $parts[1].Trim()
    if ([string]::IsNullOrWhiteSpace($license)) {
        throw "Cargo package has no declared license: $name $version"
    }
    $key = "$name|$version"
    if (-not $packages.Contains($key)) {
        $packages[$key] = [pscustomobject]@{
            Name = $name
            Version = $version
            License = $license
            Repository = $parts[2].Trim()
        }
    }
}

$licenseTexts = @{}
$packagesWithoutFiles = [Collections.Generic.List[string]]::new()
foreach ($package in @($packages.Values | Sort-Object Name, Version)) {
    $directory = Find-PackageDirectory -Name $package.Name -Version $package.Version -SourceRoots $sourceRoots
    $licenseFiles = @(Get-ChildItem -LiteralPath $directory.FullName -File | Where-Object {
        $_.Name -match '^(?i:LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)(?:[-._].*)?$'
    } | Sort-Object Name)

    if ($licenseFiles.Count -eq 0) {
        $repository = if ([string]::IsNullOrWhiteSpace($package.Repository)) { '(not declared)' } else { $package.Repository }
        $packagesWithoutFiles.Add("$($package.Name) $($package.Version) | $($package.License) | $repository")
        continue
    }

    foreach ($file in $licenseFiles) {
        if ($file.Length -gt 1MB) {
            throw "Unexpected oversized license file: $($package.Name) $($file.Name)"
        }
        $text = [IO.File]::ReadAllText($file.FullName).Replace("`r`n", "`n").Replace("`r", "`n")
        $text = [regex]::Replace($text, '[ \t]+(?=\n|$)', '').Trim()
        if ([string]::IsNullOrWhiteSpace($text)) {
            throw "Empty license file: $($package.Name) $($file.Name)"
        }
        $hash = Get-TextHash -Text $text
        if (-not $licenseTexts.ContainsKey($hash)) {
            $licenseTexts[$hash] = [pscustomobject]@{
                Text = $text
                Packages = [Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
            }
        }
        [void]$licenseTexts[$hash].Packages.Add("$($package.Name) $($package.Version) [$($file.Name)]")
    }
}

if ($licenseTexts.Count -eq 0) {
    throw 'No third-party license text was collected.'
}

$lines = [Collections.Generic.List[string]]::new()
$lines.Add('VARKEEP V2 THIRD-PARTY LICENSES')
$lines.Add('================================')
$lines.Add('')
$lines.Add("Generated from Cargo.lock for target $target using normal and build dependency edges.")
$lines.Add('This file contains the resolved package inventory and the license/notice files distributed in the corresponding Cargo package sources.')
$lines.Add('Slint is used under the Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0; attribution is provided by the AboutSlint widget.')
$lines.Add('')
$lines.Add('PACKAGE INVENTORY')
$lines.Add('-----------------')
foreach ($package in @($packages.Values | Sort-Object Name, Version)) {
    $repository = if ([string]::IsNullOrWhiteSpace($package.Repository)) { '(not declared)' } else { $package.Repository }
    $lines.Add("$($package.Name) $($package.Version) | $($package.License) | $repository")
}
$lines.Add('')
$lines.Add('PACKAGES WHOSE CRATE ARCHIVE DECLARES A LICENSE BUT CONTAINS NO TOP-LEVEL LICENSE FILE')
$lines.Add('------------------------------------------------------------------------------------')
$lines.Add('These packages remain listed above with their declared SPDX expression and repository.')
foreach ($package in $packagesWithoutFiles) {
    $lines.Add($package)
}

foreach ($hash in @($licenseTexts.Keys | Sort-Object)) {
    $entry = $licenseTexts[$hash]
    $lines.Add('')
    $lines.Add("LICENSE TEXT SHA-256: $hash")
    $lines.Add('Used by:')
    foreach ($package in $entry.Packages) {
        $lines.Add("- $package")
    }
    $lines.Add('')
    $lines.Add($entry.Text)
}
$content = ($lines -join "`n") + "`n"

if ($Check) {
    if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
        throw 'THIRD-PARTY-LICENSES.txt is missing.'
    }
    if ([IO.File]::ReadAllText($outputPath) -cne $content) {
        throw 'THIRD-PARTY-LICENSES.txt is out of date. Run scripts/generate-third-party-licenses.ps1.'
    }
    Write-Host "PASS third-party license inventory ($($packages.Count) packages, $($licenseTexts.Count) unique texts)"
    exit 0
}

[IO.File]::WriteAllText($outputPath, $content, $utf8)
Write-Host "Generated $outputPath ($($packages.Count) packages, $($licenseTexts.Count) unique texts)"
