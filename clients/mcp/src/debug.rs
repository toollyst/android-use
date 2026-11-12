//! DEBUG BUILD ONLY (cargo feature `debug`, CLAUDE.md §1.2): tools that control a device
//! through `adb`. adb-dependent control is never the default path; a default build does not
//! compile this module.
//!
//! - `debug_screenshot`: `adb -s <serial> exec-out screencap -p`, returned as a PNG image.
//! - `debug_tap {x, y}`: `adb -s <serial> shell input tap <x> <y>`.
//!
//! adb runs with separate argv entries, never through a local shell; the only values that
//! reach the device shell are the two integers of `debug_tap`.
//!
//! Settings (a flag wins over the environment variable):
//!   --adb-path <path>      DROIDUSE_ADB_PATH          default `adb` on PATH
//!   --adb-serial <serial>  DROIDUSE_ADB_SERIAL        required by the tools; no device is picked
//!   --adb-timeout <secs>   DROIDUSE_ADB_TIMEOUT_SECS  default 20

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use base64::Engine;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{ErrorData, schemars, tool, tool_router};
use serde_json::json;
use tokio::process::Command;

use crate::Tools;

pub(crate) const USAGE: &str = "debug build: [--adb-path <path>] [--adb-serial <serial>] \
                                [--adb-timeout <secs>] (or DROIDUSE_ADB_PATH, DROIDUSE_ADB_SERIAL, \
                                DROIDUSE_ADB_TIMEOUT_SECS)";

#[derive(Debug, Clone)]
pub(crate) struct AdbSettings {
    path: PathBuf,
    serial: Option<String>,
    timeout: Duration,
}

fn timeout_secs(s: &str) -> Result<Duration, String> {
    match s.parse::<u64>() {
        Ok(n) if n > 0 => Ok(Duration::from_secs(n)),
        _ => Err(format!(
            "adb timeout {s:?} is not a positive number of seconds"
        )),
    }
}

impl AdbSettings {
    pub(crate) fn from_env() -> Result<Self, String> {
        Ok(Self {
            path: std::env::var_os("DROIDUSE_ADB_PATH").map_or_else(|| "adb".into(), PathBuf::from),
            serial: std::env::var("DROIDUSE_ADB_SERIAL").ok(),
            timeout: match std::env::var("DROIDUSE_ADB_TIMEOUT_SECS") {
                Ok(s) => timeout_secs(&s)?,
                Err(_) => Duration::from_secs(20),
            },
        })
    }

    /// Takes `flag` (and its value from `rest`) if it is an adb setting.
    pub(crate) fn flag(
        &mut self,
        flag: &str,
        rest: &mut impl Iterator<Item = String>,
    ) -> Result<bool, String> {
        let mut value = || rest.next().ok_or(format!("{flag} needs a value"));
        match flag {
            "--adb-path" => self.path = PathBuf::from(value()?),
            "--adb-serial" => self.serial = Some(value()?),
            "--adb-timeout" => self.timeout = timeout_secs(&value()?)?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct TapArgs {
    /// X in physical display pixels.
    x: u32,
    /// Y in physical display pixels.
    y: u32,
}

#[tool_router(router = debug_router, vis = "pub(crate)")]
impl Tools {
    #[tool(
        description = "DEBUG BUILD ONLY. A PNG screenshot of the device, taken with `adb exec-out screencap -p`."
    )]
    async fn debug_screenshot(&self) -> Result<CallToolResult, ErrorData> {
        let adb = &self.settings.adb;
        Ok(
            match run(adb, &["exec-out", "screencap", "-p"])
                .await
                .and_then(png)
            {
                Ok((bytes, width, height)) => {
                    let size = json!({"width": width, "height": height, "mimeType": "image/png"});
                    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let mut r = CallToolResult::success(vec![
                        ContentBlock::image(data, "image/png"),
                        ContentBlock::text(size.to_string()),
                    ]);
                    r.structured_content = Some(size);
                    r
                }
                Err(e) => error(e),
            },
        )
    }

    #[tool(
        description = "DEBUG BUILD ONLY. Taps the device at (x, y) in physical display pixels, with `adb shell input tap`."
    )]
    async fn debug_tap(
        &self,
        Parameters(TapArgs { x, y }): Parameters<TapArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        // Integers only: nothing else reaches the device shell.
        let (sx, sy) = (x.to_string(), y.to_string());
        let args = ["shell", "input", "tap", sx.as_str(), sy.as_str()];
        Ok(match run(&self.settings.adb, &args).await {
            Ok(_) => CallToolResult::structured(json!({"x": x, "y": y})),
            Err(e) => error(e),
        })
    }
}

fn error(message: String) -> CallToolResult {
    CallToolResult::structured_error(json!({ "message": message }))
}

/// `adb -s <serial> <args>`; its stdout. Killed when it outlives the timeout.
async fn run(s: &AdbSettings, args: &[&str]) -> Result<Vec<u8>, String> {
    let serial = s
        .serial
        .as_deref()
        .ok_or("no device: start droiduse-mcp with --adb-serial <serial> (see `adb devices`)")?;
    let child = Command::new(&s.path)
        .arg("-s")
        .arg(serial)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("starting {}: {e}", s.path.display()))?;
    // On timeout the child is dropped, which kills it.
    let out = tokio::time::timeout(s.timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("adb did not finish within {} s", s.timeout.as_secs()))?
        .map_err(|e| format!("adb: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stderr: String = stderr.trim().chars().take(500).collect();
        return Err(format!("adb exited with {}: {stderr}", out.status));
    }
    Ok(out.stdout)
}

/// The PNG and its size from the IHDR chunk (PNG §5.2, §11.2.2).
fn png(bytes: Vec<u8>) -> Result<(Vec<u8>, u32, u32), String> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    if bytes.len() < 24 || !bytes.starts_with(SIGNATURE) || &bytes[12..16] != b"IHDR" {
        return Err(format!(
            "the screencap output is not a PNG ({} bytes)",
            bytes.len()
        ));
    }
    let (width, height) = (be(&bytes[16..20]), be(&bytes[20..24]));
    Ok((bytes, width, height))
}
