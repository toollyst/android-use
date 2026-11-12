//! What the scenario bodies share: starting the real component processes, talking ADP over a
//! WebSocket, reading the server's JSON log. Every child process is killed when its handle is
//! dropped, so a failing or timed-out body leaves none behind.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::manifest::Need;

/// A body's error: the scenario FAILs with this message.
pub type Fallible<T> = Result<T, String>;

/// `Err(message)` unless `cond`.
pub fn check(cond: bool, message: impl FnOnce() -> String) -> Fallible<()> {
    if cond { Ok(()) } else { Err(message()) }
}

/// How long one wire step may take before the body fails.
const STEP: Duration = Duration::from_secs(30);

async fn step<T>(what: &str, f: impl Future<Output = T>) -> Fallible<T> {
    tokio::time::timeout(STEP, f)
        .await
        .map_err(|_| format!("{what}: no answer within {} s", STEP.as_secs()))
}

/// The component's binary, from its `DROIDUSE_E2E_<PATH>` variable (the needs check already
/// made sure it exists).
pub fn binary(env: &dyn Fn(&str) -> Option<OsString>, component: &str) -> Fallible<PathBuf> {
    let var = Need::env_var(component);
    let p = env(&var).ok_or(format!("{var} is not set"))?;
    std::fs::canonicalize(&p).map_err(|e| format!("{var} = {}: {e}", Path::new(&p).display()))
}

/// Collects the lines a child writes to one of its pipes.
#[derive(Clone, Default)]
pub struct Lines(Arc<Mutex<Vec<String>>>);

impl Lines {
    fn collect(&self, pipe: impl AsyncRead + Unpin + Send + 'static) {
        let lines = self.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.0.lock().unwrap().push(line);
            }
        });
    }

    pub fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    /// The lines that are JSON objects.
    pub fn json(&self) -> Vec<Value> {
        self.all()
            .iter()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(Value::is_object)
            .collect()
    }
}

/// A running `droiduse-adp-server` on 127.0.0.1 with an ephemeral port, its own token file and
/// a JSON log on stderr.
pub struct Server {
    _child: Child,
    pub url: String,
    pub token: String,
    pub token_file: PathBuf,
    pub log: Lines,
    _dir: tempfile::TempDir,
}

impl Server {
    pub async fn start(env: &dyn Fn(&str) -> Option<OsString>) -> Fallible<Self> {
        let bin = binary(env, "host/entrypoints/adp_server")?;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let token_file = dir.path().join("adp-token");
        let mut child = Command::new(&bin)
            .arg("--listen")
            .arg("127.0.0.1:0")
            .arg("--token-file")
            .arg(&token_file)
            .arg("--boot-image")
            .arg(dir.path().join("boot.img"))
            .arg("--vendor-boot-image")
            .arg(dir.path().join("vendor_boot.img"))
            .args(["--log-format", "json", "--log-level", "info"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("starting {}: {e}", bin.display()))?;
        let log = Lines::default();
        log.collect(child.stderr.take().expect("piped"));
        let stdout = child.stdout.take().expect("piped");
        let mut stdout = BufReader::new(stdout).lines();
        let first = step("the server's listening line", stdout.next_line())
            .await?
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                format!(
                    "the server exited without a listening line: {:?}",
                    log.all()
                )
            })?;
        let line: Value = serde_json::from_str(&first)
            .map_err(|e| format!("the listening line {first:?} is not JSON: {e}"))?;
        check(line["event"] == "listening", || {
            format!("unexpected first line {first}")
        })?;
        let url = line["url"]
            .as_str()
            .ok_or(format!("no url in {first}"))?
            .to_owned();
        let token = std::fs::read_to_string(&token_file)
            .map_err(|e| format!("reading the token file: {e}"))?
            .trim_end()
            .to_owned();
        Ok(Self {
            _child: child,
            url,
            token,
            token_file,
            log,
            _dir: dir,
        })
    }

    /// The `Authorization` header of the server's token.
    pub fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// A connection authenticated by the upgrade header, after a successful hello.
    pub async fn ready(&self) -> Fallible<Ws> {
        let mut ws = connect(&self.url, &[("authorization", &self.bearer())])
            .await
            .map_err(|e| format!("upgrade with the token: {e}"))?;
        let res = call(&mut ws, hello(1, crate_version())).await?;
        check(res.get("result").is_some(), || {
            format!("hello failed: {res}")
        })?;
        Ok(ws)
    }
}

/// The ADP version this runner was built against.
pub fn crate_version() -> &'static str {
    droiduse_contracts_adp::PROTOCOL_VERSION
}

pub fn hello(id: i64, version: &str) -> Value {
    serde_json::json!({"id": id, "method": "Connection.hello", "params": {"protocolVersion": version}})
}

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Opens a WebSocket to `url` with these extra upgrade headers.
pub async fn connect(url: &str, headers: &[(&str, &str)]) -> Result<Ws, tungstenite::Error> {
    let mut req = url.into_client_request()?;
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes()).expect("a valid header name");
        let value = HeaderValue::from_str(value).expect("a valid header value");
        req.headers_mut().insert(name, value);
    }
    tokio_tungstenite::connect_async(req)
        .await
        .map(|(ws, _)| ws)
}

/// The HTTP status of a refused upgrade.
pub fn refused_status(
    r: Result<Ws, tungstenite::Error>,
) -> Fallible<tungstenite::http::Response<Option<Vec<u8>>>> {
    match r {
        Err(tungstenite::Error::Http(resp)) => Ok(*resp),
        Err(e) => Err(format!("expected an HTTP refusal, got {e}")),
        Ok(_) => Err("expected an HTTP refusal, the upgrade succeeded".to_owned()),
    }
}

/// Sends one text message.
pub async fn send(ws: &mut Ws, text: String) -> Fallible<()> {
    step("send", ws.send(Message::text(text)))
        .await?
        .map_err(|e| format!("send: {e}"))
}

/// The next text message, as JSON. A close or the end of the connection is an error.
pub async fn next(ws: &mut Ws) -> Fallible<Value> {
    loop {
        match step("the next message", ws.next()).await? {
            Some(Ok(Message::Text(t))) => {
                return serde_json::from_str(t.as_str()).map_err(|e| format!("not JSON: {e}"));
            }
            Some(Ok(Message::Close(f))) => return Err(format!("closed by the server: {f:?}")),
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(format!("connection error: {e}")),
            None => return Err("the connection ended".to_owned()),
        }
    }
}

/// Sends a request and returns its response (events before it are skipped).
pub async fn call(ws: &mut Ws, request: Value) -> Fallible<Value> {
    send(ws, request.to_string()).await?;
    loop {
        let m = next(ws).await?;
        if m.get("id").is_some() {
            return Ok(m);
        }
    }
}

/// The status code of the close frame the server ends the connection with.
pub async fn close_code(ws: &mut Ws) -> Fallible<u16> {
    loop {
        match step("the close frame", ws.next()).await? {
            Some(Ok(Message::Close(Some(f)))) => return Ok(f.code.into()),
            Some(Ok(Message::Close(None))) => return Err("a close frame without a code".to_owned()),
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(format!("connection error before a close frame: {e}")),
            None => return Err("the connection ended without a close frame".to_owned()),
        }
    }
}

/// `error.code` of a response, or an error if it is not an error response.
pub fn error_code(res: &Value) -> &str {
    res["error"]["code"].as_str().unwrap_or("<no error>")
}

/// A running `droiduse-mcp` speaking MCP over stdio, as an MCP client starts it; its stderr is
/// collected.
pub struct Mcp {
    pub client: rmcp::service::RunningService<rmcp::RoleClient, ()>,
    pub stderr: Lines,
}

impl Mcp {
    pub async fn start(
        env: &dyn Fn(&str) -> Option<OsString>,
        adp_url: &str,
        token_file: &Path,
    ) -> Fallible<Self> {
        use rmcp::ServiceExt;
        let bin = binary(env, "clients/mcp")?;
        let mut cmd = Command::new(&bin);
        cmd.arg("--adp-url")
            .arg(adp_url)
            .arg("--token-file")
            .arg(token_file)
            .kill_on_drop(true);
        let (transport, stderr) = rmcp::transport::TokioChildProcess::builder(cmd)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("starting {}: {e}", bin.display()))?;
        let lines = Lines::default();
        if let Some(stderr) = stderr {
            lines.collect(stderr);
        }
        let client = step("the MCP initialize handshake", ().serve(transport))
            .await?
            .map_err(|e| format!("MCP initialize: {e}"))?;
        Ok(Self {
            client,
            stderr: lines,
        })
    }
}
