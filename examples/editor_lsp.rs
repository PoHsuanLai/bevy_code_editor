//! LSP integration example using bevscode's built-in `bevy_ui` popups.
//!
//! All popup rendering (completion, hover, signature help, code actions,
//! rename) plus inline decorations (inlay hints, document highlights)
//! ship inside `CodeEditorPlugins` under the `lsp` feature — no host UI
//! code required. The host spawns a `LanguageService` entity (owns the
//! LSP transport), spawns an editor with `LspSession` pointing at it,
//! and sends the `Initialize` / `DidOpen` requests.
//!
//! Run: `cargo run --example editor_lsp --features lsp`. Requires
//! `rust-analyzer` on `PATH` (`rustup component add rust-analyzer`).

use bevscode::lsp_ui::{attach_lsp, spawn_language_service, LspClient, LspRequest};
use bevscode::prelude::{BufferAnchorParam, RopeBuffer, *};
use bevscode::types::{CodeEditor, CursorState};
use bevy::prelude::*;
use bevy_lsp::messages::{LspLogMessage, LspShowMessage};

fn main() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "LSP Integration Example".to_string(),
                    resolution: (1200, 800).into(),
                    ..default()
                }),
                ..default()
            })
            .set(bevy::asset::AssetPlugin {
                file_path: "assets".into(),
                ..default()
            }),
    );

    app.add_plugins(CodeEditorPlugins);

    app.add_systems(Startup, (setup_camera, spawn_editor))
        .add_systems(PostStartup, setup_editor)
        .add_systems(
            Update,
            (
                display_lsp_info,
                auto_request_completion,
                log_lsp_server_messages,
            ),
        )
        .run();
}

fn log_lsp_server_messages(
    mut logs: MessageReader<LspLogMessage>,
    mut shows: MessageReader<LspShowMessage>,
) {
    for ev in logs.read() {
        info!("[ra log] {:?}: {}", ev.typ, ev.message);
    }
    for ev in shows.read() {
        info!("[ra show] {:?}: {}", ev.typ, ev.message);
    }
}

fn spawn_editor(mut commands: Commands) {
    commands.spawn((CodeEditor, AutoResizeViewport, Name::new("CodeEditor")));
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(EditorTheme::default().background),
            ..default()
        },
    ));
}

fn setup_editor(
    mut commands: Commands,
    editor_query: Query<Entity, With<CodeEditor>>,
    asset_server: Res<AssetServer>,
    mut set_text_writer: MessageWriter<SetTextRequested>,
    mut lsp_w: MessageWriter<LspRequest>,
) {
    let Ok(editor_entity) = editor_query.single() else {
        return;
    };

    commands.entity(editor_entity).insert((
        TextFont::from_font_size(14.0).with_font(asset_server.load("fonts/FiraMono-Regular.ttf")),
        MonoFontFaces::default().with_bold(asset_server.load("fonts/FiraMono-Medium.ttf")),
    ));

    let example_file_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("editor_lsp.rs");
    let rust_code =
        std::fs::read_to_string(&example_file_path).expect("Failed to read example file");

    set_text_writer.write(SetTextRequested {
        entity: editor_entity,
        text: rust_code.clone(),
    });

    commands
        .entity(editor_entity)
        .insert(TreeSitterGrammar::new(
            bevy_tree_sitter::arborium::lang_rust::language().into(),
            bevy_tree_sitter::arborium::lang_rust::HIGHLIGHTS_QUERY,
        ));

    let file_uri_str = format!("file://{}", example_file_path.to_string_lossy());
    #[cfg(target_os = "windows")]
    let file_uri_str = format!(
        "file:///{}",
        example_file_path.to_string_lossy().replace('\\', "/")
    );

    let doc_uri = lsp_types::Url::parse(&file_uri_str).expect("Failed to parse URI");

    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_uri =
        lsp_types::Url::from_directory_path(&project_root).expect("Failed to get project root URI");

    let markdown_then_plain = vec![
        lsp_types::MarkupKind::Markdown,
        lsp_types::MarkupKind::PlainText,
    ];
    let capabilities = lsp_types::ClientCapabilities {
        text_document: Some(lsp_types::TextDocumentClientCapabilities {
            hover: Some(lsp_types::HoverClientCapabilities {
                content_format: Some(markdown_then_plain.clone()),
                ..Default::default()
            }),
            completion: Some(lsp_types::CompletionClientCapabilities {
                completion_item: Some(lsp_types::CompletionItemCapability {
                    documentation_format: Some(markdown_then_plain.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            signature_help: Some(lsp_types::SignatureHelpClientCapabilities {
                signature_information: Some(lsp_types::SignatureInformationSettings {
                    documentation_format: Some(markdown_then_plain.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let service = match spawn_language_service(
        &mut commands,
        &mut lsp_w,
        "rust-analyzer",
        &[],
        root_uri,
        capabilities,
    ) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to start rust-analyzer: {:?}", e);
            return;
        }
    };

    attach_lsp(
        &mut commands,
        &mut lsp_w,
        editor_entity,
        service,
        doc_uri,
        "rust",
        rust_code.to_string(),
    );

    info!("LSP started for file: {:?}", example_file_path);
}

fn display_lsp_info(query: Query<&LspClient, Changed<LspClient>>) {
    if !query.is_empty() {
        debug!("LSP client state changed");
    }
}

/// Auto-trigger completion requests after typing.
fn auto_request_completion(
    editor_query: Query<(&CursorState, Ref<InstancedText<RopeBuffer>>), With<CodeEditor>>,
    mut writer: MessageWriter<bevscode::types::events::CompletionRequested>,
    _anchors: BufferAnchorParam<RopeBuffer>,
) {
    let Ok((cursor, buffer)) = editor_query.single() else {
        return;
    };

    if !buffer.is_changed() {
        return;
    }

    let cursor_pos = cursor.cursor_pos.min(buffer.rope().len_chars());
    if cursor_pos == 0 {
        return;
    }
    writer.write(bevscode::types::events::CompletionRequested::new(
        cursor_pos,
    ));
}
