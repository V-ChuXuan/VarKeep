use crate::domain::{EnvironmentVariable, RegistryValueKind, Scope, Snapshot};
use crate::privacy::redact_user_path;
use crate::windows::format_utc_timestamp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SummaryLanguage {
    Chinese,
    English,
}

pub fn render_summary(snapshot: &Snapshot, language: SummaryLanguage) -> String {
    let user_count = snapshot
        .variables
        .iter()
        .filter(|variable| variable.scope == Scope::User)
        .count();
    let system_count = snapshot.variables.len() - user_count;
    let mut output = String::new();

    match language {
        SummaryLanguage::Chinese => {
            output.push_str("# VarKeep 备份摘要\n\n");
            output.push_str("| 创建时间 | 用户变量 | 系统变量 | 合计 |\n");
            output.push_str("| --- | ---: | ---: | ---: |\n");
        }
        SummaryLanguage::English => {
            output.push_str("# VarKeep Backup Summary\n\n");
            output.push_str("| Created | User variables | System variables | Total |\n");
            output.push_str("| --- | ---: | ---: | ---: |\n");
        }
    }
    output.push_str(&format!(
        "| {} | {user_count} | {system_count} | {} |\n\n",
        format_utc_timestamp(snapshot.created_at_unix_ms),
        snapshot.variables.len()
    ));

    append_scope_table(&mut output, snapshot, Scope::User, language);
    append_scope_table(&mut output, snapshot, Scope::System, language);
    match language {
        SummaryLanguage::Chinese => {
            output.push_str("`●●●` 敏感值已隐藏　·　`***` 身份信息已隐藏　·　`…` 内容已截断\n");
        }
        SummaryLanguage::English => {
            output.push_str(
                "`●●●` sensitive value hidden · `***` identity hidden · `…` content truncated\n",
            );
        }
    }
    output
}

fn append_scope_table(
    output: &mut String,
    snapshot: &Snapshot,
    scope: Scope,
    language: SummaryLanguage,
) {
    match (language, scope) {
        (SummaryLanguage::Chinese, Scope::User) => output.push_str("## 用户变量\n\n"),
        (SummaryLanguage::Chinese, Scope::System) => output.push_str("## 系统变量\n\n"),
        (SummaryLanguage::English, Scope::User) => output.push_str("## User variables\n\n"),
        (SummaryLanguage::English, Scope::System) => {
            output.push_str("## System variables\n\n");
        }
    }
    match language {
        SummaryLanguage::Chinese => {
            output.push_str("| 变量 | 类型 | 脱敏值 |\n| --- | --- | --- |\n");
        }
        SummaryLanguage::English => {
            output.push_str("| Variable | Type | Redacted value |\n| --- | --- | --- |\n");
        }
    }

    let mut variables = snapshot
        .variables
        .iter()
        .filter(|variable| variable.scope == scope)
        .collect::<Vec<_>>();
    variables.sort_by_key(|variable| variable.normalized_name());
    for variable in variables {
        let kind = match variable.kind {
            RegistryValueKind::String => "REG_SZ",
            RegistryValueKind::ExpandString => "REG_EXPAND_SZ",
        };
        let preview = redacted_preview(variable, language);
        output.push_str(&format!(
            "| <code>{}</code> | <code>{kind}</code> | {preview} |\n",
            escape_markdown_cell(&variable.name)
        ));
    }
    output.push('\n');
}

fn redacted_preview(variable: &EnvironmentVariable, language: SummaryLanguage) -> String {
    let value = variable.value.expose();
    let length = value.chars().count();
    if value.is_empty() {
        return match language {
            SummaryLanguage::Chinese => "（空）".into(),
            SummaryLanguage::English => "(empty)".into(),
        };
    }
    if is_sensitive_name(&variable.name) {
        return format_preview("●●●", sensitive_detail(language, length));
    }

    if let Some(url) = redact_url(value) {
        return format_preview(&url, String::new());
    }

    if is_path_like(value) || is_safe_literal_name(&variable.name) {
        let sanitized = redact_user_path(value);
        let (preview, truncated) = truncate_chars(&sanitized, 104);
        let item_count = value
            .split(';')
            .filter(|item| !item.trim().is_empty())
            .count();
        let detail = match (language, item_count > 1, truncated) {
            (SummaryLanguage::Chinese, true, _) => {
                format!("（{item_count} 项，{length} 字符）")
            }
            (SummaryLanguage::English, true, _) => {
                format!("({item_count} entries, {length} characters)")
            }
            (SummaryLanguage::Chinese, false, true) => format!("（{length} 字符）"),
            (SummaryLanguage::English, false, true) => format!("({length} characters)"),
            (_, false, false) => String::new(),
        };
        return format_preview(&preview, detail);
    }

    let preview = mask_plain_value(value);
    let detail = match language {
        SummaryLanguage::Chinese => format!("（{length} 字符）"),
        SummaryLanguage::English => format!("({length} characters)"),
    };
    format_preview(&preview, detail)
}

fn sensitive_detail(language: SummaryLanguage, length: usize) -> String {
    match language {
        SummaryLanguage::Chinese => format!("（疑似敏感值，{length} 字符）"),
        SummaryLanguage::English => format!("(suspected sensitive value, {length} characters)"),
    }
}

fn format_preview(preview: &str, detail: String) -> String {
    let code = format!("<code>{}</code>", escape_markdown_cell(preview));
    if detail.is_empty() {
        code
    } else {
        format!("{code}{detail}")
    }
}

fn is_sensitive_name(name: &str) -> bool {
    let normalized = name.to_ascii_uppercase();
    let tokens = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    tokens.iter().any(|token| {
        matches!(
            *token,
            "APIKEY"
                | "ACCESSKEY"
                | "AUTHORIZATION"
                | "CLIENTSECRET"
                | "CONNECTIONSTRING"
                | "CREDENTIAL"
                | "CREDENTIALS"
                | "PASSWORD"
                | "PASSWD"
                | "PRIVATEKEY"
                | "SECRET"
                | "TOKEN"
        )
    }) || contains_token_pair(&tokens, "API", "KEY")
        || contains_token_pair(&tokens, "PRIVATE", "KEY")
        || contains_token_pair(&tokens, "ACCESS", "KEY")
        || contains_token_pair(&tokens, "CLIENT", "SECRET")
        || contains_token_pair(&tokens, "CONNECTION", "STRING")
        || normalized == "KEY"
        || normalized.ends_with("_KEY")
}

fn contains_token_pair(tokens: &[&str], first: &str, second: &str) -> bool {
    tokens
        .windows(2)
        .any(|pair| pair[0] == first && pair[1] == second)
}

fn is_safe_literal_name(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "COMSPEC"
            | "NUMBER_OF_PROCESSORS"
            | "OS"
            | "PATHEXT"
            | "PROCESSOR_ARCHITECTURE"
            | "PROCESSOR_IDENTIFIER"
            | "PROCESSOR_LEVEL"
            | "PROCESSOR_REVISION"
            | "SYSTEMDRIVE"
            | "SYSTEMROOT"
            | "TEMP"
            | "TMP"
            | "USERPROFILE"
            | "WINDIR"
    )
}

fn is_path_like(value: &str) -> bool {
    let mut saw_value = false;
    for item in value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        saw_value = true;
        let bytes = item.as_bytes();
        let drive_path = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/');
        let expanded_path =
            item.starts_with('%') && item.get(1..).and_then(|suffix| suffix.find('%')).is_some();
        if !drive_path && !expanded_path && !item.starts_with("\\\\") && !item.starts_with("//") {
            return false;
        }
    }
    saw_value
}

fn redact_url(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if !lower.starts_with("https://") && !lower.starts_with("http://") {
        return None;
    }
    let boundary = value.find(['?', '#']);
    let base = boundary.map_or(value, |index| &value[..index]);
    let sanitized = redact_url_userinfo(&redact_user_path(base));
    let (mut preview, _) = truncate_chars(&sanitized, 104);
    if let Some(index) = boundary {
        preview.push(if value.as_bytes()[index] == b'?' {
            '?'
        } else {
            '#'
        });
        preview.push_str("***");
    }
    Some(preview)
}

fn redact_url_userinfo(value: &str) -> String {
    let Some(scheme_end) = value.find("://") else {
        return value.to_owned();
    };
    let authority_start = scheme_end + 3;
    let authority_end = value[authority_start..]
        .find('/')
        .map_or(value.len(), |offset| authority_start + offset);
    let Some(userinfo_end) = value[authority_start..authority_end].rfind('@') else {
        return value.to_owned();
    };
    let at = authority_start + userinfo_end;
    format!("{}***{}", &value[..authority_start], &value[at..])
}

fn mask_plain_value(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    if characters.len() <= 4 {
        return "●".repeat(characters.len().max(1));
    }
    format!(
        "{}{}●●●{}{}",
        characters[0],
        characters[1],
        characters[characters.len() - 2],
        characters[characters.len() - 1]
    )
}

fn truncate_chars(value: &str, limit: usize) -> (String, bool) {
    let mut characters = value.chars();
    let preview = characters.by_ref().take(limit).collect::<String>();
    if characters.next().is_some() {
        (format!("{preview}…"), true)
    } else {
        (preview, false)
    }
}

fn escape_markdown_cell(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '|' => "&#124;".to_owned(),
            '\r' | '\n' | '\t' => " ".to_owned(),
            character if character.is_control() => "�".to_owned(),
            character => character.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_name_detection_avoids_generic_key_false_positives() {
        assert!(is_sensitive_name("OPENAI_API_KEY"));
        assert!(is_sensitive_name("OPENAI_APIKEY"));
        assert!(is_sensitive_name("client-secret"));
        assert!(is_sensitive_name("DATABASE_PASSWORD"));
        assert!(!is_sensitive_name("KEYBOARD_LAYOUT"));
        assert!(!is_sensitive_name("MONKEY"));
    }

    #[test]
    fn markdown_cells_do_not_allow_table_or_html_injection() {
        assert_eq!(
            escape_markdown_cell("<script>|x\n"),
            "&lt;script&gt;&#124;x "
        );
    }

    #[test]
    fn url_user_information_is_redacted() {
        let variable = EnvironmentVariable::new(
            Scope::User,
            "SERVICE_URL".into(),
            "https://alice:password@example.com/path".into(),
            RegistryValueKind::String,
        )
        .unwrap();

        let preview = redacted_preview(&variable, SummaryLanguage::English);
        assert!(preview.contains("https://***@example.com/path"));
        assert!(!preview.contains("alice:password"));
    }
}
