# VarKeep v2.3 · Windows GUI

VarKeep v2 是使用 Rust + Slint 编写的便携式 Windows 环境变量快照工具。它读取持久化的用户和系统环境变量，创建可验证的本地备份，并提供历史记录、备注、对比和安全删除。

v2 与 PowerShell CLI v1 位于同一仓库，但不会读取、迁移或删除 v1 的备份。

## 要求

- Windows 10/11 x64。
- 最终用户无需安装 Rust、PowerShell 或其他运行库。
- 创建备份不需要管理员权限；系统范围还原脚本需要管理员权限。

## 使用

从项目 Releases 下载 `varkeep-v2-windows-x64.zip`，解压到一个不会自动同步的本地目录，然后运行：

```text
varkeep.exe
```

应用把备份保存在 EXE 旁的 `backups/`。移动 EXE 不会自动移动旧备份；如需迁移，请在应用关闭时移动整个目录。

## 功能

- 创建用户和系统环境变量快照，保留 `REG_SZ` / `REG_EXPAND_SZ`。
- 默认生成用户、系统和合并范围的 PowerShell 还原脚本。
- 按时间显示历史备份，并支持单行备注、查看摘要、打开位置和确认删除。
- 选择一个备份可与当前环境对比；选择两个备份可按时间从旧到新对比。
- 对比显示变量范围、名称和变化类型；PATH 会额外列出经过身份信息处理的新增/删除目录。
- 自动选择中文或英文，也可在“关于”页面切换。
- 后台 worker 串行执行磁盘和注册表读取，避免界面直接执行长任务。

如果注册表中存在不支持的命名值类型，创建会明确失败，不会静默生成不完整备份。

## 备份产物

```text
env-backup-时间戳/
├─ snapshot.json
├─ summary.md
├─ restore/
│  ├─ user.ps1
│  ├─ system.ps1
│  └─ all.ps1
└─ note.txt          # 只有填写备注时才存在
```

- `snapshot.json` 和还原脚本包含完整明文值。
- `summary.md` 按范围显示名称、类型和长度受限的尽力脱敏预览。
- `note.txt` 不参与快照对比或还原。

应用会验证固定布局、普通文件、大小限制，以及从快照确定性生成的摘要和脚本。损坏或不完整的目录不会进入可操作的历史记录。

## 还原边界

VarKeep 不执行脚本、不请求提权，也不删除当前环境中额外存在的变量。

- `user.ps1` 写入运行脚本的 Windows 账户。
- `system.ps1` 需要可信的管理员 PowerShell。
- `all.ps1` 会先检查管理员权限，再写入当前账户和系统范围。

还原不是事务操作；脚本中途失败时可能已有部分变量写入。完成后脚本会广播 Windows 环境变化，但已经运行的应用通常仍需重启才能读取新环境。

## 安全边界

快照、脚本和备注均为明文文件。摘要脱敏依赖启发式规则，可能漏掉命名特殊的秘密、内部地址或私人路径，不能作为“可以安全分享”的证明。

应用不加密备份、不修改目录继承的 Windows ACL，也不保证抵御已经拥有当前账户或备份目录写权限的攻击者。不要把 `backups/` 放入 Git、网盘或其他自动同步位置。

完整威胁模型和漏洞报告方式见源码仓库根目录的 `SECURITY.md`。

## 从源码开发

要求 Windows x64、PowerShell 7 和 Rust 1.97.0。进入 `v2/` 后运行：

```powershell
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked
```

从仓库根目录执行完整门禁：

```powershell
pwsh -NoProfile -File .\scripts\verify.ps1
pwsh -NoProfile -File .\scripts\verify-release.ps1
pwsh -NoProfile -File .\scripts\package-release.ps1
```

`verify-release.ps1` 只替换 `v2/publish/` 中的发布文件，并在操作前后校验本地 `publish/backups/`；`package-release.ps1` 在 `dist/` 创建最终 ZIP 与 SHA-256 清单。

## 许可证

VarKeep 源码采用 MIT License。Release ZIP 中附带完整的 `LICENSE` 文件。

Slint 1.17.1 按 Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0 使用；应用顶层“关于”页面包含官方 `AboutSlint` 控件。Release ZIP 同时附带 `THIRD-PARTY-NOTICES.md` 和 `THIRD-PARTY-LICENSES.txt`。
