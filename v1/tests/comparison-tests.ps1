[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path (Split-Path -Parent $PSScriptRoot) 'backup-env.ps1'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($scriptPath, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'The tool script could not be parsed.' }
# Load definitions and their existing bounds without running the CLI or reading host values.
foreach ($statement in $ast.EndBlock.Statements) {
    if ($statement -is [System.Management.Automation.Language.FunctionDefinitionAst] -or
        ($statement -is [System.Management.Automation.Language.AssignmentStatementAst] -and
         $statement.Left.Extent.Text -like '$script:Max*')) {
        Invoke-Expression $statement.Extent.Text
    }
}

$script:passed = 0
$script:failed = 0
function Assert-Comparison {
    param([bool]$Condition, [string]$Name)
    if ($Condition) { Write-Host "PASS: $Name"; $script:passed++ }
    else { Write-Host "FAIL: $Name"; $script:failed++ }
}

$baseline = @([pscustomobject]@{ name = 'DEMO'; value = 'Abc'; kind = 'String' })
$current = @([pscustomobject]@{ name = 'DEMO'; value = 'abc'; kind = 'String' })
$result = Compare-EnvironmentScope -ScopeName User -BackupItems $baseline -CurrentItems $current
Assert-Comparison (@($result.changed).Count -eq 1) 'case-only value changes are reported'
Assert-Comparison (-not (($result | ConvertTo-Json -Depth 5).Contains('Abc'))) 'default comparison hides values'
$result = Compare-EnvironmentScope -ScopeName User -BackupItems $baseline -CurrentItems $current -IncludeValues $true
Assert-Comparison (@($result.changed).Count -eq 1 -and @($result.changed)[0] -ceq 'DEMO | backup: Abc | current: abc') 'detailed changes keep exact old and current values'
$current = @([pscustomobject]@{ name = 'DEMO'; value = 'Abc'; kind = 'ExpandString' })
$result = Compare-EnvironmentScope -ScopeName User -BackupItems $baseline -CurrentItems $current
Assert-Comparison (@($result.changed).Count -eq 1) 'registry-kind changes are reported'
$current = @([pscustomobject]@{ name = 'demo'; value = 'Abc'; kind = 'String' })
$result = Compare-EnvironmentScope -ScopeName User -BackupItems $baseline -CurrentItems $current
Assert-Comparison (@($result.changed).Count -eq 0 -and @($result.added).Count -eq 0 -and @($result.removed).Count -eq 0) 'variable names remain case-insensitive'
$result = Compare-EnvironmentScope -ScopeName User -BackupItems @() -CurrentItems $baseline -IncludeValues $true
Assert-Comparison (@($result.added)[0] -ceq 'DEMO | current: Abc') 'detailed additions retain values'
$result = Compare-EnvironmentScope -ScopeName User -BackupItems $baseline -CurrentItems @() -IncludeValues $true
Assert-Comparison (@($result.removed)[0] -ceq 'DEMO | backup: Abc') 'detailed removals retain values'
$emptyValue = @([pscustomobject]@{ name = 'EMPTY'; value = ''; kind = 'String' })
$nullValue = @([pscustomobject]@{ name = 'EMPTY'; value = $null; kind = 'String' })
$result = Compare-EnvironmentScope -ScopeName User -BackupItems $emptyValue -CurrentItems $nullValue
Assert-Comparison (@($result.changed).Count -eq 0) 'empty and null helper values keep string semantics'
$result = Compare-EnvironmentScope -ScopeName User -BackupItems @() -CurrentItems @()
Assert-Comparison (@($result.added).Count -eq 0 -and @($result.removed).Count -eq 0 -and @($result.changed).Count -eq 0) 'empty scopes compare successfully'
try {
    Assert-Comparison ($null -eq (Find-EnvValue -Items @() -Name Path)) 'empty scopes have no PATH'
} catch { Assert-Comparison $false 'empty scopes have no PATH' }

function Get-ProcessEnvironment { @() }
function Get-RegistryEnvironment { param([string]$RegistryPath) @() }
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('varkeep-comparison-tests-' + [guid]::NewGuid().ToString('N'))
try {
    try {
        $backup = Invoke-EnvironmentBackup -BackupOutputRoot $testRoot -SelectedLanguage en -CompactOutput $true 6>$null
        $snapshot = Get-Content -Raw -LiteralPath $backup.SnapshotPath | ConvertFrom-Json
        Assert-Comparison (@($snapshot.environment.user).Count -eq 0 -and @($snapshot.environment.machine).Count -eq 0 -and @($snapshot.environment.process).Count -eq 0) 'backup accepts all empty synthetic scopes'
        Invoke-BackupComparison -BackupDirectory (Get-Item -LiteralPath $backup.BackupDirectory) -Text (Get-UiText -SelectedLanguage en) 6>$null | Out-Null
        $reports = @(Get-ChildItem -LiteralPath (Join-Path $testRoot 'comparisons') -Filter 'compare-current-*.md' -Recurse)
        Assert-Comparison ($reports.Count -eq 1) 'comparison accepts all empty synthetic scopes'
    } catch {
        Assert-Comparison $false 'empty-scope backup and comparison complete'
    }

    $capacityRoot = Join-Path $testRoot 'capacity'
    New-Item -ItemType Directory -Path $capacityRoot | Out-Null
    1..$script:MaxBackupCandidates | ForEach-Object {
        New-Item -ItemType Directory -Path (Join-Path $capacityRoot "env-backup-candidate-$_") | Out-Null
    }
    $script:capacityReaderCalls = 0
    function Get-ProcessEnvironment { $script:capacityReaderCalls++; @() }
    function Get-RegistryEnvironment { param([string]$RegistryPath) $script:capacityReaderCalls++; @() }
    $capacityRejected = $false
    $capacityError = ''
    try {
        [void](Invoke-EnvironmentBackup -BackupOutputRoot $capacityRoot -SelectedLanguage en -CompactOutput $true 6>$null)
    } catch {
        $capacityError = $_.Exception.Message
        $capacityRejected = $capacityError -match '64' -and $capacityError -match 'organize'
    }
    Assert-Comparison $capacityRejected 'creation rejects the 65th candidate with recovery guidance'
    Assert-Comparison (@(Get-ChildItem -LiteralPath $capacityRoot -Directory).Count -eq $script:MaxBackupCandidates) 'capacity rejection creates no backup directory'
    Assert-Comparison ($script:capacityReaderCalls -eq 0) 'capacity is checked before environment collection'
    Remove-Item -LiteralPath (Join-Path $capacityRoot "env-backup-candidate-$script:MaxBackupCandidates")
    $lastSlot = Invoke-EnvironmentBackup -BackupOutputRoot $capacityRoot -SelectedLanguage zh -CompactOutput $true 6>$null
    Assert-Comparison (@(Get-ChildItem -LiteralPath $capacityRoot -Directory).Count -eq $script:MaxBackupCandidates -and (Test-Path -LiteralPath $lastSlot.SnapshotPath)) 'the last available candidate slot can still be created'
    $chineseCapacityRejected = $false
    try {
        [void](Invoke-EnvironmentBackup -BackupOutputRoot $capacityRoot -SelectedLanguage zh -CompactOutput $true 6>$null)
    } catch {
        $chineseCapacityRejected = $_.Exception.Message -match '64' -and $_.Exception.Message -match '整理'
    }
    Assert-Comparison $chineseCapacityRejected 'Chinese capacity errors explain manual recovery'
} finally {
    $resolvedRoot = [IO.Path]::GetFullPath($testRoot)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
        -not ([IO.Path]::GetFileName($resolvedRoot)).StartsWith('varkeep-comparison-tests-')) {
        throw 'Test cleanup path is outside its temporary directory.'
    }
    if (Test-Path -LiteralPath $resolvedRoot) { Remove-Item -LiteralPath $resolvedRoot -Recurse -Force }
}
Write-Host "Synthetic comparison result: $script:passed passed, $script:failed failed"
if ($script:failed) { exit 1 }
