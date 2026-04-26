# Changelog

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
