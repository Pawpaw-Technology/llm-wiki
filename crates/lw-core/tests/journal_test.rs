//! Tests for `lw_core::journal::append_capture` and friends (issue #37).
//!
//! Acceptance criteria covered:
//!   1. `lw capture "text"` appends to today's journal.
//!   2. Journal page auto-created with frontmatter (`title`, `tags: [journal]`,
//!      `created: YYYY-MM-DD`) when not yet present.
//!   3. Each line is timestamped `HH:MM` (24h, local timezone).
//!   4. `--tag <tag>` (repeatable) and `--source <url>` flags render correctly.
//!   7. `find_stale_captures` reports journal pages older than the threshold.

use lw_core::fs::init_wiki;
use lw_core::journal::{
    append_capture, find_stale_captures, format_capture_line, format_date_iso, format_time_hm,
    journal_path_for_date, DEFAULT_STALE_AFTER_DAYS, JOURNAL_DIR,
};
use lw_core::schema::WikiSchema;
use lw_core::WikiError;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use time::macros::{date, time};

/// Init a fresh wiki under `tmp` and return its root.
fn fresh_wiki() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    init_wiki(tmp.path(), &WikiSchema::default()).expect("init_wiki");
    tmp
}

/// Initialise a git repo at `path` with sane identity config.
fn init_repo(path: &Path) {
    let out = Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(path)
        .output()
        .expect("git init");
    assert!(out.status.success());
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(path)
        .output()
        .unwrap();
}

// ── Acceptance criterion 1 + 3: append + timestamp ───────────────────────────

#[test]
fn append_capture_creates_today_journal_with_timestamp_line() {
    let tmp = fresh_wiki();
    let d = date!(2026 - 04 - 25);
    let t = time!(10:23);

    let result = append_capture(
        tmp.path(),
        d,
        t,
        "comrak can round-trip markdown via arena AST",
        &[],
        None,
    )
    .expect("capture must succeed");

    assert!(result.created, "first capture must auto-create the page");
    let expected_path = tmp.path().join("wiki/_journal/2026-04-25.md");
    assert_eq!(result.path, expected_path);
    assert_eq!(result.display_path, "wiki/_journal/2026-04-25.md");
    assert!(expected_path.exists(), "file must exist on disk");

    let content = fs::read_to_string(&expected_path).unwrap();
    assert!(
        content.contains("- **10:23** comrak can round-trip markdown via arena AST"),
        "capture line missing or malformed; content: {content}"
    );
    // Timestamp prefix is bold + zero-padded HH:MM.
    assert!(
        content.contains("**10:23**"),
        "HH:MM bold prefix missing; got: {content}"
    );
}

// ── Acceptance criterion 2: auto-created frontmatter ─────────────────────────

#[test]
fn auto_created_journal_has_required_frontmatter_fields() {
    let tmp = fresh_wiki();
    let d = date!(2026 - 04 - 25);
    append_capture(tmp.path(), d, time!(09:00), "first capture", &[], None).unwrap();

    let path = tmp.path().join("wiki/_journal/2026-04-25.md");
    let content = fs::read_to_string(&path).unwrap();

    // Frontmatter delimiters
    assert!(
        content.starts_with("---\n"),
        "page must start with frontmatter; got: {content}"
    );

    // title: "2026-04-25"
    assert!(
        content.contains(r#"title: "2026-04-25""#) || content.contains("title: 2026-04-25"),
        "frontmatter title missing/malformed: {content}"
    );

    // tags: [journal]
    assert!(
        content.contains("tags: [journal]") || content.contains("- journal"),
        "frontmatter tags must include `journal`: {content}"
    );

    // created: 2026-04-25
    assert!(
        content.contains("created: 2026-04-25"),
        "frontmatter must include `created: 2026-04-25`: {content}"
    );

    // The body must include the ## Captures heading so the page has structure.
    assert!(
        content.contains("## Captures"),
        "auto-created page must include `## Captures` heading: {content}"
    );
}

// ── Subsequent captures append (do NOT overwrite) ─────────────────────────────

#[test]
fn second_capture_same_day_appends_does_not_overwrite() {
    let tmp = fresh_wiki();
    let d = date!(2026 - 04 - 25);

    let r1 = append_capture(tmp.path(), d, time!(10:23), "first thought", &[], None).unwrap();
    assert!(r1.created);

    let r2 = append_capture(tmp.path(), d, time!(10:25), "second thought", &[], None).unwrap();
    assert!(
        !r2.created,
        "second capture must NOT mark file as newly created"
    );

    let content = fs::read_to_string(&r2.path).unwrap();
    assert!(content.contains("**10:23** first thought"));
    assert!(content.contains("**10:25** second thought"));
    // first line must still come before second
    let first = content.find("first thought").unwrap();
    let second = content.find("second thought").unwrap();
    assert!(
        first < second,
        "captures must remain in chronological order"
    );
}

// ── Acceptance criterion 4: tags + source flags ──────────────────────────────

#[test]
fn capture_renders_tags_after_content() {
    let tmp = fresh_wiki();
    let d = date!(2026 - 04 - 25);
    let tags = vec!["rust".to_string(), "markdown".to_string()];

    let r = append_capture(
        tmp.path(),
        d,
        time!(10:25),
        "see docs.rs/comrak",
        &tags,
        None,
    )
    .unwrap();
    let content = fs::read_to_string(&r.path).unwrap();
    assert!(
        content.contains("**10:25** see docs.rs/comrak `#rust` `#markdown`"),
        "tag suffixes missing or wrong format; content: {content}"
    );
}

#[test]
fn capture_renders_source_link_at_end() {
    let tmp = fresh_wiki();
    let d = date!(2026 - 04 - 25);

    let r = append_capture(
        tmp.path(),
        d,
        time!(10:30),
        "key insight",
        &[],
        Some("https://example.com/article"),
    )
    .unwrap();
    let content = fs::read_to_string(&r.path).unwrap();
    assert!(
        content.contains("**10:30** key insight ([source](https://example.com/article))"),
        "source link missing or wrong format; content: {content}"
    );
}

#[test]
fn capture_renders_tags_and_source_together() {
    let tmp = fresh_wiki();
    let d = date!(2026 - 04 - 25);
    let tags = vec!["rust".to_string()];

    let r = append_capture(
        tmp.path(),
        d,
        time!(11:00),
        "combined",
        &tags,
        Some("https://example.com"),
    )
    .unwrap();
    let content = fs::read_to_string(&r.path).unwrap();
    assert!(
        content.contains("**11:00** combined `#rust` ([source](https://example.com))"),
        "tags+source combo missing/wrong; content: {content}"
    );
}

// ── Empty content rejected ───────────────────────────────────────────────────

#[test]
fn empty_content_returns_error() {
    let tmp = fresh_wiki();
    let err = append_capture(
        tmp.path(),
        date!(2026 - 04 - 25),
        time!(10:00),
        "   ",
        &[],
        None,
    )
    .expect_err("empty content must be rejected");
    matches!(err, WikiError::Internal(_) | WikiError::Io(_));
}

// ── Multi-day routing ────────────────────────────────────────────────────────

#[test]
fn captures_on_different_days_route_to_separate_files() {
    let tmp = fresh_wiki();
    append_capture(
        tmp.path(),
        date!(2026 - 04 - 24),
        time!(23:00),
        "yesterday",
        &[],
        None,
    )
    .unwrap();
    append_capture(
        tmp.path(),
        date!(2026 - 04 - 25),
        time!(00:01),
        "today",
        &[],
        None,
    )
    .unwrap();

    assert!(tmp.path().join("wiki/_journal/2026-04-24.md").exists());
    assert!(tmp.path().join("wiki/_journal/2026-04-25.md").exists());

    let yesterday = fs::read_to_string(tmp.path().join("wiki/_journal/2026-04-24.md")).unwrap();
    assert!(yesterday.contains("yesterday"));
    assert!(!yesterday.contains("today"));

    let today = fs::read_to_string(tmp.path().join("wiki/_journal/2026-04-25.md")).unwrap();
    assert!(today.contains("today"));
    assert!(!today.contains("yesterday"));
}

// ── Special chars / very long content survive round-trip ─────────────────────

#[test]
fn capture_with_special_chars_round_trips() {
    let tmp = fresh_wiki();
    let weird = r#"line with 中文, emoji 🚀, and "quotes" + (parens)"#;
    let r = append_capture(
        tmp.path(),
        date!(2026 - 04 - 25),
        time!(12:00),
        weird,
        &[],
        None,
    )
    .unwrap();
    let content = fs::read_to_string(&r.path).unwrap();
    assert!(
        content.contains(weird),
        "special chars must survive verbatim; content: {content}"
    );
}

#[test]
fn capture_with_long_content_persists_full_text() {
    let tmp = fresh_wiki();
    let long: String = "abcde ".repeat(500);
    let r = append_capture(
        tmp.path(),
        date!(2026 - 04 - 25),
        time!(13:00),
        &long,
        &[],
        None,
    )
    .unwrap();
    let content = fs::read_to_string(&r.path).unwrap();
    assert!(content.contains(long.trim()));
}

// ── Pure-formatting helpers ──────────────────────────────────────────────────

#[test]
fn format_date_iso_pads_month_and_day() {
    assert_eq!(format_date_iso(date!(2026 - 01 - 02)), "2026-01-02");
    assert_eq!(format_date_iso(date!(2026 - 12 - 31)), "2026-12-31");
}

#[test]
fn format_time_hm_pads_to_two_digits() {
    assert_eq!(format_time_hm(time!(09:05)), "09:05");
    assert_eq!(format_time_hm(time!(23:59)), "23:59");
    assert_eq!(format_time_hm(time!(00:00)), "00:00");
}

#[test]
fn journal_path_for_date_lives_under_wiki_journal_dir() {
    let p = journal_path_for_date(Path::new("/tmp/v"), date!(2026 - 04 - 25));
    assert_eq!(p, Path::new("/tmp/v/wiki/_journal/2026-04-25.md"));
    assert!(p.to_string_lossy().contains(JOURNAL_DIR));
}

#[test]
fn format_capture_line_strips_leading_hash_on_tags() {
    // Caller may pass `rust` or `#rust`; either way the rendered tag is `\`#rust\``.
    let line = format_capture_line(
        time!(10:00),
        "x",
        &["#rust".to_string(), "markdown".to_string()],
        None,
    );
    assert!(
        line.ends_with("`#rust` `#markdown`"),
        "expected normalized tag suffix, got: {line}"
    );
}

// ── Acceptance criterion 7: stale journal lint ───────────────────────────────

#[test]
fn find_stale_captures_returns_empty_when_journal_dir_missing() {
    let tmp = fresh_wiki();
    // Default schema scaffolds wiki/_journal/ but no captures yet — even
    // without the dir, find_stale_captures must not error.
    let v = find_stale_captures(tmp.path(), DEFAULT_STALE_AFTER_DAYS).unwrap();
    assert!(v.is_empty());
}

#[test]
fn find_stale_captures_flags_old_journal_pages_via_git_age() {
    // Stand up a wiki inside a git repo, commit a journal page with an
    // antedated commit, then assert it's flagged stale.
    let tmp = fresh_wiki();
    init_repo(tmp.path());
    // Seed with the .lw scaffold so HEAD exists.
    Command::new("git")
        .args(["add", "."])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    // Append a capture for today, then commit it with a backdated date.
    let today = date!(2026 - 04 - 25);
    append_capture(tmp.path(), today, time!(10:00), "old capture", &[], None).unwrap();
    Command::new("git")
        .args(["add", "wiki/_journal"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    // Commit with date 30 days in the past.
    let backdate = "30 days ago";
    let out = Command::new("git")
        .args(["commit", "-m", "backdate"])
        .env("GIT_AUTHOR_DATE", backdate)
        .env("GIT_COMMITTER_DATE", backdate)
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "backdated commit failed: {out:?}");

    let stale = find_stale_captures(tmp.path(), 7).unwrap();
    assert!(
        !stale.is_empty(),
        "30-day-old capture must be flagged when threshold=7"
    );
    assert!(
        stale[0].path.contains("_journal"),
        "stale finding path must contain _journal: {:?}",
        stale[0].path
    );
    assert!(
        stale[0].age_days >= 7,
        "stale.age_days must be > threshold; got {}",
        stale[0].age_days
    );
}

#[test]
fn find_stale_captures_skips_recent_pages() {
    let tmp = fresh_wiki();
    init_repo(tmp.path());
    Command::new("git")
        .args(["add", "."])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    append_capture(
        tmp.path(),
        date!(2026 - 04 - 25),
        time!(10:00),
        "recent",
        &[],
        None,
    )
    .unwrap();
    Command::new("git")
        .args(["add", "wiki/_journal"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    // Commit with current time — should NOT be flagged.
    let out = Command::new("git")
        .args(["commit", "-m", "now"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let stale = find_stale_captures(tmp.path(), 7).unwrap();
    assert!(
        stale.is_empty(),
        "fresh capture must not be flagged stale; got: {stale:?}"
    );
}
