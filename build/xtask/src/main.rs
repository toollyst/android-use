//! `cargo xtask <command>`: repo automation entry point.
//!
//! Commands:
//!   check-layers [--root <dir>]   layer dependency rules + forbidden-concept lint
//!
//! Exit status: 0 = clean, 1 = violations found, 2 = the check could not run.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>\n\ncommands:\n  check-layers [--root <dir>]   enforce LAYERS.toml / LAYER.toml (dependency rules, forbidden concepts)";

fn repo_root() -> PathBuf {
    // build/xtask -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check_layers(args: &[String]) -> ExitCode {
    let mut root = repo_root();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => match it.next() {
                Some(r) => root = PathBuf::from(r),
                None => {
                    eprintln!("check-layers: --root needs a directory\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            other => {
                eprintln!("check-layers: unknown argument `{other}`\n\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    match checklayers::check(&root) {
        Ok(report) => {
            for v in &report.violations {
                println!("{v}");
            }
            let s = &report.stats;
            for f in &s.skipped_binary {
                eprintln!("check-layers: note: content of non-text file not checked: {f}");
            }
            if report.is_clean() {
                println!(
                    "check-layers: OK ({} workspace crates, {} files scanned)",
                    s.crates_checked, s.files_scanned
                );
                ExitCode::SUCCESS
            } else {
                println!(
                    "check-layers: FAILED, {} violation(s) ({} workspace crates, {} files scanned)",
                    report.violations.len(),
                    s.crates_checked,
                    s.files_scanned
                );
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("check-layers: error: {e}");
            ExitCode::from(2)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((cmd, rest)) if cmd == "check-layers" => check_layers(rest),
        Some((cmd, _)) if cmd == "-h" || cmd == "--help" || cmd == "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some((cmd, _)) => {
            eprintln!("xtask: unknown command `{cmd}`\n\n{USAGE}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
