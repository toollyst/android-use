//! Fixture tests: each patch rule fails when violated and passes when the patch is correct.
//! Assertions compare the exact rule ids reported.

use std::fs;
use std::path::{Path, PathBuf};

use patchlint::Report;

/// A patch that follows docs/08-upstream-and-patches.md §2: `git format-patch` output with
/// the header template, a `droiduse:` marker on every hunk and 3 changed lines (Budget 4).
const GOOD: &str = "\
From 1234567890abcdef1234567890abcdef12345678 Mon Sep 17 00:00:00 2001
From: Droid Dev <dev@example.invalid>
Date: Thu, 12 Nov 2026 10:00:00 +0900
Subject: [PATCH] droiduse: add traversal hook

Purpose:      let the inspector observe traversals
Hook-for:     guest/vendor/services/inspector
Layer:        guest
Budget:       4
Upstream:     n/a
Remove-when:  upstream offers an equivalent API
Scope-cell:   M2 inspect tree
---
 core/java/android/view/ViewRootImpl.java | 3 +++
 1 file changed, 3 insertions(+)

diff --git a/core/java/android/view/ViewRootImpl.java b/core/java/android/view/ViewRootImpl.java
index 1111111..2222222 100644
--- a/core/java/android/view/ViewRootImpl.java
+++ b/core/java/android/view/ViewRootImpl.java
@@ -10,3 +10,5 @@ class ViewRootImpl {
     void performTraversals() {
+        // droiduse: inspector hook
+        DroidInspector.onTraversal(this);
         measure();
     }
@@ -40,2 +42,3 @@ class ViewRootImpl {
     void die() {
+        DroidInspector.onDie(this); // droiduse: inspector hook
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

struct Fx {
    dir: tempfile::TempDir,
}

impl Fx {
    /// A repo with the real patch directory skeleton (series + README.md, no patches).
    fn new() -> Fx {
        let fx = Fx {
            dir: tempfile::tempdir().unwrap(),
        };
        for x in ["wick", "aosp", "kernel"] {
            fx.file(&format!("third_party/{x}/patches/README.md"), "# patches\n");
        }
        fx.file("third_party/wick/patches/series", "# order\n");
        fx.file(
            "third_party/aosp/patches/series",
            "# <project-path> <patch-file>\n",
        );
        fx.file("third_party/aosp/security/README.md", "# ASB\n");
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

    fn run(&self) -> Report {
        patchlint::check(self.dir.path()).expect("lint must run")
    }
}

const WICK: &str = "third_party/wick/patches/0001-droiduse-ext-hook.patch";
const AOSP: &str =
    "third_party/aosp/patches/frameworks/base/0001-droiduse-viewroot-inspector-hook.patch";

/// Lints `text` as a single wick patch.
fn lint(text: &str) -> Report {
    let fx = Fx::new();
    fx.file(WICK, text);
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
fn assert_message(r: &Report, needle: &str) {
    assert!(
        r.violations.iter().any(|v| v.to_string().contains(needle)),
        "no violation mentions `{needle}`; violations:\n{}",
        dump(r)
    );
}

/// `GOOD` with the line starting with `field` replaced by `field` + `value`
/// (or removed when `value` is `None`).
fn with_field(field: &str, value: Option<&str>) -> String {
    let mut found = false;
    let out: Vec<String> = GOOD
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
    assert!(found, "{field} not in GOOD");
    out.join("\n") + "\n"
}

// ---------------------------------------------------------------- clean

#[test]
fn repo_without_patches_passes() {
    let r = Fx::new().run();
    assert!(r.is_clean(), "{}", dump(&r));
    assert_eq!(r.patches_checked, 0);
}

#[test]
fn good_patch_passes() {
    let fx = Fx::new();
    fx.file(WICK, GOOD);
    fx.file(
        AOSP,
        &GOOD.replace("Layer:        guest", "Layer:        vmm"),
    );
    let r = fx.run();
    assert!(r.is_clean(), "{}", dump(&r));
    assert_eq!(r.patches_checked, 2);
}

#[test]
fn pending_upstream_link_passes() {
    let r = lint(&with_field(
        "Upstream:",
        Some("    pending https://github.com/example/wick/pull/12"),
    ));
    assert!(r.is_clean(), "{}", dump(&r));
}

#[test]
fn encoded_subject_passes() {
    // `git format-patch` writes a non-ASCII subject as an RFC 2047 encoded word.
    let text = GOOD.replace(
        "Subject: [PATCH] droiduse: add traversal hook",
        "Subject: [PATCH] =?UTF-8?q?droiduse:=20=ED=9B=85=20=EC=B6=94=EA=B0=80?=",
    );
    let r = lint(&text);
    assert!(r.is_clean(), "{}", dump(&r));
}

#[test]
fn hand_written_patch_without_mbox_line_passes() {
    let text = GOOD.lines().skip(1).collect::<Vec<_>>().join("\n") + "\n";
    let r = lint(&text);
    assert!(r.is_clean(), "{}", dump(&r));
}

// ---------------------------------------------------------------- header

#[test]
fn missing_header_field_fails() {
    for f in FIELDS {
        let r = lint(&with_field(f, None));
        assert_rules(&r, &["patch/header-missing"]);
        assert_message(&r, &format!("`{f}`"));
    }
    for f in ["From:", "Subject:"] {
        let r = lint(&with_field(f, None));
        assert_rules(&r, &["patch/header-missing"]);
        assert_message(&r, &format!("`{f}`"));
    }
}

#[test]
fn field_after_the_message_does_not_count() {
    // A field below the `---` separator is not in the header.
    let text = with_field("Layer:", None).replace("---\n", "---\nLayer:        guest\n");
    let r = lint(&text);
    assert_rules(&r, &["patch/header-missing"]);
    assert_message(&r, "`Layer:`");
}

#[test]
fn malformed_header_field_fails() {
    let cases: &[(&str, &str)] = &[
        ("Subject:", "[PATCH] add traversal hook"),
        ("Subject:", "[PATCH 1/2] droiduse: add traversal hook"),
        ("Subject:", "[PATCH] droiduse: <한 줄 요약>"),
        ("Subject:", "[PATCH] droiduse:"),
        ("From:", ""),
        ("Purpose:", ""),
        ("Purpose:", "<이 패치가 존재하는 이유>"),
        ("Hook-for:", "/abs/path"),
        ("Hook-for:", "guest/../host/vmm"),
        ("Hook-for:", "guest\\vendor"),
        ("Hook-for:", "guest//vendor"),
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
        "Layer:        guest\n",
        "Layer:        guest\nLayer:        vmm\n",
    );
    let r = lint(&text);
    assert_rules(&r, &["patch/header-invalid"]);
    assert_message(&r, "`Layer:` appears 2 times");
}

// ---------------------------------------------------------------- markers

#[test]
fn hunk_without_marker_fails() {
    let text = GOOD.replace(
        "+        DroidInspector.onDie(this); // droiduse: inspector hook",
        "+        DroidInspector.onDie(this);",
    );
    let r = lint(&text);
    assert_rules(&r, &["patch/marker"]);
    assert_eq!(r.violations.len(), 1, "{}", dump(&r));
    assert_message(&r, "hunk 2 (@@ -40,2 +42,3 @@)");
    assert_message(&r, "core/java/android/view/ViewRootImpl.java");
}

#[test]
fn marker_must_be_on_an_added_line() {
    // The marker only on a context line (already upstream / from another patch) does not count.
    let text = GOOD
        .replace(
            "+        DroidInspector.onDie(this); // droiduse: inspector hook",
            "+        DroidInspector.onDie(this);",
        )
        .replace("     void die() {", "     void die() { // droiduse: old");
    let r = lint(&text);
    assert_rules(&r, &["patch/marker"]);
}

#[test]
fn removal_only_hunk_fails() {
    // A hunk that only removes lines cannot carry a marker.
    let text = GOOD.replace(
        "@@ -40,2 +42,3 @@ class ViewRootImpl {\n     void die() {\n+        DroidInspector.onDie(this); // droiduse: inspector hook\n     }\n",
        "@@ -40,3 +42,2 @@ class ViewRootImpl {\n     void die() {\n-        cleanup();\n     }\n",
    );
    let r = lint(&text);
    assert_rules(&r, &["patch/marker"]);
}

// ---------------------------------------------------------------- budget

#[test]
fn budget_exceeded_fails() {
    // 3 added lines, Budget 2.
    let r = lint(&with_field("Budget:", Some("      2")));
    assert_rules(&r, &["patch/budget"]);
    assert_message(&r, "3 changed lines exceed `Budget:` 2");

    // Exactly at the budget passes.
    let r = lint(&with_field("Budget:", Some("      3")));
    assert!(r.is_clean(), "{}", dump(&r));
}

#[test]
fn removed_lines_count_toward_the_budget() {
    // 3 added + 1 removed = 4 > 3.
    let text = with_field("Budget:", Some("      3")).replace(
        "     void die() {\n+        DroidInspector",
        "     void die() {\n-        cleanup();\n+        DroidInspector",
    );
    let text = text.replace("@@ -40,2 +42,3 @@", "@@ -40,3 +42,3 @@");
    let r = lint(&text);
    assert_rules(&r, &["patch/budget"]);
    assert_message(&r, "4 changed lines");
}

#[test]
fn every_file_of_a_patch_counts() {
    let second = "\
diff --git a/core/java/android/app/Activity.java b/core/java/android/app/Activity.java
index 3333333..4444444 100644
--- a/core/java/android/app/Activity.java
+++ b/core/java/android/app/Activity.java
@@ -1,1 +1,3 @@
 class Activity {
+    // droiduse: activity hook
+    void hook() {}
--\x20
";
    let text = GOOD.replace("--\x20\n", second);
    assert_ne!(text, GOOD);
    // 3 + 2 = 5 > 4.
    let r = lint(&text);
    assert_rules(&r, &["patch/budget"]);
    assert_message(&r, "5 changed lines");
}

// ---------------------------------------------------------------- structure

#[test]
fn unparsable_patch_fails() {
    // No diff at all.
    let r = lint(&GOOD[..GOOD.find("---\n").unwrap()]);
    assert_rules(&r, &["patch/parse"]);

    // Hunk counts that do not match the lines.
    let r = lint(&GOOD.replace("@@ -10,3 +10,5 @@", "@@ -10,3 +10,9 @@"));
    assert_rules(&r, &["patch/parse"]);
}

#[test]
fn every_file_in_a_patch_dir_is_a_patch() {
    // Only `series` and `README.md` at the top of patches/ are not patches; anything else
    // (another extension, a nested README) is linted, so nothing escapes by its name.
    let fx = Fx::new();
    fx.file("third_party/wick/patches/notes.txt", "free text\n");
    fx.file(
        "third_party/aosp/patches/frameworks/base/README.md",
        "# x\n",
    );
    fx.file("third_party/wick/patches/0002-droiduse-x.diff", GOOD);
    let r = fx.run();
    assert_rules(&r, &["patch/parse", "patch/header-missing"]);
    assert_message(&r, "third_party/wick/patches/notes.txt");
    assert_message(&r, "third_party/aosp/patches/frameworks/base/README.md");
    assert_eq!(r.patches_checked, 3);
}

#[test]
fn security_backports_are_not_our_patches() {
    // third_party/aosp/security/ holds ASB backports, not hook patches.
    let fx = Fx::new();
    fx.file(
        "third_party/aosp/security/2026-11/0001-asb.patch",
        "not ours\n",
    );
    assert!(fx.run().is_clean());
}

#[cfg(unix)]
#[test]
fn symlink_in_patch_dir_fails() {
    let fx = Fx::new();
    fx.file("elsewhere/0001.patch", GOOD);
    std::os::unix::fs::symlink(
        fx.path("elsewhere/0001.patch"),
        fx.path("third_party/wick/patches/0001-droiduse-link.patch"),
    )
    .unwrap();
    let r = fx.run();
    assert_rules(&r, &["patch/file"]);
}

#[test]
fn missing_third_party_cannot_run() {
    let dir = tempfile::tempdir().unwrap();
    assert!(patchlint::check(dir.path()).is_err());
    assert!(patchlint::check(&Path::new(dir.path()).join("nope")).is_err());
}

#[test]
fn several_rules_are_reported_together() {
    let text = with_field("Layer:", Some("host"))
        .replace(
            "+        DroidInspector.onDie(this); // droiduse: inspector hook",
            "+        DroidInspector.onDie(this);",
        )
        .replace("Budget:       4", "Budget:       1");
    let r = lint(&text);
    assert_rules(
        &r,
        &["patch/budget", "patch/header-invalid", "patch/marker"],
    );
}
