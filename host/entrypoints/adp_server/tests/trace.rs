//! One trace id on the spans of the server, runtime and vmm for one request, seen in the JSON log.
//! Its own test binary: it installs the global subscriber.

mod common;

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use common::{call, connect_hello, start};
use droiduse_entrypoints_adp_server::TraceMode;
use droiduse_entrypoints_adp_server::logging::{self, LogFormat};
use serde_json::{Value, json};
use tracing::level_filters::LevelFilter;

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Buffer {
    fn lines(&self) -> Vec<Value> {
        let text = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}

const CLIENT_TRACE: &str = "0af7651916cd43dd8448eb211c80319c";
const REQUEST_SPAN: &str = "host/entrypoints/adp_server#request";
const LAYER_SPANS: [&str; 3] = [
    REQUEST_SPAN,
    "host/runtime/instance#create",
    "host/vmm/api#create",
];

/// The request span of a span-close line: the closed span itself (`span`) or one enclosing it
/// (`spans`).
fn request_span(line: &Value) -> Option<&Value> {
    std::iter::once(&line["span"])
        .chain(line["spans"].as_array()?)
        .find(|s| s["name"] == REQUEST_SPAN)
}

/// For each layer span closed under a request whose field `key` is `value`: that request's
/// `trace_id`.
fn trace_ids(lines: &[Value], key: &str, value: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in lines {
        if line["fields"]["message"] != "close" {
            continue;
        }
        let Some(name) = line["span"]["name"].as_str() else {
            continue;
        };
        if !LAYER_SPANS.contains(&name) {
            continue;
        }
        let Some(req) = request_span(line) else {
            continue;
        };
        if req[key] == value {
            out.push((
                name.to_owned(),
                req["trace_id"].as_str().unwrap().to_owned(),
            ));
        }
    }
    out
}

fn create_target(id: u64) -> Value {
    json!({
        "id": id,
        "method": "Target.createTarget",
        "traceparent": format!("00-{CLIENT_TRACE}-b7ad6b7169203331-01"),
    })
}

#[tokio::test]
async fn one_trace_id_across_server_runtime_and_vmm() {
    let buffer = Buffer::default();
    let writer = buffer.clone();
    logging::init(LogFormat::Json, LevelFilter::INFO, move || writer.clone()).unwrap();

    // Default: a new trace, linked to the client's.
    let r = start(TraceMode::NewWithLink).await;
    let mut ws = connect_hello(&r).await;
    let res = call(&mut ws, create_target(2)).await;
    assert_eq!(res["error"]["data"]["reason"], "host/vmm/api#create");

    let lines = buffer.lines();
    let found = trace_ids(&lines, "link_trace_id", CLIENT_TRACE);
    let names: Vec<_> = found.iter().map(|(n, _)| n.as_str()).collect();
    for layer in LAYER_SPANS {
        assert!(
            names.contains(&layer),
            "no closed {layer} span in {lines:#?}"
        );
    }
    let trace = &found[0].1;
    assert_ne!(trace, CLIENT_TRACE);
    assert_eq!(trace.len(), 32);
    assert!(found.iter().all(|(_, t)| t == trace), "{found:?}");

    // Continue: the client's trace.
    let r = start(TraceMode::Continue).await;
    let mut ws = connect_hello(&r).await;
    call(&mut ws, create_target(2)).await;
    let found = trace_ids(&buffer.lines(), "trace_id", CLIENT_TRACE);
    let names: Vec<_> = found.iter().map(|(n, _)| n.as_str()).collect();
    for layer in LAYER_SPANS {
        assert!(
            names.contains(&layer),
            "no closed {layer} span in trace {CLIENT_TRACE}"
        );
    }

    // The token is never in the log, on either authentication path.
    let mut ws = common::connect_with(&r.url, &[]).await.unwrap();
    let res = call(
        &mut ws,
        json!({"id": 1, "method": "Connection.auth", "params": {"token": r.token}}),
    )
    .await;
    assert_eq!(res["result"], json!({}));
    let log = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert!(!log.contains(&r.token));
}
