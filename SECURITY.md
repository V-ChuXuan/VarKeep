# Security policy

## Supported versions

| Version | Support |
| --- | --- |
| VarKeep v2.3 | Supported |
| PowerShell v1.1 | Supported |

## Sensitive data and threat model

VarKeep snapshots persistent Windows User and System environment variables. `snapshot.json` and the scripts under `restore/` contain the original values in plaintext. Optional `note.txt` files may also contain personal information. `summary.md` is a best-effort redacted preview, not a guarantee that arbitrary secrets will be detected. Do not commit, synchronize, or share any `backups/` directory.

The application does not encrypt backups or replace inherited Windows ACLs. It is intended to reduce accidental loss and common local path mistakes, not to protect data from an attacker who already has write access to the current Windows account or backup directory.

VarKeep generates restore scripts but never executes them, requests elevation, or silently removes environment variables. Review a script before running it. The user section of `restore/user.ps1` and `restore/all.ps1` targets the Windows account running the script, so do not run it as a different administrator account. Run `restore/system.ps1` or `restore/all.ps1` only from a trusted elevated PowerShell session when system restoration is intended; both scripts check administrator membership before writing system values.

## Reporting a vulnerability

After the repository is published on GitHub, use its private security advisory reporting feature when available. Do not place environment values, snapshots, restore scripts, access tokens, passwords, or private paths in a public issue.

For non-sensitive security hardening suggestions, a normal issue is acceptable. Include the affected version, expected behavior, and minimal reproduction steps without real secrets.

## Release verification

Official release assets include separate v1 and v2 ZIP files plus `SHA256SUMS.txt`. Compare the ZIP hash with the checksum downloaded from the same trusted GitHub Release before running the executable. A matching SHA-256 value detects transfer corruption or a mismatched file; by itself it does not prove publisher identity or protect against a compromised release page.

The automated release pipeline rejects backup directories, snapshot files, generated restore-script instances, build directories, and unexpected archive entries. The v2 ZIP also contains `THIRD-PARTY-NOTICES.md` and a reproducibly generated `THIRD-PARTY-LICENSES.txt` for its locked Windows x64 runtime and build dependencies.
