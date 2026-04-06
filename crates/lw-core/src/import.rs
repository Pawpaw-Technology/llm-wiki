use crate::page::Page;
use crate::{Result, WikiError};
use serde::Deserialize;

#[derive(Debug)]
pub struct ImportedPage {
    pub title: String,
    pub slug: String,
    pub page: Page,
    pub source_id: String,
}

#[derive(Deserialize)]
struct Tweet {
    id: String,
    full_text: String,
    screen_name: String,
    name: String,
    created_at: String,
    url: String,
    favorite_count: u64,
    bookmark_count: u64,
    views_count: u64,
    #[allow(dead_code)]
    retweet_count: u64,
    #[allow(dead_code)]
    quote_count: u64,
    #[allow(dead_code)]
    reply_count: u64,
}

/// Parse a Twitter JSON export into ImportedPage entries.
pub fn parse_twitter_json(json_str: &str, limit: Option<usize>) -> Result<Vec<ImportedPage>> {
    let tweets: Vec<Tweet> = serde_json::from_str(json_str)
        .map_err(|e| WikiError::YamlParse(format!("Invalid Twitter JSON: {e}")))?;

    let mut pages = Vec::new();
    for tweet in &tweets {
        if let Some(max) = limit {
            if pages.len() >= max {
                break;
            }
        }

        if tweet.full_text.len() < 20 {
            continue;
        }

        let title = tweet
            .full_text
            .chars()
            .take(60)
            .collect::<String>()
            .replace('\n', " ");
        let title = title.trim().to_string();

        let slug = slugify_title(&title);

        let engagement = format!(
            "likes:{} bookmarks:{} views:{}",
            tweet.favorite_count, tweet.bookmark_count, tweet.views_count
        );

        let body = format!(
            "{}\n\n---\nSource: {}\nAuthor: @{} ({})\nDate: {}\nEngagement: {}\n",
            tweet.full_text, tweet.url, tweet.screen_name, tweet.name, tweet.created_at, engagement
        );

        let page = Page {
            title: title.clone(),
            tags: vec![],
            decay: Some("fast".to_string()),
            sources: vec!["twitter-import".to_string()],
            author: Some(tweet.screen_name.clone()),
            generator: Some("lw-import".to_string()),
            body: body.clone(),
        };

        pages.push(ImportedPage {
            title,
            slug: if slug.is_empty() {
                format!("tweet-{}", tweet.id)
            } else {
                slug
            },
            page,
            source_id: tweet.id.clone(),
        });
    }

    Ok(pages)
}

fn slugify_title(title: &str) -> String {
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
