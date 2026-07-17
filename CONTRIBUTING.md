# Contributing to VarKeep

感谢你改进 VarKeep。这个项目刻意保持小而直接：v1 是 PowerShell CLI/TUI，v2 是 Rust + Slint GUI，两者共享发布仓库，但不共享运行数据。

## 开始之前

- 使用 Windows 10/11 x64。
- 安装 PowerShell 7。
- v2 使用仓库固定的 Rust 1.97.0 工具链。
- 不要在 Issue、日志、测试夹具或提交中使用真实环境变量值、用户名、主机名、私人路径和备份文件。

## 本地验证

从仓库根目录运行：

```powershell
pwsh -NoProfile -File .\scripts\verify.ps1
```

涉及发布内容时还要运行：

```powershell
pwsh -NoProfile -File .\scripts\verify-release.ps1
pwsh -NoProfile -File .\scripts\package-release.ps1
```

不要删除或跳过测试来让门禁通过。修复行为缺陷时先增加能够复现问题的回归测试。

## 修改原则

- 保持 v1/v2 的备份目录和兼容边界独立。
- 不自动执行还原脚本、不自动提权、不静默删除环境变量。
- 新增磁盘读取必须有大小或数量边界，并拒绝 reparse point 等异常对象。
- 摘要、错误和默认对比不得输出原始环境变量值。
- 不增加依赖，除非现有标准库和项目工具无法合理完成需求，并在 PR 中解释选择。
- UI 保持紧凑、平面、少文案，不增加装饰性胶囊、渐变和阴影。

## 提交和 Pull Request

一个提交只处理一个清晰问题。提交说明应解释为什么修改，而不只是复述文件变化。Pull Request 请包含：

- 用户可见变化和兼容性影响。
- 安全或隐私边界是否变化。
- 已运行的验证命令及结果。
- 未测试的范围，例如真实 HKLM 写入。
- UI 变化的截图（仅在界面发生变化时）。

提交前确认 `git status` 不包含 `backups/`、`snapshot.json`、生成的还原脚本、`target/`、`publish/` 或 `dist/`。

## 漏洞报告

敏感安全问题请遵循 [SECURITY.md](SECURITY.md)，不要在公开 Issue 中粘贴快照、令牌或私人路径。

## 许可证

提交代码即表示你有权提供该贡献，并同意贡献内容按项目的 [MIT License](LICENSE) 发布。第三方代码或资产必须同时提供来源和兼容许可证信息。
