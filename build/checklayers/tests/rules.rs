//! Fixture-workspace tests: every rule class fails when violated and passes when clean.
//!
//! Each fixture is a throwaway repo in a temp dir that uses the repo's *real* `LAYERS.toml`
//! and layer-root `LAYER.toml` files, so these tests prove that the rules as currently
//! written catch the violations. Tests that need a different configuration overwrite
//! single files in the fixture. Assertions compare the exact rule ids reported.

use std::fs;
use std::path::{Path, PathBuf};

use checklayers::Report;

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
        let global = checklayers::config::load_layers(&real).unwrap();
        for l in global.layers {
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
        // The real contracts graph and its allow_siblings edges.
        fx.krate("contracts/common", &[])
            .krate("contracts/spi", &["contracts/common"])
            .krate("contracts/machine", &["contracts/common", "contracts/spi"])
            .krate("contracts/instance", &["contracts/common", "contracts/spi"])
            .krate("contracts/device", &[])
            .krate("host/base/ipc", &[])
            .krate("host/vmm/boot", &["host/base/ipc", "contracts/machine"])
            .krate(
                "host/devices/gpu",
                &["host/base/ipc", "contracts/common", "contracts/device"],
            )
            .krate("host/runtime/snapshot", &["contracts/machine"])
            .krate(
                "host/runtime/instance",
                &[
                    "host/vmm/boot",
                    "host/base/ipc",
                    "contracts/machine",
                    "contracts/instance",
                ],
            )
            .krate("host/services/inspect/proto", &["contracts/common"])
            .krate(
                "host/services/inspect",
                &[
                    "host/runtime/instance",
                    "contracts/common",
                    // Same first-level module (inspect): allowed.
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
            .krate(
                "testing/e2e",
                &["host/entrypoints/cli", "clients/sdk", "host/vmm/boot"],
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

    /// Appends text to an existing fixture file.
    fn append(&mut self, rel: &str, text: &str) -> &mut Fx {
        let old = fs::read_to_string(self.path(rel)).unwrap();
        self.file(rel, &format!("{old}\n{text}"))
    }

    /// Replaces `from` (which must occur in the file) with `to` in a fixture file.
    fn edit(&mut self, rel: &str, from: &str, to: &str) -> &mut Fx {
        let old = fs::read_to_string(self.path(rel)).unwrap();
        assert!(old.contains(from), "`{from}` not found in {rel}");
        self.file(rel, &old.replacen(from, to, 1))
    }

    /// Sets `[contracts] allow_external` in the fixture's LAYERS.toml (parsed, not matched as
    /// text, so the test does not depend on how the real file spells the list).
    fn set_allow_external(&mut self, names: &[&str]) -> &mut Fx {
        let mut doc: toml::Table =
            toml::from_str(&fs::read_to_string(self.path("LAYERS.toml")).unwrap()).unwrap();
        let contracts = doc
            .entry("contracts")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .expect("[contracts] is a table");
        contracts.insert(
            "allow_external".into(),
            toml::Value::Array(names.iter().map(|n| toml::Value::from(*n)).collect()),
        );
        let text = toml::to_string(&doc).unwrap();
        self.file("LAYERS.toml", &text)
    }
}

/// `[contracts] allow_external` of the real LAYERS.toml.
fn real_allow_external() -> Vec<String> {
    let doc: toml::Table =
        toml::from_str(&fs::read_to_string(repo_root().join("LAYERS.toml")).unwrap()).unwrap();
    doc.get("contracts")
        .and_then(|c| c.get("allow_external"))
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .map(|v| v.as_str().expect("crate name").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Crate names that are certainly not in the real allowlist (`cargo metadata --no-deps` does
/// not resolve them, so they need not exist).
fn unlisted(n: usize) -> Vec<String> {
    let real = real_allow_external();
    let names: Vec<String> = (0..n).map(|i| format!("droiduse-unlisted-{i}")).collect();
    assert!(names.iter().all(|n| !real.contains(n)));
    names
}

impl Fx {
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

/// Rule ids reported, sorted and deduplicated.
fn ids(r: &Report) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = r.violations.iter().map(|v| v.rule.id()).collect();
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

/// Asserts the report contains exactly these rule ids (any order) and nothing else.
#[track_caller]
fn assert_rules(r: &Report, expected: &[&str]) {
    let mut want = expected.to_vec();
    want.sort();
    want.dedup();
    assert_eq!(ids(r), want, "violations:\n{}", dump(r));
}

#[track_caller]
fn assert_clean(r: &Report) {
    assert!(r.is_clean(), "violations:\n{}", dump(r));
}

#[track_caller]
fn assert_message(r: &Report, needle: &str) {
    assert!(
        r.violations.iter().any(|v| v.to_string().contains(needle)),
        "no violation mentions `{needle}`; violations:\n{}",
        dump(r)
    );
}

#[track_caller]
fn assert_no_message(r: &Report, needle: &str) {
    assert!(
        !r.violations.iter().any(|v| v.to_string().contains(needle)),
        "a violation mentions `{needle}`; violations:\n{}",
        dump(r)
    );
}

// ---------------------------------------------------------------- clean

#[test]
fn clean_repo_passes() {
    let r = Fx::clean().run();
    assert_clean(&r);
    assert_eq!(r.stats.crates_checked, 16);
}

#[test]
fn crlf_config_and_sources_are_read_the_same() {
    // A checkout with CRLF line endings (the repo's .gitattributes prevents it, but the
    // checker must not depend on that): same result, violations still caught.
    let mut fx = Fx::clean();
    let mut crlf = vec![
        "LAYERS.toml".to_string(),
        "host/vmm/boot/src/lib.rs".to_string(),
    ];
    for l in checklayers::config::load_layers(&repo_root())
        .unwrap()
        .layers
    {
        let lt = l.path.join("LAYER.toml");
        if fx.path(&lt.to_string_lossy()).is_file() {
            crlf.push(lt.to_string_lossy().into_owned());
        }
    }
    for rel in &crlf {
        let text = fs::read_to_string(fx.path(rel)).unwrap();
        fx.file(rel, &text.replace("\r\n", "\n").replace('\n', "\r\n"));
    }
    assert_clean(&fx.run());
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub fn load_container() {}\r\npub fn android_props() {}\r\n",
    );
    assert_rules(&fx.run(), &["concept/forbidden"]);
}

// ---------------------------------------------------------------- dependency rules

#[test]
fn upward_dependency_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("host/base/ipc", &["host/runtime/snapshot"]);
    let r = fx.run();
    assert_rules(&r, &["dep/upward"]);
    assert_message(&r, "host/base/ipc/Cargo.toml: dep/upward");
}

#[test]
fn same_level_sibling_layers_fail() {
    let mut fx = Fx::clean();
    fx.add_deps("host/vmm/boot", &["host/devices/gpu"]);
    assert_rules(&fx.run(), &["dep/same-level"]);
}

#[test]
fn sibling_modules_fail_in_every_layer() {
    // services (previously the only forbid_siblings layer).
    let mut fx = Fx::clean();
    fx.add_deps("host/services/screen", &["host/services/inspect"]);
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(
        &r,
        "host/services/screen/Cargo.toml: dep/sibling-module: dependency `host-services-inspect`",
    );
    assert_message(&r, "`screen` -> `inspect`");

    // runtime: sibling modules were allowed before; now forbidden by default.
    let mut fx = Fx::clean();
    fx.add_deps("host/runtime/instance", &["host/runtime/snapshot"]);
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(&r, "`instance` -> `snapshot`");

    // A nested crate of one module depending on another module.
    let mut fx = Fx::clean();
    fx.add_deps("host/services/inspect/proto", &["host/services/screen"]);
    assert_rules(&fx.run(), &["dep/sibling-module"]);

    // dev-dependencies count too.
    let mut fx = Fx::clean();
    fx.krate("host/vmm/disk", &[]);
    fx.add_deps("host/vmm/boot", &["dev:host/vmm/disk"]);
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(&r, "dev-dependency");

    // A crate at the layer root is a module of its own.
    let mut fx = Fx::clean();
    fx.krate("host/devices", &["host/devices/gpu"]);
    assert_rules(&fx.run(), &["dep/sibling-module"]);
}

#[test]
fn same_module_dependencies_pass() {
    let mut fx = Fx::clean();
    fx.krate("host/runtime/snapshot/store", &[]);
    fx.add_deps("host/runtime/snapshot", &["host/runtime/snapshot/store"]);
    fx.krate(
        "host/runtime/snapshot/diff",
        &["host/runtime/snapshot/store"],
    );
    assert_clean(&fx.run());
}

#[test]
fn testing_layer_has_no_sibling_exception() {
    // Owner decision: testing gets no allow_siblings edge.
    let mut fx = Fx::clean();
    fx.krate("testing/support", &[]);
    fx.add_deps("testing/e2e", &["testing/support"]);
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(&r, "`e2e` -> `support`");
}

#[test]
fn contracts_sibling_edges_are_directional() {
    // contracts/LAYER.toml lists machine/instance -> common, spi and spi -> common:
    // the real graph (Fx::clean) passes.
    assert_clean(&Fx::clean().run());

    // An unlisted edge to a target other modules may use: device has no edge.
    let mut fx = Fx::clean();
    fx.add_deps("contracts/device", &["contracts/common"]);
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(&r, "`device` -> `common`");

    // An unlisted edge between modules that do have other edges.
    let mut fx = Fx::clean();
    fx.add_deps("contracts/machine", &["contracts/instance"]);
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(&r, "`machine` -> `instance`");

    // The reverse of a listed edge: common -> device is not device -> common.
    let mut fx = Fx::clean();
    fx.edit(
        "contracts/LAYER.toml",
        "[allow_siblings]\n",
        "[allow_siblings]\ndevice = [\"common\"]\n",
    );
    fx.add_deps("contracts/common", &["contracts/device"]);
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(&r, "`common` -> `device`");
}

#[test]
fn allow_siblings_edge_applies_in_its_direction_only() {
    let mut fx = Fx::clean();
    fx.add_deps("host/services/screen", &["host/services/inspect"]);
    fx.append(
        "host/services/LAYER.toml",
        "[allow_siblings]\nscreen = [\"inspect\"]\n",
    );
    assert_clean(&fx.run());

    // Reverse edge: inspect -> screen is not listed.
    let mut fx = Fx::clean();
    fx.add_deps("host/services/inspect", &["host/services/screen"]);
    fx.append(
        "host/services/LAYER.toml",
        "[allow_siblings]\nscreen = [\"inspect\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &["dep/sibling-module"]);
    assert_message(&r, "`inspect` -> `screen`");
}

#[test]
fn allow_siblings_must_be_one_directional() {
    // Both directions: rejected, and no edge of the table is applied.
    let mut fx = Fx::clean();
    fx.add_deps("host/services/screen", &["host/services/inspect"]);
    fx.append(
        "host/services/LAYER.toml",
        "[allow_siblings]\nscreen = [\"inspect\"]\ninspect = [\"screen\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &["config/invalid", "dep/sibling-module"]);
    assert_message(
        &r,
        "host/services/LAYER.toml: config/invalid: allow_siblings lists `inspect` and `screen` in both directions",
    );

    // A 3-cycle: rejected the same way.
    let mut fx = Fx::clean();
    fx.krate("host/services/input", &["host/runtime/instance"]);
    fx.add_deps("host/services/screen", &["host/services/inspect"]);
    fx.append(
        "host/services/LAYER.toml",
        "[allow_siblings]\nscreen = [\"inspect\"]\ninspect = [\"input\"]\ninput = [\"screen\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &["config/invalid", "dep/sibling-module"]);
    assert_message(
        &r,
        "allow_siblings edges form a cycle among `input`, `inspect`, `screen`",
    );
    assert_message(&r, "`screen` -> `inspect`");

    // A self edge is meaningless (the same module is always allowed) and rejected.
    let mut fx = Fx::clean();
    fx.append(
        "host/services/LAYER.toml",
        "[allow_siblings]\nscreen = [\"screen\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &["config/invalid"]);
    assert_message(&r, "`screen` -> `screen`");
}

#[test]
fn allow_siblings_cannot_be_abused() {
    // Only in the layer-root LAYER.toml: a nested file may only narrow.
    let mut fx = Fx::clean();
    fx.add_deps("host/services/inspect", &["host/services/screen"]);
    fx.file(
        "host/services/inspect/LAYER.toml",
        "[layer]\n\n[allow_siblings]\ninspect = [\"screen\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &["config/widens", "dep/sibling-module"]);
    assert_message(&r, "host/services/inspect/LAYER.toml: config/widens");

    // No wildcard, as source or as target.
    for table in [
        "[allow_siblings]\nscreen = [\"*\"]\n",
        "[allow_siblings]\n\"*\" = [\"inspect\"]\n",
    ] {
        let mut fx = Fx::clean();
        fx.add_deps("host/services/screen", &["host/services/inspect"]);
        fx.append("host/services/LAYER.toml", table);
        assert_rules(&fx.run(), &["config/invalid", "dep/sibling-module"]);
    }

    // Entries are first-level module names of this layer, never paths to other layers.
    let mut fx = Fx::clean();
    fx.add_deps("host/vmm/boot", &["host/devices/gpu"]);
    fx.append(
        "host/vmm/LAYER.toml",
        "[allow_siblings]\nboot = [\"../devices\", \"host/devices/gpu\", \"gpu\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &["config/invalid", "dep/same-level"]);
    assert_message(&r, "`../devices`");
    assert_message(&r, "`host/devices/gpu`");
    // `gpu` is no module of vmm.
    assert_message(&r, "`gpu`");

    // A sibling exception does not lift LAYER.toml `allow` lists or global rules:
    // `contracts` in base is just an unknown module.
    let mut fx = Fx::clean();
    fx.add_deps("host/base/ipc", &["contracts/common"]);
    fx.append(
        "host/base/LAYER.toml",
        "[allow_siblings]\nipc = [\"contracts\"]\n",
    );
    assert_rules(&fx.run(), &["config/invalid", "dep/layer-toml-allow"]);

    // The old per-target list form is gone.
    let mut fx = Fx::clean();
    fx.edit(
        "host/services/LAYER.toml",
        "[layer]\n",
        "[layer]\nallow_siblings = [\"inspect\"]\n",
    );
    assert_rules(&fx.run(), &["config/invalid"]);
}

#[test]
fn forbid_siblings_key_is_gone() {
    let mut fx = Fx::clean();
    fx.edit(
        "host/runtime/LAYER.toml",
        "[layer]\n",
        "[layer]\nforbid_siblings = false\n",
    );
    assert_rules(&fx.run(), &["config/invalid"]);
}

#[test]
fn contracts_depending_on_any_layer_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("contracts/machine", &["host/base/ipc"]);
    assert_rules(&fx.run(), &["dep/contracts-self-contained"]);

    let mut fx = Fx::clean();
    // Upstream crates are not workspace members (root excludes third_party).
    fx.krate_unlisted("third_party/patched");
    fx.add_deps("contracts/common", &["third_party/patched"]);
    assert_rules(&fx.run(), &["dep/contracts-self-contained"]);
}

#[test]
fn contracts_external_dependency_fails() {
    // Normal, build, dev and target-specific dependencies all count.
    let names = unlisted(4);
    for (section, name) in [
        "dependencies",
        "build-dependencies",
        "dev-dependencies",
        "target.'cfg(unix)'.dependencies",
    ]
    .into_iter()
    .zip(&names)
    {
        let mut fx = Fx::clean();
        fx.append(
            "contracts/device/Cargo.toml",
            &format!("[{section}]\n{name} = \"1\"\n"),
        );
        let r = fx.run();
        assert_rules(&r, &["dep/contracts-external"]);
        assert_message(&r, "contracts/device/Cargo.toml: dep/contracts-external");
        assert_message(&r, &format!("`{name}`"));
    }

    // Renaming does not hide the real crate name.
    let mut fx = Fx::clean();
    fx.append(
        "contracts/device/Cargo.toml",
        &format!(
            "[dependencies]\nalias = {{ package = \"{}\", version = \"1\" }}\n",
            names[0]
        ),
    );
    let r = fx.run();
    assert_rules(&r, &["dep/contracts-external"]);
    assert_message(&r, &format!("`{}`", names[0]));

    // Other layers are not restricted by the contracts allowlist.
    let mut fx = Fx::clean();
    fx.append(
        "host/base/ipc/Cargo.toml",
        &format!("[dependencies]\n{} = \"1\"\n", names[0]),
    );
    assert_clean(&fx.run());
}

#[test]
fn contracts_external_allowlist_applies() {
    // Every entry of the real allowlist passes, spread over every dependency kind.
    let real = real_allow_external();
    let sections = [
        "dependencies",
        "build-dependencies",
        "dev-dependencies",
        "target.'cfg(unix)'.dependencies",
    ];
    let mut by_section: Vec<Vec<&str>> = vec![Vec::new(); sections.len()];
    for (i, name) in real.iter().enumerate() {
        by_section[i % sections.len()].push(name);
    }
    let mut text = String::new();
    for (section, names) in sections.iter().zip(&by_section) {
        if names.is_empty() {
            continue;
        }
        text.push_str(&format!("[{section}]\n"));
        for n in names {
            text.push_str(&format!("{n} = \"1\"\n"));
        }
    }
    let mut fx = Fx::clean();
    fx.append("contracts/device/Cargo.toml", &text);
    assert_clean(&fx.run());

    // An entry added to the list applies; anything unlisted next to it is still rejected.
    let names = unlisted(2);
    let mut fx = Fx::clean();
    fx.set_allow_external(&[&names[0]]);
    fx.append(
        "contracts/device/Cargo.toml",
        &format!(
            "[dependencies]\n{} = \"1\"\n[dev-dependencies]\n{} = \"1\"\n",
            names[0], names[1]
        ),
    );
    let r = fx.run();
    assert_rules(&r, &["dep/contracts-external"]);
    assert_message(&r, &format!("`{}`", names[1]));
    assert_no_message(&r, &format!("`{}`", names[0]));

    // An entry removed from the list no longer applies.
    if let Some(first) = real.first() {
        let rest: Vec<&str> = real.iter().skip(1).map(String::as_str).collect();
        let mut fx = Fx::clean();
        fx.set_allow_external(&rest);
        fx.append(
            "contracts/device/Cargo.toml",
            &format!("[dependencies]\n{first} = \"1\"\n"),
        );
        let r = fx.run();
        assert_rules(&r, &["dep/contracts-external"]);
        assert_message(&r, &format!("`{first}`"));
    }
}

#[test]
fn contracts_external_allowlist_cannot_be_abused() {
    let names = unlisted(1);
    let mut fx = Fx::clean();
    fx.set_allow_external(&[&names[0]]);

    // A listed name from a non-crates.io source (git) is still a violation.
    fx.append(
        "contracts/device/Cargo.toml",
        &format!(
            "[dependencies]\n{} = {{ git = \"https://example.invalid/x\" }}\n",
            names[0]
        ),
    );
    let r = fx.run();
    assert_rules(&r, &["dep/contracts-external"]);
    assert_message(&r, "git+https://example.invalid/x");

    // A listed name used as a rename alias for an unlisted crate is still a violation.
    let mut fx = Fx::clean();
    fx.set_allow_external(&[&names[0]]);
    fx.append(
        "contracts/device/Cargo.toml",
        &format!(
            "[dependencies]\n{} = {{ package = \"droiduse-unlisted-other\", version = \"1\" }}\n",
            names[0]
        ),
    );
    let r = fx.run();
    assert_rules(&r, &["dep/contracts-external"]);
    assert_message(&r, "`droiduse-unlisted-other`");

    // No wildcard (and no malformed names) in the allowlist.
    let mut fx = Fx::clean();
    fx.set_allow_external(&["*", "ser de"]);
    fx.append(
        "contracts/device/Cargo.toml",
        &format!("[dependencies]\n{} = \"1\"\n", names[0]),
    );
    let r = fx.run();
    assert_rules(&r, &["config/invalid", "dep/contracts-external"]);
    assert_message(&r, "LAYERS.toml: config/invalid");

    // The allowlist lives only in LAYERS.toml, not in a LAYER.toml.
    let mut fx = Fx::clean();
    fx.edit(
        "contracts/LAYER.toml",
        "[layer]\n",
        &format!("[layer]\nallow_external = [\"{}\"]\n", names[0]),
    );
    fx.append(
        "contracts/device/Cargo.toml",
        &format!("[dependencies]\n{} = \"1\"\n", names[0]),
    );
    // Fail closed: the invalid file is not applied, so its allow_siblings is lost too.
    assert_rules(
        &fx.run(),
        &[
            "config/invalid",
            "dep/contracts-external",
            "dep/sibling-module",
        ],
    );
}

#[test]
fn product_code_depending_on_clients_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("host/entrypoints/cli", &["clients/sdk"]);
    assert_rules(&fx.run(), &["dep/on-clients"]);
}

#[test]
fn clients_depending_on_host_fails() {
    let mut fx = Fx::clean();
    fx.add_deps("clients/sdk", &["host/base/ipc"]);
    assert_rules(&fx.run(), &["dep/clients-only-contracts"]);
}

#[test]
fn product_code_depending_on_testing_fails() {
    let mut fx = Fx::clean();
    fx.krate("testing/support", &[]);
    fx.add_deps("host/runtime/instance", &["testing/support"]);
    assert_rules(&fx.run(), &["dep/on-testing"]);

    // dev-dependencies count too.
    let mut fx = Fx::clean();
    fx.krate("testing/support", &[]);
    fx.add_deps("host/vmm/boot", &["dev:testing/support"]);
    let r = fx.run();
    assert_rules(&r, &["dep/on-testing"]);
    assert_message(&r, "dev-dependency");

    // Even contracts and clients may not.
    let mut fx = Fx::clean();
    fx.krate("testing/support", &[]);
    fx.add_deps("clients/sdk", &["testing/support"]);
    assert_rules(&fx.run(), &["dep/on-testing"]);
}

#[test]
fn layer_toml_allow_list_narrows() {
    // runtime (L4) -> devices (L2) is allowed by LAYERS.toml levels, but
    // host/runtime/LAYER.toml allow = ["base", "contracts", "vmm"] excludes it.
    let mut fx = Fx::clean();
    fx.add_deps("host/runtime/instance", &["host/devices/gpu"]);
    let r = fx.run();
    assert_rules(&r, &["dep/layer-toml-allow"]);
    assert_message(&r, "host/runtime/LAYER.toml");

    // base allow = ["third_party"]: even contracts is excluded.
    let mut fx = Fx::clean();
    fx.add_deps("host/base/ipc", &["contracts/common"]);
    assert_rules(&fx.run(), &["dep/layer-toml-allow"]);
}

#[test]
fn nested_layer_toml_narrows_further() {
    let mut fx = Fx::clean();
    fx.file(
        "host/services/inspect/LAYER.toml",
        "[layer]\nname = \"services\"\nallow = [\"contracts\"]\n",
    );
    let r = fx.run();
    assert_rules(&r, &["dep/layer-toml-allow"]);
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
    assert_rules(&r, &["config/widens"]);

    // And it does not let the dependency through.
    fx.add_deps("host/base/ipc", &["host/runtime/snapshot"]);
    assert_rules(&fx.run(), &["dep/upward", "config/widens"]);
}

#[test]
fn malformed_layer_toml_fails() {
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/LAYER.toml",
        "[layer]\nname = \"vmm\"\nalow = [\"base\"]\n",
    );
    assert_rules(&fx.run(), &["config/invalid"]);

    let mut fx = Fx::clean();
    fx.file("host/vmm/LAYER.toml", "[layer]\nname = \"devices\"\n");
    assert_rules(&fx.run(), &["config/invalid"]);

    // An unknown allow entry is reported; the list still narrows (vmm's real deps now fail).
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/LAYER.toml",
        "[layer]\nname = \"vmm\"\nallow = [\"nonexistent\"]\n",
    );
    assert_rules(&fx.run(), &["dep/layer-toml-allow", "config/invalid"]);
}

#[test]
fn testing_may_depend_on_everything() {
    // Covered by clean_repo_passes (testing/e2e -> entrypoints, clients, vmm); make it explicit.
    let mut fx = Fx::clean();
    fx.add_deps(
        "testing/e2e",
        &["host/devices/gpu", "contracts/common", "host/base/ipc"],
    );
    assert_clean(&fx.run());
}

#[test]
fn dependency_on_tooling_or_unlayered_path_fails() {
    let mut fx = Fx::clean();
    fx.krate("build/helper", &[]);
    fx.add_deps("host/vmm/boot", &["build/helper"]);
    assert_rules(&fx.run(), &["dep/unlayered-target"]);
}

#[test]
fn crate_outside_any_layer_fails() {
    let mut fx = Fx::clean();
    fx.krate("misc/thing", &[]);
    assert_rules(&fx.run(), &["crate/unlayered"]);
}

#[test]
fn crate_not_in_workspace_fails() {
    let mut fx = Fx::clean();
    fx.krate_unlisted("host/vmm/disk");
    let r = fx.run();
    assert_rules(&r, &["crate/not-in-workspace"]);
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
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(
        &r,
        "host/vmm/boot/src/lib.rs:1: concept/forbidden: \"android\" in identifier",
    );

    // Multi-word concept, camel case.
    let mut fx = Fx::clean();
    fx.file("host/devices/gpu/src/lib.rs", "pub struct PackageName;\n");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "\"package_name\"");

    // "*" concepts apply to every layer; case-insensitive.
    let mut fx = Fx::clean();
    fx.file(
        "host/runtime/instance/src/lib.rs",
        "pub struct QEMURunner;\n",
    );
    assert_rules(&fx.run(), &["concept/forbidden"]);

    let mut fx = Fx::clean();
    fx.file(
        "host/services/screen/src/lib.rs",
        "pub fn f(tenant_id: u32) {}\n",
    );
    assert_rules(&fx.run(), &["concept/forbidden"]);
}

#[test]
fn layer_specific_concepts_do_not_leak_to_other_layers() {
    // "android" is forbidden in vmm/devices only.
    let mut fx = Fx::clean();
    fx.file(
        "host/runtime/instance/src/lib.rs",
        "pub fn android_boot_config() {}\npub const K: &str = \"androidboot.hardware\";\n",
    );
    assert_clean(&fx.run());
}

#[test]
fn forbidden_concept_in_string_literal_fails() {
    let mut fx = Fx::clean();
    fx.file(
        "host/devices/gpu/src/lib.rs",
        "pub const S: &str = \"Android guest\";\n",
    );
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "string literal");

    // `//` inside a (raw) string is not a comment.
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub const U: &str = r#\"see http://x/aosp\"#;\n",
    );
    assert_rules(&fx.run(), &["concept/forbidden"]);

    // Non-Rust files are checked too (TOML string in Cargo.toml metadata).
    let mut fx = Fx::clean();
    fx.krate_with("host/vmm/api", &[], "description = \"VM API for Android\"");
    assert_rules(&fx.run(), &["concept/forbidden"]);
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
    assert_clean(&fx.run());
}

#[test]
fn forbidden_concept_in_file_or_dir_name_fails() {
    let mut fx = Fx::clean();
    fx.file("host/vmm/boot/src/aosp.rs", "");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "file name `aosp.rs`");

    let mut fx = Fx::clean();
    fx.file("host/devices/gpu/src/activity/mod.rs", "");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "directory name `activity`");

    // Prefix match in a file name.
    let mut fx = Fx::clean();
    fx.file("host/vmm/boot/src/androidboot.rs", "");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "file name `androidboot.rs`");
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
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "dependency crate name `android_logger`");

    // Prefix match in a crate name.
    let mut fx = Fx::clean();
    fx.append(
        "host/devices/gpu/Cargo.toml",
        "[build-dependencies]\nandroidboot-sys = \"1\"\n",
    );
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "build-dependency crate name `androidboot-sys`");
}

#[test]
fn concept_matching_is_word_prefix() {
    // A word that starts with a forbidden word is flagged.
    for (file, src) in [
        ("host/vmm/boot/src/lib.rs", "pub fn androidboot_x() {}\n"),
        ("host/services/screen/src/lib.rs", "pub fn tenants() {}\n"),
        ("contracts/common/src/lib.rs", "pub struct Qemud;\n"),
        ("host/vmm/boot/src/lib.rs", "pub fn activitymanager() {}\n"),
        (
            "clients/sdk/src/lib.rs",
            "pub const B: &str = \"billings\";\n",
        ),
    ] {
        let mut fx = Fx::clean();
        fx.file(file, src);
        let r = fx.run();
        assert_rules(&r, &["concept/forbidden"]);
        assert_message(&r, &format!("{file}:1: concept/forbidden"));
    }

    // Only a prefix: the forbidden word inside or at the end of a word is not flagged.
    let mut fx = Fx::clean();
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub fn subactivity() {}\npub fn multitenant() {}\n",
    );
    assert_clean(&fx.run());

    // Separate words still match exactly.
    let mut fx = Fx::clean();
    fx.file("host/vmm/boot/src/lib.rs", "pub fn sub_activity() {}\n");
    assert_rules(&fx.run(), &["concept/forbidden"]);
}

#[test]
fn multi_word_concept_prefix_on_last_word() {
    // package_name: `package` exact, then a word starting with `name`.
    let mut fx = Fx::clean();
    fx.file("host/devices/gpu/src/lib.rs", "pub fn package_names() {}\n");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "\"package_name\"");

    // The first word is not a prefix match; the words must be contiguous.
    let mut fx = Fx::clean();
    fx.file(
        "host/devices/gpu/src/lib.rs",
        "pub fn packages_name() {}\npub fn package_or_name() {}\npub fn name_package() {}\n",
    );
    assert_clean(&fx.run());
}

#[test]
fn concept_exception_applies() {
    let mut fx = Fx::clean();
    fx.edit(
        "LAYERS.toml",
        "[forbidden_exceptions]\n",
        "[forbidden_exceptions]\n\"android\" = [\"androidboot\"]\n\"package_name\" = [\"namespace\"]\n",
    );
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub const K: &str = \"androidboot.hardware\";\npub fn package_namespace() {}\n",
    );
    assert_clean(&fx.run());

    // Exact words only: a longer word with the excepted word as prefix is still flagged,
    // and so is the concept itself.
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub fn androidbootx() {}\npub fn android() {}\npub fn package_namespaces() {}\n",
    );
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "host/vmm/boot/src/lib.rs:1:");
    assert_message(&r, "host/vmm/boot/src/lib.rs:2:");
    assert_message(&r, "host/vmm/boot/src/lib.rs:3:");
}

#[test]
fn concept_exception_is_scoped_to_its_concept() {
    // Two concepts share the prefix; an exception for `pixel` does not exempt from `pix`.
    let mut fx = Fx::clean();
    fx.file(
        "host/services/inspect/LAYER.toml",
        "[layer]\nforbid_concepts = [\"pix\", \"pixel\"]\n",
    );
    fx.edit(
        "LAYERS.toml",
        "[forbidden_exceptions]\n",
        "[forbidden_exceptions]\n\"pixel\" = [\"pixelate\"]\n",
    );
    fx.file("host/services/inspect/src/lib.rs", "pub fn pixelate() {}\n");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_eq!(r.violations.len(), 1, "{}", dump(&r));
    assert_message(&r, "\"pix\" in identifier `pixelate`");
}

#[test]
fn concept_exception_cannot_be_abused() {
    // Excepting the concept word itself would disable the concept.
    let mut fx = Fx::clean();
    fx.edit(
        "LAYERS.toml",
        "[forbidden_exceptions]\n",
        "[forbidden_exceptions]\n\"qemu\" = [\"qemu\", \"QEMU\"]\n",
    );
    fx.file("host/base/ipc/src/lib.rs", "pub fn qemu() {}\n");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden", "config/widens"]);
    assert_message(&r, "LAYERS.toml: config/widens");

    // Multi-word, separated or wildcard entries are not single words; words that do not
    // extend the concept's last word are meaningless; neither is applied.
    let mut fx = Fx::clean();
    fx.edit(
        "LAYERS.toml",
        "[forbidden_exceptions]\n",
        "[forbidden_exceptions]\n\"android\" = [\"android_boot\", \"AndroidBoot\", \"*\", \"androidboot.x\", \"boot\"]\n",
    );
    fx.file(
        "host/vmm/boot/src/lib.rs",
        "pub fn android_boot() {}\npub fn AndroidBoot() {}\n",
    );
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden", "config/invalid"]);
    for e in [
        "`android_boot`",
        "`AndroidBoot`",
        "`*`",
        "`androidboot.x`",
        "`boot`",
    ] {
        assert_message(&r, e);
    }

    // The key must be a concept that is actually forbidden somewhere.
    let mut fx = Fx::clean();
    fx.edit(
        "LAYERS.toml",
        "[forbidden_exceptions]\n",
        "[forbidden_exceptions]\n\"androi\" = [\"androidboot\"]\n",
    );
    fx.file("host/vmm/boot/src/lib.rs", "pub fn androidboot() {}\n");
    assert_rules(&fx.run(), &["concept/forbidden", "config/invalid"]);

    // Exceptions live only in LAYERS.toml, not in a LAYER.toml.
    let mut fx = Fx::clean();
    fx.edit(
        "host/vmm/LAYER.toml",
        "[layer]\n",
        "[layer]\nforbidden_exceptions = [\"androidboot\"]\n",
    );
    fx.file("host/vmm/boot/src/lib.rs", "pub fn androidboot() {}\n");
    assert_rules(&fx.run(), &["concept/forbidden", "config/invalid"]);
}

#[test]
fn layer_toml_forbid_concepts_apply() {
    // clients/LAYER.toml forbid_concepts = ["tenant", "billing"].
    let mut fx = Fx::clean();
    fx.file("clients/sdk/src/lib.rs", "pub fn billing() {}\n");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
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
    assert_rules(&r, &["concept/forbidden"]);
    assert_eq!(r.violations.len(), 1, "{}", dump(&r));
    assert_message(&r, "host/services/inspect/src/lib.rs:1");
}

// ---------------------------------------------------------------- patch files

const PATCH: &str = "\
From 0123456789abcdef Mon Sep 17 00:00:00 2001
Subject: [PATCH] droiduse: hook point next to the qemu compat code

Hook-for: host/vmm (not qemu)
---
 qemu/compat.c | 2 ++
 1 file changed, 2 insertions(+)

diff --git a/qemu/compat.c b/qemu/compat.c
index 1111111..2222222 100644
--- a/qemu/compat.c
+++ b/qemu/compat.c
@@ -10,4 +10,5 @@ static int qemu_probe(void)
 int qemu_compat = 1;
-int old_qemu_flag;
+int droiduse_hook = 0;
+++counter;
 return qemu_compat;
 }
@@ -40 +42 @@
-x
+y
\\ No newline at end of file
--
2.40.0
";

#[test]
fn patch_files_check_only_added_lines() {
    // `qemu` in headers, the commit message, context and removed lines: not checked.
    let mut fx = Fx::clean();
    fx.file("third_party/wick/patches/0001-droiduse-hook.patch", PATCH);
    fx.file("third_party/aosp/patches/0001-droiduse-hook.diff", PATCH);
    assert_clean(&fx.run());

    // `qemu` on an added line: checked, reported at the patch file's line.
    let mut fx = Fx::clean();
    fx.file(
        "third_party/wick/patches/0001-droiduse-hook.patch",
        &PATCH.replace("+int droiduse_hook = 0;", "+int droiduse_qemu_hook = 0;"),
    );
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_eq!(r.violations.len(), 1, "{}", dump(&r));
    assert_message(
        &r,
        "third_party/wick/patches/0001-droiduse-hook.patch:16: concept/forbidden: \"qemu\" in added line",
    );

    // An added line whose content starts with `++` is an added line, not a `+++` header.
    let mut fx = Fx::clean();
    fx.file(
        "third_party/wick/patches/0001-droiduse-hook.diff",
        &PATCH.replace("+++counter;", "+++qemu_counter;"),
    );
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "0001-droiduse-hook.diff:17:");

    // Fail closed: a `+` line outside any recognized hunk is checked too.
    let mut fx = Fx::clean();
    fx.file(
        "third_party/wick/patches/0001-droiduse-hook.patch",
        "garbage\n+qemu here\n+++ b/qemu.c\n",
    );
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_eq!(r.violations.len(), 1, "{}", dump(&r));
    assert_message(&r, "0001-droiduse-hook.patch:2:");
}

#[test]
fn patch_file_names_are_still_checked() {
    let mut fx = Fx::clean();
    fx.file("third_party/wick/patches/0002-qemu-compat.patch", "");
    let r = fx.run();
    assert_rules(&r, &["concept/forbidden"]);
    assert_message(&r, "file name `0002-qemu-compat.patch`");
}
