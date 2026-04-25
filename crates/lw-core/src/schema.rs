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
    /// Optional `[journal]` block. Configures how `lw lint` flags
    /// unprocessed journal entries (issue #37).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal: Option<JournalConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalConfig {
    /// Threshold (in days, since the journal page's last git commit) above
    /// which `lw lint` flags the page as an unprocessed capture. When the
    /// `[journal]` block is missing, the default is 7.
    #[serde(default)]
    pub stale_after_days: Option<u32>,
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

    /// Effective `stale_after_days` for journal pages. Falls back to
    /// [`crate::journal::DEFAULT_STALE_AFTER_DAYS`] when the schema does not
    /// configure `[journal] stale_after_days = N`.
    pub fn journal_stale_after_days(&self) -> u32 {
        self.journal
            .as_ref()
            .and_then(|j| j.stale_after_days)
            .unwrap_or(crate::journal::DEFAULT_STALE_AFTER_DAYS)
    }
}

impl Default for WikiSchema {
    fn default() -> Self {
        let mut categories: HashMap<String, CategoryConfig> = HashMap::new();
        categories.insert(
            "_journal".to_string(),
            CategoryConfig {
                review_days: None,
                // Captures must be friction-free — no required_fields.
                required_fields: Vec::new(),
                template: "## Captures\n".to_string(),
            },
        );
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
                    "_journal".into(),
                ],
                decay_defaults: HashMap::from([
                    ("product".into(), "fast".into()),
                    ("architecture".into(), "normal".into()),
                    ("training".into(), "normal".into()),
                    ("infra".into(), "normal".into()),
                    ("tools".into(), "fast".into()),
                    ("ops".into(), "normal".into()),
                    // Journal pages are write-once, never go stale via decay.
                    ("_journal".into(), "evergreen".into()),
                ]),
            },
            categories,
            journal: Some(JournalConfig {
                stale_after_days: Some(crate::journal::DEFAULT_STALE_AFTER_DAYS),
            }),
        }
    }
}
