//! In-process fake language server for integration tests.
//!
//! Pair an [`LspClient`] with a [`FakeLanguageServer`] over a [`FakeTransport`]
//! to drive the whole `bevy_lsp` stack — JSON-RPC framing, async-lsp routing,
//! the response bridge, and the ECS drain — without spawning a real LSP
//! subprocess. Modeled on Zed's `FakeLanguageServer`.
//!
//! ```no_run
//! use bevy_lsp::test_support::FakeLanguageServer;
//! use bevy_lsp::LspClient;
//! use lsp_types::{request::Completion, CompletionItem, CompletionResponse, ServerCapabilities};
//!
//! let (transport, fake) = FakeLanguageServer::new(ServerCapabilities::default());
//! fake.set_request_handler::<Completion, _, _>(|_params| async move {
//!     Ok(Some(CompletionResponse::Array(vec![CompletionItem {
//!         label: "println!".into(),
//!         ..Default::default()
//!     }])))
//! });
//!
//! let mut client = LspClient::new();
//! client.start_with(transport);
//! ```
//!
//! [`LspClient`]: crate::LspClient
//! [`FakeTransport`]: crate::transport::FakeTransport

use std::collections::HashMap;
use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::tracing::TracingLayer;
use async_lsp::{ClientSocket, ErrorCode, ResponseError};
use bevy_log::warn;
use bevy_tasks::{AsyncComputeTaskPool, Task};
use futures::channel::oneshot;
use lsp_types::notification::Notification;
use lsp_types::request::{Initialize, Request, Shutdown};
use lsp_types::{InitializeResult, ServerCapabilities};
use serde_json::Value as JsonValue;
use tower::ServiceBuilder;

use crate::transport::{FakeTransport, FakeTransportEndpoints};

type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type HandlerFn = Box<dyn Fn(JsonValue) -> BoxFut<Result<JsonValue, ResponseError>> + Send + Sync>;
type HandlerMap = Arc<Mutex<HashMap<&'static str, HandlerFn>>>;

/// A notification received by the fake server from the client side. Method
/// matches the LSP wire name (`textDocument/didOpen`, `initialized`, etc.).
#[derive(Debug, Clone)]
pub struct ReceivedNotification {
    pub method: String,
    pub params: JsonValue,
}

/// An in-process language server. Lives on the server side of a [`FakeTransport`]
/// and lets tests register typed handlers per LSP request method, send
/// server-initiated notifications, and observe client-initiated notifications.
pub struct FakeLanguageServer {
    handlers: HandlerMap,
    server_socket: ClientSocket,
    received_notifications: async_channel::Receiver<ReceivedNotification>,
    exit_tx: Option<oneshot::Sender<()>>,
    driver: Option<Task<()>>,
}

impl FakeLanguageServer {
    /// Build a connected (transport, server) pair. `capabilities` are returned
    /// from the default `initialize` handler unless the test overrides it via
    /// [`Self::set_request_handler::<Initialize>`].
    pub fn new(capabilities: ServerCapabilities) -> (FakeTransport, FakeLanguageServer) {
        let FakeTransportEndpoints {
            transport,
            server_reader,
            server_writer,
            exit_tx,
        } = FakeTransport::duplex();

        let handlers: HandlerMap = Arc::new(Mutex::new(HashMap::new()));
        let (notif_tx, notif_rx) = async_channel::unbounded();

        // Default `initialize` returns the supplied capabilities. Tests can
        // override by calling `set_request_handler::<Initialize>` later.
        install_default_initialize(&handlers, capabilities);
        install_default_shutdown(&handlers);

        let (mainloop, server_socket) = async_lsp::MainLoop::new_server(|_client| {
            let router = build_server_router(handlers.clone(), notif_tx);
            ServiceBuilder::new()
                .layer(TracingLayer::default())
                .layer(CatchUnwindLayer::default())
                .layer(ConcurrencyLayer::default())
                .service(router)
        });

        let pool = AsyncComputeTaskPool::get();
        let driver = pool.spawn(async move {
            if let Err(err) = mainloop.run_buffered(server_reader, server_writer).await {
                warn!("[fake-lsp] mainloop exited: {err}");
            }
        });

        (
            transport,
            FakeLanguageServer {
                handlers,
                server_socket,
                received_notifications: notif_rx,
                exit_tx: Some(exit_tx),
                driver: Some(driver),
            },
        )
    }

    /// Install a typed handler for request method `R`. Replaces any previous
    /// handler for the same method. The handler is called every time the
    /// client issues an `R` request; deserialization of `R::Params` and
    /// serialization of `R::Result` happen inside this wrapper.
    pub fn set_request_handler<R, F, Fut>(&self, handler: F)
    where
        R: Request,
        R::Params: serde::de::DeserializeOwned + Send + 'static,
        R::Result: serde::Serialize + Send + 'static,
        F: Fn(R::Params) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<R::Result, ResponseError>> + Send + 'static,
    {
        let boxed: HandlerFn = Box::new(move |params: JsonValue| {
            let fut = serde_json::from_value::<R::Params>(params).map(&handler);
            Box::pin(async move {
                match fut {
                    Ok(fut) => {
                        let result = fut.await?;
                        Ok(serde_json::to_value(result).expect("serialize R::Result"))
                    }
                    Err(_) => Err(ResponseError::new(
                        ErrorCode::INVALID_PARAMS,
                        "fake server: failed to deserialize params",
                    )),
                }
            })
        });
        self.handlers.lock().unwrap().insert(R::METHOD, boxed);
    }

    /// Remove a previously-installed handler for request method `R`. After
    /// removal, the client will receive `METHOD_NOT_FOUND` until a new handler
    /// is installed.
    pub fn remove_request_handler<R: Request>(&self) {
        self.handlers.lock().unwrap().remove(R::METHOD);
    }

    /// Send a server-initiated notification to the client (e.g.
    /// `textDocument/publishDiagnostics`).
    pub fn notify<N: Notification>(&self, params: N::Params) {
        if let Err(err) = self.server_socket.notify::<N>(params) {
            warn!("[fake-lsp] notify {} failed: {err}", N::METHOD);
        }
    }

    /// Try to pop one notification the client has sent us. Returns `None` if
    /// the queue is empty.
    pub fn try_recv_notification(&self) -> Option<ReceivedNotification> {
        self.received_notifications.try_recv().ok()
    }

    /// Number of unread client→server notifications. Useful for "did the
    /// client park" checks.
    pub fn notifications_pending(&self) -> usize {
        self.received_notifications.len()
    }

    /// Trigger the transport's exit signal AND drop the server driver, which
    /// closes the underlying pipes. The `LspClient` crash watchdog observes
    /// both — the closed pipes cause `MainLoop::run_buffered` to return, and
    /// the exit signal resolves the `TransportHandle::exited` future — then
    /// emits [`crate::LspResponse::Crashed`].
    pub fn simulate_exit(&mut self) {
        if let Some(tx) = self.exit_tx.take() {
            let _ = tx.send(());
        }
        // Dropping the driver Task cancels it, which drops the server-side
        // reader/writer halves and closes the piper pipes from below.
        self.driver.take();
    }
}

fn install_default_initialize(handlers: &HandlerMap, capabilities: ServerCapabilities) {
    let capabilities = Arc::new(capabilities);
    let h: HandlerFn = Box::new(move |_params: JsonValue| {
        let caps = capabilities.clone();
        Box::pin(async move {
            let result = InitializeResult {
                capabilities: (*caps).clone(),
                server_info: None,
            };
            Ok(serde_json::to_value(result).expect("serialize InitializeResult"))
        })
    });
    handlers.lock().unwrap().insert(Initialize::METHOD, h);
}

fn install_default_shutdown(handlers: &HandlerMap) {
    let h: HandlerFn = Box::new(|_params: JsonValue| Box::pin(async move { Ok(JsonValue::Null) }));
    handlers.lock().unwrap().insert(Shutdown::METHOD, h);
}

fn build_server_router(
    handlers: HandlerMap,
    notif_tx: async_channel::Sender<ReceivedNotification>,
) -> Router<()> {
    let mut router: Router<()> = Router::new(());

    router.unhandled_request(move |_, req| {
        let lookup = handlers.lock().unwrap().get(req.method.as_str()).map(|h| {
            // Re-clone handler future by re-invoking via the boxed closure.
            // We cannot clone the boxed closure itself; instead we hold the
            // lock just long enough to call it and produce the future.
            h(req.params.clone())
        });
        async move {
            match lookup {
                Some(fut) => fut.await,
                None => Err(ResponseError::new(
                    ErrorCode::METHOD_NOT_FOUND,
                    format!("fake server: no handler for {}", req.method),
                )),
            }
        }
    });

    router.unhandled_notification(move |_, notif| {
        let _ = notif_tx.try_send(ReceivedNotification {
            method: notif.method,
            params: notif.params,
        });
        ControlFlow::Continue(())
    });

    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{LspMessage, LspResponse};
    use crate::LspClient;
    use bevy_tasks::AsyncComputeTaskPool;
    use lsp_types::{ClientCapabilities, Url};
    use std::time::{Duration, Instant};

    fn ensure_pool() {
        AsyncComputeTaskPool::get_or_init(Default::default);
    }

    /// Spin the client's response channel until either `predicate` finds the
    /// response it wants or `timeout` expires. Returns the matched response.
    fn await_response(
        client: &LspClient,
        timeout: Duration,
        mut predicate: impl FnMut(&LspResponse) -> bool,
    ) -> Option<LspResponse> {
        let deadline = Instant::now() + timeout;
        loop {
            while let Some(resp) = client.try_recv() {
                if predicate(&resp) {
                    return Some(resp);
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn initialize_handshake_roundtrips_capabilities() {
        ensure_pool();

        let caps = ServerCapabilities {
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            ..Default::default()
        };

        let (transport, _fake) = FakeLanguageServer::new(caps.clone());

        let mut client = LspClient::new();
        client.start_with(transport);
        client.send(LspMessage::Initialize {
            root_uri: Url::parse("file:///tmp/test").unwrap(),
            capabilities: Box::new(ClientCapabilities::default()),
        });

        let resp = await_response(&client, Duration::from_secs(5), |r| {
            matches!(r, LspResponse::Initialized { .. })
        })
        .expect("initialize never came back");

        let LspResponse::Initialized { capabilities } = resp else {
            unreachable!()
        };
        assert_eq!(capabilities.hover_provider, caps.hover_provider);
    }

    // NOTE: full end-to-end coverage (`client.send` → server, `fake.notify`
    // → client, simulated crash) lives in `crates/bevscode/tests/lsp_integration.rs`.
    // Those tests drive a Bevy `App` whose schedule pumps `drain_lsp_responses`
    // each frame — the same model production uses. The harness only stalls
    // when the test thread itself busy-polls `client.try_recv` from outside
    // the smol pool; under `app.update()`, traffic flows.
}
