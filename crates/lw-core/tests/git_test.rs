use lw_core::git::{FreshnessLevel, compute_freshness};

#[test]
fn fast_decay_stale_after_30() {
    assert_eq!(compute_freshness("fast", 31, 90), FreshnessLevel::Stale);
    assert_eq!(compute_freshness("fast", 10, 90), FreshnessLevel::Fresh);
    assert_eq!(compute_freshness("fast", 25, 90), FreshnessLevel::Suspect);
}

#[test]
fn normal_decay_uses_default_days() {
    assert_eq!(compute_freshness("normal", 91, 90), FreshnessLevel::Stale);
    assert_eq!(compute_freshness("normal", 30, 90), FreshnessLevel::Fresh);
    assert_eq!(compute_freshness("normal", 70, 90), FreshnessLevel::Suspect);
}

#[test]
fn evergreen_never_stale() {
    assert_eq!(
        compute_freshness("evergreen", 9999, 90),
        FreshnessLevel::Fresh
    );
}

#[test]
fn unknown_decay_defaults_to_normal() {
    assert_eq!(compute_freshness("unknown", 91, 90), FreshnessLevel::Stale);
}
