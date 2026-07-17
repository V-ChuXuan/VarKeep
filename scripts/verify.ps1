[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$v2 = Join-Path $projectRoot 'v2'

function Invoke-Checked {
    param([string] $Name, [scriptblock] $Action)
    & $Action
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE."
    }
    Write-Host "PASS $Name"
}

function Assert-LocalMarkdownLinks {
    param([Parameter(Mandatory)][string[]] $Paths)

    foreach ($path in $Paths) {
        $file = Get-Item -LiteralPath $path
        $text = Get-Content -Raw -LiteralPath $file.FullName
        foreach ($match in [regex]::Matches($text, '!?(?:\[[^\]]*\])\((?<target>[^)]+)\)')) {
            $target = $match.Groups['target'].Value.Trim()
            if ($target -match '^(?i:https?://|mailto:|#)') {
                continue
            }
            $target = [Uri]::UnescapeDataString(($target -split '#', 2)[0])
            $resolved = [IO.Path]::GetFullPath((Join-Path $file.DirectoryName $target))
            if (-not $resolved.StartsWith([IO.Path]::GetFullPath($projectRoot), [StringComparison]::OrdinalIgnoreCase) -or
                -not (Test-Path -LiteralPath $resolved)) {
                throw "Broken or escaping local Markdown link in $($file.FullName): $target"
            }
        }
    }
}

try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -or
        -not (Get-Command rustc -ErrorAction SilentlyContinue) -or
        -not (Get-Command pwsh -ErrorAction SilentlyContinue)) {
        Write-Error 'Required tool missing: cargo, rustc, and pwsh are required.'
        exit 2
    }

    Invoke-Checked 'v1 gate' { & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'verify-v1.ps1') }

    $publicDocs = @(
        (Join-Path $projectRoot 'README.md'),
        (Join-Path $projectRoot 'CONTRIBUTING.md'),
        (Join-Path $projectRoot 'SECURITY.md'),
        (Join-Path $projectRoot 'v1\README.md'),
        (Join-Path $projectRoot 'v2\README.md')
    )
    Assert-LocalMarkdownLinks -Paths $publicDocs
    $publicDocText = ($publicDocs | ForEach-Object { Get-Content -Raw -LiteralPath $_ }) -join "`n"
    if ($publicDocText -match '(?m)^# env-var-backup\s*$' -or
        $publicDocText -match '(?i)publish[/\\]varkeep\.exe') {
        throw 'Public documentation contains a stale project title or local publish executable path.'
    }
    Write-Host 'PASS public documentation links and release entry points'

    $legacyRuntimeHits = Get-ChildItem -LiteralPath (Join-Path $v2 'src'), (Join-Path $v2 'ui') -File -Recurse |
        Select-String -Pattern '(?i)\bv1\b|legacy'
    if ($legacyRuntimeHits) {
        throw 'v2 runtime still contains a v1/legacy compatibility path.'
    }
    Write-Host 'PASS v1 and v2 runtime independence'

    $cargoFile = Get-Content -Raw (Join-Path $v2 'Cargo.toml')
    if ($cargoFile -notmatch '(?m)^name\s*=\s*"varkeep"\s*$') {
        throw 'The v2 Cargo package must be named varkeep.'
    }
    if ($cargoFile -notmatch '(?m)^version\s*=\s*"2\.3\.0"\s*$') {
        throw 'The v2 Cargo package version must be 2.3.0.'
    }
    if ($cargoFile -notmatch 'slint\s*=\s*\{[^\r\n]*version\s*=\s*"=1\.17\.1"') {
        throw 'Slint dependency is not pinned to =1.17.1.'
    }
    if ($cargoFile -notmatch 'winresource\s*=\s*"=0\.1\.31"') {
        throw 'winresource build dependency is not pinned to =0.1.31.'
    }
    if ($cargoFile -match '(?m)^\s*tokio\s*=') {
        throw 'Tokio must not be a direct dependency.'
    }
    Write-Host 'PASS pinned Slint dependency and no direct Tokio'

    $versionFiles = @(
        (Join-Path $projectRoot 'README.md'),
        (Join-Path $v2 'README.md'),
        (Join-Path $v2 'ui\app.slint'),
        (Join-Path $v2 'src\main.rs'),
        (Join-Path $v2 'src\ui_adapter.rs')
    )
    foreach ($versionFile in $versionFiles) {
        $versionText = Get-Content -Raw -LiteralPath $versionFile
        if ($versionText -match 'v2\.2' -or $versionText -match 'version\s*=\s*"2\.2\.0"') {
            throw "Stale v2.2 version marker remains in $versionFile"
        }
    }
    Write-Host 'PASS v2.3 product version consistency'

    Push-Location $v2
    try {
        $rustVersion = (& rustc --version)
        if ($LASTEXITCODE -ne 0 -or $rustVersion -notmatch '^rustc 1\.97\.0 ') {
            throw "Expected Rust 1.97.0 from rust-toolchain.toml; found: $rustVersion"
        }
        Write-Host 'PASS Rust 1.97.0'

        $metadata = (& cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json)
        if ($metadata.packages.Count -ne 1) {
            throw 'v2 must contain exactly one Cargo package.'
        }
        $tree = (& cargo tree --locked)
        if ($LASTEXITCODE -ne 0 -or ($tree -join "`n") -match '(?m)(^|\s)tokio v') {
            throw 'Dependency tree contains Tokio or could not be inspected.'
        }
        Write-Host 'PASS single package and dependency tree has no Tokio'

        Invoke-Checked 'cargo fmt' { & cargo fmt --all -- --check }
        Invoke-Checked 'cargo check' { & cargo check --locked --all-targets }
        Invoke-Checked 'cargo clippy' { & cargo clippy --locked --all-targets -- -D warnings }
        Invoke-Checked 'cargo test' { & cargo test --locked }
    }
    finally {
        Pop-Location
    }

    $uiFile = Get-Content -Raw (Join-Path $v2 'ui\app.slint')
    if ($uiFile -notmatch 'icon\s*:\s*@image-url\("\.\./assets/branding/varkeep-app-icon\.svg"\)') {
        throw 'The Slint window does not embed the VarKeep app icon.'
    }
    Write-Host 'PASS VarKeep Slint window icon'
    if ($uiFile -notmatch 'AboutSlint\s*\{[^}]*\}' -or
        $uiFile -notmatch 'about-visible' -or
        $uiFile -notmatch 'text:\s*root\.about-text' -or
        $uiFile -notmatch 'clicked\s*=>\s*\{\s*root\.about-visible\s*=\s*true') {
        throw 'AboutSlint is missing or is not directly reachable from top-level navigation.'
    }
    Write-Host 'PASS AboutSlint attribution and top-level reachability'

    if ($uiFile -notmatch 'callback\s+toggle-language\s*\(\s*\)' -or
        $uiFile -notmatch 'language-button-text' -or
        $uiFile -notmatch 'clicked\s*=>\s*\{\s*root\.toggle-language\(\)') {
        throw 'The top-level language switch is missing or not directly reachable.'
    }
    Write-Host 'PASS top-level Chinese/English language switch'

    $largeRadius = [regex]::Matches($uiFile, 'border-radius\s*:\s*(\d+)px') |
        Where-Object { [int] $_.Groups[1].Value -gt 5 }
    if ($largeRadius -or
        $uiFile -match 'drop-shadow' -or
        $uiFile -match '(linear|radial)-gradient') {
        throw 'The main UI must remain flat: no pills, decorative shadows, or gradients.'
    }
    if (([regex]::Matches($uiFile, 'primary\s*:\s*true')).Count -ne 0) {
        throw 'The main UI must use neutral Fluent buttons without primary: true.'
    }
    Write-Host 'PASS flat visual contract and neutral Fluent buttons'

    if ($uiFile -match 'scope-text' -or $uiFile -match 'text\s*:\s*row\.scope') {
        throw 'The backup list must not display a redundant scope column.'
    }
    if ($uiFile -notmatch 'width\s*:\s*230px\s*;\s*spacing\s*:\s*4px\s*;\s*padding-top\s*:\s*8px\s*;\s*padding-bottom\s*:\s*8px') {
        throw 'The three row actions must be vertically centered in the 44px backup row.'
    }
    Write-Host 'PASS compact backup list and centered row actions'

    foreach ($noteContract in @(
        'note:\s*string',
        'note-text',
        'add-note-text',
        'callback\s+request-note-edit\s*\(string\)',
        'callback\s+save-note\s*\(string\)',
        'component\s+FlatLineEdit',
        'TextInput',
        'init\s*=>\s*\{\s*note-input\.focus\(\)',
        'width\s*:\s*170px[\s\S]*text\s*:\s*root\.note-text[\s\S]*width\s*:\s*230px'
    )) {
        if ($uiFile -notmatch $noteContract) {
            throw "The backup note column/editor contract is missing: $noteContract"
        }
    }
    if ($uiFile -match 'import\s*\{[^}]*\bLineEdit\b' -or
        $uiFile -match ':=\s*LineEdit\s*\{') {
        throw 'The note editor must use the flat TextInput wrapper, not the styled LineEdit.'
    }
    Write-Host 'PASS backup note column and editor contract'

    foreach ($accessibilityContract in @(
        'accessible-label\s*:\s*root\.backup-time-text\s*\+\s*" "\s*\+\s*row\.timestamp',
        'changed\s+has-focus\s*=>',
        'if\s*\(self\.has-focus\)\s*\{\s*root\.activate-backup\(row\.id\)'
    )) {
        if ($uiFile -notmatch $accessibilityContract) {
            throw "The backup rows must preserve their keyboard and screen-reader contract: $accessibilityContract"
        }
    }
    Write-Host 'PASS backup row keyboard and screen-reader contract'

    $mainFile = Get-Content -Raw (Join-Path $v2 'src\main.rs')
    foreach ($setter in @(
        'set_window_title_text',
        'set_language_button_text',
        'set_about_text',
        'set_close_text',
        'set_create_text',
        'set_working_text',
        'set_view_summary_text',
        'set_open_location_text',
        'set_delete_text',
        'set_cancel_text',
        'set_backup_time_text',
        'set_note_text',
        'set_add_note_text',
        'set_edit_note_title_text',
        'set_note_placeholder_text',
        'set_save_text',
        'set_actions_text',
        'set_empty_text',
        'set_about_title_text',
        'set_about_body_text',
        'set_delete_title_text',
        'set_delete_prompt_text',
        'set_delete_irreversible_text',
        'set_delete_confirm_visible',
        'set_note_editor_visible',
        'set_note_editor_text',
        'set_selection_count',
        'set_selection_count_text',
        'set_compare_action_text',
        'set_comparison_title_text',
        'set_comparison_detail_text',
        'set_comparison_visible',
        'set_status_text',
        'set_status_tone',
        'set_status_visible',
        'set_backup_rows',
        'set_has_backups',
        'set_has_invalid_backups',
        'set_invalid_backup_text'
    )) {
        if (-not $mainFile.Contains($setter, [StringComparison]::Ordinal)) {
            throw "Localized UI property is not wired from Rust: $setter"
        }
    }
    Write-Host 'PASS localized static and dynamic UI property wiring'

    $ignore = Get-Content -Raw (Join-Path $projectRoot '.gitignore')
    foreach ($required in @('v1/backups/', 'v2/backups/', 'v2/publish/', 'v2/target/')) {
        if (-not $ignore.Contains($required, [StringComparison]::Ordinal)) {
            throw "Missing ignore rule: $required"
        }
    }
    if (Test-Path -LiteralPath (Join-Path $projectRoot '.git')) {
        $tracked = (& git -C $projectRoot ls-files)
        if ($tracked -match '(^|/)(v1|v2)/backups/') {
            throw 'A sensitive backup path is tracked by Git.'
        }
    }
    Write-Host 'PASS sensitive output ignore rules'

    foreach ($requiredRepositoryFile in @(
        '.github\workflows\ci.yml',
        '.github\workflows\release.yml',
        'CHANGELOG.md',
        'CONTRIBUTING.md',
        'SECURITY.md',
        'scripts\generate-third-party-licenses.ps1',
        'scripts\package-release.ps1',
        'scripts\verify-package.ps1',
        'scripts\verify-release-tag.ps1',
        'v2\THIRD-PARTY-LICENSES.txt',
        'v2\THIRD-PARTY-NOTICES.md'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $projectRoot $requiredRepositoryFile) -PathType Leaf)) {
            throw "Missing repository release file: $requiredRepositoryFile"
        }
    }
    $ciWorkflow = Get-Content -Raw (Join-Path $projectRoot '.github\workflows\ci.yml')
    if ($ciWorkflow -notmatch 'actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd\s+#\s+v6\.0\.2' -or
        $ciWorkflow -notmatch 'permissions:\s*\r?\n\s*contents:\s*read' -or
        $ciWorkflow -notmatch 'rustup toolchain install 1\.97\.0' -or
        $ciWorkflow -notmatch 'scripts\\verify-release\.ps1') {
        throw 'CI workflow is missing the pinned Windows release gate or read-only permission.'
    }
    $releaseWorkflow = Get-Content -Raw (Join-Path $projectRoot '.github\workflows\release.yml')
    if ($releaseWorkflow -notmatch 'tags:\s*\r?\n\s*-\s*"v\*"' -or
        $releaseWorkflow -notmatch 'contents:\s*write' -or
        $releaseWorkflow -notmatch 'persist-credentials:\s*false' -or
        $releaseWorkflow -notmatch 'actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd\s+#\s+v6\.0\.2' -or
        $releaseWorkflow -notmatch 'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a\s+#\s+v7\.0\.1' -or
        $releaseWorkflow -notmatch 'scripts\\verify-release-tag\.ps1' -or
        $releaseWorkflow -notmatch 'scripts\\package-release\.ps1' -or
        $releaseWorkflow -notmatch 'gh release create.+--verify-tag') {
        throw 'Release workflow is missing the tag gate, package verification, or hardened release command.'
    }
    Invoke-Checked 'third-party license inventory' {
        & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'generate-third-party-licenses.ps1') -Check
    }
    Write-Host 'PASS single-repository CI and tag release contracts'
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
