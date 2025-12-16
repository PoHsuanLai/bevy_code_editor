# Bevy Code Editor

A high-performance, GPU-accelerated code editor plugin for [Bevy](https://bevyengine.org/).

## Features

- **GPU-Accelerated Rendering**: Uses Bevy's text rendering system for fast, smooth display
- **Efficient Text Buffer**: Built on [ropey](https://github.com/cessen/ropey) for efficient text editing operations
- **Syntax Highlighting**: Optional tree-sitter integration for language-aware highlighting
- **LSP Support**: Optional Language Server Protocol integration for advanced IDE features
- **Performance Optimizations**:
  - Viewport culling (only renders visible lines)
  - Entity pooling (reuses text entities)
  - Debounced updates (~60fps)
  - Incremental highlighting
- **Fully Customizable**:
  - Themes (colors, fonts)
  - Editor behavior (tabs, wrapping, cursor)
  - UI elements (line numbers, rulers, gutter)
  - Performance settings

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
bevy = "0.17"
bevy_code_editor = "0.1"
```

Basic usage:

```rust
use bevy::prelude::*;
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(CodeEditorPlugin::default())
        .run();
}
```

## Examples

Run the basic example:

```bash
cargo run --example basic_editor
```

## Features

### Default Features

- `syntax-highlighting`: Enables tree-sitter based syntax highlighting

### Optional Features

- `lsp`: Enables Language Server Protocol support

### Disabling Default Features

For a minimal, performance-focused editor:

```toml
[dependencies]
bevy_code_editor = { version = "0.1", default-features = false }
```

## Customization

### Using Presets

```rust
use bevy_code_editor::prelude::*;

// VSCode-like defaults
let settings = EditorSettings::vscode_like();

// Minimal/performance-focused
let settings = EditorSettings::minimal();

App::new()
    .add_plugins(CodeEditorPlugin::with_settings(settings))
    .run();
```

### Custom Settings

```rust
use bevy_code_editor::prelude::*;

let settings = EditorSettings {
    font: FontSettings {
        family: "fonts/MyFont.ttf".to_string(),
        size: 20.0,
        char_width: 15.0,
        line_height: 30.0,
        ..Default::default()
    },
    theme: Theme::dark(), // or Theme::minimal()
    ui: UISettings {
        show_line_numbers: true,
        highlight_active_line: true,
        ..Default::default()
    },
    // ... customize other settings
    ..Default::default()
};
```

## API Overview

### Core Types

- `CodeEditorPlugin`: The main Bevy plugin
- `CodeEditorState`: Resource containing editor state (text, cursor, selection)
- `EditorSettings`: Comprehensive configuration options
- `ViewportDimensions`: Tracks viewport size for rendering

### Working with Text

```rust
fn modify_text(mut state: ResMut<CodeEditorState>) {
    // Set text
    state.set_text("Hello, world!");

    // Insert character
    state.insert_char('a');

    // Delete
    state.delete_backward();
    state.delete_forward();

    // Move cursor
    state.move_cursor(5);  // Move 5 chars right
    state.move_cursor(-3); // Move 3 chars left

    // Get text
    let text = state.text();
    let line_count = state.line_count();
}
```

### Syntax Highlighting

Enable the `syntax-highlighting` feature and configure a highlighter:

```rust
#[cfg(feature = "syntax-highlighting")]
use tree_sitter_highlight::HighlightConfiguration;

fn setup_highlighting(mut state: ResMut<CodeEditorState>) {
    // Configure your tree-sitter grammar
    let config = HighlightConfiguration::new(
        tree_sitter_rust::language(),
        tree_sitter_rust::HIGHLIGHT_QUERY,
        "",
        "",
    ).unwrap();

    state.highlight_config = Some(config);
}
```

### LSP Integration

Enable the `lsp` feature:

```rust
#[cfg(feature = "lsp")]
use bevy_code_editor::lsp::*;

fn setup_lsp(lsp_client: Res<LspClient>) {
    lsp_client.send(LspMessage::Initialize {
        root_uri: Url::parse("file:///path/to/project").unwrap(),
        capabilities: ClientCapabilities::default(),
    });
}
```

## Architecture

### Coordinate System

The editor uses a top-left coordinate system for layout:
- (0, 0) is the top-left corner
- Y increases downward
- Coordinates are converted to Bevy's center-origin system internally

### Layout Constants

```
┌──────────────────────────────────────┐
│ 20px margin                          │
│ ├─ Line Numbers                      │
│ │                                    │
│ 60px separator                       │
│ │                                    │
│ 70px code margin                     │
│ ├─ Code Text                         │
│ │                                    │
└──────────────────────────────────────┘
```

### Rendering Pipeline

1. **Debounce Updates**: Throttle updates to ~60fps
2. **Detect Resize**: Update viewport-dependent positions
3. **Update Scroll**: Reposition visible entities
4. **Update Text**: Rebuild text entities (with pooling)
5. **Update Line Numbers**: Update gutter display
6. **Update Selection**: Render selection highlights
7. **Update Cursor**: Position and animate cursor

## Performance Tips

1. **Use Viewport Culling**: Already enabled by default
2. **Enable Entity Pooling**: Already enabled by default
3. **Adjust Debounce**: Lower `debounce_ms` for faster updates, higher for better performance
4. **Disable Features**: Use `default-features = false` for minimal build
5. **Limit Syntax Highlighting**: Set `max_syntax_highlight_size` to skip highlighting large files

## Comparison with Other Solutions

| Feature | bevy_code_editor | DOM-based | Custom WGPU |
|---------|------------------|-----------|-------------|
| Performance | ✓ GPU-accelerated | ✗ Limited | ✓ Fully custom |
| Integration | ✓ Native Bevy | ✗ Separate | ~ Complex |
| Flexibility | ✓ Highly customizable | ~ Limited by DOM | ✓ Unlimited |
| Ease of Use | ✓ Plugin + systems | ✓ Simple HTML | ✗ Complex setup |

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT license

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Roadmap

- [ ] Code folding
- [ ] Minimap
- [ ] Multi-cursor support
- [ ] Find/replace UI
- [ ] Bracket matching visualization
- [ ] Indent guides
- [ ] Git diff indicators
- [ ] More LSP features (completion UI, hover, etc.)
- [ ] More syntax highlighting themes
- [ ] Word wrap
- [ ] Horizontal scrolling
