[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$v1 = Join-Path $projectRoot 'v1'

$expectedPrivateBackup = [ordered]@{
    'backups\env-backup-20260716-002718\snapshot.json' = '91262E417440E2242FB1785B5ED764C170924C1F111AB4E11DD5850CBD762108'
    'backups\env-backup-20260716-002718\summary.md' = 'F58284B0725F3ED5CC741F31DF835F25B5E7E98E25DB8B45583E37ADD05D76AC'
    'backups\env-backup-20260716-002718\summary.txt' = 'C93DF34B29E3635EEE01F0EB8A455070EA14F9CEF7CB6F2381A23466730CC349'
}

function Assert-ProtectedFiles {
    param([System.Collections.IDictionary] $Files)

    foreach ($entry in $Files.GetEnumerator()) {
        $path = Join-Path $v1 $entry.Key
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "v1 protected file is missing: $($entry.Key)"
        }
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
        if ($actual -ne $entry.Value) {
            throw "v1 protected file changed: $($entry.Key)"
        }
    }
}

try {
    $privateSnapshot = Join-Path $v1 'backups\env-backup-20260716-002718\snapshot.json'
    if (Test-Path -LiteralPath $privateSnapshot -PathType Leaf) {
        Assert-ProtectedFiles $expectedPrivateBackup
        Write-Host 'PASS v1 local private backup hashes (3/3)'
    }
    else {
        Write-Host 'SKIP v1 local private backup hashes (ignored files are absent)'
    }

    & pwsh -NoProfile -File (Join-Path $v1 'tests\run-tests.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw 'v1 regression suite failed.'
    }
    Write-Host 'PASS v1 regression suite (65/65)'
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
