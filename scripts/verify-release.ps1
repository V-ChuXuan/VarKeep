[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$v2 = Join-Path $projectRoot 'v2'
$releaseExe = Join-Path $v2 'target\x86_64-pc-windows-msvc\release\varkeep.exe'
$publish = Join-Path $v2 'publish'

function Get-BackupManifest {
    param([string] $Root)

    if (-not (Test-Path -LiteralPath $Root)) {
        return @()
    }
    $rootItem = Get-Item -LiteralPath $Root -Force
    if (-not $rootItem.PSIsContainer -or
        ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'The local backups path is not a normal directory.'
    }
    $items = @(Get-ChildItem -LiteralPath $Root -Force -Recurse)
    if ($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) {
        throw 'The local backups path contains a reparse point.'
    }
    return @($items | Where-Object { -not $_.PSIsContainer } | Sort-Object FullName | ForEach-Object {
        [pscustomobject]@{
            Relative = [IO.Path]::GetRelativePath($Root, $_.FullName).Replace('\', '/')
            Length = $_.Length
            Hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
        }
    })
}

try {
    & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'verify.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'Development verification failed.'
    }

    Push-Location $v2
    try {
        $previousEncodedRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
        $separator = [char] 0x1f
        $env:CARGO_ENCODED_RUSTFLAGS = @(
            "--remap-path-prefix=$env:USERPROFILE=~",
            "--remap-path-prefix=$projectRoot=."
        ) -join $separator
        try {
            & cargo build --locked --release --target x86_64-pc-windows-msvc
            if ($LASTEXITCODE -ne 0) {
                throw 'Release build failed.'
            }
        }
        finally {
            if ($null -eq $previousEncodedRustFlags) {
                Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
            }
            else {
                $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedRustFlags
            }
        }
    }
    finally {
        Pop-Location
    }

    $bytes = [IO.File]::ReadAllBytes($releaseExe)
    if ($bytes.Length -lt 512 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw 'Release executable is not a valid PE file.'
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    $subsystem = [BitConverter]::ToUInt16($bytes, $peOffset + 24 + 68)
    if ($subsystem -ne 2) {
        throw "Release executable is not Windows GUI subsystem (found $subsystem)."
    }
    Write-Host 'PASS Windows GUI PE subsystem'

    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VarKeepIconResource {
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern uint ExtractIconEx(string file, int index, IntPtr[] large, IntPtr[] small, uint count);
}
'@
    $iconGroupCount = [VarKeepIconResource]::ExtractIconEx($releaseExe, -1, $null, $null, 0)
    if ($iconGroupCount -lt 1) {
        throw 'Release executable has no embedded Windows icon group.'
    }
    $versionInfo = (Get-Item -LiteralPath $releaseExe).VersionInfo
    if ($versionInfo.ProductName -ne 'VarKeep' -or
        $versionInfo.FileDescription -ne 'VarKeep - Windows environment variable snapshots' -or
        $versionInfo.OriginalFilename -ne 'varkeep.exe' -or
        $versionInfo.FileVersion -notlike '2.3.0*' -or
        $versionInfo.ProductVersion -notlike '2.3.0*') {
        throw 'Release executable is missing VarKeep Windows file metadata.'
    }
    Write-Host 'PASS VarKeep PE icon and file metadata'

    $sizeMiB = $bytes.Length / 1MB
    if ($sizeMiB -gt 40) {
        throw ('Release executable exceeds 40 MiB hard limit: {0:N2} MiB' -f $sizeMiB)
    }
    Write-Host ('PASS Release size {0:N2} MiB (WPF baseline 61.83 MiB)' -f $sizeMiB)

    $absolutePublish = [IO.Path]::GetFullPath($publish).TrimEnd('\')
    $expectedPublish = [IO.Path]::GetFullPath((Join-Path $v2 'publish')).TrimEnd('\')
    if ($absolutePublish -ne $expectedPublish -or -not $absolutePublish.StartsWith([IO.Path]::GetFullPath($v2), [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Publish path escaped the v2 project.'
    }
    $backupPath = Join-Path $absolutePublish 'backups'
    $backupManifestBefore = @(Get-BackupManifest $backupPath)
    if (Test-Path -LiteralPath $absolutePublish) {
        $existingPublish = Get-Item -LiteralPath $absolutePublish -Force
        if (($existingPublish.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            -not $existingPublish.PSIsContainer) {
            throw 'Existing publish path is not a normal directory.'
        }
        $allowedExisting = @(
            'varkeep.exe',
            'env-var-backup.exe',
            'LICENSE',
            'README.md',
            'THIRD-PARTY-LICENSES.txt',
            'THIRD-PARTY-NOTICES.md',
            'backups'
        )
        $unexpected = @(Get-ChildItem -LiteralPath $absolutePublish -Force | Where-Object {
            $_.Name -notin $allowedExisting -or
            ($_.Name -ne 'backups' -and ($_.PSIsContainer -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)))
        })
        if ($unexpected) {
            throw 'Publish contains an unexpected item; no release files were replaced.'
        }
        foreach ($releaseName in $allowedExisting | Where-Object { $_ -ne 'backups' }) {
            $releasePath = Join-Path $absolutePublish $releaseName
            if (Test-Path -LiteralPath $releasePath) {
                Remove-Item -LiteralPath $releasePath -Force
            }
        }
    }
    else {
        New-Item -ItemType Directory -Path $absolutePublish | Out-Null
    }
    Copy-Item -LiteralPath $releaseExe -Destination (Join-Path $absolutePublish 'varkeep.exe')
    Copy-Item -LiteralPath (Join-Path $v2 'README.md') -Destination $absolutePublish
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $absolutePublish
    Copy-Item -LiteralPath (Join-Path $v2 'THIRD-PARTY-LICENSES.txt') -Destination $absolutePublish
    Copy-Item -LiteralPath (Join-Path $v2 'THIRD-PARTY-NOTICES.md') -Destination $absolutePublish

    $backupManifestAfter = @(Get-BackupManifest $backupPath)
    if (Compare-Object $backupManifestBefore $backupManifestAfter -Property Relative, Length, Hash) {
        throw 'Local backups changed while replacing release files.'
    }
    Write-Host "PASS preserved local backup files ($($backupManifestAfter.Count))"

    $publishItems = @(Get-ChildItem -LiteralPath $absolutePublish -Force)
    $unexpectedDirectories = @($publishItems | Where-Object {
        $_.PSIsContainer -and $_.Name -ne 'backups'
    })
    if ($unexpectedDirectories) {
        throw 'Publish contains an unexpected directory.'
    }
    $actualFiles = @($publishItems | Where-Object { -not $_.PSIsContainer } | ForEach-Object {
        [IO.Path]::GetRelativePath($absolutePublish, $_.FullName).Replace('\', '/')
    } | Sort-Object)
    $expectedFiles = @('varkeep.exe', 'LICENSE', 'README.md', 'THIRD-PARTY-LICENSES.txt', 'THIRD-PARTY-NOTICES.md') | Sort-Object
    if (Compare-Object $expectedFiles $actualFiles) {
        throw 'Release package does not match the exact allowlist.'
    }
    $forbiddenNames = @('snapshot.json', 'user.ps1', 'system.ps1', 'all.ps1', 'restore-user-env.ps1', 'restore-machine-env.ps1')
    if ($publishItems | Where-Object { -not $_.PSIsContainer -and $_.Name -in $forbiddenNames }) {
        throw 'Release package root contains a sensitive backup artifact.'
    }

    $releaseText = [Text.Encoding]::ASCII.GetString($bytes)
    foreach ($forbiddenMarker in @('sentinel-ui-secret', 'ENV_VAR_BACKUP_TEST', '--test-')) {
        if ($releaseText.Contains($forbiddenMarker, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Release executable contains a forbidden test marker: $forbiddenMarker"
        }
    }
    foreach ($forbiddenBuildPath in @($env:USERPROFILE, $projectRoot)) {
        foreach ($spelling in @($forbiddenBuildPath, $forbiddenBuildPath.Replace('\', '/'))) {
            if ($releaseText.Contains($spelling, [StringComparison]::OrdinalIgnoreCase)) {
                throw "Release executable contains a local build path: $spelling"
            }
        }
    }
    Write-Host 'PASS release binary has no test-only markers'
    Write-Host 'PASS release binary has no local build paths'
    Write-Host 'PASS exact five-file release allowlist'
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
