pub(crate) fn redact_path_entry(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                '�'
            } else {
                character
            }
        })
        .collect::<String>();
    let candidate = sanitized.trim_matches('"');
    let bytes = candidate.as_bytes();
    let drive_path = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let expanded_path = candidate.starts_with('%')
        && candidate
            .get(1..)
            .and_then(|suffix| suffix.find('%'))
            .is_some();

    if drive_path || expanded_path {
        redact_user_path(candidate)
    } else if candidate.starts_with("\\\\") || candidate.starts_with("//") {
        redact_unc_host(&redact_user_path(candidate))
    } else {
        "●●●".into()
    }
}

pub(crate) fn redact_user_path(value: &str) -> String {
    let mut output = value.to_owned();
    for marker in ["\\users\\", "/users/"] {
        let mut search_from = 0usize;
        loop {
            let lower = output.to_ascii_lowercase();
            let Some(offset) = lower[search_from..].find(marker) else {
                break;
            };
            let start = search_from + offset;
            let segment_start = start + marker.len();
            let segment_end = output[segment_start..]
                .find(['\\', '/'])
                .map_or(output.len(), |offset| segment_start + offset);
            if segment_end == segment_start {
                break;
            }
            if &output[segment_start..segment_end] != "***" {
                output.replace_range(segment_start..segment_end, "***");
            }
            search_from = segment_start + 3;
        }
    }
    output
}

fn redact_unc_host(value: &str) -> String {
    let prefix_length = 2;
    let Some(host_end) = value[prefix_length..]
        .find(['\\', '/'])
        .map(|offset| prefix_length + offset)
    else {
        return "\\\\***".into();
    };
    format!("{}***{}", &value[..prefix_length], &value[host_end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entries_hide_user_and_unc_host_identity() {
        assert_eq!(
            redact_path_entry(r"C:\Users\Alice\bin"),
            r"C:\Users\***\bin"
        );
        assert_eq!(redact_path_entry(r"\\desktop\tools"), r"\\***\tools");
        assert_eq!(redact_path_entry("not-a-path-secret"), "●●●");
    }
}
