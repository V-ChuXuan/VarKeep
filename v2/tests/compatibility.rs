use std::path::PathBuf;
use varkeep::domain::{RegistryValueKind, Scope};
use varkeep::storage::{generate_restore_scripts, validate_backup_directory};
use varkeep::summary::{SummaryLanguage, render_summary};

#[test]
fn v23_synthetic_backup_remains_valid_without_migration() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v2.3-backup");
    let snapshot = validate_backup_directory(&directory).unwrap();
    assert_eq!(snapshot.schema_version, 2);
    assert_eq!(snapshot.created_at_unix_ms, 1_700_000_000_000);
    assert_eq!(snapshot.variables.len(), 3);
    assert!(snapshot.variables.iter().any(|value| {
        value.scope == Scope::User
            && value.name == "Path"
            && value.kind == RegistryValueKind::ExpandString
            && value.value.expose() == r"C:\Tools;%SystemRoot%\System32"
    }));
    assert_eq!(
        render_summary(&snapshot, SummaryLanguage::English).as_bytes(),
        include_bytes!("fixtures/v2.3-backup/summary.md")
    );
    let scripts = generate_restore_scripts(&snapshot).unwrap();
    assert_eq!(
        scripts.user.as_bytes(),
        include_bytes!("fixtures/v2.3-backup/restore/user.ps1")
    );
    assert_eq!(
        scripts.system.as_bytes(),
        include_bytes!("fixtures/v2.3-backup/restore/system.ps1")
    );
    assert_eq!(
        scripts.all.as_bytes(),
        include_bytes!("fixtures/v2.3-backup/restore/all.ps1")
    );
}
