[CmdletBinding()]
param([string] $Executable)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
if (-not $Executable) { $Executable = Join-Path $project 'v2/target/x86_64-pc-windows-msvc/release/varkeep.exe' }
$Executable = (Get-Item -LiteralPath $Executable).FullName
$output = Join-Path $project ('v2/target/output/native-check/run-' + [guid]::NewGuid().ToString('N'))
$appRoot = Join-Path $output 'app'
$backupRoot = Join-Path $appRoot 'backups'
$fixture = Join-Path $project 'v2/tests/fixtures/v2.3-backup'
New-Item -ItemType Directory -Path $backupRoot -Force | Out-Null
$names = @('env-backup-1700000000000', 'env-backup-1700000000000-1')
foreach ($name in $names) { Copy-Item -LiteralPath $fixture -Destination (Join-Path $backupRoot $name) -Recurse }
New-Item -ItemType Directory -Path (Join-Path $backupRoot 'env-backup-broken') | Out-Null
$ownExe = Join-Path $appRoot 'varkeep.exe'
Copy-Item -LiteralPath $Executable -Destination $ownExe
$note = Join-Path $backupRoot ($names[1] + '/note.txt')
[IO.File]::WriteAllText($note, 'original synthetic note')
$protected = @{}
Get-ChildItem -LiteralPath $backupRoot -Recurse -File | Where-Object Name -ne 'note.txt' | ForEach-Object {
    $protected[$_.FullName] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
}
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
$providers = [Reflection.Assembly]::LoadWithPartialName('UIAutomationClientsideProviders')
$providerType = $providers.GetType('UIAutomationClientsideProviders.UIAutomationClientSideProviders', $true)
$providerTable = $providerType.GetField('ClientSideProviderDescriptionTable').GetValue($null)
# Register the public native-control proxies from a managed client frame before any UIA query.
Add-Type -ReferencedAssemblies @('System.Runtime', [Windows.Automation.AutomationElement].Assembly.Location, [Windows.Automation.AutomationIdentifier].Assembly.Location) -TypeDefinition @'
using System.Runtime.CompilerServices;
using System.Windows.Automation;
public static class VarKeepUiaClient {
    [MethodImpl(MethodImplOptions.NoInlining)]
    public static void Register(ClientSideProviderDescription[] table) { ClientSettings.RegisterClientSideProviders(table); }
}
'@
[VarKeepUiaClient]::Register($providerTable)
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class VarKeepNativeCheck {
    private delegate bool EnumProc(IntPtr window, IntPtr param);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc proc, IntPtr param);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint id);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wparam, StringBuilder text, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wparam, string text, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] private static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
    [StructLayout(LayoutKind.Sequential)] private struct GuiInfo { public uint Size,Flags; public IntPtr Active,Focus,Capture,Menu,Move,Caret; public Rect CaretRect; }
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left,Top,Right,Bottom; }
    public static IntPtr[] Windows(uint pid, string className) {
        var windows=new List<IntPtr>();
        EnumWindows((window,param)=>{ uint owner; GetWindowThreadProcessId(window,out owner);
            var name=new StringBuilder(256); GetClassName(window,name,name.Capacity);
            if(owner==pid && name.ToString()==className) windows.Add(window); return true; },IntPtr.Zero);
        return windows.ToArray();
    }
    public static string Text(IntPtr window) {
        var text=new StringBuilder(65536); IntPtr result;
        if(SendMessageTimeout(window,13,new IntPtr(text.Capacity),text,2,2000,out result)==IntPtr.Zero) throw new InvalidOperationException("Own control text timed out.");
        return text.ToString();
    }
    public static void SetText(IntPtr window,string text) {
        IntPtr result;
        if(SendMessageTimeout(window,12,IntPtr.Zero,text,2,2000,out result)==IntPtr.Zero) throw new InvalidOperationException("Own edit timed out.");
    }
    public static long Number(IntPtr window,uint message,long wparam,long lparam) {
        IntPtr result;
        if(SendMessageTimeout(window,message,new IntPtr(wparam),new IntPtr(lparam),2,2000,out result)==IntPtr.Zero) throw new InvalidOperationException("Own control query timed out.");
        return result.ToInt64();
    }
    public static IntPtr Focus(IntPtr window) {
        uint pid; uint thread=GetWindowThreadProcessId(window,out pid);
        var info=new GuiInfo { Size=(uint)Marshal.SizeOf(typeof(GuiInfo)) };
        if(!GetGUIThreadInfo(thread,ref info)) throw new InvalidOperationException("Own focus unavailable.");
        return info.Focus;
    }
    public static int[] Size(IntPtr window) {
        Rect rect; if(!GetWindowRect(window,out rect)) throw new InvalidOperationException("Own window rectangle unavailable.");
        return new int[] { rect.Right-rect.Left,rect.Bottom-rect.Top };
    }
}
'@
$report = [ordered]@{
    status = 'running'; executableSha256 = (Get-FileHash -LiteralPath $ownExe -Algorithm SHA256).Hash.ToLowerInvariant()
    checks = [Collections.Generic.List[string]]::new()
    gaps = @('No real environment capture or restore', 'No Narrator speech or real 200% scaling validation', 'No install/upgrade/uninstall validation')
}
$process = $null
$lock = $null
function Assert-Check([bool] $Condition, [string] $Name) {
    if (-not $Condition) { throw $Name }
    $report.checks.Add($Name)
}
function Wait-For([scriptblock] $Probe, [string] $Name) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($watch.Elapsed.TotalSeconds -lt 10) {
        if ($process.HasExited) { throw "Own process exited while waiting for $Name" }
        $value = & $Probe
        if ($value) { return $value }
        Start-Sleep -Milliseconds 80
    }
    throw "Timed out waiting for $Name"
}
function Send-Command([IntPtr] $Window, [int] $Id) {
    [uint32] $owner = 0
    [void][VarKeepNativeCheck]::GetWindowThreadProcessId($Window, [ref]$owner)
    if ($owner -ne $process.Id) { throw 'Window does not belong to the fixture process.' }
    if (-not [VarKeepNativeCheck]::PostMessage($Window, 0x111, [IntPtr]$Id, [IntPtr]::Zero)) { throw 'Command post failed.' }
}
function Wait-Dialog {
    Wait-For { @([VarKeepNativeCheck]::Windows($process.Id, '#32770')) | Select-Object -First 1 } 'own modal dialog'
}
function Close-Dialog([IntPtr] $Dialog) {
    Send-Command $Dialog 2
    [void](Wait-For { -not [VarKeepNativeCheck]::IsWindow($Dialog) } 'dialog cancellation')
}
function Save-Window([IntPtr] $Window, [string] $Name) {
    $size = [VarKeepNativeCheck]::Size($Window)
    $bitmap = [Drawing.Bitmap]::new($size[0], $size[1])
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try { $captured = [VarKeepNativeCheck]::PrintWindow($Window, $dc, 2) }
    finally { $graphics.ReleaseHdc($dc) }
    try {
        if (-not $captured) { throw 'Own window capture failed.' }
        $bitmap.Save((Join-Path $output $Name), [Drawing.Imaging.ImageFormat]::Png)
    }
    finally { $graphics.Dispose(); $bitmap.Dispose() }
}
function Send-Key([IntPtr] $Window, [int] $Key) {
    [void][VarKeepNativeCheck]::PostMessage($Window, 0x100, [IntPtr]$Key, [IntPtr]::Zero)
    [void][VarKeepNativeCheck]::PostMessage($Window, 0x101, [IntPtr]$Key, [IntPtr]::Zero)
}
function Get-HistoryRows {
    $types = [Windows.Automation.OrCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::DataItem))
    $items = $listElement.FindAll([Windows.Automation.TreeScope]::Descendants, $types)
    if ($items.Count -eq 2) { return ,$items }
}
try {
    $process = Start-Process -FilePath $ownExe -WorkingDirectory $appRoot -WindowStyle Hidden -PassThru
    $main = Wait-For { @([VarKeepNativeCheck]::Windows($process.Id, 'VarKeepMainWindow')) | Select-Object -First 1 } 'main window'
    # The first ShowWindow honors STARTUPINFO's hidden flag; reveal only our fixture window without activation.
    [void][VarKeepNativeCheck]::ShowWindow($main, 8)
    $list = Wait-For {
        $handle = [VarKeepNativeCheck]::GetDlgItem($main, 100)
        if ($handle -ne [IntPtr]::Zero -and [VarKeepNativeCheck]::IsWindowEnabled([VarKeepNativeCheck]::GetDlgItem($main,102))) { $handle }
    } 'initialized history control'
    $root = [Windows.Automation.AutomationElement]::FromHandle($main)
    $listElement = [Windows.Automation.AutomationElement]::FromHandle($list)
    Save-Window $main 'startup.png'
    $report.initialControls = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
        [ordered]@{ name = $_.Current.Name; type = $_.Current.ControlType.ProgrammaticName; id = $_.Current.AutomationId }
    })
    [void](Wait-For { [VarKeepNativeCheck]::Number($list, 0x1004, 0, 0) -eq 2 } 'two synthetic history rows')
    [void](Wait-For { Get-HistoryRows } 'native accessible history rows')
    $report.actualWindowDpi = [VarKeepNativeCheck]::GetDpiForWindow($main)
    Assert-Check ($report.actualWindowDpi -gt 0) 'native window exposes actual DPI'
    foreach ($id in @(101,102,103,104,105,106,107,108,109,110)) {
        $handle = [VarKeepNativeCheck]::GetDlgItem($main, $id)
        $control = [Windows.Automation.AutomationElement]::FromHandle($handle)
        Assert-Check ($control.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $control.Current.Name.Length -gt 0) "button $id exposes a native accessible name and role"
    }
    Save-Window $main 'main.png'
    Send-Command $main 110
    $dialog = Wait-Dialog
    Assert-Check (-not [VarKeepNativeCheck]::IsWindowEnabled($main)) 'issue dialog disables its owner'
    $issues = [VarKeepNativeCheck]::Text([VarKeepNativeCheck]::GetDlgItem($dialog, 201))
    Assert-Check ($issues.Contains('env-backup-broken') -and $issues -notmatch 'synthetic-fixture-secret') 'issue detail identifies invalid backup without secret values'
    Close-Dialog $dialog
    foreach ($index in @(0,1)) {
        $row = (Get-HistoryRows)[$index]
        $row.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
        $row.SetFocus()
        $report['row' + $index + 'Before'] = [ordered]@{ state = [VarKeepNativeCheck]::Number($list, 0x102c, $index, 0xf00f); focus = [VarKeepNativeCheck]::Focus($main).ToInt64(); list = $list.ToInt64(); patterns = @($row.GetSupportedPatterns() | ForEach-Object ProgrammaticName) }
        Send-Key $list 0x20
        Start-Sleep -Milliseconds 120
        $report['row' + $index + 'After'] = [VarKeepNativeCheck]::Number($list, 0x102c, $index, 0xf00f)
        $report['row' + $index + 'Status'] = [VarKeepNativeCheck]::Text([VarKeepNativeCheck]::GetDlgItem($main,111))
        Assert-Check ([VarKeepNativeCheck]::Number($list, 0x102c, $index, 0xf000) -eq 0x2000) "Space toggles comparison row $index"
    }
    Send-Command $main 103
    $dialog = Wait-Dialog
    $context = [VarKeepNativeCheck]::Text([VarKeepNativeCheck]::GetDlgItem($dialog, 203))
    $detail = [VarKeepNativeCheck]::Text([VarKeepNativeCheck]::GetDlgItem($dialog, 201))
    Assert-Check ($context.Contains($names[0]) -and $context.Contains($names[1]) -and $context.Contains([string][char]0x2192)) 'comparison names both backups and direction'
    Assert-Check (-not $detail.Contains('synthetic-fixture-secret')) 'comparison default is redacted'
    Save-Window $dialog 'comparison.png'
    Close-Dialog $dialog
    # Compare selection and the active row are independent; select the newest row for editing.
    (Get-HistoryRows)[0].GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Start-Sleep -Milliseconds 120
    Send-Command $main 108
    $dialog = Wait-Dialog
    $edit = [VarKeepNativeCheck]::GetDlgItem($dialog, 201)
    Assert-Check ([VarKeepNativeCheck]::Focus($dialog) -eq $edit) 'note dialog initially focuses its edit'
    Send-Key $edit 9
    [void](Wait-For { [VarKeepNativeCheck]::Focus($dialog) -ne $edit } 'Tab moves modal focus')
    $tabFocus = [VarKeepNativeCheck]::Focus($dialog)
    Assert-Check ($tabFocus -eq [VarKeepNativeCheck]::GetDlgItem($dialog,1) -or $tabFocus -eq [VarKeepNativeCheck]::GetDlgItem($dialog,2)) 'Tab reaches a modal action'
    [VarKeepNativeCheck]::SetText($edit, 'updated synthetic note')
    $lock = [IO.File]::Open($note, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    Send-Command $dialog 1
    [void](Wait-For { [VarKeepNativeCheck]::Text([VarKeepNativeCheck]::GetDlgItem($dialog, 202)) } 'inline note save error')
    Assert-Check ([VarKeepNativeCheck]::Text($edit) -eq 'updated synthetic note') 'note failure keeps draft in modal edit'
    Assert-Check ([VarKeepNativeCheck]::IsWindow($dialog)) 'note save failure keeps its modal open'
    Save-Window $dialog 'note-error.png'
    $lock.Dispose(); $lock = $null
    Assert-Check ([IO.File]::ReadAllText($note) -eq 'original synthetic note') 'failed replacement preserves original note bytes'
    Send-Command $dialog 1
    [void](Wait-For { -not [VarKeepNativeCheck]::IsWindow($dialog) } 'successful note save')
    Assert-Check ([IO.File]::ReadAllText($note) -eq 'updated synthetic note') 'retry commits note after lock is released'
    Send-Command $main 109
    $dialog = Wait-Dialog
    # Escape is dispatched through the real modal message loop, not a direct delete cancellation callback.
    [void][VarKeepNativeCheck]::PostMessage($dialog, 0x100, [IntPtr]0x1b, [IntPtr]::Zero)
    [void][VarKeepNativeCheck]::PostMessage($dialog, 0x101, [IntPtr]0x1b, [IntPtr]::Zero)
    [void](Wait-For { -not [VarKeepNativeCheck]::IsWindow($dialog) } 'Escape cancels deletion')
    Assert-Check (Test-Path -LiteralPath (Join-Path $backupRoot $names[1])) 'cancel deletion keeps selected backup'
    foreach ($path in $protected.Keys) {
        Assert-Check ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -eq $protected[$path]) ('unchanged fixture artifact ' + [IO.Path]::GetFileName($path))
    }
    [void][VarKeepNativeCheck]::PostMessage($main, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
    Assert-Check ($process.WaitForExit(10000) -and $process.ExitCode -eq 0) 'idle native application closes normally'
    $report.status = 'passed'
}
catch { $report.status = 'failed'; $report.error = $_.Exception.Message; Write-Host "FAIL native UI: $($report.error)" }
finally {
    if ($lock) { $lock.Dispose() }
    if ($process -and -not $process.HasExited) {
        $report.forcedTermination = $true
        $process.Kill(); [void]$process.WaitForExit(5000)
    }
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
    Write-Host "Native UI result: $($report.status); report: $output/report.json"
}
if ($report.status -ne 'passed') { exit 1 }
