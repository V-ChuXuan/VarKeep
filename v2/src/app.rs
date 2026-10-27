use crate::domain::{Snapshot, compare_snapshots};
use crate::storage::{
    BackupListing, BackupRecord, InvalidBackup, compare_backup_order, create_backup_with_language,
    delete_named_backup, list_backups_with_diagnostics, now_unix_ms, resolve_named_backup,
    resolve_named_snapshot, update_backup_note,
};
use crate::summary::SummaryLanguage;
use crate::ui_adapter::UiStatus;
use crate::windows::{
    format_local_timestamp, open_directory, open_file, read_persistent_environment,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

#[derive(Clone)]
pub enum Job {
    Refresh,
    CreateBackup { language: SummaryLanguage },
    CompareWithCurrent { name: String },
    CompareBackups { first: String, second: String },
    ViewSummary { name: String },
    OpenBackup { name: String },
    UpdateNote { name: String, note: String },
    DeleteBackup { name: String },
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiEvent {
    Ready {
        status: UiStatus,
        records: Vec<BackupRecord>,
        invalid_count: usize,
        invalid_backups: Vec<InvalidBackup>,
    },
    Error {
        code: &'static str,
        listing: Option<BackupListing>,
    },
}

pub struct Worker {
    pub jobs: Sender<Job>,
    pub events: Receiver<UiEvent>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(base_directory: PathBuf) -> Self {
        let (job_tx, job_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || run_worker(base_directory, job_rx, event_tx));
        Self {
            jobs: job_tx,
            events: event_rx,
            handle: Some(handle),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.jobs.send(Job::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_worker(base: PathBuf, jobs: Receiver<Job>, events: Sender<UiEvent>) {
    let root = base.join("backups");
    while let Ok(job) = jobs.recv() {
        if matches!(job, Job::Shutdown) {
            break;
        }
        let result = handle_job(&root, job);
        let event = match result {
            Ok((status, listing)) => UiEvent::Ready {
                status,
                records: listing.records,
                invalid_count: listing.invalid_count,
                invalid_backups: listing.invalid_backups,
            },
            Err(code) => UiEvent::Error {
                code,
                listing: list_backups_with_diagnostics(&root).ok(),
            },
        };
        if events.send(event).is_err() {
            break;
        }
    }
}

fn handle_job(root: &std::path::Path, job: Job) -> Result<(UiStatus, BackupListing), &'static str> {
    let status = match job {
        Job::Refresh => {
            let listing = list_backups_with_diagnostics(root).map_err(|error| error.code())?;
            return Ok((UiStatus::BackupsFound(listing.records.len()), listing));
        }
        Job::CreateBackup { language } => {
            let variables = read_persistent_environment().map_err(|error| error.code())?;
            let snapshot =
                Snapshot::new_v2(now_unix_ms(), variables).map_err(|error| error.code())?;
            let record = create_backup_with_language(root, &snapshot, language)
                .map_err(|error| error.code())?;
            UiStatus::BackupCreated(record.name)
        }
        Job::CompareWithCurrent { name } => {
            let (record, snapshot) =
                resolve_named_snapshot(root, &name).map_err(|error| error.code())?;
            let current = read_persistent_environment().map_err(|error| error.code())?;
            let summary = crate::storage::compare_backup_with(&snapshot, &current);
            UiStatus::Comparison {
                summary,
                baseline: comparison_label(&record),
                current: None,
            }
        }
        Job::CompareBackups { first, second } => {
            if first == second {
                return Err("same_backup");
            }
            let (first_record, first_snapshot) =
                resolve_named_snapshot(root, &first).map_err(|error| error.code())?;
            let (second_record, second_snapshot) =
                resolve_named_snapshot(root, &second).map_err(|error| error.code())?;
            let (baseline, current, baseline_record, current_record) =
                if compare_backup_order(&first_record, &second_record).is_gt() {
                    (
                        &first_snapshot,
                        &second_snapshot,
                        &first_record,
                        &second_record,
                    )
                } else {
                    (
                        &second_snapshot,
                        &first_snapshot,
                        &second_record,
                        &first_record,
                    )
                };
            let summary = compare_snapshots(&baseline.variables, &current.variables);
            UiStatus::Comparison {
                summary,
                baseline: comparison_label(baseline_record),
                current: Some(comparison_label(current_record)),
            }
        }
        Job::ViewSummary { name } => {
            let record = resolve_named_backup(root, &name).map_err(|error| error.code())?;
            open_file(&record.directory.join("summary.md")).map_err(|error| error.code())?;
            UiStatus::SummaryOpened
        }
        Job::OpenBackup { name } => {
            let record = resolve_named_backup(root, &name).map_err(|error| error.code())?;
            open_directory(&record.directory).map_err(|error| error.code())?;
            UiStatus::DirectoryOpened
        }
        Job::UpdateNote { name, note } => {
            update_backup_note(root, &name, &note).map_err(|error| error.code())?;
            UiStatus::NoteUpdated(name)
        }
        Job::DeleteBackup { name } => {
            delete_named_backup(root, &name).map_err(|error| error.code())?;
            UiStatus::BackupDeleted(name)
        }
        Job::Shutdown => unreachable!(),
    };
    let listing = list_backups_with_diagnostics(root).map_err(|error| error.code())?;
    Ok((status, listing))
}

fn comparison_label(record: &BackupRecord) -> String {
    format!(
        "{} ({})",
        format_local_timestamp(record.created_at_unix_ms),
        record.name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{EnvironmentVariable, RegistryValueKind, Scope};
    use crate::storage::{create_backup, now_unix_ms};
    use std::fs;
    use std::time::Duration;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("varkeep-app-{name}-{}", now_unix_ms()))
    }

    fn snapshot(stamp: u64, include_added: bool) -> Snapshot {
        let mut variables = vec![
            EnvironmentVariable::new(
                Scope::User,
                "BASE".into(),
                "secret".into(),
                RegistryValueKind::String,
            )
            .unwrap(),
        ];
        if include_added {
            variables.push(
                EnvironmentVariable::new(
                    Scope::System,
                    "ADDED".into(),
                    "another-secret".into(),
                    RegistryValueKind::String,
                )
                .unwrap(),
            );
        }
        Snapshot::new_v2(stamp, variables).unwrap()
    }

    #[test]
    fn compare_backups_orders_selected_records_from_old_to_new() {
        let root = temp_dir("compare");
        let older = create_backup(&root, &snapshot(100, false)).unwrap();
        let newer = create_backup(&root, &snapshot(200, true)).unwrap();

        let (status, _) = handle_job(
            &root,
            Job::CompareBackups {
                first: newer.name.clone(),
                second: older.name.clone(),
            },
        )
        .unwrap();

        let UiStatus::Comparison {
            summary,
            baseline,
            current,
        } = status
        else {
            panic!("comparison status was not returned");
        };
        assert_eq!(summary.added, 1);
        assert_eq!(summary.removed, 0);
        assert!(baseline.contains(&older.name));
        assert!(current.unwrap().contains(&newer.name));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compare_backups_orders_timestamp_collisions_independently_of_selection_order() {
        let root = temp_dir("compare-collision");
        let older = create_backup(&root, &snapshot(100, false)).unwrap();
        let newer = create_backup(&root, &snapshot(100, true)).unwrap();
        for (first, second) in [(&older.name, &newer.name), (&newer.name, &older.name)] {
            let (status, _) = handle_job(
                &root,
                Job::CompareBackups {
                    first: first.clone(),
                    second: second.clone(),
                },
            )
            .unwrap();
            let UiStatus::Comparison {
                summary,
                baseline,
                current,
            } = status
            else {
                panic!("comparison status was not returned");
            };
            assert_eq!(summary.added, 1);
            assert_eq!(summary.removed, 0);
            assert!(baseline.contains(&older.name));
            assert!(current.unwrap().contains(&newer.name));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_backup_job_removes_only_the_named_record() {
        let root = temp_dir("delete");
        let selected = create_backup(&root, &snapshot(100, false)).unwrap();
        let kept = create_backup(&root, &snapshot(200, true)).unwrap();

        let (status, listing) = handle_job(
            &root,
            Job::DeleteBackup {
                name: selected.name.clone(),
            },
        )
        .unwrap();

        assert_eq!(status, UiStatus::BackupDeleted(selected.name));
        assert_eq!(listing.records, vec![kept]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_job_can_recover_an_imported_over_capacity_history() {
        let root = temp_dir("delete-over-capacity");
        let selected = create_backup(&root, &snapshot(100, false)).unwrap();
        for index in 0..crate::storage::MAX_BACKUP_CANDIDATES {
            fs::create_dir(root.join(format!("env-backup-invalid-{index}"))).unwrap();
        }
        let (status, listing) = handle_job(
            &root,
            Job::DeleteBackup {
                name: selected.name.clone(),
            },
        )
        .unwrap();
        assert_eq!(status, UiStatus::BackupDeleted(selected.name));
        assert!(listing.records.is_empty());
        assert_eq!(listing.invalid_count, crate::storage::MAX_BACKUP_CANDIDATES);
        assert!(!selected.directory.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn update_note_job_refreshes_only_the_selected_backup_metadata() {
        let root = temp_dir("note-update");
        let selected = create_backup(&root, &snapshot(100, false)).unwrap();
        let kept = create_backup(&root, &snapshot(200, true)).unwrap();

        let (status, listing) = handle_job(
            &root,
            Job::UpdateNote {
                name: selected.name.clone(),
                note: "系统升级前".into(),
            },
        )
        .unwrap();

        assert_eq!(status, UiStatus::NoteUpdated(selected.name.clone()));
        assert_eq!(
            listing
                .records
                .iter()
                .find(|record| record.name == selected.name)
                .unwrap()
                .note,
            "系统升级前"
        );
        assert_eq!(
            listing
                .records
                .iter()
                .find(|record| record.name == kept.name)
                .unwrap()
                .note,
            ""
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worker_refreshes_the_listing_after_a_failed_delete() {
        let base = temp_dir("failed-delete-refresh");
        let root = base.join("backups");
        let selected = create_backup(&root, &snapshot(100, false)).unwrap();
        fs::write(selected.directory.join("summary.md"), "tampered").unwrap();
        let worker = Worker::start(base.clone());

        worker
            .jobs
            .send(Job::DeleteBackup {
                name: selected.name,
            })
            .unwrap();
        let event = worker.events.recv_timeout(Duration::from_secs(5)).unwrap();

        let UiEvent::Error { code, listing } = event else {
            panic!("failed delete did not return a refreshed error event");
        };
        let listing = listing.expect("failed delete should still permit a bounded refresh");
        assert_eq!(code, "backup_not_found");
        assert!(listing.records.is_empty());
        assert_eq!(listing.invalid_count, 1);
        drop(worker);
        fs::remove_dir_all(base).unwrap();
    }
}
