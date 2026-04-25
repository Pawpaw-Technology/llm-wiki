use serde_json::{Map, Value};
use std::io::Write;
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
/// `entry` MUST contain a `_lw_version` field.
///
/// Conflict detection works on structure, not version: an existing entry is a
/// clean upgrade (`Updated`) when its fields other than `_lw_version` equal
/// what we'd write. Any structural divergence — different `command`/`args`,
/// or extra user-added fields we don't manage — yields `Conflict` so the
/// user's customization survives.
pub fn merge_entry(
    config: &mut Value,
    key_path: &str,
    entry: Value,
) -> anyhow::Result<MergeOutcome> {
    if !entry
        .as_object()
        .map(|o| o.contains_key(VERSION_MARKER))
        .unwrap_or(false)
    {
        anyhow::bail!("entry must include '{VERSION_MARKER}' field");
    }

    let parts: Vec<&str> = key_path.split('.').collect();
    let (last, parents) = parts.split_last().unwrap();

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
        Some(existing) if managed_fields_match(existing, &entry) => {
            obj.insert((*last).to_string(), entry);
            Ok(MergeOutcome::Updated)
        }
        Some(existing) => Ok(MergeOutcome::Conflict {
            existing: existing.clone(),
        }),
    }
}

/// True when two entries agree on every field except `_lw_version`. Used to
/// distinguish a plain version-marker bump from a real user edit.
fn managed_fields_match(a: &Value, b: &Value) -> bool {
    let (Some(a_obj), Some(b_obj)) = (a.as_object(), b.as_object()) else {
        return false;
    };
    let strip = |obj: &Map<String, Value>| -> Map<String, Value> {
        obj.iter()
            .filter(|(k, _)| k.as_str() != VERSION_MARKER)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };
    strip(a_obj) == strip(b_obj)
}

/// Atomically write a JSON file: backup → temp → fsync → rename.
///
/// Staging uses `tempfile::NamedTempFile::new_in(parent)` so the temp path
/// is unique and exclusive — a pre-existing symlink at any predictable name
/// cannot redirect the write to a victim file.  Backup filenames carry
/// nanosecond resolution so two backups created within the same second never
/// collide.  The parent directory is fsynced after the rename on Unix to
/// ensure the directory entry is durable.
///
/// Returns the backup path so callers can report it.
pub fn atomic_write_with_backup(
    path: &Path,
    body: &str,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;

    // Back up the existing file with a nanosecond-resolution suffix so
    // two consecutive calls in the same second produce distinct paths.
    let backup_path = if path.exists() {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let bak = path.with_extension(format!(
            "{}.bak.{nanos}",
            path.extension().and_then(|s| s.to_str()).unwrap_or("")
        ));
        std::fs::copy(path, &bak)?;
        Some(bak)
    } else {
        None
    };

    // Stage through an exclusive temp file in the destination directory.
    // NamedTempFile::new_in creates a file with an unpredictable name,
    // so a symlink at any guessable path cannot intercept the write.
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(body.as_bytes())?;
    tmp.as_file().sync_all()?;
    // persist() atomically renames the temp file to `path`.
    // On drop without persist(), NamedTempFile deletes the temp file —
    // no orphan *.tmp files are ever left behind.
    tmp.persist(path).map_err(|e| e.error)?;

    sync_parent_dir(parent)?;
    Ok(backup_path)
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> anyhow::Result<()> {
    let dir = std::fs::File::open(parent)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> anyhow::Result<()> {
    Ok(())
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

/// Result of an ownership-checked removal attempt.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum RemoveOutcome {
    /// Entry was present, matched the managed shape, and has been removed.
    Removed,
    /// Entry was present but appears user-edited; left untouched.
    PreservedUserEdited { existing: Value },
    /// Entry was not present at `key_path`; nothing to do.
    NotPresent,
}

/// Remove a managed entry only if its current shape matches `canonical`.
///
/// Ownership is determined by `managed_fields_match`: the existing entry must
/// agree with `canonical` on every field except `_lw_version`. If the entry
/// has extra fields or a different `command`/`args`, it is considered
/// user-edited and left untouched.
///
/// * `Removed` — entry matched; caller should persist `config`.
/// * `PreservedUserEdited` — entry exists but was user-modified; caller should warn.
/// * `NotPresent` — key_path resolved to nothing; caller should report no-op.
#[allow(dead_code)]
pub fn remove_if_managed(config: &mut Value, key_path: &str, canonical: &Value) -> RemoveOutcome {
    let parts: Vec<&str> = key_path.split('.').collect();
    let (last, parents) = parts.split_last().unwrap();

    // Navigate to the parent object, following existing structure (read-only).
    let mut cursor: &Value = config;
    for p in parents {
        match cursor.get(*p) {
            Some(child) => cursor = child,
            None => return RemoveOutcome::NotPresent,
        }
    }
    let existing = match cursor.as_object().and_then(|o| o.get(*last)) {
        Some(v) => v.clone(),
        None => return RemoveOutcome::NotPresent,
    };

    if managed_fields_match(&existing, canonical) {
        // Ownership confirmed — now remove via mutable traversal.
        remove_entry(config, key_path);
        RemoveOutcome::Removed
    } else {
        RemoveOutcome::PreservedUserEdited { existing }
    }
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
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0")).unwrap();
        assert_eq!(outcome, MergeOutcome::Inserted);
        assert_eq!(
            cfg["mcpServers"]["llm-wiki"][VERSION_MARKER],
            json!("0.2.0")
        );
    }

    #[test]
    fn merge_noop_when_identical() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.2.0")}});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0")).unwrap();
        assert_eq!(outcome, MergeOutcome::NoOp);
    }

    #[test]
    fn merge_updates_when_only_version_differs() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.1.0")}});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0")).unwrap();
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
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0")).unwrap();
        match outcome {
            MergeOutcome::Conflict { existing } => assert_eq!(existing, user_edited),
            _ => panic!("expected Conflict"),
        }
        assert_eq!(cfg["mcpServers"]["llm-wiki"], user_edited);
    }

    #[test]
    fn merge_updates_cross_release_when_structure_matches() {
        // Real-world scenario: v0.2.2 wrote the entry, user upgrades to
        // v0.2.3 and reruns `lw integrate`. The only field that differs
        // is `_lw_version`; command/args are what we'd write. This MUST
        // be treated as a clean upgrade, not Conflict.
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.2.2")}});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.3")).unwrap();
        assert_eq!(
            outcome,
            MergeOutcome::Updated,
            "expected Updated for cross-release upgrade"
        );
        assert_eq!(
            cfg["mcpServers"]["llm-wiki"][VERSION_MARKER],
            json!("0.2.3")
        );
    }

    #[test]
    fn merge_conflict_when_existing_has_unmanaged_extra_field() {
        // User added an `env` key that `lw integrate` never writes. Even
        // though the `_lw_version` markers would match, the extra field
        // means we must NOT silently replace — the user's addition would
        // be lost.
        let mut user_extended = entry("0.2.3");
        user_extended["env"] = json!({"LW_WIKI_ROOT": "/tmp/custom"});
        let mut cfg = json!({"mcpServers": {"llm-wiki": user_extended.clone()}});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.3")).unwrap();
        assert!(
            matches!(outcome, MergeOutcome::Conflict { .. }),
            "expected Conflict when existing entry has unmanaged extra field, got {outcome:?}"
        );
        assert_eq!(cfg["mcpServers"]["llm-wiki"], user_extended);
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
        merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0")).unwrap();
        assert_eq!(cfg["mcpServers"]["other-tool"], json!({"command": "other"}));
        assert_eq!(cfg["permissions"]["allow"], json!(["foo"]));
    }

    #[test]
    fn merge_rejects_entry_without_version_marker() {
        let mut cfg = json!({});
        let bad = json!({"command": "lw", "args": ["serve"]});
        let result = merge_entry(&mut cfg, "mcpServers.llm-wiki", bad);
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

    // ---- remove_if_managed tests (Criterion 3) ----

    /// T1: managed entry removed cleanly.
    #[test]
    fn remove_if_managed_removes_managed_entry() {
        let canonical = entry("0.2.0");
        let mut cfg = json!({"mcpServers": {"llm-wiki": canonical.clone()}});
        let outcome = remove_if_managed(&mut cfg, "mcpServers.llm-wiki", &canonical);
        assert_eq!(outcome, RemoveOutcome::Removed);
        assert!(
            cfg["mcpServers"]["llm-wiki"].is_null(),
            "entry should be absent after removal"
        );
    }

    /// T1b: managed entry removed even when only version differs (shape match ignores version).
    #[test]
    fn remove_if_managed_removes_when_version_differs() {
        let installed = entry("0.1.0");
        let canonical = entry("0.2.0"); // current release
        let mut cfg = json!({"mcpServers": {"llm-wiki": installed}});
        let outcome = remove_if_managed(&mut cfg, "mcpServers.llm-wiki", &canonical);
        assert_eq!(outcome, RemoveOutcome::Removed);
        assert!(cfg["mcpServers"]["llm-wiki"].is_null());
    }

    /// T2: user-edited entry (extra `env` field) is preserved with PreservedUserEdited.
    #[test]
    fn remove_if_managed_preserves_user_edited_extra_field() {
        let canonical = entry("0.2.0");
        let mut user_extended = entry("0.2.0");
        user_extended["env"] = json!({"LW_WIKI_ROOT": "/tmp/custom"});
        let mut cfg = json!({"mcpServers": {"llm-wiki": user_extended.clone()}});
        let outcome = remove_if_managed(&mut cfg, "mcpServers.llm-wiki", &canonical);
        assert!(
            matches!(outcome, RemoveOutcome::PreservedUserEdited { .. }),
            "expected PreservedUserEdited, got {outcome:?}"
        );
        // Entry must still be present and unchanged.
        assert_eq!(cfg["mcpServers"]["llm-wiki"], user_extended);
    }

    /// T2b: user-edited entry (modified `args`) is preserved.
    #[test]
    fn remove_if_managed_preserves_user_edited_args() {
        let canonical = entry("0.2.0");
        let mut user_edited = entry("0.2.0");
        user_edited["args"] = json!(["serve", "--root", "/my/wiki"]);
        let mut cfg = json!({"mcpServers": {"llm-wiki": user_edited.clone()}});
        let outcome = remove_if_managed(&mut cfg, "mcpServers.llm-wiki", &canonical);
        assert!(
            matches!(outcome, RemoveOutcome::PreservedUserEdited { .. }),
            "expected PreservedUserEdited, got {outcome:?}"
        );
        assert_eq!(cfg["mcpServers"]["llm-wiki"], user_edited);
    }

    /// T3: missing key returns NotPresent.
    #[test]
    fn remove_if_managed_not_present_when_key_absent() {
        let canonical = entry("0.2.0");
        let mut cfg = json!({});
        let outcome = remove_if_managed(&mut cfg, "mcpServers.llm-wiki", &canonical);
        assert_eq!(outcome, RemoveOutcome::NotPresent);
    }

    /// T3b: NotPresent when parent key exists but target key is absent.
    #[test]
    fn remove_if_managed_not_present_when_target_absent_but_parent_exists() {
        let canonical = entry("0.2.0");
        let mut cfg = json!({"mcpServers": {"other-tool": {}}});
        let outcome = remove_if_managed(&mut cfg, "mcpServers.llm-wiki", &canonical);
        assert_eq!(outcome, RemoveOutcome::NotPresent);
        // sibling must be untouched
        assert_eq!(cfg["mcpServers"]["other-tool"], json!({}));
    }

    /// Criterion 4: A pre-existing symlink at the old predictable `*.tmp` path
    /// must NOT redirect the write to a victim file. The victim must remain
    /// unchanged and the destination must receive the new content.
    #[cfg(unix)]
    #[test]
    fn atomic_write_symlink_at_predictable_tmp_does_not_redirect() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let victim = dir.path().join("victim.json");

        // Write initial content to destination and victim
        std::fs::write(&path, "{\"old\": true}").unwrap();
        std::fs::write(&victim, "VICTIM_CONTENT").unwrap();

        // Create symlink at the OLD predictable temp path: settings.json.tmp
        // (which the buggy code would have used as its staging file)
        let predictable_tmp = dir.path().join("settings.json.tmp");
        symlink(&victim, &predictable_tmp).unwrap();

        // Run the write — must not follow the symlink to victim
        let backup = atomic_write_with_backup(&path, "{\"new\": true}").unwrap();

        // Destination has new content
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"new\": true}",
            "destination should have new content"
        );
        // Victim must be completely untouched
        assert_eq!(
            std::fs::read_to_string(&victim).unwrap(),
            "VICTIM_CONTENT",
            "victim file must be unchanged"
        );
        // A backup of the original must exist
        assert!(
            backup.is_some(),
            "backup should be created for existing file"
        );
    }

    /// Criterion 5: Two consecutive calls on the same path within the same
    /// second must produce two distinct backup files (no clobber).
    #[test]
    fn atomic_write_two_consecutive_backups_are_distinct() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");

        // First write — creates the file
        std::fs::write(&path, "{\"v\": 1}").unwrap();
        let bak1 = atomic_write_with_backup(&path, "{\"v\": 2}")
            .unwrap()
            .expect("first backup must exist");

        // Second write (immediately after, same second)
        let bak2 = atomic_write_with_backup(&path, "{\"v\": 3}")
            .unwrap()
            .expect("second backup must exist");

        // The two backup paths must differ
        assert_ne!(
            bak1, bak2,
            "consecutive backups must have distinct paths (no clobber)"
        );
        // Both backup files must exist with the correct content
        assert!(bak1.exists(), "first backup file must exist on disk");
        assert!(bak2.exists(), "second backup file must exist on disk");
        assert_eq!(
            std::fs::read_to_string(&bak1).unwrap(),
            "{\"v\": 1}",
            "first backup should contain original content"
        );
        assert_eq!(
            std::fs::read_to_string(&bak2).unwrap(),
            "{\"v\": 2}",
            "second backup should contain second-write content"
        );
    }

    /// Criterion 6: After a successful write, the destination directory must
    /// contain no orphan `*.tmp` files.
    #[test]
    fn atomic_write_leaves_no_orphan_tmp_files() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");

        // Write once (new file, no backup)
        atomic_write_with_backup(&path, "{\"first\": true}").unwrap();

        // Write again (existing file, generates a backup)
        atomic_write_with_backup(&path, "{\"second\": true}").unwrap();

        // Scan the directory for any *.tmp files — there must be none
        let tmp_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|ext| ext == "tmp")
                    .unwrap_or(false)
            })
            .collect();

        assert!(
            tmp_files.is_empty(),
            "no orphan *.tmp files should remain after a successful write; found: {:?}",
            tmp_files.iter().map(|e| e.path()).collect::<Vec<_>>()
        );
    }
}
