# Changelog

## [0.4.0](https://github.com/Pawpaw-Technology/llm-wiki/compare/v0.3.0...v0.4.0) (2026-04-27)


### Features

* **core:** alias index foundation ([#100](https://github.com/Pawpaw-Technology/llm-wiki/issues/100)) ([#113](https://github.com/Pawpaw-Technology/llm-wiki/issues/113)) ([1a49971](https://github.com/Pawpaw-Technology/llm-wiki/commit/1a4997162d3bee3d58af62876b01e1b019ed9933))
* **core:** unlinked-mention matcher (closes [#101](https://github.com/Pawpaw-Technology/llm-wiki/issues/101)) ([#115](https://github.com/Pawpaw-Technology/llm-wiki/issues/115)) ([d6b2bb9](https://github.com/Pawpaw-Technology/llm-wiki/commit/d6b2bb9ae769e38a771da170df6262acb2d26e27))
* **lint:** unlinked-mentions rule (closes [#102](https://github.com/Pawpaw-Technology/llm-wiki/issues/102)) ([#116](https://github.com/Pawpaw-Technology/llm-wiki/issues/116)) ([35b6ffd](https://github.com/Pawpaw-Technology/llm-wiki/commit/35b6ffd70f8878432562c71894e1e63038563a38))
* **mcp:** unlinked_mentions in wiki_write/wiki_new responses (closes [#103](https://github.com/Pawpaw-Technology/llm-wiki/issues/103)) ([#117](https://github.com/Pawpaw-Technology/llm-wiki/issues/117)) ([0e3d1b4](https://github.com/Pawpaw-Technology/llm-wiki/commit/0e3d1b40177c0bf7cfaea7b3656d217363924262))


### Bug Fixes

* **lint:** zero-out freshness counts under --rule filter (closes [#118](https://github.com/Pawpaw-Technology/llm-wiki/issues/118)) ([#120](https://github.com/Pawpaw-Technology/llm-wiki/issues/120)) ([ef1958d](https://github.com/Pawpaw-Technology/llm-wiki/commit/ef1958d57513be650ba45a19e3288ddb812f30d8))
* **search:** cache writer in early-check to avoid LockFailure race ([#119](https://github.com/Pawpaw-Technology/llm-wiki/issues/119)) ([d6b4539](https://github.com/Pawpaw-Technology/llm-wiki/commit/d6b4539d96bd5177189b6422390029ae96c09a2e))

## [0.3.0](https://github.com/Pawpaw-Technology/llm-wiki/compare/v0.2.6...v0.3.0) (2026-04-26)


### Features

* **cli:** lw new subcommand (closes [#60](https://github.com/Pawpaw-Technology/llm-wiki/issues/60)) ([#84](https://github.com/Pawpaw-Technology/llm-wiki/issues/84)) ([0af1a03](https://github.com/Pawpaw-Technology/llm-wiki/commit/0af1a03e4c9ed0eb99d5591dd819a25f11b36014))
* **core,cli,mcp:** backlink index + wiki_backlinks tool (closes [#39](https://github.com/Pawpaw-Technology/llm-wiki/issues/39)) ([#94](https://github.com/Pawpaw-Technology/llm-wiki/issues/94)) ([30b5ebe](https://github.com/Pawpaw-Technology/llm-wiki/commit/30b5ebebb406829364d782c6b4a8c22c850e3d00))
* **core,cli,mcp:** frontmatter field queries (closes [#41](https://github.com/Pawpaw-Technology/llm-wiki/issues/41)) ([#95](https://github.com/Pawpaw-Technology/llm-wiki/issues/95)) ([fd119a3](https://github.com/Pawpaw-Technology/llm-wiki/commit/fd119a30ea926b3645d3efbf9f2a52d89acb5759))
* **core,cli,mcp:** git auto-commit integration (closes [#38](https://github.com/Pawpaw-Technology/llm-wiki/issues/38)) ([#90](https://github.com/Pawpaw-Technology/llm-wiki/issues/90)) ([daf28e7](https://github.com/Pawpaw-Technology/llm-wiki/commit/daf28e758423105104b6d2b24639f346c42fa99c))
* **core:** lw_core::new_page schema-enforced page creation (closes [#59](https://github.com/Pawpaw-Technology/llm-wiki/issues/59)) ([#83](https://github.com/Pawpaw-Technology/llm-wiki/issues/83)) ([6b79b6b](https://github.com/Pawpaw-Technology/llm-wiki/commit/6b79b6b9805cc7a102840ac7a2a3483e89a9f4b3))
* **mcp:** wiki_new tool (closes [#61](https://github.com/Pawpaw-Technology/llm-wiki/issues/61)) ([#86](https://github.com/Pawpaw-Technology/llm-wiki/issues/86)) ([51a1164](https://github.com/Pawpaw-Technology/llm-wiki/commit/51a1164d42ae6dea6ef6e0159046376399808aea))
* **schema:** per-category templates and required fields (closes [#58](https://github.com/Pawpaw-Technology/llm-wiki/issues/58)) ([#82](https://github.com/Pawpaw-Technology/llm-wiki/issues/82)) ([3189dca](https://github.com/Pawpaw-Technology/llm-wiki/commit/3189dca6b1acc558e6bfe6c13037d53e1a4d673c))
* **skills:** knowledge-capture skill (closes [#40](https://github.com/Pawpaw-Technology/llm-wiki/issues/40)) ([#89](https://github.com/Pawpaw-Technology/llm-wiki/issues/89)) ([d2dd13c](https://github.com/Pawpaw-Technology/llm-wiki/commit/d2dd13c57e0d193a350cbffcdd93ab99ce530794))
* **templates:** ship [categories.*] blocks for starter vaults (closes [#62](https://github.com/Pawpaw-Technology/llm-wiki/issues/62)) ([#85](https://github.com/Pawpaw-Technology/llm-wiki/issues/85)) ([315498d](https://github.com/Pawpaw-Technology/llm-wiki/commit/315498d7872e96b602a6e184ee003b19211e6864))


### Bug Fixes

* **ci:** switch release-please to release-type=simple for workspace inheritance ([#105](https://github.com/Pawpaw-Technology/llm-wiki/issues/105)) ([e4c359d](https://github.com/Pawpaw-Technology/llm-wiki/commit/e4c359dca03214dabada36ab82a769da560cdc5e))
* **ci:** tell release-please to scan Cargo.toml for the version annotation ([#107](https://github.com/Pawpaw-Technology/llm-wiki/issues/107)) ([7d60305](https://github.com/Pawpaw-Technology/llm-wiki/commit/7d60305ff4046936eb0b58a7176d34773e4c414f))
* **cli:** allow hyphen values for --content args (closes [#96](https://github.com/Pawpaw-Technology/llm-wiki/issues/96)) ([#98](https://github.com/Pawpaw-Technology/llm-wiki/issues/98)) ([b16170c](https://github.com/Pawpaw-Technology/llm-wiki/commit/b16170c0c7831e2a7dc77f2701bad7e33efc2f96))
* **core,cli,mcp:** silence .lw/ dirty-warning noise (closes [#97](https://github.com/Pawpaw-Technology/llm-wiki/issues/97)) ([#99](https://github.com/Pawpaw-Technology/llm-wiki/issues/99)) ([78ecfb1](https://github.com/Pawpaw-Technology/llm-wiki/commit/78ecfb1da57d64d820528ffbf2ab2ae803234fc4))
* **core:** suppress dirty-warning false positive on collapsed untracked dirs ([#92](https://github.com/Pawpaw-Technology/llm-wiki/issues/92)) ([8c0139e](https://github.com/Pawpaw-Technology/llm-wiki/commit/8c0139ebf273db84f9d00a15e3e196435cf5f80c))
* **core:** vault-relative path in PageAlreadyExists (closes [#87](https://github.com/Pawpaw-Technology/llm-wiki/issues/87)) ([#88](https://github.com/Pawpaw-Technology/llm-wiki/issues/88)) ([ba430af](https://github.com/Pawpaw-Technology/llm-wiki/commit/ba430af4a28e5ac7a3dec0f9d61c7320278be01b))
