//! Fixture-workspace tests: every rule class fails when violated and passes when clean.
//!
//! Each fixture is a throwaway repo in a temp dir that uses the repo's *real* `LAYERS.toml`
//! and layer-root `LAYER.toml` files, so these tests prove that the rules as currently
//! written catch the violations. Tests that need a different configuration overwrite
//! single files in the fixture.

use std::fs;
use std::path::{Path, PathBuf};

use checklayers::{Report, Rule};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Fx {
    dir: tempfile::TempDir,
    members: Vec<String>,
    /// Crates written so far: (path, deps, extra [package] lines).
    crates: Vec<(String, Vec<String>, String)>,
}

impl Fx {
    /// Empty repo with the real layer configuration.
    fn empty() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let real = repo_root();
        fs::copy(real.join("LAYERS.toml"), dir.path().join("LAYERS.toml")).unwrap();
        let (layers, _) = checklayers::config::load_layers(&real).unwrap();
        for l in layers {
            let d = dir.path().join(&l.path);
            fs::create_dir_all(&d).unwrap();
            let lt = real.join(&l.path).join("LAYER.toml");
            if lt.is_file() {
                fs::copy(&lt, d.join("LAYER.toml")).unwrap();
            }
        }
        Fx {
            dir,
            members: Vec::new(),
            crates: Vec::new(),
        }
    }

    /// A repo that follows every rule, covering each layer and the allowed edge kinds.
    fn clean() -> Fx {
        let mut fx = Fx::empty();
        fx.krate("contracts/common", &[])
            .krate("contracts/machine", &["contracts/common"])
            .krate("host/base/ipc", &[])
            .krate("host/vmm/boot", &["host/base/ipc", "contracts/machine"])
            .krate("host/devices/gpu", &["host/base/ipc", "contracts/common"])
            .krate("host/runtime/snapshot", &["contracts/machine"])
            .krate(
                "host/runtime/instance",
                &[
                    "host/vmm/boot",
                    "host/base/ipc",
                    "contracts/machine",
                    // Same layer, not under forbid_siblings.
                    "host/runtime/snapshot",
                ],
            )
            .krate("host/services/inspect/proto", &["contracts/common"])
            .krate(
                "host/services/inspect",
                &[
                    "host/runtime/instance",
                    "contracts/common",
                    // Same module (inspect) under forbid_siblings: allowed.
                    "host/services/inspect/proto",
                ],
            )
            .krate("host/services/screen", &["host/runtime/instance"])
            .krate(
                "host/entrypoints/cli",
                &[
                    "host/services/inspect",
                    "host/services/screen",
                    "host/runtime/instance",
                ],
            )
            .krate("clients/sdk", &["contracts/common"])
            .krate("testing/support", &[])
            .krate(
                "testing/e2e",
                &[
                    "host/entrypoints/cli",
                    "clients/sdk",
                    "host/vmm/boot",
                    "testing/support",
                ],
            );
        // Upstream crate: not a workspace member, exempt from the coverage check.
        fx.file(
            "third_party/wick/src/Cargo.toml",
            "[package]\nname = \"wick\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        fx.file("third_party/wick/src/src/lib.rs", "");
        // Forbidden words only in comments and documents: allowed.
        fx.file(
            "host/vmm/boot/src/lib.rs",
            "//! Loads the boot image container; Android semantics live in runtime.\n\
             /// Not an android activity parser.\n\
             /* aosp package_name\n /* nested: android */ still a comment */\n\
             pub fn load_container() {}\n",
        );
        fx.file(
            "host/vmm/boot/README.md",
            "# boot\nNo QEMU, no tenant, no billing, no Android here.\n",
        );
        fx.file(
            "host/runtime/instance/src/lib.rs",
            "// tenant labels are opaque\npub fn f() {}\n",
        );
        fx
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn file(&mut self, rel: &str, content: &str) -> &mut Fx {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
        self
    }

    /// Writes a crate at `rel` with path dependencies on other fixture crates.
    /// A dep written as `dev:<path>` becomes a dev-dependency.
    fn krate(&mut self, rel: &str, deps: &[&str]) -> &mut Fx {
        self.krate_with(rel, deps, "")
    }

    fn krate_with(&mut self, rel: &str, deps: &[&str], extra: &str) -> &mut Fx {
        self.write_crate(rel, deps, extra);
        self.members.push(rel.to_string());
        self
    }

    /// A crate that is *not* listed as a workspace member.
    fn krate_unlisted(&mut self, rel: &str) -> &mut Fx {
        self.write_crate(rel, &[], "");
        self
    }

    /// Adds dependencies to an existing fixture crate (rewrites its Cargo.toml).
    fn add_deps(&mut self, rel: &str, deps: &[&str]) -> &mut Fx {
        let (_, old, extra) = self
            .crates
            .iter()
            .find(|(p, _, _)| p == rel)
            .cloned()
            .expect("add_deps on unknown fixture crate");
        let mut all: Vec<&str> = old.iter().map(String::as_str).collect();
        all.extend_from_slice(deps);
        self.write_crate(rel, &all, &extra);
        self
    }

    fn write_crate(&mut self, rel: &str, deps: &[&str], extra: &str) {
        self.crates.retain(|(p, _, _)| p != rel);
        self.crates.push((
            rel.to_string(),
            deps.iter().map(|d| d.to_string()).collect(),
            extra.to_string(),
        ));
        let name = crate_name(rel);
        let toml = format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{extra}\n{}",
            dep_lines(rel, deps)
        );
        self.file(&format!("{rel}/Cargo.toml"), &toml);
        let lib = self.path(rel).join("src/lib.rs");
        if !lib.exists() {
            self.file(&format!("{rel}/src/lib.rs"), "");
        }
    }

    fn run(&self) -> Report {
        let members: Vec<String> = self.members.iter().map(|m| format!("\"{m}\"")).collect();
        fs::write(
            self.path("Cargo.toml"),
            format!(
                "[workspace]\nresolver = \"3\"\nmembers = [{}]\nexclude = [\"third_party\"]\n",
                members.join(", ")
            ),
        )
        .unwrap();
        checklayers::check(self.dir.path()).expect("check must run")
    }
}

fn crate_name(rel: &str) -> String {
    rel.replace('/', "-")
}

fn dep_lines(from: &str, deps: &[&str]) -> String {
    let up = "../".repeat(from.split('/').count());
    let mut normal = String::new();
    let mut dev = String::new();
    for d in deps {
        let (section, target) = match d.strip_prefix("dev:") {
            Some(t) => (&mut dev, t),
            None => (&mut normal, *d),
        };
        section.push_str(&format!(
            "{} = {{ path = \"{up}{target}\" }}\n",
            crate_name(target)
        ));
    }
    let mut s = String::new();
    if !normal.is_empty() {
        s.push_str("[dependencies]\n");
        s.push_str(&normal);
    }
    if !dev.is_empty() {
        s.push_str("[dev-dependencies]\n");
        s.push_str(&dev);
    }
    s
}

fn rules(r: &Report) -> Vec<Rule> {
    let mut v: Vec<Rule> = r.violations.iter().map(|v| v.rule).collect();
    v.sort();
    v.dedup();
    v
}

fn dump(r: &Report) -> String {
    r.violations
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Asserts the report contains exactly these rule kinds (in `Rule` order) and nothing else.
#[track_caller]
fn assert_rules(r: &Report, expected: &[Rule]) {
    assert_eq!(rules(r), expected, "violations:\n{}", dump(r));
}

#[track_caller]
fn assert_message(r: &Report, needle: &str) {
    assert!(
        r.violations.iter().any(|v| v.to_string().contains(needle)),
        "no violation mentions `{needle}`; violations:\n{}",
        dump(r)
    );
}

// ---------------------------------------------------------------- clean

#[test]
fn clean_repo_passes() {
    let r = Fx::clean().run();
    assert!(r.is_clean(), "violations:\n{}", dump(&r));
    assert_eq!(r.stats.crates_checked, 14);
}

// ---------------------------------------------------------------- dependency rules

#[test]
fn upward_dependency_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("host/base/ipc", &["host/runtime/snapshot"]);
    let r = fx.run();
    assert_rules(&r, &[Rule::Upward]);
    assert_message(&r, "host/base/ipc/Cargo.toml: dep/upward");
}

#[test]
fn same_level_sibling_layers_fail() {
    let mut fx = Fx::clean();
    fx.add_deps("host/vmm/boot", &["host/devices/gpu"]);
    assert_rules(&fx.run(), &[Rule::SameLevel]);
}

#[test]
fn sibling_modules_under_forbid_siblings_fail() {
    let mut fx = Fx::clean();
    fx.add_deps("host/services/screen", &["host/services/inspect"]);
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbidSiblings]);
    assert_message(&r, "`screen` -> `inspect`");
}

#[test]
fn contracts_depending_on_any_layer_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("contracts/machine", &["host/base/ipc"]);
    assert_rules(&fx.run(), &[Rule::ContractsSelfContained]);

    let mut fx = Fx::clean();
    // Upstream crates are not workspace members (root excludes third_party).
    fx.krate_unlisted("third_party/patched");
    fx.add_deps("contracts/common", &["third_party/patched"]);
    assert_rules(&fx.run(), &[Rule::ContractsSelfContained]);
}

#[test]
fn product_code_depending_on_clients_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("host/entrypoints/cli", &["clients/sdk"]);
    assert_rules(&fx.run(), &[Rule::DepOnClients]);
}

#[test]
fn clients_depending_on_host_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("clients/sdk", &["host/base/ipc"]);
    assert_rules(&fx.run(), &[Rule::ClientsOnlyContracts]);
}

#[test]
fn product_code_depending_on_testing_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("host/runtime/instance", &["testing/support"]);
    assert_rules(&fx.run(), &[Rule::DepOnTesting]);

    // dev-dependencies count too.
    let mut fx = Fx::clean();
    fx.add_deps("host/vmm/boot", &["dev:testing/support"]);
    let r = fx.run();
    assert_rules(&r, &[Rule::DepOnTesting]);
    assert_message(&r, "dev-dependency");

    // Even contracts and clients may not.
    let mut fx = Fx::clean();
    fx.add_deps("clients/sdk", &["testing/support"]);
    assert_rules(&fx.run(), &[Rule::DepOnTesting]);
}

#[test]
fn layer_toml_allow_list_narrows() {
    // runtime (L4) -> devices (L2) is allowed by LAYERS.toml levels, but
    // host/runtime/LAYER.toml allow = ["base", "contracts", "vmm"] excludes it.
    let mut fx = Fx::clean();
    fx.add_deps("host/runtime/instance", &["host/devices/gpu"]);
    let r = fx.run();
    assert_rules(&r, &[Rule::NotAllowed]);
    assert_message(&r, "host/runtime/LAYER.toml");

    // base allow = ["third_party"]: even contracts is excluded.
    let mut fx = Fx::clean();
    fx.add_deps("host/base/ipc", &["contracts/common"]);
    assert_rules(&fx.run(), &[Rule::NotAllowed]);
}

#[test]
fn nested_layer_toml_narrows_further() {
    let mut fx = Fx::clean();
    fx.file(
        "host/services/inspect/LAYER.toml",
        "[layer]\nname = \"services\"\nallow = [\"contracts\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &[Rule::NotAllowed]);
    assert_message(&r, "host/services/inspect/LAYER.toml");
}

#[test]
fn layer_toml_cannot_widen() {
    let mut fx = Fx::clean();
    fx.file(
        "host/base/LAYER.toml",
        "[layer]\nname = \"base\"\nallow = [\"third_party\", \"runtime\"]\n",
    );
    // A widening entry is a violation even before any crate uses it.
    let r = fx.run();
    assert_rules(&r, &[Rule::ConfigWidens]);

    // And it does not let the dependency through.
    fx.add_deps("host/base/ipc", &["host/runtime/snapshot"]);
    assert_rules(&fx.run(), &[Rule::Upward, Rule::ConfigWidens]);
}

#[test]
fn malformed_layer_toml_fails() {
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/LAYER.toml",
        "[layer]\nname = \"vmm\"\nalow = [\"base\"]\n",
    );
    assert_rules(&fx.run(), &[Rule::ConfigInvalid]);

    let mut fx = Fx::clean();
    fx.file("host/vmm/LAYER.toml", "[layer]\nname = \"devices\"\n");
    assert_rules(&fx.run(), &[Rule::ConfigInvalid]);

    // An unknown allow entry is reported; the list still narrows (vmm's real deps now fail).
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/LAYER.toml",
        "[layer]\nname = \"vmm\"\nallow = [\"nonexistent\"]\n",
    );
    assert_rules(&fx.run(), &[Rule::NotAllowed, Rule::ConfigInvalid]);
}

#[test]
fn testing_may_depend_on_everything() {
    // Covered by clean_repo_passes (testing/e2e -> entrypoints, clients, vmm); make it explicit.
    let mut fx = Fx::clean();
    fx.add_deps(
        "testing/support",
        &["host/devices/gpu", "contracts/common", "host/base/ipc"],
    );
    assert!(fx.run().is_clean());
}

#[test]
fn dependency_on_tooling_or_unlayered_path_fails() {
    let mut fx = Fx::clean();
    fx.krate("build/helper", &[]);
    fx.add_deps("host/vmm/boot", &["build/helper"]);
    assert_rules(&fx.run(), &[Rule::UnlayeredTarget]);
}

#[test]
fn crate_outside_any_layer_fails() {
    let mut fx = Fx::clean();
    fx.krate("misc/thing", &[]);
    assert_rules(&fx.run(), &[Rule::UnlayeredCrate]);
}

#[test]
fn crate_not_in_workspace_fails() {
    let mut fx = Fx::clean();
    fx.krate_unlisted("host/vmm/disk");
    let r = fx.run();
    assert_rules(&r, &[Rule::NotInWorkspace]);
    assert_message(&r, "host/vmm/disk/Cargo.toml");
}

// ---------------------------------------------------------------- forbidden concepts

#[test]
fn forbidden_concept_in_identifier_fails() {
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub fn parse_android_boot() {}\n",
    );
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_message(
        &r,
        "host/vmm/boot/src/lib.rs:1: concept/forbidden: \"android\" in identifier",
    );

    // Multi-word concept, camel case.
    let mut fx = Fx::clean();
    fx.file("host/devices/gpu/src/lib.rs", "pub struct PackageName;\n");
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_message(&r, "\"package_name\"");

    // "*" concepts apply to every layer; case-insensitive.
    let mut fx = Fx::clean();
    fx.file(
        "host/runtime/instance/src/lib.rs",
        "pub struct QEMURunner;\n",
    );
    assert_rules(&fx.run(), &[Rule::ForbiddenConcept]);

    let mut fx = Fx::clean();
    fx.file(
        "host/services/screen/src/lib.rs",
        "pub fn f(tenant_id: u32) {}\n",
    );
    assert_rules(&fx.run(), &[Rule::ForbiddenConcept]);
}

#[test]
fn layer_specific_concepts_do_not_leak_to_other_layers() {
    // "android" is forbidden in vmm/devices only.
    let mut fx = Fx::clean();
    fx.file(
        "host/runtime/instance/src/lib.rs",
        "pub fn android_boot_config() {}\n",
    );
    assert!(fx.run().is_clean());
}

#[test]
fn forbidden_concept_in_string_literal_fails() {
    let mut fx = Fx::clean();
    fx.file(
        "host/devices/gpu/src/lib.rs",
        "pub const S: &str = \"Android guest\";\n",
    );
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_message(&r, "string literal");

    // `//` inside a (raw) string is not a comment.
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub const U: &str = r#\"see http://x/aosp\"#;\n",
    );
    assert_rules(&fx.run(), &[Rule::ForbiddenConcept]);

    // Non-Rust files are checked too (TOML string in Cargo.toml metadata).
    let mut fx = Fx::clean();
    fx.krate_with("host/vmm/api", &[], "description = \"VM API for Android\"");
    assert_rules(&fx.run(), &[Rule::ForbiddenConcept]);
}

#[test]
fn forbidden_word_only_in_comments_or_docs_passes() {
    let mut fx = Fx::clean();
    fx.file(
        "host/devices/gpu/src/lib.rs",
        "// android\n/// AOSP activity\n//! package_name\n/* qemu */\npub fn f() {}\n",
    );
    fx.file(
        "host/devices/gpu/NOTES.md",
        "android aosp qemu tenant billing\n",
    );
    fx.file(
        "host/devices/gpu/build.sh",
        "# android here is a comment\necho ok\n",
    );
    let r = fx.run();
    assert!(r.is_clean(), "violations:\n{}", dump(&r));
}

#[test]
fn forbidden_concept_in_file_or_dir_name_fails() {
    let mut fx = Fx::clean();
    fx.file("host/vmm/boot/src/aosp.rs", "");
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_message(&r, "file name `aosp.rs`");

    let mut fx = Fx::clean();
    fx.file("host/devices/gpu/src/activity/mod.rs", "");
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_message(&r, "directory name `activity`");
}

#[test]
fn forbidden_concept_in_dependency_crate_name_fails() {
    let mut fx = Fx::clean();
    let toml = fs::read_to_string(fx.path("host/vmm/boot/Cargo.toml")).unwrap();
    fx.file(
        "host/vmm/boot/Cargo.toml",
        &format!("{toml}\n[target.'cfg(unix)'.dependencies]\nlogger = {{ package = \"android_logger\", version = \"0.13\" }}\n"),
    );
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_message(&r, "dependency crate name `android_logger`");
}

#[test]
fn concept_matching_is_word_level() {
    let mut fx = Fx::clean();
    fx.file("host/vmm/boot/src/lib.rs", "pub fn subactivity() {}\n");
    assert!(fx.run().is_clean());

    fx.file("host/vmm/boot/src/lib.rs", "pub fn sub_activity() {}\n");
    assert_rules(&fx.run(), &[Rule::ForbiddenConcept]);
}

#[test]
fn layer_toml_forbid_concepts_apply() {
    // clients/LAYER.toml forbid_concepts = ["tenant", "billing"].
    let mut fx = Fx::clean();
    fx.file("clients/sdk/src/lib.rs", "pub fn billing() {}\n");
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_message(&r, "clients/LAYER.toml");

    // A nested LAYER.toml adds concepts for its subtree only.
    let mut fx = Fx::clean();
    fx.file(
        "host/services/inspect/LAYER.toml",
        "[layer]\nforbid_concepts = [\"pixel\"]\n",
    );
    fx.file("host/services/inspect/src/lib.rs", "pub fn pixel() {}\n");
    fx.file("host/services/screen/src/lib.rs", "pub fn pixel() {}\n");
    let r = fx.run();
    assert_rules(&r, &[Rule::ForbiddenConcept]);
    assert_eq!(r.violations.len(), 1, "{}", dump(&r));
    assert_message(&r, "host/services/inspect/src/lib.rs:1");
}
