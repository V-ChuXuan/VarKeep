use crate::view_state::ViewState;
use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::Instant;
use varkeep::app::{Job, UiEvent, Worker};
use varkeep::summary::SummaryLanguage;
use varkeep::ui_adapter::{
    Language, UiStatus, render_comparison_context, render_comparison_detail, render_delete_prompt,
    render_invalid_backup_notice, render_invalid_backups, render_selection_count, render_status,
    strings,
};
use varkeep::windows::{format_local_timestamp, prefers_chinese_ui};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForWindow, SystemParametersInfoForDpi,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, IsWindowEnabled, SetFocus,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const LIST: i32 = 100;
const CREATE: i32 = 101;
const REFRESH: i32 = 102;
const COMPARE: i32 = 103;
const LANGUAGE: i32 = 104;
const ABOUT: i32 = 105;
const SUMMARY: i32 = 106;
const OPEN: i32 = 107;
const NOTE: i32 = 108;
const DELETE: i32 = 109;
const ISSUES: i32 = 110;
const STATUS: i32 = 111;
const EDIT: i32 = 201;
const ERROR: i32 = 202;
const CONTEXT: i32 = 203;
const RENDER_ROWS: u32 = WM_APP + 1;

struct App {
    state: RefCell<ViewState>,
    worker: Worker,
    window: Cell<HWND>,
    dialog: Cell<HWND>,
    busy: Cell<bool>,
    rendering: Cell<bool>,
    font: Cell<HFONT>,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn localized(language: Language, chinese: &'static str, english: &'static str) -> &'static str {
    match language {
        Language::Chinese => chinese,
        Language::English => english,
    }
}

fn set_text(window: HWND, text: &str) {
    // SAFETY: the UTF-16 buffer lives through this synchronous call.
    unsafe { SetWindowTextW(window, wide(text).as_ptr()) };
}

fn control(window: HWND, id: i32) -> HWND {
    // SAFETY: all IDs name children of this owned window.
    unsafe { GetDlgItem(window, id) }
}

fn enable(window: HWND, enabled: bool) {
    // SAFETY: handles belong to this UI thread; a missing child is harmless.
    unsafe { EnableWindow(window, enabled as i32) };
}

fn send(window: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    // SAFETY: callers supply owned HWNDs and message-specific buffers that outlive the call.
    unsafe { SendMessageW(window, message, wparam, lparam) }
}

fn make_control(parent: HWND, class: &str, text: &str, id: i32, style: u32) -> HWND {
    // SAFETY: child strings are copied by CreateWindowExW; no state borrow crosses this call.
    unsafe {
        CreateWindowExW(
            if class == "EDIT" || class == "SysListView32" {
                WS_EX_CLIENTEDGE
            } else {
                0
            },
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            1,
            1,
            parent,
            id as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        )
    }
}

fn rect(window: HWND) -> RECT {
    let mut area = RECT::default();
    // SAFETY: area is a valid output buffer.
    unsafe { GetClientRect(window, &mut area) };
    area
}

fn dpi(window: HWND) -> u32 {
    // SAFETY: only this thread's live window is queried.
    unsafe { GetDpiForWindow(window) }.max(96)
}

fn scaled(value: i32, dpi: u32) -> i32 {
    (value * dpi as i32 + 48) / 96
}

fn initial_client_size(scale: u32) -> (i32, i32) {
    (scaled(860, scale), scaled(570, scale))
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn initial_layout_keeps_toolbar_and_actions_separate_at_supported_dpis() {
        for scale in [96, 120, 192] {
            let (width, height) = initial_client_size(scale);
            let controls = main_layout(width, height, scale);
            for (_, [x, y, w, h]) in controls {
                assert!(x >= 0 && y >= 0 && w > 0 && h > 0);
                assert!(scaled(x + w, scale) <= width);
                assert!(scaled(y + h, scale) <= height);
            }
            for row in [
                [CREATE, REFRESH, COMPARE, LANGUAGE, ABOUT],
                [SUMMARY, OPEN, NOTE, DELETE, ISSUES],
            ] {
                for pair in row.windows(2) {
                    let left = controls.iter().find(|(id, _)| *id == pair[0]).unwrap().1;
                    let right = controls.iter().find(|(id, _)| *id == pair[1]).unwrap().1;
                    assert!(scaled(left[0] + left[2], scale) < scaled(right[0], scale));
                }
            }
        }
    }
}

fn main_layout(client_width: i32, client_height: i32, scale: u32) -> [(i32, [i32; 4]); 12] {
    let width = client_width * 96 / scale as i32;
    let height = client_height * 96 / scale as i32;
    [
        (CREATE, [14, 14, 112, 30]),
        (REFRESH, [134, 14, 78, 30]),
        (COMPARE, [220, 14, 158, 30]),
        (LANGUAGE, [width - 178, 14, 80, 30]),
        (ABOUT, [width - 90, 14, 76, 30]),
        (STATUS, [14, 54, width - 28, 42]),
        (LIST, [14, 100, width - 28, height - 162]),
        (SUMMARY, [14, height - 48, 106, 30]),
        (OPEN, [128, height - 48, 106, 30]),
        (NOTE, [242, height - 48, 110, 30]),
        (DELETE, [360, height - 48, 80, 30]),
        (ISSUES, [width - 126, height - 48, 112, 30]),
    ]
}

fn place(window: HWND, id: i32, x: i32, y: i32, width: i32, height: i32, scale: u32) {
    // SAFETY: controls remain owned by their parent; coordinates are client pixels.
    unsafe {
        MoveWindow(
            control(window, id),
            scaled(x, scale),
            scaled(y, scale),
            scaled(width.max(1), scale),
            scaled(height.max(1), scale),
            1,
        );
    }
}

fn create_font(window: HWND) -> HFONT {
    // SAFETY: NONCLIENTMETRICSW is a plain output structure with the required size.
    unsafe {
        let mut metrics: NONCLIENTMETRICSW = zeroed();
        metrics.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        if SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            &mut metrics as *mut _ as *mut _,
            0,
            dpi(window),
        ) != 0
        {
            CreateFontIndirectW(&metrics.lfMessageFont)
        } else {
            null_mut()
        }
    }
}

fn apply_font(window: HWND, font: HFONT, ids: &[i32]) {
    let handle = if font.is_null() {
        // SAFETY: this stock object is shared and is never deleted here.
        unsafe { GetStockObject(DEFAULT_GUI_FONT) as HFONT }
    } else {
        font
    };
    for id in ids {
        send(control(window, *id), WM_SETFONT, handle as usize, 1);
    }
}

pub(crate) fn show_startup_error(error: &str) {
    // SAFETY: a system-owned message box copies the strings during the call.
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("VarKeep").as_ptr(),
            MB_OK | MB_ICONERROR,
        )
    };
}

pub(crate) fn run() -> Result<(), String> {
    let base = std::env::current_exe()
        .map_err(|_| "Cannot locate VarKeep executable.")?
        .parent()
        .map(PathBuf::from)
        .ok_or("Cannot locate VarKeep directory.")?;
    let language = if prefers_chinese_ui() {
        Language::Chinese
    } else {
        Language::English
    };
    let app = Box::new(App {
        state: RefCell::new(ViewState {
            language,
            status: UiStatus::Ready,
            records: Vec::new(),
            invalid_count: 0,
            invalid_backups: Vec::new(),
            active_name: None,
            selected_names: Vec::new(),
            pending_note_name: None,
            note_draft: String::new(),
            pending_delete_name: None,
            create_completed_until: None,
            transient_status_until: None,
        }),
        worker: Worker::start(base),
        window: Cell::new(null_mut()),
        dialog: Cell::new(null_mut()),
        busy: Cell::new(false),
        rendering: Cell::new(false),
        font: Cell::new(null_mut()),
    });
    // SAFETY: App has a stable Box address until the message loop exits; callbacks never free it.
    unsafe {
        let instance = GetModuleHandleW(null());
        let class_name = wide("VarKeepMainWindow");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hIcon: LoadIconW(instance, std::ptr::without_provenance(1)),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as usize as HBRUSH,
            lpszClassName: class_name.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return Err("Cannot register VarKeep window.".into());
        }
        let common = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES,
        };
        if InitCommonControlsEx(&common) == 0 {
            return Err("Cannot initialize Windows controls.".into());
        }
        let window = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class_name.as_ptr(),
            wide("VarKeep v2.4").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            860,
            570,
            null_mut(),
            null_mut(),
            instance,
            app.as_ref() as *const App as *const _,
        );
        if window.is_null() {
            return Err("Cannot create VarKeep window.".into());
        }
        app.window.set(window);
        let scale = dpi(window);
        let (width, height) = initial_client_size(scale);
        let mut initial = RECT {
            right: width,
            bottom: height,
            ..Default::default()
        };
        if AdjustWindowRectExForDpi(
            &mut initial,
            WS_OVERLAPPEDWINDOW,
            0,
            WS_EX_CONTROLPARENT,
            scale,
        ) == 0
            || SetWindowPos(
                window,
                null_mut(),
                0,
                0,
                initial.right - initial.left,
                initial.bottom - initial.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            ) == 0
        {
            DestroyWindow(window);
            return Err("Cannot size VarKeep window.".into());
        }
        ShowWindow(window, SW_SHOW);
        app.submit(Job::Refresh);
        let mut message: MSG = zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == -1 {
                DestroyWindow(window);
                return Err("Windows message loop failed.".into());
            }
            if result == 0 {
                break;
            }
            if IsDialogMessageW(window, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    let font = app.font.replace(null_mut());
    if !font.is_null() {
        // SAFETY: every child was destroyed before the message loop returned.
        unsafe { DeleteObject(font as HGDIOBJ) };
    }
    Ok(())
}

impl App {
    fn init_controls(&self, window: HWND) -> bool {
        self.window.set(window);
        let mut ok = true;
        for id in [
            CREATE, REFRESH, COMPARE, LANGUAGE, ABOUT, SUMMARY, OPEN, NOTE, DELETE, ISSUES,
        ] {
            ok &= !make_control(window, "BUTTON", "", id, WS_TABSTOP | BS_PUSHBUTTON as u32)
                .is_null();
        }
        ok &= !make_control(window, "STATIC", "", STATUS, 0).is_null();
        let list = make_control(
            window,
            "SysListView32",
            "",
            LIST,
            WS_TABSTOP | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS,
        );
        ok &= !list.is_null();
        send(
            list,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            0,
            (LVS_EX_CHECKBOXES | LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize,
        );
        for (index, width) in [210, 450].into_iter().enumerate() {
            let mut title = wide(if index == 0 { "备份时间" } else { "备注" });
            let column = LVCOLUMNW {
                mask: LVCF_TEXT | LVCF_WIDTH,
                cx: width,
                pszText: title.as_mut_ptr(),
                ..Default::default()
            };
            send(list, LVM_INSERTCOLUMNW, index, &column as *const _ as isize);
        }
        self.font.set(create_font(window));
        apply_font(
            window,
            self.font.get(),
            &[
                LIST, CREATE, REFRESH, COMPARE, LANGUAGE, ABOUT, SUMMARY, OPEN, NOTE, DELETE,
                ISSUES, STATUS,
            ],
        );
        self.layout();
        self.render(true);
        // SAFETY: this window owns the timer and removes it on destruction.
        unsafe { SetTimer(window, 1, 80, None) != 0 && ok }
    }

    fn layout(&self) {
        let window = self.window.get();
        let scale = dpi(window);
        let area = rect(window);
        let width = area.right * 96 / scale as i32;
        for (id, [x, y, width, height]) in main_layout(area.right, area.bottom, scale) {
            place(window, id, x, y, width, height, scale);
        }
        send(
            control(window, LIST),
            LVM_SETCOLUMNWIDTH,
            0,
            scaled(210, scale) as isize,
        );
        send(
            control(window, LIST),
            LVM_SETCOLUMNWIDTH,
            1,
            scaled((width - 260).max(100), scale) as isize,
        );
    }

    fn render(&self, rows: bool) {
        // Copy presentation data before any Win32 call, which can synchronously re-enter callbacks.
        let (language, status, records, active, selected, invalid_count, completed) = {
            let state = self.state.borrow();
            (
                state.language,
                state.status.clone(),
                state.records.clone(),
                state.active_name.clone(),
                state.selected_names.clone(),
                state.invalid_count,
                state.create_completed_until.is_some(),
            )
        };
        let text = strings(language);
        let window = self.window.get();
        for (id, caption) in [
            (
                CREATE,
                if completed {
                    text.completed
                } else {
                    text.create
                },
            ),
            (REFRESH, text.refresh),
            (
                COMPARE,
                if selected.len() == 1 {
                    text.compare_current
                } else {
                    text.compare_selected
                },
            ),
            (LANGUAGE, text.language_button),
            (ABOUT, text.about),
            (SUMMARY, text.view_summary),
            (OPEN, text.open_location),
            (NOTE, text.edit_note),
            (DELETE, text.delete_backup),
            (ISSUES, text.view_issues),
        ] {
            set_text(control(window, id), caption);
        }
        let mut status_text = if records.is_empty()
            && matches!(status, UiStatus::Ready | UiStatus::BackupsFound(_))
        {
            text.empty_title.to_owned()
        } else {
            render_status(language, &status)
        };
        status_text.push_str("   ·   ");
        status_text.push_str(&render_selection_count(language, selected.len()));
        if invalid_count > 0 {
            status_text.push_str("\r\n");
            status_text.push_str(&render_invalid_backup_notice(language, invalid_count));
        }
        set_text(control(window, STATUS), &status_text);
        let busy = self.busy.get();
        for id in [CREATE, REFRESH, LANGUAGE, LIST] {
            enable(control(window, id), !busy);
        }
        enable(control(window, COMPARE), !busy && !selected.is_empty());
        for id in [SUMMARY, OPEN, NOTE, DELETE] {
            enable(control(window, id), !busy && active.is_some());
        }
        enable(control(window, ISSUES), !busy && invalid_count > 0);
        if !rows {
            return;
        }
        self.rendering.set(true);
        let list = control(window, LIST);
        send(list, WM_SETREDRAW, 0, 0);
        send(list, LVM_DELETEALLITEMS, 0, 0);
        for (index, record) in records.iter().enumerate() {
            let mut timestamp = wide(&format_local_timestamp(record.created_at_unix_ms));
            let state = if selected.contains(&record.name) {
                2 << 12
            } else {
                1 << 12
            } | if active.as_deref() == Some(record.name.as_str()) {
                LVIS_SELECTED | LVIS_FOCUSED
            } else {
                0
            };
            let item = LVITEMW {
                mask: LVIF_TEXT | LVIF_STATE,
                iItem: index as i32,
                pszText: timestamp.as_mut_ptr(),
                state,
                stateMask: LVIS_STATEIMAGEMASK | LVIS_SELECTED | LVIS_FOCUSED,
                ..Default::default()
            };
            send(list, LVM_INSERTITEMW, 0, &item as *const _ as isize);
            // Checkbox lists initialize inserted items as unchecked; restore the model afterwards.
            send(list, LVM_SETITEMSTATE, index, &item as *const _ as isize);
            let mut note = wide(&record.note);
            let sub = LVITEMW {
                iSubItem: 1,
                pszText: note.as_mut_ptr(),
                ..Default::default()
            };
            send(list, LVM_SETITEMTEXTW, index, &sub as *const _ as isize);
        }
        for (index, label) in [text.backup_time, text.note].into_iter().enumerate() {
            let mut label = wide(label);
            let column = LVCOLUMNW {
                mask: LVCF_TEXT,
                pszText: label.as_mut_ptr(),
                ..Default::default()
            };
            send(list, LVM_SETCOLUMNW, index, &column as *const _ as isize);
        }
        send(list, WM_SETREDRAW, 1, 0);
        // SAFETY: repaint follows a complete item update on this owned list.
        unsafe { InvalidateRect(list, null(), 1) };
        self.rendering.set(false);
    }

    fn submit(&self, job: Job) {
        if self.busy.replace(true) {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.status = UiStatus::Working;
            state.create_completed_until = None;
            state.transient_status_until = None;
        }
        if self.worker.jobs.send(job).is_err() {
            self.busy.set(false);
            self.state.borrow_mut().status = UiStatus::Error("worker_unavailable");
        }
        self.render(false);
    }

    fn poll(&self) {
        let event = self.worker.events.try_recv().ok();
        let has_event = event.is_some();
        let mut show_comparison = false;
        let mut close_note = false;
        let mut note_error = None;
        let changed = {
            let mut state = self.state.borrow_mut();
            let now = Instant::now();
            let expired = state.expire_create_completed(now) | state.expire_transient_status(now);
            match event {
                Some(UiEvent::Ready {
                    status,
                    records,
                    invalid_count,
                    invalid_backups,
                }) => {
                    show_comparison = matches!(status, UiStatus::Comparison { .. });
                    close_note = matches!(status, UiStatus::NoteUpdated(_));
                    state.invalid_backups = invalid_backups;
                    state.apply_event(status, records, invalid_count);
                    true
                }
                Some(UiEvent::Error { code, listing }) => {
                    let editing_note = state.pending_note_name.is_some();
                    if let Some(listing) = listing {
                        state.invalid_backups = listing.invalid_backups;
                        state.apply_event(
                            UiStatus::Error(code),
                            listing.records,
                            listing.invalid_count,
                        );
                    } else {
                        state.status = UiStatus::Error(code);
                    }
                    if editing_note {
                        note_error = Some(render_status(state.language, &state.status));
                    }
                    true
                }
                None => expired,
            }
        };
        if !changed {
            return;
        }
        if has_event {
            self.busy.set(false);
        }
        self.render(has_event);
        let dialog = self.dialog.get();
        if !dialog.is_null() {
            if close_note {
                // SAFETY: this is the currently active owned modal dialog.
                unsafe { EndDialog(dialog, IDOK as isize) };
            } else if let Some(error) = note_error {
                set_text(control(dialog, ERROR), &error);
                enable(control(dialog, EDIT), true);
                enable(control(dialog, IDOK), true);
                enable(control(dialog, IDCANCEL), true);
                // SAFETY: return keyboard editing to the retained draft after a failed save.
                unsafe { SetFocus(control(dialog, EDIT)) };
            }
        } else if show_comparison {
            self.comparison_dialog();
        }
    }

    fn command(&self, id: i32) {
        if self.busy.get() || !self.dialog.get().is_null() {
            return;
        }
        match id {
            CREATE => {
                let language = self.state.borrow().language;
                self.submit(Job::CreateBackup {
                    language: if language == Language::Chinese {
                        SummaryLanguage::Chinese
                    } else {
                        SummaryLanguage::English
                    },
                });
            }
            REFRESH => self.submit(Job::Refresh),
            COMPARE => {
                let job = self.state.borrow().comparison_job();
                if let Some(job) = job {
                    self.submit(job);
                }
            }
            LANGUAGE => {
                let language = self.state.borrow().language.toggle();
                self.state.borrow_mut().language = language;
                self.render(true);
            }
            ABOUT => {
                let language = self.state.borrow().language;
                self.read_dialog(strings(language).about_title, "", "VarKeep\r\nMIT License");
            }
            ISSUES => {
                let (language, issues) = {
                    let state = self.state.borrow();
                    (
                        state.language,
                        render_invalid_backups(state.language, &state.invalid_backups),
                    )
                };
                self.read_dialog(strings(language).view_issues, "", &issues);
            }
            SUMMARY | OPEN | NOTE | DELETE => {
                let name = self.state.borrow().active_name.clone();
                let Some(name) = name else {
                    return;
                };
                match id {
                    SUMMARY => self.submit(Job::ViewSummary { name }),
                    OPEN => self.submit(Job::OpenBackup { name }),
                    NOTE => {
                        self.state.borrow_mut().request_note_edit(&name);
                        let language = self.state.borrow().language;
                        self.open_dialog(
                            DialogKind::Note,
                            strings(language).edit_note_title,
                            "",
                            "",
                        );
                        if !self.busy.get() {
                            self.state.borrow_mut().cancel_note_edit();
                        }
                    }
                    DELETE => {
                        let (language, target) = {
                            let state = self.state.borrow();
                            (
                                state.language,
                                state
                                    .records
                                    .iter()
                                    .find(|record| record.name == name)
                                    .map(|record| {
                                        format!(
                                            "{} ({})",
                                            format_local_timestamp(record.created_at_unix_ms),
                                            record.name
                                        )
                                    })
                                    .unwrap_or_default(),
                            )
                        };
                        self.state.borrow_mut().request_delete(&name);
                        let text = strings(language);
                        let prompt = format!(
                            "{}\r\n\r\n{}",
                            render_delete_prompt(language, &target),
                            text.delete_irreversible
                        );
                        // SAFETY: MessageBox supplies native modality and safe default No.
                        let answer = unsafe {
                            MessageBoxW(
                                self.window.get(),
                                wide(&prompt).as_ptr(),
                                wide(text.delete_title).as_ptr(),
                                MB_YESNOCANCEL | MB_DEFBUTTON2 | MB_ICONWARNING,
                            )
                        };
                        self.state.borrow_mut().pending_delete_name = None;
                        if answer == IDYES {
                            self.submit(Job::DeleteBackup { name });
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn comparison_dialog(&self) {
        let (language, context, body) = {
            let state = self.state.borrow();
            let UiStatus::Comparison { summary, .. } = &state.status else {
                return;
            };
            (
                state.language,
                render_comparison_context(state.language, &state.status),
                format!(
                    "{}\r\n\r\n{}",
                    render_status(state.language, &state.status),
                    render_comparison_detail(state.language, summary)
                ),
            )
        };
        self.read_dialog(strings(language).comparison_title, &context, &body);
        self.state.borrow_mut().dismiss_comparison();
        self.render(false);
    }

    fn read_dialog(&self, title: &str, context: &str, body: &str) {
        self.open_dialog(DialogKind::Read, title, context, body);
    }

    fn open_dialog(&self, kind: DialogKind, title: &str, context: &str, body: &str) {
        let focus = unsafe { GetFocus() };
        let data = DialogData {
            app: self,
            kind,
            title,
            context,
            body,
            font: Cell::new(null_mut()),
        };
        let template = EmptyDialog::new();
        // SAFETY: aligned template and stack data outlive the complete modal call.
        let result = unsafe {
            DialogBoxIndirectParamW(
                GetModuleHandleW(null()),
                &template.header,
                self.window.get(),
                Some(dialog_proc),
                &data as *const _ as isize,
            )
        };
        self.dialog.set(null_mut());
        let font = data.font.replace(null_mut());
        if !font.is_null() {
            unsafe { DeleteObject(font as HGDIOBJ) };
        }
        if result == -1 {
            let language = self.state.borrow().language;
            show_startup_error(localized(
                language,
                "无法打开对话框。",
                "Cannot open dialog.",
            ));
        }
        // SAFETY: restore only a still-live enabled control belonging to the main window.
        unsafe {
            if IsWindow(focus) != 0 && IsWindowEnabled(focus) != 0 {
                SetFocus(focus);
            }
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    // SAFETY: userdata points to the stable App owned by run, never a mutable alias.
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *const App;
        if pointer.is_null() {
            return DefWindowProcW(window, message, wparam, lparam);
        }
        let app = &*pointer;
        match message {
            WM_CREATE => {
                if app.init_controls(window) {
                    0
                } else {
                    -1
                }
            }
            WM_SIZE => {
                app.layout();
                0
            }
            WM_COMMAND => {
                app.command((wparam & 0xffff) as i32);
                0
            }
            WM_TIMER => {
                app.poll();
                0
            }
            RENDER_ROWS => {
                app.render(true);
                0
            }
            WM_NOTIFY => {
                let header = &*(lparam as *const NMHDR);
                if header.hwndFrom == control(window, LIST)
                    && header.code == LVN_ITEMCHANGED
                    && !app.rendering.get()
                {
                    let notification = &*(lparam as *const NMLISTVIEW);
                    if notification.iItem >= 0 && !app.busy.get() {
                        let name = app
                            .state
                            .borrow()
                            .records
                            .get(notification.iItem as usize)
                            .map(|record| record.name.clone());
                        if let Some(name) = name {
                            if (notification.uOldState ^ notification.uNewState)
                                & LVIS_STATEIMAGEMASK
                                != 0
                            {
                                app.state.borrow_mut().toggle_selection(&name);
                                // Let ListView finish its checkbox update before rebuilding rows.
                                PostMessageW(window, RENDER_ROWS, 0, 0);
                            } else if notification.uNewState & LVIS_SELECTED != 0 {
                                app.state.borrow_mut().activate(&name);
                                app.render(false);
                            }
                        }
                    }
                }
                0
            }
            WM_GETMINMAXINFO => {
                let limits = &mut *(lparam as *mut MINMAXINFO);
                limits.ptMinTrackSize.x = scaled(720, dpi(window));
                limits.ptMinTrackSize.y = scaled(400, dpi(window));
                0
            }
            WM_DPICHANGED => {
                let suggested = &*(lparam as *const RECT);
                SetWindowPos(
                    window,
                    null_mut(),
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let old = app.font.replace(create_font(window));
                apply_font(
                    window,
                    app.font.get(),
                    &[
                        LIST, CREATE, REFRESH, COMPARE, LANGUAGE, ABOUT, SUMMARY, OPEN, NOTE,
                        DELETE, ISSUES, STATUS,
                    ],
                );
                if !old.is_null() {
                    DeleteObject(old as HGDIOBJ);
                }
                app.layout();
                0
            }
            WM_CLOSE => {
                if !app.dialog.get().is_null() {
                    return 0;
                }
                if !app.busy.get() {
                    DestroyWindow(window);
                } else {
                    let language = app.state.borrow().language;
                    MessageBoxW(
                        window,
                        wide(localized(
                            language,
                            "请等待当前操作完成后关闭。",
                            "Wait for the current operation to finish.",
                        ))
                        .as_ptr(),
                        wide("VarKeep").as_ptr(),
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
                0
            }
            WM_DESTROY => {
                KillTimer(window, 1);
                PostQuitMessage(0);
                0
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                DefWindowProcW(window, message, wparam, lparam)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DialogKind {
    Note,
    Read,
}

struct DialogData<'a> {
    app: &'a App,
    kind: DialogKind,
    title: &'a str,
    context: &'a str,
    body: &'a str,
    font: Cell<HFONT>,
}

#[repr(C, align(4))]
struct EmptyDialog {
    header: DLGTEMPLATE,
    menu: u16,
    class: u16,
    title: u16,
}

impl EmptyDialog {
    fn new() -> Self {
        Self {
            header: DLGTEMPLATE {
                style: WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32,
                dwExtendedStyle: WS_EX_CONTROLPARENT,
                cdit: 0,
                x: 0,
                y: 0,
                cx: 390,
                cy: 250,
            },
            menu: 0,
            class: 0,
            title: 0,
        }
    }
}

fn dialog_layout(window: HWND, kind: DialogKind) {
    let scale = dpi(window);
    let area = rect(window);
    let width = area.right * 96 / scale as i32;
    let height = area.bottom * 96 / scale as i32;
    if kind == DialogKind::Note {
        place(window, CONTEXT, 14, 12, width - 28, 24, scale);
        place(window, EDIT, 14, 42, width - 28, 30, scale);
        place(window, ERROR, 14, 82, width - 28, height - 136, scale);
        place(window, IDOK, width - 184, height - 44, 78, 30, scale);
        place(window, IDCANCEL, width - 98, height - 44, 84, 30, scale);
    } else {
        place(window, CONTEXT, 14, 12, width - 28, 52, scale);
        place(window, EDIT, 14, 72, width - 28, height - 128, scale);
        place(window, IDCANCEL, width - 98, height - 44, 84, 30, scale);
    }
}

unsafe extern "system" fn dialog_proc(
    window: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    // SAFETY: DialogData lives on open_dialog's stack until EndDialog returns to it.
    unsafe {
        if message == WM_INITDIALOG {
            SetWindowLongPtrW(window, GWLP_USERDATA, lparam);
        }
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *const DialogData<'_>;
        if pointer.is_null() {
            return 0;
        }
        let data = &*pointer;
        match message {
            WM_INITDIALOG => {
                data.app.dialog.set(window);
                let language = data.app.state.borrow().language;
                let text = strings(language);
                set_text(window, data.title);
                let note = data.kind == DialogKind::Note;
                make_control(
                    window,
                    "STATIC",
                    if note {
                        text.note_placeholder
                    } else {
                        data.context
                    },
                    CONTEXT,
                    0,
                );
                let content = if note {
                    data.app.state.borrow().note_draft.clone()
                } else {
                    data.body.replace('\n', "\r\n").replace("\r\r\n", "\r\n")
                };
                let edit = make_control(
                    window,
                    "EDIT",
                    &content,
                    EDIT,
                    WS_TABSTOP
                        | if note {
                            ES_AUTOHSCROLL as u32
                        } else {
                            ES_MULTILINE as u32
                                | ES_READONLY as u32
                                | ES_AUTOVSCROLL as u32
                                | WS_VSCROLL
                        },
                );
                if note {
                    send(edit, EM_SETLIMITTEXT, 400, 0);
                    make_control(window, "STATIC", "", ERROR, 0);
                    make_control(
                        window,
                        "BUTTON",
                        text.save,
                        IDOK,
                        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                    );
                }
                make_control(
                    window,
                    "BUTTON",
                    if note { text.cancel } else { text.close },
                    IDCANCEL,
                    WS_TABSTOP
                        | if note {
                            BS_PUSHBUTTON as u32
                        } else {
                            BS_DEFPUSHBUTTON as u32
                        },
                );
                data.font.set(create_font(window));
                apply_font(
                    window,
                    data.font.get(),
                    &[EDIT, ERROR, CONTEXT, IDOK, IDCANCEL],
                );
                let owner_rect = {
                    let mut r = RECT::default();
                    GetWindowRect(data.app.window.get(), &mut r);
                    r
                };
                let width = scaled(if note { 510 } else { 690 }, dpi(window));
                let height = scaled(if note { 250 } else { 500 }, dpi(window));
                SetWindowPos(
                    window,
                    null_mut(),
                    owner_rect.left + ((owner_rect.right - owner_rect.left - width) / 2).max(0),
                    owner_rect.top + ((owner_rect.bottom - owner_rect.top - height) / 2).max(0),
                    width,
                    height,
                    SWP_NOZORDER,
                );
                dialog_layout(window, data.kind);
                SetFocus(edit);
                if note {
                    send(edit, EM_SETSEL, 0, -1);
                }
                0
            }
            WM_SIZE => {
                dialog_layout(window, data.kind);
                1
            }
            WM_COMMAND => match (wparam & 0xffff) as i32 {
                IDOK if data.kind == DialogKind::Note && !data.app.busy.get() => {
                    let edit = control(window, EDIT);
                    let length = GetWindowTextLengthW(edit).clamp(0, 400) as usize;
                    let mut buffer = vec![0u16; length + 1];
                    let copied = GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32)
                        .max(0) as usize;
                    let note = String::from_utf16_lossy(&buffer[..copied]);
                    let name = {
                        let mut state = data.app.state.borrow_mut();
                        state.note_draft = note.clone();
                        state.pending_note_name.clone()
                    };
                    if let Some(name) = name {
                        set_text(control(window, ERROR), "");
                        for id in [EDIT, IDOK, IDCANCEL] {
                            enable(control(window, id), false);
                        }
                        data.app.submit(Job::UpdateNote { name, note });
                        if !data.app.busy.get() {
                            let error = {
                                let state = data.app.state.borrow();
                                render_status(state.language, &state.status)
                            };
                            set_text(control(window, ERROR), &error);
                            for id in [EDIT, IDOK, IDCANCEL] {
                                enable(control(window, id), true);
                            }
                        }
                    }
                    1
                }
                IDCANCEL => {
                    if !data.app.busy.get() {
                        EndDialog(window, IDCANCEL as isize);
                    }
                    1
                }
                _ => 0,
            },
            WM_CLOSE => {
                if !data.app.busy.get() {
                    EndDialog(window, IDCANCEL as isize);
                }
                1
            }
            WM_DPICHANGED => {
                let suggested = &*(lparam as *const RECT);
                SetWindowPos(
                    window,
                    null_mut(),
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let old = data.font.replace(create_font(window));
                apply_font(
                    window,
                    data.font.get(),
                    &[EDIT, ERROR, CONTEXT, IDOK, IDCANCEL],
                );
                if !old.is_null() {
                    DeleteObject(old as HGDIOBJ);
                }
                dialog_layout(window, data.kind);
                1
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                0
            }
            _ => 0,
        }
    }
}
