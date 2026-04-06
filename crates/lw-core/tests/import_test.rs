use lw_core::import::{ImportedPage, parse_twitter_json};

#[test]
fn parse_twitter_json_basic() {
    let json = r#"[
        {
            "id": "123",
            "full_text": "This is a test tweet about AI agents and their capabilities.",
            "screen_name": "testuser",
            "name": "Test User",
            "created_at": "2025-12-05 13:24:28",
            "url": "https://twitter.com/testuser/status/123",
            "favorite_count": 100,
            "bookmark_count": 50,
            "views_count": 5000,
            "retweet_count": 10,
            "quote_count": 5,
            "reply_count": 3
        }
    ]"#;
    let pages = parse_twitter_json(json, None).unwrap();
    assert_eq!(pages.len(), 1);
    assert!(!pages[0].title.is_empty());
    assert!(pages[0].page.body.contains("AI agents"));
    assert_eq!(pages[0].source_id, "123");
}

#[test]
fn parse_twitter_json_with_limit() {
    let json = r#"[
        {"id":"1","full_text":"Tweet one about testing software.","screen_name":"a","name":"A","created_at":"2025-01-01 00:00:00","url":"","favorite_count":0,"bookmark_count":0,"views_count":0,"retweet_count":0,"quote_count":0,"reply_count":0},
        {"id":"2","full_text":"Tweet two about coding in Rust.","screen_name":"b","name":"B","created_at":"2025-01-02 00:00:00","url":"","favorite_count":0,"bookmark_count":0,"views_count":0,"retweet_count":0,"quote_count":0,"reply_count":0},
        {"id":"3","full_text":"Tweet three about system design.","screen_name":"c","name":"C","created_at":"2025-01-03 00:00:00","url":"","favorite_count":0,"bookmark_count":0,"views_count":0,"retweet_count":0,"quote_count":0,"reply_count":0}
    ]"#;
    let pages = parse_twitter_json(json, Some(2)).unwrap();
    assert_eq!(pages.len(), 2);
}

#[test]
fn parse_twitter_json_skips_short_tweets() {
    let json = r#"[
        {"id":"1","full_text":"Too short","screen_name":"a","name":"A","created_at":"2025-01-01 00:00:00","url":"","favorite_count":0,"bookmark_count":0,"views_count":0,"retweet_count":0,"quote_count":0,"reply_count":0},
        {"id":"2","full_text":"This tweet is long enough to be meaningful content for a wiki page.","screen_name":"b","name":"B","created_at":"2025-01-02 00:00:00","url":"","favorite_count":0,"bookmark_count":0,"views_count":0,"retweet_count":0,"quote_count":0,"reply_count":0}
    ]"#;
    let pages = parse_twitter_json(json, None).unwrap();
    assert_eq!(pages.len(), 1); // short tweet skipped
}

#[test]
fn parse_twitter_json_preserves_engagement_data() {
    let json = r#"[
        {
            "id": "456",
            "full_text": "A substantive tweet about transformer architecture and self-attention mechanisms.",
            "screen_name": "mlresearcher",
            "name": "ML Researcher",
            "created_at": "2025-06-15 09:30:00",
            "url": "https://twitter.com/mlresearcher/status/456",
            "favorite_count": 250,
            "bookmark_count": 120,
            "views_count": 15000,
            "retweet_count": 45,
            "quote_count": 12,
            "reply_count": 8
        }
    ]"#;
    let pages = parse_twitter_json(json, None).unwrap();
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    assert!(page.page.body.contains("likes:250"));
    assert!(page.page.body.contains("bookmarks:120"));
    assert!(page.page.body.contains("views:15000"));
    assert!(page.page.body.contains("@mlresearcher"));
}

#[test]
fn parse_twitter_json_generates_slug() {
    let json = r#"[
        {
            "id": "789",
            "full_text": "Understanding the Role of Attention in Neural Networks for NLP tasks.",
            "screen_name": "airesearch",
            "name": "AI Research",
            "created_at": "2025-03-20 14:00:00",
            "url": "",
            "favorite_count": 0,
            "bookmark_count": 0,
            "views_count": 0,
            "retweet_count": 0,
            "quote_count": 0,
            "reply_count": 0
        }
    ]"#;
    let pages = parse_twitter_json(json, None).unwrap();
    assert_eq!(pages.len(), 1);
    assert!(!pages[0].slug.is_empty());
    // Slug should be lowercase and hyphen-separated
    assert!(
        pages[0]
            .slug
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-')
    );
}

#[test]
fn parse_twitter_json_page_has_correct_metadata() {
    let json = r#"[
        {
            "id": "999",
            "full_text": "Exploring the intersection of reinforcement learning and language models.",
            "screen_name": "deeplearner",
            "name": "Deep Learner",
            "created_at": "2025-08-10 16:45:00",
            "url": "https://twitter.com/deeplearner/status/999",
            "favorite_count": 50,
            "bookmark_count": 25,
            "views_count": 3000,
            "retweet_count": 8,
            "quote_count": 2,
            "reply_count": 1
        }
    ]"#;
    let pages = parse_twitter_json(json, None).unwrap();
    let page = &pages[0].page;
    assert!(page.tags.is_empty()); // classification is agent's job
    assert_eq!(page.decay.as_deref(), Some("fast"));
    assert!(page.sources.contains(&"twitter-import".to_string()));
    assert_eq!(page.author.as_deref(), Some("deeplearner"));
    assert_eq!(page.generator.as_deref(), Some("lw-import"));
}

#[test]
fn parse_twitter_json_empty_array() {
    let json = "[]";
    let pages = parse_twitter_json(json, None).unwrap();
    assert!(pages.is_empty());
}

#[test]
fn parse_twitter_json_invalid_json() {
    let result = parse_twitter_json("not json at all", None);
    assert!(result.is_err());
}
