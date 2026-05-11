# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-05-11

### Added
- `clipboard-wasm` feature in `bevy_instanced_text_edit` and `bevscode`: WASM clipboard
  backend backed by `navigator.clipboard.writeText` (fire-and-forget writes; reads return
  `None` since the browser clipboard API is async-only).
- `clipboard` feature in `bevscode` that explicitly opts in to the `arboard` native backend,
  mirroring how `bevy_instanced_text_edit` already gates it.
- First publish of `bevscode` to crates.io.

### Changed
- `bevscode` no longer has an unconditional `arboard` dependency; clipboard support is now
  opt-in via the `clipboard` feature (enabled by default for native builds).
- `bevy_instanced_text_edit` is now pulled in with `default-features = false` by `bevscode`
  so that hosts can choose their clipboard backend independently.

### Fixed
- WASM builds no longer fail to resolve `arboard` (a native-only crate).

## [0.1.0] - 2026-04-01

### Added
- Initial release of `bevy_instanced_text`, `bevy_instanced_text_edit`, `bevy_tree_sitter`,
  and `bevy_lsp`.
- GPU-accelerated instanced text rendering engine (`bevy_instanced_text`).
- Editable text widget with selection, cursor, undo/redo, and pluggable clipboard
  (`bevy_instanced_text_edit`).
- Tree-sitter integration for incremental syntax highlighting (`bevy_tree_sitter`).
- LSP client with JSON-RPC stdio transport, completion, and hover UI (`bevy_lsp`).
- `bevscode` editor plugin combining all of the above with a VS Code–like UX.
- Smooth scroll animation using VS Code's backdate approach.
- `vscode_like()` and `minimal()` settings presets.
- Viewport culling, entity pooling, and debounced updates for large-file performance.
