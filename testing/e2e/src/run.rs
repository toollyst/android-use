//! Selecting, running and judging scenarios.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::manifest::{Expected, MILESTONES, Need, Reason, Scenario};

/// What a scenario run produced, before it is compared with the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    Pass,
    /// Error code [`Reason::CODE`]. `unmet` lists every need not met (the reason first) so
    /// the report shows all the work in front of the scenario, not only the next step.
    Red {
        reason: Reason,
        unmet: Vec<Reason>,
    },
    /// Any other error: a wrong result, an unexpected error code, a broken setup.
    Fail(String),
}

/// The judged outcome of one scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Red { reason: Reason, unmet: Vec<Reason> },
    Fail(String),
}

/// Strict matching: only the declared outcome is accepted; everything else is FAIL.
pub fn judge(expected: &Expected, observed: Observed) -> Verdict {
    match (expected, observed) {
        (Expected::Pass, Observed::Pass) => Verdict::Pass,
        (Expected::Red(want), Observed::Red { reason, unmet }) if *want == reason => {
            Verdict::Red { reason, unmet }
        }
        (Expected::Red(want), Observed::Red { reason, .. }) => Verdict::Fail(format!(
            "RED for `{reason}`, but the manifest declares `{want}`"
        )),
        (Expected::Red(want), Observed::Pass) => Verdict::Fail(format!(
            "unexpected PASS (the manifest declares RED `{want}`): update the manifest"
        )),
        (Expected::Pass, Observed::Red { reason, .. }) => Verdict::Fail(format!(
            "RED for `{reason}`, but the manifest declares PASS"
        )),
        (_, Observed::Fail(m)) => Verdict::Fail(m),
    }
}

/// The scenarios a target list selects: `all`, a milestone (`m0`..`m6`), a tag, or an id
/// prefix of whole dot-separated segments. A target that selects nothing is an error (a typo
/// must not turn into an empty, green run).
pub fn select<'a>(
    scenarios: &'a [Scenario],
    targets: &[String],
) -> Result<Vec<&'a Scenario>, String> {
    if targets.is_empty() {
        return Err("no target given".to_string());
    }
    let hit = |s: &Scenario, t: &str| {
        t == "all"
            || (MILESTONES.contains(&t) && s.milestone == t)
            || s.tags.iter().any(|g| g == t)
            || s.id == t
            || s.id.starts_with(&format!("{t}."))
    };
    for t in targets {
        if !scenarios.iter().any(|s| hit(s, t)) {
            return Err(format!("target `{t}` selects no scenario"));
        }
    }
    Ok(scenarios
        .iter()
        .filter(|s| targets.iter().any(|t| hit(s, t)))
        .collect())
}

/// The commands, events and error codes `contracts/adp/protocol.json` defines, as
/// `Domain.name` and bare error code names.
pub fn adp_surfaces(protocol_json: &str) -> Result<BTreeSet<String>, String> {
    let v: Value =
        serde_json::from_str(protocol_json).map_err(|e| format!("protocol.json: {e}"))?;
    let mut out = BTreeSet::new();
    for e in v["errors"].as_array().into_iter().flatten() {
        if let Some(n) = e["name"].as_str() {
            out.insert(n.to_string());
        }
    }
    for d in v["domains"].as_array().into_iter().flatten() {
        let dom = d["domain"]
            .as_str()
            .ok_or("protocol.json: a domain without a name")?;
        for key in ["commands", "events"] {
            for c in d[key].as_array().into_iter().flatten() {
                if let Some(n) = c["name"].as_str() {
                    out.insert(format!("{dom}.{n}"));
                }
            }
        }
    }
    Ok(out)
}

/// Checks a scenario's needs in order. A component is located only through its
/// environment variable (an explicit path); one that is set to a path that does not exist
/// is a broken setup (FAIL), never a skip.
pub fn check_needs(
    s: &Scenario,
    adp: &BTreeSet<String>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Result<Vec<Reason>, String> {
    let mut unmet = Vec::new();
    for n in &s.needs {
        let met = match n {
            Need::Surface { component, name } => component == "contracts/adp" && adp.contains(name),
            Need::Component(c) => {
                let var = Need::env_var(c);
                match env(&var) {
                    None => false,
                    Some(p) if Path::new(&p).exists() => true,
                    Some(p) => {
                        return Err(format!(
                            "{var} = {} does not exist",
                            Path::new(&p).display()
                        ));
                    }
                }
            }
        };
        if !met {
            unmet.push(Reason::missing(n));
        }
    }
    Ok(unmet)
}

/// The run of a scenario whose needs are all met but that has no body (`bodies.rs`) yet:
/// the one place that says so, as a named RED.
pub fn body_not_written() -> Observed {
    let need = Need::Surface {
        component: "testing/e2e".to_string(),
        name: "scenario-body".to_string(),
    };
    let reason = Reason::missing(&need);
    Observed::Red {
        reason: reason.clone(),
        unmet: vec![reason],
    }
}

/// The standard scenario run without bodies: needs first, then [`body_not_written`].
pub fn observe(
    s: &Scenario,
    adp: &BTreeSet<String>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Observed {
    observe_with(s, adp, env, &|_| None)
}

/// Needs first, then the scenario's body; `body` answers `None` for a scenario without one.
pub fn observe_with(
    s: &Scenario,
    adp: &BTreeSet<String>,
    env: &dyn Fn(&str) -> Option<OsString>,
    body: &dyn Fn(&Scenario) -> Option<Observed>,
) -> Observed {
    match check_needs(s, adp, env) {
        Err(m) => Observed::Fail(m),
        Ok(unmet) if !unmet.is_empty() => Observed::Red {
            reason: unmet[0].clone(),
            unmet,
        },
        Ok(_) => body(s).unwrap_or_else(body_not_written),
    }
}

pub type Runner = Arc<dyn Fn(&Scenario) -> Observed + Send + Sync>;

/// Runs each scenario on its own thread; a panic or a run longer than `timeout` is FAIL.
pub fn run(scenarios: &[&Scenario], timeout: Duration, runner: Runner) -> Vec<(Scenario, Verdict)> {
    let mut out = Vec::new();
    for s in scenarios {
        let (tx, rx) = mpsc::channel();
        let (r, sc) = (runner.clone(), (*s).clone());
        thread::spawn(move || {
            let _ = tx.send(r(&sc));
        });
        let verdict = match rx.recv_timeout(timeout) {
            Ok(o) => judge(&s.expected, o),
            Err(RecvTimeoutError::Timeout) => {
                Verdict::Fail(format!("timed out after {} s", timeout.as_secs_f64()))
            }
            Err(RecvTimeoutError::Disconnected) => Verdict::Fail("panicked".to_string()),
        };
        out.push(((*s).clone(), verdict));
    }
    out
}

/// The machine-readable report: counts, the RED list (the remaining work) and the FAILs.
pub fn report(targets: &[String], results: &[(Scenario, Verdict)]) -> Value {
    let count = |f: fn(&Verdict) -> bool| results.iter().filter(|(_, v)| f(v)).count();
    let remaining: Vec<Value> = results
        .iter()
        .filter_map(|(s, v)| match v {
            Verdict::Red { reason, unmet } => Some(json!({
                "id": s.id,
                "title": s.title,
                "milestone": s.milestone,
                "scope": s.scope,
                "tags": s.tags,
                "code": Reason::CODE,
                "reason": reason,
                "unmet": unmet,
            })),
            _ => None,
        })
        .collect();
    let failures: Vec<Value> = results
        .iter()
        .filter_map(|(s, v)| match v {
            Verdict::Fail(m) => Some(json!({ "id": s.id, "message": m })),
            _ => None,
        })
        .collect();
    json!({
        "targets": targets,
        "pass": count(|v| matches!(v, Verdict::Pass)),
        "red": count(|v| matches!(v, Verdict::Red { .. })),
        "fail": count(|v| matches!(v, Verdict::Fail(_))),
        "remaining": remaining,
        "failures": failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario(
        id: &str,
        milestone: &str,
        tags: &[&str],
        needs: &[&str],
        expected: Expected,
    ) -> Scenario {
        Scenario {
            id: id.to_string(),
            title: "t".to_string(),
            milestone: milestone.to_string(),
            scope: "gate:M0".to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            needs: needs.iter().map(|n| Need::parse(n).unwrap()).collect(),
            expected,
        }
    }

    fn red(r: &str) -> Expected {
        Expected::Red(Reason::parse(r).unwrap())
    }

    fn observed_red(r: &str) -> Observed {
        let reason = Reason::parse(r).unwrap();
        Observed::Red {
            reason: reason.clone(),
            unmet: vec![reason],
        }
    }

    fn no_env(_: &str) -> Option<OsString> {
        None
    }

    #[test]
    fn strict_matching() {
        let want = red("missing:clients/mcp");
        assert!(matches!(
            judge(&want, observed_red("missing:clients/mcp")),
            Verdict::Red { .. }
        ));
        // a different reason, an unexpected PASS, a RED where PASS is declared: FAIL
        assert!(matches!(
            judge(&want, observed_red("host/vmm/api#create")),
            Verdict::Fail(_)
        ));
        assert!(
            matches!(judge(&want, Observed::Pass), Verdict::Fail(m) if m.contains("unexpected PASS"))
        );
        assert!(matches!(
            judge(&Expected::Pass, observed_red("host/vmm/api#create")),
            Verdict::Fail(_)
        ));
        assert_eq!(judge(&Expected::Pass, Observed::Pass), Verdict::Pass);
        assert!(matches!(
            judge(&Expected::Pass, Observed::Fail("x".into())),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn filter_by_milestone_tag_and_id_prefix() {
        let all = vec![
            scenario(
                "adp.auth.header",
                "m0",
                &["adp", "security"],
                &[],
                Expected::Pass,
            ),
            scenario("adp.authz.revoked", "m1", &["adp"], &[], Expected::Pass),
            scenario("mcp.tools", "m0", &["mcp"], &[], Expected::Pass),
        ];
        let ids = |t: &[&str]| -> Vec<String> {
            let t: Vec<String> = t.iter().map(|s| s.to_string()).collect();
            select(&all, &t)
                .unwrap()
                .iter()
                .map(|s| s.id.clone())
                .collect()
        };
        assert_eq!(ids(&["m0"]), ["adp.auth.header", "mcp.tools"]);
        assert_eq!(ids(&["security"]), ["adp.auth.header"]);
        assert_eq!(ids(&["adp.auth"]), ["adp.auth.header"]); // whole segments: not adp.authz
        assert_eq!(ids(&["m1", "mcp"]), ["adp.authz.revoked", "mcp.tools"]);
        assert_eq!(ids(&["all"]).len(), 3);
        assert!(select(&all, &["m5".to_string()]).is_err());
        assert!(select(&all, &["adp.au".to_string()]).is_err());
        assert!(select(&all, &[]).is_err());
    }

    #[test]
    fn needs_in_order_missing_component_is_a_declared_red() {
        let adp = adp_surfaces(r#"{"errors":[{"name":"TIMEOUT"}],"domains":[{"domain":"Input","commands":[{"name":"click"}]}]}"#).unwrap();
        let s = scenario(
            "x.y",
            "m2",
            &["adp"],
            &[
                "contracts/adp#Input.click",
                "contracts/adp#App.stop",
                "host/base/trace#export",
                "clients/mcp",
            ],
            red("missing:contracts/adp#App.stop"),
        );
        let o = observe(&s, &adp, &no_env);
        let Observed::Red { reason, unmet } = &o else {
            panic!("{o:?}")
        };
        assert_eq!(reason.as_str(), "missing:contracts/adp#App.stop");
        let unmet: Vec<&str> = unmet.iter().map(Reason::as_str).collect();
        assert_eq!(
            unmet,
            [
                "missing:contracts/adp#App.stop",
                "missing:host/base/trace#export",
                "missing:clients/mcp"
            ]
        );
        assert!(matches!(judge(&s.expected, o), Verdict::Red { .. }));
    }

    #[test]
    fn present_component_moves_on_and_a_bad_path_fails() {
        let s = scenario(
            "x.y",
            "m0",
            &["mcp"],
            &["clients/mcp"],
            red("missing:clients/mcp"),
        );
        let here = std::env::current_exe().unwrap().into_os_string();
        let found = move |v: &str| (v == "DROIDUSE_E2E_CLIENTS_MCP").then(|| here.clone());
        let o = observe(&s, &BTreeSet::new(), &found);
        assert_eq!(o, body_not_written());
        // the declared RED no longer holds: strict matching turns it into FAIL
        assert!(matches!(judge(&s.expected, o), Verdict::Fail(_)));
        let bad = |_: &str| Some(OsString::from("/no/such/binary"));
        assert!(
            matches!(observe(&s, &BTreeSet::new(), &bad), Observed::Fail(m) if m.contains("does not exist"))
        );
    }

    #[test]
    fn timeout_and_panic_are_fail() {
        let s = scenario("x.y", "m0", &["adp"], &[], Expected::Pass);
        let slow: Runner = Arc::new(|_| {
            thread::sleep(Duration::from_secs(5));
            Observed::Pass
        });
        let r = run(&[&s], Duration::from_millis(50), slow);
        assert!(
            matches!(&r[0].1, Verdict::Fail(m) if m.contains("timed out")),
            "{r:?}"
        );
        let boom: Runner = Arc::new(|_| panic!("scenario body panicked on purpose"));
        let r = run(&[&s], Duration::from_secs(5), boom);
        assert!(
            matches!(&r[0].1, Verdict::Fail(m) if m == "panicked"),
            "{r:?}"
        );
        let ok: Runner = Arc::new(|_| Observed::Pass);
        assert_eq!(run(&[&s], Duration::from_secs(5), ok)[0].1, Verdict::Pass);
    }
}
