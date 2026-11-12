//! The log: `tracing` events and span closings, plain text or one JSON object per line.

use std::str::FromStr;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;

/// How log lines are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Human-readable text. The default.
    #[default]
    Plain,
    /// One JSON object per line, with the fields of every enclosing span (`spans`).
    Json,
}

impl FromStr for LogFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "plain" => Ok(Self::Plain),
            "json" => Ok(Self::Json),
            _ => Err(format!("unknown log format {s:?} (plain or json)")),
        }
    }
}

/// Installs the global subscriber: events at `level` and above, and a line when a span closes,
/// written to `writer`. Fails if a global subscriber is already set.
pub fn init<W>(format: LogFormat, level: LevelFilter, writer: W) -> Result<(), String>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    let builder = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(writer);
    match format {
        LogFormat::Plain => builder.with_ansi(false).try_init(),
        LogFormat::Json => builder.json().with_span_list(true).try_init(),
    }
    .map_err(|e| e.to_string())
}
