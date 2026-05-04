//! LSP (Language Server Protocol) integration example
//!
//! Demonstrates how to integrate the code editor with LSP for advanced features
//! like auto-completion, go-to-definition, and diagnostics.
//!
//! Note: This is a minimal example showing the structure. A full LSP implementation
//! requires additional setup and a running language server.

use bevy::prelude::*;
use bevy_code_editor::prelude::*;

fn main() {
    #[cfg(feature = "lsp")]
    {
        run_with_lsp();
    }

    #[cfg(not(feature = "lsp"))]
    {
        run_without_lsp();
    }
}

#[cfg(feature = "lsp")]
fn run_with_lsp() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "LSP Integration Example".to_string(),
            resolution: (1200, 800).into(),
            ..default()
        }),
        ..default()
    }));

    // EguiPlugin must be added BEFORE editor plugins so contexts are available
    #[cfg(feature = "egui-overlays")]
    {
        app.add_plugins(bevy_egui::EguiPlugin::default());
    }

    app.add_plugins(CodeEditorPlugin::default())
        .add_plugins(EditorUiPlugin::default());

    // LspPlugin is already added by CodeEditorPlugin when the lsp feature is enabled.
    // Just add the UI layer.
    #[cfg(feature = "egui-overlays")]
    {
        app.add_plugins(LspEguiUiPlugin::default());
    }
    #[cfg(not(feature = "egui-overlays"))]
    {
        app.add_plugins(LspUiPlugin::default());
    }

    app.add_systems(PostStartup, setup_editor)
        .add_systems(Update, (display_lsp_info, auto_request_completion))
        .run();
}

#[cfg(feature = "lsp")]
fn setup_editor(
    mut editor_query: Query<
        (
            &mut CodeEditorState,
            &mut CursorState,
            &mut TextViewState,
            &mut EditHistoryState,
            &mut SelectionState,
            &mut SyntaxCacheState,
        ),
        With<CodeEditor>,
    >,
    mut lsp_client: ResMut<bevy_code_editor::lsp::LspClient>,
    mut lsp_sync: ResMut<bevy_code_editor::lsp::LspSyncState>,
    #[cfg(feature = "tree-sitter")] mut syntax: ResMut<bevy_code_editor::plugin::SyntaxResource>,
) {
    let Ok((_state, mut cursor, mut tv, mut hist, mut sel, mut syntax_cache)) =
        editor_query.single_mut()
    else {
        return;
    };

    // Read the source code of this example file
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let example_file_path = current_dir.join("examples/lsp.rs");
    let rust_code =
        std::fs::read_to_string(&example_file_path).expect("Failed to read example file");

    hist.set_text(
        &mut sel,
        &mut syntax_cache,
        &mut cursor,
        &mut tv,
        &rust_code,
    );

    // Define Rust language configuration
    let rust_lang = Language {
        name: "rust".to_string(),
        #[cfg(feature = "tree-sitter")]
        tree_sitter: Some(TreeSitterConfig {
            grammar: tree_sitter_rust::LANGUAGE.into(),
            highlights_query: tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
        }),
        lsp_command: Some(("rust-analyzer", &[])),
    };

    // Set up tree-sitter syntax highlighting
    #[cfg(feature = "tree-sitter")]
    {
        if let Some(provider) = rust_lang.create_tree_sitter_provider() {
            syntax.set_provider(provider);
        }
    }

    let file_uri_str = format!("file://{}", example_file_path.to_string_lossy());
    #[cfg(target_os = "windows")]
    let file_uri_str = format!(
        "file:///{}",
        example_file_path.to_string_lossy().replace('\\', "/")
    );

    let doc_uri = lsp_types::Url::parse(&file_uri_str).expect("Failed to parse URI");

    // Start rust-analyzer
    // Make sure 'rust-analyzer' is in your PATH (rustup component add rust-analyzer)
    if let Err(e) = lsp_client.start("rust-analyzer", &[]) {
        error!("Failed to start rust-analyzer: {:?}", e);
        // Fallback or just return
        return;
    }

    // Initialize
    // rootUri is the project root, which is usually the directory containing Cargo.toml
    let project_root = current_dir;
    let root_uri =
        lsp_types::Url::from_directory_path(&project_root).expect("Failed to get project root URI");
    let capabilities = lsp_types::ClientCapabilities::default();

    lsp_client.send(bevy_code_editor::lsp::LspMessage::Initialize {
        root_uri: root_uri.clone(),
        capabilities,
    });

    // Send initialized notification immediately
    lsp_client.send(bevy_code_editor::lsp::LspMessage::Initialized);

    // Open the document
    lsp_client.send(bevy_code_editor::lsp::LspMessage::DidOpen {
        uri: doc_uri.clone(), // Use the actual example file's URI
        language_id: "rust".to_string(),
        version: 1,
        text: rust_code.to_string(),
    });

    lsp_sync.document_uri = Some(doc_uri); // Store the document URI in lsp_sync

    info!("LSP started for file: {:?}", example_file_path);
}

#[cfg(feature = "lsp")]
fn display_lsp_info(lsp_client: Res<LspClient>) {
    if lsp_client.is_changed() {
        info!("LSP client state changed");
    }
}

/// Auto-trigger completion requests after typing.
///
/// The input system doesn't emit RequestCompletionEvent yet, so this bridges
/// the gap by watching for content changes and firing completion at the cursor.
#[cfg(feature = "lsp")]
fn auto_request_completion(
    editor_query: Query<(&CodeEditorState, &CursorState, &TextViewState), With<CodeEditor>>,
    mut writer: MessageWriter<bevy_code_editor::types::events::RequestCompletionEvent>,
) {
    let Ok((_state, cursor, tv)) = editor_query.single() else {
        return;
    };

    if !tv.pending_update {
        return;
    }

    let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
    if cursor_pos == 0 {
        return;
    }

    let line = tv.rope.char_to_line(cursor_pos);
    let line_start = tv.rope.line_to_char(line);
    let character = cursor_pos - line_start;

    writer.write(bevy_code_editor::types::events::RequestCompletionEvent::new(line, character));
}

#[cfg(not(feature = "lsp"))]
fn run_without_lsp() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "LSP Example (Feature Not Enabled)".to_string(),
                resolution: (1200, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugin::default())
        .add_systems(PostStartup, show_lsp_message)
        .run();
}

#[cfg(not(feature = "lsp"))]
fn show_lsp_message(
    mut editor_query: Query<
        (
            &mut CodeEditorState,
            &mut CursorState,
            &mut TextViewState,
            &mut EditHistoryState,
            &mut SelectionState,
            &mut SyntaxCacheState,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((mut state, mut cursor, mut tv, mut hist, mut sel, mut syntax_cache)) =
        editor_query.single_mut()
    else {
        return;
    };

    let message = r#"LSP feature is not enabled!

To run this example with LSP support, use:

    cargo run --example lsp_integration --features lsp

The LSP feature provides:
- Real-time diagnostics (errors, warnings)
- Auto-completion suggestions
- Go-to-definition
- Hover information
- Code actions (quick fixes)
- Symbol search
- Refactoring support

Implementation notes:
- Requires a language server (e.g., rust-analyzer for Rust)
- Uses tower-lsp for LSP protocol handling
- Runs asynchronously with Tokio runtime
- Integrates with the editor's change tracking

Example LSP setup for Rust:
1. Install rust-analyzer
2. Start the language server
3. Connect via stdio or TCP
4. Send textDocument/didOpen notification
5. Receive diagnostics and other features

For a complete LSP implementation, see the bevy_code_editor documentation.
"#;

    hist.set_text(&mut sel, &mut syntax_cache, &mut cursor, &mut tv, message);
}
