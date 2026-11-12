//! Exit codes of the runner binary and of `testing/e2e/run.sh` against a fixture root, and
//! validation of the real inventory against the real `docs/scope.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const SCOPE: &str = "\
| Feature | Contract | Stage |
|---|---|---|
| Coordinate input | device | M1 |
| Audio | device | after v1 |
";

const PROTOCOL: &str = r#"{ "errors": [ { "name": "INVALID_REQUEST" } ],
  "domains": [ { "domain": "Connection", "commands": [ { "name": "hello" } ] } ] }"#;

const MANIFEST: &str = r#"timeout_s = 30

[[scenario]]
id = "adp.hello.compatible"
title = "hello"
milestone = "m0"
scope = "gate:M0"
tags = ["adp"]
needs = ["contracts/adp#Connection.hello", "host/entrypoints/adp_server"]
expect = "red"
reason = "missing:host/entrypoints/adp_server"

[[scenario]]
id = "input.tap"
title = "tap"
milestone = "m1"
scope = "Coordinate input"
tags = ["adp", "input"]
needs = ["contracts/adp#Target.createTarget", "host/entrypoints/adp_server"]
expect = "red"
reason = "missing:contracts/adp#Target.createTarget"
"#;

fn fixture(manifest: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, text) in [
        ("docs/scope.md", SCOPE),
        ("contracts/adp/protocol.json", PROTOCOL),
        ("testing/e2e/scenarios.toml", manifest),
    ] {
        let p = dir.path().join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    dir
}

/// A command with every `DROIDUSE_E2E_*` variable of this environment removed, so the
/// outcome does not depend on what the developer or CI has set.
fn clean(mut c: Command) -> Command {
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("DROIDUSE_E2E_") {
            c.env_remove(k);
        }
    }
    c
}

fn output(mut c: Command) -> (i32, String) {
    let out = c.output().expect("runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().expect("exit code"), text)
}

fn e2e(root: &Path, args: &[&str]) -> Command {
    let mut c = clean(Command::new(env!("CARGO_BIN_EXE_e2e")));
    c.arg("--root").arg(root).args(args);
    c
}

#[track_caller]
fn assert_exit(got: (i32, String), want: i32) -> String {
    assert_eq!(got.0, want, "output:\n{}", got.1);
    got.1
}

#[test]
fn declared_reds_exit_0_and_the_report_lists_them() {
    let fx = fixture(MANIFEST);
    let out = assert_exit(output(e2e(fx.path(), &["all"])), 0);
    assert!(out.contains("2 red, 0 fail"), "{out}");
    let report: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fx.path().join("target/e2e/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["red"], 2);
    let tap = &report["remaining"][1];
    assert_eq!(tap["id"], "input.tap");
    assert_eq!(tap["code"], "UNIMPLEMENTED");
    assert_eq!(
        tap["unmet"],
        serde_json::json!([
            "missing:contracts/adp#Target.createTarget",
            "missing:host/entrypoints/adp_server"
        ])
    );
}

#[test]
fn a_wrong_reason_exits_1() {
    let fx = fixture(&MANIFEST.replace(
        "reason = \"missing:host/entrypoints/adp_server\"",
        "reason = \"missing:clients/mcp\"",
    ));
    let out = assert_exit(output(e2e(fx.path(), &["m0"])), 1);
    assert!(out.contains("FAIL adp.hello.compatible"), "{out}");
}

#[test]
fn a_component_that_appears_breaks_the_declared_red() {
    // The fixture "server" exists, so the scenario gets past its needs; its declared RED no
    // longer holds and the run fails until the manifest and the scenario body are updated.
    // (An id without a body in bodies.rs: a real body would start the fixture "server".)
    let fx = fixture(&MANIFEST.replace("adp.hello.compatible", "adp.fixture.no-body"));
    let mut c = e2e(fx.path(), &["adp"]);
    c.env(
        "DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER",
        env!("CARGO_BIN_EXE_e2e"),
    );
    let out = assert_exit(output(c), 1);
    assert!(out.contains("missing:testing/e2e#scenario-body"), "{out}");
    // input.tap still stops at the missing contract surface
    assert!(out.contains("RED  input.tap"), "{out}");
}

#[test]
fn usage_and_manifest_errors_exit_2() {
    let fx = fixture(MANIFEST);
    assert_exit(output(e2e(fx.path(), &["m5"])), 2);
    assert_exit(output(e2e(fx.path(), &[])), 2);
    assert_exit(output(e2e(fx.path(), &["--frobnicate", "all"])), 2);
    let dup = fixture(&format!(
        "{MANIFEST}{}",
        &MANIFEST[MANIFEST.find("[[scenario]]").unwrap()..]
    ));
    let out = assert_exit(output(e2e(dup.path(), &["all"])), 2);
    assert!(out.contains("duplicate id"), "{out}");
}

/// A path with `/` separators: Git Bash on Windows accepts `D:/a/b`, and the script's
/// `dirname "$0"` splits on `/` only.
fn slashed(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// The first `bash` on PATH, as a CI step `shell: bash` runs it. On Windows
/// `Command::new("bash")` searches the system directory before PATH and finds the WSL
/// launcher `System32\bash.exe`; Git Bash is what the script is written for.
fn bash() -> PathBuf {
    if !cfg!(windows) {
        return PathBuf::from("bash");
    }
    let path = std::env::var_os("PATH").expect("PATH is set");
    let system32 =
        PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set")).join("System32");
    let found = std::env::split_paths(&path)
        .map(|d| d.join("bash.exe"))
        .find(|p| p.is_file())
        .expect("bash.exe on PATH (Git Bash)");
    assert!(
        !found.parent().is_some_and(|d| d
            .to_string_lossy()
            .eq_ignore_ascii_case(&system32.to_string_lossy())),
        "the first bash.exe on PATH is the WSL launcher ({}); put Git Bash ahead of System32",
        found.display()
    );
    found
}

/// `bash testing/e2e/run.sh --root <dir> <args>`. The script's `cargo run` gets its own
/// target directory: in the shared one it could relink the `e2e` binary the other tests
/// here execute, which Windows refuses (the file is in use).
fn run_sh(root: &Path, args: &[&str]) -> (i32, String) {
    let mut c = clean(Command::new(bash()));
    c.arg(slashed(&repo_root().join("testing/e2e/run.sh")))
        .arg("--root")
        .arg(slashed(root))
        .args(args)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("run-sh-target"),
        );
    output(c)
}

#[test]
fn run_sh_passes_the_exit_code_through() {
    let fx = fixture(MANIFEST);
    assert_exit(run_sh(fx.path(), &["m1"]), 0);
    let bad = fixture(&MANIFEST.replace(
        "missing:contracts/adp#Target.createTarget",
        "missing:clients/mcp",
    ));
    assert_exit(run_sh(bad.path(), &["m1"]), 1);
    assert_exit(run_sh(fx.path(), &["no-such-target"]), 2);
}

#[test]
fn the_inventory_is_valid_and_covers_every_v1_scope_row() {
    let root = repo_root();
    let rows = droiduse_testing_e2e::manifest::scope_rows(
        &fs::read_to_string(root.join("docs/scope.md")).unwrap(),
    );
    let text = fs::read_to_string(root.join("testing/e2e/scenarios.toml")).unwrap();
    if let Err(errs) = droiduse_testing_e2e::manifest::load(&text, &rows) {
        panic!("testing/e2e/scenarios.toml:\n  {}", errs.join("\n  "));
    }
}
