//! `tools/patch/lint.sh` (`cargo xtask patch-lint`): lints our upstream patches.
//!
//! Scope: every file under `third_party/<x>/patches/` except `series` and `README.md` at the
//! top of that directory (docs/08-upstream-and-patches.md §2 "디렉터리"). Each one is linted
//! as a patch, whatever its name, so nothing escapes by its name. Other directories under
//! `third_party/<x>/` (e.g. `aosp/security/` ASB backports) are not ours.
//!
//! Policy: `third_party/patches.toml` (see `config.rs`; omitted keys take the defaults).
//!
//! Always checked (docs/08 §2, CLAUDE.md §5):
//! - **Header template**: `From:`, `Subject: [PATCH] droiduse: <summary>` email headers, and
//!   `Purpose:`, `Hook-for:`, `Layer:`, `Budget:`, `Upstream:`, `Remove-when:`,
//!   `Scope-cell:` in the commit message (before `---`), each once and well-formed.
//! - **Marker**: every hunk adds a line containing `droiduse:`.
//! - **Budget**: changed lines (per `[budget] count`) <= `Budget:`; a `Budget:` above the cap
//!   needs an approved overrun (`[budget] overrun`).
//!
//! Switchable rules (`[rules]`, all on by default): file name, `series` consistency, `Layer`
//! per upstream, `Hook-for` exists, `Scope-cell` is a `docs/scope.md` row.
//!
//! Headers are parsed with `mailparse` (RFC 5322 folding, RFC 2047 encoded words as written
//! by `git format-patch`), diffs with `diffy` (git extended diff format).

pub mod config;
pub mod owners;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use diffy::Line;
use diffy::patch_set::{FileOperation, ParseOptions, PatchKind, PatchSet};

use config::{Config, Count, Overrun, RemovalOnly, SeriesFormat};

/// Directory holding upstream code (docs/08 §2: `third_party/<x>/patches/`).
pub const UPSTREAM_ROOT: &str = "third_party";
/// Patch directory inside each upstream directory.
pub const PATCH_DIR: &str = "patches";
/// Files at the top of a patch directory that are not patches.
pub const NON_PATCH_FILES: &[&str] = &["series", "README.md"];
/// The scope matrix `Scope-cell:` refers to.
pub const SCOPE_FILE: &str = "docs/scope.md";

/// The marker every hunk must add (docs/08 §2.3).
pub const MARKER: &str = "droiduse:";
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
/// Optional commit-message field recording who approved a `Budget:` above the cap.
pub const APPROVAL_FIELD: &str = "Budget-approved-by";

/// A fatal problem: the lint could not run (including configuration errors).
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
    /// `Budget:` above the cap without an approved overrun.
    BudgetOverrun,
    /// File name is not `NNNN-droiduse-<name>.patch`.
    Name,
    /// `series` and the patch files disagree.
    Series,
    /// `Layer:` is not the one configured for the upstream.
    Layer,
    /// `Hook-for:` does not exist in the repo.
    HookFor,
    /// `Scope-cell:` is not a row of docs/scope.md.
    ScopeCell,
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
            Rule::BudgetOverrun => "patch/budget-overrun",
            Rule::Name => "patch/name",
            Rule::Series => "patch/series",
            Rule::Layer => "patch/layer",
            Rule::HookFor => "patch/hook-for",
            Rule::ScopeCell => "patch/scope-cell",
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

fn violation(location: &str, rule: Rule, message: String) -> Violation {
    Violation {
        location: location.to_string(),
        rule,
        message,
    }
}

/// Lints every patch of the repo at `root`.
pub fn check(root: &Path) -> Result<Report, Error> {
    let upstream = root.join(UPSTREAM_ROOT);
    let meta = fs::symlink_metadata(&upstream)
        .map_err(|e| Error(format!("cannot read {}: {e}", upstream.display())))?;
    if !meta.is_dir() {
        return Err(Error(format!("{} is not a directory", upstream.display())));
    }
    let cfg = config::load(root)?;
    let scope = if cfg.rules.scope_cell_exists {
        Some(load_scope(root)?)
    } else {
        None
    };

    let mut violations = Vec::new();
    let mut per_upstream: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for x in sorted_entries(&upstream)? {
        let rel = PathBuf::from(UPSTREAM_ROOT).join(&x).join(PATCH_DIR);
        match fs::symlink_metadata(root.join(&rel)) {
            Ok(m) if m.is_dir() => {
                if !cfg.upstream.contains_key(&x) {
                    return Err(Error(format!(
                        "{}: {} exists but has no [upstream.{x}] entry",
                        config::CONFIG_FILE,
                        slash(&rel)
                    )));
                }
                let mut files = Vec::new();
                collect(root, &rel, true, &mut files, &mut violations)?;
                per_upstream.insert(x, files);
            }
            Ok(_) => violations.push(violation(
                &slash(&rel),
                Rule::File,
                "a patch directory must be a real directory (not a symlink or file)".into(),
            )),
            Err(_) => {}
        }
    }

    let mut patches_checked = 0;
    for (x, files) in &per_upstream {
        patches_checked += files.len();
        for rel in files {
            let ctx = Ctx {
                root,
                cfg: &cfg,
                upstream: x,
                rel: slash(rel),
                scope: scope.as_ref(),
            };
            lint_file(&ctx, &mut violations);
        }
        if cfg.rules.series {
            check_series(root, x, cfg.upstream[x].series, files, &mut violations);
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
            out.push(violation(
                &slash(&child),
                Rule::File,
                "only regular files may be in a patch directory (symlinks are not followed)".into(),
            ));
        }
    }
    Ok(())
}

/// The 기능 cells (first column) of the body rows of every table in docs/scope.md.
fn load_scope(root: &Path) -> Result<BTreeSet<String>, Error> {
    let text = fs::read_to_string(root.join(SCOPE_FILE)).map_err(|e| {
        Error(format!(
            "cannot read {SCOPE_FILE} ([rules] scope_cell_exists needs it): {e}"
        ))
    })?;
    let cells = |l: &str| -> Option<Vec<String>> {
        let t = l.trim();
        t.starts_with('|').then(|| {
            t.trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect()
        })
    };
    let is_separator = |row: &[String]| {
        row.iter().all(|c| {
            !c.is_empty() && c.contains('-') && c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
        })
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut out = BTreeSet::new();
    for (i, l) in lines.iter().enumerate() {
        let Some(row) = cells(l) else { continue };
        let next_is_sep = lines
            .get(i + 1)
            .and_then(|n| cells(n))
            .is_some_and(|n| is_separator(&n));
        if is_separator(&row) || next_is_sep {
            continue;
        }
        if let Some(first) = row.first()
            && !first.is_empty()
        {
            out.insert(first.clone());
        }
    }
    Ok(out)
}

/// One patch file being linted.
struct Ctx<'a> {
    root: &'a Path,
    cfg: &'a Config,
    upstream: &'a str,
    /// Repo-relative path, `/`-separated.
    rel: String,
    scope: Option<&'a BTreeSet<String>>,
}

fn lint_file(ctx: &Ctx<'_>, out: &mut Vec<Violation>) {
    let loc = ctx.rel.as_str();
    let name = loc.rsplit('/').next().unwrap_or(loc);
    if ctx.cfg.rules.file_name && !is_patch_name(name) {
        out.push(violation(
            loc,
            Rule::Name,
            format!("file name `{name}` is not `NNNN-droiduse-<name>.patch` (<name>: lowercase kebab-case)"),
        ));
    }
    let text = match fs::read(ctx.root.join(loc)) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                out.push(violation(
                    loc,
                    Rule::Parse,
                    "not UTF-8 text; patches must be text".into(),
                ));
                return;
            }
        },
        Err(e) => {
            out.push(violation(
                loc,
                Rule::Unreadable,
                format!("cannot read: {e}"),
            ));
            return;
        }
    };
    let mut found = Vec::new();
    let fields = check_header(&text, &ctx.cfg.header, &mut found);
    check_fields(ctx, &fields, &mut found);
    let (added, removed) = check_diff(&text, ctx.cfg.marker.removal_only, &mut found);
    check_budget(ctx, &fields, added, removed, &mut found);
    out.extend(found.into_iter().map(|(r, m)| violation(loc, r, m)));
}

/// `NNNN-droiduse-<name>.patch`, `<name>` lowercase kebab-case.
fn is_patch_name(n: &str) -> bool {
    let Some(stem) = n.strip_suffix(".patch") else {
        return false;
    };
    let (num, rest) = stem.split_at_checked(4).unwrap_or(("", ""));
    let Some(name) = rest.strip_prefix("-droiduse-") else {
        return false;
    };
    num.len() == 4
        && num.bytes().all(|b| b.is_ascii_digit())
        && !name.is_empty()
        && name.split('-').all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Valid values of the header template, `None` where missing or invalid.
#[derive(Default)]
struct Fields {
    hook_for: Option<String>,
    layer: Option<String>,
    budget: Option<usize>,
    scope_cell: Option<String>,
    /// `Budget-approved-by:` as written, if present and a valid login.
    approver: Option<String>,
}

/// A value still holding the template's `<...>` placeholder, or empty.
fn unfilled(v: &str) -> bool {
    v.is_empty() || (v.starts_with('<') && v.ends_with('>'))
}

/// One repo-relative path (`/`-separated, no `.`/`..`/empty components, no whitespace).
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

fn is_positive(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse::<u64>().ok())
        .flatten()
        .filter(|n| *n > 0)
}

/// `[PATCH] droiduse: <summary>`; with `numbered`, also `[PATCH vN]`, `[PATCH n/m]`,
/// `[PATCH vN n/m]` (1 <= n <= m).
fn is_subject(v: &str, numbered: bool) -> bool {
    let Some(rest) = v.strip_prefix("[PATCH") else {
        return false;
    };
    let Some((tags, after)) = rest.split_once(']') else {
        return false;
    };
    match after.strip_prefix(" droiduse: ") {
        Some(summary) if !unfilled(summary.trim()) => {}
        _ => return false,
    }
    if tags.is_empty() {
        return true;
    }
    if !numbered {
        return false;
    }
    let version = |t: &str| t.strip_prefix('v').and_then(is_positive).is_some();
    let nm = |t: &str| {
        t.split_once('/')
            .and_then(|(n, m)| Some((is_positive(n)?, is_positive(m)?)))
            .is_some_and(|(n, m)| n <= m)
    };
    let Some(tags) = tags.strip_prefix(' ') else {
        return false;
    };
    match tags.split(' ').collect::<Vec<_>>().as_slice() {
        [a] => version(a) || nm(a),
        [a, b] => version(a) && nm(b),
        _ => false,
    }
}

/// Exactly one mailbox with an address `local@domain`.
fn is_mailbox(v: &str) -> bool {
    let Ok(list) = mailparse::addrparse(v) else {
        return false;
    };
    match list.as_slice() {
        [mailparse::MailAddr::Single(info)] => {
            let a = &info.addr;
            a.split_once('@')
                .is_some_and(|(l, d)| !l.is_empty() && !d.is_empty() && !d.contains('@'))
                && !a.chars().any(|c| c.is_whitespace() || c == '<' || c == '>')
        }
        _ => false,
    }
}

/// Checks the header template.
fn check_header(text: &str, cfg: &config::Header, out: &mut Vec<(Rule, String)>) -> Fields {
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
    let single = |name: &str,
                  vals: &[String],
                  required: bool,
                  out: &mut Vec<(Rule, String)>|
     -> Option<String> {
        match vals {
            [] => {
                if required {
                    out.push(missing(name));
                }
                None
            }
            [v] => Some(v.clone()),
            _ => {
                out.push(invalid(name, format!("appears {} times", vals.len())));
                None
            }
        }
    };
    if let Some(v) = single("From", &from, true, out) {
        if v.is_empty() {
            out.push(invalid("From", "is empty".into()));
        } else if cfg.from_mailbox && !is_mailbox(&v) {
            out.push(invalid(
                "From",
                format!("must be one mailbox like `Name <user@host>`, found `{v}`"),
            ));
        }
    }
    if let Some(v) = single("Subject", &subject, true, out)
        && !is_subject(&v, cfg.numbered_subject)
    {
        let form = if cfg.numbered_subject {
            "`[PATCH] droiduse: <한 줄 요약>` (or `[PATCH vN n/m] droiduse: ...`)"
        } else {
            "`[PATCH] droiduse: <한 줄 요약>`"
        };
        out.push(invalid("Subject", format!("must be {form}, found `{v}`")));
    }

    // Commit-message fields: from the end of the email headers to the `---` separator
    // (or the first diff line if there is none).
    let body = text.get(body_start..).unwrap_or("");
    let mut seen: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        if line == "---"
            || line.starts_with("diff --git ")
            || line.starts_with("--- ")
            || line.starts_with("@@ ")
        {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && (MESSAGE_FIELDS.contains(&k) || k == APPROVAL_FIELD)
        {
            seen.push((k.to_string(), v.trim().to_string()));
        }
    }
    let values = |f: &str| -> Vec<String> {
        seen.iter()
            .filter(|(k, _)| k == f)
            .map(|(_, v)| v.clone())
            .collect()
    };
    let mut fields = Fields::default();
    for f in MESSAGE_FIELDS {
        let Some(v) = single(f, &values(f), true, out) else {
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
            "Hook-for" => fields.hook_for = Some(v),
            "Layer" if !LAYERS.contains(&v.as_str()) => out.push(invalid(
                f,
                format!("must be one of {}, found `{v}`", LAYERS.join(" | ")),
            )),
            "Layer" => fields.layer = Some(v),
            "Budget" => match is_positive(&v).and_then(|n| usize::try_from(n).ok()) {
                Some(n) => fields.budget = Some(n),
                None => out.push(invalid(
                    f,
                    format!("must be a positive whole number of lines, found `{v}`"),
                )),
            },
            "Upstream" if !is_upstream(&v) => out.push(invalid(
                f,
                format!("must be `pending https://<link>` or `n/a`, found `{v}`"),
            )),
            "Scope-cell" => fields.scope_cell = Some(v),
            _ => {}
        }
    }
    if let Some(v) = single(APPROVAL_FIELD, &values(APPROVAL_FIELD), false, out) {
        if owners::user_login(&v).is_some() {
            fields.approver = Some(v);
        } else {
            out.push(invalid(
                APPROVAL_FIELD,
                format!("must be one GitHub user login (`login` or `@login`), found `{v}`"),
            ));
        }
    }
    fields
}

/// The switchable rules on header values.
fn check_fields(ctx: &Ctx<'_>, f: &Fields, out: &mut Vec<(Rule, String)>) {
    let rules = &ctx.cfg.rules;
    let want = &ctx.cfg.upstream[ctx.upstream].layer;
    if rules.layer_matches_upstream
        && let Some(l) = &f.layer
        && l != want
    {
        out.push((
            Rule::Layer,
            format!(
                "`Layer: {l}` but {UPSTREAM_ROOT}/{} patches must declare `{want}` ([upstream.{}] in {})",
                ctx.upstream,
                ctx.upstream,
                config::CONFIG_FILE
            ),
        ));
    }
    if rules.hook_for_exists
        && let Some(h) = &f.hook_for
        && fs::symlink_metadata(ctx.root.join(h)).is_err()
    {
        out.push((
            Rule::HookFor,
            format!("`Hook-for:` `{h}` does not exist in the repo"),
        ));
    }
    if let (Some(scope), Some(c)) = (ctx.scope, &f.scope_cell)
        && !scope.contains(c)
    {
        out.push((
            Rule::ScopeCell,
            format!("`Scope-cell:` `{c}` is not the 기능 cell of a row in {SCOPE_FILE}"),
        ));
    }
}

fn file_name(op: &FileOperation<'_, str>) -> String {
    match op {
        FileOperation::Delete(p) | FileOperation::Create(p) => p.to_string(),
        FileOperation::Modify { modified, .. } => modified.to_string(),
        FileOperation::Rename { to, .. } | FileOperation::Copy { to, .. } => to.to_string(),
    }
}

/// Checks markers on the diff part. Returns (added, removed) lines over all hunks.
fn check_diff(
    text: &str,
    removal_only: RemovalOnly,
    out: &mut Vec<(Rule, String)>,
) -> (usize, usize) {
    let (mut added, mut removed) = (0usize, 0usize);
    for fp in PatchSet::parse(text, ParseOptions::gitdiff()) {
        let fp = match fp {
            Ok(fp) => fp,
            Err(e) => {
                out.push((Rule::Parse, format!("not a parseable patch: {e}")));
                break;
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
            let (mut adds, mut marked) = (0usize, false);
            for l in h.lines() {
                match l {
                    Line::Insert(s) => {
                        adds += 1;
                        marked |= s.contains(MARKER);
                    }
                    Line::Delete(_) => removed += 1,
                    Line::Context(_) => {}
                }
            }
            added += adds;
            if marked || (adds == 0 && removal_only == RemovalOnly::Allow) {
                continue;
            }
            let (o, n) = (h.old_range(), h.new_range());
            let what = if adds == 0 {
                "only removes lines, so it cannot carry"
            } else {
                "adds no line with"
            };
            out.push((
                Rule::Marker,
                format!(
                    "hunk {} (@@ -{},{} +{},{} @@) of `{name}` {what} a `{MARKER}` marker",
                    i + 1,
                    o.start(),
                    o.len(),
                    n.start(),
                    n.len()
                ),
            ));
        }
    }
    (added, removed)
}

/// Changed lines against `Budget:`, and `Budget:` against the cap.
fn check_budget(
    ctx: &Ctx<'_>,
    f: &Fields,
    added: usize,
    removed: usize,
    out: &mut Vec<(Rule, String)>,
) {
    let Some(budget) = f.budget else { return };
    let count: Count = ctx.cfg.budget.count;
    let changed = count.apply(added, removed);
    if changed > budget {
        out.push((
            Rule::Budget,
            format!(
                "{changed} changed lines ({}) exceed `Budget:` {budget}",
                count.name()
            ),
        ));
    }
    let cap = ctx.cfg.cap(ctx.upstream) as usize;
    if budget <= cap {
        return;
    }
    let over = format!("`Budget:` {budget} is above the cap {cap}");
    let why = match ctx.cfg.budget.overrun {
        Overrun::Forbidden => format!(
            "{over}; overruns are forbidden ([budget] overrun = \"forbidden\" in {})",
            config::CONFIG_FILE
        ),
        Overrun::Owners => {
            let Some(who) = &f.approver else {
                out.push((
                    Rule::BudgetOverrun,
                    format!(
                        "{over} and no valid `{APPROVAL_FIELD}:` names an owner of this patch in OWNERS"
                    ),
                ));
                return;
            };
            let login = owners::user_login(who).unwrap_or_default();
            match owners::owners_of(ctx.root, &ctx.rel) {
                Ok(Some(owners)) if owners.contains(&login) => return,
                Ok(Some(_)) => format!(
                    "{over} and `{APPROVAL_FIELD}:` `{who}` is not a user owning {} in OWNERS",
                    ctx.rel
                ),
                Ok(None) => format!(
                    "{over} and no OWNERS rule covers {}, so `{who}` cannot approve it",
                    ctx.rel
                ),
                Err(e) => format!("{over} and the approval cannot be verified: {e}"),
            }
        }
    };
    out.push((Rule::BudgetOverrun, why));
}

/// `third_party/<x>/patches/series` lists every patch file exactly once, and every entry is
/// one of them.
fn check_series(
    root: &Path,
    x: &str,
    format: SeriesFormat,
    files: &[PathBuf],
    out: &mut Vec<Violation>,
) {
    let dir = format!("{UPSTREAM_ROOT}/{x}/{PATCH_DIR}");
    let loc = format!("{dir}/series");
    let files: BTreeSet<String> = files.iter().map(|p| slash(p)).collect();
    let text = match fs::read_to_string(root.join(&loc)) {
        Ok(t) => t,
        Err(e) => {
            if !files.is_empty() {
                out.push(violation(
                    &loc,
                    Rule::Series,
                    format!(
                        "cannot read the series file ({e}); {} patch file(s) have no order",
                        files.len()
                    ),
                ));
            }
            return;
        }
    };
    let form = match format {
        SeriesFormat::File => "`<patch-file>`",
        SeriesFormat::ProjectFile => "`<project-path> <patch-file>`",
    };
    let mut listed: BTreeSet<String> = BTreeSet::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("");
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let entry = match (format, tokens.as_slice()) {
            (_, []) => continue,
            (SeriesFormat::File, [p]) => *p,
            (SeriesFormat::ProjectFile, [project, p]) => {
                if !is_repo_path(project) || !p.starts_with(&format!("{project}/")) {
                    out.push(violation(
                        &loc,
                        Rule::Series,
                        format!(
                            "line {}: patch `{p}` is not under its project path `{project}/`",
                            i + 1
                        ),
                    ));
                    continue;
                }
                *p
            }
            _ => {
                out.push(violation(
                    &loc,
                    Rule::Series,
                    format!("line {}: expected {form}, found `{}`", i + 1, line.trim()),
                ));
                continue;
            }
        };
        if !is_repo_path(entry) {
            out.push(violation(
                &loc,
                Rule::Series,
                format!("line {}: `{entry}` is not a path inside {dir}", i + 1),
            ));
            continue;
        }
        let full = format!("{dir}/{entry}");
        if !files.contains(&full) {
            out.push(violation(
                &loc,
                Rule::Series,
                format!("line {}: `{entry}` is not a patch file in {dir}", i + 1),
            ));
        } else if !listed.insert(full) {
            out.push(violation(
                &loc,
                Rule::Series,
                format!("line {}: `{entry}` is listed more than once", i + 1),
            ));
        }
    }
    for f in files.difference(&listed) {
        let entry = f.strip_prefix(&format!("{dir}/")).unwrap_or(f);
        out.push(violation(
            &loc,
            Rule::Series,
            format!("patch file `{entry}` is not listed"),
        ));
    }
}
