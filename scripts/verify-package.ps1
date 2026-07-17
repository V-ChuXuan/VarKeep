[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $projectRoot 'dist'

function Get-ZipEntries {
    param([string] $Path)

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        return @($archive.Entries | Where-Object { $_.FullName -notmatch '/$' } | ForEach-Object {
            $_.FullName.Replace('\', '/')
        } | Sort-Object)
    }
    finally {
        $archive.Dispose()
    }
}

function Get-ZipText {
    param([string] $Path, [string] $EntryName)

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $entry = $archive.GetEntry($EntryName)
        if ($null -eq $entry) {
            throw "Release archive is missing $EntryName."
        }
        $reader = [IO.StreamReader]::new($entry.Open(), [Text.UTF8Encoding]::new($false), $true)
        try {
            return $reader.ReadToEnd()
        }
        finally {
            $reader.Dispose()
        }
    }
    finally {
        $archive.Dispose()
    }
}

function Assert-PackagedMarkdownLinks {
    param([string] $ArchivePath, [string[]] $Entries)

    $readme = Get-ZipText -Path $ArchivePath -EntryName 'README.md'
    foreach ($match in [regex]::Matches($readme, '!?(?:\[[^\]]*\])\((?<target>[^)]+)\)')) {
        $target = $match.Groups['target'].Value.Trim()
        if ($target -match '^(?i:https?://|mailto:|#)') {
            continue
        }
        $target = [Uri]::UnescapeDataString(($target -split '#', 2)[0]).Replace('\', '/').TrimStart('./')
        if ($target -notin $Entries) {
            throw "Packaged README points to a missing file: $target"
        }
    }
}

try {
    $expectedArtifacts = @(
        'SHA256SUMS.txt',
        'varkeep-v1-cli.zip',
        'varkeep-v2-windows-x64.zip'
    ) | Sort-Object
    if (-not (Test-Path -LiteralPath $dist -PathType Container)) {
        throw 'Release dist directory is missing.'
    }
    $actualArtifacts = @(Get-ChildItem -LiteralPath $dist -Force | ForEach-Object {
        if ($_.PSIsContainer -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Release dist contains a directory or reparse point.'
        }
        $_.Name
    } | Sort-Object)
    if (Compare-Object $expectedArtifacts $actualArtifacts) {
        throw 'Release dist does not match the exact three-file allowlist.'
    }

    $packages = @{
        'varkeep-v1-cli.zip' = @('LICENSE', 'README.md', 'backup-env.ps1', 'scripts/check-script-ast.ps1', 'start-interactive.cmd', 'tests/run-tests.ps1') | Sort-Object
        'varkeep-v2-windows-x64.zip' = @('LICENSE', 'README.md', 'THIRD-PARTY-LICENSES.txt', 'THIRD-PARTY-NOTICES.md', 'varkeep.exe') | Sort-Object
    }
    foreach ($package in $packages.GetEnumerator()) {
        $archivePath = Join-Path $dist $package.Key
        $entries = @(Get-ZipEntries $archivePath)
        if (Compare-Object $package.Value $entries) {
            throw "Release archive has unexpected contents: $($package.Key)"
        }
        Assert-PackagedMarkdownLinks -ArchivePath $archivePath -Entries $entries
        if ($entries -match '(^|/)(backups|target|snapshot\.json)(/|$)' -or
            $entries -match '(^|/)restore/(user|system|all)\.ps1$' -or
            $entries -match 'restore-(user|machine)-env\.ps1') {
            throw "Release archive contains a sensitive or generated path: $($package.Key)"
        }
    }

    $v1SmokeRoot = Join-Path ([IO.Path]::GetTempPath()) ("varkeep-v1-package-test-" + [guid]::NewGuid().ToString('N'))
    try {
        Expand-Archive -LiteralPath (Join-Path $dist 'varkeep-v1-cli.zip') -DestinationPath $v1SmokeRoot
        & pwsh -NoProfile -File (Join-Path $v1SmokeRoot 'tests\run-tests.ps1') *> $null
        if ($LASTEXITCODE -ne 0) {
            throw 'The v1 release archive regression suite failed.'
        }
    }
    finally {
        $absoluteSmokeRoot = [IO.Path]::GetFullPath($v1SmokeRoot)
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if ($absoluteSmokeRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and
            [IO.Path]::GetFileName($absoluteSmokeRoot).StartsWith('varkeep-v1-package-test-', [StringComparison]::Ordinal)) {
            Remove-Item -LiteralPath $absoluteSmokeRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    $expectedHashLines = @('varkeep-v1-cli.zip', 'varkeep-v2-windows-x64.zip') | ForEach-Object {
        $hash = (Get-FileHash -LiteralPath (Join-Path $dist $_) -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $_"
    }
    $actualHashLines = @([IO.File]::ReadAllLines((Join-Path $dist 'SHA256SUMS.txt')) | Where-Object { $_ })
    if (Compare-Object $expectedHashLines $actualHashLines) {
        throw 'SHA256SUMS.txt does not match the release archives.'
    }

    Write-Host 'PASS exact v1 and v2 release archive allowlists'
    Write-Host 'PASS packaged README links resolve inside each archive'
    Write-Host 'PASS v1 release archive regression suite'
    Write-Host 'PASS release archive SHA-256 manifest'
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
