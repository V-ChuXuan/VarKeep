[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Command,
    [Parameter(Position = 1)]
    [string]$Target,
    [string]$OutputRoot,
    [string]$Label,
    [ValidateSet("zh", "en")]
    [string]$Language,
    [switch]$Interactive,
    [switch]$OpenOutputDirectory,
    [switch]$IncludeValuesInReports,
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptRoot = if (-not [string]::IsNullOrWhiteSpace($PSScriptRoot)) {
    $PSScriptRoot
} elseif (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
    Split-Path -Parent $PSCommandPath
} else {
    (Get-Location).Path
}

$scriptFilePath = if (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
    $PSCommandPath
} elseif ($null -ne $MyInvocation.MyCommand -and -not [string]::IsNullOrWhiteSpace($MyInvocation.MyCommand.Path)) {
    $MyInvocation.MyCommand.Path
} else {
    Join-Path $scriptRoot "backup-env.ps1"
}

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $scriptRoot "backups"
}

$script:MaxArtifactBytes = 16MB
$script:MaxRootEntries = 128
$script:MaxBackupCandidates = 64
$script:MaxDirectoryEntries = 64
$script:MaxVariablesPerScope = 4096

function Get-DefaultLanguage {
    param(
        [ValidateSet("", "zh", "en")]
        [string]$ExplicitLanguage = "",
        [string]$UiCultureName = [System.Globalization.CultureInfo]::CurrentUICulture.Name
    )

    if (-not [string]::IsNullOrWhiteSpace($ExplicitLanguage)) {
        return $ExplicitLanguage
    }

    if ($UiCultureName -like "zh*") {
        return "zh"
    }

    return "en"
}

function Read-TextWithDefault {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Prompt,
        [Parameter(Mandatory = $true)]
        [string]$DefaultValue
    )

    $value = Read-Host ("{0} [{1}]" -f $Prompt, $DefaultValue)
    if ([string]::IsNullOrWhiteSpace($value)) {
        return $DefaultValue
    }

    return $value.Trim()
}

function Read-NumberChoice {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Prompt,
        [Parameter(Mandatory = $true)]
        [string[]]$Options,
        [int]$DefaultChoice = 1,
        [string]$InvalidMessage = "Please enter a valid number."
    )

    while ($true) {
        Write-Host $Prompt
        for ($index = 0; $index -lt $Options.Count; $index++) {
            Write-Host ("  {0}. {1}" -f ($index + 1), $Options[$index])
        }

        $answer = Read-Host ("[{0}]" -f $DefaultChoice)
        if ([string]::IsNullOrWhiteSpace($answer)) {
            return $DefaultChoice
        }

        $selected = 0
        if ([int]::TryParse($answer.Trim(), [ref]$selected) -and $selected -ge 1 -and $selected -le $Options.Count) {
            return $selected
        }

        Write-Host $InvalidMessage
    }
}

function Get-UiText {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet("zh", "en")]
        [string]$SelectedLanguage
    )

    if ($SelectedLanguage -eq "zh") {
        return @{
            Title = "环境变量备份"
            Description = "将创建快照、摘要和三种范围的还原脚本。"
            ExistingBackups = "已有备份: {0}"
            LatestBackup = "最新备份: {0}"
            MainMenuPrompt = "请选择操作"
            MenuNewBackup = "新建备份"
            BackupModePrompt = "选择备份方式"
            MenuQuickBackup = "快速备份（使用默认设置）"
            MenuCustomBackup = "自定义备份"
            MenuBack = "返回"
            MenuOpenLatest = "打开最近备份目录"
            MenuViewSummary = "查看备份摘要"
            MenuCompare = "对比当前环境和备份"
            MenuRestore = "重新生成还原脚本（不执行）"
            MenuExit = "退出"
            NoBackups = "没有找到备份。"
            SelectBackup = "选择备份"
            OutputFolder = "备份输出目录"
            BackupLabel = "备份标签（可留空）"
            Yes = "是"
            No = "否"
            SummaryTitle = "备份确认:"
            SummaryOutputFolder = "  输出目录: {0}"
            SummaryLabel = "  标签: {0}"
            EmptyLabel = "（无）"
            CreateNow = "现在创建备份?"
            Canceled = "已取消。没有创建备份。"
            EnterNumber = "请输入有效数字。"
            Created = "备份已创建:"
            Files = "文件:"
            CompactSuccess = "备份完成，完整性检查通过。"
            CompactFiles = "文件: snapshot.json、summary.md、restore/user.ps1、restore/system.ps1、restore/all.ps1"
            PostBackupPrompt = "接下来"
            PostBackupExit = "退出"
            PostBackupOpen = "打开备份目录"
            PostBackupMainMenu = "返回主菜单"
            IntegrityPassed = "完整性检查通过。"
            IntegrityFailed = "完整性检查失败:"
            SensitiveDataWarning = "注意：snapshot.json 和还原脚本包含敏感环境变量值，请勿提交、同步或外发。"
            SummaryMissing = "摘要文件不存在。"
            CompareCreated = "对比报告已创建:"
            RestoreCreated = "还原脚本已创建:"
            PressEnterToExit = "按 Enter 退出"
            ErrorTitle = "执行失败:"
        }
    }

    return @{
        Title = "Environment Variable Backup"
        Description = "This creates a snapshot, summary, and three restore scopes."
        ExistingBackups = "Existing backups: {0}"
        LatestBackup = "Latest backup: {0}"
        MainMenuPrompt = "Choose an action"
        MenuNewBackup = "Create new backup"
        BackupModePrompt = "Choose a backup mode"
        MenuQuickBackup = "Quick backup (use defaults)"
        MenuCustomBackup = "Custom backup"
        MenuBack = "Back"
        MenuOpenLatest = "Open latest backup folder"
        MenuViewSummary = "View backup summary"
        MenuCompare = "Compare current environment with backup"
        MenuRestore = "Regenerate restore scripts (do not run)"
        MenuExit = "Exit"
        NoBackups = "No backups found."
        SelectBackup = "Select backup"
        OutputFolder = "Output folder"
        BackupLabel = "Backup label (optional)"
        Yes = "Yes"
        No = "No"
        SummaryTitle = "Backup summary:"
        SummaryOutputFolder = "  Output folder: {0}"
        SummaryLabel = "  Label: {0}"
        EmptyLabel = "(none)"
        CreateNow = "Create backup now?"
        Canceled = "Canceled. No backup was created."
        EnterNumber = "Please enter a valid number."
        Created = "Backup created:"
        Files = "Files:"
        CompactSuccess = "Backup completed and passed integrity validation."
        CompactFiles = "Files: snapshot.json, summary.md, restore/user.ps1, restore/system.ps1, restore/all.ps1"
        PostBackupPrompt = "Next"
        PostBackupExit = "Exit"
        PostBackupOpen = "Open backup folder"
        PostBackupMainMenu = "Return to main menu"
        IntegrityPassed = "Integrity check passed."
        IntegrityFailed = "Integrity check failed:"
        SensitiveDataWarning = "Warning: snapshot.json and restore scripts contain sensitive environment values. Do not commit, sync, or share them."
        SummaryMissing = "Summary file does not exist."
        CompareCreated = "Comparison report created:"
        RestoreCreated = "Restore scripts created:"
        PressEnterToExit = "Press Enter to exit"
        ErrorTitle = "Error:"
    }
}

function Read-OptionalText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Prompt
    )

    $value = Read-Host $Prompt
    if ([string]::IsNullOrWhiteSpace($value)) {
        return ""
    }

    return $value.Trim()
}

function Read-YesNoNumber {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Prompt,
        [Parameter(Mandatory = $true)]
        [hashtable]$Text,
        [bool]$DefaultYes = $true
    )

    $defaultChoice = if ($DefaultYes) { 1 } else { 2 }
    $choice = Read-NumberChoice `
        -Prompt $Prompt `
        -Options @($Text.Yes, $Text.No) `
        -DefaultChoice $defaultChoice `
        -InvalidMessage $Text.EnterNumber

    return $choice -eq 1
}

function Wait-BeforeInteractiveExit {
    param(
        [hashtable]$Text
    )

    if ($null -eq $Text) {
        $Text = Get-UiText -SelectedLanguage "zh"
    }

    if (-not [Console]::IsInputRedirected) {
        [void](Read-Host $Text.PressEnterToExit)
    }
}

function Show-Help {
    $scriptName = Split-Path -Leaf $scriptFilePath
    @(
        "env-var-backup",
        "",
        "Usage:",
        "  pwsh -File .\$scriptName                         # interactive menu",
        "  pwsh -File .\$scriptName --help                  # show help",
        "  pwsh -File .\$scriptName -Help                   # show help",
        "  pwsh -File .\$scriptName help                    # show help",
        "  pwsh -File .\$scriptName backup [-Label name]",
        "  pwsh -File .\$scriptName list",
        "  pwsh -File .\$scriptName open [latest|backup-dir]",
        "  pwsh -File .\$scriptName compare [latest|backup-dir]",
        "  pwsh -File .\$scriptName restore-script [latest|backup-dir]",
        "",
        "Options:",
        "  -OutputRoot <path>        Backup root. Default: .\backups",
        "  -Label <name>             Optional backup label for backup command",
        "  -Language zh|en           Console output language",
        "  -OpenOutputDirectory      Open backup folder after creating a backup",
        "  -IncludeValuesInReports   Include environment values in comparison reports only",
        "  -Interactive              Force interactive menu",
        "",
        "Notes:",
        "  Every backup generates separate user and machine restore scripts.",
        "  Generated restore scripts are not executed automatically.",
        "  snapshot.json and restore scripts contain real environment variable values.",
        "  Human-readable reports hide environment values unless explicitly requested."
    ) | ForEach-Object { Write-Host $_ }
}

function Show-BackupDirectorySummary {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath,
        [Parameter(Mandatory = $true)]
        [hashtable]$Text
    )

    if (-not (Test-Path -LiteralPath $RootPath)) {
        Write-Host ($Text.ExistingBackups -f 0)
        return
    }

    $existingBackups = @(Get-BackupDirectories -RootPath $RootPath)

    Write-Host ($Text.ExistingBackups -f $existingBackups.Count)
    if ($existingBackups.Count -gt 0) {
        Write-Host ($Text.LatestBackup -f $existingBackups[0].Name)
    }
}

function Get-BackupDirectories {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath
    )

    if (-not (Test-Path -LiteralPath $RootPath)) {
        return @()
    }

    $rootEntries = 0
    $candidateEntries = 0
    $candidates = [System.Collections.Generic.List[System.IO.DirectoryInfo]]::new()
    foreach ($entryPath in [System.IO.Directory]::EnumerateFileSystemEntries($RootPath)) {
        $rootEntries++
        if ($rootEntries -gt $script:MaxRootEntries) {
            throw "Backup root contains too many entries."
        }
        $entry = Get-Item -LiteralPath $entryPath -Force -ErrorAction Stop
        if (-not $entry.PSIsContainer -or $entry.Name -notlike "env-backup-*") {
            continue
        }
        $candidateEntries++
        if ($candidateEntries -gt $script:MaxBackupCandidates) {
            throw "Backup root contains too many backup candidates."
        }
        $candidates.Add($entry)
    }

    $validationText = Get-UiText -SelectedLanguage "en"
    return @(
        $candidates |
            Where-Object { Test-BackupIntegrity -BackupDirectory $_.FullName -Text $validationText -Quiet $true } |
            Sort-Object LastWriteTime -Descending
    )
}

function Read-BackupSelection {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath,
        [Parameter(Mandatory = $true)]
        [hashtable]$Text
    )

    $backups = @(Get-BackupDirectories -RootPath $RootPath)
    if ($backups.Count -eq 0) {
        Write-Host $Text.NoBackups
        return $null
    }

    $options = @(
        $backups |
            Select-Object -First 10 |
            ForEach-Object {
                "{0} ({1})" -f $_.Name, $_.LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss")
            }
    )
    $choice = Read-NumberChoice -Prompt $Text.SelectBackup -Options $options -DefaultChoice 1 -InvalidMessage $Text.EnterNumber
    return $backups[$choice - 1]
}

function Resolve-BackupTarget {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath,
        [string]$TargetValue
    )

    if ([string]::IsNullOrWhiteSpace($TargetValue) -or $TargetValue -eq "latest") {
        $latest = @(Get-BackupDirectories -RootPath $RootPath | Select-Object -First 1)
        if ($latest.Count -eq 0) {
            return $null
        }

        return $latest[0]
    }

    $candidate = if ([System.IO.Path]::IsPathRooted($TargetValue)) {
        $TargetValue
    } else {
        Join-Path $RootPath $TargetValue
    }

    if (-not (Test-Path -LiteralPath $candidate -PathType Container)) {
        return $null
    }
    $validationText = Get-UiText -SelectedLanguage "en"
    if (-not (Test-BackupIntegrity -BackupDirectory $candidate -Text $validationText -Quiet $true)) {
        return $null
    }
    return Get-Item -LiteralPath $candidate
}

function Show-BackupList {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath,
        [int]$Limit = 20
    )

    $backups = @(Get-BackupDirectories -RootPath $RootPath | Select-Object -First $Limit)
    if ($backups.Count -eq 0) {
        Write-Host "No backups found."
        return
    }

    foreach ($backup in $backups) {
        Write-Host ("{0}`t{1}`t{2}" -f $backup.Name, $backup.LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss"), $backup.FullName)
    }
}

function Show-BackupSummary {
    param(
        [Parameter(Mandatory = $true)]
        [System.IO.DirectoryInfo]$BackupDirectory,
        [Parameter(Mandatory = $true)]
        [hashtable]$Text
    )

    $summaryPath = Join-Path $BackupDirectory.FullName "summary.md"
    if (-not (Test-Path -LiteralPath $summaryPath)) {
        Write-Host $Text.SummaryMissing
        return
    }

    Write-Host ""
    Get-Content -LiteralPath $summaryPath | ForEach-Object { Write-Host $_ }
}

function Test-BackupArtifactFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }
    $item = Get-Item -LiteralPath $Path -Force
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        return $false
    }
    return $item.Length -le $script:MaxArtifactBytes
}

function Test-NoReparseComponents {
    param([Parameter(Mandatory = $true)][string]$Path)

    $current = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    while ($null -ne $current) {
        if (($current.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            return $false
        }
        $current = $current.Parent
    }
    return $true
}

function Test-ExactChildNames {
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$ExpectedNames
    )

    $actual = [System.Collections.Generic.List[string]]::new()
    foreach ($entryPath in [System.IO.Directory]::EnumerateFileSystemEntries($Directory)) {
        if ($actual.Count -ge $script:MaxDirectoryEntries) {
            throw "Backup directory contains too many entries."
        }
        $actual.Add([System.IO.Path]::GetFileName($entryPath))
    }
    $actual = @($actual | Sort-Object)
    $expected = @($ExpectedNames | Sort-Object)
    return $null -eq (Compare-Object -ReferenceObject $expected -DifferenceObject $actual)
}

function Assert-SnapshotShape {
    param([Parameter(Mandatory = $true)][object]$Snapshot)

    if ($null -eq $Snapshot.metadata -or [int]$Snapshot.metadata.schemaVersion -ne 2) {
        throw "Unsupported snapshot schema."
    }
    if ([string]$Snapshot.metadata.language -notin @("zh", "en")) {
        throw "Invalid snapshot language."
    }
    foreach ($scope in @("process", "user", "machine")) {
        if ($null -eq $Snapshot.environment.$scope -or $null -eq $Snapshot.pathBreakdown.$scope) {
            throw "Missing snapshot scope '$scope'."
        }
        $names = @{}
        if (@($Snapshot.environment.$scope).Count -gt $script:MaxVariablesPerScope) {
            throw "Too many variables in '$scope'."
        }
        foreach ($item in @($Snapshot.environment.$scope)) {
            $properties = @($item.PSObject.Properties.Name)
            if ("name" -notin $properties -or "value" -notin $properties -or [string]::IsNullOrEmpty([string]$item.name) -or [string]$item.name -match "[=`0]") {
                throw "Invalid variable in '$scope'."
            }
            if (([string]$item.value).Contains([char]0)) {
                throw "Variable value contains NUL in '$scope'."
            }
            $normalized = ([string]$item.name).ToUpperInvariant()
            if ($names.ContainsKey($normalized)) {
                throw "Duplicate variable in '$scope'."
            }
            $names[$normalized] = $true
            if ($scope -eq "process") {
                if ("kind" -notin $properties -or [string]$item.kind -ne "Process") {
                    throw "Invalid process variable kind."
                }
            } elseif ("kind" -notin $properties -or [string]$item.kind -notin @("String", "ExpandString")) {
                throw "Invalid registry variable kind."
            }
        }
    }
}

function Test-BackupIntegrity {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BackupDirectory,
        [Parameter(Mandatory = $true)]
        [hashtable]$Text,
        [bool]$Quiet = $false
    )

    $errors = @()
    try {
        if (-not (Test-NoReparseComponents -Path $BackupDirectory)) {
            throw "Backup path contains a reparse point."
        }
        $directoryItem = Get-Item -LiteralPath $BackupDirectory -Force -ErrorAction Stop
        if (-not $directoryItem.PSIsContainer -or ($directoryItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Backup path is not a regular directory."
        }
        if (-not (Test-ExactChildNames -Directory $BackupDirectory -ExpectedNames @("snapshot.json", "summary.md", "restore"))) {
            throw "Unexpected backup layout."
        }
        $restoreDirectory = Join-Path $BackupDirectory "restore"
        $restoreItem = Get-Item -LiteralPath $restoreDirectory -Force -ErrorAction Stop
        if (-not $restoreItem.PSIsContainer -or ($restoreItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Restore path is not a regular directory."
        }
        if (-not (Test-ExactChildNames -Directory $restoreDirectory -ExpectedNames @("user.ps1", "system.ps1", "all.ps1"))) {
            throw "Unexpected restore layout."
        }

        $paths = @(
            (Join-Path $BackupDirectory "snapshot.json"),
            (Join-Path $BackupDirectory "summary.md"),
            (Join-Path $restoreDirectory "user.ps1"),
            (Join-Path $restoreDirectory "system.ps1"),
            (Join-Path $restoreDirectory "all.ps1")
        )
        foreach ($path in $paths) {
            if (-not (Test-BackupArtifactFile -Path $path)) {
                throw "Invalid artifact file: $([System.IO.Path]::GetFileName($path))"
            }
        }

        $snapshot = [System.IO.File]::ReadAllText($paths[0]) | ConvertFrom-Json
        Assert-SnapshotShape -Snapshot $snapshot
        $language = [string]$snapshot.metadata.language
        if ([System.IO.File]::ReadAllText($paths[1]) -cne (New-SummaryMarkdown -Snapshot $snapshot -SelectedLanguage $language)) {
            throw "Summary content does not match snapshot."
        }
        $expectedScripts = @(
            (New-RestoreScript -Snapshot $snapshot -Scopes @("user")),
            (New-RestoreScript -Snapshot $snapshot -Scopes @("machine")),
            (New-RestoreScript -Snapshot $snapshot -Scopes @("user", "machine"))
        )
        for ($index = 0; $index -lt $expectedScripts.Count; $index++) {
            if ([System.IO.File]::ReadAllText($paths[$index + 2]) -cne $expectedScripts[$index]) {
                throw "Restore script content does not match snapshot."
            }
        }
    } catch {
        $errors += $_.Exception.Message
    }

    if ($errors.Count -eq 0) {
        if (-not $Quiet) {
            Write-Host $Text.IntegrityPassed
        }
        return $true
    }

    if (-not $Quiet) {
        Write-Host $Text.IntegrityFailed
        $errors | ForEach-Object { Write-Host "  $_" }
    }
    return $false
}

function Get-RegistryEnvironment {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RegistryPath
    )

    if (-not (Test-Path $RegistryPath)) {
        return @()
    }

    $key = Get-Item -LiteralPath $RegistryPath
    try {
        return @(
            $key.GetValueNames() |
                Sort-Object |
                ForEach-Object {
                    $name = [string]$_
                    if ([string]::IsNullOrEmpty($name) -or $name.Contains("=") -or $name.Contains([char]0)) {
                        return
                    }
                    $kind = $key.GetValueKind($name).ToString()
                    if ($kind -notin @("String", "ExpandString")) {
                        throw "Unsupported registry value kind '$kind' for environment variable '$name'."
                    }
                    [ordered]@{
                        name  = $name
                        value = [string]$key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                        kind  = $kind
                    }
                }
        )
    } finally {
        $key.Dispose()
    }
}

function Split-PathEntries {
    param(
        [AllowNull()]
        [string]$PathValue
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return @()
    }

    return @(
        $PathValue.Split(";", [System.StringSplitOptions]::RemoveEmptyEntries) |
            ForEach-Object { $_.Trim() } |
            Where-Object { $_ } |
            Select-Object -Unique
    )
}

function Get-ProcessEnvironment {
    return @(
        Get-ChildItem Env: |
            Sort-Object Name |
            ForEach-Object {
                [ordered]@{
                    name  = $_.Name
                    value = [string]$_.Value
                    kind  = "Process"
                }
            }
    )
}

function Find-EnvValue {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Items,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $match = $Items | Where-Object { $_.name -eq $Name } | Select-Object -First 1
    if ($null -eq $match) {
        return $null
    }

    return [string]$match.value
}

function ConvertTo-EnvironmentMap {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [object[]]$Items
    )

    $map = @{}
    foreach ($item in $Items) {
        $map[[string]$item.name] = [string]$item.value
    }

    return $map
}

function ConvertTo-ReportValue {
    param(
        [AllowNull()]
        [string]$Value
    )

    if ($null -eq $Value) {
        return ""
    }

    return $Value.Replace("`r", "\r").Replace("`n", "\n")
}

function ConvertTo-MarkdownCell {
    param([AllowNull()][string]$Value)

    if ($null -eq $Value) {
        return ""
    }

    return $Value.Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;").Replace("|", "&#124;").Replace("`r", " ").Replace("`n", " ").Replace("`t", " ")
}

function Test-SensitiveVariableName {
    param([Parameter(Mandatory = $true)][string]$Name)

    $normalized = $Name.ToUpperInvariant()
    $tokens = @($normalized -split '[^A-Z0-9]+' | Where-Object { $_ })
    $sensitiveTokens = @(
        "APIKEY", "ACCESSKEY", "AUTHORIZATION", "CLIENTSECRET", "CONNECTIONSTRING",
        "CREDENTIAL", "CREDENTIALS", "PASSWORD", "PASSWD", "PRIVATEKEY", "SECRET", "TOKEN"
    )
    if (@($tokens | Where-Object { $_ -in $sensitiveTokens }).Count -gt 0) {
        return $true
    }

    if ($normalized -eq "KEY" -or $normalized.EndsWith("_KEY")) {
        return $true
    }

    $pairs = @("API KEY", "PRIVATE KEY", "ACCESS KEY", "CLIENT SECRET", "CONNECTION STRING")
    $joined = $tokens -join " "
    return @($pairs | Where-Object { $joined.Contains($_) }).Count -gt 0
}

function Protect-IdentityPath {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)

    $redacted = [regex]::Replace($Value, '(?i)([\\/]+Users[\\/]+)[^\\/;]+', '${1}***')
    if ($redacted -match '^(?i:https?://)') {
        $redacted = [regex]::Replace($redacted, '(?i)^(https?://)[^/@]+@', '${1}***@')
        $boundary = $redacted.IndexOfAny([char[]]@('?', '#'))
        if ($boundary -ge 0) {
            $redacted = $redacted.Substring(0, $boundary + 1) + "***"
        }
    }
    return $redacted
}

function Get-RedactedPreview {
    param(
        [Parameter(Mandatory = $true)][object]$Item,
        [Parameter(Mandatory = $true)][ValidateSet("zh", "en")][string]$SelectedLanguage
    )

    $name = [string]$Item.name
    $value = [string]$Item.value
    $length = $value.Length
    if ($length -eq 0) {
        return $(if ($SelectedLanguage -eq "zh") { "（空）" } else { "(empty)" })
    }
    if (Test-SensitiveVariableName -Name $name) {
        $detail = if ($SelectedLanguage -eq "zh") { "（疑似敏感值，$length 字符）" } else { " (suspected sensitive value, $length characters)" }
        return "<code>●●●</code>$detail"
    }

    $safe = Protect-IdentityPath -Value $value
    $pathLike = $safe -match '(?i)(^|;)\s*(?:[A-Z]:[\\/]|\\\\|//|%[^%]+%)'
    if ($pathLike -or $safe -match '^(?i:https?://)') {
        $preview = if ($safe.Length -gt 104) { $safe.Substring(0, 104) + "…" } else { $safe }
        $detail = if ($safe.Length -gt 104) {
            if ($SelectedLanguage -eq "zh") { "（$length 字符）" } else { " ($length characters)" }
        } else { "" }
        return "<code>$(ConvertTo-MarkdownCell -Value $preview)</code>$detail"
    }

    $masked = if ($length -le 4) {
        "●" * [Math]::Max($length, 1)
    } else {
        $value.Substring(0, 2) + "●●●" + $value.Substring($length - 2, 2)
    }
    $plainDetail = if ($SelectedLanguage -eq "zh") { "（$length 字符）" } else { " ($length characters)" }
    return "<code>$(ConvertTo-MarkdownCell -Value $masked)</code>$plainDetail"
}

function New-SummaryMarkdown {
    param(
        [Parameter(Mandatory = $true)][object]$Snapshot,
        [Parameter(Mandatory = $true)][ValidateSet("zh", "en")][string]$SelectedLanguage
    )

    $lines = [System.Collections.Generic.List[string]]::new()
    $isChinese = $SelectedLanguage -eq "zh"
    $lines.Add($(if ($isChinese) { "# VarKeep v1 备份摘要" } else { "# VarKeep v1 Backup Summary" }))
    $lines.Add("")
    $lines.Add($(if ($isChinese) { "| 创建时间 | 进程变量 | 用户变量 | 系统变量 |" } else { "| Created | Process variables | User variables | System variables |" }))
    $lines.Add("| --- | ---: | ---: | ---: |")
    $lines.Add("| $($Snapshot.metadata.createdAtLocal) | $(@($Snapshot.environment.process).Count) | $(@($Snapshot.environment.user).Count) | $(@($Snapshot.environment.machine).Count) |")

    foreach ($scope in @("process", "user", "machine")) {
        $lines.Add("")
        $heading = if ($isChinese) {
            @{ process = "## 进程变量"; user = "## 用户变量"; machine = "## 系统变量" }[$scope]
        } else {
            @{ process = "## Process variables"; user = "## User variables"; machine = "## System variables" }[$scope]
        }
        $lines.Add($heading)
        $lines.Add("")
        $lines.Add($(if ($isChinese) { "| 变量 | 类型 | 脱敏值 |" } else { "| Variable | Type | Redacted value |" }))
        $lines.Add("| --- | --- | --- |")
        foreach ($item in @($Snapshot.environment.$scope | Sort-Object name)) {
            $kind = if ($scope -eq "process") { "PROCESS" } elseif ([string]$item.kind -eq "ExpandString") { "REG_EXPAND_SZ" } else { "REG_SZ" }
            $name = ConvertTo-MarkdownCell -Value ([string]$item.name)
            $preview = Get-RedactedPreview -Item $item -SelectedLanguage $SelectedLanguage
            $lines.Add("| <code>$name</code> | <code>$kind</code> | $preview |")
        }
    }

    $lines.Add("")
    $lines.Add($(if ($isChinese) { "`●●●` 敏感值已隐藏　·　`***` 身份信息已隐藏　·　`…` 内容已截断" } else { "`●●●` sensitive value hidden · `***` identity hidden · `…` content truncated" }))
    return ($lines -join [Environment]::NewLine) + [Environment]::NewLine
}

function Compare-EnvironmentScope {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ScopeName,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [object[]]$BackupItems,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [object[]]$CurrentItems,
        [bool]$IncludeValues = $false
    )

    $backupMap = ConvertTo-EnvironmentMap -Items $BackupItems
    $currentMap = ConvertTo-EnvironmentMap -Items $CurrentItems
    $backupNames = @($backupMap.Keys | Sort-Object)
    $currentNames = @($currentMap.Keys | Sort-Object)
    $addedNames = @($currentNames | Where-Object { -not $backupMap.ContainsKey($_) })
    $removedNames = @($backupNames | Where-Object { -not $currentMap.ContainsKey($_) })
    $changedNames = @(
        $backupNames |
            Where-Object { $currentMap.ContainsKey($_) -and $backupMap[$_] -ne $currentMap[$_] }
    )
    $added = if ($IncludeValues) {
        @($addedNames | ForEach-Object { "{0} | current: {1}" -f $_, (ConvertTo-ReportValue -Value $currentMap[$_]) })
    } else {
        $addedNames
    }
    $removed = if ($IncludeValues) {
        @($removedNames | ForEach-Object { "{0} | backup: {1}" -f $_, (ConvertTo-ReportValue -Value $backupMap[$_]) })
    } else {
        $removedNames
    }
    $changed = if ($IncludeValues) {
        @(
            $changedNames |
                ForEach-Object {
                    "{0} | backup: {1} | current: {2}" -f `
                        $_, `
                        (ConvertTo-ReportValue -Value $backupMap[$_]), `
                        (ConvertTo-ReportValue -Value $currentMap[$_])
                }
        )
    } else {
        $changedNames
    }

    return [ordered]@{
        scope = $ScopeName
        added = $added
        removed = $removed
        changed = $changed
    }
}

function Compare-PathScope {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ScopeName,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [object[]]$BackupEntries,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [object[]]$CurrentEntries
    )

    $backupSet = @{}
    foreach ($entry in $BackupEntries) {
        $backupSet[[string]$entry] = $true
    }

    $currentSet = @{}
    foreach ($entry in $CurrentEntries) {
        $currentSet[[string]$entry] = $true
    }

    return [ordered]@{
        scope = $ScopeName
        added = @($currentSet.Keys | Where-Object { -not $backupSet.ContainsKey($_) } | Sort-Object)
        removed = @($backupSet.Keys | Where-Object { -not $currentSet.ContainsKey($_) } | Sort-Object)
    }
}

function Add-ComparisonSection {
    param(
        [Parameter(Mandatory = $true)]
        [System.Collections.ArrayList]$Lines,
        [Parameter(Mandatory = $true)]
        [string]$Title,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [object[]]$Items
    )

    [void]$Lines.Add("")
    [void]$Lines.Add("### $Title")
    [void]$Lines.Add("")
    if ($Items.Count -eq 0) {
        [void]$Lines.Add("- (none)")
        return
    }

    foreach ($item in $Items) {
        [void]$Lines.Add("- $item")
    }
}

function Get-EnvironmentDisplayLines {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Items,
        [bool]$IncludeValues = $false
    )

    return @(
        foreach ($item in ($Items | Sort-Object name)) {
            $name = [string]$item.name
            if (-not $IncludeValues) {
                $name
                continue
            }

            $value = if ($null -eq $item.value) {
                ""
            } else {
                [string]$item.value
            }
            $normalizedValue = $value -replace "`r`n", "`n"

            if ($normalizedValue.Contains("`n")) {
                "{0}<<VALUE" -f $name
                $normalizedValue.Split("`n")
                "VALUE"
            } else {
                "{0}={1}" -f $name, $normalizedValue
            }
        }
    )
}

function Format-MarkdownCodeBlock {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Lines
    )

    $content = if ($Lines.Count -gt 0) {
        $Lines
    } else {
        @("(none)")
    }

    return @('```text') + $content + @('```')
}

function Invoke-BackupComparison {
    param(
        [Parameter(Mandatory = $true)]
        [System.IO.DirectoryInfo]$BackupDirectory,
        [Parameter(Mandatory = $true)]
        [hashtable]$Text,
        [bool]$IncludeValues = $false
    )

    if (-not (Test-BackupIntegrity -BackupDirectory $BackupDirectory.FullName -Text $Text -Quiet $true)) {
        throw "Backup integrity validation failed."
    }
    $snapshotPath = Join-Path $BackupDirectory.FullName "snapshot.json"
    $snapshot = Get-Content -LiteralPath $snapshotPath -Raw | ConvertFrom-Json
    $currentProcessEnv = @(Get-ProcessEnvironment)
    $currentUserEnv = @(Get-RegistryEnvironment -RegistryPath "HKCU:\Environment")
    $currentMachineEnv = @(Get-RegistryEnvironment -RegistryPath "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
    $currentPath = [ordered]@{
        process = @(Split-PathEntries -PathValue (Find-EnvValue -Items $currentProcessEnv -Name "Path"))
        user = @(Split-PathEntries -PathValue (Find-EnvValue -Items $currentUserEnv -Name "Path"))
        machine = @(Split-PathEntries -PathValue (Find-EnvValue -Items $currentMachineEnv -Name "Path"))
    }

    $envComparisons = @(
        Compare-EnvironmentScope -ScopeName "Process" -BackupItems @($snapshot.environment.process) -CurrentItems $currentProcessEnv -IncludeValues $IncludeValues
        Compare-EnvironmentScope -ScopeName "User" -BackupItems @($snapshot.environment.user) -CurrentItems $currentUserEnv -IncludeValues $IncludeValues
        Compare-EnvironmentScope -ScopeName "Machine" -BackupItems @($snapshot.environment.machine) -CurrentItems $currentMachineEnv -IncludeValues $IncludeValues
    )
    $pathComparisons = @(
        Compare-PathScope -ScopeName "Process" -BackupEntries @($snapshot.pathBreakdown.process) -CurrentEntries @($currentPath.process)
        Compare-PathScope -ScopeName "User" -BackupEntries @($snapshot.pathBreakdown.user) -CurrentEntries @($currentPath.user)
        Compare-PathScope -ScopeName "Machine" -BackupEntries @($snapshot.pathBreakdown.machine) -CurrentEntries @($currentPath.machine)
    )

    $comparisonDirectory = Join-Path (Join-Path $BackupDirectory.Parent.FullName "comparisons") $BackupDirectory.Name
    [void](New-Item -ItemType Directory -Path $comparisonDirectory -Force)
    $reportPath = Join-Path $comparisonDirectory ("compare-current-{0}.md" -f (Get-Date -Format "yyyyMMdd-HHmmss"))
    $lines = [System.Collections.ArrayList]::new()
    [void]$lines.Add("# Environment Backup Comparison")
    [void]$lines.Add("")
    [void]$lines.Add("- Backup: $($BackupDirectory.FullName)")
    [void]$lines.Add("- ComparedAt: $((Get-Date).ToString("yyyy-MM-dd HH:mm:ss zzz"))")

    foreach ($comparison in $envComparisons) {
        [void]$lines.Add("")
        [void]$lines.Add("## $($comparison.scope) variables")
        Add-ComparisonSection -Lines $lines -Title "Added in current" -Items @($comparison.added)
        Add-ComparisonSection -Lines $lines -Title "Removed from current" -Items @($comparison.removed)
        Add-ComparisonSection -Lines $lines -Title "Changed values" -Items @($comparison.changed)
    }

    foreach ($comparison in $pathComparisons) {
        [void]$lines.Add("")
        [void]$lines.Add("## $($comparison.scope) PATH")
        Add-ComparisonSection -Lines $lines -Title "Added in current PATH" -Items @($comparison.added)
        Add-ComparisonSection -Lines $lines -Title "Removed from current PATH" -Items @($comparison.removed)
    }

    $lines | Set-Content -LiteralPath $reportPath -Encoding UTF8
    Write-Host $Text.CompareCreated
    Write-Host "  $reportPath"
    return $reportPath
}

function ConvertTo-Base64Utf16 {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)

    return [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($Value))
}

function New-RestoreScript {
    param(
        [Parameter(Mandatory = $true)][object]$Snapshot,
        [Parameter(Mandatory = $true)][ValidateSet("user", "machine")][string[]]$Scopes
    )

    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add("# Generated by VarKeep v1. Review before running.")
    $lines.Add("# Variables added after this backup are not deleted.")
    $lines.Add("`$ErrorActionPreference = 'Stop'")
    $lines.Add("`$encoding = [System.Text.Encoding]::Unicode")
    if ($Scopes -contains "machine") {
        $lines.Add("`$identity = [Security.Principal.WindowsIdentity]::GetCurrent()")
        $lines.Add("`$principal = [Security.Principal.WindowsPrincipal]::new(`$identity)")
        $lines.Add("if (-not `$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Administrator privileges are required.' }")
    }

    foreach ($scope in $Scopes) {
        $root = if ($scope -eq "user") { "CurrentUser" } else { "LocalMachine" }
        $keyPath = if ($scope -eq "user") { "Environment" } else { "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" }
        $lines.Add("")
        $lines.Add("`$key = [Microsoft.Win32.Registry]::$root.OpenSubKey('$keyPath', `$true)")
        $lines.Add("if (`$null -eq `$key) { throw 'Registry key unavailable.' }")
        $lines.Add("try {")
        foreach ($item in @($Snapshot.environment.$scope | Sort-Object name)) {
            $kind = [string]$item.kind
            if ($kind -notin @("String", "ExpandString")) {
                throw "Unsupported registry value kind '$kind'."
            }
            $name = ConvertTo-Base64Utf16 -Value ([string]$item.name)
            $value = ConvertTo-Base64Utf16 -Value ([string]$item.value)
            $lines.Add("  `$name = `$encoding.GetString([Convert]::FromBase64String('$name'))")
            $lines.Add("  `$value = `$encoding.GetString([Convert]::FromBase64String('$value'))")
            $lines.Add("  `$key.SetValue(`$name, `$value, [Microsoft.Win32.RegistryValueKind]::$kind)")
        }
        $lines.Add("} finally {")
        $lines.Add("  `$key.Dispose()")
        $lines.Add("}")
    }

    $lines.Add("")
    $lines.Add("if (-not ('VarKeepV1EnvironmentNotifier' -as [type])) {")
    $lines.Add("  Add-Type -TypeDefinition @'")
    $lines.Add("using System;")
    $lines.Add("using System.Runtime.InteropServices;")
    $lines.Add("public static class VarKeepV1EnvironmentNotifier {")
    $lines.Add("    [DllImport(`"user32.dll`", SetLastError = true, CharSet = CharSet.Unicode)]")
    $lines.Add("    public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);")
    $lines.Add("}")
    $lines.Add("'@")
    $lines.Add("}")
    $lines.Add("`$broadcastResult = [UIntPtr]::Zero")
    $lines.Add("`$sent = [VarKeepV1EnvironmentNotifier]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 0x0002, 5000, [ref]`$broadcastResult)")
    $lines.Add("if (`$sent -eq [IntPtr]::Zero) { throw 'Environment change notification failed.' }")
    return ($lines -join "`r`n") + "`r`n"
}

function New-RestoreScripts {
    param(
        [Parameter(Mandatory = $true)]
        [System.IO.DirectoryInfo]$BackupDirectory,
        [Parameter(Mandatory = $true)]
        [hashtable]$Text,
        [bool]$Quiet = $false
    )

    $snapshotPath = Join-Path $BackupDirectory.FullName "snapshot.json"
    $snapshot = Get-Content -LiteralPath $snapshotPath -Raw | ConvertFrom-Json
    $restoreDirectory = Join-Path $BackupDirectory.FullName "restore"
    [void](New-Item -ItemType Directory -Path $restoreDirectory -Force)
    $userScriptPath = Join-Path $restoreDirectory "user.ps1"
    $machineScriptPath = Join-Path $restoreDirectory "system.ps1"
    $allScriptPath = Join-Path $restoreDirectory "all.ps1"
    $encoding = [System.Text.UTF8Encoding]::new($false)

    [System.IO.File]::WriteAllText($userScriptPath, (New-RestoreScript -Snapshot $snapshot -Scopes @("user")), $encoding)
    [System.IO.File]::WriteAllText($machineScriptPath, (New-RestoreScript -Snapshot $snapshot -Scopes @("machine")), $encoding)
    [System.IO.File]::WriteAllText($allScriptPath, (New-RestoreScript -Snapshot $snapshot -Scopes @("user", "machine")), $encoding)

    if (-not $Quiet) {
        Write-Host $Text.RestoreCreated
        Write-Host "  $userScriptPath"
        Write-Host "  $machineScriptPath"
        Write-Host "  $allScriptPath"
    }

    return @($userScriptPath, $machineScriptPath, $allScriptPath)
}

function Start-InteractiveBackupSetup {
    param(
        [Parameter(Mandatory = $true)]
        [string]$DefaultOutputRoot,
        [string]$DefaultLabel = "",
        [Parameter(Mandatory = $true)]
        [ValidateSet("zh", "en")]
        [string]$SelectedLanguage
    )

    $text = Get-UiText -SelectedLanguage $SelectedLanguage

    Write-Host ""
    Write-Host $text.Title
    Write-Host $text.Description
    Write-Host ""

    Show-BackupDirectorySummary -RootPath $DefaultOutputRoot -Text $text
    Write-Host ""

    $selectedOutputRoot = Read-TextWithDefault -Prompt $text.OutputFolder -DefaultValue $DefaultOutputRoot
    $selectedLabel = Read-OptionalText -Prompt $text.BackupLabel
    if ([string]::IsNullOrWhiteSpace($selectedLabel)) {
        $selectedLabel = $DefaultLabel
    }

    Write-Host ""
    Write-Host $text.SummaryTitle
    Write-Host ($text.SummaryOutputFolder -f $selectedOutputRoot)
    Write-Host ($text.SummaryLabel -f $(if ([string]::IsNullOrWhiteSpace($selectedLabel)) { $text.EmptyLabel } else { $selectedLabel }))

    if (-not (Read-YesNoNumber -Prompt $text.CreateNow -Text $text -DefaultYes $true)) {
        Write-Host $text.Canceled
        return [ordered]@{
            Canceled = $true
            Language = $SelectedLanguage
        }
    }

    return [ordered]@{
        OutputRoot = $selectedOutputRoot
        Label = $selectedLabel
        Language = $SelectedLanguage
        Canceled = $false
    }
}

function Start-InteractiveBackupFlow {
    param(
        [Parameter(Mandatory = $true)]
        [string]$DefaultOutputRoot,
        [string]$DefaultLabel = "",
        [Parameter(Mandatory = $true)]
        [ValidateSet("zh", "en")]
        [string]$SelectedLanguage,
        [bool]$IncludeValues = $false
    )

    $text = Get-UiText -SelectedLanguage $SelectedLanguage
    $mode = Read-NumberChoice `
        -Prompt $text.BackupModePrompt `
        -Options @($text.MenuQuickBackup, $text.MenuCustomBackup, $text.MenuBack) `
        -DefaultChoice 1 `
        -InvalidMessage $text.EnterNumber

    $backupResult = switch ($mode) {
        1 {
            Invoke-EnvironmentBackup `
                -BackupOutputRoot $DefaultOutputRoot `
                -BackupLabel $DefaultLabel `
                -SelectedLanguage $SelectedLanguage `
                -IncludeValues $IncludeValues `
                -CompactOutput $true
        }
        2 {
            $backupOptions = Start-InteractiveBackupSetup `
                -DefaultOutputRoot $DefaultOutputRoot `
                -DefaultLabel $DefaultLabel `
                -SelectedLanguage $SelectedLanguage

            if (-not $backupOptions.Canceled) {
                Invoke-EnvironmentBackup `
                    -BackupOutputRoot $backupOptions.OutputRoot `
                    -BackupLabel $backupOptions.Label `
                    -SelectedLanguage $backupOptions.Language `
                    -IncludeValues $IncludeValues `
                    -CompactOutput $true
            }
        }
    }

    if ($null -eq $backupResult) {
        return [ordered]@{
            ExitRequested = $false
            BackupDirectory = $null
        }
    }

    Write-Host ""
    $nextAction = Read-NumberChoice `
        -Prompt $text.PostBackupPrompt `
        -Options @($text.PostBackupExit, $text.PostBackupOpen, $text.PostBackupMainMenu) `
        -DefaultChoice 1 `
        -InvalidMessage $text.EnterNumber

    if ($nextAction -eq 2) {
        Invoke-Item -LiteralPath $backupResult.BackupDirectory
    }

    return [ordered]@{
        ExitRequested = $nextAction -ne 3
        BackupDirectory = $backupResult.BackupDirectory
    }
}

function Invoke-EnvironmentBackup {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BackupOutputRoot,
        [string]$BackupLabel,
        [ValidateSet("zh", "en")]
        [string]$SelectedLanguage = "en",
        [bool]$ShouldOpenOutputDirectory = $false,
        [bool]$IncludeValues = $false,
        [bool]$CompactOutput = $false
    )

    $text = Get-UiText -SelectedLanguage $SelectedLanguage
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $safeLabel = if ([string]::IsNullOrWhiteSpace($BackupLabel)) {
        ""
    } else {
        "-" + (($BackupLabel -replace '[^a-zA-Z0-9._-]', '-').Trim('-'))
    }

    $backupName = "env-backup-{0}{1}" -f $timestamp, $safeLabel
    $backupDir = Join-Path $BackupOutputRoot $backupName
    $suffix = 1
    while (Test-Path -LiteralPath $backupDir) {
        $backupDir = Join-Path $BackupOutputRoot ("{0}-{1}" -f $backupName, $suffix)
        $suffix++
    }
    New-Item -ItemType Directory -Path $backupDir | Out-Null

    try {
    $processEnv = @(Get-ProcessEnvironment)
    $userEnv = @(Get-RegistryEnvironment -RegistryPath "HKCU:\Environment")
    $machineEnv = @(Get-RegistryEnvironment -RegistryPath "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
    $processPathEntries = @(Split-PathEntries -PathValue (Find-EnvValue -Items $processEnv -Name "Path"))
    $userPathEntries = @(Split-PathEntries -PathValue (Find-EnvValue -Items $userEnv -Name "Path"))
    $machinePathEntries = @(Split-PathEntries -PathValue (Find-EnvValue -Items $machineEnv -Name "Path"))

    $snapshot = [ordered]@{
        metadata = [ordered]@{
            schemaVersion = 2
            createdAtLocal = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss zzz")
            createdAtUtc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
            language = $SelectedLanguage
            powerShellVersion = $PSVersionTable.PSVersion.ToString()
        }
        environment = [ordered]@{
            process = $processEnv
            user = $userEnv
            machine = $machineEnv
        }
        pathBreakdown = [ordered]@{
            process = $processPathEntries
            user = $userPathEntries
            machine = $machinePathEntries
        }
    }
    $jsonPath = Join-Path $backupDir 'snapshot.json'
    $markdownPath = Join-Path $backupDir 'summary.md'
    $encoding = [System.Text.UTF8Encoding]::new($false)
    $snapshotJson = $snapshot | ConvertTo-Json -Depth 8
    $storedSnapshot = $snapshotJson | ConvertFrom-Json
    Assert-SnapshotShape -Snapshot $storedSnapshot
    [System.IO.File]::WriteAllText($jsonPath, $snapshotJson, $encoding)
    [System.IO.File]::WriteAllText($markdownPath, (New-SummaryMarkdown -Snapshot $storedSnapshot -SelectedLanguage $SelectedLanguage), $encoding)

    $restoreScriptPaths = @(
        New-RestoreScripts `
            -BackupDirectory (Get-Item -LiteralPath $backupDir) `
            -Text $text `
            -Quiet $true
    )

    if (-not $CompactOutput) {
        Write-Host $text.Created
        Write-Host "  $backupDir"
        Write-Host $text.Files
        Write-Host "  $jsonPath"
        Write-Host "  $markdownPath"
        $restoreScriptPaths | ForEach-Object { Write-Host "  $_" }
    }

    if (-not (Test-BackupIntegrity -BackupDirectory $backupDir -Text $text -Quiet $CompactOutput)) {
        throw "Backup integrity validation failed."
    }
    } catch {
        if (Test-Path -LiteralPath $backupDir) {
            Remove-Item -LiteralPath $backupDir -Recurse -Force
        }
        throw
    }

    if ($CompactOutput) {
        Write-Host ""
        Write-Host $text.CompactSuccess
        Write-Host $text.CompactFiles
    }

    Write-Host $text.SensitiveDataWarning

    if ($ShouldOpenOutputDirectory) {
        Invoke-Item -LiteralPath $backupDir
    }

    return [ordered]@{
        BackupDirectory = $backupDir
        SnapshotPath = $jsonPath
        SummaryPath = $markdownPath
        UserRestoreScriptPath = $restoreScriptPaths[0]
        MachineRestoreScriptPath = $restoreScriptPaths[1]
        CombinedRestoreScriptPath = $restoreScriptPaths[2]
    }
}

function Start-InteractiveMenu {
    param(
        [Parameter(Mandatory = $true)]
        [string]$DefaultOutputRoot,
        [string]$DefaultLabel = "",
        [Parameter(Mandatory = $true)]
        [ValidateSet("zh", "en")]
        [string]$SelectedLanguage,
        [bool]$IncludeValues = $false
    )

    $text = Get-UiText -SelectedLanguage $SelectedLanguage

    while ($true) {
        Write-Host ""
        Write-Host $text.Title
        Show-BackupDirectorySummary -RootPath $DefaultOutputRoot -Text $text
        Write-Host ""

        $choice = Read-NumberChoice `
            -Prompt $text.MainMenuPrompt `
            -Options @(
                $text.MenuNewBackup,
                $text.MenuOpenLatest,
                $text.MenuViewSummary,
                $text.MenuCompare,
                $text.MenuRestore,
                $text.MenuExit
            ) `
            -DefaultChoice 1 `
            -InvalidMessage $text.EnterNumber

        switch ($choice) {
            1 {
                $backupFlowResult = Start-InteractiveBackupFlow `
                    -DefaultOutputRoot $DefaultOutputRoot `
                    -DefaultLabel $DefaultLabel `
                    -SelectedLanguage $SelectedLanguage `
                    -IncludeValues $IncludeValues

                if ($backupFlowResult.ExitRequested) {
                    return [ordered]@{ Language = $SelectedLanguage }
                }
            }
            2 {
                $latest = @(Get-BackupDirectories -RootPath $DefaultOutputRoot | Select-Object -First 1)
                if ($latest.Count -eq 0) {
                    Write-Host $text.NoBackups
                } else {
                    Invoke-Item -LiteralPath $latest[0].FullName
                }
            }
            3 {
                $backup = Read-BackupSelection -RootPath $DefaultOutputRoot -Text $text
                if ($null -ne $backup) {
                    Show-BackupSummary -BackupDirectory $backup -Text $text
                }
            }
            4 {
                $backup = Read-BackupSelection -RootPath $DefaultOutputRoot -Text $text
                if ($null -ne $backup) {
                    [void](Invoke-BackupComparison -BackupDirectory $backup -Text $text -IncludeValues $IncludeValues)
                }
            }
            5 {
                $backup = Read-BackupSelection -RootPath $DefaultOutputRoot -Text $text
                if ($null -ne $backup) {
                    [void](New-RestoreScripts -BackupDirectory $backup -Text $text)
                }
            }
            6 {
                return [ordered]@{
                    Language = $SelectedLanguage
                }
            }
        }

        if ([Console]::IsInputRedirected) {
            return [ordered]@{
                Language = $SelectedLanguage
            }
        }
    }
}

function Invoke-CliCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CommandName,
        [string]$TargetValue,
        [Parameter(Mandatory = $true)]
        [string]$BackupRoot,
        [string]$BackupLabel,
        [ValidateSet("zh", "en")]
        [string]$SelectedLanguage = "en",
        [bool]$ShouldOpenOutputDirectory = $false,
        [bool]$IncludeValues = $false
    )

    $text = Get-UiText -SelectedLanguage $SelectedLanguage

    switch ($CommandName) {
        "backup" {
            [void](Invoke-EnvironmentBackup `
                -BackupOutputRoot $BackupRoot `
                -BackupLabel $BackupLabel `
                -SelectedLanguage $SelectedLanguage `
                -ShouldOpenOutputDirectory $ShouldOpenOutputDirectory `
                -IncludeValues $IncludeValues)
        }
        "list" {
            Show-BackupList -RootPath $BackupRoot
        }
        "open" {
            $backup = Resolve-BackupTarget -RootPath $BackupRoot -TargetValue $TargetValue
            if ($null -eq $backup) {
                Write-Host $text.NoBackups
                exit 1
            }

            Invoke-Item -LiteralPath $backup.FullName
        }
        "compare" {
            $backup = Resolve-BackupTarget -RootPath $BackupRoot -TargetValue $TargetValue
            if ($null -eq $backup) {
                Write-Host $text.NoBackups
                exit 1
            }

            [void](Invoke-BackupComparison -BackupDirectory $backup -Text $text -IncludeValues $IncludeValues)
        }
        "restore-script" {
            $backup = Resolve-BackupTarget -RootPath $BackupRoot -TargetValue $TargetValue
            if ($null -eq $backup) {
                Write-Host $text.NoBackups
                exit 1
            }

            [void](New-RestoreScripts -BackupDirectory $backup -Text $text)
        }
        "restore" {
            $backup = Resolve-BackupTarget -RootPath $BackupRoot -TargetValue $TargetValue
            if ($null -eq $backup) {
                Write-Host $text.NoBackups
                exit 1
            }

            [void](New-RestoreScripts -BackupDirectory $backup -Text $text)
        }
    }
}

$normalizedCommand = if ([string]::IsNullOrWhiteSpace($Command)) {
    ""
} else {
    $Command.Trim().ToLowerInvariant()
}

$helpCommands = @("-h", "--help", "/?", "help")
$shouldShowHelp = $Help.IsPresent -or ($helpCommands -contains $normalizedCommand)
$knownCommands = @("", "backup", "list", "open", "compare", "restore-script", "restore")
$shouldUseInteractiveMode = $Interactive -or [string]::IsNullOrWhiteSpace($normalizedCommand)

$selectedLanguage = Get-DefaultLanguage -ExplicitLanguage $Language
$exitText = Get-UiText -SelectedLanguage $selectedLanguage

try {
    if ($shouldShowHelp) {
        Show-Help
    } elseif (-not ($knownCommands -contains $normalizedCommand)) {
        Write-Host "Unknown command: $Command"
        Write-Host ""
        Show-Help
        exit 1
    } elseif ($shouldUseInteractiveMode) {
        $interactiveResult = Start-InteractiveMenu `
            -DefaultOutputRoot $OutputRoot `
            -DefaultLabel $Label `
            -SelectedLanguage $selectedLanguage `
            -IncludeValues $IncludeValuesInReports.IsPresent

        $exitText = Get-UiText -SelectedLanguage $interactiveResult.Language
    } else {
        Invoke-CliCommand `
            -CommandName $normalizedCommand `
            -TargetValue $Target `
            -BackupRoot $OutputRoot `
            -BackupLabel $Label `
            -SelectedLanguage $selectedLanguage `
            -ShouldOpenOutputDirectory $OpenOutputDirectory.IsPresent `
            -IncludeValues $IncludeValuesInReports.IsPresent
    }
} catch {
    Write-Host ""
    Write-Host $exitText.ErrorTitle
    Write-Host $_.Exception.Message

    if ($shouldUseInteractiveMode) {
        Wait-BeforeInteractiveExit -Text $exitText
    }

    exit 1
}
