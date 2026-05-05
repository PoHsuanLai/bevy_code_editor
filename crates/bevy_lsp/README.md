# bevy_lsp

Per-entity Language Server Protocol transport for Bevy. Just the protocol — no popups, no completion filtering, no debouncing, no UI state.

What this crate is for: editors, debugger UIs, hot-reload tooling, AI panels asking an LSP for completions, code-search consumers. Anything that wants to talk JSON-RPC to a language server.

What this crate is **not** for: rendering completion popups (that's the consumer's job), tracking "what's selected" (consumer's job), filtering / fuzzy matching (consumer's job).

## Architecture

The protocol layer lives as **per-entity Components**, not global resources. A host application can have many editors / many servers at once just by spawning entities.

| Component | What it carries |
|---|---|
| **`LspClient`** | One running language-server peer. Holds the async-lsp `ServerSocket`, an mpsc bridge from the async side into ECS via `try_recv`, and an abort handle for the spawned MainLoop task. |
| **`LspDocument`** | One open document on that server: `{ uri: Url, version: i32, language_id: String }`. Bumped each time the editor sends `did_change`. |
| **`ServerCapabilities`** | Parsed capabilities from the `initialize` response. Populated by the consumer when it observes `LspResponse::Initialized`. |

Pair the three on one entity to model an editor-document-server triple. Two editors? Two entities, two clients, two URIs.

The transport runs `async-lsp 0.2` on a shared tokio runtime via `bevy-tokio-tasks`. Outgoing requests are spawned as tokio tasks; their typed results are pushed into a per-client mpsc channel. ECS systems drain the channel via `LspClient::try_recv()`.

## Quick start

```rust
use bevy::prelude::*;
use bevy_lsp::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(LspPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, drain_responses)
        .run();
}

fn setup(mut commands: Commands, runtime: Res<TokioTasksRuntime>) {
    let uri = lsp_types::Url::parse("file:///tmp/main.rs").unwrap();

    let mut client = LspClient::new();
    client.start(&runtime, "rust-analyzer", &[]).unwrap();

    commands.spawn((
        client,
        LspDocument::new(uri, "rust"),
        ServerCapabilities::default(),
    ));
}

fn drain_responses(mut q: Query<&LspClient>) {
    for client in &q {
        while let Some(response) = client.try_recv() {
            // Match on response variant, update your state.
            // E.g. LspResponse::Hover { content, range } -> show a tooltip.
        }
    }
}
```

The `LspPlugin` adds `bevy-tokio-tasks::TokioTasksPlugin` if the host hasn't already. That gives you a `TokioTasksRuntime` resource for `LspClient::start()`.

## Sending messages

```rust
client.send(LspMessage::Hover {
    uri: doc.uri.clone(),
    position: rope_char_to_lsp_position(&rope, cursor_pos, PositionEncoding::Utf16),
});
```

The full set of `LspMessage` variants covers initialize/initialized/did_open/did_change, completion, hover, goto-definition, references, formatting, signature-help, code-action, inlay-hint, execute-command, document-highlight, prepare-rename, rename.

## Position helpers

LSP measures positions in **code units of a negotiated encoding** (UTF-16 by spec default). Char counts are wrong for non-ASCII content — `é`, CJK, emoji all drift.

`bevy_lsp::pos` provides the conversions:

```rust
use bevy_lsp::{rope_char_to_lsp_position, lsp_position_to_rope_char, PositionEncoding};

let pos = rope_char_to_lsp_position(&rope, cursor_char, PositionEncoding::Utf16);
let back = lsp_position_to_rope_char(&rope, pos, PositionEncoding::Utf16);
```

7 table-driven tests cover ASCII, é (Latin-1 supplemental), 中 (BMP CJK), 🎉 (supplementary plane / surrogate pair), multi-line, range construction, byte round-trips.

## Capability-aware sending

Before sending a feature-specific request, check the server advertises support:

```rust
client.send_if_supported(LspMessage::Hover { uri, position }, &capabilities);
```

`Initialize`, `Initialized`, `DidOpen`, `DidChange`, `ExecuteCommand` are always permitted. Everything else is gated on the matching `supports_*()` predicate from `ServerCapabilities`.

## What's not here

This crate is purely transport + per-document state. UI state (popup visibility, completion filter strings, fuzzy matching, debounce timers) lives in the consumer. The editor's [`bevy_code_editor`](../bevy_code_editor) `lsp_ui` module is a worked example of building popup state on top.

## License

MIT OR Apache-2.0
