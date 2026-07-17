# VarKeep

<p align="center">
  <img src="v2/assets/branding/varkeep-mark.svg" alt="VarKeep" width="72">
</p>

VarKeep 是一个本地优先的 Windows 环境变量备份工具。它保存用户和系统环境变量、生成便于检查的脱敏摘要，并提供需要人工确认后才能运行的 PowerShell 还原脚本。

仓库同时维护两个相互独立的版本：

| 版本 | 适合谁 | 运行方式 | 备份目录 |
| --- | --- | --- | --- |
| [v1 · PowerShell CLI/TUI](v1/) | 喜欢脚本、命令行或数字菜单的用户 | PowerShell 7 | `v1/backups/` |
| [v2 · Rust + Slint GUI](v2/) | 希望使用原生 Windows 图形界面的用户 | 便携 `varkeep.exe` | EXE 旁的 `backups/` |

v1 和 v2 不会读取、迁移或删除彼此的备份。

## 系统要求

- Windows 10/11 x64。
- v2 最终用户无需安装 Rust 或 PowerShell。
- v1 需要 [PowerShell 7](https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows)。
- 只有运行系统范围还原脚本时才需要管理员权限。

## 快速开始

### v2 GUI

从本仓库的 **Releases** 页面下载 `varkeep-v2-windows-x64.zip`，解压到一个不会自动同步的本地目录，然后运行：

```text
varkeep.exe
```

点击“创建备份”即可生成快照、摘要和三种还原脚本。应用不会安装服务、修改 PATH、自动提权或执行还原脚本。

### v1 CLI/TUI

从 Releases 下载 `varkeep-v1-cli.zip` 并解压。双击 `start-interactive.cmd`，或在解压目录运行：

```powershell
pwsh -NoProfile -File .\backup-env.ps1
```

完整命令和参数见 [v1 使用说明](v1/README.md)。

## 备份内容

两个版本的新备份都使用相同的核心布局：

```text
env-backup-时间/
├─ snapshot.json
├─ summary.md
└─ restore/
   ├─ user.ps1
   ├─ system.ps1
   └─ all.ps1
```

- `snapshot.json` 和还原脚本包含完整明文值。
- `summary.md` 仅提供尽力脱敏、长度受限的人工预览。
- 还原脚本保留 `REG_SZ` 与 `REG_EXPAND_SZ`，但只写入快照中的变量，不删除后来新增的变量。

## 安全提醒

环境变量可能包含令牌、密码、连接字符串和私人路径。即使摘要看起来已经脱敏，也不要把任何备份目录提交到 Git、放入云同步目录或发送给他人。

VarKeep 不加密备份，也不替换目录继承的 Windows ACL。详细威胁模型、恢复注意事项和漏洞报告方式见 [SECURITY.md](SECURITY.md)。

## 从源码开发

要求 Windows x64、PowerShell 7 和 Rust 1.97.0。仓库中的工具链文件会固定 Rust 版本。

```powershell
pwsh -NoProfile -File .\scripts\verify.ps1
pwsh -NoProfile -File .\scripts\verify-release.ps1
pwsh -NoProfile -File .\scripts\package-release.ps1
```

打包脚本在 `dist/` 生成独立的 v1/v2 ZIP 和 `SHA256SUMS.txt`。发布白名单会拒绝本地备份、构建目录和意外文件。

贡献前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。版本变化见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

VarKeep 源码采用 [MIT License](LICENSE)。

v2 使用 Slint 1.17.1 的 Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0；应用顶层“关于”页面包含官方 `AboutSlint` 控件。v2 发布包同时提供第三方许可证材料。
