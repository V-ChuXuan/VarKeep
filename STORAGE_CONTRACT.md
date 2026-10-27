# Backup access and comparison contract

## 1. Scope
Rust backup discovery, selected actions and UI errors; PowerShell comparison helpers.
Existing CLI commands, snapshot formats, roots and manual restore scripts remain compatible.

## 2. Signatures
`resolve_named_backup(root, name) -> Result<BackupRecord, StorageError>`
`resolve_named_snapshot(root, name) -> Result<(BackupRecord, Snapshot), StorageError>`
`list_backups_with_diagnostics(root) -> Result<BackupListing, StorageError>`
`Compare-EnvironmentScope -ScopeName -BackupItems -CurrentItems [-IncludeValues]`

## 3. Contracts
Selected resolution rejects path components, alternate streams, trailing dots/spaces,
linked components and paths outside the direct root child. It validates the selected
snapshot, summary and all scripts. It does not enumerate or validate sibling backups.
Creation rejects 64 or more named candidates before creating a partial directory.
History validation retains the 128-root-entry bound and never silently truncates.
Names compare case-insensitively in v1; values and registry kinds compare exactly.
Default reports hide values; detailed opt-in report formatting remains unchanged.

Discovery diagnostics retain each invalid candidate's name and validation code,
without values or link traversal. Selected-action failures retain the existing
`backup_not_found` boundary. The GUI shows the diagnostic list on request.

Comparison results carry their baseline and target identities; a missing target
identity denotes the current environment. PATH reordering is reported when the
case-insensitive entry collection is unchanged but its order differs.

Notes remain bounded single-line text. Saving writes and synchronizes a new
temporary file beside the backup directory, then renames it over the note; a
failed write or replacement preserves the previous note. The modal editor keeps
its draft and displays the failure locally until success or cancellation.

## 4. Validation and errors
| Condition | Behavior |
| --- | --- |
| At least 64 named candidates | Creation fails with too_many_backup_candidates, no partial backup |
| More than 128 root entries | History fails with too_many_backup_root_entries and recovery guidance |
| Invalid selected name/artifacts | backup_not_found; no action |
| Linked/malformed candidate | Count as invalid, no traversal/action |
| Empty environment scope | Valid backup/comparison, no PATH entries |
| Values differ only in case, or kinds differ | Report changed variable |

## 5. Good/base/bad cases
A normal history creates and lists complete backups. A 65-entry externally populated
history can be listed and reduced through existing user-confirmed deletion. A root
over 128 entries requires manual organization as stated by the UI; selected storage
resolution remains independent. No automatic deletion or migration is introduced.

## 6. Tests
Storage regressions cover pre-write capacity, 65-entry recovery, independent selected
access with oversized/corrupt siblings, traversal and reparse rejection, and valid
artifact checks. Synthetic PowerShell comparison tests exercise exact values/kinds,
name case, opt-in reports and all-empty backup/comparison flows with stubbed readers.
The repository gate runs synthetic tests alongside the packaged v1 suite. The
packaged suite substitutes only the host readers in a temporary script copy;
normal tests never capture actual environment values. The v2.3 synthetic fixture
locks the original summary and restore-script bytes for upgrade compatibility.

## 7. Wrong versus correct
Wrong: enumerate and validate all history to locate one backup, then reject the
entire history after creating an over-capacity backup.
Correct: bound discovery, validate the chosen backup at its action boundary, and
check capacity before writing. Preserve separate v1/v2 data and explicit restore actions.
