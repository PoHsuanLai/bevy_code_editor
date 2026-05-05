# bevy_tree_sitter

Text-rendering-agnostic tree-sitter integration for Bevy. Returns capture names, not colors.

What this crate is for: code editors mapping captures to a theme, code-outline panels, AI agents reasoning about syntactic structure, structural search tools, log viewers highlighting stack traces — anything that wants tree-sitter parsing without dragging in a renderer.

What this crate is **not** for: deciding what color a `keyword` should be (that's a theme decision, lives in the consumer), shaping or rendering glyphs, owning a buffer.

## Architecture

The integration is two layers:

| Layer | What it does |
|---|---|
| **`SyntaxProvider` trait** | Pluggable structural-highlight backend. `highlight_range(text, start_line, end_line, start_byte) -> Vec<Vec<HighlightRange>>` returns capture-name byte ranges. The trait knows nothing about Bevy or rendering. |
| **`TreeSitterProvider` struct** | The canonical impl. Wraps a `tree_sitter::Parser`, `tree_sitter::Language`, query, and the cached `Tree`. `apply_sync_edit` keeps the tree valid for query work while re-parsing happens off-thread. |

Async parsing runs on `AsyncComputeTaskPool`. `spawn_parse` attaches a `ParseTask` Component to a transient entity; `poll_parse_tasks` (registered by `TreeSitterPlugin`) polls it and emits `ParseCompleted { target, content_version, tree }`. The consumer routes the new tree back into its `TreeSitterProvider`.

## Quick start

```rust
use bevy::prelude::*;
use bevy_tree_sitter::prelude::*;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(TreeSitterPlugin)
        .run();
}
```

To parse a file:

```rust
use bevy_tree_sitter::{spawn_parse, ParseCompleted, TreeSitterProvider};
use ropey::Rope;

fn kick_off_parse(mut commands: Commands, q: Query<(Entity, &TreeSitterProvider, &MyBuffer)>) {
    for (entity, provider, buf) in &q {
        let parser = provider.clone_parser();    // borrowed just long enough to spawn
        let language = provider.language();
        let cached = provider.cached_tree_with_edits();
        spawn_parse(
            &mut commands,
            entity,
            buf.content_version,
            buf.rope.clone(),
            parser,
            language,
            cached,
        );
    }
}

fn apply_completed(
    mut events: MessageReader<ParseCompleted>,
    mut q: Query<&mut TreeSitterProvider>,
) {
    for event in events.read() {
        if let Ok(mut provider) = q.get_mut(event.target) {
            provider.set_parsed_tree(event.content_version, event.tree.clone());
        }
    }
}
```

## Capture names, not colors

`HighlightRange { byte_range, capture_name: Arc<str> }` — the capture name is the raw tree-sitter query capture (e.g. `"keyword"`, `"function.method"`, `"string"`). Mapping that to a `Color` is the consumer's job.

`Arc<str>` so emitting one highlight per token doesn't allocate — the same `"keyword"` string is shared across thousands of ranges in a typical file.

The editor's `bevy_code_editor::syntax_highlighting` module is a worked example: it holds a `Theme` HashMap from capture name to `Color` and converts `HighlightRange` runs into engine `StyleRun`s on the fly.

## Languages

`Language { config: TreeSitterConfig, source }` carries the language descriptor + canonical highlight query. The `bevy_tree_sitter::languages` convenience module provides built-in `Language` constructors for Rust, Python, JavaScript, TypeScript, etc.

```rust
use bevy_tree_sitter::languages;

let lang = languages::rust();
let provider = TreeSitterProvider::new(lang);
```

## What's not here

- **Themes.** The crate emits capture names; theming is the consumer's job. `bevy_code_editor` ships a default theme; AI / outline consumers can ignore colors entirely.
- **Buffer storage.** Consumers hand a `&str` or `Rope` to `highlight_range` / `spawn_parse`. The crate doesn't own the buffer.
- **Bevy reflection on `Tree` / `Parser` / `Task`.** Tree-sitter's C-binding types don't implement `Reflect`. The capture-name `Arc<str>` strings are reachable via the consumer's own reflected types.

## Re-export

The crate re-exports the underlying `tree-sitter` crate as `bevy_tree_sitter::ts` so consumers can name `ts::Tree`, `ts::InputEdit`, etc. without taking a direct dep on the C-binding crate.

## License

MIT OR Apache-2.0
