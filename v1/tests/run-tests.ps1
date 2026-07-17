[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$scriptPath = Join-Path $projectRoot "backup-env.ps1"
$launcherPath = Join-Path $projectRoot "start-interactive.cmd"
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("env-var-backup-tests-{0}" -f [guid]::NewGuid().ToString("N"))
$secretName = "ENV_VAR_BACKUP_TEST_SECRET"
$originalSecret = [Environment]::GetEnvironmentVariable($secretName, "Process")
$secretBefore = "test-secret-before-{0}" -f [guid]::NewGuid().ToString("N")
$secretAfter = "test-secret-after-{0}" -f [guid]::NewGuid().ToString("N")
$script:passed = 0
$script:failed = 0

function Assert-True {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$Condition,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if ($Condition) {
        Write-Host "PASS: $Name"
        $script:passed++
        return
    }

    Write-Host "FAIL: $Name"
    $script:failed++
}

function Invoke-Tool {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $output = @(& pwsh -NoProfile -File $scriptPath @Arguments 2>&1)
    return [ordered]@{
        ExitCode = $LASTEXITCODE
        Output = $output
    }
}

try {
    New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
    [Environment]::SetEnvironmentVariable($secretName, $secretBefore, "Process")

    $help = Invoke-Tool -Arguments @("--help")
    Assert-True -Condition ($help.ExitCode -eq 0) -Name "help exits successfully"
    Assert-True -Condition (($help.Output -join "`n") -match "IncludeValuesInReports") -Name "help documents detailed report opt-in"

    $defaultRoot = Join-Path $testRoot "default"
    $backup = Invoke-Tool -Arguments @("backup", "-OutputRoot", $defaultRoot, "-Label", "default", "-Language", "en")
    Assert-True -Condition ($backup.ExitCode -eq 0) -Name "backup command exits successfully"

    $backupDirectories = @(Get-ChildItem -LiteralPath $defaultRoot -Directory -ErrorAction SilentlyContinue)
    Assert-True -Condition ($backupDirectories.Count -eq 1) -Name "backup creates one timestamped directory"

    if ($backupDirectories.Count -eq 1) {
        $backupDirectory = $backupDirectories[0]
        $snapshotPath = Join-Path $backupDirectory.FullName "snapshot.json"
        $summaryPath = Join-Path $backupDirectory.FullName "summary.md"
        $summaryTxtPath = Join-Path $backupDirectory.FullName "summary.txt"
        $userRestorePath = Join-Path $backupDirectory.FullName "restore/user.ps1"
        $machineRestorePath = Join-Path $backupDirectory.FullName "restore/system.ps1"
        $allRestorePath = Join-Path $backupDirectory.FullName "restore/all.ps1"
        $snapshotText = Get-Content -LiteralPath $snapshotPath -Raw
        $summaryText = Get-Content -LiteralPath $summaryPath -Raw

        Assert-True -Condition ($snapshotText.Contains($secretBefore)) -Name "raw snapshot retains environment values"
        Assert-True -Condition (-not $summaryText.Contains($secretBefore)) -Name "default summary hides environment values"
        Assert-True -Condition ($summaryText.Contains($secretName)) -Name "default summary retains variable names"
        Assert-True -Condition (-not $summaryText.Contains($env:USERNAME)) -Name "summary redacts the Windows user identity"
        Assert-True -Condition (-not (Test-Path -LiteralPath $summaryTxtPath)) -Name "backup no longer creates duplicate summary.txt"
        Assert-True -Condition (Test-Path -LiteralPath $userRestorePath) -Name "backup creates a user restore script by default"
        Assert-True -Condition (Test-Path -LiteralPath $machineRestorePath) -Name "backup creates a machine restore script by default"
        Assert-True -Condition (Test-Path -LiteralPath $allRestorePath) -Name "backup creates a combined restore script by default"
        $snapshot = $snapshotText | ConvertFrom-Json
        Assert-True -Condition (@($snapshot.environment.user | Where-Object { $_.kind -notin @("String", "ExpandString") }).Count -eq 0) -Name "snapshot records user registry value kinds"
        Assert-True -Condition (@($snapshot.environment.machine | Where-Object { $_.kind -notin @("String", "ExpandString") }).Count -eq 0) -Name "snapshot records machine registry value kinds"
        Assert-True -Condition (($backup.Output -join "`n").Contains($backupDirectory.FullName)) -Name "CLI backup output retains the absolute backup path"
        Assert-True -Condition (($backup.Output -join "`n").Contains($userRestorePath) -and ($backup.Output -join "`n").Contains($machineRestorePath) -and ($backup.Output -join "`n").Contains($allRestorePath)) -Name "CLI backup output lists all restore script paths"

        [Environment]::SetEnvironmentVariable($secretName, $secretAfter, "Process")
        $compareDefault = Invoke-Tool -Arguments @("compare", $backupDirectory.FullName, "-OutputRoot", $defaultRoot, "-Language", "en")
        Assert-True -Condition ($compareDefault.ExitCode -eq 0) -Name "default comparison exits successfully"
        $defaultCompareRoot = Join-Path (Join-Path $defaultRoot "comparisons") $backupDirectory.Name
        $defaultComparePath = @(Get-ChildItem -LiteralPath $defaultCompareRoot -Filter "compare-current-*.md" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1)
        if ($defaultComparePath.Count -eq 1) {
            $defaultCompareText = Get-Content -LiteralPath $defaultComparePath[0].FullName -Raw
            Assert-True -Condition (-not $defaultCompareText.Contains($secretBefore) -and -not $defaultCompareText.Contains($secretAfter)) -Name "default comparison hides old and current values"
        } else {
            Assert-True -Condition $false -Name "default comparison creates a report"
        }
    }

    [Environment]::SetEnvironmentVariable($secretName, $secretBefore, "Process")
    $detailedRoot = Join-Path $testRoot "detailed"
    $detailedBackup = Invoke-Tool -Arguments @("backup", "-OutputRoot", $detailedRoot, "-Label", "detailed", "-Language", "en", "-IncludeValuesInReports")
    Assert-True -Condition ($detailedBackup.ExitCode -eq 0) -Name "detailed backup switch is accepted"
    $detailedDirectories = @(Get-ChildItem -LiteralPath $detailedRoot -Directory -ErrorAction SilentlyContinue)
    if ($detailedDirectories.Count -eq 1) {
        $detailedSummary = Get-Content -LiteralPath (Join-Path $detailedDirectories[0].FullName "summary.md") -Raw
        Assert-True -Condition (-not $detailedSummary.Contains($secretBefore)) -Name "summary keeps sensitive values redacted even with detailed comparison reports enabled"

        [Environment]::SetEnvironmentVariable($secretName, $secretAfter, "Process")
        $detailedCompare = Invoke-Tool -Arguments @("compare", $detailedDirectories[0].FullName, "-OutputRoot", $detailedRoot, "-Language", "en", "-IncludeValuesInReports")
        Assert-True -Condition ($detailedCompare.ExitCode -eq 0) -Name "detailed comparison switch is accepted"
        $detailedCompareRoot = Join-Path (Join-Path $detailedRoot "comparisons") $detailedDirectories[0].Name
        $detailedComparePath = @(Get-ChildItem -LiteralPath $detailedCompareRoot -Filter "compare-current-*.md" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1)
        if ($detailedComparePath.Count -eq 1) {
            $detailedCompareText = Get-Content -LiteralPath $detailedComparePath[0].FullName -Raw
            Assert-True -Condition ($detailedCompareText.Contains($secretBefore) -and $detailedCompareText.Contains($secretAfter)) -Name "detailed comparison includes old and current values"
        } else {
            Assert-True -Condition $false -Name "detailed comparison creates a report"
        }
    } else {
        Assert-True -Condition $false -Name "detailed backup creates one timestamped directory"
    }

    $interactiveRoot = Join-Path $testRoot "interactive"
    $interactiveOutput = @("", "", "") | & pwsh -NoProfile -File $scriptPath -Interactive -Language en -OutputRoot $interactiveRoot 2>&1
    $interactiveExitCode = $LASTEXITCODE
    $interactiveDirectories = @(Get-ChildItem -LiteralPath $interactiveRoot -Directory -ErrorAction SilentlyContinue)
    Assert-True -Condition ($interactiveExitCode -eq 0) -Name "interactive quick backup exits successfully with redirected input"
    Assert-True -Condition ($interactiveDirectories.Count -eq 1) -Name "three default Enter presses create a quick backup and exit"
    Assert-True -Condition (($interactiveOutput -join "`n") -match "Quick backup") -Name "interactive flow presents the quick backup option"
    Assert-True -Condition (-not ($interactiveOutput -join "`n").Contains($interactiveRoot)) -Name "interactive backup output omits absolute paths"
    Assert-True -Condition (($interactiveOutput -join "`n") -match "restore[/\\]user\.ps1" -and ($interactiveOutput -join "`n") -match "restore[/\\]system\.ps1" -and ($interactiveOutput -join "`n") -match "restore[/\\]all\.ps1") -Name "interactive result lists all restore script names"
    Assert-True -Condition (($interactiveOutput -join "`n") -match "Next|接下来") -Name "interactive result presents post-backup actions"

    $menuOutput = @("6") | & pwsh -NoProfile -File $scriptPath -Interactive -Language en -OutputRoot $interactiveRoot 2>&1
    Assert-True -Condition (-not ($menuOutput -join "`n").Contains($interactiveRoot)) -Name "interactive main menu omits the absolute latest-backup path"
    Assert-True -Condition (($menuOutput -join "`n").Contains($interactiveDirectories[0].Name)) -Name "interactive main menu retains the latest backup name"

    $cancelRoot = Join-Path $testRoot "canceled-custom"
    $cancelInput = @("", "2", "", "", "2", "2")
    $cancelOutput = $cancelInput | & pwsh -NoProfile -File $scriptPath -Interactive -Language en -OutputRoot $cancelRoot 2>&1
    $cancelExitCode = $LASTEXITCODE
    $canceledDirectories = @(Get-ChildItem -LiteralPath $cancelRoot -Directory -ErrorAction SilentlyContinue)
    Assert-True -Condition ($cancelExitCode -eq 0) -Name "canceling custom backup exits successfully"
    Assert-True -Condition ($canceledDirectories.Count -eq 0) -Name "canceling custom backup creates no backup directory"
    Assert-True -Condition (($cancelOutput -join "`n") -match "Canceled") -Name "canceling custom backup reports the outcome"

    . $scriptPath help *> $null

    $fixtureRoot = Join-Path $testRoot "fixture-root"
    $fixtureDirectory = Join-Path $fixtureRoot "env-backup-20000101-000000-fixture"
    New-Item -ItemType Directory -Path $fixtureDirectory -Force | Out-Null
    $restoreSecretName = "ENV_VAR_BACKUP_RESTORE_TEST"
    $restoreSecretValue = "restore-test-'`$``-line1`r`nline2-{0}" -f [guid]::NewGuid().ToString("N")
    $fixture = [ordered]@{
        metadata = [ordered]@{
            schemaVersion = 2
            createdAtLocal = "2000-01-01 00:00:00 +00:00"
            createdAtUtc = "2000-01-01T00:00:00Z"
            language = "en"
        }
        environment = [ordered]@{
            process = @()
            user = @(
                [ordered]@{ name = $restoreSecretName; value = $restoreSecretValue; kind = "String" },
                [ordered]@{ name = "EXPANDED_PATH"; value = '%USERPROFILE%\bin'; kind = "ExpandString" },
                [ordered]@{ name = "EMPTY_VALUE"; value = ""; kind = "String" }
            )
            machine = @([ordered]@{ name = "SYSTEM_FIXTURE"; value = '%SystemRoot%'; kind = "ExpandString" })
        }
        pathBreakdown = [ordered]@{ process = @(); user = @(); machine = @() }
    }
    $fixture | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $fixtureDirectory "snapshot.json") -Encoding UTF8
    $storedFixture = Get-Content -LiteralPath (Join-Path $fixtureDirectory "snapshot.json") -Raw | ConvertFrom-Json
    [System.IO.File]::WriteAllText((Join-Path $fixtureDirectory "summary.md"), (New-SummaryMarkdown -Snapshot $storedFixture -SelectedLanguage "en"), [System.Text.UTF8Encoding]::new($false))
    [void](New-RestoreScripts -BackupDirectory (Get-Item -LiteralPath $fixtureDirectory) -Text (Get-UiText -SelectedLanguage "en") -Quiet $true)
    $beforeRestore = [Environment]::GetEnvironmentVariable($restoreSecretName, "User")
    $restore = Invoke-Tool -Arguments @("restore-script", $fixtureDirectory, "-OutputRoot", $fixtureRoot, "-Language", "en")
    $afterRestore = [Environment]::GetEnvironmentVariable($restoreSecretName, "User")
    $restoreScriptText = Get-Content -LiteralPath (Join-Path $fixtureDirectory "restore/user.ps1") -Raw
    $allRestoreText = Get-Content -LiteralPath (Join-Path $fixtureDirectory "restore/all.ps1") -Raw
    Assert-True -Condition ($restore.ExitCode -eq 0) -Name "restore-script command exits successfully"
    Assert-True -Condition (-not $restoreScriptText.Contains($restoreSecretValue)) -Name "restore script encodes values instead of embedding PowerShell literals"
    Assert-True -Condition ($restoreScriptText.Contains("RegistryValueKind]::ExpandString")) -Name "restore script preserves REG_EXPAND_SZ"
    Assert-True -Condition ($restoreScriptText.Contains("RegistryValueKind]::String")) -Name "restore script preserves REG_SZ and empty strings"
    Assert-True -Condition ($allRestoreText.IndexOf("Administrator") -lt $allRestoreText.IndexOf("SetValue")) -Name "combined restore script checks administrator rights before writes"
    Assert-True -Condition ($beforeRestore -eq $afterRestore) -Name "restore-script does not modify the environment"

    $astChecker = Join-Path $projectRoot "scripts/check-script-ast.ps1"
    if (-not (Test-Path -LiteralPath $astChecker -PathType Leaf)) {
        $astChecker = Join-Path (Split-Path -Parent $projectRoot) "scripts/check-script-ast.ps1"
    }
    $astPassed = $true
    foreach ($scriptName in @("user.ps1", "system.ps1", "all.ps1")) {
        & pwsh -NoProfile -File $astChecker -Path (Join-Path (Join-Path $fixtureDirectory "restore") $scriptName) *> $null
        $astPassed = $astPassed -and $LASTEXITCODE -eq 0
    }
    Assert-True -Condition $astPassed -Name "restore scripts pass the PowerShell AST allowlist"

    $isolatedKeyPath = "Software\VarKeepTests-v1-{0}" -f [guid]::NewGuid().ToString("N")
    $isolatedKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($isolatedKeyPath)
    $isolatedKey.Dispose()
    $isolatedScriptPath = Join-Path $testRoot "isolated-v1-restore.ps1"
    $isolatedScriptText = $restoreScriptText.Replace("OpenSubKey('Environment', `$true)", "OpenSubKey('$isolatedKeyPath', `$true)")
    [System.IO.File]::WriteAllText($isolatedScriptPath, $isolatedScriptText, [System.Text.UTF8Encoding]::new($false))
    try {
        & pwsh -NoProfile -File $isolatedScriptPath *> $null
        $isolatedExitCode = $LASTEXITCODE
        $isolatedKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($isolatedKeyPath)
        $isolatedValue = [string]$isolatedKey.GetValue($restoreSecretName, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $isolatedValueKind = $isolatedKey.GetValueKind($restoreSecretName).ToString()
        $isolatedExpandValue = [string]$isolatedKey.GetValue("EXPANDED_PATH", $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $isolatedExpandKind = $isolatedKey.GetValueKind("EXPANDED_PATH").ToString()
        $isolatedKey.Dispose()
        Assert-True -Condition ($isolatedExitCode -eq 0) -Name "v1 restore script executes against an isolated registry key"
        Assert-True -Condition ($isolatedValue -ceq $restoreSecretValue -and $isolatedValueKind -eq "String") -Name "v1 isolated execution preserves special characters and REG_SZ"
        Assert-True -Condition ($isolatedExpandValue -ceq '%USERPROFILE%\bin' -and $isolatedExpandKind -eq "ExpandString") -Name "v1 isolated execution preserves REG_EXPAND_SZ without expansion"
    } finally {
        [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($isolatedKeyPath, $false)
    }

    $nulSnapshot = $fixture | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    $nulSnapshot.environment.user[0].value = "a$([char]0)b"
    $nulRejected = $false
    try {
        Assert-SnapshotShape -Snapshot $nulSnapshot
    } catch {
        $nulRejected = $true
    }
    Assert-True -Condition $nulRejected -Name "v1 snapshot validation rejects NUL values"

    $urlFixture = [pscustomobject]@{ name = "SERVICE_URL"; value = "https://alice:password@example.com/path" }
    $urlPreview = Get-RedactedPreview -Item $urlFixture -SelectedLanguage "en"
    Assert-True -Condition ($urlPreview.Contains("https://***@example.com/path") -and -not $urlPreview.Contains("alice:password")) -Name "summary redacts URL user information"

    Assert-True -Condition (Test-BackupIntegrity -BackupDirectory $fixtureDirectory -Text (Get-UiText -SelectedLanguage "en") -Quiet $true) -Name "strict integrity accepts deterministic fixture"

    $extraArtifact = Join-Path $fixtureDirectory "extra.txt"
    Set-Content -LiteralPath $extraArtifact -Value "unexpected"
    Assert-True -Condition (-not (Test-BackupIntegrity -BackupDirectory $fixtureDirectory -Text (Get-UiText -SelectedLanguage "en") -Quiet $true)) -Name "strict integrity rejects extra artifacts"
    Remove-Item -LiteralPath $extraArtifact

    $systemScript = Join-Path $fixtureDirectory "restore/system.ps1"
    $heldSystemScript = Join-Path $testRoot "held-system.ps1"
    Move-Item -LiteralPath $systemScript -Destination $heldSystemScript
    Assert-True -Condition (-not (Test-BackupIntegrity -BackupDirectory $fixtureDirectory -Text (Get-UiText -SelectedLanguage "en") -Quiet $true)) -Name "strict integrity rejects missing artifacts"
    Move-Item -LiteralPath $heldSystemScript -Destination $systemScript

    $fixtureSummary = Join-Path $fixtureDirectory "summary.md"
    $heldSummary = Join-Path $testRoot "held-summary.md"
    Move-Item -LiteralPath $fixtureSummary -Destination $heldSummary
    New-Item -ItemType Directory -Path $fixtureSummary | Out-Null
    Assert-True -Condition (-not (Test-BackupIntegrity -BackupDirectory $fixtureDirectory -Text (Get-UiText -SelectedLanguage "en") -Quiet $true)) -Name "strict integrity rejects non-file artifacts"
    Remove-Item -LiteralPath $fixtureSummary
    Move-Item -LiteralPath $heldSummary -Destination $fixtureSummary

    $fixtureRestore = Join-Path $fixtureDirectory "restore"
    $heldRestore = Join-Path $testRoot "held-restore"
    Move-Item -LiteralPath $fixtureRestore -Destination $heldRestore
    New-Item -ItemType Junction -Path $fixtureRestore -Target $heldRestore | Out-Null
    Assert-True -Condition (-not (Test-BackupIntegrity -BackupDirectory $fixtureDirectory -Text (Get-UiText -SelectedLanguage "en") -Quiet $true)) -Name "strict integrity rejects reparse-point artifacts"
    Remove-Item -LiteralPath $fixtureRestore
    Move-Item -LiteralPath $heldRestore -Destination $fixtureRestore

    Add-Content -LiteralPath (Join-Path $fixtureDirectory "summary.md") -Value "tampered"
    Assert-True -Condition (-not (Test-BackupIntegrity -BackupDirectory $fixtureDirectory -Text (Get-UiText -SelectedLanguage "en") -Quiet $true)) -Name "strict integrity rejects a tampered summary"

    $rootEntryLimitDirectory = Join-Path $testRoot "root-entry-limit"
    New-Item -ItemType Directory -Path $rootEntryLimitDirectory | Out-Null
    0..$script:MaxRootEntries | ForEach-Object { Set-Content -LiteralPath (Join-Path $rootEntryLimitDirectory "entry-$_") -Value "" }
    $rootLimitRejected = $false
    try { [void](Get-BackupDirectories -RootPath $rootEntryLimitDirectory) } catch { $rootLimitRejected = $true }
    Assert-True -Condition $rootLimitRejected -Name "v1 backup listing bounds root entries"

    $candidateLimitDirectory = Join-Path $testRoot "candidate-limit"
    New-Item -ItemType Directory -Path $candidateLimitDirectory | Out-Null
    0..$script:MaxBackupCandidates | ForEach-Object { New-Item -ItemType Directory -Path (Join-Path $candidateLimitDirectory "env-backup-$_") | Out-Null }
    $candidateLimitRejected = $false
    try { [void](Get-BackupDirectories -RootPath $candidateLimitDirectory) } catch { $candidateLimitRejected = $true }
    Assert-True -Condition $candidateLimitRejected -Name "v1 backup listing bounds candidate directories"

    $directoryEntryLimit = Join-Path $testRoot "directory-entry-limit"
    New-Item -ItemType Directory -Path $directoryEntryLimit | Out-Null
    0..$script:MaxDirectoryEntries | ForEach-Object { Set-Content -LiteralPath (Join-Path $directoryEntryLimit "entry-$_") -Value "" }
    $directoryLimitRejected = $false
    try { [void](Test-ExactChildNames -Directory $directoryEntryLimit -ExpectedNames @()) } catch { $directoryLimitRejected = $true }
    Assert-True -Condition $directoryLimitRejected -Name "v1 integrity validation bounds backup directory entries"

    $launcherPauseCount = @(Select-String -LiteralPath $launcherPath -Pattern '^\s*pause\s*$').Count
    Assert-True -Condition ($launcherPauseCount -eq 1) -Name "launcher has only the missing-pwsh safety pause"

    Assert-True -Condition ((Get-DefaultLanguage -UiCultureName "zh-CN") -eq "zh") -Name "Chinese UI culture selects Chinese"
    Assert-True -Condition ((Get-DefaultLanguage -UiCultureName "en-US") -eq "en") -Name "non-Chinese UI culture selects English"
    $uiText = Get-UiText -SelectedLanguage "zh"
    Assert-True -Condition ($uiText.ContainsKey("MenuQuickBackup") -and $uiText.ContainsKey("MenuCustomBackup")) -Name "interactive UI exposes quick and custom backup choices"
    Assert-True -Condition ($uiText.ContainsKey("PostBackupPrompt") -and $uiText.ContainsKey("PostBackupExit") -and $uiText.ContainsKey("PostBackupOpen") -and $uiText.ContainsKey("PostBackupMainMenu")) -Name "interactive UI exposes post-backup actions"

    $normalExitWaitCalls = @(Select-String -LiteralPath $scriptPath -Pattern '^\s*Wait-BeforeInteractiveExit\s+-Text').Count
    Assert-True -Condition ($normalExitWaitCalls -eq 1) -Name "only the interactive error path waits before exit"

    $script:choiceQueue = [System.Collections.Generic.Queue[int]]::new()
    $script:openedPath = $null
    function Read-NumberChoice {
        param($Prompt, $Options, $DefaultChoice, $InvalidMessage)
        return $script:choiceQueue.Dequeue()
    }
    function Invoke-Item {
        param([string]$LiteralPath)
        $script:openedPath = $LiteralPath
    }

    $script:choiceQueue.Enqueue(1)
    $script:choiceQueue.Enqueue(3)
    $returnFlow = Start-InteractiveBackupFlow -DefaultOutputRoot (Join-Path $testRoot "return-flow") -SelectedLanguage "en" 6>$null
    Assert-True -Condition (-not $returnFlow.ExitRequested) -Name "post-backup main-menu action keeps the interactive session active"

    $script:choiceQueue.Enqueue(1)
    $script:choiceQueue.Enqueue(2)
    $openFlow = Start-InteractiveBackupFlow -DefaultOutputRoot (Join-Path $testRoot "open-flow") -SelectedLanguage "en" 6>$null
    Assert-True -Condition ($openFlow.ExitRequested) -Name "post-backup open action exits the interactive session"
    Assert-True -Condition ($script:openedPath -eq $openFlow.BackupDirectory) -Name "post-backup open action targets the new backup directory"

    function Get-RegistryEnvironment {
        throw "Injected registry read failure"
    }
    $failedRoot = Join-Path $testRoot "injected-failure"
    $failureWasObserved = $false
    try {
        [void](Invoke-EnvironmentBackup -BackupOutputRoot $failedRoot -SelectedLanguage "en")
    } catch {
        $failureWasObserved = $true
    }
    $failedDirectories = @(Get-ChildItem -LiteralPath $failedRoot -Directory -ErrorAction SilentlyContinue)
    Assert-True -Condition $failureWasObserved -Name "injected collection failure reaches the caller"
    Assert-True -Condition ($failedDirectories.Count -eq 0) -Name "failed backup removes its partial directory"
} finally {
    [Environment]::SetEnvironmentVariable($secretName, $originalSecret, "Process")
    if (Test-Path -LiteralPath $testRoot) {
        Remove-Item -LiteralPath $testRoot -Recurse -Force
    }
}

Write-Host ""
Write-Host ("Result: {0} passed, {1} failed" -f $script:passed, $script:failed)
if ($script:failed -gt 0) {
    exit 1
}
