//! Byte transport for the JSON-RPC connection to a language server.
//!
//! [`LspTransport`] abstracts what's underneath `async-lsp`'s
//! [`MainLoop::run_buffered`]: a child process speaking stdio on native, or a
//! WebSocket to a server-side proxy in the browser. The shape above the
//! transport (`LspClient::send`, the response bridge, the router) doesn't
//! care which one it is.
//!
//! [`MainLoop::run_buffered`]: async_lsp::MainLoop::run_buffered

use bevy_tasks::Task;
use futures::io::{AsyncRead, AsyncWrite};
use std::future::Future;
use std::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
pub mod stdio;
#[cfg(not(target_arch = "wasm32"))]
pub use stdio::StdioTransport;

#[cfg(target_arch = "wasm32")]
pub mod websocket;
#[cfg(target_arch = "wasm32")]
pub use websocket::WebSocketTransport;

/// One open connection's runtime state.
///
/// Held by [`crate::LspClient`] so [`crate::LspClient::shutdown`] / `Drop` can
/// close the underlying transport, and so the crash watchdog has a future to
/// `.await` for unexpected exits.
pub struct TransportHandle {
    /// Background tasks the transport spawned to keep its byte streams alive
    /// (e.g. the stderr drainer on stdio). Dropped together with the handle.
    pub auxiliary_tasks: Vec<Task<()>>,
    /// Resolves when the transport exits. The crash watchdog awaits this to
    /// decide whether to emit [`crate::LspResponse::Crashed`].
    pub exited: Pin<Box<dyn Future<Output = ()> + Send>>,
    /// PID of the local process hosting the language server, if any. Surfaced
    /// in [`InitializeParams::process_id`]; `None` on transports where the
    /// concept doesn't apply (WebSocket, in-browser worker).
    ///
    /// [`InitializeParams::process_id`]: lsp_types::InitializeParams::process_id
    pub client_process_id: Option<u32>,
}

/// Connect to a language server and hand back the byte streams `async-lsp`
/// drives. Implementations are one-shot: [`Self::connect`] consumes `self`
/// and produces the runtime artefacts.
pub trait LspTransport: Send + 'static {
    type Reader: AsyncRead + Send + Unpin + 'static;
    type Writer: AsyncWrite + Send + Unpin + 'static;
    type Connect: Future<Output = std::io::Result<(Self::Reader, Self::Writer, TransportHandle)>>
        + Send;

    fn connect(self) -> Self::Connect;
}
