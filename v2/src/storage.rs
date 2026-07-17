use crate::domain::{
    ComparisonSummary, DomainError, EnvironmentVariable, RegistryValueKind, Scope, Snapshot,
    compare_snapshots,
};
use crate::summary::{SummaryLanguage, render_summary};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const REQUIRED_FILES: [&str; 5] = [
    "snapshot.json",
    "summary.md",
    "restore/user.ps1",
    "restore/system.ps1",
    "restore/all.ps1",
];
const RESTORE_DIRECTORY: &str = "restore";
pub const NOTE_FILE: &str = "note.txt";
pub const MAX_NOTE_CHARS: usize = 100;
const MAX_NOTE_BYTES: u64 = MAX_NOTE_CHARS as u64 * 4;
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_BACKUP_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_BACKUP_CANDIDATES: usize = 64;
pub const MAX_DIRECTORY_ENTRIES: usize = 64;
pub const MAX_ROOT_ENTRIES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageError {
    code: &'static str,
}

impl StorageError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for StorageError {}

impl From<DomainError> for StorageError {
    fn from(_value: DomainError) -> Self {
        Self::new("invalid_snapshot")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupRecord {
    pub name: String,
    pub directory: PathBuf,
    pub created_at_unix_ms: u64,
    pub variable_count: usize,
    pub note: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackupListing {
    pub records: Vec<BackupRecord>,
    pub invalid_count: usize,
}

pub fn load_v2_snapshot(path: &Path) -> Result<Snapshot, StorageError> {
    let bytes = read_limited(path)?;
    let snapshot: Snapshot =
        serde_json::from_slice(&bytes).map_err(|_| StorageError::new("invalid_v2_json"))?;
    snapshot.validate_v2()?;
    Ok(snapshot)
}

pub fn create_backup(root: &Path, snapshot: &Snapshot) -> Result<BackupRecord, StorageError> {
    create_backup_with_language(root, snapshot, SummaryLanguage::English)
}

pub fn create_backup_with_language(
    root: &Path,
    snapshot: &Snapshot,
    language: SummaryLanguage,
) -> Result<BackupRecord, StorageError> {
    snapshot.validate_v2()?;
    ensure_no_reparse_components(root)?;
    fs::create_dir_all(root).map_err(|_| StorageError::new("backup_root_unavailable"))?;
    ensure_no_reparse_components(root)?;
    let stamp = snapshot.created_at_unix_ms;
    let final_name = unique_name(root, &format!("env-backup-{stamp}"));
    let final_path = root.join(&final_name);
    let partial_path = root.join(format!(".partial-{final_name}"));
    fs::create_dir(&partial_path).map_err(|_| StorageError::new("partial_create_failed"))?;

    let result = (|| {
        let restore_path = partial_path.join(RESTORE_DIRECTORY);
        fs::create_dir(&restore_path).map_err(|_| StorageError::new("partial_create_failed"))?;
        write_snapshot_file(&partial_path.join("snapshot.json"), snapshot)?;
        let markdown = render_summary(snapshot, language);
        write_file(&partial_path.join("summary.md"), markdown.as_bytes())?;
        let scripts = generate_restore_scripts(snapshot)?;
        write_file(&restore_path.join("user.ps1"), scripts.user.as_bytes())?;
        write_file(&restore_path.join("system.ps1"), scripts.system.as_bytes())?;
        write_file(&restore_path.join("all.ps1"), scripts.all.as_bytes())?;
        fs::rename(&partial_path, &final_path)
            .map_err(|_| StorageError::new("backup_commit_failed"))?;
        Ok(BackupRecord {
            name: final_name,
            directory: final_path,
            created_at_unix_ms: snapshot.created_at_unix_ms,
            variable_count: snapshot.variables.len(),
            note: String::new(),
        })
    })();

    if result.is_err() && partial_path.exists() {
        let _ = fs::remove_dir_all(&partial_path);
    }
    result
}

pub fn list_backups(root: &Path) -> Result<Vec<BackupRecord>, StorageError> {
    Ok(list_backups_with_diagnostics(root)?.records)
}

pub fn resolve_named_backup(root: &Path, name: &str) -> Result<BackupRecord, StorageError> {
    list_backups_with_diagnostics(root)?
        .records
        .into_iter()
        .find(|record| record.name == name)
        .ok_or_else(|| StorageError::new("backup_not_found"))
}

pub fn list_backups_with_diagnostics(root: &Path) -> Result<BackupListing, StorageError> {
    if !root.exists() {
        return Ok(BackupListing::default());
    }
    ensure_no_reparse_components(root)?;
    let canonical_root = root
        .canonicalize()
        .map_err(|_| StorageError::new("backup_root_unavailable"))?;
    let mut records = Vec::new();
    let mut invalid_count = 0usize;
    let mut candidate_count = 0usize;
    for entry in read_bounded_root_entries(root)? {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".partial-") || !name.starts_with("env-backup-") {
            continue;
        }
        candidate_count += 1;
        if candidate_count > MAX_BACKUP_CANDIDATES {
            return Err(StorageError::new("too_many_backup_candidates"));
        }
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| StorageError::new("backup_read_failed"))?;
        let canonical_path = path.canonicalize().ok();
        let is_direct_child = canonical_path
            .as_deref()
            .and_then(Path::parent)
            .is_some_and(|parent| parent == canonical_root);
        if metadata.file_type().is_symlink()
            || is_windows_reparse_point(&metadata)
            || !is_direct_child
        {
            invalid_count += 1;
            continue;
        }
        if let Ok(snapshot) = validate_backup_directory(&path) {
            let note = read_backup_note(&path);
            records.push(BackupRecord {
                name,
                directory: path,
                created_at_unix_ms: snapshot.created_at_unix_ms,
                variable_count: snapshot.variables.len(),
                note,
            });
        } else {
            invalid_count += 1;
        }
    }
    records.sort_by(|left, right| {
        right
            .created_at_unix_ms
            .cmp(&left.created_at_unix_ms)
            .then_with(|| backup_collision_index(right).cmp(&backup_collision_index(left)))
            .then_with(|| right.name.cmp(&left.name))
    });
    Ok(BackupListing {
        records,
        invalid_count,
    })
}

fn backup_collision_index(record: &BackupRecord) -> u64 {
    let base = format!("env-backup-{}", record.created_at_unix_ms);
    if record.name == base {
        return 0;
    }
    record
        .name
        .strip_prefix(&format!("{base}-"))
        .and_then(|suffix| suffix.parse().ok())
        .unwrap_or(0)
}

fn read_bounded_root_entries(root: &Path) -> Result<Vec<fs::DirEntry>, StorageError> {
    let mut output = Vec::new();
    for entry in fs::read_dir(root).map_err(|_| StorageError::new("backup_list_failed"))? {
        if output.len() >= MAX_ROOT_ENTRIES {
            return Err(StorageError::new("too_many_backup_root_entries"));
        }
        output.push(entry.map_err(|_| StorageError::new("backup_list_failed"))?);
    }
    Ok(output)
}

fn validate_backup_tree(path: &Path) -> Result<u64, StorageError> {
    let mut total = 0u64;
    for (index, entry) in fs::read_dir(path)
        .map_err(|_| StorageError::new("backup_read_failed"))?
        .enumerate()
    {
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(StorageError::new("too_many_backup_entries"));
        }
        let entry = entry.map_err(|_| StorageError::new("backup_read_failed"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| StorageError::new("backup_read_failed"))?;
        if metadata.file_type().is_symlink() || is_windows_reparse_point(&metadata) {
            return Err(StorageError::new("backup_entry_reparse"));
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        match name.as_ref() {
            "snapshot.json" | "summary.md" | NOTE_FILE if metadata.is_file() => {
                total = total.saturating_add(checked_backup_file_size(&metadata)?);
            }
            RESTORE_DIRECTORY if metadata.is_dir() => {
                total = total.saturating_add(validate_restore_tree(&entry.path())?);
            }
            _ => return Err(StorageError::new("unexpected_backup_entry")),
        }
    }
    Ok(total)
}

fn validate_restore_tree(path: &Path) -> Result<u64, StorageError> {
    let mut total = 0u64;
    let mut count = 0usize;
    for entry in fs::read_dir(path).map_err(|_| StorageError::new("backup_read_failed"))? {
        count += 1;
        if count > 3 {
            return Err(StorageError::new("unexpected_backup_entry"));
        }
        let entry = entry.map_err(|_| StorageError::new("backup_read_failed"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| StorageError::new("backup_read_failed"))?;
        if metadata.file_type().is_symlink() || is_windows_reparse_point(&metadata) {
            return Err(StorageError::new("backup_entry_reparse"));
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !matches!(name.as_ref(), "user.ps1" | "system.ps1" | "all.ps1") || !metadata.is_file() {
            return Err(StorageError::new("unexpected_backup_entry"));
        }
        total = total.saturating_add(checked_backup_file_size(&metadata)?);
    }
    Ok(total)
}

fn checked_backup_file_size(metadata: &fs::Metadata) -> Result<u64, StorageError> {
    if metadata.len() > MAX_FILE_BYTES {
        return Err(StorageError::new("backup_file_too_large"));
    }
    Ok(metadata.len())
}

pub fn validate_backup_directory(path: &Path) -> Result<Snapshot, StorageError> {
    ensure_no_reparse_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::new("backup_missing"))?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
    {
        return Err(StorageError::new("backup_path_invalid"));
    }
    let total = validate_backup_tree(path)?;
    if total > MAX_BACKUP_BYTES {
        return Err(StorageError::new("backup_too_large"));
    }
    for file in REQUIRED_FILES {
        let target = path.join(file);
        let metadata =
            fs::symlink_metadata(&target).map_err(|_| StorageError::new("backup_incomplete"))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(StorageError::new("backup_incomplete"));
        }
        File::open(&target).map_err(|_| StorageError::new("backup_incomplete"))?;
    }
    let snapshot = load_v2_snapshot(&path.join("snapshot.json"))?;
    let stored_summary = read_limited(&path.join("summary.md"))?;
    let expected_chinese = render_summary(&snapshot, SummaryLanguage::Chinese);
    let expected_english = render_summary(&snapshot, SummaryLanguage::English);
    if stored_summary != expected_chinese.as_bytes()
        && stored_summary != expected_english.as_bytes()
    {
        return Err(StorageError::new("summary_mismatch"));
    }
    let scripts = generate_restore_scripts(&snapshot)?;
    let stored_user = read_limited(&path.join("restore/user.ps1"))?;
    let stored_system = read_limited(&path.join("restore/system.ps1"))?;
    let stored_all = read_limited(&path.join("restore/all.ps1"))?;
    if stored_user != scripts.user.as_bytes()
        || stored_system != scripts.system.as_bytes()
        || stored_all != scripts.all.as_bytes()
    {
        return Err(StorageError::new("restore_script_mismatch"));
    }
    Ok(snapshot)
}

pub fn compare_backup_with(
    snapshot: &Snapshot,
    current: &[EnvironmentVariable],
) -> ComparisonSummary {
    compare_snapshots(&snapshot.variables, current)
}

pub fn compare_named_backups(
    root: &Path,
    baseline_name: &str,
    current_name: &str,
) -> Result<ComparisonSummary, StorageError> {
    if baseline_name == current_name {
        return Err(StorageError::new("same_backup"));
    }
    let listing = list_backups_with_diagnostics(root)?;
    let baseline = listing
        .records
        .iter()
        .find(|record| record.name == baseline_name)
        .ok_or_else(|| StorageError::new("backup_not_found"))?;
    let current = listing
        .records
        .iter()
        .find(|record| record.name == current_name)
        .ok_or_else(|| StorageError::new("backup_not_found"))?;
    let baseline_snapshot = validate_backup_directory(&baseline.directory)?;
    let current_snapshot = validate_backup_directory(&current.directory)?;
    Ok(compare_snapshots(
        &baseline_snapshot.variables,
        &current_snapshot.variables,
    ))
}

pub fn delete_named_backup(root: &Path, name: &str) -> Result<(), StorageError> {
    let record = resolve_named_backup(root, name)?;
    validate_backup_directory(&record.directory)?;
    ensure_no_reparse_components(root)?;
    let canonical_root = root
        .canonicalize()
        .map_err(|_| StorageError::new("backup_root_unavailable"))?;
    let canonical_target = record
        .directory
        .canonicalize()
        .map_err(|_| StorageError::new("backup_not_found"))?;
    let metadata = fs::symlink_metadata(&canonical_target)
        .map_err(|_| StorageError::new("backup_not_found"))?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
        || canonical_target.parent() != Some(canonical_root.as_path())
    {
        return Err(StorageError::new("backup_path_invalid"));
    }
    fs::remove_dir_all(&canonical_target).map_err(|_| StorageError::new("backup_delete_failed"))?;
    Ok(())
}

pub fn update_backup_note(root: &Path, name: &str, note: &str) -> Result<(), StorageError> {
    let note = normalize_note(note)?;
    let record = resolve_named_backup(root, name)?;
    validate_backup_directory(&record.directory)?;
    let path = record.directory.join(NOTE_FILE);

    if note.is_empty() {
        match fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && !is_windows_reparse_point(&metadata) =>
            {
                fs::remove_file(path).map_err(|_| StorageError::new("note_write_failed"))?;
            }
            Ok(_) => return Err(StorageError::new("backup_entry_reparse")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(StorageError::new("note_write_failed")),
        }
        return Ok(());
    }

    if let Ok(metadata) = fs::symlink_metadata(&path)
        && (!metadata.is_file()
            || metadata.file_type().is_symlink()
            || is_windows_reparse_point(&metadata))
    {
        return Err(StorageError::new("backup_entry_reparse"));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|_| StorageError::new("note_write_failed"))?;
    file.write_all(note.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| StorageError::new("note_write_failed"))
}

fn normalize_note(note: &str) -> Result<String, StorageError> {
    if note.chars().any(char::is_control) {
        return Err(StorageError::new("invalid_note"));
    }
    let trimmed = note.trim();
    if trimmed.chars().count() > MAX_NOTE_CHARS {
        return Err(StorageError::new("invalid_note"));
    }
    Ok(trimmed.to_owned())
}

fn read_backup_note(directory: &Path) -> String {
    let path = directory.join(NOTE_FILE);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return String::new();
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
        || metadata.len() > MAX_NOTE_BYTES
    {
        return String::new();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|note| normalize_note(&note).ok())
        .unwrap_or_default()
}

pub fn regenerate_restore_scripts(directory: &Path) -> Result<BackupRecord, StorageError> {
    let snapshot = validate_backup_directory(directory)?;
    let root = directory
        .parent()
        .ok_or_else(|| StorageError::new("backup_path_invalid"))?;
    let regenerated = Snapshot::new_v2(now_unix_ms(), snapshot.variables.clone())?;
    let summary = read_limited(&directory.join("summary.md"))?;
    let language = if summary == render_summary(&snapshot, SummaryLanguage::Chinese).as_bytes() {
        SummaryLanguage::Chinese
    } else {
        SummaryLanguage::English
    };
    create_backup_with_language(root, &regenerated, language)
}

pub struct RestoreScripts {
    pub user: String,
    pub system: String,
    pub all: String,
}

pub fn generate_restore_scripts(snapshot: &Snapshot) -> Result<RestoreScripts, StorageError> {
    snapshot.validate_v2()?;
    ensure_restore_script_budget(snapshot, &[Scope::User])?;
    ensure_restore_script_budget(snapshot, &[Scope::System])?;
    ensure_restore_script_budget(snapshot, &[Scope::User, Scope::System])?;
    Ok(RestoreScripts {
        user: render_script(snapshot, Scope::User),
        system: render_script(snapshot, Scope::System),
        all: render_combined_script(snapshot),
    })
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn read_limited(path: &Path) -> Result<Vec<u8>, StorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::new("file_unavailable"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(StorageError::new("file_type_invalid"));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(StorageError::new("file_too_large"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .map(|file| file.take(MAX_FILE_BYTES + 1))
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| StorageError::new("file_read_failed"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(StorageError::new("file_too_large"));
    }
    Ok(bytes)
}

fn unique_name(root: &Path, base: &str) -> String {
    if !root.join(base).exists() && !root.join(format!(".partial-{base}")).exists() {
        return base.to_owned();
    }
    for suffix in 1..=9_999 {
        let candidate = format!("{base}-{suffix}");
        if !root.join(&candidate).exists() && !root.join(format!(".partial-{candidate}")).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", now_unix_ms())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(StorageError::new("file_too_large"));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| StorageError::new("file_create_failed"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| StorageError::new("file_write_failed"))
}

fn write_snapshot_file(path: &Path, snapshot: &Snapshot) -> Result<(), StorageError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| StorageError::new("file_create_failed"))?;
    let mut writer = LimitedWriter::new(file, MAX_FILE_BYTES);
    serde_json::to_writer_pretty(&mut writer, snapshot)
        .map_err(|_| StorageError::new("snapshot_encode_failed"))?;
    writer
        .finish()
        .map_err(|_| StorageError::new("file_write_failed"))
}

struct LimitedWriter {
    file: File,
    written: u64,
    limit: u64,
}

impl LimitedWriter {
    const fn new(file: File, limit: u64) -> Self {
        Self {
            file,
            written: 0,
            limit,
        }
    }

    fn finish(self) -> std::io::Result<()> {
        self.file.sync_all()
    }
}

impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self.written.saturating_add(bytes.len() as u64);
        if next > self.limit {
            return Err(std::io::Error::other("size limit exceeded"));
        }
        let count = self.file.write(bytes)?;
        self.written = self.written.saturating_add(count as u64);
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

fn ensure_no_reparse_components(path: &Path) -> Result<(), StorageError> {
    for component in path.ancestors().filter(|candidate| candidate.exists()) {
        let metadata = fs::symlink_metadata(component)
            .map_err(|_| StorageError::new("backup_path_invalid"))?;
        if metadata.file_type().is_symlink() || is_windows_reparse_point(&metadata) {
            return Err(StorageError::new("backup_path_reparse"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_windows_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn ensure_restore_script_budget(snapshot: &Snapshot, scopes: &[Scope]) -> Result<(), StorageError> {
    let mut bytes = 2_048u64;
    for variable in snapshot
        .variables
        .iter()
        .filter(|item| scopes.contains(&item.scope))
    {
        let name_bytes = variable.name.encode_utf16().count() as u64 * 2;
        let value_bytes = variable.value.expose().encode_utf16().count() as u64 * 2;
        let encoded_name = name_bytes.div_ceil(3) * 4;
        let encoded_value = value_bytes.div_ceil(3) * 4;
        bytes = bytes
            .checked_add(encoded_name)
            .and_then(|total| total.checked_add(encoded_value))
            .and_then(|total| total.checked_add(256))
            .ok_or_else(|| StorageError::new("file_too_large"))?;
        if bytes > MAX_FILE_BYTES {
            return Err(StorageError::new("file_too_large"));
        }
    }
    Ok(())
}

fn render_script(snapshot: &Snapshot, scope: Scope) -> String {
    let mut output = script_header();
    if scope == Scope::User {
        append_current_user_notice(&mut output);
    }
    if scope == Scope::System {
        append_admin_check(&mut output);
    }
    append_scope_block(&mut output, snapshot, scope);
    append_environment_change_notification(&mut output);
    output
}

fn render_combined_script(snapshot: &Snapshot) -> String {
    let mut output = script_header();
    append_current_user_notice(&mut output);
    append_admin_check(&mut output);
    append_scope_block(&mut output, snapshot, Scope::User);
    append_scope_block(&mut output, snapshot, Scope::System);
    append_environment_change_notification(&mut output);
    output
}

fn script_header() -> String {
    String::from(
        "# Generated by VarKeep v2. Review before running.\r\n$ErrorActionPreference = 'Stop'\r\n$encoding = [System.Text.Encoding]::Unicode\r\n",
    )
}

fn append_admin_check(output: &mut String) {
    output.push_str(
        "$identity = [Security.Principal.WindowsIdentity]::GetCurrent()\r\n$principal = [Security.Principal.WindowsPrincipal]::new($identity)\r\nif (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Administrator privileges are required.' }\r\n",
    );
}

fn append_current_user_notice(output: &mut String) {
    output.push_str("# User values target the Windows account running this script.\r\n");
}

fn append_scope_block(output: &mut String, snapshot: &Snapshot, scope: Scope) {
    let (root, key_path) = match scope {
        Scope::User => ("CurrentUser", "Environment"),
        Scope::System => (
            "LocalMachine",
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        ),
    };
    output.push_str(&format!(
        "$key = [Microsoft.Win32.Registry]::{root}.OpenSubKey('{key_path}', $true)\r\nif ($null -eq $key) {{ throw 'Registry key unavailable.' }}\r\ntry {{\r\n"
    ));
    for variable in snapshot.variables.iter().filter(|item| item.scope == scope) {
        let kind = match variable.kind {
            RegistryValueKind::String => "String",
            RegistryValueKind::ExpandString => "ExpandString",
        };
        output.push_str(&format!(
            "  $name = $encoding.GetString([Convert]::FromBase64String('{}'))\r\n  $value = $encoding.GetString([Convert]::FromBase64String('{}'))\r\n  $key.SetValue($name, $value, [Microsoft.Win32.RegistryValueKind]::{kind})\r\n",
            base64_utf16(&variable.name),
            base64_utf16(variable.value.expose()),
        ));
    }
    output.push_str("} finally {\r\n  $key.Dispose()\r\n}\r\n");
}

fn append_environment_change_notification(output: &mut String) {
    output.push_str(
        "if (-not ('VarKeepEnvironmentNotifier' -as [type])) {\r\nAdd-Type -TypeDefinition @'\r\nusing System;\r\nusing System.Runtime.InteropServices;\r\npublic static class VarKeepEnvironmentNotifier {\r\n    [DllImport(\"user32.dll\", SetLastError = true, CharSet = CharSet.Unicode)]\r\n    public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);\r\n}\r\n'@\r\n}\r\n$broadcastResult = [UIntPtr]::Zero\r\n$sent = [VarKeepEnvironmentNotifier]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 0x0002, 5000, [ref]$broadcastResult)\r\nif ($sent -eq [IntPtr]::Zero) { throw 'Environment change notification failed.' }\r\n",
    );
}

fn base64_utf16(value: &str) -> String {
    let bytes = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    base64(&bytes)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("varkeep-v2-{name}-{}", now_unix_ms()))
    }

    fn snapshot() -> Snapshot {
        Snapshot::new_v2(
            123,
            vec![
                EnvironmentVariable::new(
                    Scope::User,
                    "DEMO".into(),
                    "secret '$`\r\n".into(),
                    RegistryValueKind::String,
                )
                .unwrap(),
                EnvironmentVariable::new(
                    Scope::System,
                    "PATHLIKE".into(),
                    "%SystemRoot%\\Demo".into(),
                    RegistryValueKind::ExpandString,
                )
                .unwrap(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn create_backup_commits_the_new_grouped_artifact_layout() {
        let root = temp_dir("create");
        let record = create_backup(&root, &snapshot()).unwrap();
        assert!(record.directory.join("snapshot.json").is_file());
        assert!(record.directory.join("summary.md").is_file());
        assert!(record.directory.join("restore/user.ps1").is_file());
        assert!(record.directory.join("restore/system.ps1").is_file());
        assert!(record.directory.join("restore/all.ps1").is_file());
        assert!(!record.directory.join("summary.txt").exists());
        assert!(!record.directory.join("restore-user-env.ps1").exists());
        assert!(!record.directory.join("restore-machine-env.ps1").exists());

        let summary = fs::read_to_string(record.directory.join("summary.md")).unwrap();
        assert!(summary.contains("| Variable | Type | Redacted value |"));
        assert!(summary.contains("<code>DEMO</code>"));
        assert!(summary.contains("<code>PATHLIKE</code>"));
        assert!(!summary.contains("secret '$`"));
        assert_eq!(list_backups(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn localized_summary_is_structured_and_redacts_sensitive_paths_and_urls() {
        let root = temp_dir("localized-summary");
        let snapshot = Snapshot::new_v2(
            123,
            vec![
                EnvironmentVariable::new(
                    Scope::User,
                    "OPENAI_API_KEY".into(),
                    "sk-sensitive-fixture".into(),
                    RegistryValueKind::String,
                )
                .unwrap(),
                EnvironmentVariable::new(
                    Scope::User,
                    "Path".into(),
                    r"C:\Users\Alice\bin;C:\Program Files\Rust\bin".into(),
                    RegistryValueKind::ExpandString,
                )
                .unwrap(),
                EnvironmentVariable::new(
                    Scope::System,
                    "SERVICE_ENDPOINT".into(),
                    "https://example.com/api?token=fixture".into(),
                    RegistryValueKind::String,
                )
                .unwrap(),
            ],
        )
        .unwrap();

        let record =
            create_backup_with_language(&root, &snapshot, SummaryLanguage::Chinese).unwrap();
        let summary = fs::read_to_string(record.directory.join("summary.md")).unwrap();

        assert!(summary.starts_with("# VarKeep 备份摘要"));
        assert!(summary.contains("| 创建时间 | 用户变量 | 系统变量 | 合计 |"));
        assert!(summary.contains("## 用户变量"));
        assert!(summary.contains("## 系统变量"));
        assert!(summary.contains(r"C:\Users\***\bin"));
        assert!(summary.contains("https://example.com/api?***"));
        assert!(summary.contains("疑似敏感值"));
        assert!(!summary.contains("Alice"));
        assert!(!summary.contains("sk-sensitive-fixture"));
        assert!(!summary.contains("token=fixture"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn optional_note_round_trips_and_can_be_cleared() {
        let root = temp_dir("note-round-trip");
        let record = create_backup(&root, &snapshot()).unwrap();
        assert_eq!(record.note, "");

        update_backup_note(&root, &record.name, "  系统升级前  ").unwrap();
        let records = list_backups(&root).unwrap();
        assert_eq!(records[0].note, "系统升级前");
        assert_eq!(
            fs::read_to_string(record.directory.join(NOTE_FILE)).unwrap(),
            "系统升级前"
        );

        update_backup_note(&root, &record.name, "   ").unwrap();
        assert!(!record.directory.join(NOTE_FILE).exists());
        assert_eq!(list_backups(&root).unwrap()[0].note, "");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_note_input_is_rejected_without_overwriting_the_previous_note() {
        let root = temp_dir("note-validation");
        let record = create_backup(&root, &snapshot()).unwrap();
        update_backup_note(&root, &record.name, "保留").unwrap();

        for invalid in [
            "line one\nline two".to_owned(),
            "\nleading newline".to_owned(),
            "nul\0byte".to_owned(),
            "字".repeat(101),
        ] {
            assert_eq!(
                update_backup_note(&root, &record.name, &invalid)
                    .unwrap_err()
                    .code(),
                "invalid_note"
            );
        }
        assert_eq!(list_backups(&root).unwrap()[0].note, "保留");
        assert_eq!(
            update_backup_note(&root, "../outside", "nope")
                .unwrap_err()
                .code(),
            "backup_not_found"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_backups_compare_old_to_new_without_exposing_values() {
        let root = temp_dir("compare-named");
        let baseline = create_backup(&root, &snapshot()).unwrap();
        let current_snapshot = Snapshot::new_v2(
            456,
            vec![
                EnvironmentVariable::new(
                    Scope::User,
                    "DEMO".into(),
                    "new-secret".into(),
                    RegistryValueKind::String,
                )
                .unwrap(),
                EnvironmentVariable::new(
                    Scope::System,
                    "ADDED".into(),
                    "another-secret".into(),
                    RegistryValueKind::String,
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let current = create_backup(&root, &current_snapshot).unwrap();

        assert_eq!(
            compare_named_backups(&root, &baseline.name, &baseline.name)
                .unwrap_err()
                .code(),
            "same_backup"
        );

        let result = compare_named_backups(&root, &baseline.name, &current.name).unwrap();

        assert_eq!(result.added, 1);
        assert_eq!(result.removed, 1);
        assert_eq!(result.changed, 1);
        assert_eq!(result.unchanged, 0);
        let debug = format!("{result:?}");
        assert!(!debug.contains("new-secret"));
        assert!(!debug.contains("another-secret"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tampered_summary_is_rejected_before_it_can_be_opened() {
        let root = temp_dir("summary-mismatch");
        let record = create_backup(&root, &snapshot()).unwrap();
        fs::write(record.directory.join("summary.md"), "# forged summary").unwrap();

        assert_eq!(
            validate_backup_directory(&record.directory)
                .unwrap_err()
                .code(),
            "summary_mismatch"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn old_flat_artifact_layout_is_rejected_without_mutation() {
        let root = temp_dir("old-layout");
        let record = create_backup(&root, &snapshot()).unwrap();
        let snapshot_before = fs::read(record.directory.join("snapshot.json")).unwrap();

        fs::remove_dir_all(record.directory.join("restore")).unwrap();
        fs::write(record.directory.join("summary.txt"), "old summary").unwrap();
        fs::write(record.directory.join("restore-user-env.ps1"), "# old").unwrap();
        fs::write(record.directory.join("restore-machine-env.ps1"), "# old").unwrap();

        assert_eq!(
            validate_backup_directory(&record.directory)
                .unwrap_err()
                .code(),
            "unexpected_backup_entry"
        );
        assert_eq!(
            fs::read(record.directory.join("snapshot.json")).unwrap(),
            snapshot_before
        );
        assert!(record.directory.join("summary.txt").is_file());
        assert!(record.directory.join("restore-user-env.ps1").is_file());
        assert!(record.directory.join("restore-machine-env.ps1").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_backup_delete_removes_only_the_selected_complete_backup() {
        let root = temp_dir("delete-selected");
        let first = create_backup(&root, &snapshot()).unwrap();
        let second_snapshot = Snapshot::new_v2(456, snapshot().variables.clone()).unwrap();
        let second = create_backup(&root, &second_snapshot).unwrap();

        delete_named_backup(&root, &first.name).unwrap();

        assert!(!first.directory.exists());
        assert!(second.directory.is_dir());
        assert_eq!(list_backups(&root).unwrap(), vec![second]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_backup_delete_rejects_unknown_or_tampered_directories() {
        let root = temp_dir("delete-invalid");
        let record = create_backup(&root, &snapshot()).unwrap();
        let unrelated = root.join("unrelated");
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("keep.txt"), "keep").unwrap();

        assert_eq!(
            delete_named_backup(&root, "../unrelated")
                .unwrap_err()
                .code(),
            "backup_not_found"
        );
        assert!(unrelated.join("keep.txt").is_file());

        fs::write(record.directory.join("restore/user.ps1"), "# tampered").unwrap();
        assert_eq!(
            delete_named_backup(&root, &record.name).unwrap_err().code(),
            "backup_not_found"
        );
        assert!(record.directory.is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn partial_directories_are_ignored_without_being_modified() {
        let root = temp_dir("partial");
        let partial = root.join(".partial-env-backup-1");
        fs::create_dir_all(&partial).unwrap();
        fs::write(partial.join("snapshot.json"), "sensitive-test-fixture").unwrap();
        fs::create_dir_all(root.join("env-backup-2")).unwrap();
        fs::write(root.join("env-backup-2/snapshot.json"), "{}").unwrap();
        let listing = list_backups_with_diagnostics(&root).unwrap();
        assert!(listing.records.is_empty());
        assert_eq!(listing.invalid_count, 1);
        assert!(partial.join("snapshot.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unrelated_root_entry_count_is_bounded_before_processing() {
        let root = temp_dir("root-entry-limit");
        fs::create_dir_all(&root).unwrap();
        for index in 0..=MAX_ROOT_ENTRIES {
            fs::write(root.join(format!("unrelated-{index}")), []).unwrap();
        }
        assert_eq!(
            list_backups_with_diagnostics(&root).unwrap_err().code(),
            "too_many_backup_root_entries"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tampered_restore_script_is_reported_as_invalid_without_a_path() {
        let root = temp_dir("script-mismatch");
        for (index, file) in ["restore/user.ps1", "restore/system.ps1"]
            .into_iter()
            .enumerate()
        {
            let backup_root = root.join(index.to_string());
            let record = create_backup(&backup_root, &snapshot()).unwrap();
            fs::write(record.directory.join(file), "# truncated").unwrap();
            let listing = list_backups_with_diagnostics(&backup_root).unwrap();
            assert!(listing.records.is_empty());
            assert_eq!(listing.invalid_count, 1);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backups_are_sorted_by_snapshot_time_then_name() {
        let root = temp_dir("sort");
        let first = Snapshot::new_v2(100, snapshot().variables.clone()).unwrap();
        let newest = Snapshot::new_v2(300, snapshot().variables.clone()).unwrap();
        let middle = Snapshot::new_v2(200, snapshot().variables.clone()).unwrap();
        create_backup(&root, &first).unwrap();
        for _ in 0..12 {
            create_backup(&root, &newest).unwrap();
        }
        create_backup(&root, &middle).unwrap();

        let records = list_backups(&root).unwrap();
        assert_eq!(records[0].created_at_unix_ms, 300);
        assert_eq!(records[1].created_at_unix_ms, 300);
        assert_eq!(records[0].name, "env-backup-300-11");
        assert_eq!(records[1].name, "env-backup-300-10");
        assert_eq!(records[11].name, "env-backup-300");
        assert_eq!(records[12].created_at_unix_ms, 200);
        assert_eq!(records[13].created_at_unix_ms, 100);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_candidate_count_is_bounded() {
        let root = temp_dir("candidate-limit");
        fs::create_dir_all(&root).unwrap();
        for index in 0..=MAX_BACKUP_CANDIDATES {
            fs::create_dir(root.join(format!("env-backup-{index}"))).unwrap();
        }
        assert_eq!(
            list_backups_with_diagnostics(&root).unwrap_err().code(),
            "too_many_backup_candidates"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_directory_entry_count_is_bounded() {
        let root = temp_dir("entry-limit");
        let record = create_backup(&root, &snapshot()).unwrap();
        for index in 0..MAX_DIRECTORY_ENTRIES {
            fs::write(record.directory.join(format!("extra-{index}")), []).unwrap();
        }
        let listing = list_backups_with_diagnostics(&root).unwrap();
        assert!(listing.records.is_empty());
        assert_eq!(listing.invalid_count, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn script_regeneration_creates_a_new_complete_backup() {
        let root = temp_dir("script-regenerate");
        let original = create_backup(&root, &snapshot()).unwrap();
        let original_snapshot = fs::read(original.directory.join("snapshot.json")).unwrap();
        let regenerated = regenerate_restore_scripts(&original.directory).unwrap();
        assert_ne!(original.directory, regenerated.directory);
        assert_eq!(
            fs::read(original.directory.join("snapshot.json")).unwrap(),
            original_snapshot
        );
        for file in REQUIRED_FILES {
            assert!(regenerated.directory.join(file).is_file(), "missing {file}");
        }
        assert_eq!(list_backups(&root).unwrap().len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_restore_script_is_rejected_before_rendering() {
        let value = "x".repeat(crate::domain::MAX_VALUE_UTF16_BYTES / 2);
        let variables = (0..17)
            .map(|index| {
                EnvironmentVariable::new(
                    Scope::User,
                    format!("LARGE_{index}"),
                    value.clone(),
                    RegistryValueKind::String,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let large = Snapshot::new_v2(1, variables).unwrap();
        let Err(error) = generate_restore_scripts(&large) else {
            panic!("oversized restore script was accepted");
        };
        assert_eq!(error.code(), "file_too_large");
    }

    #[cfg(windows)]
    #[test]
    fn backup_root_reparse_point_is_rejected() {
        use std::os::windows::fs::symlink_dir;

        let root = temp_dir("reparse");
        let target = root.join("target");
        let link = root.join("backups");
        fs::create_dir_all(&target).unwrap();
        if symlink_dir(&target, &link).is_err() {
            fs::remove_dir_all(root).unwrap();
            return;
        }
        assert_eq!(
            create_backup(&link, &snapshot()).unwrap_err().code(),
            "backup_path_reparse"
        );
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn backup_candidate_reparse_point_is_ignored() {
        use std::os::windows::fs::symlink_dir;

        let root = temp_dir("candidate-reparse");
        let backups = root.join("backups");
        let target = root.join("outside");
        let link = backups.join("env-backup-junction");
        fs::create_dir_all(&backups).unwrap();
        fs::create_dir_all(&target).unwrap();
        if symlink_dir(&target, &link).is_err() {
            fs::remove_dir_all(root).unwrap();
            return;
        }
        let listing = list_backups_with_diagnostics(&backups).unwrap();
        assert!(listing.records.is_empty());
        assert_eq!(listing.invalid_count, 1);
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generated_scripts_cover_user_system_and_combined_scopes_without_execution() {
        let scripts = generate_restore_scripts(&snapshot()).unwrap();
        assert!(scripts.user.contains("CurrentUser"));
        assert!(!scripts.user.contains("LocalMachine"));
        assert!(scripts.system.contains("LocalMachine"));
        assert!(scripts.all.contains("CurrentUser"));
        assert!(scripts.all.contains("LocalMachine"));
        let admin_check = scripts.all.find("WindowsPrincipal").unwrap();
        let first_registry_write = scripts.all.find("OpenSubKey").unwrap();
        assert!(admin_check < first_registry_write);
        for script in [&scripts.user, &scripts.system, &scripts.all] {
            assert!(script.starts_with("# Generated by VarKeep v2."));
            assert!(script.contains("$key.SetValue"));
            assert!(script.contains("SendMessageTimeout"));
            assert!(script.contains("Environment change notification failed"));
            assert!(
                script.rfind("$key.SetValue").unwrap() < script.find("SendMessageTimeout").unwrap()
            );
            assert!(!script.contains("Remove"));
            assert!(!script.contains("Start-Process"));
            assert!(!script.contains("Invoke-Expression"));
        }
    }

    #[test]
    fn generated_scripts_round_trip_scope_name_value_and_kind() {
        let snapshot = snapshot();
        let scripts = generate_restore_scripts(&snapshot).unwrap();

        for (scope, script) in [(Scope::User, scripts.user), (Scope::System, scripts.system)] {
            let actual = parse_script_entries(&script);
            let expected = snapshot
                .variables
                .iter()
                .filter(|variable| variable.scope == scope)
                .collect::<Vec<_>>();
            assert_eq!(actual.len(), expected.len());

            for ((name, value, kind), variable) in actual.iter().zip(expected) {
                let expected_kind = match variable.kind {
                    RegistryValueKind::String => "String",
                    RegistryValueKind::ExpandString => "ExpandString",
                };
                assert!(name == &variable.name);
                assert!(value == variable.value.expose());
                assert!(kind == expected_kind);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn generated_user_script_executes_against_isolated_registry_key() {
        let key_path = format!(
            "Software\\VarKeepTests-v2-{}-{}",
            std::process::id(),
            now_unix_ms()
        );
        let setup = format!(
            "[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('{}').Dispose()",
            key_path
        );
        assert!(
            std::process::Command::new("pwsh")
                .args(["-NoProfile", "-Command", &setup])
                .status()
                .unwrap()
                .success()
        );

        let root = temp_dir("execute-script");
        fs::create_dir_all(&root).unwrap();
        let script_path = root.join("user.ps1");
        let mut isolated_snapshot = snapshot();
        isolated_snapshot.variables[1].scope = Scope::User;
        let script = generate_restore_scripts(&isolated_snapshot)
            .unwrap()
            .user
            .replace(
                "OpenSubKey('Environment', $true)",
                &format!("OpenSubKey('{key_path}', $true)"),
            );
        fs::write(&script_path, script).unwrap();
        let execution = std::process::Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(&script_path)
            .status()
            .unwrap();
        let query = format!(
            "$ErrorActionPreference='Stop'; $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('{}'); $value=$key.GetValue('DEMO',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames); $kind=$key.GetValueKind('DEMO'); $expanded=$key.GetValue('PATHLIKE',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames); $expandedKind=$key.GetValueKind('PATHLIKE'); [Console]::Write($kind.ToString()+'|'+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($value))+'|'+$expandedKind.ToString()+'|'+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($expanded))); $key.Dispose()",
            key_path
        );
        let query_output = std::process::Command::new("pwsh")
            .args(["-NoProfile", "-Command", &query])
            .output()
            .unwrap();
        let cleanup = format!(
            "[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree('{}',$false)",
            key_path
        );
        let cleanup_status = std::process::Command::new("pwsh")
            .args(["-NoProfile", "-Command", &cleanup])
            .status()
            .unwrap();
        fs::remove_dir_all(root).unwrap();

        assert!(execution.success());
        assert!(query_output.status.success());
        assert!(cleanup_status.success());
        let expected = format!(
            "String|{}|ExpandString|{}",
            base64(
                &"secret '$`\r\n"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>()
            ),
            base64(
                &"%SystemRoot%\\Demo"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(String::from_utf8(query_output.stdout).unwrap(), expected);
    }

    #[cfg(windows)]
    #[test]
    fn generated_scripts_pass_powershell_ast_allowlist() {
        let root = temp_dir("ast");
        fs::create_dir_all(&root).unwrap();
        let scripts = generate_restore_scripts(&snapshot()).unwrap();
        let user = root.join("user.ps1");
        let system = root.join("system.ps1");
        let all = root.join("all.ps1");
        fs::write(&user, scripts.user).unwrap();
        fs::write(&system, scripts.system).unwrap();
        fs::write(&all, scripts.all).unwrap();
        let checker = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("scripts/check-script-ast.ps1");
        for script in [&user, &system, &all] {
            let status = std::process::Command::new("pwsh")
                .args(["-NoProfile", "-File"])
                .arg(&checker)
                .arg("-Path")
                .arg(script)
                .status()
                .unwrap();
            assert!(status.success());
        }
        fs::remove_dir_all(root).unwrap();
    }

    fn parse_script_entries(script: &str) -> Vec<(String, String, String)> {
        let mut output = Vec::new();
        let mut name = None;
        let mut value = None;
        for line in script.lines() {
            if line.contains("$name =") {
                name = Some(decode_script_string(quoted_argument(line)));
            } else if line.contains("$value =") {
                value = Some(decode_script_string(quoted_argument(line)));
            } else if line.contains("$key.SetValue") {
                let kind = line
                    .rsplit("::")
                    .next()
                    .and_then(|suffix| suffix.strip_suffix(')'))
                    .unwrap()
                    .to_owned();
                output.push((name.take().unwrap(), value.take().unwrap(), kind));
            }
        }
        output
    }

    fn quoted_argument(line: &str) -> &str {
        line.split('\'').nth(1).unwrap()
    }

    fn decode_script_string(encoded: &str) -> String {
        let mut bytes = Vec::new();
        let mut chunks = encoded.as_bytes().chunks_exact(4);
        for chunk in &mut chunks {
            let values = [
                base64_value(chunk[0]),
                base64_value(chunk[1]),
                base64_value(chunk[2]),
                base64_value(chunk[3]),
            ];
            bytes.push((values[0] << 2) | (values[1] >> 4));
            if chunk[2] != b'=' {
                bytes.push((values[1] << 4) | (values[2] >> 2));
            }
            if chunk[3] != b'=' {
                bytes.push((values[2] << 6) | values[3]);
            }
        }
        assert!(chunks.remainder().is_empty());
        let mut words = bytes.chunks_exact(2);
        let decoded = words
            .by_ref()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        assert!(words.remainder().is_empty());
        String::from_utf16(&decoded).unwrap()
    }

    fn base64_value(byte: u8) -> u8 {
        match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 0,
            _ => panic!("invalid Base64 fixture"),
        }
    }
}
