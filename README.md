# bevy_code_editor

A GPU-accelerated code editor for the Bevy game engine. I built this because I thought it would be cool to have a fully-featured text editor running inside a game engine, in case anyone wants their player to code.

## Features

- **GPU-accelerated text rendering** - Uses a custom glyph atlas and per-line mesh system
- **Syntax highlighting** - Tree-sitter integration for accurate highlighting
- **Code folding** - Fold functions, classes, and blocks
- **LSP support** (optional) - Autocomplete, hover info, diagnostics
- **Multi-cursor editing** - Ctrl+D to add cursors at matching selections
- **Bracket matching** - Highlights matching brackets
- **Find/replace** - Standard search functionality
- **Minimap** - VSCode-style minimap with viewport indicator
- **Customizable themes** - Built-in VSCode-like and minimal themes
- **Undo/redo** - Full edit history
- **Auto-indentation** - Smart indentation and auto-closing brackets

## Quick Start

```bash
# Basic example
cargo run --example basic_editor

# With syntax highlighting
cargo run --example tree-sitter

# With LSP integration
cargo run --example lsp_integration --features lsp
```

## Usage

```rust
use bevy::prelude::*;
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(CodeEditorPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, mut state: ResMut<CodeEditorState>) {
    state.set_text("fn main() {\n    println!(\"Hello, world!\");\n}");
}
```

## Feature Flags

- `tree-sitter` (default) - Syntax highlighting via tree-sitter
- `clipboard` (default) - System clipboard support
- `lsp` - Language Server Protocol integration

Minimal build: `cargo build --no-default-features`

## Known Issues

1. **Scrolling despawns all visible entities after edit** - Mesh vertices have absolute Y positions baked in, so position changes require full rebuild. This is ~60-70 entities on every scroll.
2. **Tree-sitter completion triggers viewport rebuild** - When async parsing finishes, all visible entities get despawned/respawned even though only some lines changed highlighting.
3. **Cache invalidation inefficient** - Cache checks content_version for validity, but when only tree_version changes (highlighting updates), we still rebuild everything.Text Buffer

## License

MIT OR Apache-2.0

## Credits

Built with:

- [Bevy](https://bevyengine.org/) - Game engine
- [Ropey](https://github.com/cessen/ropey) - Text buffer
- [Tree-sitter](https://tree-sitter.github.io/) - Parsing and syntax highlighting
- [tower-lsp](https://github.com/ebkalderon/tower-lsp) - LSP implementation
- [rustybuzz](https://github.com/RazrFalcon/rustybuzz) - Text shaping

Inspired by [Zed](https://zed.dev/), [Helix](https://helix-editor.com/), and VSCode.
