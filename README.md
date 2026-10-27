<div align="center">

<img src="v2/assets/branding/varkeep-mark.svg" alt="VarKeep 标志" width="128">

# VarKeep

**修改 Windows 环境变量前，留一份可以检查和手工还原的本地备份。**

保存快照，比较变化；需要恢复时，先检查脚本，再由你运行。

[![GitHub Release](https://img.shields.io/github/v/release/V-ChuXuan/VarKeep?style=flat-square)](https://github.com/V-ChuXuan/VarKeep/releases)
[![Windows x64](https://img.shields.io/badge/platform-Windows%20x64-0078D4?style=flat-square)](#安装)
[![MIT License](https://img.shields.io/badge/license-MIT-3DA639?style=flat-square)](LICENSE)

[下载应用](https://github.com/V-ChuXuan/VarKeep/releases) · [快速开始](#快速开始) · [GUI 使用说明](v2/README.md) · [English](README.en.md)

</div>

## 演示

![VarKeep 原生界面的历史列表、备注和备份操作](docs/images/varkeep-main.png)

截图使用合成备份，不含真实环境变量。

## 安装

**系统要求：Windows 10/11 x64。** 从 [Releases](https://github.com/V-ChuXuan/VarKeep/releases) 下载并解压到本地、不会自动同步的目录。

| 版本 | 下载文件 | 运行方式 | 默认备份目录 |
| --- | --- | --- | --- |
| **v2 GUI（推荐）** | `varkeep-v2-windows-x64.zip` | 双击 `varkeep.exe`，无需安装 Rust 或 PowerShell | EXE 旁的 `backups/` |
| v1 CLI/TUI | `varkeep-v1-cli.zip` | 安装 [PowerShell 7](https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows)，双击 `start-interactive.cmd` | 脚本旁的 `backups/` |

两个版本独立保存和管理备份，不会读取、迁移或删除彼此的数据。需要命令行或数字菜单时，见 [v1 CLI/TUI 使用说明](v1/README.md)。

发布页同时提供 `SHA256SUMS.txt`，可核对下载 ZIP 的 SHA-256。解压后按下面的步骤开始使用。

## 快速开始

1. 打开 `varkeep.exe`，点击“创建备份”。创建快照无需管理员权限。
2. 选择一条历史记录，点击“查看摘要”检查内容；可以添加备注，说明备份用途。
3. 勾选一份备份，与当前持久化环境比较；勾选两份，则按旧备份 → 新备份比较。
4. 需要还原时，点击“打开位置”，检查 `restore/` 中的脚本，再手工运行所需脚本。

应用不会替你执行还原或请求管理员权限。系统范围还原需要你主动打开可信的管理员 PowerShell，详见下方的[用法](#用法)。

## 功能

- 保存用户、系统环境变量，保留 `REG_SZ` 与 `REG_EXPAND_SZ` 类型。
- 生成完整快照、尽力脱敏的摘要，以及用户、系统和合并范围的还原脚本。
- 比较变量的新增、删除、值或类型变化；GUI 结果标明比较方向，并提示 PATH 仅顺序变化。
- 在原生历史列表中查看摘要、编辑备注、打开位置和确认删除；支持中文、英文及键盘入口。
- 查看无效备份的目录名和原因；备注保存失败时保留旧内容和编辑草稿。

GUI 操作和容量边界见 [v2 使用说明](v2/README.md)；CLI 命令、详细比较报告和数字菜单见 [v1 使用说明](v1/README.md)。

## 用法

**备份内容**由五个核心文件组成：

```
env-backup-…/
├─ snapshot.json
├─ summary.md
└─ restore/
   ├─ user.ps1
   ├─ system.ps1
   └─ all.ps1
```

`snapshot.json` 和还原脚本保存完整明文值，`summary.md` 提供长度受限、尽力脱敏的预览。v1 还保存进程环境供查看和比较；进程变量不参与还原。v2 填写备注后会另外生成 `note.txt`。

| 还原脚本 | 写入范围 | 运行要求 |
| --- | --- | --- |
| `user.ps1` | 运行脚本的 Windows 账户 | 在需要恢复的账户下运行 |
| `system.ps1` | 系统环境变量 | 管理员 PowerShell |
| `all.ps1` | 当前账户与系统环境变量 | 管理员 PowerShell，先确认账户正确 |

还原脚本只写入快照中的变量，不删除后来新增的变量。还原不是事务操作，中途失败时可能已有部分变量写入；运行前应检查脚本，并避免同时修改环境变量。完成后会广播环境变化，已经运行的应用通常仍需重启。

备份随应用目录保留。移动 EXE 不会自动搬走旧备份；需要迁移时，先关闭应用，再移动整个目录。已有 64 个备份候选时，需要手工整理后才能继续创建；GUI 的额外历史边界见 [v2 使用说明](v2/README.md)。

## 什么时候不该用

**安全提醒：备份不是可以公开分享的文件。** 环境变量可能包含令牌、密码、连接字符串和私人路径，摘要脱敏也可能遗漏这些内容。不要把 `backups/` 放入 Git、网盘或其他自动同步目录。

VarKeep 不加密备份，也不替换目录继承的 Windows ACL。完整性校验用于发现普通损坏和异常布局，不能证明文件未被拥有本机写权限的人整体替换。它不提供自动还原或完整系统回滚。

威胁模型、账户与权限边界、漏洞报告方式见 [SECURITY.md](SECURITY.md)。

## 开发

从源码开发需要 Windows x64、PowerShell 7 和固定的 Rust 1.97.0 工具链。在仓库根目录运行：

```powershell
pwsh -NoProfile -File .\scripts\verify.ps1
pwsh -NoProfile -File .\scripts\package-release.ps1
```

验证覆盖默认合成测试、格式、Clippy 和许可证清单；打包命令另外构建、检查发布文件，在 `dist/` 生成两个 ZIP 与 `SHA256SUMS.txt`。打包会替换发布输出，请先阅读 [贡献指南](CONTRIBUTING.md)。本地验证和打包即可准备版本交付，无需部署 CI/CD。

从源码开发的 GUI 细节见 [v2 开发说明](v2/README.md#从源码开发)，版本变化见 [CHANGELOG.md](CHANGELOG.md)。

## 许可证

VarKeep 源码采用 [MIT License](LICENSE)。

GUI 使用 Windows 原生控件，无需额外的界面署名组件。发布包保留 [第三方说明](v2/THIRD-PARTY-NOTICES.md) 和 [依赖许可证文本](v2/THIRD-PARTY-LICENSES.txt)。

<p align="center"><sub>如果 VarKeep 对你有帮助，欢迎点亮 ⭐，让更多有同样需求的人发现它。</sub></p>