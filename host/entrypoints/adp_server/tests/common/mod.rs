//! Starts the server in-process on an ephemeral port and talks to it like a client.

#![allow(dead_code)] // each test binary uses a different part

use std::sync::Arc;

use droiduse_contracts_machine::CpuTemplateRef;
use droiduse_entrypoints_adp_server::{ListenAddr, Server, ServerSettings, Services, TraceMode};
use droiduse_runtime_auth::{LocalTokenAuthenticator, OwnerAuthorizer};
use droiduse_runtime_instance::{InstanceSettings, LocalInstanceManager, vmm_machine_factory};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct Running {
    pub url: String,
    pub token: String,
    _dir: tempfile::TempDir,
    task: JoinHandle<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub async fn start(trace_mode: TraceMode) -> Running {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token");
    let authenticator = LocalTokenAuthenticator::from_token_file(&path).unwrap();
    let token = std::fs::read_to_string(&path).unwrap();
    let instances = LocalInstanceManager::new(
        vmm_machine_factory(),
        InstanceSettings {
            vcpus: 1,
            memory_mib: 1024,
            cpu_template: CpuTemplateRef("default".to_owned()),
            boot_image: dir.path().join("boot.img"),
            vendor_boot_image: dir.path().join("vendor_boot.img"),
        },
    );
    let services = Services {
        authenticator: Arc::new(authenticator),
        authorizer: Arc::new(OwnerAuthorizer),
        instances: Arc::new(instances),
    };
    let settings = ServerSettings {
        trace_mode,
        ..ServerSettings::default()
    };
    let server = Server::bind(
        &ListenAddr::Tcp("127.0.0.1:0".parse().unwrap()),
        services,
        settings,
    )
    .await
    .unwrap();
    let url = server.url().unwrap();
    let task = tokio::spawn(server.serve());
    Running {
        url,
        token,
        _dir: dir,
        task,
    }
}

/// Opens a WebSocket, with these extra upgrade headers.
pub async fn connect_with(url: &str, headers: &[(&str, &str)]) -> Result<Ws, tungstenite::Error> {
    let mut req = url.into_client_request().unwrap();
    for (name, value) in headers {
        req.headers_mut().insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    tokio_tungstenite::connect_async(req)
        .await
        .map(|(ws, _)| ws)
}

/// Opens a WebSocket authenticated by the upgrade header.
pub async fn connect(r: &Running) -> Ws {
    let bearer = format!("Bearer {}", r.token);
    connect_with(&r.url, &[("authorization", &bearer)])
        .await
        .unwrap()
}

/// Opens a WebSocket authenticated by the header, and says hello.
pub async fn connect_hello(r: &Running) -> Ws {
    let mut ws = connect(r).await;
    let res = call(
        &mut ws,
        json!({"id": 1, "method": "Connection.hello", "params": {"protocolVersion": "0.1.0"}}),
    )
    .await;
    assert_eq!(res["result"]["protocolVersion"], "0.1.0", "{res}");
    ws
}

/// The next text message.
pub async fn next(ws: &mut Ws) -> Value {
    loop {
        match ws.next().await.expect("connection open").unwrap() {
            Message::Text(t) => return serde_json::from_str(t.as_str()).unwrap(),
            Message::Close(f) => panic!("closed: {f:?}"),
            _ => {}
        }
    }
}

/// Sends a request and returns its response (events before it are skipped).
pub async fn call(ws: &mut Ws, request: Value) -> Value {
    ws.send(Message::text(request.to_string())).await.unwrap();
    loop {
        let m = next(ws).await;
        if m.get("id").is_some() {
            return m;
        }
    }
}

/// The close code the server ends the connection with.
pub async fn close_code(ws: &mut Ws) -> u16 {
    loop {
        match ws.next().await {
            Some(Ok(Message::Close(Some(f)))) => return f.code.into(),
            Some(Ok(_)) => {}
            other => panic!("no close frame: {other:?}"),
        }
    }
}
