//! In-memory [`LspTransport`] for tests. Pairs the [`LspClient`] with an
//! in-process fake server over two `piper` SPSC byte pipes — no subprocess,
//! no sockets, no real `rust-analyzer`. The client side looks identical to
//! [`StdioTransport`] from `async-lsp`'s perspective; the server side hands
//! back the matching reader/writer for the fake to drive (see
//! [`crate::test_support::FakeLanguageServer`]).
//!
//! Each direction is its own `piper::pipe`, giving independent read/write
//! halves with atomic-waker wakeups. This avoids any duplex `BiLock` that
//! would serialize concurrent reads against concurrent writes and stall
//! `async-lsp`'s `select_biased!` main loop.
//!
//! [`StdioTransport`]: super::stdio::StdioTransport
//! [`LspClient`]: crate::LspClient

use futures::channel::oneshot;
use futures::FutureExt;
use piper::{Reader, Writer};

use super::{BoxedFuture, LspTransport, TransportHandle};

/// Per-direction buffer size for the in-memory pipe. Generous enough that
/// `initialize` + capability payloads never block, small enough to surface
/// pathological back-pressure bugs.
pub const FAKE_PIPE_BUFFER: usize = 64 * 1024;

/// In-memory transport. Constructed via [`FakeTransport::duplex`], which
/// returns both the transport (to hand to [`crate::LspClient::start_with`])
/// and the server-side reader/writer (to drive the fake server).
pub struct FakeTransport {
    client_reader: Reader,
    client_writer: Writer,
    exit_signal: oneshot::Receiver<()>,
}

/// Both halves of a [`FakeTransport`] pair, plus the trigger for simulated
/// transport exit. `transport` plugs into [`crate::LspClient::start_with`];
/// `server_reader` / `server_writer` are what the fake LSP server reads from
/// and writes to; firing `exit_tx` closes the transport from below the LSP
/// layer, which the crash watchdog in `LspClient` surfaces as
/// [`crate::LspResponse::Crashed`].
pub struct FakeTransportEndpoints {
    pub transport: FakeTransport,
    pub server_reader: Reader,
    pub server_writer: Writer,
    pub exit_tx: oneshot::Sender<()>,
}

impl FakeTransport {
    /// Build two unidirectional pipes wired between a client and a fake server.
    pub fn duplex() -> FakeTransportEndpoints {
        // Direction A: client -> server bytes.
        let (server_reader, client_writer) = piper::pipe(FAKE_PIPE_BUFFER);
        // Direction B: server -> client bytes.
        let (client_reader, server_writer) = piper::pipe(FAKE_PIPE_BUFFER);
        let (exit_tx, exit_rx) = oneshot::channel();
        FakeTransportEndpoints {
            transport: FakeTransport {
                client_reader,
                client_writer,
                exit_signal: exit_rx,
            },
            server_reader,
            server_writer,
            exit_tx,
        }
    }
}

impl LspTransport for FakeTransport {
    type Reader = Reader;
    type Writer = Writer;
    type Connect = BoxedFuture<std::io::Result<(Self::Reader, Self::Writer, TransportHandle)>>;

    fn connect(self) -> Self::Connect {
        async move {
            let exit_signal = self.exit_signal;
            let exited = async move {
                let _ = exit_signal.await;
            }
            .boxed();
            Ok((
                self.client_reader,
                self.client_writer,
                TransportHandle {
                    auxiliary_tasks: Vec::new(),
                    exited,
                    client_process_id: None,
                },
            ))
        }
        .boxed()
    }
}
