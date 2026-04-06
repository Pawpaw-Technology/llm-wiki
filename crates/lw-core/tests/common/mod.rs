//! Shared test harness for lw-core integration tests.
//!
//! `TestWiki` owns a `TempDir` and provides helpers to set up an isolated wiki
//! environment. Every test gets its own filesystem — no shared state, safe for
//! `--test-threads=N` with any N.

use lw_core::fs::{init_wiki, write_page};
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use lw_core::search::TantivySearcher;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// An isolated wiki environment backed by a temporary directory.
/// Dropped automatically at end of test scope.
pub struct TestWiki {
    _tmp: TempDir,
    root: PathBuf,
    #[allow(dead_code)]
    schema: WikiSchema,
}

impl TestWiki {
    /// Create a bare, initialized wiki with default schema.
    pub fn new() -> Self {
        Self::with_schema(WikiSchema::default())
    }

    /// Create with a custom schema.
    pub fn with_schema(schema: WikiSchema) -> Self {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let root = tmp.path().to_path_buf();
        init_wiki(&root, &schema).expect("failed to init wiki");
        Self {
            _tmp: tmp,
            root,
            schema,
        }
    }

    /// Wiki root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path to the wiki/ subdirectory.
    pub fn wiki_dir(&self) -> PathBuf {
        self.root.join("wiki")
    }

    /// The schema this wiki was initialized with.
    #[allow(dead_code)]
    pub fn schema(&self) -> &WikiSchema {
        &self.schema
    }

    /// Write a page into the wiki. `rel_path` is relative to wiki/, e.g.
    /// `"architecture/transformer.md"`.
    pub fn write_page(&self, rel_path: &str, page: &Page) {
        let abs = self.root.join("wiki").join(rel_path);
        write_page(&abs, page).expect("failed to write page");
    }

    /// Write the canonical 5-page sample set. Returns the (rel_path, Page) pairs.
    pub fn with_sample_pages(&self) -> Vec<(String, Page)> {
        let pages = sample_pages();
        for (rel, page) in &pages {
            self.write_page(rel, page);
        }
        pages
    }

    /// Create a TantivySearcher with its index isolated inside this wiki's temp dir.
    pub fn searcher(&self) -> TantivySearcher {
        let index_dir = self.root.join(lw_core::INDEX_DIR);
        std::fs::create_dir_all(&index_dir).expect("failed to create index dir");
        TantivySearcher::new(&index_dir).expect("failed to create searcher")
    }

    /// Create a file at an arbitrary path under the temp root (useful for ingest sources).
    /// Returns the absolute path.
    pub fn create_file(&self, rel_path: &str, content: &str) -> PathBuf {
        let abs = self.root.join(rel_path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).expect("failed to create parent dirs");
        }
        std::fs::write(&abs, content).expect("failed to write file");
        abs
    }
}

/// Build a Page with the given metadata.
pub fn make_page(title: &str, tags: &[&str], decay: &str, body: &str) -> Page {
    Page {
        title: title.to_string(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        decay: Some(decay.to_string()),
        sources: vec![],
        author: None,
        generator: None,
        body: body.to_string(),
    }
}

/// The canonical 5-page sample set for integration tests.
pub fn sample_pages() -> Vec<(String, Page)> {
    vec![
        (
            "architecture/transformer-architecture.md".into(),
            make_page(
                "Transformer Architecture",
                &["architecture", "attention", "deep-learning"],
                "evergreen",
                "The Transformer architecture uses multi-head self-attention \
                 layers interleaved with feed-forward networks. Key innovations: \
                 scaled dot-product attention, positional encoding, layer normalization.",
            ),
        ),
        (
            "architecture/flash-attention-2.md".into(),
            make_page(
                "Flash Attention 2",
                &["architecture", "attention", "optimization"],
                "normal",
                "Flash Attention 2 reduces memory from O(N^2) to O(N) with 2-4x speedup \
                 by tiling attention computation in SRAM instead of materializing the full matrix.",
            ),
        ),
        (
            "training/rlhf-training.md".into(),
            make_page(
                "RLHF Training",
                &["training", "rlhf", "alignment"],
                "normal",
                "RLHF aligns LLMs with human preferences via three stages: SFT, reward model, \
                 PPO. Builds on [[transformer-architecture]]. Uses [[flash-attention-2]] for speed.",
            ),
        ),
        (
            "tools/pytorch.md".into(),
            make_page(
                "PyTorch",
                &["tools", "pytorch", "framework"],
                "fast",
                "PyTorch is the dominant DL framework. Features: torch.distributed, FSDP, \
                 torch.compile, native mixed-precision (bf16, fp16).",
            ),
        ),
        (
            "training/lora-fine-tuning.md".into(),
            make_page(
                "LoRA Fine-tuning",
                &["training", "finetuning", "efficiency"],
                "normal",
                "LoRA injects trainable low-rank matrices into frozen model layers, \
                 reducing parameters by 10,000x. Variants: QLoRA, DoRA, AdaLoRA.",
            ),
        ),
    ]
}
