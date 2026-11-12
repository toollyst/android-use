//! Fixture tests: each patch rule and each `third_party/patches.toml` setting, for its default
//! and at least one non-default value. Fixtures use the repo's real `third_party/patches.toml`
//! and `docs/scope.md`; tests that need another policy edit the fixture copy. Assertions
//! compare the exact rule ids reported.

use std::fs;
use std::path::{Path, PathBuf};

use patchlint::Report;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A wick patch that follows docs/08-upstream-and-patches.md §2 and the default policy:
/// `git format-patch` output, the header template, a `droiduse:` marker on every hunk,
/// 3 added lines (Budget 4).
const GOOD: &str = "\
From 1234567890abcdef1234567890abcdef12345678 Mon Sep 17 00:00:00 2001
From: Droid Dev <dev@example.invalid>
Date: Thu, 12 Nov 2026 10:00:00 +0900
Subject: [PATCH] droiduse: add boot payload hook

Purpose:      let host/vmm/boot supply kernel, initrd and cmdline
Hook-for:     host/vmm/boot
Layer:        vmm
Budget:       4
Upstream:     n/a
Remove-when:  upstream offers an equivalent API
Scope-cell:   Android 부팅 (boot.img/vendor_boot/bootconfig)
---
 src/vmm/src/builder.rs | 3 +++
 1 file changed, 3 insertions(+)

diff --git a/src/vmm/src/builder.rs b/src/vmm/src/builder.rs
index 1111111..2222222 100644
--- a/src/vmm/src/builder.rs
+++ b/src/vmm/src/builder.rs
@@ -10,3 +10,5 @@ fn build_microvm() {
     let vm = Vm::new();
+    // droiduse: boot payload hook
+    ext::boot_payload(&vm);
     vm.start();
 }
@@ -40,2 +42,3 @@ fn teardown() {
     drop(vm);
+    ext::teardown(); // droiduse: boot payload hook
 }
--\x20
2.47.0
";

const FIELDS: &[&str] = &[
    "Purpose:",
    "Hook-for:",
    "Layer:",
    "Budget:",
    "Upstream:",
    "Remove-when:",
    "Scope-cell:",
];

const WICK_NAME: &str = "0001-droiduse-boot-payload-hook.patch";
const WICK: &str = "third_party/wick/patches/0001-droiduse-boot-payload-hook.patch";
const AOSP: &str =
    "third_party/aosp/patches/frameworks/base/0001-droiduse-viewroot-inspector-hook.patch";
const CFG: &str = "third_party/patches.toml";

struct Fx {
    dir: tempfile::TempDir,
}

impl Fx {
    /// A repo with the real policy file, `docs/scope.md` and OWNERS, the patch directory
    /// skeleton (series + README.md) and the `Hook-for` modules the fixtures name.
    fn new() -> Fx {
        let fx = Fx {
            dir: tempfile::tempdir().unwrap(),
        };
        let real = repo_root();
        for f in [CFG, "docs/scope.md", "OWNERS"] {
            fx.file(f, &fs::read_to_string(real.join(f)).unwrap());
        }
        for x in ["wick", "aosp", "kernel"] {
            fx.file(
                &format!("third_party/{x}/patches/README.md"),
                &readme_with(""),
            );
        }
        fx.file("third_party/wick/patches/series", "# order\n");
        fx.file(
            "third_party/aosp/patches/series",
            "# <project-path> <patch-file>\n",
        );
        fx.file("third_party/aosp/security/README.md", "# ASB\n");
        fx.file("host/vmm/boot/README.md", "");
        fx.file("guest/vendor/services/inspector/README.md", "");
        fx
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn file(&self, rel: &str, content: &str) -> &Fx {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
        self
    }

    /// Replaces `from` (which must occur) with `to` in a fixture file.
    fn edit(&self, rel: &str, from: &str, to: &str) -> &Fx {
        let old = fs::read_to_string(self.path(rel)).unwrap();
        assert!(old.contains(from), "`{from}` not in {rel}");
        self.file(rel, &old.replacen(from, to, 1))
    }

    /// Edits the policy file.
    fn cfg(&self, from: &str, to: &str) -> &Fx {
        self.edit(CFG, from, to)
    }

    /// Writes the wick patch `text` and lists it in the wick series.
    fn wick(&self, text: &str) -> &Fx {
        self.file(WICK, text);
        self.file(
            "third_party/wick/patches/series",
            &format!("# order\n{WICK_NAME}\n"),
        )
    }

    /// Regenerates the README tables (so tests of other rules see them up to date), then lints.
    fn run(&self) -> Report {
        patchlint::write_readmes(self.dir.path()).expect("README tables must generate");
        self.run_raw()
    }

    /// Lints without touching the READMEs.
    fn run_raw(&self) -> Report {
        patchlint::check(self.dir.path()).expect("lint must run")
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap()
    }

    fn error(&self) -> String {
        match patchlint::check(self.dir.path()) {
            Ok(r) => panic!("expected a config error, got:\n{}", dump(&r)),
            Err(e) => e.to_string(),
        }
    }
}

/// Lints `text` as the single wick patch under the real policy.
fn lint(text: &str) -> Report {
    let fx = Fx::new();
    fx.wick(text);
    fx.run()
}

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

/// `text` with the line starting with `field` replaced by `field` + `value`
/// (or removed when `value` is `None`).
fn set_field(text: &str, field: &str, value: Option<&str>) -> String {
    let mut found = false;
    let out: Vec<String> = text
        .lines()
        .filter_map(|l| {
            if l.starts_with(field) {
                found = true;
                value.map(|v| format!("{field} {v}"))
            } else {
                Some(l.to_string())
            }
        })
        .collect();
    assert!(found, "{field} not in the patch");
    out.join("\n") + "\n"
}

fn with_field(field: &str, value: Option<&str>) -> String {
    set_field(GOOD, field, value)
}

/// `GOOD` with an extra commit-message line after `Scope-cell:`.
fn with_extra(line: &str) -> String {
    GOOD.replace(
        "Scope-cell:   Android 부팅 (boot.img/vendor_boot/bootconfig)\n",
        &format!("Scope-cell:   Android 부팅 (boot.img/vendor_boot/bootconfig)\n{line}\n"),
    )
}

/// `GOOD` whose second hunk only removes a line.
fn removal_only_hunk() -> String {
    let r = GOOD.replace(
        "@@ -40,2 +42,3 @@ fn teardown() {\n     drop(vm);\n+    ext::teardown(); // droiduse: boot payload hook\n }\n",
        "@@ -40,3 +42,2 @@ fn teardown() {\n     drop(vm);\n-    cleanup();\n }\n",
    );
    assert_ne!(r, GOOD);
    r
}

/// `GOOD` plus a second file that the patch deletes.
fn file_deletion() -> String {
    let deletion = "\
diff --git a/src/vmm/src/old.rs b/src/vmm/src/old.rs
deleted file mode 100644
index 3333333..0000000
--- a/src/vmm/src/old.rs
+++ /dev/null
@@ -1 +0,0 @@
-fn old() {}
--\x20
";
    let r = GOOD.replace("--\x20\n", deletion);
    assert_ne!(r, GOOD);
    r
}

// ---------------------------------------------------------------- clean

#[test]
fn real_repo_passes_with_the_real_policy() {
    let r = patchlint::check(&repo_root()).expect("lint must run");
    assert_clean(&r);
}

#[test]
fn repo_without_patches_passes() {
    let r = Fx::new().run();
    assert_clean(&r);
    assert_eq!(r.patches_checked, 0);
}

#[test]
fn good_patches_pass() {
    let fx = Fx::new();
    fx.wick(GOOD);
    let aosp = set_field(
        &set_field(GOOD, "Layer:", Some("       guest")),
        "Hook-for:",
        Some("    guest/vendor/services/inspector"),
    );
    fx.file(AOSP, &aosp);
    fx.file(
        "third_party/aosp/patches/series",
        "# <project-path> <patch-file>\nframeworks/base   frameworks/base/0001-droiduse-viewroot-inspector-hook.patch\n",
    );
    let r = fx.run();
    assert_clean(&r);
    assert_eq!(r.patches_checked, 2);
}

#[test]
fn pending_upstream_link_passes() {
    assert_clean(&lint(&with_field(
        "Upstream:",
        Some("    pending https://github.com/example/wick/pull/12"),
    )));
}

#[test]
fn encoded_subject_passes() {
    // `git format-patch` writes a non-ASCII subject as an RFC 2047 encoded word.
    assert_clean(&lint(&GOOD.replace(
        "Subject: [PATCH] droiduse: add boot payload hook",
        "Subject: [PATCH] =?UTF-8?q?droiduse:=20=ED=9B=85=20=EC=B6=94=EA=B0=80?=",
    )));
}

#[test]
fn hand_written_patch_without_mbox_line_passes() {
    assert_clean(&lint(
        &(GOOD.lines().skip(1).collect::<Vec<_>>().join("\n") + "\n"),
    ));
}

// ---------------------------------------------------------------- header template

#[test]
fn missing_header_field_fails() {
    for f in FIELDS.iter().chain(&["From:", "Subject:"]) {
        let r = lint(&with_field(f, None));
        assert_rules(&r, &["patch/header-missing"]);
        assert_message(&r, &format!("`{f}`"));
    }
}

#[test]
fn field_after_the_message_does_not_count() {
    let text = with_field("Layer:", None).replace("---\n", "---\nLayer:        vmm\n");
    let r = lint(&text);
    assert_rules(&r, &["patch/header-missing"]);
    assert_message(&r, "`Layer:`");
}

#[test]
fn malformed_header_field_fails() {
    let cases: &[(&str, &str)] = &[
        ("Subject:", "[PATCH] add boot payload hook"),
        ("Subject:", "[PATCH 1/2] droiduse: add boot payload hook"),
        ("Subject:", "[PATCH v2] droiduse: add boot payload hook"),
        ("Subject:", "[PATCH] droiduse: <한 줄 요약>"),
        ("Subject:", "[PATCH] droiduse:"),
        ("From:", ""),
        ("Purpose:", ""),
        ("Purpose:", "<이 패치가 존재하는 이유>"),
        ("Hook-for:", "/abs/path"),
        ("Hook-for:", "host/../host/vmm/boot"),
        ("Hook-for:", "host\\vmm\\boot"),
        ("Hook-for:", "host//vmm/boot"),
        ("Layer:", "host"),
        ("Layer:", "guest | vmm"),
        ("Budget:", "0"),
        ("Budget:", "-3"),
        ("Budget:", "+3"),
        ("Budget:", "forty"),
        ("Budget:", "4 lines"),
        ("Upstream:", "pending"),
        ("Upstream:", "pending <링크>"),
        ("Upstream:", "pending http://example.invalid/pr/1"),
        ("Upstream:", "pending https://"),
        ("Upstream:", "merged https://example.invalid/pr/1"),
        ("Upstream:", "N/A"),
        ("Remove-when:", ""),
        ("Scope-cell:", "<docs/scope.md의 해당 행>"),
    ];
    for (field, value) in cases {
        let r = lint(&with_field(field, Some(value)));
        assert_rules(&r, &["patch/header-invalid"]);
        assert_message(&r, &format!("`{field}`"));
    }
}

#[test]
fn duplicate_header_field_fails() {
    let text = GOOD.replace(
        "Layer:        vmm\n",
        "Layer:        vmm\nLayer:        vmm\n",
    );
    let r = lint(&text);
    assert_rules(&r, &["patch/header-invalid"]);
    assert_message(&r, "`Layer:` appears 2 times");
}

// ---------------------------------------------------------------- [header]

#[test]
fn numbered_subject_setting() {
    let numbered = [
        "[PATCH 1/2] droiduse: add boot payload hook",
        "[PATCH v2] droiduse: add boot payload hook",
        "[PATCH v3 2/5] droiduse: add boot payload hook",
    ];
    // Default (false): only `[PATCH] droiduse: `.
    for s in numbered {
        assert_rules(
            &lint(&with_field("Subject:", Some(s))),
            &["patch/header-invalid"],
        );
    }
    // true: numbered and versioned prefixes pass; malformed ones still fail.
    for s in numbered {
        let fx = Fx::new();
        fx.cfg("numbered_subject = false", "numbered_subject = true");
        fx.wick(&with_field("Subject:", Some(s)));
        assert_clean(&fx.run());
    }
    for s in [
        "[PATCH x/y] droiduse: add",
        "[PATCH 0/2] droiduse: add",
        "[PATCH 3/2] droiduse: add",
        "[PATCH v] droiduse: add",
        "[PATCH 1/2] add",
    ] {
        let fx = Fx::new();
        fx.cfg("numbered_subject = false", "numbered_subject = true");
        fx.wick(&with_field("Subject:", Some(s)));
        assert_rules(&fx.run(), &["patch/header-invalid"]);
    }
}

#[test]
fn from_mailbox_setting() {
    // Default (true): a valid single mailbox.
    for v in [
        "Droid Dev",
        "Dev <not-an-address>",
        "a@x.invalid, b@x.invalid",
        "<author>",
    ] {
        let r = lint(&with_field("From:", Some(v)));
        assert_rules(&r, &["patch/header-invalid"]);
        assert_message(&r, "`From:`");
    }
    assert_clean(&lint(&with_field("From:", Some("dev@example.invalid"))));
    // false: any non-empty author.
    let fx = Fx::new();
    fx.cfg("from_mailbox = true", "from_mailbox = false");
    fx.wick(&with_field("From:", Some("Droid Dev")));
    assert_clean(&fx.run());
}

// ---------------------------------------------------------------- markers, [marker]

#[test]
fn hunk_without_marker_fails() {
    let text = GOOD.replace(
        "+    ext::teardown(); // droiduse: boot payload hook",
        "+    ext::teardown();",
    );
    let r = lint(&text);
    assert_rules(&r, &["patch/marker"]);
    assert_eq!(r.violations.len(), 1, "{}", dump(&r));
    assert_message(&r, "hunk 2 (@@ -40,2 +42,3 @@)");
    assert_message(&r, "b/src/vmm/src/builder.rs");
}

#[test]
fn marker_must_be_on_an_added_line() {
    let text = GOOD
        .replace(
            "+    ext::teardown(); // droiduse: boot payload hook",
            "+    ext::teardown();",
        )
        .replace("     drop(vm);", "     drop(vm); // droiduse: old");
    assert_rules(&lint(&text), &["patch/marker"]);
}

#[test]
fn removal_only_setting() {
    // Default ("fail"): removal-only hunks and file deletions cannot carry a marker.
    assert_rules(&lint(&removal_only_hunk()), &["patch/marker"]);
    assert_rules(&lint(&file_deletion()), &["patch/marker"]);
    // "allow": they need none; a hunk that adds lines still does.
    for text in [removal_only_hunk(), file_deletion()] {
        let fx = Fx::new();
        fx.cfg("removal_only = \"fail\"", "removal_only = \"allow\"");
        fx.wick(&text);
        assert_clean(&fx.run());
    }
    let fx = Fx::new();
    fx.cfg("removal_only = \"fail\"", "removal_only = \"allow\"");
    fx.wick(&GOOD.replace(
        "+    ext::teardown(); // droiduse: boot payload hook",
        "+    ext::teardown();",
    ));
    assert_rules(&fx.run(), &["patch/marker"]);
}

// ---------------------------------------------------------------- [budget]

#[test]
fn changed_lines_above_budget_fail() {
    let r = lint(&with_field("Budget:", Some("      2")));
    assert_rules(&r, &["patch/budget"]);
    assert_message(&r, "3 changed lines (added+removed) exceed `Budget:` 2");
    assert_clean(&lint(&with_field("Budget:", Some("      3"))));
}

#[test]
fn every_file_of_a_patch_counts() {
    let second = "\
diff --git a/src/vmm/src/lib.rs b/src/vmm/src/lib.rs
index 3333333..4444444 100644
--- a/src/vmm/src/lib.rs
+++ b/src/vmm/src/lib.rs
@@ -1,1 +1,3 @@
 mod builder;
+// droiduse: ext module
+mod ext;
--\x20
";
    let text = GOOD.replace("--\x20\n", second);
    assert_ne!(text, GOOD);
    let r = lint(&text);
    assert_rules(&r, &["patch/budget"]);
    assert_message(&r, "5 changed lines");
}

/// 3 added + 4 removed lines (hunk 2 removes four), `Budget:` given.
fn removal_heavy(budget: &str) -> String {
    let text = GOOD.replace(
        "@@ -40,2 +42,3 @@ fn teardown() {\n     drop(vm);\n+    ext::teardown(); // droiduse: boot payload hook\n }\n",
        "@@ -40,6 +42,3 @@ fn teardown() {\n     drop(vm);\n-    a();\n-    b();\n-    c();\n-    d();\n+    ext::teardown(); // droiduse: boot payload hook\n }\n",
    );
    assert_ne!(text, GOOD);
    set_field(&text, "Budget:", Some(budget))
}

#[test]
fn count_setting() {
    // Default "added+removed": 3 + 4 = 7 > 6.
    let r = lint(&removal_heavy("      6"));
    assert_rules(&r, &["patch/budget"]);
    assert_message(&r, "7 changed lines (added+removed)");
    assert_clean(&lint(&removal_heavy("      7")));

    // "added": 3 <= 3.
    let fx = Fx::new();
    fx.cfg("count = \"added+removed\"", "count = \"added\"");
    fx.wick(&removal_heavy("      3"));
    assert_clean(&fx.run());

    // "max": max(3, 4) = 4 > 3, 4 <= 4.
    let fx = Fx::new();
    fx.cfg("count = \"added+removed\"", "count = \"max\"");
    fx.wick(&removal_heavy("      3"));
    let r = fx.run();
    assert_rules(&r, &["patch/budget"]);
    assert_message(&r, "4 changed lines (max(added, removed))");
    fx.wick(&removal_heavy("      4"));
    assert_clean(&fx.run());
}

#[test]
fn cap_setting() {
    // Default cap 30: `Budget: 30` is fine, 31 needs an approved overrun.
    assert_clean(&lint(&with_field("Budget:", Some("      30"))));
    let r = lint(&with_field("Budget:", Some("      31")));
    assert_rules(&r, &["patch/budget-overrun"]);
    assert_message(&r, "`Budget:` 31 is above the cap 30");

    // A lower global cap.
    let fx = Fx::new();
    fx.cfg("cap = 30", "cap = 3");
    fx.wick(GOOD);
    assert_rules(&fx.run(), &["patch/budget-overrun"]);

    // A per-upstream cap overrides the global one.
    let fx = Fx::new();
    fx.cfg(
        "[upstream.wick]\nlayer = \"vmm\"",
        "[upstream.wick]\nlayer = \"vmm\"\ncap = 40",
    );
    fx.wick(&with_field("Budget:", Some("      40")));
    assert_clean(&fx.run());
    fx.wick(&with_field("Budget:", Some("      41")));
    let r = fx.run();
    assert_rules(&r, &["patch/budget-overrun"]);
    assert_message(&r, "the cap 40");
}

fn overrun_approved_by(who: &str) -> String {
    set_field(
        &with_extra(&format!("Budget-approved-by: {who}")),
        "Budget:",
        Some("      31"),
    )
}

#[test]
fn overrun_owners_setting() {
    // Default "owners": the approver must own the patch in OWNERS.
    let fx = Fx::new();
    fx.file(
        "OWNERS",
        "*                       @root-owner\nthird_party/**/patches  @alice  @org/patch-team   # patches\n",
    );
    fx.wick(&overrun_approved_by("alice"));
    assert_clean(&fx.run());
    fx.wick(&overrun_approved_by("@Alice"));
    assert_clean(&fx.run());

    // Not an owner of this path, a team name, or no approver at all.
    for who in ["root-owner", "bob", "org"] {
        fx.wick(&overrun_approved_by(who));
        let r = fx.run();
        assert_rules(&r, &["patch/budget-overrun"]);
        assert_message(&r, &format!("`{who}`"));
    }
    fx.wick(&with_field("Budget:", Some("      31")));
    assert_rules(&fx.run(), &["patch/budget-overrun"]);

    // The nearest OWNERS file with an applying rule wins.
    fx.file("third_party/wick/OWNERS", "patches @carol\n");
    fx.wick(&overrun_approved_by("alice"));
    assert_rules(&fx.run(), &["patch/budget-overrun"]);
    fx.wick(&overrun_approved_by("carol"));
    assert_clean(&fx.run());
}

#[test]
fn owners_placeholder_never_validates() {
    // An OWNERS that still lists `<owner>` placeholders approves no one.
    let fx = Fx::new();
    fx.file(
        "OWNERS",
        "*                       <owner>\nthird_party/**/patches  <owner>\n",
    );
    for who in ["owner", "<owner>"] {
        fx.wick(&overrun_approved_by(who));
        let r = fx.run();
        assert!(
            ids(&r).contains(&"patch/budget-overrun"),
            "{who}: {}",
            dump(&r)
        );
    }
    // A malformed approver is also a header error, even without an overrun.
    let r = lint(&with_extra("Budget-approved-by: <owner>"));
    assert_rules(&r, &["patch/header-invalid"]);
    assert_message(&r, "`Budget-approved-by:`");
}

#[test]
fn real_owners_names_a_login_that_approves_patches() {
    // The real OWNERS (copied by Fx::new) parses, and its patch owner can approve an overrun.
    let text = fs::read_to_string(repo_root().join("OWNERS")).unwrap();
    let owner = text
        .lines()
        .find(|l| l.starts_with("third_party/**/patches"))
        .and_then(|l| l.split_whitespace().nth(1))
        .expect("OWNERS: a third_party/**/patches rule");
    assert!(patchlint::owners::user_login(owner).is_some(), "{owner}");
    let fx = Fx::new();
    fx.wick(&overrun_approved_by(owner));
    assert_clean(&fx.run());
    // and someone else cannot
    fx.wick(&overrun_approved_by("not-an-owner"));
    assert_rules(&fx.run(), &["patch/budget-overrun"]);
}

#[test]
fn malformed_owners_fails_closed() {
    let fx = Fx::new();
    fx.file("OWNERS", "third_party/**/patches\n");
    fx.wick(&overrun_approved_by("alice"));
    let r = fx.run();
    assert_rules(&r, &["patch/budget-overrun"]);
    assert_message(&r, "OWNERS");
}

#[test]
fn overrun_forbidden_setting() {
    let fx = Fx::new();
    fx.file("OWNERS", "third_party/**/patches  @alice\n");
    fx.cfg("overrun = \"owners\"", "overrun = \"forbidden\"");
    fx.wick(&overrun_approved_by("alice"));
    let r = fx.run();
    assert_rules(&r, &["patch/budget-overrun"]);
    assert_message(&r, "forbidden");
}

// ---------------------------------------------------------------- [rules]

#[test]
fn file_name_rule() {
    for bad in [
        "0001-boot-payload-hook.patch",
        "001-droiduse-boot.patch",
        "0001-droiduse-Boot.patch",
        "0001-droiduse-boot.diff",
        "0001-droiduse-.patch",
        "0001-droiduse-a--b.patch",
        "0001-droiduse-boot-.patch",
        "notes.txt",
    ] {
        let fx = Fx::new();
        fx.file(&format!("third_party/wick/patches/{bad}"), GOOD);
        fx.file("third_party/wick/patches/series", &format!("{bad}\n"));
        let r = fx.run();
        assert_rules(&r, &["patch/name"]);
    }
    // Off: any name.
    let fx = Fx::new();
    fx.cfg("file_name = true", "file_name = false");
    fx.file("third_party/wick/patches/boot.diff", GOOD);
    fx.file("third_party/wick/patches/series", "boot.diff\n");
    assert_clean(&fx.run());
}

#[test]
fn series_rule() {
    // Unlisted patch.
    let fx = Fx::new();
    fx.file(WICK, GOOD);
    let r = fx.run();
    assert_rules(&r, &["patch/series"]);
    assert_message(&r, &format!("`{WICK_NAME}` is not listed"));

    // Listed twice, and an entry without a file.
    fx.file(
        "third_party/wick/patches/series",
        &format!("{WICK_NAME}\n{WICK_NAME}\n0002-droiduse-gone.patch\n"),
    );
    let r = fx.run();
    assert_rules(&r, &["patch/series"]);
    assert_message(&r, "listed more than once");
    assert_message(&r, "`0002-droiduse-gone.patch` is not a patch file");

    // Unsafe entries.
    fx.file("third_party/wick/patches/series", "../../../etc/passwd\n");
    assert_rules(&fx.run(), &["patch/series"]);

    // Missing series file while patches exist.
    let fx = Fx::new();
    fx.file(WICK, GOOD);
    fs::remove_file(fx.path("third_party/wick/patches/series")).unwrap();
    assert_rules(&fx.run(), &["patch/series"]);

    // aosp form: `<project-path> <patch-file>`, the file under `<project-path>/`.
    let aosp = set_field(
        &set_field(GOOD, "Layer:", Some("       guest")),
        "Hook-for:",
        Some("    guest/vendor/services/inspector"),
    );
    for series in [
        "frameworks/base/0001-droiduse-viewroot-inspector-hook.patch\n",
        "frameworks/native frameworks/base/0001-droiduse-viewroot-inspector-hook.patch\n",
        "frameworks/base frameworks/base/0001-droiduse-viewroot-inspector-hook.patch extra\n",
    ] {
        let fx = Fx::new();
        fx.file(AOSP, &aosp);
        fx.file("third_party/aosp/patches/series", series);
        assert_rules(&fx.run(), &["patch/series"]);
    }

    // The series format is per upstream config.
    let fx = Fx::new();
    fx.cfg(
        "layer = \"guest\"\nseries = \"project-file\"",
        "layer = \"guest\"\nseries = \"file\"",
    );
    fx.file(AOSP, &aosp);
    fx.file(
        "third_party/aosp/patches/series",
        "frameworks/base/0001-droiduse-viewroot-inspector-hook.patch\n",
    );
    assert_clean(&fx.run());

    // Off: nothing about series is checked.
    let fx = Fx::new();
    fx.cfg("series = true", "series = false");
    fx.file(WICK, GOOD);
    assert_clean(&fx.run());
}

#[test]
fn layer_matches_upstream_rule() {
    let guest = with_field("Layer:", Some("       guest"));
    let r = lint(&guest);
    assert_rules(&r, &["patch/layer"]);
    assert_message(
        &r,
        "`Layer: guest` but third_party/wick patches must declare `vmm`",
    );

    // The mapping is config.
    let fx = Fx::new();
    fx.cfg(
        "[upstream.wick]\nlayer = \"vmm\"",
        "[upstream.wick]\nlayer = \"guest\"",
    );
    fx.wick(&guest);
    assert_clean(&fx.run());

    // Off.
    let fx = Fx::new();
    fx.cfg(
        "layer_matches_upstream = true",
        "layer_matches_upstream = false",
    );
    fx.wick(&guest);
    assert_clean(&fx.run());
}

#[test]
fn hook_for_exists_rule() {
    let missing = with_field("Hook-for:", Some("    host/vmm/nothing-here"));
    let r = lint(&missing);
    assert_rules(&r, &["patch/hook-for"]);
    assert_message(&r, "`host/vmm/nothing-here`");

    let fx = Fx::new();
    fx.cfg("hook_for_exists = true", "hook_for_exists = false");
    fx.wick(&missing);
    assert_clean(&fx.run());
}

#[test]
fn scope_cell_exists_rule() {
    for v in ["Nope", "기능", "M1", "Android 부팅"] {
        let r = lint(&with_field("Scope-cell:", Some(v)));
        assert_rules(&r, &["patch/scope-cell"]);
    }
    // Another real row passes.
    assert_clean(&lint(&with_field("Scope-cell:", Some("  View 트리"))));

    let fx = Fx::new();
    fx.cfg("scope_cell_exists = true", "scope_cell_exists = false");
    fx.wick(&with_field("Scope-cell:", Some("Nope")));
    assert_clean(&fx.run());
}

// ---------------------------------------------------------------- structure

#[test]
fn unparsable_patch_fails() {
    let r = lint(&GOOD[..GOOD.find("---\n").unwrap()]);
    assert_rules(&r, &["patch/parse"]);
    let r = lint(&GOOD.replace("@@ -10,3 +10,5 @@", "@@ -10,3 +10,9 @@"));
    assert_rules(&r, &["patch/parse"]);
}

#[test]
fn every_file_in_a_patch_dir_is_a_patch() {
    // Only `series` and `README.md` at the top of patches/ are not patches.
    let fx = Fx::new();
    fx.cfg("file_name = true", "file_name = false");
    fx.cfg("series = true", "series = false");
    fx.file("third_party/wick/patches/notes.txt", "free text\n");
    fx.file(
        "third_party/aosp/patches/frameworks/base/README.md",
        "# x\n",
    );
    let r = fx.run();
    assert_rules(&r, &["patch/parse", "patch/header-missing"]);
    assert_message(&r, "third_party/wick/patches/notes.txt");
    assert_message(&r, "third_party/aosp/patches/frameworks/base/README.md");
    assert_eq!(r.patches_checked, 2);
}

#[test]
fn security_backports_are_not_our_patches() {
    let fx = Fx::new();
    fx.file(
        "third_party/aosp/security/2026-11/0001-asb.patch",
        "not ours\n",
    );
    assert_clean(&fx.run());
}

#[cfg(unix)]
#[test]
fn symlink_in_patch_dir_fails() {
    let fx = Fx::new();
    fx.file("elsewhere/x.patch", GOOD);
    std::os::unix::fs::symlink(fx.path("elsewhere/x.patch"), fx.path(WICK)).unwrap();
    fx.file("third_party/wick/patches/series", &format!("{WICK_NAME}\n"));
    assert!(ids(&fx.run()).contains(&"patch/file"));
}

#[test]
fn several_rules_are_reported_together() {
    let text = with_field("Layer:", Some("host"))
        .replace(
            "+    ext::teardown(); // droiduse: boot payload hook",
            "+    ext::teardown();",
        )
        .replace("Budget:       4", "Budget:       1");
    assert_rules(
        &lint(&text),
        &["patch/budget", "patch/header-invalid", "patch/marker"],
    );
}

// ---------------------------------------------------------------- config errors (exit 2)

#[test]
fn missing_inputs_cannot_run() {
    let dir = tempfile::tempdir().unwrap();
    assert!(patchlint::check(dir.path()).is_err());

    let fx = Fx::new();
    fs::remove_file(fx.path(CFG)).unwrap();
    assert!(fx.error().contains("third_party/patches.toml"));

    let fx = Fx::new();
    fs::remove_file(fx.path("docs/scope.md")).unwrap();
    assert!(fx.error().contains("docs/scope.md"));

    // scope.md is not needed when the rule is off.
    fs::remove_file(fx.path(CFG)).unwrap();
    fx.file(CFG, "[rules]\nscope_cell_exists = false\n");
    assert_clean(&fx.run());
}

#[test]
fn omitted_keys_take_the_defaults() {
    let fx = Fx::new();
    fx.file(CFG, "");
    fx.wick(GOOD);
    assert_clean(&fx.run());
    fx.wick(&with_field("Budget:", Some("      31")));
    assert_rules(&fx.run(), &["patch/budget-overrun"]);
}

#[test]
fn config_errors_fail_closed() {
    let cases: &[(&str, &str, &str)] = &[
        ("cap = 30", "cap = 30\ncapp = 1", "capp"),
        ("cap = 30", "cap = 0", "cap"),
        ("cap = 30", "cap = -1", "cap"),
        ("cap = 30", "cap = \"30\"", "cap"),
        ("count = \"added+removed\"", "count = \"lines\"", "lines"),
        ("overrun = \"owners\"", "overrun = \"maybe\"", "maybe"),
        (
            "numbered_subject = false",
            "numbered_subject = \"no\"",
            "numbered_subject",
        ),
        ("removal_only = \"fail\"", "removal_only = \"skip\"", "skip"),
        ("file_name = true", "file_name = 1", "file_name"),
        ("file_name = true", "file_nam = true", "file_nam"),
        (
            "[upstream.wick]\nlayer = \"vmm\"",
            "[upstream.wick]\nlayer = \"host\"",
            "host",
        ),
        (
            "[upstream.wick]\nlayer = \"vmm\"",
            "[upstream.wick]\nlayer = \"vmm\"\ncap = 0",
            "cap",
        ),
        ("series = \"file\"", "series = \"lines\"", "lines"),
        ("[upstream.kernel]", "[upstream.gfxstream]", "gfxstream"),
        ("[header]", "[headers]", "headers"),
    ];
    for (from, to, needle) in cases {
        let fx = Fx::new();
        fx.cfg(from, to);
        let e = fx.error();
        assert!(
            e.contains(needle),
            "`{to}`: error `{e}` does not mention `{needle}`"
        );
    }

    // A patch directory without an [upstream.<x>] entry.
    let fx = Fx::new();
    fx.file("third_party/rutabaga/patches/README.md", "");
    assert!(fx.error().contains("[upstream.rutabaga]"));
}

// ---------------------------------------------------------------- README table

/// A README with hand-written text around a generated block whose body is `body`.
fn readme_with(body: &str) -> String {
    format!(
        "# patches\n\nHand-written intro.\n\n{}\n{}{}\n\n## Plan\nhand-written plan\n",
        patchlint::readme::BEGIN,
        if body.is_empty() {
            "_패치 없음._\n"
        } else {
            body
        },
        patchlint::readme::END
    )
}

const WICK_README: &str = "third_party/wick/patches/README.md";

#[test]
fn readme_table_is_generated_from_headers() {
    let fx = Fx::new();
    fx.wick(GOOD);
    // Not regenerated yet: out of date.
    let r = fx.run_raw();
    assert_rules(&r, &["patch/readme"]);
    assert_message(&r, "third_party/wick/patches/README.md: patch/readme");
    assert_message(&r, "cargo xtask patch-readme");

    // Generated: up to date, the rest of the README untouched.
    let changed = patchlint::write_readmes(fx.dir.path()).unwrap();
    assert_eq!(changed, vec![WICK_README.to_string()]);
    assert_clean(&fx.run_raw());
    let text = fx.read(WICK_README);
    let want = readme_with(&format!(
        "| Patch | Summary | Purpose | Hook-for | Layer | Budget | Upstream | Remove-when | Scope-cell |\n\
         |---|---|---|---|---|---|---|---|---|\n\
         | [{WICK_NAME}]({WICK_NAME}) | add boot payload hook | let host/vmm/boot supply kernel, initrd and cmdline | host/vmm/boot | vmm | 4 | n/a | upstream offers an equivalent API | Android 부팅 (boot.img/vendor_boot/bootconfig) |\n"
    ));
    assert_eq!(text, want);
    // Idempotent and byte-stable.
    assert!(patchlint::write_readmes(fx.dir.path()).unwrap().is_empty());
    assert_eq!(fx.read(WICK_README), want);

    // A header change makes it stale again.
    fx.wick(&with_field("Purpose:", Some("     something else")));
    assert_rules(&fx.run_raw(), &["patch/readme"]);
}

#[test]
fn readme_hand_edit_inside_the_block_fails() {
    let fx = Fx::new();
    fx.wick(GOOD);
    patchlint::write_readmes(fx.dir.path()).unwrap();
    fx.edit(WICK_README, "| vmm | 4 |", "| vmm | 40 |");
    assert_rules(&fx.run_raw(), &["patch/readme"]);
    // Text outside the block is free.
    let fx = Fx::new();
    fx.edit(WICK_README, "hand-written plan", "a different plan");
    assert_clean(&fx.run_raw());
}

#[test]
fn readme_markers_are_required() {
    for text in [
        "# patches\nno markers\n".to_string(),
        readme_with("") + &readme_with(""),
        format!("{}\n{}\n", patchlint::readme::END, patchlint::readme::BEGIN),
    ] {
        let fx = Fx::new();
        fx.file(WICK_README, &text);
        assert_rules(&fx.run_raw(), &["patch/readme"]);
        // The writer does not guess where the table goes.
        assert!(patchlint::write_readmes(fx.dir.path()).is_err());
        assert_eq!(fx.read(WICK_README), text);
    }
    let fx = Fx::new();
    fs::remove_file(fx.path(WICK_README)).unwrap();
    assert_rules(&fx.run_raw(), &["patch/readme"]);
}

#[test]
fn readme_rows_follow_series_order() {
    let fx = Fx::new();
    let second = "0002-droiduse-second-hook.patch";
    fx.file(WICK, GOOD);
    fx.file(
        &format!("third_party/wick/patches/{second}"),
        &GOOD.replace("add boot payload hook", "second hook"),
    );
    fx.file(
        "third_party/wick/patches/series",
        &format!("{second}\n{WICK_NAME}\n"),
    );
    assert_clean(&fx.run());
    let text = fx.read(WICK_README);
    let a = text.find(second).unwrap();
    let b = text.find(WICK_NAME).unwrap();
    assert!(a < b, "series order not kept:\n{text}");
}

#[test]
fn readme_with_crlf_line_endings_is_up_to_date() {
    let fx = Fx::new();
    fx.wick(GOOD);
    patchlint::write_readmes(fx.dir.path()).unwrap();
    let crlf = fx.read(WICK_README).replace('\n', "\r\n");
    fx.file(WICK_README, &crlf);
    assert_clean(&fx.run_raw());
}

#[test]
fn readme_columns_setting() {
    let fx = Fx::new();
    fx.cfg(
        "columns = [\"patch\", \"summary\", \"purpose\", \"hook-for\", \"layer\", \"budget\", \"upstream\", \"remove-when\", \"scope-cell\"]",
        "columns = [\"patch\", \"purpose\"]",
    );
    fx.wick(GOOD);
    assert_clean(&fx.run());
    let text = fx.read(WICK_README);
    assert!(
        text.contains(&format!(
            "| Patch | Purpose |\n|---|---|\n| [{WICK_NAME}]({WICK_NAME}) | let host/vmm/boot supply kernel, initrd and cmdline |\n"
        )),
        "{text}"
    );

    for (to, needle) in [
        ("columns = [\"purpose\"]", "patch"),
        ("columns = [\"patch\", \"patch\"]", "twice"),
        ("columns = [\"patch\", \"author\"]", "author"),
    ] {
        let fx = Fx::new();
        fx.cfg(
            "columns = [\"patch\", \"summary\", \"purpose\", \"hook-for\", \"layer\", \"budget\", \"upstream\", \"remove-when\", \"scope-cell\"]",
            to,
        );
        let e = fx.error();
        assert!(e.contains(needle), "`{to}`: {e}");
    }
}

#[test]
fn readme_rule_setting() {
    let fx = Fx::new();
    fx.cfg("readme = true", "readme = false");
    fx.wick(GOOD);
    fx.file(WICK_README, "# no markers at all\n");
    assert_clean(&fx.run_raw());
}

#[test]
fn readme_cells_escape_pipes() {
    let fx = Fx::new();
    fx.wick(&with_field("Purpose:", Some("     a | b")));
    assert_clean(&fx.run());
    assert!(fx.read(WICK_README).contains("| a \\| b |"));
}
