//! Rust ADP client (`docs/04-adp.md`), on tokio and tokio-tungstenite, with the generated
//! types of `contracts/adp`.
//!
//! [`Client::connect`] opens the WebSocket with `Authorization: Bearer <token>` on the
//! upgrade and performs the mandatory `Connection.hello`. [`Client::call`] sends a typed
//! connection-scoped command and waits for the response with the same `id`; an ADP error
//! comes back as [`Error::Adp`] with its code, message and `data.reason`. Events are read
//! and ignored (no event or session API yet); binary messages are ignored too.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

pub use droiduse_contracts_adp as adp;
use droiduse_contracts_adp::{Command, PROTOCOL_VERSION, connection, envelope, errors::ErrorCode};
pub use droiduse_contracts_common::TraceParent;
use droiduse_contracts_common::{SpanId, TraceFlags, TraceId};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

#[cfg(any(test, feature = "test-support"))]
pub mod fake;
#[cfg(test)]
mod tests;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<envelope::Response, String>>>>>;

/// An error the server answered with (`envelope::Error`).
#[derive(Debug, Clone, PartialEq)]
pub struct AdpError {
    /// The wire code, for example `UNIMPLEMENTED`. See [`AdpError::known_code`].
    pub code: String,
    pub message: String,
    /// `data.reason`: the component and operation that answered (`host/vmm/api#create`). For
    /// diagnosis; decide by the code.
    pub reason: Option<String>,
    /// The whole `data` object.
    pub data: Option<Map<String, Value>>,
}

impl AdpError {
    /// The code as a code of this protocol version; `None` for a code it does not define.
    pub fn known_code(&self) -> Option<ErrorCode> {
        serde_json::from_value(Value::String(self.code.clone())).ok()
    }
}

impl From<envelope::Error> for AdpError {
    fn from(e: envelope::Error) -> Self {
        let reason = e
            .data
            .as_ref()
            .and_then(|d| d.get("reason"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Self {
            code: e.code,
            message: e.message,
            reason,
            data: e.data,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// Reading the token file, or the system random source.
    Io(std::io::Error),
    /// The WebSocket connection (including an upgrade refused with HTTP 401 or 429).
    WebSocket(tungstenite::Error),
    /// The server answered the command with an error.
    Adp(AdpError),
    /// A message that does not follow the ADP envelope, or a value that cannot be sent.
    Protocol(String),
    /// The connection ended before the response arrived.
    Closed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::WebSocket(e) => write!(f, "websocket: {e}"),
            Error::Adp(e) => {
                write!(f, "{}: {}", e.code, e.message)?;
                if let Some(r) = &e.reason {
                    write!(f, " (reason: {r})")?;
                }
                Ok(())
            }
            Error::Protocol(m) => write!(f, "protocol: {m}"),
            Error::Closed => f.write_str("the connection closed before the response"),
        }
    }
}

impl std::error::Error for Error {}

impl From<tungstenite::Error> for Error {
    fn from(e: tungstenite::Error) -> Self {
        Error::WebSocket(e)
    }
}

/// The token in a token file: its content without surrounding whitespace (a trailing
/// newline is not part of the token).
pub fn read_token_file(path: &Path) -> Result<String, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("token file {}: {e}", path.display()),
        ))
    })?;
    let token = text.trim();
    if token.is_empty() {
        return Err(Error::Protocol(format!(
            "token file {} is empty",
            path.display()
        )));
    }
    Ok(token.to_string())
}

/// A `traceparent` that starts a new trace: random trace-id and parent-id from the system
/// random source, `sampled` set.
pub fn new_traceparent() -> Result<TraceParent, Error> {
    loop {
        let (mut t, mut s) = ([0u8; 16], [0u8; 8]);
        getrandom::fill(&mut t)
            .and_then(|()| getrandom::fill(&mut s))
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        // All zeros is invalid (W3C Trace Context §3.2.2.3, §3.2.2.4): draw again.
        if let (Some(t), Some(s)) = (TraceId::from_bytes(t), SpanId::from_bytes(s)) {
            return Ok(TraceParent::new(t, s, TraceFlags::SAMPLED));
        }
    }
}

/// An authenticated ADP connection after `Connection.hello`.
pub struct Client {
    sink: tokio::sync::Mutex<SplitSink<Ws, Message>>,
    pending: Pending,
    next_id: AtomicI64,
    reader: Option<JoinHandle<()>>,
    server_version: String,
}

impl Client {
    /// Connects to `url` (`ws://127.0.0.1:<port>`), authenticates with `token` on the
    /// upgrade and performs `Connection.hello`. `traceparent` is sent with the hello.
    pub async fn connect(
        url: &str,
        token: &str,
        traceparent: Option<TraceParent>,
    ) -> Result<Client, Error> {
        let mut req = url.into_client_request()?;
        let mut auth = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| Error::Protocol("the token is not a valid header value".into()))?;
        auth.set_sensitive(true);
        req.headers_mut().insert(AUTHORIZATION, auth);
        let (ws, _) = tokio_tungstenite::connect_async(req).await?;
        let (sink, stream) = ws.split();
        let pending = Pending::default();
        let reader = tokio::spawn(read_loop(stream, pending.clone()));
        let mut client = Client {
            sink: tokio::sync::Mutex::new(sink),
            pending,
            next_id: AtomicI64::new(1),
            reader: Some(reader),
            server_version: String::new(),
        };
        let hello = connection::HelloParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
        };
        client.server_version = client
            .call_traced(hello, traceparent)
            .await?
            .protocol_version;
        Ok(client)
    }

    /// The server's ADP version from `Connection.hello`.
    pub fn server_version(&self) -> &str {
        &self.server_version
    }

    /// Sends a connection-scoped command and waits for its response.
    pub async fn call<C: Command>(&self, params: C) -> Result<C::Returns, Error> {
        self.call_traced(params, None).await
    }

    /// [`Client::call`] with a W3C `traceparent` on the request.
    pub async fn call_traced<C: Command>(
        &self,
        params: C,
        traceparent: Option<TraceParent>,
    ) -> Result<C::Returns, Error> {
        let params = match serde_json::to_value(params) {
            Ok(Value::Object(m)) => m,
            Ok(_) => {
                return Err(Error::Protocol(format!(
                    "the params of {} are not a JSON object",
                    C::METHOD
                )));
            }
            Err(e) => return Err(Error::Protocol(e.to_string())),
        };
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = envelope::Request {
            id,
            session_id: None,
            method: C::METHOD.to_string(),
            params: Some(params),
            traceparent: traceparent.map(|t| t.to_string()),
            tracestate: None,
        };
        let text = serde_json::to_string(&req).map_err(|e| Error::Protocol(e.to_string()))?;
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        if let Err(e) = self.sink.lock().await.send(Message::text(text)).await {
            lock(&self.pending).remove(&id);
            return Err(e.into());
        }
        let resp = rx
            .await
            .map_err(|_| Error::Closed)?
            .map_err(Error::Protocol)?;
        match (resp.result, resp.error) {
            (Some(r), None) => serde_json::from_value(Value::Object(r))
                .map_err(|e| Error::Protocol(format!("the result of {}: {e}", C::METHOD))),
            (None, Some(e)) => Err(Error::Adp(e.into())),
            _ => Err(Error::Protocol(format!(
                "the response to {} has not exactly one of result and error",
                C::METHOD
            ))),
        }
    }

    /// Closes the connection (WebSocket close handshake) and waits for it to end.
    pub async fn close(mut self) -> Result<(), Error> {
        self.sink.lock().await.close().await?;
        if let Some(r) = self.reader.take() {
            let _ = r.await;
        }
        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(r) = &self.reader {
            r.abort();
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Hands each response to the call waiting for its id. Events are ignored for now. When the
/// connection ends, the waiting calls get [`Error::Closed`].
async fn read_loop(mut stream: SplitStream<Ws>, pending: Pending) {
    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(id) = v.get("id").and_then(Value::as_i64) else {
            continue; // an Event
        };
        let parsed = serde_json::from_value::<envelope::Response>(v)
            .map_err(|e| format!("a response that is not an ADP Response: {e}"));
        if let Some(tx) = lock(&pending).remove(&id) {
            let _ = tx.send(parsed);
        }
    }
    lock(&pending).clear();
}
