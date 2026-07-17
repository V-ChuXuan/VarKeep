# VarKeep v1 · PowerShell CLI/TUI

VarKeep v1 是 Windows 环境变量快照工具的 PowerShell 7 版本。它适合命令行、自动化脚本和偏好数字菜单的用户，与 v2 GUI 的源码和备份目录完全独立。

## 要求

- Windows 10/11。
- PowerShell 7（`pwsh.exe`）。
- 创建备份不需要管理员权限；系统范围还原脚本需要管理员权限。

## 使用

从项目 Releases 下载并解压 `varkeep-v1-cli.zip`。双击：

```text
start-interactive.cmd
```

也可以直接使用 CLI：

```powershell
pwsh -NoProfile -File .\backup-env.ps1 --help
pwsh -NoProfile -File .\backup-env.ps1 backup -Label before-pnpm
pwsh -NoProfile -File .\backup-env.ps1 list
pwsh -NoProfile -File .\backup-env.ps1 compare latest
pwsh -NoProfile -File .\backup-env.ps1 restore-script latest
pwsh -NoProfile -File .\backup-env.ps1 open latest
```

未指定 `-Language` 时，界面根据 Windows UI 语言选择中文或英文。默认备份目录为脚本旁的 `backups/`。

## 数字菜单

菜单提供新建备份、打开最近备份、查看摘要、与当前环境对比和重新生成还原脚本。日常快速备份只需三次 Enter：

```text
新建备份 → 快速备份 → 退出
```

自定义备份允许调整输出目录和标签，并在创建前再次确认。

## 备份产物

```text
env-backup-YYYYMMDD-HHmmss/
├─ snapshot.json
├─ summary.md
└─ restore/
   ├─ user.ps1
   ├─ system.ps1
   └─ all.ps1
```

- `snapshot.json` 保存进程、用户和系统环境的完整原始值；进程变量仅用于查看和对比。
- `summary.md` 按范围列出名称、注册表类型和尽力脱敏的值预览。
- 三份脚本分别还原用户、系统和合并范围，保留 `REG_SZ` / `REG_EXPAND_SZ`。
- 脚本完成写入后通知 Windows 环境已经变化，但不会刷新已经运行的应用进程。

VarKeep 只生成脚本，不执行、不自动提权，也不删除快照之外的变量。`system.ps1` 和 `all.ps1` 会在写入前检查管理员权限。还原不是事务操作；运行前应检查脚本，并避免在恢复过程中同时修改环境变量。

## 对比

```powershell
pwsh -NoProfile -File .\backup-env.ps1 compare latest
```

报告保存在：

```text
backups/comparisons/<备份名>/compare-current-YYYYMMDD-HHmmss.md
```

默认报告显示变量名和 PATH 条目差异。只有显式添加 `-IncludeValuesInReports` 才会写入完整旧值和当前值；这种报告与快照同样敏感。

## 完整性与安全边界

工具会限制目录枚举和文件大小，拒绝异常布局、reparse point、损坏快照，以及与快照不一致的摘要或还原脚本。这是防误操作和发现普通损坏的完整性检查，不是数字签名，无法证明备份没有被拥有本机写权限的攻击者整体替换。

摘要脱敏依赖变量名和内容特征，只适合本地快速检查。它可能漏掉命名特殊的秘密、内部地址或私人路径，因此不要提交、同步、外发或粘贴 `backups/` 中的任何文件。

完整安全说明和漏洞报告方式见源码仓库根目录的 `SECURITY.md`。

## 源码验证

在源码仓库的 `v1/` 目录运行：

```powershell
pwsh -NoProfile -File .\tests\run-tests.ps1
```

测试覆盖产物布局、注册表类型、特殊字符、摘要脱敏、完整性校验、比较、菜单交互和隔离注册表中的脚本执行，不会写入真实用户/系统环境键。

## 许可证

VarKeep 源码采用 MIT License。Release ZIP 中附带完整的 `LICENSE` 文件。
