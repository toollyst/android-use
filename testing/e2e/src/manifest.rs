//! The scenario manifest (`testing/e2e/scenarios.toml`): loading and validation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

pub const MILESTONES: [&str; 7] = ["m0", "m1", "m2", "m3", "m4", "m5", "m6"];

/// The named reason of a RED outcome. RED always carries the error code [`Reason::CODE`].
///
/// The naming convention lives here and nowhere else:
/// - `missing:<component>`: the component does not exist (binary, artifact);
/// - `missing:<component>#<surface>`: the component exists or not, but the surface the
///   scenario needs is not defined (for example an ADP command not in `protocol.json`);
/// - `<component>#<operation>`: the component answered `UNIMPLEMENTED` for that operation.
///
/// `<component>` is a repo path (`host/vmm/api`), `<surface>`/`<operation>` a name
/// (`Target.createTarget`, `create`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Reason(String);

impl Reason {
    pub const CODE: &str = "UNIMPLEMENTED";

    pub fn parse(s: &str) -> Result<Self, String> {
        let body = s.strip_prefix("missing:").unwrap_or(s);
        let ok = match body.split_once('#') {
            Some((c, op)) => is_component(c) && is_name(op),
            None => s.starts_with("missing:") && is_component(body),
        };
        if ok {
            Ok(Reason(s.to_string()))
        } else {
            Err(format!(
                "reason `{s}` is not `missing:<component>`, `missing:<component>#<surface>` \
                 or `<component>#<operation>`"
            ))
        }
    }

    /// `missing:<what>` for a need that is not met.
    pub fn missing(need: &Need) -> Self {
        Reason(format!("missing:{need}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a scenario needs before its body can run, in the order the scenario needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Need {
    /// A component located through its environment variable ([`Need::env_var`]).
    Component(String),
    /// A surface of a component: met only if a definition the runner reads lists it
    /// (`contracts/adp#<name>` against `protocol.json`); every other surface is undefined.
    Surface { component: String, name: String },
}

impl Need {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.split_once('#') {
            Some((c, n)) if is_component(c) && is_name(n) => Ok(Need::Surface {
                component: c.to_string(),
                name: n.to_string(),
            }),
            None if is_component(s) => Ok(Need::Component(s.to_string())),
            _ => Err(format!(
                "need `{s}` is not `<component>` or `<component>#<surface>`"
            )),
        }
    }

    /// `DROIDUSE_E2E_` + the component path in upper case, `/` as `_`
    /// (`host/entrypoints/adp_server` → `DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER`).
    pub fn env_var(component: &str) -> String {
        format!(
            "DROIDUSE_E2E_{}",
            component.replace('/', "_").to_ascii_uppercase()
        )
    }
}

impl fmt::Display for Need {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Need::Component(c) => f.write_str(c),
            Need::Surface { component, name } => write!(f, "{component}#{name}"),
        }
    }
}

fn is_component(s: &str) -> bool {
    !s.is_empty()
        && s.split('/').all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}

fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

fn is_word(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    Pass,
    Red(Reason),
}

#[derive(Debug, Clone)]
pub struct Scenario {
    pub id: String,
    pub title: String,
    pub milestone: String,
    /// A `docs/scope.md` row (its Feature cell) or a roadmap gate `gate:M0`..`gate:M6`.
    pub scope: String,
    pub tags: Vec<String>,
    pub needs: Vec<Need>,
    pub expected: Expected,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    /// Per-scenario timeout (a setting in the manifest).
    pub timeout_s: u64,
    pub scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    timeout_s: u64,
    #[serde(default, rename = "scenario")]
    scenarios: Vec<RawScenario>,
    #[serde(default)]
    not_e2e: Vec<RawNotE2e>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScenario {
    id: String,
    title: String,
    milestone: String,
    scope: String,
    tags: Vec<String>,
    #[serde(default)]
    needs: Vec<String>,
    expect: String,
    reason: Option<String>,
}

/// A `docs/scope.md` row that no E2E scenario can verify, with the reason.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNotE2e {
    scope: String,
    why: String,
}

/// The rows of the `docs/scope.md` tables: Feature cell → Stage cell.
pub fn scope_rows(scope_md: &str) -> BTreeMap<String, String> {
    let mut rows = BTreeMap::new();
    for line in scope_md.lines() {
        let Some(inner) = line.trim().strip_prefix('|') else {
            continue;
        };
        let cells: Vec<&str> = inner
            .trim_end_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        let (Some(first), Some(last)) = (cells.first(), cells.last()) else {
            continue;
        };
        if cells.len() < 2 || *first == "Feature" || first.starts_with("---") {
            continue;
        }
        rows.insert(first.to_string(), last.to_string());
    }
    rows
}

/// Parses and validates the manifest against the `docs/scope.md` rows. Every problem is
/// reported, not only the first.
pub fn load(text: &str, rows: &BTreeMap<String, String>) -> Result<Manifest, Vec<String>> {
    let raw: RawManifest = toml::from_str(text).map_err(|e| vec![e.to_string()])?;
    let mut errs = Vec::new();
    if raw.timeout_s == 0 {
        errs.push("timeout_s must be at least 1".to_string());
    }
    let is_gate = |s: &str| {
        s.strip_prefix("gate:M")
            .is_some_and(|n| n.len() == 1 && ("0"..="6").contains(&n))
    };
    let mut ids = BTreeSet::new();
    let mut covered = BTreeSet::new();
    let mut scenarios = Vec::new();
    for s in raw.scenarios {
        let at = format!("scenario `{}`", s.id);
        if !s.id.split('.').all(is_word) {
            errs.push(format!(
                "{at}: id must be dot-separated lowercase words ([a-z0-9-])"
            ));
        }
        if !ids.insert(s.id.clone()) {
            errs.push(format!("{at}: duplicate id"));
        }
        if s.title.trim().is_empty() || s.title.contains('\n') {
            errs.push(format!("{at}: title must be one non-empty line"));
        }
        if !MILESTONES.contains(&s.milestone.as_str()) {
            errs.push(format!("{at}: milestone `{}` is not m0..m6", s.milestone));
        }
        if rows.contains_key(&s.scope) {
            covered.insert(s.scope.clone());
        } else if !is_gate(&s.scope) {
            errs.push(format!(
                "{at}: scope `{}` is neither a docs/scope.md row nor gate:M0..gate:M6",
                s.scope
            ));
        }
        if s.tags.is_empty() {
            errs.push(format!("{at}: no tags"));
        }
        let mut tags = BTreeSet::new();
        for t in &s.tags {
            if !is_word(t) || MILESTONES.contains(&t.as_str()) || t == "all" {
                errs.push(format!(
                    "{at}: tag `{t}` must be a lowercase word other than a milestone or `all`"
                ));
            }
            if !tags.insert(t) {
                errs.push(format!("{at}: duplicate tag `{t}`"));
            }
        }
        let mut needs = Vec::new();
        for n in &s.needs {
            match Need::parse(n) {
                Ok(n) => needs.push(n),
                Err(e) => errs.push(format!("{at}: {e}")),
            }
        }
        let expected = match (s.expect.as_str(), &s.reason) {
            ("pass", None) => Some(Expected::Pass),
            ("red", Some(r)) => match Reason::parse(r) {
                Ok(r) => Some(Expected::Red(r)),
                Err(e) => {
                    errs.push(format!("{at}: {e}"));
                    None
                }
            },
            ("pass", Some(_)) => {
                errs.push(format!("{at}: expect = \"pass\" takes no reason"));
                None
            }
            ("red", None) => {
                errs.push(format!("{at}: expect = \"red\" needs a reason"));
                None
            }
            (other, _) => {
                errs.push(format!(
                    "{at}: expect `{other}` is not \"pass\" or \"red\" (FAIL is a result, never \
                     an expectation)"
                ));
                None
            }
        };
        if let Some(expected) = expected {
            scenarios.push(Scenario {
                id: s.id,
                title: s.title,
                milestone: s.milestone,
                scope: s.scope,
                tags: s.tags,
                needs,
                expected,
            });
        }
    }
    let mut not_e2e = BTreeSet::new();
    for n in &raw.not_e2e {
        if !rows.contains_key(&n.scope) {
            errs.push(format!("not_e2e `{}`: not a docs/scope.md row", n.scope));
        }
        if n.why.trim().is_empty() {
            errs.push(format!("not_e2e `{}`: no reason given", n.scope));
        }
        if covered.contains(&n.scope) {
            errs.push(format!("not_e2e `{}`: also covered by a scenario", n.scope));
        }
        if !not_e2e.insert(n.scope.clone()) {
            errs.push(format!("not_e2e `{}`: listed twice", n.scope));
        }
    }
    for (row, stage) in rows {
        let v1 = stage.len() == 2 && stage.starts_with('M') && ("0"..="6").contains(&&stage[1..]);
        if v1 && !covered.contains(row) && !not_e2e.contains(row) {
            errs.push(format!(
                "docs/scope.md row `{row}` ({stage}) has no scenario and is not listed in not_e2e"
            ));
        }
    }
    if errs.is_empty() {
        Ok(Manifest {
            timeout_s: raw.timeout_s,
            scenarios,
        })
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCOPE: &str = "\
| Feature | Contract | Stage |
|---|---|---|
| Coordinate input | device | M1 |
| Audio | device | after v1 |
| KeyMint (host) | keymint | M1 |
";

    fn manifest(body: &str) -> String {
        format!("timeout_s = 5\n\n[[not_e2e]]\nscope = \"KeyMint (host)\"\nwhy = \"VTS\"\n\n{body}")
    }

    const OK: &str = r#"
[[scenario]]
id = "input.tap"
title = "A tap is delivered"
milestone = "m1"
scope = "Coordinate input"
tags = ["adp"]
needs = ["contracts/adp#Input.dispatchTouch", "host/entrypoints/adp_server"]
expect = "red"
reason = "missing:host/entrypoints/adp_server"
"#;

    fn errors(text: &str) -> Vec<String> {
        load(text, &scope_rows(SCOPE)).expect_err("invalid manifest")
    }

    #[test]
    fn scope_rows_skip_header_and_separator() {
        let rows = scope_rows(SCOPE);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows["Coordinate input"], "M1");
        assert_eq!(rows["Audio"], "after v1");
    }

    #[test]
    fn valid_manifest_loads() {
        let m = load(&manifest(OK), &scope_rows(SCOPE)).unwrap();
        assert_eq!(m.scenarios.len(), 1);
        assert_eq!(m.scenarios[0].needs.len(), 2);
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let e = errors(&manifest(&format!("{OK}{OK}")));
        assert!(e.iter().any(|e| e.contains("duplicate id")), "{e:?}");
    }

    #[test]
    fn unknown_scope_row_is_rejected() {
        let e = errors(&manifest(&OK.replace("Coordinate input", "Teleportation")));
        assert!(e.iter().any(|e| e.contains("Teleportation")), "{e:?}");
        // and the row it no longer covers is reported too
        assert!(
            e.iter()
                .any(|e| e.contains("`Coordinate input` (M1) has no scenario")),
            "{e:?}"
        );
    }

    #[test]
    fn gate_scope_is_accepted() {
        let gate = format!(
            "{OK}{}",
            OK.replace("input.tap", "x.y")
                .replace("Coordinate input", "gate:M0")
        );
        assert!(load(&manifest(&gate), &scope_rows(SCOPE)).is_ok());
        let e = errors(&manifest(&format!(
            "{OK}{}",
            OK.replace("input.tap", "x.y")
                .replace("Coordinate input", "gate:M7")
        )));
        assert!(e.iter().any(|e| e.contains("gate:M7")), "{e:?}");
    }

    #[test]
    fn uncovered_v1_row_is_rejected_but_after_v1_is_not() {
        let e = errors(&manifest(""));
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("Coordinate input"), "{e:?}");
    }

    #[test]
    fn not_e2e_must_name_a_row_with_a_reason_and_not_overlap() {
        let text = format!(
            "{}\n[[not_e2e]]\nscope = \"Coordinate input\"\nwhy = \"\"\n\n[[not_e2e]]\nscope = \"Nope\"\nwhy = \"x\"\n",
            manifest(OK)
        );
        let e = errors(&text);
        assert!(e.iter().any(|e| e.contains("no reason given")), "{e:?}");
        assert!(e.iter().any(|e| e.contains("also covered")), "{e:?}");
        assert!(
            e.iter()
                .any(|e| e.contains("`Nope`: not a docs/scope.md row")),
            "{e:?}"
        );
    }

    #[test]
    fn expectations_are_pass_or_red_with_a_valid_reason() {
        for (bad, want) in [
            (
                "expect = \"red\"\nreason = \"adp server missing\"",
                "is not `missing:",
            ),
            (
                "expect = \"fail\"\nreason = \"missing:x\"",
                "FAIL is a result",
            ),
            (
                "expect = \"pass\"\nreason = \"missing:x\"",
                "takes no reason",
            ),
        ] {
            let text = OK.replace(
                "expect = \"red\"\nreason = \"missing:host/entrypoints/adp_server\"",
                bad,
            );
            let e = errors(&manifest(&text));
            assert!(e.iter().any(|e| e.contains(want)), "{bad}: {e:?}");
        }
    }

    #[test]
    fn tags_cannot_shadow_milestones_or_all() {
        let e = errors(&manifest(
            &OK.replace("tags = [\"adp\"]", "tags = [\"m1\", \"all\"]"),
        ));
        assert_eq!(e.iter().filter(|e| e.contains("tag `")).count(), 2, "{e:?}");
    }

    #[test]
    fn reason_and_need_syntax() {
        for ok in [
            "missing:host/entrypoints/adp_server",
            "missing:contracts/adp#Target.createTarget",
            "host/vmm/api#create",
        ] {
            assert!(Reason::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "host/vmm/api",
            "missing:",
            "missing:Host/x",
            "x#",
            "missing:a//b",
        ] {
            assert!(Reason::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            Need::env_var("host/entrypoints/adp_server"),
            "DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER"
        );
        let n = Need::parse("contracts/adp#App.stop").unwrap();
        assert_eq!(
            Reason::missing(&n).as_str(),
            "missing:contracts/adp#App.stop"
        );
        assert!(Need::parse("adp server").is_err());
    }
}
