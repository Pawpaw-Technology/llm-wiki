use lw_core::schema::WikiSchema;

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
