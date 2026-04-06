use std::path::Path;
use std::process::Command;

/// Get the age in days of a file based on its last git commit.
/// Returns None if not in a git repo or file has no git history.
#[tracing::instrument]
pub fn page_age_days(path: &Path) -> Option<i64> {
    let output = Command::new("git")
        .args([
            "log",
            "--follow",
            "-1",
            "--format=%aI",
            "--",
            path.to_str()?,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        tracing::debug!(path = %path.display(), "git log returned non-zero");
        return None;
    }

    let date_str = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if date_str.is_empty() {
        return None;
    }

    // Parse ISO 8601 date manually (avoid chrono dependency)
    // Format: 2026-04-06T12:00:00+08:00
    // We just need the date part for day-level granularity
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;

    // Parse the date using git's own timestamp conversion
    let git_ts = Command::new("git")
        .args([
            "log",
            "--follow",
            "-1",
            "--format=%at",
            "--",
            path.to_str()?,
        ])
        .output()
        .ok()?;

    if !git_ts.status.success() {
        tracing::debug!(path = %path.display(), "git log timestamp returned non-zero");
        return None;
    }

    let ts: i64 = String::from_utf8(git_ts.stdout).ok()?.trim().parse().ok()?;
    Some((now - ts) / 86400)
}

/// Freshness level of a wiki page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessLevel {
    Fresh,
    Suspect,
    Stale,
}

impl std::fmt::Display for FreshnessLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FreshnessLevel::Fresh => write!(f, "FRESH"),
            FreshnessLevel::Suspect => write!(f, "SUSPECT"),
            FreshnessLevel::Stale => write!(f, "STALE"),
        }
    }
}

/// Compute freshness from decay level and age.
/// - fast: stale after 30 days
/// - normal: stale after `default_days` (usually 90)
/// - evergreen: never stale by time
#[tracing::instrument]
pub fn compute_freshness(decay: &str, age_days: i64, default_days: u32) -> FreshnessLevel {
    let threshold = match decay {
        "fast" => 30,
        "evergreen" => return FreshnessLevel::Fresh,
        _ => default_days as i64,
    };

    if age_days > threshold {
        FreshnessLevel::Stale
    } else if age_days > threshold * 3 / 4 {
        FreshnessLevel::Suspect
    } else {
        FreshnessLevel::Fresh
    }
}
