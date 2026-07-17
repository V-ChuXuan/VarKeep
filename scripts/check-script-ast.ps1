[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string[]] $Path
)

$ErrorActionPreference = 'Stop'
$allowedMembers = @('Dispose', 'FromBase64String', 'GetCurrent', 'GetString', 'IsInRole', 'new', 'OpenSubKey', 'SendMessageTimeout', 'SetValue')

foreach ($scriptPath in $Path) {
    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile(
        $scriptPath,
        [ref] $tokens,
        [ref] $errors
    )
    if ($errors.Count -gt 0) {
        throw 'Generated restore script has PowerShell parse errors.'
    }

    $commands = $ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.CommandAst]
    }, $true)
    foreach ($command in $commands) {
        if ($command.GetCommandName() -ne 'Add-Type' -or $command.CommandElements.Count -ne 3) {
            throw 'Generated restore script contains an unapproved command invocation.'
        }
        if ($command.CommandElements[1].ParameterName -ne 'TypeDefinition' -or
            $command.CommandElements[2] -isnot [System.Management.Automation.Language.StringConstantExpressionAst]) {
            throw 'Generated restore script contains a dynamic Add-Type invocation.'
        }
    }

    $memberCalls = $ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.InvokeMemberExpressionAst]
    }, $true)
    foreach ($call in $memberCalls) {
        $member = $call.Member.Value
        if ($member -notin $allowedMembers) {
            throw "Generated restore script contains an unapproved member call: $member"
        }
    }

    $text = [IO.File]::ReadAllText($scriptPath)
    foreach ($forbidden in @('Invoke-Expression', 'Start-Process', 'Remove-Item', 'Remove-ItemProperty', 'DownloadString')) {
        if ($text.Contains($forbidden, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Generated restore script contains forbidden text: $forbidden"
        }
    }
}

Write-Host 'PASS restore script parser and AST allowlist'
