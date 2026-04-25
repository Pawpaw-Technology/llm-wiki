//! Journal-first quick capture (issue #37) — STUB.
//!
//! Real implementation lands in the GREEN step. This stub keeps the crate
//! compiling so the failing tests have something to call.

use crate::Result;
use std::path::{Path, PathBuf};
use time::{Date, OffsetDateTime, Time};

pub const JOURNAL_DIR: &str = "_journal";
pub const DEFAULT_STALE_AFTER_DAYS: u32 = 7;

#[derive(Debug)]
pub struct CaptureAppend {
    pub path: PathBuf,
    pub created: bool,
    pub line: String,
    pub display_path: String,
}

#[derive(Debug, Clone)]
pub struct StaleJournalFinding {
    pub path: String,
    pub age_days: i64,
}

pub fn append_capture(
    _wiki_root: &Path,
    _date: Date,
    _time: Time,
    _content: &str,
    _tags: &[String],
    _source: Option<&str>,
) -> Result<CaptureAppend> {
    unimplemented!("journal::append_capture not yet implemented (#37)")
}

pub fn format_date_iso(_date: Date) -> String {
    unimplemented!()
}

pub fn format_time_hm(_time: Time) -> String {
    unimplemented!()
}

pub fn journal_path_for_date(_wiki_root: &Path, _date: Date) -> PathBuf {
    unimplemented!()
}

pub fn format_capture_line(
    _time: Time,
    _content: &str,
    _tags: &[String],
    _source: Option<&str>,
) -> String {
    unimplemented!()
}

pub fn local_now() -> OffsetDateTime {
    unimplemented!()
}

pub fn find_stale_captures(
    _wiki_root: &Path,
    _threshold_days: u32,
) -> Result<Vec<StaleJournalFinding>> {
    unimplemented!()
}
