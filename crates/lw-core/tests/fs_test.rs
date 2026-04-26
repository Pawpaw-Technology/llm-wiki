use lw_core::fs::{
    atomic_write, category_from_path, discover_wiki_root, init_wiki, list_pages, read_page,
    write_page,
};
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn init_creates_structure() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();
    assert!(root.join(".lw/schema.toml").exists());
    assert!(root.join("wiki/architecture").is_dir());
    assert!(root.join("wiki/training").is_dir());
    assert!(root.join("wiki/_uncategorized").is_dir());
    assert!(root.join("raw/papers").is_dir());
    assert!(root.join("raw/articles").is_dir());
    assert!(root.join("raw/assets").is_dir());
}

#[test]
fn write_and_read_page() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();
    let page = Page {
        title: "Test Page".to_string(),
        tags: vec!["architecture".to_string()],
        decay: None,
        sources: vec![],
        author: Some("alice".to_string()),
        generator: None,
        related: None,
        status: None,
        aliases: vec![],
        body: "Hello world.\n".to_string(),
    };
    let path = root.join("wiki/architecture/test-page.md");
    write_page(&path, &page).unwrap();
    assert!(path.exists());
    let loaded = read_page(&path).unwrap();
    assert_eq!(loaded.title, "Test Page");
    assert_eq!(loaded.body.trim(), "Hello world.");
}

#[test]
fn list_pages_finds_markdown() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();
    let p1 = Page {
        title: "A".into(),
        tags: vec![],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        status: None,
        aliases: vec![],
        body: "A.\n".into(),
    };
    let p2 = Page {
        title: "B".into(),
        tags: vec![],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        status: None,
        aliases: vec![],
        body: "B.\n".into(),
    };
    write_page(&root.join("wiki/architecture/a.md"), &p1).unwrap();
    write_page(&root.join("wiki/training/b.md"), &p2).unwrap();
    let pages = list_pages(&root.join("wiki")).unwrap();
    assert_eq!(pages.len(), 2);
}

#[cfg(unix)]
#[test]
fn list_pages_ignores_symlinked_files_and_directories() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    let wiki_dir = root.join("wiki");
    let real_page = wiki_dir.join("architecture/real.md");
    std::fs::write(&real_page, "---\ntitle: Real\n---\n\ninside\n").unwrap();

    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(
        outside.join("secret.md"),
        "---\ntitle: Secret\n---\n\noutside\n",
    )
    .unwrap();
    std::fs::create_dir_all(outside.join("collection")).unwrap();
    std::fs::write(
        outside.join("collection/nested.md"),
        "---\ntitle: Nested\n---\n\noutside\n",
    )
    .unwrap();

    symlink(
        outside.join("secret.md"),
        wiki_dir.join("architecture/secret.md"),
    )
    .unwrap();
    symlink(
        outside.join("collection"),
        wiki_dir.join("architecture/collection"),
    )
    .unwrap();

    let pages = list_pages(&wiki_dir).unwrap();

    assert_eq!(
        pages,
        vec![std::path::PathBuf::from("architecture/real.md")]
    );
}

#[test]
fn read_nonexistent_page_errors() {
    let result = read_page(Path::new("/nonexistent/page.md"));
    assert!(result.is_err());
}

#[test]
fn category_from_path_works() {
    let p = std::path::PathBuf::from("architecture/transformer.md");
    assert_eq!(category_from_path(&p), Some("architecture".to_string()));
    let p2 = std::path::PathBuf::from("test.md");
    assert_eq!(category_from_path(&p2), None);
}

#[test]
fn discover_wiki_root_from_subdir() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();
    // Should find wiki root from a subdirectory
    let subdir = root.join("wiki/architecture");
    let discovered = discover_wiki_root(&subdir).unwrap();
    assert_eq!(discovered, root.to_path_buf());
}

#[test]
fn discover_wiki_root_not_found() {
    let tmp = TempDir::new().unwrap();
    // No wiki initialized here
    assert!(discover_wiki_root(tmp.path()).is_none());
}

/// Test A: write_page leaves no *.tmp file behind in the page directory after a
/// successful write.
#[test]
fn write_page_leaves_no_tmp_file() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    let page = Page {
        title: "Atomic Test".to_string(),
        tags: vec![],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        status: None,
        aliases: vec![],
        body: "content\n".to_string(),
    };
    let path = root.join("wiki/architecture/atomic-test.md");
    write_page(&path, &page).unwrap();

    // The page must exist and be readable.
    assert!(path.exists());

    // No NamedTempFile leftovers should remain in the parent directory.
    // `NamedTempFile::new_in` defaults to `prefix = ".tmp"` with a random suffix
    // (e.g. ".tmpABCDEF"). Such dotfile names have no extension as Rust sees it,
    // so we must match on the file_name prefix instead of `Path::extension()`.
    let parent = path.parent().unwrap();
    let leftover_tmps: Vec<_> = std::fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp"))
        .collect();
    assert!(
        leftover_tmps.is_empty(),
        "write_page left behind tmp files: {leftover_tmps:?}"
    );
}

/// Test B (Unix-only): A pre-existing symlink at the destination page path must
/// NOT be followed — the victim file pointed to by the symlink must remain
/// intact. With the old `std::fs::write(page_path, body)` implementation the
/// kernel would follow the symlink and overwrite the victim. With the
/// rename(2)-based atomic_write the symlink directory entry is replaced with
/// the new regular file and the victim is left untouched.
#[cfg(unix)]
#[test]
fn write_page_does_not_follow_victim_symlink() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    let page_dir = root.join("wiki/architecture");
    let page_path = page_dir.join("test.md");

    // Create a victim file outside the wiki tree.
    let victim_path = tmp.path().join("victim.txt");
    std::fs::write(&victim_path, b"SECRET CONTENT").unwrap();

    // Plant the symlink AT the destination page path. With non-atomic
    // `std::fs::write(page_path, body)` this would follow the symlink and
    // clobber `victim.txt`. The rename-based atomic write must replace the
    // symlink entry rather than follow it.
    symlink(&victim_path, &page_path).unwrap();

    // Write the page — this must succeed without clobbering the victim.
    let page = Page {
        title: "Symlink Safe".to_string(),
        tags: vec![],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        status: None,
        aliases: vec![],
        body: "safe\n".to_string(),
    };
    write_page(&page_path, &page).unwrap();

    // The victim file must be untouched.
    let victim_contents = std::fs::read(&victim_path).unwrap();
    assert_eq!(
        victim_contents, b"SECRET CONTENT",
        "victim file was clobbered by write_page"
    );

    // The page itself should now be a regular file, not a symlink.
    let meta = std::fs::symlink_metadata(&page_path).unwrap();
    assert!(
        meta.file_type().is_file(),
        "page_path should be a regular file after atomic write, not a symlink"
    );
}

/// Test C: atomic_write (the low-level helper) round-trips bytes correctly.
#[test]
fn atomic_write_round_trips_bytes() {
    let tmp = TempDir::new().unwrap();
    let dest = tmp.path().join("output.md");
    let body = b"hello atomic world\n";
    atomic_write(&dest, body).unwrap();
    let got = std::fs::read(&dest).unwrap();
    assert_eq!(got, body);
}
