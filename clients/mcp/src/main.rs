//! `droiduse-mcp`: an MCP server over stdio (`docs/04-adp.md` §8) on top of ADP, built on the
//! official MCP Rust SDK (rmcp).
//!
//! Settings (a flag wins over the environment variable):
//!   --adp-url <url>      DROIDUSE_ADP_URL         the ADP server, e.g. ws://127.0.0.1:<port>
//!   --token-file <path>  DROIDUSE_ADP_TOKEN_FILE  the per-user token file (plaintext token)
//!
//! Tools (only for the ADP commands that exist): `instance_list` (Target.getTargets),
//! `instance_create` (Target.createTarget), `instance_close` (Target.closeTarget). Each call
//! reads the token file, opens an ADP connection, runs the command and closes it. Each call
//! starts a trace: a new W3C traceparent goes with every request, and its trace id is logged
//! to stderr (stdout carries MCP only). An ADP error becomes a tool error result with the
//! code, the message and the reason.
//!
//! A build with the cargo feature `debug` adds the debug-only adb tools (`debug.rs`) and their
//! settings, and then starts without an ADP endpoint: the instance tools answer "ADP endpoint
//! not configured" instead. A default build has neither.

#[cfg(feature = "debug")]
mod debug;

use std::path::PathBuf;
use std::process::ExitCode;

use droiduse_sdk::adp::Command;
use droiduse_sdk::adp::target::{
    CloseTargetParams, CreateTargetParams, GetTargetsParams, TargetID,
};
use droiduse_sdk::{Client, Error, new_traceparent, read_token_file};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde_json::{Value, json};

const USAGE: &str = "usage: droiduse-mcp --adp-url <ws://127.0.0.1:port> --token-file <path>\n\
                     (or DROIDUSE_ADP_URL, DROIDUSE_ADP_TOKEN_FILE)";

#[derive(Debug, Clone)]
struct Settings {
    /// The ADP server and the token file. Always set in a default build; a debug build may
    /// run without them (the instance tools then fail).
    adp: Option<(String, PathBuf)>,
    #[cfg(feature = "debug")]
    adb: debug::AdbSettings,
}

fn settings() -> Result<Settings, String> {
    let mut url = std::env::var("DROIDUSE_ADP_URL").ok();
    let mut token = std::env::var_os("DROIDUSE_ADP_TOKEN_FILE").map(PathBuf::from);
    #[cfg(feature = "debug")]
    let mut adb = debug::AdbSettings::from_env()?;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--adp-url" => url = Some(it.next().ok_or("--adp-url needs a value")?),
            "--token-file" => {
                token = Some(PathBuf::from(
                    it.next().ok_or("--token-file needs a value")?,
                ));
            }
            other => {
                #[cfg(feature = "debug")]
                if adb.flag(other, &mut it)? {
                    continue;
                }
                return Err(format!("unknown argument `{other}`"));
            }
        }
    }
    let adp = match (url, token) {
        (Some(url), Some(token)) => Some((url, token)),
        _ if cfg!(feature = "debug") => None,
        (None, _) => return Err("no ADP URL (--adp-url or DROIDUSE_ADP_URL)".to_owned()),
        (_, None) => {
            return Err("no token file (--token-file or DROIDUSE_ADP_TOKEN_FILE)".to_owned());
        }
    };
    Ok(Settings {
        adp,
        #[cfg(feature = "debug")]
        adb,
    })
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct CloseArgs {
    /// The instance target to close, as instance_list or instance_create returns it.
    #[serde(rename = "targetId")]
    target_id: String,
}

#[derive(Clone)]
struct Tools {
    settings: Settings,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Tools {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            #[cfg(not(feature = "debug"))]
            tool_router: Self::tool_router(),
            #[cfg(feature = "debug")]
            tool_router: Self::tool_router() + Self::debug_router(),
        }
    }

    #[tool(description = "List the Android instances (ADP Target.getTargets).")]
    async fn instance_list(&self) -> Result<CallToolResult, ErrorData> {
        Ok(self.run("instance_list", GetTargetsParams {}).await)
    }

    #[tool(
        description = "Create a new Android instance from the server's defaults (ADP Target.createTarget)."
    )]
    async fn instance_create(&self) -> Result<CallToolResult, ErrorData> {
        Ok(self.run("instance_create", CreateTargetParams {}).await)
    }

    #[tool(description = "Stop and destroy an Android instance (ADP Target.closeTarget).")]
    async fn instance_close(
        &self,
        Parameters(args): Parameters<CloseArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = CloseTargetParams {
            target_id: TargetID(args.target_id),
        };
        Ok(self.run("instance_close", params).await)
    }
}

impl Tools {
    /// One tool call: a new trace, one ADP connection, one command.
    async fn run<C: Command>(&self, tool: &str, params: C) -> CallToolResult {
        let Some((adp_url, token_file)) = &self.settings.adp else {
            return CallToolResult::structured_error(json!({
                "message": "ADP endpoint not configured: start droiduse-mcp with --adp-url and --token-file",
            }));
        };
        let tp = match new_traceparent() {
            Ok(tp) => tp,
            Err(e) => return failure(&e),
        };
        eprintln!(
            "droiduse-mcp: {tool}: trace {} ({})",
            tp.trace_id(),
            C::METHOD
        );
        let result = async {
            let token = read_token_file(token_file)?;
            let client = Client::connect(adp_url, &token, Some(tp)).await?;
            let result = client.call_traced(params, Some(tp)).await;
            let _ = client.close().await;
            result
        }
        .await;
        match result {
            Ok(r) => CallToolResult::structured(serde_json::to_value(r).unwrap_or(Value::Null)),
            Err(e) => {
                eprintln!("droiduse-mcp: {tool}: trace {}: {e}", tp.trace_id());
                failure(&e)
            }
        }
    }
}

/// A tool error result. For an ADP error it carries the code, the message and the reason.
fn failure(e: &Error) -> CallToolResult {
    match e {
        Error::Adp(a) => CallToolResult::structured_error(json!({
            "code": a.code,
            "message": a.message,
            "reason": a.reason,
        })),
        other => CallToolResult::structured_error(json!({ "message": other.to_string() })),
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Tools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        )
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let settings = match settings() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("droiduse-mcp: {e}\n{USAGE}");
            #[cfg(feature = "debug")]
            eprintln!("{}", debug::USAGE);
            return ExitCode::from(2);
        }
    };
    #[cfg(feature = "debug")]
    eprintln!(
        "droiduse-mcp: WARNING: debug build: debug_screenshot and debug_tap control a device \
         through adb; not for production"
    );
    let service = match Tools::new(settings).serve(rmcp::transport::stdio()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("droiduse-mcp: MCP initialization failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    match service.waiting().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("droiduse-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
