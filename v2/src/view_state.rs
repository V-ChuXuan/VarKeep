use std::time::{Duration, Instant};
use varkeep::app::Job;
use varkeep::storage::{BackupRecord, InvalidBackup};
use varkeep::ui_adapter::{Language, UiStatus};

pub(crate) struct ViewState {
    pub(crate) language: Language,
    pub(crate) status: UiStatus,
    pub(crate) records: Vec<BackupRecord>,
    pub(crate) invalid_count: usize,
    pub(crate) invalid_backups: Vec<InvalidBackup>,
    pub(crate) active_name: Option<String>,
    pub(crate) selected_names: Vec<String>,
    pub(crate) pending_note_name: Option<String>,
    pub(crate) note_draft: String,
    pub(crate) pending_delete_name: Option<String>,
    pub(crate) create_completed_until: Option<Instant>,
    pub(crate) transient_status_until: Option<Instant>,
}

impl ViewState {
    pub(crate) fn contains_record(&self, name: &str) -> bool {
        self.records.iter().any(|record| record.name == name)
    }

    pub(crate) fn activate(&mut self, name: &str) {
        if self.contains_record(name) {
            self.active_name = Some(name.to_owned());
        }
    }

    pub(crate) fn toggle_selection(&mut self, name: &str) {
        self.activate(name);
        if let Some(index) = self.selected_names.iter().position(|item| item == name) {
            self.selected_names.remove(index);
        } else if self.selected_names.len() < 2 && self.contains_record(name) {
            self.selected_names.push(name.to_owned());
        }
    }

    pub(crate) fn comparison_job(&self) -> Option<Job> {
        match self.selected_names.as_slice() {
            [name] => Some(Job::CompareWithCurrent { name: name.clone() }),
            [first, second] => Some(Job::CompareBackups {
                first: first.clone(),
                second: second.clone(),
            }),
            _ => None,
        }
    }

    pub(crate) fn request_delete(&mut self, name: &str) {
        if self.contains_record(name) {
            self.pending_delete_name = Some(name.to_owned());
        }
    }

    pub(crate) fn request_note_edit(&mut self, name: &str) {
        if let Some(record) = self.records.iter().find(|record| record.name == name) {
            self.active_name = Some(name.to_owned());
            self.pending_note_name = Some(name.to_owned());
            self.note_draft = record.note.clone();
        }
    }

    pub(crate) fn cancel_note_edit(&mut self) {
        self.pending_note_name = None;
        self.note_draft.clear();
    }

    pub(crate) fn dismiss_comparison(&mut self) {
        if matches!(self.status, UiStatus::Comparison { .. }) {
            self.status = UiStatus::Ready;
        }
    }

    pub(crate) fn apply_event(
        &mut self,
        status: UiStatus,
        records: Vec<BackupRecord>,
        invalid_count: usize,
    ) {
        let deleted_index = match &status {
            UiStatus::BackupDeleted(name) => {
                self.records.iter().position(|record| record.name == *name)
            }
            _ => None,
        };

        self.records = records;
        self.invalid_count = invalid_count;
        self.selected_names
            .retain(|name| self.records.iter().any(|record| record.name == *name));
        let should_close_note_editor = matches!(&status, UiStatus::NoteUpdated(_))
            || (!matches!(&status, UiStatus::Error(_))
                && self
                    .pending_note_name
                    .as_deref()
                    .is_some_and(|name| !self.contains_record(name)));
        if should_close_note_editor {
            self.cancel_note_edit();
        }
        self.pending_delete_name = self
            .pending_delete_name
            .take()
            .filter(|name| self.records.iter().any(|record| record.name == *name));

        match &status {
            UiStatus::BackupCreated(name) if self.contains_record(name) => {
                self.active_name = Some(name.clone());
            }
            UiStatus::BackupDeleted(_) => {
                self.active_name = deleted_index
                    .and_then(|index| self.records.get(index).or_else(|| self.records.last()))
                    .map(|record| record.name.clone());
            }
            _ if self
                .active_name
                .as_deref()
                .is_some_and(|name| self.contains_record(name)) => {}
            _ => {
                self.active_name = self.records.first().map(|record| record.name.clone());
            }
        }
        self.create_completed_until = matches!(&status, UiStatus::BackupCreated(_))
            .then(|| Instant::now() + Duration::from_millis(800));
        self.transient_status_until = matches!(
            &status,
            UiStatus::NoteUpdated(_) | UiStatus::BackupDeleted(_)
        )
        .then(|| Instant::now() + Duration::from_millis(1_200));
        self.status = status;
    }

    pub(crate) fn expire_create_completed(&mut self, now: Instant) -> bool {
        if self
            .create_completed_until
            .is_some_and(|deadline| now >= deadline)
        {
            self.create_completed_until = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn expire_transient_status(&mut self, now: Instant) -> bool {
        if self
            .transient_status_until
            .is_some_and(|deadline| now >= deadline)
        {
            self.transient_status_until = None;
            if matches!(
                self.status,
                UiStatus::NoteUpdated(_) | UiStatus::BackupDeleted(_)
            ) {
                self.status = UiStatus::Ready;
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn record(name: &str, stamp: u64) -> BackupRecord {
        BackupRecord {
            name: name.into(),
            directory: PathBuf::from(name),
            created_at_unix_ms: stamp,
            variable_count: 1,
            note: String::new(),
        }
    }

    fn state() -> ViewState {
        ViewState {
            language: Language::Chinese,
            status: UiStatus::Ready,
            records: vec![
                record("new", 300),
                record("middle", 200),
                record("old", 100),
            ],
            invalid_count: 0,
            invalid_backups: Vec::new(),
            active_name: Some("new".into()),
            selected_names: Vec::new(),
            pending_note_name: None,
            note_draft: String::new(),
            pending_delete_name: None,
            create_completed_until: None,
            transient_status_until: None,
        }
    }

    #[test]
    fn selection_stops_at_two_and_keeps_checked_rows_removable() {
        let mut state = state();

        state.toggle_selection("new");
        state.toggle_selection("middle");
        state.toggle_selection("old");
        assert_eq!(state.selected_names, vec!["new", "middle"]);

        state.toggle_selection("new");
        state.toggle_selection("old");
        assert_eq!(state.selected_names, vec!["middle", "old"]);
    }

    #[test]
    fn comparison_job_matches_one_or_two_selected_records() {
        let mut state = state();
        state.toggle_selection("new");
        assert!(matches!(
            state.comparison_job(),
            Some(Job::CompareWithCurrent { name }) if name == "new"
        ));

        state.toggle_selection("old");
        assert!(matches!(
            state.comparison_job(),
            Some(Job::CompareBackups { first, second }) if first == "new" && second == "old"
        ));
    }

    #[test]
    fn deleting_active_record_selects_the_adjacent_record() {
        let mut state = state();
        state.active_name = Some("middle".into());
        state.selected_names = vec!["middle".into()];
        state.pending_delete_name = Some("middle".into());

        state.apply_event(
            UiStatus::BackupDeleted("middle".into()),
            vec![record("new", 300), record("old", 100)],
            0,
        );

        assert_eq!(state.active_name.as_deref(), Some("old"));
        assert!(state.selected_names.is_empty());
        assert!(state.pending_delete_name.is_none());
    }

    #[test]
    fn selecting_a_row_also_activates_its_keyboard_actions() {
        let mut state = state();

        state.toggle_selection("old");

        assert_eq!(state.active_name.as_deref(), Some("old"));
    }

    #[test]
    fn completed_create_feedback_expires_without_changing_the_result_status() {
        let mut state = state();
        let now = Instant::now();
        state.create_completed_until = Some(now);

        assert!(state.expire_create_completed(now));
        assert!(state.create_completed_until.is_none());
        assert_eq!(state.status, UiStatus::Ready);
    }

    #[test]
    fn saved_note_feedback_expires_and_returns_to_the_quiet_ready_state() {
        let mut state = state();
        let mut records = state.records.clone();
        records[0].note = "新备注".into();

        state.apply_event(UiStatus::NoteUpdated("new".into()), records, 0);
        let deadline = state
            .transient_status_until
            .expect("note feedback should have an expiry");

        assert!(state.expire_transient_status(deadline));
        assert_eq!(state.status, UiStatus::Ready);
        assert!(state.transient_status_until.is_none());
    }

    #[test]
    fn deleted_backup_feedback_expires_and_returns_to_the_quiet_ready_state() {
        let mut state = state();

        state.apply_event(
            UiStatus::BackupDeleted("middle".into()),
            vec![record("new", 300), record("old", 100)],
            0,
        );
        let deadline = state
            .transient_status_until
            .expect("delete feedback should have an expiry");

        assert!(state.expire_transient_status(deadline));
        assert_eq!(state.status, UiStatus::Ready);
        assert!(state.transient_status_until.is_none());
    }

    #[test]
    fn note_editor_tracks_the_selected_record_and_closes_after_save() {
        let mut state = state();
        state.records[1].note = "原备注".into();

        state.request_note_edit("middle");
        assert_eq!(state.pending_note_name.as_deref(), Some("middle"));
        assert_eq!(state.note_draft, "原备注");

        let mut records = state.records.clone();
        records[1].note = "新备注".into();
        state.apply_event(UiStatus::NoteUpdated("middle".into()), records, 0);

        assert!(state.pending_note_name.is_none());
        assert!(state.note_draft.is_empty());
        assert_eq!(state.records[1].note, "新备注");
    }

    #[test]
    fn failed_note_save_preserves_the_target_and_draft() {
        let mut state = state();
        state.records[1].note = "原备注".into();
        state.request_note_edit("middle");
        state.note_draft = "尚未保存的草稿".into();
        state.apply_event(
            UiStatus::Error("note_write_failed"),
            state.records.clone(),
            0,
        );

        assert_eq!(state.pending_note_name.as_deref(), Some("middle"));
        assert_eq!(state.note_draft, "尚未保存的草稿");
        assert_eq!(state.records[1].note, "原备注");
    }

    #[test]
    fn failed_note_save_preserves_draft_when_refreshed_listing_loses_target() {
        let mut state = state();
        state.request_note_edit("middle");
        state.note_draft = "尚未保存的草稿".into();

        state.apply_event(
            UiStatus::Error("backup_not_found"),
            vec![record("new", 300), record("old", 100)],
            1,
        );

        assert!(!state.contains_record("middle"));
        assert_eq!(state.pending_note_name.as_deref(), Some("middle"));
        assert_eq!(state.note_draft, "尚未保存的草稿");
        assert_eq!(state.status, UiStatus::Error("backup_not_found"));
    }
}
