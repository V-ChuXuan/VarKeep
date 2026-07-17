[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $projectRoot 'dist'
$work = Join-Path ([IO.Path]::GetTempPath()) ("varkeep-package-" + [guid]::NewGuid().ToString('N'))
$v1Stage = Join-Path $work 'v1'
$v2Stage = Join-Path $work 'v2'

function Assert-NormalDirectory {
    param([string] $Path, [string] $Label)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "$Label is not a directory."
    }
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "$Label is a reparse point."
    }
}

try {
    & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'verify-release.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'Release verification failed.'
    }

    $absoluteDist = [IO.Path]::GetFullPath($dist).TrimEnd('\')
    $expectedDist = [IO.Path]::GetFullPath((Join-Path $projectRoot 'dist')).TrimEnd('\')
    if ($absoluteDist -ne $expectedDist -or
        -not $absoluteDist.StartsWith([IO.Path]::GetFullPath($projectRoot), [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Release dist path escaped the project.'
    }
    if (Test-Path -LiteralPath $absoluteDist) {
        Assert-NormalDirectory $absoluteDist 'Release dist'
        $allowedExisting = @('SHA256SUMS.txt', 'varkeep-v1-cli.zip', 'varkeep-v2-windows-x64.zip')
        $unexpected = @(Get-ChildItem -LiteralPath $absoluteDist -Force | Where-Object {
            $_.PSIsContainer -or
            ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $_.Name -notin $allowedExisting
        })
        if ($unexpected) {
            throw 'Release dist contains an unexpected item; no files were replaced.'
        }
        foreach ($name in $allowedExisting) {
            $target = Join-Path $absoluteDist $name
            if (Test-Path -LiteralPath $target) {
                Remove-Item -LiteralPath $target -Force
            }
        }
    }
    else {
        New-Item -ItemType Directory -Path $absoluteDist | Out-Null
    }

    New-Item -ItemType Directory -Path $v1Stage, $v2Stage | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'v1\backup-env.ps1') -Destination $v1Stage
    Copy-Item -LiteralPath (Join-Path $projectRoot 'v1\start-interactive.cmd') -Destination $v1Stage
    Copy-Item -LiteralPath (Join-Path $projectRoot 'v1\README.md') -Destination $v1Stage
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $v1Stage
    New-Item -ItemType Directory -Path (Join-Path $v1Stage 'tests'), (Join-Path $v1Stage 'scripts') | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'v1\tests\run-tests.ps1') -Destination (Join-Path $v1Stage 'tests')
    Copy-Item -LiteralPath (Join-Path $projectRoot 'scripts\check-script-ast.ps1') -Destination (Join-Path $v1Stage 'scripts')

    $publish = Join-Path $projectRoot 'v2\publish'
    foreach ($name in @('varkeep.exe', 'README.md', 'LICENSE', 'THIRD-PARTY-LICENSES.txt', 'THIRD-PARTY-NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $publish $name) -Destination $v2Stage
    }

    $v1Zip = Join-Path $absoluteDist 'varkeep-v1-cli.zip'
    $v2Zip = Join-Path $absoluteDist 'varkeep-v2-windows-x64.zip'
    Compress-Archive -Path (Join-Path $v1Stage '*') -DestinationPath $v1Zip -CompressionLevel Optimal
    Compress-Archive -Path (Join-Path $v2Stage '*') -DestinationPath $v2Zip -CompressionLevel Optimal

    $hashLines = @($v1Zip, $v2Zip) | ForEach-Object {
        $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $([IO.Path]::GetFileName($_))"
    }
    $utf8 = [Text.UTF8Encoding]::new($false)
    [IO.File]::WriteAllText(
        (Join-Path $absoluteDist 'SHA256SUMS.txt'),
        (($hashLines -join "`n") + "`n"),
        $utf8
    )

    & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'verify-package.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'Release package verification failed.'
    }
    Write-Host "Release archives created in $absoluteDist"
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
finally {
    $absoluteWork = [IO.Path]::GetFullPath($work)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if ($absoluteWork.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($absoluteWork).StartsWith('varkeep-package-', [StringComparison]::Ordinal)) {
        Remove-Item -LiteralPath $absoluteWork -Recurse -Force -ErrorAction SilentlyContinue
    }
}
