use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::privacy::redact_path_entry;

pub const SCHEMA_VERSION: u32 = 2;
pub const MAX_VARIABLES_PER_SCOPE: usize = 4_096;
pub const MAX_NAME_UTF16_UNITS: usize = 16_383;
pub const MAX_VALUE_UTF16_BYTES: usize = 1024 * 1024;
pub const MAX_SCOPE_UTF16_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    User,
    System,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistryValueKind {
    String,
    ExpandString,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Result<Self, DomainError> {
        if value.contains('\0') {
            return Err(DomainError::new("value_contains_nul"));
        }
        if value.encode_utf16().count() * 2 > MAX_VALUE_UTF16_BYTES {
            return Err(DomainError::new("value_too_large"));
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariable {
    pub scope: Scope,
    pub name: String,
    pub value: SecretValue,
    pub kind: RegistryValueKind,
}

impl EnvironmentVariable {
    pub fn new(
        scope: Scope,
        name: String,
        value: String,
        kind: RegistryValueKind,
    ) -> Result<Self, DomainError> {
        validate_name(&name)?;
        Ok(Self {
            scope,
            name,
            value: SecretValue::new(value)?,
            kind,
        })
    }

    pub fn normalized_name(&self) -> String {
        self.name.to_uppercase()
    }
}

impl fmt::Debug for EnvironmentVariable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentVariable")
            .field("scope", &self.scope)
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .field("kind", &self.kind)
            .finish()
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub schema_version: u32,
    pub created_at_unix_ms: u64,
    pub variables: Vec<EnvironmentVariable>,
}

impl Snapshot {
    pub fn new_v2(
        created_at_unix_ms: u64,
        variables: Vec<EnvironmentVariable>,
    ) -> Result<Self, DomainError> {
        validate_variables(&variables)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            created_at_unix_ms,
            variables,
        })
    }

    pub fn validate_v2(&self) -> Result<(), DomainError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(DomainError::new("unsupported_schema"));
        }
        validate_variables(&self.variables)
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Snapshot")
            .field("schema_version", &self.schema_version)
            .field("created_at_unix_ms", &self.created_at_unix_ms)
            .field("variable_count", &self.variables.len())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainError {
    code: &'static str,
}

impl DomainError {
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for DomainError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComparisonSummary {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub changes: Vec<ChangeRecord>,
    pub path_changes: Vec<PathChangeRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeRecord {
    pub scope: Scope,
    pub name: String,
    pub kind: ChangeKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathChangeRecord {
    pub scope: Scope,
    pub entry: String,
    pub kind: PathChangeKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathChangeKind {
    Added,
    Removed,
}

pub fn compare_snapshots(
    baseline: &[EnvironmentVariable],
    current: &[EnvironmentVariable],
) -> ComparisonSummary {
    let left = to_map(baseline);
    let right = to_map(current);
    let mut keys = left.keys().chain(right.keys()).cloned().collect::<Vec<_>>();
    keys.sort();
    keys.dedup();

    let mut result = ComparisonSummary::default();
    for key in keys {
        let old = left.get(&key);
        let now = right.get(&key);
        let (record_kind, display_name) = match (old, now) {
            (None, Some(value)) => {
                result.added += 1;
                (ChangeKind::Added, value.name.clone())
            }
            (Some(value), None) => {
                result.removed += 1;
                (ChangeKind::Removed, value.name.clone())
            }
            (Some(old), Some(now)) if old.value == now.value && old.kind == now.kind => {
                result.unchanged += 1;
                (ChangeKind::Unchanged, now.name.clone())
            }
            (Some(_), Some(now)) => {
                result.changed += 1;
                (ChangeKind::Changed, now.name.clone())
            }
            (None, None) => continue,
        };
        result.changes.push(ChangeRecord {
            scope: key.0,
            name: display_name,
            kind: record_kind,
        });
    }
    result.path_changes = compare_path_entries(&left, &right);
    result
}

fn compare_path_entries(
    baseline: &BTreeMap<(Scope, String), &EnvironmentVariable>,
    current: &BTreeMap<(Scope, String), &EnvironmentVariable>,
) -> Vec<PathChangeRecord> {
    let mut changes = Vec::new();
    for scope in [Scope::User, Scope::System] {
        let key = (scope, "PATH".to_owned());
        let baseline_entries = path_entry_map(baseline.get(&key).map(|item| item.value.expose()));
        let current_entries = path_entry_map(current.get(&key).map(|item| item.value.expose()));

        changes.extend(
            baseline_entries
                .iter()
                .filter(|(normalized, _)| !current_entries.contains_key(*normalized))
                .map(|(_, entry)| PathChangeRecord {
                    scope,
                    entry: redact_path_entry(entry),
                    kind: PathChangeKind::Removed,
                }),
        );
        changes.extend(
            current_entries
                .iter()
                .filter(|(normalized, _)| !baseline_entries.contains_key(*normalized))
                .map(|(_, entry)| PathChangeRecord {
                    scope,
                    entry: redact_path_entry(entry),
                    kind: PathChangeKind::Added,
                }),
        );
    }
    changes
}

fn path_entry_map(value: Option<&str>) -> BTreeMap<String, String> {
    value
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .fold(BTreeMap::new(), |mut entries, entry| {
            entries
                .entry(entry.to_uppercase())
                .or_insert_with(|| entry.to_owned());
            entries
        })
}

fn to_map(variables: &[EnvironmentVariable]) -> BTreeMap<(Scope, String), &EnvironmentVariable> {
    variables
        .iter()
        .map(|variable| ((variable.scope, variable.normalized_name()), variable))
        .collect()
}

fn validate_name(name: &str) -> Result<(), DomainError> {
    if name.is_empty() {
        return Err(DomainError::new("name_empty"));
    }
    if name.contains('\0') || name.contains('=') {
        return Err(DomainError::new("name_invalid"));
    }
    if name.encode_utf16().count() > MAX_NAME_UTF16_UNITS {
        return Err(DomainError::new("name_too_large"));
    }
    Ok(())
}

fn validate_variables(variables: &[EnvironmentVariable]) -> Result<(), DomainError> {
    let mut counts = BTreeMap::<Scope, usize>::new();
    let mut names = BTreeMap::<(Scope, String), ()>::new();
    for variable in variables {
        validate_name(&variable.name)?;
        SecretValue::new(variable.value.expose().to_owned())?;
        let count = counts.entry(variable.scope).or_default();
        *count += 1;
        if *count > MAX_VARIABLES_PER_SCOPE {
            return Err(DomainError::new("too_many_variables"));
        }
        if names
            .insert((variable.scope, variable.normalized_name()), ())
            .is_some()
        {
            return Err(DomainError::new("duplicate_variable"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variable(name: &str, value: &str, kind: RegistryValueKind) -> EnvironmentVariable {
        EnvironmentVariable::new(Scope::User, name.into(), value.into(), kind).unwrap()
    }

    #[test]
    fn secret_debug_is_redacted() {
        let item = variable("TOKEN", "sentinel-secret", RegistryValueKind::String);
        let debug = format!("{item:?}");
        assert!(!debug.contains("sentinel-secret"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn duplicate_names_are_case_insensitive() {
        let error = Snapshot::new_v2(
            1,
            vec![
                variable("Path", "one", RegistryValueKind::String),
                variable("PATH", "two", RegistryValueKind::String),
            ],
        )
        .unwrap_err();
        assert_eq!(error.code(), "duplicate_variable");
    }

    #[test]
    fn comparison_hides_values_and_matches_case_insensitively() {
        let baseline = vec![variable("Path", "old-secret", RegistryValueKind::String)];
        let current = vec![variable("PATH", "new-secret", RegistryValueKind::String)];
        let result = compare_snapshots(&baseline, &current);
        assert_eq!(result.changed, 1);
        let debug = format!("{result:?}");
        assert!(!debug.contains("old-secret"));
        assert!(!debug.contains("new-secret"));
    }

    #[test]
    fn comparison_lists_path_entry_changes_without_identity_leaks() {
        let baseline = vec![variable(
            "Path",
            r"C:\Tools;C:\Users\Alice\old;%SystemRoot%\System32",
            RegistryValueKind::ExpandString,
        )];
        let current = vec![variable(
            "PATH",
            r"c:\tools;C:\Users\Alice\new;%SYSTEMROOT%\System32",
            RegistryValueKind::ExpandString,
        )];

        let result = compare_snapshots(&baseline, &current);

        assert_eq!(result.path_changes.len(), 2);
        assert!(result.path_changes.iter().any(|change| {
            change.kind == PathChangeKind::Removed && change.entry == r"C:\Users\***\old"
        }));
        assert!(result.path_changes.iter().any(|change| {
            change.kind == PathChangeKind::Added && change.entry == r"C:\Users\***\new"
        }));
        assert!(!format!("{result:?}").contains("Alice"));
    }

    #[test]
    fn empty_value_is_valid_but_nul_is_rejected() {
        assert!(
            variable("EMPTY", "", RegistryValueKind::String)
                .value
                .expose()
                .is_empty()
        );
        assert_eq!(
            EnvironmentVariable::new(
                Scope::User,
                "BAD".into(),
                "a\0b".into(),
                RegistryValueKind::String,
            )
            .unwrap_err()
            .code(),
            "value_contains_nul"
        );
    }
}
