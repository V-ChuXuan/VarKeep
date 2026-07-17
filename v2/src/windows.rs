use crate::domain::{EnvironmentVariable, RegistryValueKind, Scope};
use std::fmt;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsError {
    code: &'static str,
}

impl WindowsError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for WindowsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for WindowsError {}

#[cfg(windows)]
pub fn prefers_chinese_ui() -> bool {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

    // SAFETY: GetUserDefaultUILanguage takes no pointers and returns the caller's UI LANGID.
    ui_language_is_chinese(unsafe { GetUserDefaultUILanguage() })
}

#[cfg(not(windows))]
pub fn prefers_chinese_ui() -> bool {
    false
}

fn ui_language_is_chinese(language_id: u16) -> bool {
    const PRIMARY_LANGUAGE_MASK: u16 = 0x03ff;
    const CHINESE_PRIMARY_LANGUAGE: u16 = 0x0004;
    language_id & PRIMARY_LANGUAGE_MASK == CHINESE_PRIMARY_LANGUAGE
}

#[cfg(windows)]
pub fn format_utc_timestamp(unix_ms: u64) -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;

    let Some(file_time) = unix_ms_to_file_time(unix_ms) else {
        return unix_ms.to_string();
    };
    // SAFETY: SYSTEMTIME is a plain output buffer initialized before use.
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: the pointers reference valid values for the duration of the call.
    if unsafe { FileTimeToSystemTime(&file_time, &mut utc) } == 0 {
        return unix_ms.to_string();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        utc.wYear, utc.wMonth, utc.wDay, utc.wHour, utc.wMinute
    )
}

#[cfg(not(windows))]
pub fn format_utc_timestamp(unix_ms: u64) -> String {
    unix_ms.to_string()
}

#[cfg(windows)]
pub fn format_local_timestamp(unix_ms: u64) -> String {
    use std::ptr::null;
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    let Some(file_time) = unix_ms_to_file_time(unix_ms) else {
        return unix_ms.to_string();
    };
    // SAFETY: both SYSTEMTIME values are plain output buffers initialized before use.
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: the pointers reference valid values for the duration of the call.
    if unsafe { FileTimeToSystemTime(&file_time, &mut utc) } == 0 {
        return unix_ms.to_string();
    }
    // SAFETY: a null timezone selects the active system timezone and output points to valid memory.
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: the pointers reference valid values for the duration of the call.
    if unsafe { SystemTimeToTzSpecificLocalTime(null(), &utc, &mut local) } == 0 {
        return unix_ms.to_string();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute
    )
}

#[cfg(windows)]
fn unix_ms_to_file_time(unix_ms: u64) -> Option<windows_sys::Win32::Foundation::FILETIME> {
    use windows_sys::Win32::Foundation::FILETIME;

    const WINDOWS_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    let ticks = unix_ms
        .checked_mul(10_000)?
        .checked_add(WINDOWS_EPOCH_TICKS)?;
    Some(FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    })
}

#[cfg(not(windows))]
pub fn format_local_timestamp(unix_ms: u64) -> String {
    unix_ms.to_string()
}

#[cfg(windows)]
pub fn read_persistent_environment() -> Result<Vec<EnvironmentVariable>, WindowsError> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let mut output = Vec::new();
    output.extend(read_registry_scope(
        HKEY_CURRENT_USER,
        "Environment",
        Scope::User,
    )?);
    output.extend(read_registry_scope(
        HKEY_LOCAL_MACHINE,
        "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        Scope::System,
    )?);
    Ok(output)
}

#[cfg(not(windows))]
pub fn read_persistent_environment() -> Result<Vec<EnvironmentVariable>, WindowsError> {
    Err(WindowsError::new("windows_required"))
}

pub fn open_directory(path: &Path) -> Result<(), WindowsError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| WindowsError::new("directory_unavailable"))?;
    if !canonical.is_dir() {
        return Err(WindowsError::new("directory_unavailable"));
    }
    std::process::Command::new("explorer.exe")
        .arg(canonical)
        .spawn()
        .map(|_| ())
        .map_err(|_| WindowsError::new("open_directory_failed"))
}

pub fn open_file(path: &Path) -> Result<(), WindowsError> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| WindowsError::new("file_unavailable"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(WindowsError::new("file_unavailable"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| WindowsError::new("file_unavailable"))?;
    std::process::Command::new("explorer.exe")
        .arg(canonical)
        .spawn()
        .map(|_| ())
        .map_err(|_| WindowsError::new("open_file_failed"))
}

#[cfg(windows)]
fn read_registry_scope(
    root: windows_sys::Win32::System::Registry::HKEY,
    key_path: &str,
    scope: Scope,
) -> Result<Vec<EnvironmentVariable>, WindowsError> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, KEY_READ, RegCloseKey, RegEnumValueW, RegOpenKeyExW,
    };

    let wide_path = key_path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut key: HKEY = null_mut();
    // SAFETY: the path is NUL terminated, the output handle is valid, and the handle is closed below.
    let status = unsafe { RegOpenKeyExW(root, wide_path.as_ptr(), 0, KEY_READ, &mut key) };
    if status != ERROR_SUCCESS {
        return Err(WindowsError::new("registry_open_failed"));
    }

    let result = (|| {
        let mut output = Vec::new();
        let mut total_utf16_bytes = 0usize;
        let mut index = 0u32;
        loop {
            let mut name = vec![0u16; crate::domain::MAX_NAME_UTF16_UNITS + 1];
            let mut data = vec![0u8; crate::domain::MAX_VALUE_UTF16_BYTES + 2];
            let mut name_len = name.len() as u32;
            let mut data_len = data.len() as u32;
            let mut value_type = 0u32;
            // SAFETY: buffers are writable for the lengths provided and remain alive for the call.
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    null_mut(),
                    &mut value_type,
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status == ERROR_MORE_DATA {
                return Err(WindowsError::new("registry_value_too_large"));
            }
            if status != ERROR_SUCCESS {
                return Err(WindowsError::new("registry_read_failed"));
            }
            index += 1;
            let name = String::from_utf16(&name[..name_len as usize])
                .map_err(|_| WindowsError::new("registry_name_invalid"))?;
            if name.is_empty() || name.contains(['\0', '=']) {
                continue;
            }
            let kind = registry_value_kind(value_type)?;
            if !data_len.is_multiple_of(2) {
                return Err(WindowsError::new("registry_value_invalid"));
            }
            let mut words = data[..data_len as usize]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>();
            if words.last() == Some(&0) {
                words.pop();
            }
            let value = String::from_utf16(&words)
                .map_err(|_| WindowsError::new("registry_value_invalid"))?;
            let item_bytes = name_len as usize * 2 + data_len as usize;
            total_utf16_bytes = total_utf16_bytes
                .checked_add(item_bytes)
                .ok_or_else(|| WindowsError::new("registry_scope_too_large"))?;
            if total_utf16_bytes > crate::domain::MAX_SCOPE_UTF16_BYTES {
                return Err(WindowsError::new("registry_scope_too_large"));
            }
            if output.len() >= crate::domain::MAX_VARIABLES_PER_SCOPE {
                return Err(WindowsError::new("registry_scope_too_large"));
            }
            output.push(
                EnvironmentVariable::new(scope, name, value, kind)
                    .map_err(|_| WindowsError::new("registry_value_rejected"))?,
            );
        }
        Ok(output)
    })();
    // SAFETY: key was initialized by RegOpenKeyExW and is no longer used after closing.
    unsafe { RegCloseKey(key) };
    result
}

#[cfg(windows)]
fn registry_value_kind(value_type: u32) -> Result<RegistryValueKind, WindowsError> {
    use windows_sys::Win32::System::Registry::{REG_EXPAND_SZ, REG_SZ};

    match value_type {
        REG_SZ => Ok(RegistryValueKind::String),
        REG_EXPAND_SZ => Ok(RegistryValueKind::ExpandString),
        _ => Err(WindowsError::new("registry_value_kind_unsupported")),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn persistent_registry_values_can_be_read_without_exposing_them() {
        let values = read_persistent_environment().expect("persistent registry should be readable");
        assert!(values.iter().all(|value| {
            matches!(
                value.kind,
                RegistryValueKind::String | RegistryValueKind::ExpandString
            )
        }));
    }

    #[test]
    fn ui_language_detection_only_selects_chinese_langids() {
        assert!(ui_language_is_chinese(0x0804));
        assert!(ui_language_is_chinese(0x0404));
        assert!(!ui_language_is_chinese(0x0409));
        assert!(!ui_language_is_chinese(0));
    }

    #[test]
    fn local_timestamp_uses_compact_numeric_format() {
        let formatted = format_local_timestamp(0);
        assert_eq!(formatted.len(), 16);
        assert_eq!(&formatted[4..5], "-");
        assert_eq!(&formatted[7..8], "-");
        assert_eq!(&formatted[10..11], " ");
        assert_eq!(&formatted[13..14], ":");
    }

    #[test]
    fn utc_timestamp_is_deterministic_and_labeled() {
        assert_eq!(format_utc_timestamp(0), "1970-01-01 00:00 UTC");
    }

    #[test]
    fn unsupported_registry_value_kind_is_rejected() {
        assert_eq!(
            registry_value_kind(windows_sys::Win32::System::Registry::REG_DWORD)
                .unwrap_err()
                .code(),
            "registry_value_kind_unsupported"
        );
    }
}
