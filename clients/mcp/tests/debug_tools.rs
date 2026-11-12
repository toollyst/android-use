//! The debug-only adb tools (`--features debug`) of the `droiduse-mcp` binary, driven by an MCP
//! client against a fake adb.
//!
//! This test binary is also the fake adb: started with `FAKE_ADB_LOG` set, it appends its argv
//! (as a JSON array) to that file and answers by `FAKE_ADB_MODE`: `ok` (a 1x1 PNG for
//! `screencap`, nothing otherwise), `fail` (exit 1 with a message), `hang` (sleeps), `notpng`.
//! A Rust binary rather than a script, so it runs the same on Linux, macOS and Windows.
//! `harness = false`: `main` either plays adb or runs the tests below in order.

use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, Instant};

use base64::Engine;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::{ServiceExt, serde_json};
use serde_json::{Value, json};

/// A 1x1 RGBA PNG.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

const SERIAL: &str = "127.0.0.1:5555";

fn fake_adb(log: PathBuf) -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .expect("argv log");
    writeln!(f, "{}", serde_json::to_string(&args).unwrap()).unwrap();
    match std::env::var("FAKE_ADB_MODE").as_deref() {
        Ok("fail") => {
            eprintln!("error: device '{SERIAL}' not found");
            1
        }
        Ok("hang") => {
            std::thread::sleep(Duration::from_secs(120));
            0
        }
        Ok("notpng") => {
            print!("this is not a png");
            0
        }
        _ => {
            if args.iter().any(|a| a == "screencap") {
                std::io::stdout().write_all(PNG_1X1).unwrap();
            }
            0
        }
    }
}

struct Mcp {
    client: RunningService<RoleClient, ()>,
    dir: tempfile::TempDir,
}

impl Mcp {
    /// `droiduse-mcp` with this binary as adb, the fake in `mode`, and `args`.
    async fn start(mode: &str, args: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_droiduse-mcp"));
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("DROIDUSE_") {
                cmd.env_remove(k);
            }
        }
        cmd.arg("--adb-path")
            .arg(std::env::current_exe().unwrap())
            .args(args)
            .env("FAKE_ADB_LOG", dir.path().join("argv.log"))
            .env("FAKE_ADB_MODE", mode)
            .kill_on_drop(true);
        let client = ().serve(TokioChildProcess::new(cmd).unwrap()).await.unwrap();
        Self { client, dir }
    }

    async fn call(&self, tool: &str, args: Option<Value>) -> Result<CallToolResult, String> {
        let mut p = CallToolRequestParams::new(tool.to_owned());
        if let Some(Value::Object(a)) = args {
            p = p.with_arguments(a);
        }
        self.client.call_tool(p).await.map_err(|e| e.to_string())
    }

    /// The argv of every adb run so far.
    fn adb_runs(&self) -> Vec<Vec<String>> {
        let path: &Path = &self.dir.path().join("argv.log");
        match std::fs::read_to_string(path) {
            Ok(text) => text
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    async fn stop(self) {
        let _ = self.client.cancel().await;
    }
}

fn error_message(r: &CallToolResult) -> String {
    assert_eq!(r.is_error, Some(true), "expected a tool error: {r:?}");
    r.structured_content.as_ref().expect("structured error")["message"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn argv(args: &[&str]) -> Vec<Vec<String>> {
    vec![args.iter().map(|s| s.to_string()).collect()]
}

async fn tools_are_listed_and_instance_tools_need_adp() {
    let mcp = Mcp::start("ok", &["--adb-serial", SERIAL]).await;
    let mut names: Vec<String> = mcp
        .client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "debug_screenshot",
            "debug_tap",
            "instance_close",
            "instance_create",
            "instance_list"
        ]
    );
    let r = mcp.call("instance_list", None).await.unwrap();
    assert!(error_message(&r).contains("ADP endpoint not configured"));
    mcp.stop().await;
}

async fn screenshot_returns_the_png_and_its_size() {
    let mcp = Mcp::start("ok", &["--adb-serial", SERIAL]).await;
    let r = mcp.call("debug_screenshot", None).await.unwrap();
    assert_eq!(r.is_error, Some(false), "{r:?}");
    let image = r.content[0].as_image().expect("image content");
    assert_eq!(image.mime_type, "image/png");
    let data = base64::engine::general_purpose::STANDARD
        .decode(&image.data)
        .unwrap();
    assert_eq!(data, PNG_1X1);
    assert_eq!(
        r.structured_content,
        Some(json!({"width": 1, "height": 1, "mimeType": "image/png"}))
    );
    assert_eq!(
        mcp.adb_runs(),
        argv(&["-s", SERIAL, "exec-out", "screencap", "-p"])
    );
    mcp.stop().await;
}

async fn tap_runs_input_tap_with_the_integers() {
    let mcp = Mcp::start("ok", &["--adb-serial", SERIAL]).await;
    let r = mcp
        .call("debug_tap", Some(json!({"x": 10, "y": 2340})))
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(false), "{r:?}");
    assert_eq!(r.structured_content, Some(json!({"x": 10, "y": 2340})));
    assert_eq!(
        mcp.adb_runs(),
        argv(&["-s", SERIAL, "shell", "input", "tap", "10", "2340"])
    );
    mcp.stop().await;
}

async fn tap_rejects_anything_but_non_negative_integers() {
    let mcp = Mcp::start("ok", &["--adb-serial", SERIAL]).await;
    for args in [
        json!({"x": -1, "y": 5}),
        json!({"x": 1.5, "y": 5}),
        json!({"x": "1; reboot", "y": 5}),
        json!({"x": 1}),
    ] {
        let rejected = match mcp.call("debug_tap", Some(args.clone())).await {
            Err(_) => true,
            Ok(r) => r.is_error == Some(true),
        };
        assert!(rejected, "{args} was accepted");
    }
    assert!(mcp.adb_runs().is_empty(), "adb ran: {:?}", mcp.adb_runs());
    mcp.stop().await;
}

async fn adb_failure_is_a_tool_error() {
    let mcp = Mcp::start("fail", &["--adb-serial", SERIAL]).await;
    let m = error_message(&mcp.call("debug_screenshot", None).await.unwrap());
    assert!(m.contains("adb exited") && m.contains("not found"), "{m}");
    let m = error_message(
        &mcp.call("debug_tap", Some(json!({"x": 1, "y": 2})))
            .await
            .unwrap(),
    );
    assert!(m.contains("adb exited"), "{m}");
    mcp.stop().await;
}

async fn output_that_is_not_a_png_is_a_tool_error() {
    let mcp = Mcp::start("notpng", &["--adb-serial", SERIAL]).await;
    let m = error_message(&mcp.call("debug_screenshot", None).await.unwrap());
    assert!(m.contains("not a PNG"), "{m}");
    mcp.stop().await;
}

async fn a_hanging_adb_times_out() {
    let mcp = Mcp::start("hang", &["--adb-serial", SERIAL, "--adb-timeout", "1"]).await;
    let started = Instant::now();
    let m = error_message(&mcp.call("debug_screenshot", None).await.unwrap());
    assert!(m.contains("did not finish within 1 s"), "{m}");
    assert!(started.elapsed() < Duration::from_secs(15));
    mcp.stop().await;
}

async fn without_a_serial_no_device_is_picked() {
    let mcp = Mcp::start("ok", &[]).await;
    let m = error_message(&mcp.call("debug_screenshot", None).await.unwrap());
    assert!(m.contains("--adb-serial"), "{m}");
    assert!(mcp.adb_runs().is_empty());
    mcp.stop().await;
}

type Test = fn() -> Pin<Box<dyn Future<Output = ()>>>;

const TESTS: &[(&str, Test)] = &[
    ("tools_are_listed_and_instance_tools_need_adp", || {
        Box::pin(tools_are_listed_and_instance_tools_need_adp())
    }),
    ("screenshot_returns_the_png_and_its_size", || {
        Box::pin(screenshot_returns_the_png_and_its_size())
    }),
    ("tap_runs_input_tap_with_the_integers", || {
        Box::pin(tap_runs_input_tap_with_the_integers())
    }),
    ("tap_rejects_anything_but_non_negative_integers", || {
        Box::pin(tap_rejects_anything_but_non_negative_integers())
    }),
    ("adb_failure_is_a_tool_error", || {
        Box::pin(adb_failure_is_a_tool_error())
    }),
    ("output_that_is_not_a_png_is_a_tool_error", || {
        Box::pin(output_that_is_not_a_png_is_a_tool_error())
    }),
    ("a_hanging_adb_times_out", || {
        Box::pin(a_hanging_adb_times_out())
    }),
    ("without_a_serial_no_device_is_picked", || {
        Box::pin(without_a_serial_no_device_is_picked())
    }),
];

fn main() {
    if let Some(log) = std::env::var_os("FAKE_ADB_LOG") {
        std::process::exit(fake_adb(PathBuf::from(log)));
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    for (name, test) in TESTS {
        eprint!("test {name} ... ");
        rt.block_on(test());
        eprintln!("ok");
    }
    eprintln!("{} debug tool tests passed", TESTS.len());
}
