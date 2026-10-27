# Changelog

All notable user-visible changes to VarKeep are recorded here.

## 2.4.0 - 2026-10-27

- 用 Windows 原生列表、按钮和模态对话框替换 Slint，不再需要额外的界面署名组件；保留第三方许可证文本。
- 修复 v1 创建第 65 份备份后无法列出历史的问题，在写入前检查容量。
- 无效备份可查看目录名和原因；比较结果标明基准与目标，并解释 PATH 仅顺序变化。
- 备注采用临时写入后原子替换，失败保留旧内容和编辑草稿，并在当前对话框提示错误。
- 补齐键盘入口，默认回归使用合成数据，保留旧备份模板兼容夹具，移除绑定 Slint 排版的源码正则检查。

### 其他修复

- Preserve v1 value case and registry-kind changes in comparisons; support empty environment scopes.
- Check v2 backup capacity before writing, while allowing recovery from externally added excess backups.
- Resolve and validate selected backups directly, retaining path and artifact checks without repeatedly validating all siblings.
- Count linked backup candidates as invalid and provide specific localized capacity-recovery guidance.
- Add synthetic comparison, capacity, selected-access and path-boundary regressions.

## 2.3.0 - 2026-07-17

### Added

- Added an optional note column for v2 backup records.
- Added localized note editing with a 100-character, single-line limit.
- Added Windows CI, tag-based Release automation, separate v1/v2 ZIP files, and SHA-256 checksums.
- Added an explicit security policy and release-package allowlist verification.
- Added separate user, system, and combined restore scripts under each backup's `restore/` directory.
- Added localized Markdown summaries with structured variable tables and conservative value redaction.
- Added publication-ready root, v1, and v2 documentation plus a concise contribution guide.
- Added a reproducible third-party license inventory for the locked v2 runtime and build dependency graph.

### Changed

- Prepared v1 and v2 for publication from one repository while keeping their runtime data independent.
- Made the exact Rust 1.97.0 toolchain file authoritative for local and CI builds.
- Replaced duplicate text/Markdown count summaries with one human-readable `summary.md`.
- Made one-time success feedback temporary and replaced the styled note field with a flat neutral input.
- Updated v1 to the grouped five-file artifact layout, type-preserving restore scripts, redacted Markdown summary, and deterministic integrity validation.
- Made v1/v2 restore scripts broadcast the Windows environment change notification after successful registry writes.
- Changed v2 comparisons from count-only feedback to a name-level result window, including redacted per-entry PATH additions and removals.
- Rejected unsupported persistent registry value kinds instead of silently omitting them.
- Added bounded v1 directory enumeration, Base64 UTF-16 v1 restore payloads, bounded v2 comparison details, and isolated-registry execution tests for both versions.
- Clarified that redacted summaries are local previews rather than safe-to-share exports, and documented the non-transactional restore boundary.

## 2.2.0

### Added

- Introduced the Rust + Slint Windows GUI as the independent v2 implementation.
- Added one- and two-backup comparison, safe deletion, bilingual UI, integrity checks, restore-script generation, and portable four-file releases.

## 1.1.0

### Changed

- Replaced the legacy flat v1 outputs with `snapshot.json`, `summary.md`, and `restore/{user,system,all}.ps1`.
- Preserved Windows registry value kinds and added strict deterministic artifact validation.

## 1.0.0

### Added

- Added the PowerShell CLI/TUI implementation with quick and custom backups, summaries, comparisons, and separate user/system restore scripts.
