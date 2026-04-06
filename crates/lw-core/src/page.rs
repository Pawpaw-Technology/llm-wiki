use crate::{Result, WikiError};
use gray_matter::{Matter, ParsedEntity, engine::YAML};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub title: String,
    pub tags: Vec<String>,
    pub decay: Option<String>,
    pub sources: Vec<String>,
    pub author: Option<String>,
    pub generator: Option<String>,
    pub body: String,
}

impl Page {
    pub fn new(title: &str, tags: &[&str], body: &str) -> Self {
        Self {
            title: title.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            decay: None,
            sources: vec![],
            author: None,
            generator: None,
            body: body.to_string(),
        }
    }

    pub fn parse(markdown: &str) -> Result<Self> {
        let matter = Matter::<YAML>::new();
        let parsed: ParsedEntity = matter
            .parse(markdown)
            .map_err(|e| WikiError::YamlParse(e.to_string()))?;

        let yaml_str = parsed.matter.as_str();
        if yaml_str.is_empty() {
            return Err(WikiError::YamlParse("no frontmatter found".into()));
        }

        let fm: Frontmatter =
            serde_yml::from_str(yaml_str).map_err(|e| WikiError::YamlParse(e.to_string()))?;

        if fm.title.is_empty() {
            return Err(WikiError::YamlParse("title is required".into()));
        }

        Ok(Self {
            title: fm.title,
            tags: fm.tags,
            decay: fm.decay,
            sources: fm.sources,
            author: fm.author,
            generator: fm.generator,
            body: parsed.content,
        })
    }

    pub fn frontmatter(&self) -> Frontmatter {
        Frontmatter {
            title: self.title.clone(),
            tags: self.tags.clone(),
            decay: self.decay.clone(),
            sources: self.sources.clone(),
            author: self.author.clone(),
            generator: self.generator.clone(),
        }
    }

    pub fn to_markdown(&self) -> String {
        let yaml = serde_yml::to_string(&self.frontmatter())
            .expect("frontmatter serialization should not fail");
        format!("---\n{}---\n\n{}", yaml, self.body.trim_start())
    }
}

/// Convert a title to a URL-safe slug.
/// Preserves alphanumeric chars and CJK characters, replaces others with hyphens.
pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c > '\u{2E7F}' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-")
}
