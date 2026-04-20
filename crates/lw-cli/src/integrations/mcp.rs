use serde_json::{Map, Value};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERSION_MARKER: &str = "_lw_version";

/// Result of a merge attempt.
#[derive(Debug, PartialEq)]
pub enum MergeOutcome {
    /// Entry inserted (was absent).
    Inserted,
    /// Entry updated; previous version matched expected (clean upgrade).
    Updated,
    /// Entry exists but appears user-edited; not modified.
    Conflict { existing: Value },
    /// Entry already matches desired; no-op.
    NoOp,
}

/// Merge a managed entry into a JSON config.
///
/// `key_path` is dot-separated (e.g., "mcpServers.llm-wiki").
/// `entry` MUST contain a `_lw_version` field; it will be added if missing.
/// `expected_prev_version` is the version we last shipped; if the existing entry's
/// `_lw_version` matches, we treat it as an unmodified upgrade and replace silently.
/// If it does not match (or `_lw_version` is absent), we treat it as user-edited
/// and return `Conflict` without modifying.
pub fn merge_entry(
    config: &mut Value,
    key_path: &str,
    mut entry: Value,
    expected_prev_version: Option<&str>,
) -> anyhow::Result<MergeOutcome> {
    // Ensure entry has a version marker
    if !entry
        .as_object()
        .map(|o| o.contains_key(VERSION_MARKER))
        .unwrap_or(false)
    {
        anyhow::bail!("entry must include '{VERSION_MARKER}' field");
    }

    let parts: Vec<&str> = key_path.split('.').collect();
    let (last, parents) = parts.split_last().unwrap();

    // Walk / create parents
    let mut cursor = config;
    for p in parents {
        if !cursor.is_object() {
            *cursor = Value::Object(Map::new());
        }
        let obj = cursor.as_object_mut().unwrap();
        cursor = obj
            .entry((*p).to_string())
            .or_insert(Value::Object(Map::new()));
    }
    if !cursor.is_object() {
        *cursor = Value::Object(Map::new());
    }
    let obj = cursor.as_object_mut().unwrap();

    match obj.get(*last) {
        None => {
            obj.insert((*last).to_string(), entry);
            Ok(MergeOutcome::Inserted)
        }
        Some(existing) if existing == &entry => Ok(MergeOutcome::NoOp),
        Some(existing) => {
            let existing_ver = existing.get(VERSION_MARKER).and_then(|v| v.as_str());
            match (existing_ver, expected_prev_version) {
                (Some(ev), Some(pv)) if ev == pv => {
                    // Clean upgrade. Entry is required to include VERSION_MARKER
                    // (we bailed upstream otherwise), but be defensive and fall
                    // back to "unknown" if the shape somehow drifted.
                    let new_ver = entry
                        .get(VERSION_MARKER)
                        .cloned()
                        .unwrap_or_else(|| Value::String("unknown".into()));
                    if let Some(obj_entry) = entry.as_object_mut() {
                        obj_entry.insert(VERSION_MARKER.into(), new_ver);
                    }
                    obj.insert((*last).to_string(), entry);
                    Ok(MergeOutcome::Updated)
                }
                _ => Ok(MergeOutcome::Conflict {
                    existing: existing.clone(),
                }),
            }
        }
    }
}

/// Atomically write a JSON file: backup → temp → fsync → rename.
/// Returns the backup path so callers can report it.
pub fn atomic_write_with_backup(
    path: &Path,
    body: &str,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    let backup_path = if path.exists() {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let bak = path.with_extension(format!(
            "{}.bak.{ts}",
            path.extension().and_then(|s| s.to_str()).unwrap_or("")
        ));
        std::fs::copy(path, &bak)?;
        Some(bak)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        None
    };
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, body)?;
    let f = std::fs::File::open(&tmp)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(backup_path)
}

/// Remove an entry from JSON config by key_path. Returns true if removed.
pub fn remove_entry(config: &mut Value, key_path: &str) -> bool {
    let parts: Vec<&str> = key_path.split('.').collect();
    let (last, parents) = parts.split_last().unwrap();
    let mut cursor = config;
    for p in parents {
        match cursor.get_mut(*p) {
            Some(child) => cursor = child,
            None => return false,
        }
    }
    cursor
        .as_object_mut()
        .map(|o| o.remove(*last).is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn entry(version: &str) -> Value {
        json!({
            "command": "lw",
            "args": ["serve"],
            VERSION_MARKER: version
        })
    }

    #[test]
    fn merge_inserts_when_absent() {
        let mut cfg = json!({});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0"), None).unwrap();
        assert_eq!(outcome, MergeOutcome::Inserted);
        assert_eq!(
            cfg["mcpServers"]["llm-wiki"][VERSION_MARKER],
            json!("0.2.0")
        );
    }

    #[test]
    fn merge_noop_when_identical() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.2.0")}});
        let outcome = merge_entry(
            &mut cfg,
            "mcpServers.llm-wiki",
            entry("0.2.0"),
            Some("0.2.0"),
        )
        .unwrap();
        assert_eq!(outcome, MergeOutcome::NoOp);
    }

    #[test]
    fn merge_updates_when_prev_version_matches() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.1.0")}});
        let outcome = merge_entry(
            &mut cfg,
            "mcpServers.llm-wiki",
            entry("0.2.0"),
            Some("0.1.0"),
        )
        .unwrap();
        assert_eq!(outcome, MergeOutcome::Updated);
        assert_eq!(
            cfg["mcpServers"]["llm-wiki"][VERSION_MARKER],
            json!("0.2.0")
        );
    }

    #[test]
    fn merge_conflict_when_user_edited() {
        let mut user_edited = entry("0.1.0");
        user_edited["args"] = json!(["serve", "--root", "/custom"]);
        let mut cfg = json!({"mcpServers": {"llm-wiki": user_edited.clone()}});
        let outcome = merge_entry(
            &mut cfg,
            "mcpServers.llm-wiki",
            entry("0.2.0"),
            Some("0.0.1"),
        )
        .unwrap();
        match outcome {
            MergeOutcome::Conflict { existing } => assert_eq!(existing, user_edited),
            _ => panic!("expected Conflict"),
        }
        // Config must be unchanged
        assert_eq!(cfg["mcpServers"]["llm-wiki"], user_edited);
    }

    #[test]
    fn merge_preserves_sibling_entries() {
        let mut cfg = json!({
            "mcpServers": {
                "other-tool": {"command": "other"},
                "llm-wiki": entry("0.1.0"),
            },
            "permissions": {"allow": ["foo"]},
        });
        merge_entry(
            &mut cfg,
            "mcpServers.llm-wiki",
            entry("0.2.0"),
            Some("0.1.0"),
        )
        .unwrap();
        assert_eq!(cfg["mcpServers"]["other-tool"], json!({"command": "other"}));
        assert_eq!(cfg["permissions"]["allow"], json!(["foo"]));
    }

    #[test]
    fn merge_rejects_entry_without_version_marker() {
        let mut cfg = json!({});
        let bad = json!({"command": "lw", "args": ["serve"]});
        let result = merge_entry(&mut cfg, "mcpServers.llm-wiki", bad, None);
        assert!(result.is_err());
    }

    #[test]
    fn atomic_write_creates_backup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{\"old\": true}").unwrap();
        let backup = atomic_write_with_backup(&path, "{\"new\": true}").unwrap();
        assert!(backup.is_some());
        let bak = backup.unwrap();
        assert!(bak.exists());
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), "{\"old\": true}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"new\": true}");
    }

    #[test]
    fn atomic_write_no_backup_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let backup = atomic_write_with_backup(&path, "{}").unwrap();
        assert!(backup.is_none());
        assert!(path.exists());
    }

    #[test]
    fn remove_entry_returns_true_when_present() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.2.0"), "other": {}}});
        assert!(remove_entry(&mut cfg, "mcpServers.llm-wiki"));
        assert!(cfg["mcpServers"]["llm-wiki"].is_null());
        assert_eq!(cfg["mcpServers"]["other"], json!({}));
    }

    #[test]
    fn remove_entry_returns_false_when_absent() {
        let mut cfg = json!({});
        assert!(!remove_entry(&mut cfg, "mcpServers.llm-wiki"));
    }
}
