//! `droiduse-adp-server`: runs the ADP server on this host.
//!
//! Settings come from flags or the environment (`--help` lists both). Once listening it prints
//! one JSON line to stdout, `{"event":"listening","url":"ws://…/","tokenFile":"…"}`; the log goes
//! to stderr.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use droiduse_contracts_machine::CpuTemplateRef;
use droiduse_entrypoints_adp_server::logging::{self, LogFormat};
use droiduse_entrypoints_adp_server::{ListenAddr, Server, ServerSettings, Services, TraceMode};
use droiduse_runtime_auth::{LocalTokenAuthenticator, OwnerAuthorizer};
use droiduse_runtime_instance::{InstanceSettings, LocalInstanceManager, vmm_machine_factory};
use tracing::level_filters::LevelFilter;

#[derive(Debug, Parser)]
#[command(
    name = "droiduse-adp-server",
    version,
    about = "The ADP server of this host"
)]
struct Args {
    /// TCP address to listen on; port 0 lets the OS choose.
    #[arg(long, env = "DROIDUSE_ADP_LISTEN", default_value = "127.0.0.1:0")]
    listen: SocketAddr,

    /// The server token file; created with a new token when absent.
    /// Default: <user config dir>/droiduse/adp-token.
    #[arg(long, env = "DROIDUSE_ADP_TOKEN_FILE")]
    token_file: Option<PathBuf>,

    /// Trace of a request with a valid traceparent: `new-with-link` or `continue`.
    #[arg(long, env = "DROIDUSE_ADP_TRACE", default_value = "new-with-link", value_parser = parse_trace_mode)]
    trace: TraceMode,

    /// Largest WebSocket message in bytes.
    #[arg(long, env = "DROIDUSE_ADP_MAX_MESSAGE_BYTES", default_value_t = 16 * 1024 * 1024)]
    max_message_bytes: usize,

    /// vCPUs of each instance.
    #[arg(long, env = "DROIDUSE_INSTANCE_VCPUS", default_value_t = 4)]
    vcpus: u8,

    /// Guest memory of each instance (MiB).
    #[arg(long, env = "DROIDUSE_INSTANCE_MEMORY_MIB", default_value_t = 4096)]
    memory_mib: u32,

    /// CPU template of each instance.
    #[arg(
        long,
        env = "DROIDUSE_INSTANCE_CPU_TEMPLATE",
        default_value = "default"
    )]
    cpu_template: String,

    /// Guest boot image. Default: <user data dir>/droiduse/images/boot.img.
    #[arg(long, env = "DROIDUSE_INSTANCE_BOOT_IMAGE")]
    boot_image: Option<PathBuf>,

    /// Guest vendor_boot image. Default: <user data dir>/droiduse/images/vendor_boot.img.
    #[arg(long, env = "DROIDUSE_INSTANCE_VENDOR_BOOT_IMAGE")]
    vendor_boot_image: Option<PathBuf>,

    /// Log format on stderr: `plain` or `json`.
    #[arg(long, env = "DROIDUSE_LOG_FORMAT", default_value = "plain")]
    log_format: LogFormat,

    /// Lowest log level: `off`, `error`, `warn`, `info`, `debug` or `trace`.
    #[arg(long, env = "DROIDUSE_LOG_LEVEL", default_value = "info")]
    log_level: LevelFilter,
}

fn parse_trace_mode(s: &str) -> Result<TraceMode, String> {
    match s {
        "new-with-link" => Ok(TraceMode::NewWithLink),
        "continue" => Ok(TraceMode::Continue),
        _ => Err(format!(
            "unknown trace mode {s:?} (new-with-link or continue)"
        )),
    }
}

fn user_dir(dir: Option<PathBuf>, what: &str) -> Result<PathBuf, String> {
    dir.map(|d| d.join("droiduse"))
        .ok_or_else(|| format!("no user {what} directory is known; set the path explicitly"))
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("droiduse-adp-server: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), String> {
    logging::init(args.log_format, args.log_level, std::io::stderr)?;

    let token_file = match args.token_file {
        Some(p) => p,
        None => user_dir(dirs::config_dir(), "config")?.join("adp-token"),
    };
    let images = || user_dir(dirs::data_dir(), "data").map(|d| d.join("images"));
    let boot_image = match args.boot_image {
        Some(p) => p,
        None => images()?.join("boot.img"),
    };
    let vendor_boot_image = match args.vendor_boot_image {
        Some(p) => p,
        None => images()?.join("vendor_boot.img"),
    };

    let authenticator = LocalTokenAuthenticator::from_token_file(&token_file)
        .map_err(|e| format!("token file {}: {e}", token_file.display()))?;
    let instances = LocalInstanceManager::new(
        vmm_machine_factory(),
        InstanceSettings {
            vcpus: args.vcpus,
            memory_mib: args.memory_mib,
            cpu_template: CpuTemplateRef(args.cpu_template),
            boot_image,
            vendor_boot_image,
        },
    );
    let services = Services {
        authenticator: Arc::new(authenticator),
        authorizer: Arc::new(OwnerAuthorizer),
        instances: Arc::new(instances),
    };
    let settings = ServerSettings {
        trace_mode: args.trace,
        max_message_bytes: args.max_message_bytes,
    };
    let server = Server::bind(&ListenAddr::Tcp(args.listen), services, settings)
        .await
        .map_err(|e| format!("listening on {}: {e}", args.listen))?;
    let url = server.url().map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({ "event": "listening", "url": url, "tokenFile": token_file })
    );
    tracing::info!(%url, "listening");

    tokio::select! {
        () = server.serve() => Ok(()),
        r = tokio::signal::ctrl_c() => r.map_err(|e| e.to_string()),
    }
}
