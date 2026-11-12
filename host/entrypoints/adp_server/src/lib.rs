//! host/entrypoints/adp_server: the ADP WebSocket JSON-RPC server (`docs/04-adp.md` §4,
//! `contracts/adp/protocol.json`).
//!
//! - [`Server::bind`] opens a listener ([`ListenAddr`]; TCP for now) and [`Server::serve`]
//!   accepts connections. Each connection is an HTTP/1.1 upgrade (hyper) to a WebSocket
//!   (tokio-tungstenite). The upgrade is authenticated before the 101 when it carries
//!   `Authorization: Bearer`; otherwise the first message must be `Connection.auth`.
//! - Then `Connection.hello` is required once. The commands built so far are
//!   `Target.getTargets`, `Target.createTarget` and `Target.closeTarget`, over the injected
//!   [`InstanceManager`]; every other command of the protocol answers `UNIMPLEMENTED` with the
//!   reason `host/entrypoints/adp_server#<method>`.
//! - Every request runs in a span `host/entrypoints/adp_server#request` that carries the
//!   request's `trace_id`, so the spans of the layers below it are in the same trace.
//!
//! Not built yet: other listener kinds (Unix socket, named pipe), the failed-authentication rate
//! limit, the authentication deadline, keepalive pings, binary messages and uploads, sessions,
//! target events to other connections, concurrent requests on one connection (requests are
//! answered one after another, so no two are in flight).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod connection;
mod dispatch;
mod http;
pub mod logging;
mod trace;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use droiduse_contracts_instance::InstanceManager;
use droiduse_contracts_spi::{Authenticator, Authorizer, ChannelCtx, Transport};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

/// The component name in `Error::reason`.
pub(crate) const COMPONENT: &str = "host/entrypoints/adp_server";

/// Where the server listens.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ListenAddr {
    /// A TCP address. Port 0 lets the OS choose one.
    Tcp(SocketAddr),
}

/// Whether a request's valid `traceparent` is continued or only linked
/// (`docs/04-adp.md` §4; a server setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceMode {
    /// A new trace for every request; a valid client `traceparent` is recorded as a link
    /// (`link_trace_id`). The default.
    #[default]
    NewWithLink,
    /// The trace of a valid client `traceparent` is continued; a new trace otherwise.
    Continue,
}

/// Server settings.
#[derive(Debug, Clone)]
pub struct ServerSettings {
    /// See [`TraceMode`].
    pub trace_mode: TraceMode,
    /// Largest WebSocket message, and frame, in bytes (`docs/04-adp.md` §4: default 16 MiB).
    pub max_message_bytes: usize,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            trace_mode: TraceMode::default(),
            max_message_bytes: 16 * 1024 * 1024,
        }
    }
}

/// What the server calls: injected by whoever starts it.
#[derive(Clone)]
pub struct Services {
    /// Verifies credentials.
    pub authenticator: Arc<dyn Authenticator>,
    /// Decides every command.
    pub authorizer: Arc<dyn Authorizer>,
    /// The instances of this host.
    pub instances: Arc<dyn InstanceManager>,
}

/// State shared by every connection.
pub(crate) struct Shared {
    pub(crate) services: Services,
    pub(crate) settings: ServerSettings,
}

/// A bound server.
pub struct Server {
    listener: Listener,
    shared: Arc<Shared>,
}

/// The listeners, one variant per kind ([`ListenAddr`]).
enum Listener {
    Tcp(TcpListener),
}

impl Server {
    /// Binds `addr`.
    pub async fn bind(
        addr: &ListenAddr,
        services: Services,
        settings: ServerSettings,
    ) -> io::Result<Self> {
        let listener = match addr {
            ListenAddr::Tcp(a) => Listener::Tcp(TcpListener::bind(a).await?),
        };
        Ok(Self {
            listener,
            shared: Arc::new(Shared { services, settings }),
        })
    }

    /// The WebSocket URL clients connect to.
    pub fn url(&self) -> io::Result<String> {
        match &self.listener {
            Listener::Tcp(l) => Ok(format!("ws://{}/", l.local_addr()?)),
        }
    }

    /// Accepts and serves connections until the future is dropped.
    pub async fn serve(self) {
        loop {
            match &self.listener {
                Listener::Tcp(l) => match l.accept().await {
                    Ok((stream, peer)) => {
                        let ctx = ChannelCtx::new(Transport::Tcp { peer });
                        tokio::spawn(http::serve(TokioIo::new(stream), ctx, self.shared.clone()));
                    }
                    Err(e) => {
                        // Out of file descriptors and the like: wait, then accept again.
                        tracing::warn!(error = %e, "accepting a connection failed");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                },
            }
        }
    }
}
