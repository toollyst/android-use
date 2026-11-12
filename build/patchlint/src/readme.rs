//! The patch table in `third_party/<x>/patches/README.md`, generated from the patch headers.
//!
//! The headers are the single source. The table sits between the [`BEGIN`] and [`END`] marker
//! lines; everything else in the README is left as written. `cargo xtask patch-readme`
//! writes it, and the lint's `[rules] readme` check fails when it is out of date.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{Column, Config};
use crate::{Error, PATCH_DIR, UPSTREAM_ROOT, slash};

/// First line of the generated block.
pub const BEGIN: &str =
    "<!-- BEGIN GENERATED: patch table from the patch headers. Run `cargo xtask patch-readme`. -->";
/// Last line of the generated block.
pub const END: &str = "<!-- END GENERATED: patch table -->";

/// Repo-relative path of the README of upstream `x`.
pub fn readme_path(x: &str) -> String {
    format!("{UPSTREAM_ROOT}/{x}/{PATCH_DIR}/README.md")
}

/// One patch as the table shows it.
pub struct Row {
    /// Path relative to the patch directory, `/`-separated.
    pub path: String,
    /// Header values as written (see `Fields::raw`).
    pub raw: std::collections::BTreeMap<String, String>,
}

fn cell(s: &str) -> String {
    s.trim().replace('\\', "\\\\").replace('|', "\\|")
}

fn header(c: Column) -> &'static str {
    match c {
        Column::Patch => "Patch",
        Column::Summary => "Summary",
        Column::Purpose => "Purpose",
        Column::HookFor => "Hook-for",
        Column::Layer => "Layer",
        Column::Budget => "Budget",
        Column::Upstream => "Upstream",
        Column::RemoveWhen => "Remove-when",
        Column::ScopeCell => "Scope-cell",
    }
}

fn value(c: Column, row: &Row) -> String {
    let field = |k: &str| row.raw.get(k).map(String::as_str).unwrap_or("");
    match c {
        Column::Patch => format!("[{}]({})", cell(&row.path), row.path.replace(' ', "%20")),
        Column::Summary => cell(field("Subject")),
        other => cell(field(header(other))),
    }
}

/// The generated block, marker lines included, `\n`-terminated lines.
pub fn block(cfg: &Config, rows: &[Row]) -> String {
    let mut s = format!("{BEGIN}\n");
    if rows.is_empty() {
        s.push_str("_패치 없음._\n");
    } else {
        let cols = &cfg.readme.columns;
        let line = |cells: Vec<String>| format!("| {} |\n", cells.join(" | "));
        s.push_str(&line(cols.iter().map(|c| header(*c).to_string()).collect()));
        s.push_str(&format!("|{}\n", "---|".repeat(cols.len())));
        for r in rows {
            s.push_str(&line(cols.iter().map(|c| value(*c, r)).collect()));
        }
    }
    s.push_str(END);
    s.push('\n');
    s
}

/// Where the block is in `text` (CRLF already normalized): byte range of the marker lines and
/// what lies between, or an error message.
fn locate(text: &str) -> Result<std::ops::Range<usize>, String> {
    let mut begins = Vec::new();
    let mut ends = Vec::new();
    let mut pos = 0;
    for line in text.split_inclusive('\n') {
        let l = line.strip_suffix('\n').unwrap_or(line);
        if l == BEGIN {
            begins.push(pos);
        } else if l == END {
            ends.push(pos + line.len());
        }
        pos += line.len();
    }
    match (begins.as_slice(), ends.as_slice()) {
        ([b], [e]) if b < e => Ok(*b..*e),
        ([], []) => Err("has no generated patch table; add the two marker lines".into()),
        _ => Err("must contain the begin and end marker lines exactly once, in order".into()),
    }
}

/// `text` with its block replaced by `block` (`text` with `\r\n` normalized to `\n`).
pub fn splice(text: &str, block: &str) -> Result<String, String> {
    let text = text.replace("\r\n", "\n");
    let r = locate(&text)?;
    let mut block = block.to_string();
    // Keep the file's end exactly as it was after the END line.
    if !text[..r.end].ends_with('\n') {
        block.pop();
    }
    Ok(format!("{}{}{}", &text[..r.start], block, &text[r.end..]))
}

/// The patch order: `series` entries first (as listed), then the rest by path.
pub fn order(root: &Path, x: &str, files: &[PathBuf]) -> Vec<String> {
    let dir = format!("{UPSTREAM_ROOT}/{x}/{PATCH_DIR}");
    let mut rest: Vec<String> = files
        .iter()
        .map(|p| slash(p))
        .filter_map(|p| p.strip_prefix(&format!("{dir}/")).map(str::to_string))
        .collect();
    rest.sort();
    let mut out = Vec::new();
    if let Ok(series) = fs::read_to_string(root.join(&dir).join("series")) {
        for line in series.lines() {
            let line = line.split('#').next().unwrap_or("");
            if let Some(p) = line.split_whitespace().last()
                && let Some(i) = rest.iter().position(|r| r == p)
            {
                out.push(rest.remove(i));
            }
        }
    }
    out.extend(rest);
    out
}

/// Reads a README, `None` if it does not exist.
pub fn read(root: &Path, x: &str) -> Result<Option<String>, Error> {
    match fs::read_to_string(root.join(readme_path(x))) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error(format!("cannot read {}: {e}", readme_path(x)))),
    }
}
