//! End-to-end exit codes of the commands CI runs: the real `xtask` binary (and
//! `tools/patch/lint.sh`) against fixture repos, each with a clean case (exit 0), a
//! deliberate violation (exit 1) and a case that cannot run (exit 2). The library tests of
//! each checker prove that every rule is detected; these prove that a violation actually
//! fails the CI step.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn copy(root: &Path, rel: &str) {
    let text = fs::read_to_string(repo_root().join(rel)).unwrap();
    write(root, rel, &text);
}

/// Runs `cargo xtask <cmd> <args> --root <root>` as CI does (the built binary).
fn xtask(cmd: &str, args: &[&str], root: &Path) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg(cmd)
        .args(args)
        .arg("--root")
        .arg(root)
        .output()
        .expect("xtask runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().expect("exit code"), text)
}

#[track_caller]
fn assert_exit(got: (i32, String), want: i32) -> String {
    assert_eq!(got.0, want, "output:\n{}", got.1);
    got.1
}

// ---------------------------------------------------------------------------
// check-layers
// ---------------------------------------------------------------------------

/// A one-crate workspace (`host/vmm/boot`) with the real LAYERS.toml and layer LAYER.toml
/// files.
fn layers_fixture(lib_rs: &str, deps: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    copy(root, "LAYERS.toml");
    for l in checklayers::config::load_layers(&repo_root())
        .unwrap()
        .layers
    {
        let lt = l.path.join("LAYER.toml");
        if repo_root().join(&lt).is_file() {
            copy(root, &lt.to_string_lossy());
        }
        // The first-level module directories (empty): LAYER.toml [allow_siblings] names them.
        for e in fs::read_dir(repo_root().join(&l.path)).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                fs::create_dir_all(root.join(&l.path).join(e.file_name())).unwrap();
            }
        }
    }
    write(
        root,
        "Cargo.toml",
        "[workspace]\nresolver = \"3\"\nmembers = [\"host/vmm/boot\", \"host/runtime/instance\"]\n",
    );
    write(
        root,
        "host/vmm/boot/Cargo.toml",
        &format!(
            "[package]\nname = \"boot\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{deps}"
        ),
    );
    write(root, "host/vmm/boot/src/lib.rs", lib_rs);
    write(
        root,
        "host/runtime/instance/Cargo.toml",
        "[package]\nname = \"instance\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write(root, "host/runtime/instance/src/lib.rs", "pub fn f() {}\n");
    dir
}

#[test]
fn check_layers_clean_exits_0() {
    let fx = layers_fixture("pub fn load_container() {}\n", "");
    assert_exit(xtask("check-layers", &[], fx.path()), 0);
}

#[test]
fn check_layers_forbidden_concept_exits_1() {
    let fx = layers_fixture("pub fn parse_android_manifest() {}\n", "");
    let out = assert_exit(xtask("check-layers", &[], fx.path()), 1);
    assert!(out.contains("android"), "{out}");
}

#[test]
fn check_layers_upward_dependency_exits_1() {
    // vmm (L2) may not depend on runtime (L4)
    let fx = layers_fixture(
        "pub fn load_container() {}\n",
        "instance = { path = \"../../runtime/instance\" }\n",
    );
    assert_exit(xtask("check-layers", &[], fx.path()), 1);
}

#[test]
fn check_layers_without_config_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    assert_exit(xtask("check-layers", &[], dir.path()), 2);
}

// ---------------------------------------------------------------------------
// patch-lint (xtask and tools/patch/lint.sh)
// ---------------------------------------------------------------------------

/// The real patch policy, patch directories, docs/scope.md and OWNERS, without patches.
fn patch_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for rel in [
        "third_party/patches.toml",
        "third_party/wick/patches/README.md",
        "third_party/wick/patches/series",
        "third_party/aosp/patches/README.md",
        "third_party/aosp/patches/series",
        "third_party/aosp/security/README.md",
        "third_party/kernel/patches/README.md",
        "docs/scope.md",
        "OWNERS",
    ] {
        copy(dir.path(), rel);
    }
    dir
}

/// A wick patch without the header template or a `droiduse:` marker, listed in the series.
fn add_bad_patch(root: &Path) {
    let name = "0001-droiduse-boot-payload-hook.patch";
    write(
        root,
        &format!("third_party/wick/patches/{name}"),
        "From 1234567890abcdef1234567890abcdef12345678 Mon Sep 17 00:00:00 2001\n\
         From: Dev <dev@example.invalid>\n\
         Date: Thu, 12 Nov 2026 10:00:00 +0900\n\
         Subject: [PATCH] droiduse: add boot payload hook\n\
         \n\
         ---\n\
         \x20src/a.rs | 1 +\n\
         \n\
         diff --git a/src/a.rs b/src/a.rs\n\
         --- a/src/a.rs\n\
         +++ b/src/a.rs\n\
         @@ -1 +1,2 @@\n\
         \x20fn a() {}\n\
         +fn b() {}\n",
    );
    let series = fs::read_to_string(root.join("third_party/wick/patches/series")).unwrap();
    write(
        root,
        "third_party/wick/patches/series",
        &format!("{series}{name}\n"),
    );
}

#[test]
fn patch_lint_clean_exits_0() {
    let fx = patch_fixture();
    assert_exit(xtask("patch-lint", &[], fx.path()), 0);
}

#[test]
fn patch_lint_violation_exits_1() {
    let fx = patch_fixture();
    add_bad_patch(fx.path());
    let out = assert_exit(xtask("patch-lint", &[], fx.path()), 1);
    assert!(out.contains("patch/"), "{out}");
}

#[test]
fn patch_lint_without_policy_exits_2() {
    let fx = patch_fixture();
    fs::remove_file(fx.path().join("third_party/patches.toml")).unwrap();
    assert_exit(xtask("patch-lint", &[], fx.path()), 2);
}

/// A path with `/` separators: Git Bash on Windows accepts `D:/a/b`, and the script's
/// `dirname "$0"` splits on `/` only.
fn slashed(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// The `bash` that the CI step `shell: bash` runs: the first `bash` on PATH (the Actions
/// runner resolves it that way; on windows-2025 that is `C:\Program Files\Git\bin\bash.EXE`).
///
/// On Windows `Command::new("bash")` is not a PATH lookup: Rust searches the system
/// directory before PATH and finds `System32\bash.exe`, the WSL launcher, which on a runner
/// without a WSL distribution exits 1 with "Windows Subsystem for Linux has no installed
/// distributions". So the lookup is done here, the way the runner does it.
fn bash() -> PathBuf {
    if !cfg!(windows) {
        // execvp: a PATH lookup.
        return PathBuf::from("bash");
    }
    let path = std::env::var_os("PATH").expect("PATH is set");
    let found = std::env::split_paths(&path)
        .map(|d| d.join("bash.exe"))
        .find(|p| p.is_file())
        .expect("bash.exe on PATH (Git Bash, as the CI step `shell: bash` needs)");
    let system32 =
        PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set")).join("System32");
    let is_wsl_launcher = found.parent().is_some_and(|d| {
        d.to_string_lossy()
            .eq_ignore_ascii_case(&system32.to_string_lossy())
    });
    assert!(
        !is_wsl_launcher,
        "the first bash.exe on PATH is the WSL launcher ({}); the CI step `shell: bash` \
         would run it too. Put Git Bash ahead of System32 on PATH.",
        found.display()
    );
    found
}

/// `bash tools/patch/lint.sh --root <dir>`, the CI step itself.
///
/// The script's nested `cargo xtask` gets its own target directory. In the shared one it
/// would relink `target/debug/xtask` (`cargo run -p xtask` resolves fewer features than
/// `cargo test --workspace`) while the other tests here execute that file, which Windows
/// refuses (the file is in use).
fn lint_sh(root: &Path) -> (i32, String) {
    let out = Command::new(bash())
        .arg(slashed(&repo_root().join("tools/patch/lint.sh")))
        .arg("--root")
        .arg(slashed(root))
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("lint-sh-target"),
        )
        .output()
        .expect("bash runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().expect("exit code"), text)
}

#[test]
fn lint_sh_passes_the_exit_code_through() {
    let fx = patch_fixture();
    assert_exit(lint_sh(fx.path()), 0);
    add_bad_patch(fx.path());
    assert_exit(lint_sh(fx.path()), 1);
    fs::remove_file(fx.path().join("third_party/patches.toml")).unwrap();
    assert_exit(lint_sh(fx.path()), 2);
}

// ---------------------------------------------------------------------------
// codegen --check (adp target only: no protoc download)
// ---------------------------------------------------------------------------

fn codegen_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "build/codegen/codegen.toml",
        "[adp]\ndefinition = \"contracts/adp/protocol.json\"\nversion = \"contracts/adp/VERSION\"\nout = \"contracts/adp/src/generated\"\n",
    );
    write(
        root,
        "contracts/adp/protocol.json",
        r#"{ "envelope": { "types": [], "binary": "Framing." },
             "domains": [ { "domain": "UI", "types": [ { "id": "NodeId", "type": "string" } ] } ] }"#,
    );
    write(root, "contracts/adp/VERSION", "0.1.0\n");
    dir
}

const GENERATED: &str = "contracts/adp/src/generated/droiduse.adp.rs";

#[test]
fn codegen_check_up_to_date_exits_0() {
    let fx = codegen_fixture();
    assert_exit(xtask("codegen", &[], fx.path()), 0);
    assert_exit(xtask("codegen", &["--check"], fx.path()), 0);
}

#[test]
fn codegen_check_missing_output_exits_1() {
    let fx = codegen_fixture();
    let out = assert_exit(xtask("codegen", &["--check"], fx.path()), 1);
    assert!(out.contains("codegen/missing"), "{out}");
    assert!(
        !fx.path().join(GENERATED).exists(),
        "--check must not write"
    );
}

#[test]
fn codegen_check_hand_edit_exits_1() {
    let fx = codegen_fixture();
    assert_exit(xtask("codegen", &[], fx.path()), 0);
    let p = fx.path().join(GENERATED);
    let text = fs::read_to_string(&p).unwrap();
    fs::write(&p, text.replace("pub struct NodeId", "pub struct NodeID")).unwrap();
    let out = assert_exit(xtask("codegen", &["--check"], fx.path()), 1);
    assert!(out.contains("codegen/hand-edited"), "{out}");
}

#[test]
fn codegen_check_stale_output_exits_1() {
    let fx = codegen_fixture();
    assert_exit(xtask("codegen", &[], fx.path()), 0);
    write(fx.path(), "contracts/adp/VERSION", "0.2.0\n");
    let out = assert_exit(xtask("codegen", &["--check"], fx.path()), 1);
    assert!(out.contains("codegen/stale"), "{out}");
}

#[test]
fn codegen_check_invalid_definition_exits_2() {
    let fx = codegen_fixture();
    write(
        fx.path(),
        "contracts/adp/protocol.json",
        "{ \"domains\": [] }",
    );
    assert_exit(xtask("codegen", &["--check"], fx.path()), 2);
}

#[test]
fn unknown_command_or_argument_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    assert_exit(xtask("no-such-command", &[], dir.path()), 2);
    assert_exit(xtask("codegen", &["--frobnicate"], dir.path()), 2);
}
