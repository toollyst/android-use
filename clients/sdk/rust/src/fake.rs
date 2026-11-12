//! A fake ADP server for tests (feature `test-support`): the answers the real server gives
//! today for the commands that exist, nothing more. Not a server implementation.
//!
//! - Upgrade: `Authorization: Bearer <token>` must match, otherwise HTTP 401.
//! - `Connection.hello`: answers with [`PROTOCOL_VERSION`].
//! - `Target.getTargets`: sends a `Target.targetCreated` event first, then an empty list.
//! - `Target.createTarget`: `UNIMPLEMENTED` with `data.reason` [`CREATE_REASON`].
//! - `Target.closeTarget`: `TARGET_GONE` (no instance exists).
//! - Anything else: `METHOD_NOT_FOUND`.

use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

use droiduse_contracts_adp::PROTOCOL_VERSION;

/// The `data.reason` of the fake's `Target.createTarget` error, as the stub VM layer answers.
pub const CREATE_REASON: &str = "host/vmm/api#create";

#[derive(Default)]
struct Seen {
    authorization: Vec<Option<String>>,
    requests: Vec<Value>,
}

/// A running fake server on `127.0.0.1` (an ephemeral port).
pub struct FakeServer {
    pub url: String,
    seen: Arc<Mutex<Seen>>,
}

impl FakeServer {
    /// Starts a server that accepts `token`. With `reverse_pairs`, the answers to the commands
    /// after hello are held and sent two at a time in reverse order (to test correlation).
    pub async fn start(token: &str, reverse_pairs: bool) -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("ws://{}", listener.local_addr().expect("local address"));
        let seen = Arc::new(Mutex::new(Seen::default()));
        let (expected, s) = (format!("Bearer {token}"), seen.clone());
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let (expected, s) = (expected.clone(), s.clone());
                tokio::spawn(serve(tcp, expected, s, reverse_pairs));
            }
        });
        FakeServer { url, seen }
    }

    /// The `Authorization` header of each upgrade, in order.
    pub fn authorization(&self) -> Vec<Option<String>> {
        self.seen.lock().unwrap().authorization.clone()
    }

    /// Every request received after authentication, as JSON, in order.
    pub fn requests(&self) -> Vec<Value> {
        self.seen.lock().unwrap().requests.clone()
    }
}

// The handshake callback's error type is tungstenite's `ErrorResponse` (an HTTP response).
#[allow(clippy::result_large_err)]
async fn serve(
    tcp: tokio::net::TcpStream,
    expected: String,
    seen: Arc<Mutex<Seen>>,
    reverse_pairs: bool,
) {
    let s = seen.clone();
    let check = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let auth = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let ok = auth.as_deref() == Some(expected.as_str());
        s.lock().unwrap().authorization.push(auth);
        if ok {
            Ok(resp)
        } else {
            let mut e = ErrorResponse::new(None);
            *e.status_mut() = StatusCode::UNAUTHORIZED;
            Err(e)
        }
    };
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(tcp, check).await else {
        return;
    };
    let (mut tx, mut rx) = ws.split();
    let mut held: Vec<Message> = Vec::new();
    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(req) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        seen.lock().unwrap().requests.push(req.clone());
        let id = req["id"].clone();
        let method = req["method"].as_str().unwrap_or_default();
        let error = |code: &str, reason: Option<&str>| {
            let mut e = json!({ "code": code, "message": format!("fake: {method}") });
            if let Some(r) = reason {
                e["data"] = json!({ "reason": r });
            }
            json!({ "id": id, "error": e })
        };
        let answer = match method {
            "Connection.hello" => {
                json!({ "id": id, "result": { "protocolVersion": PROTOCOL_VERSION } })
            }
            "Target.getTargets" => {
                let event = json!({ "method": "Target.targetCreated", "params": { "targetInfo": {
                    "targetId": "fake-instance", "type": "instance", "attached": false } } });
                if tx.send(Message::text(event.to_string())).await.is_err() {
                    return;
                }
                json!({ "id": id, "result": { "targetInfos": [] } })
            }
            "Target.createTarget" => error("UNIMPLEMENTED", Some(CREATE_REASON)),
            "Target.closeTarget" => error("TARGET_GONE", None),
            _ => error("METHOD_NOT_FOUND", None),
        };
        let answer = Message::text(answer.to_string());
        if reverse_pairs && method != "Connection.hello" {
            held.push(answer);
            if held.len() < 2 {
                continue;
            }
        } else {
            held.push(answer);
        }
        while let Some(m) = held.pop() {
            if tx.send(m).await.is_err() {
                return;
            }
        }
    }
}
