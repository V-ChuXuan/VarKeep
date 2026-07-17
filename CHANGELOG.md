# Changelog

All notable user-visible changes to VarKeep are recorded here.

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
