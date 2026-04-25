use lw_core::schema::{CategoryConfig, WikiSchema};

#[test]
fn parse_full_schema() {
    let toml_str = r#"
[wiki]
name = "Acme AI Team Wiki"
default_review_days = 90

[tags]
categories = ["architecture", "training", "infra", "tools", "product", "ops"]

[tags.decay_defaults]
product = "fast"
architecture = "normal"
training = "normal"
infra = "normal"
tools = "fast"
ops = "normal"
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();
    assert_eq!(schema.wiki.name, "Acme AI Team Wiki");
    assert_eq!(schema.wiki.default_review_days, 90);
    assert_eq!(schema.tags.categories.len(), 6);
    assert_eq!(schema.tags.decay_defaults.get("product").unwrap(), "fast");
}

#[test]
fn default_schema_is_valid() {
    let schema = WikiSchema::default();
    assert_eq!(schema.wiki.default_review_days, 90);
    assert!(!schema.tags.categories.is_empty());
    let toml_str = schema.to_toml();
    let reparsed = WikiSchema::parse(&toml_str).unwrap();
    assert_eq!(schema.wiki.name, reparsed.wiki.name);
    assert_eq!(schema.tags.categories, reparsed.tags.categories);
}

#[test]
fn decay_for_category() {
    let schema = WikiSchema::default();
    assert_eq!(schema.decay_for_category("product"), "fast");
    assert_eq!(schema.decay_for_category("architecture"), "normal");
    assert_eq!(schema.decay_for_category("unknown"), "normal");
}

// ── Per-category templates (issue #58) ────────────────────────────────────────

/// Acceptance criterion 1: parse [categories.<name>] blocks with all three fields.
#[test]
fn parse_category_blocks_all_fields() {
    let toml_str = r#"
[wiki]
name = "Test Wiki"
default_review_days = 90

[tags]
categories = ["tools", "concepts"]

[categories.tools]
review_days = 90
required_fields = ["title", "tags"]
template = """
## Overview

## Usage

## See Also
"""

[categories.concepts]
review_days = 180
required_fields = ["title", "tags", "aliases"]
template = """
## Definition

## Key Properties

## Examples

## Related Concepts
"""
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();

    // tools category
    let tools = schema.categories.get("tools").unwrap();
    assert_eq!(tools.review_days, Some(90));
    assert_eq!(tools.required_fields, vec!["title", "tags"]);
    assert!(tools.template.contains("## Overview"));
    assert!(tools.template.contains("## Usage"));
    assert!(tools.template.contains("## See Also"));

    // concepts category
    let concepts = schema.categories.get("concepts").unwrap();
    assert_eq!(concepts.review_days, Some(180));
    assert_eq!(concepts.required_fields, vec!["title", "tags", "aliases"]);
    assert!(concepts.template.contains("## Definition"));
    assert!(concepts.template.contains("## Key Properties"));
    assert!(concepts.template.contains("## Examples"));
    assert!(concepts.template.contains("## Related Concepts"));
}

/// Acceptance criterion 2: schema without any [categories] block still parses (backward compat).
#[test]
fn parse_schema_without_categories_block() {
    let toml_str = r#"
[wiki]
name = "Backward Compat Wiki"
default_review_days = 60

[tags]
categories = ["architecture", "infra"]
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();
    assert!(
        schema.categories.is_empty(),
        "categories map should be empty when [categories] is absent"
    );
}

/// Acceptance criterion 3: round-trip — deserialize → serialize → deserialize preserves categories.
#[test]
fn round_trip_preserves_categories() {
    let toml_str = r#"
[wiki]
name = "Round-trip Wiki"
default_review_days = 90

[tags]
categories = ["tools"]

[categories.tools]
review_days = 90
required_fields = ["title", "tags"]
template = """
## Overview

## Usage
"""
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();
    let serialized = schema.to_toml();
    let reparsed = WikiSchema::parse(&serialized).unwrap();

    assert_eq!(reparsed.categories.len(), schema.categories.len());
    let orig_tools = schema.categories.get("tools").unwrap();
    let reparsed_tools = reparsed.categories.get("tools").unwrap();
    assert_eq!(reparsed_tools.review_days, orig_tools.review_days);
    assert_eq!(reparsed_tools.required_fields, orig_tools.required_fields);
    assert_eq!(reparsed_tools.template, orig_tools.template);
}

/// Acceptance criterion 4: category_config helper returns Some for known, None for unknown.
#[test]
fn category_config_helper() {
    let toml_str = r#"
[wiki]
name = "Helper Wiki"
default_review_days = 90

[tags]
categories = ["tools"]

[categories.tools]
review_days = 45
required_fields = ["title"]
template = ""
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();

    let cfg: Option<&CategoryConfig> = schema.category_config("tools");
    assert!(cfg.is_some(), "known category should return Some");
    assert_eq!(cfg.unwrap().review_days, Some(45));

    let missing = schema.category_config("nonexistent");
    assert!(missing.is_none(), "unknown category should return None");
}

/// Edge case: CategoryConfig with review_days absent (optional field).
#[test]
fn category_config_review_days_optional() {
    let toml_str = r#"
[wiki]
name = "No-ReviewDays Wiki"
default_review_days = 90

[tags]
categories = ["notes"]

[categories.notes]
required_fields = []
template = ""
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();
    let notes = schema.categories.get("notes").unwrap();
    assert!(
        notes.review_days.is_none(),
        "review_days should be None when absent"
    );
    assert!(notes.required_fields.is_empty());
}

/// Edge case: CategoryConfig with empty required_fields (default).
#[test]
fn category_config_empty_required_fields_default() {
    let toml_str = r#"
[wiki]
name = "Minimal Category Wiki"
default_review_days = 90

[tags]
categories = ["misc"]

[categories.misc]
review_days = 30
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();
    let misc = schema.categories.get("misc").unwrap();
    assert!(
        misc.required_fields.is_empty(),
        "required_fields should default to empty vec"
    );
    assert!(
        misc.template.is_empty(),
        "template should default to empty string"
    );
}
