//! Word splitting and matching for the forbidden-concept lint ("Case-insensitive, word-prefix match").
//!
//! A chunk of text (identifier, string literal, file name, crate name) is split into
//! lowercase words at:
//!  - any non-alphanumeric character (`_`, `-`, `.`, space, ...)
//!  - a lower-to-upper case change (`packageName` -> package, name)
//!  - the end of an upper-case run followed by a lower-case letter (`AOSPBuild` -> aosp, build)
//!  - a letter/digit change (`aosp15` -> aosp, 15)
//!
//! A concept matches when its words appear as a contiguous run of the chunk's words, where
//! every word but the last must be equal and the last chunk word need only *start with* the
//! concept's last word. So `android` matches `androidboot`, `tenant` matches `tenants`, and
//! `package_name` matches `PackageName` and `package_names`, while `activity` does not match
//! `subactivity` (prefix only). Exact words listed as exceptions for the concept
//! (`LAYERS.toml [forbidden_exceptions]`) are not matched by the prefix rule; the concept's
//! own word always matches.

use std::collections::BTreeSet;

pub fn split_words(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut words = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if !cur.is_empty() {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied();
            let boundary = (prev.is_lowercase() && c.is_uppercase())
                || (prev.is_alphabetic() != c.is_alphabetic())
                || (prev.is_uppercase()
                    && c.is_uppercase()
                    && next.is_some_and(|n| n.is_lowercase()));
            if boundary {
                words.push(std::mem::take(&mut cur));
            }
        }
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// True if `needle` occurs in `hay` as a contiguous run whose last word is a prefix match
/// (see the module docs). A hay word in `exceptions` does not prefix-match; a hay word equal
/// to the needle's last word always matches.
pub fn contains_prefix_run(
    hay: &[String],
    needle: &[String],
    exceptions: &BTreeSet<String>,
) -> bool {
    let Some((last, head)) = needle.split_last() else {
        return false;
    };
    hay.windows(needle.len()).any(|w| {
        let (w_last, w_head) = w.split_last().expect("window is non-empty");
        w_head == head
            && w_last.starts_with(last.as_str())
            && (w_last == last || !exceptions.contains(w_last))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        split_words(s)
    }

    fn m(hay: &str, needle: &str) -> bool {
        contains_prefix_run(&w(hay), &w(needle), &BTreeSet::new())
    }

    #[test]
    fn splits_case_and_separators() {
        assert_eq!(w("packageName"), ["package", "name"]);
        assert_eq!(w("PACKAGE_NAME"), ["package", "name"]);
        assert_eq!(w("AOSPBuild"), ["aosp", "build"]);
        assert_eq!(w("HTTPServer2x"), ["http", "server", "2", "x"]);
        assert_eq!(w("aosp15-boot.rs"), ["aosp", "15", "boot", "rs"]);
        assert_eq!(w("androidboot"), ["androidboot"]);
        assert!(w("__--").is_empty());
    }

    #[test]
    fn prefix_runs() {
        assert!(m("get_package_name", "package_name"));
        assert!(m("package_names", "package_name"));
        assert!(!m("packages_name", "package_name"));
        assert!(!m("package_or_name", "package_name"));
        assert!(m("Qemu", "qemu"));
        assert!(m("qemud", "qemu"));
        assert!(m("androidboot.hardware", "android"));
        assert!(m("tenants", "tenant"));
        assert!(!m("subactivity", "activity"));
        assert!(!m("andro", "android"));
        assert!(!m("anything", ""));
    }

    #[test]
    fn exceptions_are_exact_words() {
        let ex: BTreeSet<String> = ["androidboot".to_string()].into();
        let c = w("android");
        assert!(!contains_prefix_run(&w("androidboot"), &c, &ex));
        assert!(contains_prefix_run(&w("androidbootx"), &c, &ex));
        assert!(contains_prefix_run(&w("android"), &c, &ex));
        // An exception on one word does not hide another match in the same chunk.
        assert!(contains_prefix_run(&w("androidboot_android"), &c, &ex));
        // An exception equal to the concept word never disables it.
        let ex: BTreeSet<String> = ["android".to_string()].into();
        assert!(contains_prefix_run(&w("android"), &c, &ex));
    }
}
