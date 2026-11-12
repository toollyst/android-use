//! `cargo xtask <command>`: repo automation entry point.
//!
//! Commands:
//!   check-layers [--root <dir>]   layer dependency rules + forbidden-concept lint
//!   patch-lint [--root <dir>]     upstream patch lint (header, markers, budget);
//!                                 `tools/patch/lint.sh` runs this
//!
//! Exit status: 0 = clean, 1 = violations found, 2 = the check could not run.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>\n\ncommands:\n  check-layers [--root <dir>]   enforce LAYERS.toml / LAYER.toml (dependency rules, forbidden concepts)\n  patch-lint [--root <dir>]     lint third_party/<x>/patches/ (header template, droiduse: markers, budget)";

fn repo_root() -> PathBuf {
    // build/xtask -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Parses `[--root <dir>]`. `Err` carries the exit code for a usage error.
fn parse_root(cmd: &str, args: &[String]) -> Result<PathBuf, ExitCode> {
    let mut root = repo_root();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => match it.next() {
                Some(r) => root = PathBuf::from(r),
                None => {
                    eprintln!("{cmd}: --root needs a directory\n\n{USAGE}");
                    return Err(ExitCode::from(2));
                }
            },
            other => {
                eprintln!("{cmd}: unknown argument `{other}`\n\n{USAGE}");
                return Err(ExitCode::from(2));
            }
        }
    }
    Ok(root)
}

/// Prints the violations and the summary line; maps the outcome to the exit status.
fn finish(cmd: &str, violations: &[String], summary: &str) -> ExitCode {
    for v in violations {
        println!("{v}");
    }
    if violations.is_empty() {
        println!("{cmd}: OK ({summary})");
        ExitCode::SUCCESS
    } else {
        println!(
            "{cmd}: FAILED, {} violation(s) ({summary})",
            violations.len()
        );
        ExitCode::from(1)
    }
}

fn check_layers(args: &[String]) -> ExitCode {
    let root = match parse_root("check-layers", args) {
        Ok(r) => r,
        Err(code) => return code,
    };
    match checklayers::check(&root) {
        Ok(report) => {
            let s = &report.stats;
            for f in &s.skipped_binary {
                eprintln!("check-layers: note: content of non-text file not checked: {f}");
            }
            let violations: Vec<String> = report.violations.iter().map(|v| v.to_string()).collect();
            finish(
                "check-layers",
                &violations,
                &format!(
                    "{} workspace crates, {} files scanned",
                    s.crates_checked, s.files_scanned
                ),
            )
        }
        Err(e) => {
            eprintln!("check-layers: error: {e}");
            ExitCode::from(2)
        }
    }
}

fn patch_lint(args: &[String]) -> ExitCode {
    let root = match parse_root("patch-lint", args) {
        Ok(r) => r,
        Err(code) => return code,
    };
    match patchlint::check(&root) {
        Ok(report) => {
            let violations: Vec<String> = report.violations.iter().map(|v| v.to_string()).collect();
            finish(
                "patch-lint",
                &violations,
                &format!("{} patch file(s) checked", report.patches_checked),
            )
        }
        Err(e) => {
            eprintln!("patch-lint: error: {e}");
            ExitCode::from(2)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((cmd, rest)) if cmd == "check-layers" => check_layers(rest),
        Some((cmd, rest)) if cmd == "patch-lint" => patch_lint(rest),
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
