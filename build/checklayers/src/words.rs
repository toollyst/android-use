//! Word splitting for the forbidden-concept lint ("대소문자 무시, 단어 단위").
//!
//! A chunk of text (identifier, string literal, file name, crate name) is split into
//! lowercase words at:
//!  - any non-alphanumeric character (`_`, `-`, `.`, space, ...)
//!  - a lower-to-upper case change (`packageName` -> package, name)
//!  - the end of an upper-case run followed by a lower-case letter (`AOSPBuild` -> aosp, build)
//!  - a letter/digit change (`aosp15` -> aosp, 15)
//!
//! A concept matches when its own words appear as a contiguous run of the chunk's words.
//! So `package_name` matches `PackageName`, `PACKAGE_NAME` and `"package name"`, while
//! `android` does not match `androidboot` (one word).

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

/// True if `needle` occurs as a contiguous run inside `hay`.
pub fn contains_run(hay: &[String], needle: &[String]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        split_words(s)
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
    fn runs() {
        assert!(contains_run(&w("get_package_name"), &w("package_name")));
        assert!(!contains_run(&w("package_or_name"), &w("package_name")));
        assert!(contains_run(&w("Qemu"), &w("qemu")));
        assert!(!contains_run(&w("subactivity"), &w("activity")));
    }
}
