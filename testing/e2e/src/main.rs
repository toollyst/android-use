//! `e2e [--root <dir>] [--manifest <file>] [--report <file>] <target>...`
//!
//! A target is `all`, a milestone (`m0`..`m6`), a tag (`adp`, `mcp`, `trace`, `security`,
//! ...) or a scenario id prefix of whole segments (`adp.auth`). Several targets select the
//! union.
//!
//! Defaults below `--root` (the repo root): the manifest `testing/e2e/scenarios.toml`,
//! the scope matrix `docs/scope.md`, the ADP definition `contracts/adp/protocol.json`, the
//! report `target/e2e/report.json`.
//!
//! Exit status: 0 = no FAIL (PASS and declared RED only), 1 = at least one FAIL,
//! 2 = the run could not start (usage, unreadable or invalid manifest).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use droiduse_testing_e2e::bodies;
use droiduse_testing_e2e::manifest::{self, Reason};
use droiduse_testing_e2e::run::{self, Verdict};

const USAGE: &str = "usage: e2e [--root <dir>] [--manifest <file>] [--report <file>] <target>...\n\
                     targets: all | m0..m6 | <tag> | <scenario id prefix>";

struct Args {
    root: PathBuf,
    manifest: Option<PathBuf>,
    report: Option<PathBuf>,
    targets: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut a = Args {
        root: fs::canonicalize(&root).unwrap_or(root),
        manifest: None,
        report: None,
        targets: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or(format!("{flag} needs a value"));
        match arg.as_str() {
            "--root" => a.root = PathBuf::from(value("--root")?),
            "--manifest" => a.manifest = Some(PathBuf::from(value("--manifest")?)),
            "--report" => a.report = Some(PathBuf::from(value("--report")?)),
            "-h" | "--help" => return Err(String::new()),
            f if f.starts_with('-') => return Err(format!("unknown option `{f}`")),
            t => a.targets.push(t.to_string()),
        }
    }
    Ok(a)
}

fn read(p: &Path) -> Result<String, String> {
    fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(code) => code,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("e2e: error: {e}");
            }
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn main_inner() -> Result<ExitCode, String> {
    let a = parse_args()?;
    let manifest_path = a
        .manifest
        .unwrap_or_else(|| a.root.join("testing/e2e/scenarios.toml"));
    let rows = manifest::scope_rows(&read(&a.root.join("docs/scope.md"))?);
    let m = manifest::load(&read(&manifest_path)?, &rows).map_err(|errs| {
        format!(
            "invalid manifest {}:\n  {}",
            manifest_path.display(),
            errs.join("\n  ")
        )
    })?;
    let selected = run::select(&m.scenarios, &a.targets)?;
    let adp = Arc::new(run::adp_surfaces(&read(
        &a.root.join("contracts/adp/protocol.json"),
    )?)?);
    // A body gets most of the scenario timeout, so that it ends, and kills its child
    // processes, before the runner gives up on it.
    let body_limit =
        Duration::from_secs(m.timeout_s.saturating_mul(3) / 4).max(Duration::from_secs(1));
    let runner: run::Runner = Arc::new(move |s| {
        let env = |v: &str| std::env::var_os(v);
        run::observe_with(s, &adp, &env, &|s| bodies::run(s, &env, body_limit))
    });
    let results = run::run(&selected, Duration::from_secs(m.timeout_s), runner);

    for (s, v) in &results {
        match v {
            Verdict::Pass => println!("PASS {}", s.id),
            Verdict::Red { reason, .. } => println!("RED  {}  {} {reason}", s.id, Reason::CODE),
            Verdict::Fail(msg) => println!("FAIL {}  {msg}", s.id),
        }
    }
    let report = run::report(&a.targets, &results);
    let report_path = a
        .report
        .unwrap_or_else(|| a.root.join("target/e2e/report.json"));
    if let Some(dir) = report_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&report).expect("a JSON value serializes");
    fs::write(&report_path, text + "\n")
        .map_err(|e| format!("cannot write {}: {e}", report_path.display()))?;
    let fails = report["fail"].as_u64().unwrap_or(0);
    println!(
        "e2e: {} scenario(s): {} pass, {} red, {} fail (report: {})",
        results.len(),
        report["pass"],
        report["red"],
        fails,
        report_path.display()
    );
    Ok(if fails == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
