use crate::{Result, WikiError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-category configuration block (`[categories.<name>]` in schema.toml).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategoryConfig {
    pub review_days: Option<u32>,
    #[serde(default)]
    pub required_fields: Vec<String>,
    #[serde(default)]
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSchema {
    pub wiki: WikiConfig,
    pub tags: TagsConfig,
    #[serde(default)]
    pub categories: HashMap<String, CategoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiConfig {
    pub name: String,
    pub default_review_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagsConfig {
    pub categories: Vec<String>,
    #[serde(default)]
    pub decay_defaults: HashMap<String, String>,
}

impl WikiSchema {
    pub fn parse(toml_str: &str) -> Result<Self> {
        toml::from_str(toml_str).map_err(WikiError::TomlParse)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("schema serialization should not fail")
    }

    pub fn decay_for_category(&self, category: &str) -> &str {
        self.tags
            .decay_defaults
            .get(category)
            .map(|s| s.as_str())
            .unwrap_or("normal")
    }

    pub fn category_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<String> = self.tags.categories.clone();
        dirs.push("_uncategorized".to_string());
        dirs
    }

    /// Return the [`CategoryConfig`] for a named category, or `None` if not configured.
    pub fn category_config(&self, name: &str) -> Option<&CategoryConfig> {
        self.categories.get(name)
    }
}

impl Default for WikiSchema {
    fn default() -> Self {
        Self {
            wiki: WikiConfig {
                name: "LLM Wiki".to_string(),
                default_review_days: 90,
            },
            tags: TagsConfig {
                categories: vec![
                    "architecture".into(),
                    "training".into(),
                    "infra".into(),
                    "tools".into(),
                    "product".into(),
                    "ops".into(),
                ],
                decay_defaults: HashMap::from([
                    ("product".into(), "fast".into()),
                    ("architecture".into(), "normal".into()),
                    ("training".into(), "normal".into()),
                    ("infra".into(), "normal".into()),
                    ("tools".into(), "fast".into()),
                    ("ops".into(), "normal".into()),
                ]),
            },
            categories: HashMap::new(),
        }
    }
}
