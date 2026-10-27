[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$v2 = Join-Path $projectRoot 'v2'

function Invoke-Checked {
    param([string] $Name, [scriptblock] $Action)
    & $Action
    if ($LASTEXITCODE -ne 0) { throw "$Name failed with exit code $LASTEXITCODE." }
    Write-Host "PASS $Name"
}

function Assert-LocalMarkdownLinks {
    param([string[]] $Paths)
    $rootPrefix = [IO.Path]::GetFullPath($projectRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    foreach ($path in $Paths) {
        $file = Get-Item -LiteralPath $path
        $text = Get-Content -Raw -LiteralPath $file.FullName
        foreach ($match in [regex]::Matches($text, '!?(?:\[[^\]]*\])\((?<target>[^)]+)\)')) {
            $target = $match.Groups['target'].Value.Trim()
            if ($target -match '^(?i:https?://|mailto:|#)') { continue }
            $target = [Uri]::UnescapeDataString(($target -split '#', 2)[0])
            $resolved = [IO.Path]::GetFullPath((Join-Path $file.DirectoryName $target))
            if (-not $resolved.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase) -or
                -not (Test-Path -LiteralPath $resolved)) {
                throw "Broken or escaping local Markdown link in $($file.Name): $target"
            }
        }
    }
}

try {
    foreach ($tool in @('cargo', 'rustc', 'pwsh')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Required tool missing: $tool" }
    }
    Invoke-Checked 'synthetic v1 suites' { & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'verify-v1.ps1') }

    $publicDocs = @('README.md', 'CONTRIBUTING.md', 'SECURITY.md', 'v1/README.md', 'v2/README.md') |
        ForEach-Object { Join-Path $projectRoot $_ }
    Assert-LocalMarkdownLinks -Paths $publicDocs
    Write-Host 'PASS public documentation links'

    Push-Location $v2
    try {
        $rustVersion = & rustc --version
        if ($LASTEXITCODE -ne 0 -or $rustVersion -notmatch '^rustc 1\.97\.0 ') {
            throw 'Expected the pinned Rust 1.97.0 toolchain.'
        }
        $metadataText = & cargo metadata --locked --no-deps --format-version 1
        if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
        $metadata = $metadataText | ConvertFrom-Json
        if ($metadata.packages.Count -ne 1 -or $metadata.packages[0].name -ne 'varkeep' -or
            $metadata.packages[0].version -ne '2.4.0') {
            throw 'Expected one varkeep 2.4.0 package.'
        }
        $expected = @{ serde = '=1.0.228'; serde_json = '=1.0.150'; 'windows-sys' = '=0.61.2'; winresource = '=0.1.31' }
        foreach ($dependency in $metadata.packages[0].dependencies) {
            if (-not $expected.ContainsKey($dependency.name) -or $dependency.req -ne $expected[$dependency.name]) {
                throw "Unexpected direct dependency or version: $($dependency.name)"
            }
        }
        if ($metadata.packages[0].dependencies.Count -ne $expected.Count) { throw 'A required dependency is missing.' }
        $tree = & cargo tree --locked
        if ($LASTEXITCODE -ne 0 -or ($tree -join "`n") -match '(?im)(?:^|\s)(?:slint|i-slint[^\s]*|tokio) v') {
            throw 'Dependency tree contains a retired GUI/runtime dependency or could not be inspected.'
        }
        Write-Host 'PASS pinned minimal native dependency graph'
        Invoke-Checked 'cargo fmt' { & cargo fmt --all -- --check }
        Invoke-Checked 'cargo check' { & cargo check --locked --all-targets }
        Invoke-Checked 'cargo clippy' { & cargo clippy --locked --all-targets -- -D warnings }
        Invoke-Checked 'cargo test (synthetic by default)' { & cargo test --locked }
    }
    finally { Pop-Location }

    [xml] $manifest = Get-Content -Raw -LiteralPath (Join-Path $v2 'app.manifest')
    $controls = $manifest.SelectNodes("//*[local-name()='assemblyIdentity' and @name='Microsoft.Windows.Common-Controls' and @version='6.0.0.0']")
    $execution = $manifest.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    $dpi = $manifest.SelectSingleNode("//*[local-name()='dpiAwareness']")
    if ($controls.Count -ne 1 -or $execution.level -ne 'asInvoker' -or $dpi.InnerText -ne 'PerMonitorV2') {
        throw 'Native manifest must request Common Controls v6, asInvoker and PerMonitorV2.'
    }
    Write-Host 'PASS native manifest contract'

    $ignore = Get-Content -Raw -LiteralPath (Join-Path $projectRoot '.gitignore')
    foreach ($required in @('v1/backups/', 'v2/backups/', 'v2/publish/', 'v2/target/')) {
        if (-not $ignore.Contains($required, [StringComparison]::Ordinal)) { throw "Missing ignore rule: $required" }
    }
    if (Test-Path -LiteralPath (Join-Path $projectRoot '.git')) {
        $tracked = & git -C $projectRoot ls-files
        if ($LASTEXITCODE -ne 0 -or $tracked -match '(^|/)(v1|v2)/backups/') {
            throw 'Cannot verify Git inventory or a sensitive backup path is tracked.'
        }
    }
    Write-Host 'PASS sensitive output ignore rules'
    Invoke-Checked 'third-party license inventory' {
        & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'generate-third-party-licenses.ps1') -Check
    }
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
