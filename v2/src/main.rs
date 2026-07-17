#![windows_subsystem = "windows"]

use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use varkeep::app::{Job, UiEvent, Worker};
use varkeep::storage::BackupRecord;
use varkeep::summary::SummaryLanguage;
use varkeep::ui_adapter::{
    Language, UiStatus, render_comparison_detail, render_delete_prompt,
    render_invalid_backup_notice, render_selection_count, render_status, status_tone, strings,
};
use varkeep::windows::{format_local_timestamp, prefers_chinese_ui};

slint::include_modules!();

struct ViewState {
    language: Language,
    status: UiStatus,
    records: Vec<BackupRecord>,
    invalid_count: usize,
    active_name: Option<String>,
    selected_names: Vec<String>,
    pending_note_name: Option<String>,
    note_draft: String,
    pending_delete_name: Option<String>,
    create_completed_until: Option<Instant>,
    transient_status_until: Option<Instant>,
}

impl ViewState {
    fn contains_record(&self, name: &str) -> bool {
        self.records.iter().any(|record| record.name == name)
    }

    fn activate(&mut self, name: &str) {
        if self.contains_record(name) {
            self.active_name = Some(name.to_owned());
        }
    }

    fn toggle_selection(&mut self, name: &str) {
        self.activate(name);
        if let Some(index) = self.selected_names.iter().position(|item| item == name) {
            self.selected_names.remove(index);
        } else if self.selected_names.len() < 2 && self.contains_record(name) {
            self.selected_names.push(name.to_owned());
        }
    }

    fn comparison_job(&self) -> Option<Job> {
        match self.selected_names.as_slice() {
            [name] => Some(Job::CompareWithCurrent { name: name.clone() }),
            [first, second] => Some(Job::CompareBackups {
                first: first.clone(),
                second: second.clone(),
            }),
            _ => None,
        }
    }

    fn request_delete(&mut self, name: &str) {
        if self.contains_record(name) {
            self.pending_delete_name = Some(name.to_owned());
        }
    }

    fn request_note_edit(&mut self, name: &str) {
        if let Some(record) = self.records.iter().find(|record| record.name == name) {
            self.active_name = Some(name.to_owned());
            self.pending_note_name = Some(name.to_owned());
            self.note_draft = record.note.clone();
        }
    }

    fn cancel_note_edit(&mut self) {
        self.pending_note_name = None;
        self.note_draft.clear();
    }

    fn dismiss_comparison(&mut self) {
        if matches!(self.status, UiStatus::Comparison(_)) {
            self.status = UiStatus::Ready;
        }
    }

    fn apply_event(&mut self, status: UiStatus, records: Vec<BackupRecord>, invalid_count: usize) {
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
            || self
                .pending_note_name
                .as_deref()
                .is_some_and(|name| !self.contains_record(name));
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

    fn expire_create_completed(&mut self, now: Instant) -> bool {
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

    fn expire_transient_status(&mut self, now: Instant) -> bool {
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

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let language = if prefers_chinese_ui() {
        Language::Chinese
    } else {
        Language::English
    };
    let view_state = Rc::new(RefCell::new(ViewState {
        language,
        status: UiStatus::Ready,
        records: Vec::new(),
        invalid_count: 0,
        active_name: None,
        selected_names: Vec::new(),
        pending_note_name: None,
        note_draft: String::new(),
        pending_delete_name: None,
        create_completed_until: None,
        transient_status_until: None,
    }));
    apply_view(&ui, &view_state.borrow());

    let worker = Rc::new(RefCell::new(Worker::start(application_base_directory())));
    let jobs = worker.borrow().jobs.clone();

    connect_state_callback(
        &ui,
        &view_state,
        |ui, callback| ui.on_toggle_language(callback),
        |state| state.language = state.language.toggle(),
    );
    {
        let weak = ui.as_weak();
        let jobs = jobs.clone();
        let state = Rc::clone(&view_state);
        ui.on_create_backup(move || {
            let language = match state.borrow().language {
                Language::Chinese => SummaryLanguage::Chinese,
                Language::English => SummaryLanguage::English,
            };
            send_from_ui(&weak, &jobs, &state, Job::CreateBackup { language });
        });
    }

    {
        let weak = ui.as_weak();
        let state = Rc::clone(&view_state);
        ui.on_activate_backup(move |name| {
            let Some(ui) = weak.upgrade() else { return };
            state.borrow_mut().activate(name.as_str());
            apply_view(&ui, &state.borrow());
        });
    }
    {
        let weak = ui.as_weak();
        let state = Rc::clone(&view_state);
        ui.on_toggle_backup_selection(move |name| {
            let Some(ui) = weak.upgrade() else { return };
            state.borrow_mut().toggle_selection(name.as_str());
            apply_view(&ui, &state.borrow());
        });
    }
    connect_state_callback(
        &ui,
        &view_state,
        |ui, callback| ui.on_clear_backup_selection(callback),
        |state| state.selected_names.clear(),
    );
    connect_state_callback(
        &ui,
        &view_state,
        |ui, callback| ui.on_close_comparison(callback),
        ViewState::dismiss_comparison,
    );
    {
        let weak = ui.as_weak();
        let jobs = jobs.clone();
        let state = Rc::clone(&view_state);
        ui.on_compare_selected(move || {
            let Some(job) = state.borrow().comparison_job() else {
                return;
            };
            send_from_ui(&weak, &jobs, &state, job);
        });
    }
    connect_named_job(
        &ui,
        &jobs,
        &view_state,
        |name| Job::ViewSummary { name },
        |ui, callback| ui.on_view_summary(callback),
    );
    connect_named_job(
        &ui,
        &jobs,
        &view_state,
        |name| Job::OpenBackup { name },
        |ui, callback| ui.on_open_backup(callback),
    );
    {
        let weak = ui.as_weak();
        let state = Rc::clone(&view_state);
        ui.on_request_note_edit(move |name| {
            let Some(ui) = weak.upgrade() else { return };
            state.borrow_mut().request_note_edit(name.as_str());
            apply_view(&ui, &state.borrow());
        });
    }
    {
        let state = Rc::clone(&view_state);
        ui.on_note_draft_changed(move |note| {
            state.borrow_mut().note_draft = note.to_string();
        });
    }
    {
        let weak = ui.as_weak();
        let jobs = jobs.clone();
        let state = Rc::clone(&view_state);
        ui.on_save_note(move |note| {
            let Some(name) = state.borrow().pending_note_name.clone() else {
                return;
            };
            state.borrow_mut().note_draft = note.to_string();
            send_from_ui(
                &weak,
                &jobs,
                &state,
                Job::UpdateNote {
                    name,
                    note: note.to_string(),
                },
            );
        });
    }
    connect_state_callback(
        &ui,
        &view_state,
        |ui, callback| ui.on_cancel_note_edit(callback),
        ViewState::cancel_note_edit,
    );
    {
        let weak = ui.as_weak();
        let state = Rc::clone(&view_state);
        ui.on_request_delete(move |name| {
            let Some(ui) = weak.upgrade() else { return };
            state.borrow_mut().request_delete(name.as_str());
            apply_view(&ui, &state.borrow());
        });
    }
    {
        let weak = ui.as_weak();
        let jobs = jobs.clone();
        let state = Rc::clone(&view_state);
        ui.on_confirm_delete(move || {
            let Some(name) = state.borrow_mut().pending_delete_name.take() else {
                return;
            };
            send_from_ui(&weak, &jobs, &state, Job::DeleteBackup { name });
        });
    }
    connect_state_callback(
        &ui,
        &view_state,
        |ui, callback| ui.on_cancel_delete(callback),
        |state| state.pending_delete_name = None,
    );
    let weak = ui.as_weak();
    let event_worker = Rc::clone(&worker);
    let event_state = Rc::clone(&view_state);
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(80), move || {
        let now = Instant::now();
        let mut state = event_state.borrow_mut();
        let completion_expired = state.expire_create_completed(now);
        let status_expired = state.expire_transient_status(now);
        drop(state);
        let event = event_worker.borrow().events.try_recv().ok();
        if event.is_none() && !completion_expired && !status_expired {
            return;
        }
        let Some(ui) = weak.upgrade() else { return };
        if let Some(event) = event {
            ui.set_busy(false);
            let mut state = event_state.borrow_mut();
            match event {
                UiEvent::Ready {
                    status,
                    records,
                    invalid_count,
                } => state.apply_event(status, records, invalid_count),
                UiEvent::Error { code, listing } => {
                    if let Some(listing) = listing {
                        state.apply_event(
                            UiStatus::Error(code),
                            listing.records,
                            listing.invalid_count,
                        );
                    } else {
                        state.status = UiStatus::Error(code);
                        state.create_completed_until = None;
                        state.transient_status_until = None;
                    }
                }
            }
        }
        apply_view(&ui, &event_state.borrow());
    });

    send_from_ui(&ui.as_weak(), &jobs, &view_state, Job::Refresh);
    ui.run()
}

fn connect_state_callback<C, U>(
    ui: &MainWindow,
    state: &Rc<RefCell<ViewState>>,
    connect: C,
    update: U,
) where
    C: FnOnce(&MainWindow, Box<dyn Fn()>),
    U: Fn(&mut ViewState) + 'static,
{
    let weak = ui.as_weak();
    let state = Rc::clone(state);
    connect(
        ui,
        Box::new(move || {
            let Some(ui) = weak.upgrade() else { return };
            update(&mut state.borrow_mut());
            apply_view(&ui, &state.borrow());
        }),
    );
}

fn connect_named_job<B, C>(
    ui: &MainWindow,
    jobs: &Sender<Job>,
    state: &Rc<RefCell<ViewState>>,
    build: B,
    connect: C,
) where
    B: Fn(String) -> Job + 'static,
    C: FnOnce(&MainWindow, Box<dyn Fn(slint::SharedString)>),
{
    let weak = ui.as_weak();
    let jobs = jobs.clone();
    let state = Rc::clone(state);
    connect(
        ui,
        Box::new(move |name| {
            if state.borrow().contains_record(name.as_str()) {
                send_from_ui(&weak, &jobs, &state, build(name.to_string()));
            }
        }),
    );
}

fn send_from_ui(
    weak: &slint::Weak<MainWindow>,
    jobs: &Sender<Job>,
    state: &Rc<RefCell<ViewState>>,
    job: Job,
) {
    let Some(ui) = weak.upgrade() else { return };
    if ui.get_busy() {
        return;
    }
    ui.set_busy(true);
    {
        let mut state = state.borrow_mut();
        state.status = UiStatus::Working;
        state.create_completed_until = None;
        state.transient_status_until = None;
    }
    apply_view(&ui, &state.borrow());
    if jobs.send(job).is_err() {
        ui.set_busy(false);
        state.borrow_mut().status = UiStatus::Error("worker_unavailable");
        apply_view(&ui, &state.borrow());
    }
}

fn apply_view(ui: &MainWindow, state: &ViewState) {
    let text = strings(state.language);
    let selection_count = state.selected_names.len();
    let delete_target = state
        .pending_delete_name
        .as_deref()
        .and_then(|name| state.records.iter().find(|record| record.name == name))
        .map(|record| format_local_timestamp(record.created_at_unix_ms))
        .unwrap_or_default();

    ui.set_window_title_text("VarKeep v2.3".into());
    ui.set_language_button_text(text.language_button.into());
    ui.set_about_text(text.about.into());
    ui.set_close_text(text.close.into());
    ui.set_create_text(
        if state.create_completed_until.is_some() {
            text.completed
        } else {
            text.create
        }
        .into(),
    );
    ui.set_working_text(text.working.into());
    ui.set_view_summary_text(text.view_summary.into());
    ui.set_open_location_text(text.open_location.into());
    ui.set_delete_text(text.delete_backup.into());
    ui.set_cancel_text(text.cancel.into());
    ui.set_backup_time_text(text.backup_time.into());
    ui.set_note_text(text.note.into());
    ui.set_add_note_text(text.add_note.into());
    ui.set_edit_note_title_text(text.edit_note_title.into());
    ui.set_note_placeholder_text(text.note_placeholder.into());
    ui.set_save_text(text.save.into());
    ui.set_actions_text(text.actions.into());
    ui.set_empty_text(text.empty_title.into());
    ui.set_about_title_text(text.about_title.into());
    ui.set_about_body_text(text.about_body.into());
    ui.set_delete_title_text(text.delete_title.into());
    ui.set_delete_irreversible_text(text.delete_irreversible.into());
    ui.set_delete_prompt_text(render_delete_prompt(state.language, &delete_target).into());
    ui.set_delete_confirm_visible(state.pending_delete_name.is_some());
    ui.set_note_editor_visible(state.pending_note_name.is_some());
    ui.set_note_editor_text(state.note_draft.clone().into());
    ui.set_has_backups(!state.records.is_empty());
    ui.set_has_invalid_backups(state.invalid_count > 0);
    ui.set_invalid_backup_text(
        render_invalid_backup_notice(state.language, state.invalid_count).into(),
    );
    ui.set_selection_count(selection_count as i32);
    ui.set_selection_count_text(render_selection_count(state.language, selection_count).into());
    ui.set_compare_action_text(
        if selection_count == 1 {
            text.compare_current
        } else {
            text.compare_selected
        }
        .into(),
    );
    ui.set_comparison_title_text(text.comparison_title.into());
    ui.set_comparison_visible(matches!(&state.status, UiStatus::Comparison(_)));
    ui.set_comparison_detail_text(
        match &state.status {
            UiStatus::Comparison(summary) => render_comparison_detail(state.language, summary),
            _ => String::new(),
        }
        .into(),
    );
    ui.set_status_tone(status_tone(&state.status).as_index());
    ui.set_status_visible(matches!(
        &state.status,
        UiStatus::Working
            | UiStatus::Comparison(_)
            | UiStatus::NoteUpdated(_)
            | UiStatus::BackupDeleted(_)
            | UiStatus::Error(_)
    ));
    ui.set_status_text(render_status(state.language, &state.status).into());

    let rows = state
        .records
        .iter()
        .map(|record| {
            let selected = state.selected_names.contains(&record.name);
            BackupRow {
                id: record.name.clone().into(),
                timestamp: format_local_timestamp(record.created_at_unix_ms).into(),
                note: record.note.clone().into(),
                selected,
                active: state.active_name.as_deref() == Some(record.name.as_str()),
                check_enabled: selected || selection_count < 2,
            }
        })
        .collect::<Vec<_>>();
    ui.set_backup_rows(ModelRc::new(VecModel::from(rows)));
}

fn application_base_directory() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
