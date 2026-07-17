use crate::domain::{ChangeKind, ComparisonSummary, PathChangeKind, Scope};

const MAX_COMPARISON_DETAIL_LINES: usize = 500;
const MAX_COMPARISON_NAME_CHARS: usize = 160;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    Chinese,
    English,
}

impl Language {
    pub const fn toggle(self) -> Self {
        match self {
            Self::Chinese => Self::English,
            Self::English => Self::Chinese,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiStatus {
    Ready,
    Working,
    BackupsFound(usize),
    BackupCreated(String),
    Comparison(ComparisonSummary),
    SummaryOpened,
    NoteUpdated(String),
    BackupDeleted(String),
    DirectoryOpened,
    Error(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiTone {
    Info,
    Working,
    Success,
    Error,
}

impl UiTone {
    pub const fn as_index(self) -> i32 {
        match self {
            Self::Info => 0,
            Self::Working => 1,
            Self::Success => 2,
            Self::Error => 3,
        }
    }
}

pub struct UiStrings {
    pub language_button: &'static str,
    pub about: &'static str,
    pub close: &'static str,
    pub create: &'static str,
    pub completed: &'static str,
    pub working: &'static str,
    pub compare_current: &'static str,
    pub compare_selected: &'static str,
    pub comparison_title: &'static str,
    pub clear_selection: &'static str,
    pub view_summary: &'static str,
    pub open_location: &'static str,
    pub delete_backup: &'static str,
    pub delete_title: &'static str,
    pub delete_irreversible: &'static str,
    pub cancel: &'static str,
    pub backup_time: &'static str,
    pub note: &'static str,
    pub add_note: &'static str,
    pub edit_note_title: &'static str,
    pub note_placeholder: &'static str,
    pub save: &'static str,
    pub actions: &'static str,
    pub empty_title: &'static str,
    pub about_title: &'static str,
    pub about_body: &'static str,
}

pub const fn strings(language: Language) -> UiStrings {
    match language {
        Language::Chinese => UiStrings {
            language_button: "English",
            about: "关于",
            close: "关闭",
            create: "创建备份",
            completed: "已完成",
            working: "处理中…",
            compare_current: "与当前环境对比",
            compare_selected: "对比所选",
            comparison_title: "对比结果",
            clear_selection: "取消",
            view_summary: "查看摘要",
            open_location: "打开位置",
            delete_backup: "删除",
            delete_title: "删除备份",
            delete_irreversible: "此操作无法撤销。",
            cancel: "取消",
            backup_time: "备份时间",
            note: "备注",
            add_note: "添加备注",
            edit_note_title: "编辑备注",
            note_placeholder: "输入备注（最多 100 字）",
            save: "保存",
            actions: "操作",
            empty_title: "尚无备份",
            about_title: "关于 VarKeep v2.3",
            about_body: "MIT License",
        },
        Language::English => UiStrings {
            language_button: "中文",
            about: "About",
            close: "Close",
            create: "Create backup",
            completed: "Completed",
            working: "Working…",
            compare_current: "Compare with current",
            compare_selected: "Compare selected",
            comparison_title: "Comparison",
            clear_selection: "Cancel",
            view_summary: "View summary",
            open_location: "Open location",
            delete_backup: "Delete",
            delete_title: "Delete backup",
            delete_irreversible: "This action cannot be undone.",
            cancel: "Cancel",
            backup_time: "Backup time",
            note: "Note",
            add_note: "Add note",
            edit_note_title: "Edit note",
            note_placeholder: "Enter a note (100 characters max)",
            save: "Save",
            actions: "Actions",
            empty_title: "No backups yet",
            about_title: "About VarKeep v2.3",
            about_body: "MIT License",
        },
    }
}

pub const fn status_tone(status: &UiStatus) -> UiTone {
    match status {
        UiStatus::Working => UiTone::Working,
        UiStatus::BackupCreated(_)
        | UiStatus::SummaryOpened
        | UiStatus::NoteUpdated(_)
        | UiStatus::BackupDeleted(_)
        | UiStatus::DirectoryOpened => UiTone::Success,
        UiStatus::Error(_) => UiTone::Error,
        UiStatus::Ready | UiStatus::BackupsFound(_) | UiStatus::Comparison(_) => UiTone::Info,
    }
}

pub fn render_invalid_backup_notice(language: Language, invalid_count: usize) -> String {
    match language {
        Language::Chinese => {
            format!("已忽略 {invalid_count} 个无效备份；未显示路径或变量值。")
        }
        Language::English => {
            format!("{invalid_count} invalid backup(s) ignored; no paths or values shown.")
        }
    }
}

pub fn render_status(language: Language, status: &UiStatus) -> String {
    match (language, status) {
        (Language::Chinese, UiStatus::Ready) => "准备就绪".into(),
        (Language::English, UiStatus::Ready) => "Ready".into(),
        (Language::Chinese, UiStatus::Working) => "正在处理…".into(),
        (Language::English, UiStatus::Working) => "Working…".into(),
        (Language::Chinese, UiStatus::BackupsFound(count)) => {
            format!("找到 {count} 个有效备份")
        }
        (Language::English, UiStatus::BackupsFound(count)) => {
            format!("{count} backup(s) found")
        }
        (Language::Chinese, UiStatus::BackupCreated(name)) => format!("备份已创建：{name}"),
        (Language::English, UiStatus::BackupCreated(name)) => {
            format!("Backup created: {name}")
        }
        (Language::Chinese, UiStatus::Comparison(summary)) => format!(
            "新增：{}　移除：{}　变更：{}　未变：{}",
            summary.added, summary.removed, summary.changed, summary.unchanged
        ),
        (Language::English, UiStatus::Comparison(summary)) => format!(
            "Added: {}   Removed: {}   Changed: {}   Unchanged: {}",
            summary.added, summary.removed, summary.changed, summary.unchanged
        ),
        (Language::Chinese, UiStatus::SummaryOpened) => "已打开备份摘要".into(),
        (Language::English, UiStatus::SummaryOpened) => "Backup summary opened".into(),
        (Language::Chinese, UiStatus::NoteUpdated(_)) => "备注已保存".into(),
        (Language::English, UiStatus::NoteUpdated(_)) => "Note saved".into(),
        (Language::Chinese, UiStatus::BackupDeleted(_)) => "备份已删除".into(),
        (Language::English, UiStatus::BackupDeleted(_)) => "Backup deleted".into(),
        (Language::Chinese, UiStatus::DirectoryOpened) => "已打开备份目录".into(),
        (Language::English, UiStatus::DirectoryOpened) => "Backup directory opened".into(),
        (language, UiStatus::Error(code)) => friendly_error(language, code),
    }
}

pub fn render_selection_count(language: Language, count: usize) -> String {
    match language {
        Language::Chinese => format!("已选 {count} 项"),
        Language::English => format!("{count} selected"),
    }
}

pub fn render_comparison_detail(language: Language, summary: &ComparisonSummary) -> String {
    let variable_changes = summary
        .changes
        .iter()
        .filter(|change| change.kind != ChangeKind::Unchanged)
        .collect::<Vec<_>>();
    let total_lines = variable_changes.len() + summary.path_changes.len();
    let hidden_count = total_lines.saturating_sub(MAX_COMPARISON_DETAIL_LINES);
    let mut lines = variable_changes
        .into_iter()
        .map(|change| {
            let scope = match (language, change.scope) {
                (Language::Chinese, Scope::User) => "用户",
                (Language::Chinese, Scope::System) => "系统",
                (Language::English, Scope::User) => "User",
                (Language::English, Scope::System) => "System",
            };
            let kind = match (language, change.kind) {
                (Language::Chinese, ChangeKind::Added) => "新增",
                (Language::Chinese, ChangeKind::Removed) => "删除",
                (Language::Chinese, ChangeKind::Changed) => "变更",
                (Language::English, ChangeKind::Added) => "Added",
                (Language::English, ChangeKind::Removed) => "Removed",
                (Language::English, ChangeKind::Changed) => "Changed",
                (_, ChangeKind::Unchanged) => unreachable!(),
            };
            let name = bounded_safe_text(&change.name);
            format!("{scope} · {kind} · {name}")
        })
        .chain(summary.path_changes.iter().map(|change| {
            let scope = match (language, change.scope) {
                (Language::Chinese, Scope::User) => "用户",
                (Language::Chinese, Scope::System) => "系统",
                (Language::English, Scope::User) => "User",
                (Language::English, Scope::System) => "System",
            };
            let kind = match (language, change.kind) {
                (Language::Chinese, PathChangeKind::Added) => "新增",
                (Language::Chinese, PathChangeKind::Removed) => "删除",
                (Language::English, PathChangeKind::Added) => "added",
                (Language::English, PathChangeKind::Removed) => "removed",
            };
            let entry = bounded_safe_text(&change.entry);
            format!("{scope} · PATH {kind} · {entry}")
        }))
        .take(MAX_COMPARISON_DETAIL_LINES)
        .collect::<Vec<_>>();
    if hidden_count > 0 {
        lines.push(match language {
            Language::Chinese => format!("… 另有 {hidden_count} 项未显示"),
            Language::English => format!("… {hidden_count} more not shown"),
        });
    }
    if lines.is_empty() {
        lines.push(match language {
            Language::Chinese => "没有差异".into(),
            Language::English => "No differences".into(),
        });
    }
    lines.join("\n")
}

fn bounded_safe_text(value: &str) -> String {
    let safe = value
        .chars()
        .map(|character| {
            if character.is_control() {
                '�'
            } else {
                character
            }
        })
        .collect::<String>();
    let mut characters = safe.chars();
    let mut bounded = characters
        .by_ref()
        .take(MAX_COMPARISON_NAME_CHARS)
        .collect::<String>();
    if characters.next().is_some() {
        bounded.push('…');
    }
    bounded
}

pub fn render_delete_prompt(language: Language, target: &str) -> String {
    match language {
        Language::Chinese => format!("永久删除 {target} 的备份？"),
        Language::English => format!("Permanently delete the {target} backup?"),
    }
}

fn friendly_error(language: Language, code: &'static str) -> String {
    match (language, code) {
        (Language::Chinese, "no_backup") => "没有可用的完整 v2 备份。".into(),
        (Language::English, "no_backup") => "No complete v2 backup is available.".into(),
        (Language::Chinese, "backup_not_found") => "所选备份不存在或已失效。".into(),
        (Language::English, "backup_not_found") => {
            "The selected backup is missing or invalid.".into()
        }
        (Language::Chinese, "same_backup") => "请选择两个不同的备份。".into(),
        (Language::English, "same_backup") => "Select two different backups.".into(),
        (Language::Chinese, "backup_delete_failed") => "无法完整删除所选备份。".into(),
        (Language::English, "backup_delete_failed") => {
            "The selected backup could not be fully deleted.".into()
        }
        (Language::Chinese, "invalid_note") => "备注只能是最多 100 字的单行文本。".into(),
        (Language::English, "invalid_note") => {
            "The note must be a single line of at most 100 characters.".into()
        }
        (Language::Chinese, "note_write_failed") => "无法保存备注。".into(),
        (Language::English, "note_write_failed") => "The note could not be saved.".into(),
        (Language::Chinese, "registry_value_kind_unsupported") => {
            "发现不受支持的注册表值类型；未创建不完整备份。".into()
        }
        (Language::English, "registry_value_kind_unsupported") => {
            "An unsupported registry value type was found; no incomplete backup was created.".into()
        }
        (Language::Chinese, "windows_required") => "本程序仅支持 Windows。".into(),
        (Language::English, "windows_required") => "This application requires Windows.".into(),
        (Language::Chinese, _) => "操作未完成。请重试；未显示任何环境变量值。".into(),
        (Language::English, _) => {
            "The operation could not be completed. Try again. No environment values were shown."
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ChangeRecord, PathChangeRecord};

    #[test]
    fn language_toggle_and_static_catalog_are_symmetric() {
        assert_eq!(Language::Chinese.toggle(), Language::English);
        assert_eq!(Language::English.toggle(), Language::Chinese);
        assert_eq!(strings(Language::Chinese).create, "创建备份");
        assert_eq!(strings(Language::English).create, "Create backup");
        assert_eq!(strings(Language::Chinese).about_title, "关于 VarKeep v2.3");
        assert_eq!(strings(Language::English).about_title, "About VarKeep v2.3");
        assert_eq!(strings(Language::Chinese).completed, "已完成");
        assert_eq!(strings(Language::English).close, "Close");
        assert_eq!(strings(Language::Chinese).compare_current, "与当前环境对比");
        assert_eq!(
            strings(Language::English).compare_selected,
            "Compare selected"
        );
        assert_eq!(strings(Language::Chinese).view_summary, "查看摘要");
        assert_eq!(strings(Language::Chinese).note, "备注");
        assert_eq!(strings(Language::English).add_note, "Add note");
        assert_eq!(strings(Language::English).open_location, "Open location");
        assert_eq!(strings(Language::Chinese).delete_backup, "删除");
        assert_eq!(render_selection_count(Language::Chinese, 2), "已选 2 项");
        assert_eq!(render_selection_count(Language::English, 1), "1 selected");
    }

    #[test]
    fn error_projection_is_localized_and_redacted() {
        let chinese = render_status(Language::Chinese, &UiStatus::Error("file_read_failed"));
        let english = render_status(Language::English, &UiStatus::Error("file_read_failed"));
        assert!(chinese.contains("未显示任何环境变量值"));
        assert!(english.contains("No environment values were shown"));
        assert!(chinese.contains("重试"));
        assert!(english.contains("Try again"));
        assert!(!chinese.contains("file_read_failed"));
        assert!(!english.contains("file_read_failed"));
        assert!(
            render_status(
                Language::Chinese,
                &UiStatus::Error("registry_value_kind_unsupported")
            )
            .contains("不受支持的注册表值类型")
        );
    }

    #[test]
    fn visual_projection_covers_all_status_tones() {
        assert_eq!(status_tone(&UiStatus::Ready), UiTone::Info);
        assert_eq!(status_tone(&UiStatus::Working), UiTone::Working);
        assert_eq!(status_tone(&UiStatus::SummaryOpened), UiTone::Success);
        assert_eq!(status_tone(&UiStatus::Error("fixture")), UiTone::Error);
    }

    #[test]
    fn comparison_detail_lists_scopes_names_and_kinds_without_values() {
        let summary = ComparisonSummary {
            added: 1,
            removed: 1,
            changed: 1,
            unchanged: 0,
            changes: vec![
                ChangeRecord {
                    scope: Scope::User,
                    name: "USER_ADDED".into(),
                    kind: ChangeKind::Added,
                },
                ChangeRecord {
                    scope: Scope::System,
                    name: "SYSTEM_REMOVED".into(),
                    kind: ChangeKind::Removed,
                },
                ChangeRecord {
                    scope: Scope::User,
                    name: "USER_CHANGED".into(),
                    kind: ChangeKind::Changed,
                },
            ],
            path_changes: Vec::new(),
        };

        let chinese = render_comparison_detail(Language::Chinese, &summary);
        let english = render_comparison_detail(Language::English, &summary);
        assert!(chinese.contains("用户 · 新增 · USER_ADDED"));
        assert!(chinese.contains("系统 · 删除 · SYSTEM_REMOVED"));
        assert!(english.contains("User · Changed · USER_CHANGED"));
        assert!(!chinese.contains("secret"));
    }

    #[test]
    fn comparison_detail_lists_redacted_path_entries() {
        let summary = ComparisonSummary {
            changed: 1,
            changes: vec![ChangeRecord {
                scope: Scope::User,
                name: "Path".into(),
                kind: ChangeKind::Changed,
            }],
            path_changes: vec![
                PathChangeRecord {
                    scope: Scope::User,
                    entry: r"C:\Users\***\new".into(),
                    kind: PathChangeKind::Added,
                },
                PathChangeRecord {
                    scope: Scope::User,
                    entry: r"C:\Users\***\old".into(),
                    kind: PathChangeKind::Removed,
                },
            ],
            ..ComparisonSummary::default()
        };

        let chinese = render_comparison_detail(Language::Chinese, &summary);
        let english = render_comparison_detail(Language::English, &summary);
        assert!(chinese.contains(r"用户 · PATH 新增 · C:\Users\***\new"));
        assert!(english.contains(r"User · PATH removed · C:\Users\***\old"));
    }

    #[test]
    fn comparison_detail_bounds_names_and_rows() {
        let changes = (0..=MAX_COMPARISON_DETAIL_LINES)
            .map(|index| ChangeRecord {
                scope: Scope::User,
                name: format!("{index}-{}", "N".repeat(MAX_COMPARISON_NAME_CHARS + 1)),
                kind: ChangeKind::Changed,
            })
            .collect::<Vec<_>>();
        let summary = ComparisonSummary {
            changed: changes.len(),
            changes,
            ..ComparisonSummary::default()
        };

        let detail = render_comparison_detail(Language::English, &summary);
        assert_eq!(detail.lines().count(), MAX_COMPARISON_DETAIL_LINES + 1);
        assert!(detail.contains("1 more not shown"));
        assert!(detail.lines().next().unwrap().ends_with('…'));
    }

    #[test]
    fn path_details_share_the_global_row_and_length_limits() {
        let path_changes = (0..=MAX_COMPARISON_DETAIL_LINES)
            .map(|index| PathChangeRecord {
                scope: Scope::System,
                entry: format!(r"C:\{index}\{}", "P".repeat(MAX_COMPARISON_NAME_CHARS + 1)),
                kind: PathChangeKind::Added,
            })
            .collect::<Vec<_>>();
        let summary = ComparisonSummary {
            changed: 1,
            path_changes,
            ..ComparisonSummary::default()
        };

        let detail = render_comparison_detail(Language::English, &summary);
        assert_eq!(detail.lines().count(), MAX_COMPARISON_DETAIL_LINES + 1);
        assert!(detail.contains("1 more not shown"));
        assert!(detail.lines().next().unwrap().ends_with('…'));
    }
}
