//! `tools/patch/lint.sh` (`cargo xtask patch-lint`): lints our upstream patches.
//!
//! Scope: every file under `third_party/<x>/patches/` except `series` and `README.md` at the
//! top of that directory (docs/08-upstream-and-patches.md §2 "디렉터리"). Each one is linted
//! as a patch, whatever its extension, so nothing escapes by its name. `third_party/<x>/`
//! directories other than `patches/` (e.g. `aosp/security/` ASB backports) are not ours.
//!
//! Rules (docs/08 §2 and CLAUDE.md §5), each failing the run:
//!
//! - **Header template**: `From:` and `Subject: [PATCH] droiduse: <summary>` email headers,
//!   and `Purpose:`, `Hook-for:`, `Layer:`, `Budget:`, `Upstream:`, `Remove-when:`,
//!   `Scope-cell:` in the commit message (before the `---` separator), each present once
//!   and well-formed.
//! - **Marker**: every hunk adds at least one line containing `droiduse:`.
//! - **Budget**: the patch changes (adds + removes) at most `Budget:` lines, over all its
//!   files and hunks.
//!
//! Headers are parsed with `mailparse` (RFC 5322 folding, RFC 2047 encoded words as written
//! by `git format-patch`), diffs with `diffy` (git extended diff format).

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use diffy::Line;
use diffy::patch_set::{FileOperation, ParseOptions, PatchKind, PatchSet};

/// Directory holding upstream code (docs/08 §2: `third_party/<x>/patches/`).
pub const UPSTREAM_ROOT: &str = "third_party";
/// Patch directory inside each upstream directory.
pub const PATCH_DIR: &str = "patches";
/// Files at the top of a patch directory that are not patches.
pub const NON_PATCH_FILES: &[&str] = &["series", "README.md"];

/// The marker every hunk must add (docs/08 §2.3).
pub const MARKER: &str = "droiduse:";
/// Required `Subject:` prefix (docs/08 header template).
pub const SUBJECT_PREFIX: &str = "[PATCH] droiduse: ";
/// `Layer:` values (docs/08 header template: `<guest | vmm>`).
pub const LAYERS: &[&str] = &["guest", "vmm"];

/// Commit-message fields of the header template, in template order.
pub const MESSAGE_FIELDS: &[&str] = &[
    "Purpose",
    "Hook-for",
    "Layer",
    "Budget",
    "Upstream",
    "Remove-when",
    "Scope-cell",
];

/// A fatal problem: the lint could not run. Distinct from violations.
#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// A required header field is absent.
    HeaderMissing,
    /// A header field is malformed, an unfilled template placeholder, or repeated.
    HeaderInvalid,
    /// A hunk adds no line containing `droiduse:`.
    Marker,
    /// More changed lines than `Budget:`.
    Budget,
    /// The file is not a parseable text patch.
    Parse,
    /// Something in a patch directory that is not a regular file (symlink, ...).
    File,
    /// A file could not be read.
    Unreadable,
}

impl Rule {
    pub fn id(self) -> &'static str {
        match self {
            Rule::HeaderMissing => "patch/header-missing",
            Rule::HeaderInvalid => "patch/header-invalid",
            Rule::Marker => "patch/marker",
            Rule::Budget => "patch/budget",
            Rule::Parse => "patch/parse",
            Rule::File => "patch/file",
            Rule::Unreadable => "io/unreadable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// Repo-relative path, `/`-separated.
    pub location: String,
    pub rule: Rule,
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.location, self.rule.id(), self.message)
    }
}

#[derive(Debug)]
pub struct Report {
    pub violations: Vec<Violation>,
    pub patches_checked: usize,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

fn slash(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Lints every patch of the repo at `root`.
pub fn check(root: &Path) -> Result<Report, Error> {
    let upstream = root.join(UPSTREAM_ROOT);
    let meta = fs::symlink_metadata(&upstream)
        .map_err(|e| Error(format!("cannot read {}: {e}", upstream.display())))?;
    if !meta.is_dir() {
        return Err(Error(format!("{} is not a directory", upstream.display())));
    }
    let mut violations = Vec::new();
    let mut files = Vec::new();
    for x in sorted_entries(&upstream)? {
        let dir = upstream.join(&x).join(PATCH_DIR);
        match fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() => {
                let rel = PathBuf::from(UPSTREAM_ROOT).join(&x).join(PATCH_DIR);
                collect(root, &rel, true, &mut files, &mut violations)?;
            }
            Ok(_) => violations.push(Violation {
                location: slash(&PathBuf::from(UPSTREAM_ROOT).join(&x).join(PATCH_DIR)),
                rule: Rule::File,
                message: "a patch directory must be a real directory (not a symlink or file)"
                    .into(),
            }),
            Err(_) => {}
        }
    }
    let patches_checked = files.len();
    for rel in files {
        let location = slash(&rel);
        match fs::read(root.join(&rel)) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => {
                    for (rule, message) in lint_patch(&text) {
                        violations.push(Violation {
                            location: location.clone(),
                            rule,
                            message,
                        });
                    }
                }
                Err(_) => violations.push(Violation {
                    location,
                    rule: Rule::Parse,
                    message: "not UTF-8 text; patches must be text".into(),
                }),
            },
            Err(e) => violations.push(Violation {
                location,
                rule: Rule::Unreadable,
                message: format!("cannot read: {e}"),
            }),
        }
    }
    violations.sort();
    violations.dedup();
    Ok(Report {
        violations,
        patches_checked,
    })
}

fn sorted_entries(dir: &Path) -> Result<Vec<String>, Error> {
    let rd = fs::read_dir(dir).map_err(|e| Error(format!("cannot read {}: {e}", dir.display())))?;
    let mut names = Vec::new();
    for e in rd {
        let e = e.map_err(|e| Error(format!("cannot read {}: {e}", dir.display())))?;
        names.push(e.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// Collects the patch files under `rel` (repo-relative). Symlinks and other non-regular
/// entries are violations: their content cannot be attributed to this directory.
fn collect(
    root: &Path,
    rel: &Path,
    top: bool,
    files: &mut Vec<PathBuf>,
    out: &mut Vec<Violation>,
) -> Result<(), Error> {
    for name in sorted_entries(&root.join(rel))? {
        let child = rel.join(&name);
        let meta = fs::symlink_metadata(root.join(&child))
            .map_err(|e| Error(format!("cannot read {}: {e}", child.display())))?;
        if meta.is_dir() {
            collect(root, &child, false, files, out)?;
        } else if meta.is_file() {
            if !(top && NON_PATCH_FILES.contains(&name.as_str())) {
                files.push(child);
            }
        } else {
            out.push(Violation {
                location: slash(&child),
                rule: Rule::File,
                message:
                    "only regular files may be in a patch directory (symlinks are not followed)"
                        .into(),
            });
        }
    }
    Ok(())
}

/// Lints the text of one patch file. Returns `(rule, message)` pairs.
pub fn lint_patch(text: &str) -> Vec<(Rule, String)> {
    let mut out = Vec::new();
    let budget = check_header(text, &mut out);
    check_diff(text, budget, &mut out);
    out
}

/// A value still holding the template's `<...>` placeholder, or empty.
fn unfilled(v: &str) -> bool {
    v.is_empty() || (v.starts_with('<') && v.ends_with('>'))
}

/// `Hook-for:`: one repo-relative path (`/`-separated, no `.`/`..`/empty components).
fn is_repo_path(v: &str) -> bool {
    !v.starts_with('/')
        && !v.contains('\\')
        && !v.chars().any(char::is_whitespace)
        && v.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// `Upstream:`: `n/a`, or `pending ` followed by an absolute `https://` link.
fn is_upstream(v: &str) -> bool {
    if v == "n/a" {
        return true;
    }
    v.strip_prefix("pending ").is_some_and(|link| {
        link.strip_prefix("https://")
            .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'))
            && !link.chars().any(char::is_whitespace)
    })
}

/// Checks the header template. Returns the budget if `Budget:` is valid.
fn check_header(text: &str, out: &mut Vec<(Rule, String)>) -> Option<usize> {
    let missing = |f: &str| {
        (
            Rule::HeaderMissing,
            format!(
                "required header field `{f}:` is missing (template: docs/08-upstream-and-patches.md §2)"
            ),
        )
    };
    let invalid = |f: &str, why: String| (Rule::HeaderInvalid, format!("`{f}:` {why}"));

    // `git format-patch` starts with an mbox `From <sha> <date>` line; it is not a header.
    let start = if text.starts_with("From ") {
        text.find('\n').map_or(text.len(), |i| i + 1)
    } else {
        0
    };
    let (from, subject, body_start) = match mailparse::parse_headers(&text.as_bytes()[start..]) {
        Ok((headers, off)) => {
            let values = |key: &str| -> Vec<String> {
                headers
                    .iter()
                    .filter(|h| h.get_key().eq_ignore_ascii_case(key))
                    .map(|h| h.get_value().trim().to_string())
                    .collect()
            };
            (values("From"), values("Subject"), start + off)
        }
        Err(e) => {
            out.push((
                Rule::HeaderInvalid,
                format!("the email header block (`From:`, `Subject:`) cannot be parsed: {e}"),
            ));
            (Vec::new(), Vec::new(), start)
        }
    };
    let single = |name: &str, vals: &[String], out: &mut Vec<(Rule, String)>| -> Option<String> {
        match vals {
            [] => {
                out.push(missing(name));
                None
            }
            [v] => Some(v.clone()),
            _ => {
                out.push(invalid(name, format!("appears {} times", vals.len())));
                None
            }
        }
    };
    if let Some(v) = single("From", &from, out)
        && v.is_empty()
    {
        out.push(invalid("From", "is empty".into()));
    }
    if let Some(v) = single("Subject", &subject, out) {
        match v.strip_prefix(SUBJECT_PREFIX) {
            Some(summary) if !unfilled(summary.trim()) => {}
            _ => out.push(invalid(
                "Subject",
                format!("must be `{SUBJECT_PREFIX}<한 줄 요약>`, found `{v}`"),
            )),
        }
    }

    // Commit-message fields: from the end of the email headers to the `---` separator
    // (or the first diff line if there is none).
    let body = text.get(body_start..).unwrap_or("");
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        if line == "---"
            || line.starts_with("diff --git ")
            || line.starts_with("--- ")
            || line.starts_with("@@ ")
        {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && MESSAGE_FIELDS.contains(&k)
        {
            fields.push((k.to_string(), v.trim().to_string()));
        }
    }
    let mut budget = None;
    for f in MESSAGE_FIELDS {
        let vals: Vec<String> = fields
            .iter()
            .filter(|(k, _)| k == f)
            .map(|(_, v)| v.clone())
            .collect();
        let Some(v) = single(f, &vals, out) else {
            continue;
        };
        if unfilled(&v) {
            out.push(invalid(
                f,
                format!("is empty or an unfilled template placeholder (`{v}`)"),
            ));
            continue;
        }
        match *f {
            "Hook-for" if !is_repo_path(&v) => out.push(invalid(
                f,
                format!("must be one repo-relative module path like `guest/vendor/services/inspector`, found `{v}`"),
            )),
            "Layer" if !LAYERS.contains(&v.as_str()) => out.push(invalid(
                f,
                format!("must be one of {}, found `{v}`", LAYERS.join(" | ")),
            )),
            "Budget" => match v.parse::<usize>() {
                Ok(n) if n > 0 && v.bytes().all(|b| b.is_ascii_digit()) => budget = Some(n),
                _ => out.push(invalid(
                    f,
                    format!("must be a positive whole number of lines, found `{v}`"),
                )),
            },
            "Upstream" if !is_upstream(&v) => out.push(invalid(
                f,
                format!("must be `pending https://<link>` or `n/a`, found `{v}`"),
            )),
            _ => {}
        }
    }
    budget
}

fn file_name(op: &FileOperation<'_, str>) -> String {
    match op {
        FileOperation::Delete(p) | FileOperation::Create(p) => p.to_string(),
        FileOperation::Modify { modified, .. } => modified.to_string(),
        FileOperation::Rename { to, .. } | FileOperation::Copy { to, .. } => to.to_string(),
    }
}

/// Checks markers and the budget on the diff part.
fn check_diff(text: &str, budget: Option<usize>, out: &mut Vec<(Rule, String)>) {
    let mut changed = 0usize;
    for fp in PatchSet::parse(text, ParseOptions::gitdiff()) {
        let fp = match fp {
            Ok(fp) => fp,
            Err(e) => {
                out.push((Rule::Parse, format!("not a parseable patch: {e}")));
                return;
            }
        };
        let name = file_name(fp.operation());
        let PatchKind::Text(patch) = fp.patch() else {
            out.push((
                Rule::Parse,
                format!("binary change to `{name}`: it has no hunks to carry a `{MARKER}` marker"),
            ));
            continue;
        };
        if patch.hunks().is_empty() {
            out.push((
                Rule::Marker,
                format!("change to `{name}` has no hunk to carry a `{MARKER}` marker"),
            ));
        }
        for (i, h) in patch.hunks().iter().enumerate() {
            let mut marked = false;
            for l in h.lines() {
                match l {
                    Line::Insert(s) => {
                        changed += 1;
                        marked |= s.contains(MARKER);
                    }
                    Line::Delete(_) => changed += 1,
                    Line::Context(_) => {}
                }
            }
            if !marked {
                let (o, n) = (h.old_range(), h.new_range());
                out.push((
                    Rule::Marker,
                    format!(
                        "hunk {} (@@ -{},{} +{},{} @@) of `{name}` adds no line with a `{MARKER}` marker",
                        i + 1,
                        o.start(),
                        o.len(),
                        n.start(),
                        n.len()
                    ),
                ));
            }
        }
    }
    if let Some(b) = budget
        && changed > b
    {
        out.push((
            Rule::Budget,
            format!("{changed} changed lines exceed `Budget:` {b} (added + removed, all hunks)"),
        ));
    }
}
