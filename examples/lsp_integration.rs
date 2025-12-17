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
    use bevy_code_editor::lsp::*;

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "LSP Integration Example".to_string(),
                resolution: (1200, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugin::default())
        .insert_resource(LspClient::default())
        .add_systems(Startup, setup_editor)
        .add_systems(Update, (
            process_lsp_messages,
            display_lsp_info,
        ))
        .run();
}

#[cfg(feature = "lsp")]
fn setup_editor(
    mut state: ResMut<CodeEditorState>,
    mut lsp_client: ResMut<bevy_code_editor::lsp::LspClient>
) {
    // Read the source code of this example file
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let example_file_path = current_dir.join("examples/lsp_integration.rs");
    let rust_code = std::fs::read_to_string(&example_file_path).expect("Failed to read example file");

    state.set_text(&rust_code);
    
    let file_uri_str = format!("file://{}", example_file_path.to_string_lossy());
    #[cfg(target_os = "windows")]
    let file_uri_str = format!("file:///{}", example_file_path.to_string_lossy().replace('\\', "/"));

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
    let root_uri = lsp_types::Url::from_directory_path(&project_root).expect("Failed to get project root URI");
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

    state.document_uri = Some(doc_uri); // Store the document URI in state
    
    info!("LSP started for file: {:?}", example_file_path);
}

#[cfg(feature = "lsp")]
fn display_lsp_info(
    lsp_client: Res<LspClient>,
    state: Res<CodeEditorState>,
) {
    // This would display LSP information in a real implementation
    // For now, it's just a placeholder showing the structure

    if lsp_client.is_changed() {
        info!("LSP client state changed");
    }

    // In a real implementation, you would:
    // 1. Show diagnostics (errors, warnings) from the language server
    // 2. Provide auto-completion suggestions
    // 3. Show hover information
    // 4. Enable go-to-definition
    // 5. Display code actions (quick fixes)
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
        .add_systems(Startup, show_lsp_message)
        .run();
}

#[cfg(not(feature = "lsp"))]
fn show_lsp_message(mut state: ResMut<CodeEditorState>) {
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

    state.set_text(message);
}
