//! Who owns a path, from the repo's `OWNERS` files.
//!
//! Format (root `OWNERS`): `<path-glob> <owner> [<owner>...]`, `#` comments. A glob is
//! relative to the directory of its `OWNERS` file; `*` does not cross `/`. A rule applies to
//! a path when its glob matches the path or one of its ancestor directories; within one
//! file the last applying rule wins. "An OWNERS file in a subdirectory takes precedence":
//! the nearest `OWNERS` file (from the path's directory up to the repo root) with an
//! applying rule decides.
//!
//! Only GitHub user logins (`login` or `@login`) can name an approver. Team entries
//! (`org/team`) cannot be resolved to people offline, so they validate no one, and anything
//! else (e.g. the `<owner>` placeholder) is never an owner.

use std::fs;
use std::path::Path;

use globset::GlobBuilder;

/// A GitHub user login, normalized (no `@`, lowercase), or `None` if `token` is not one.
pub fn user_login(token: &str) -> Option<String> {
    let t = token.strip_prefix('@').unwrap_or(token);
    let ok = (1..=39).contains(&t.len())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !t.starts_with('-')
        && !t.ends_with('-')
        && !t.contains("--");
    ok.then(|| t.to_ascii_lowercase())
}

/// Rules of one `OWNERS` file: (glob, owner tokens). `Err` names the malformed line.
fn parse(text: &str) -> Result<Vec<(globset::GlobMatcher, Vec<String>)>, String> {
    let mut rules = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("");
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens.as_slice() {
            [] => {}
            [_] => return Err(format!("line {}: a glob without owners", i + 1)),
            [glob, owners @ ..] => {
                let g = GlobBuilder::new(glob)
                    .literal_separator(true)
                    .build()
                    .map_err(|e| format!("line {}: {e}", i + 1))?;
                rules.push((
                    g.compile_matcher(),
                    owners.iter().map(|s| s.to_string()).collect(),
                ));
            }
        }
    }
    Ok(rules)
}

/// The user logins owning `rel` (repo-relative, `/`-separated). `Ok(None)`: no rule applies.
/// `Err`: an `OWNERS` file on the way is unreadable or malformed.
pub fn owners_of(root: &Path, rel: &str) -> Result<Option<Vec<String>>, String> {
    let parts: Vec<&str> = rel.split('/').collect();
    // Directories from the path's own directory up to the root.
    for depth in (0..parts.len()).rev() {
        let dir = parts[..depth].join("/");
        let file = if dir.is_empty() {
            root.join("OWNERS")
        } else {
            root.join(&dir).join("OWNERS")
        };
        let text = match fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}/OWNERS: {e}", dir)),
        };
        let rules = parse(&text).map_err(|e| {
            let name = if dir.is_empty() {
                "OWNERS".to_string()
            } else {
                format!("{dir}/OWNERS")
            };
            format!("{name}: {e}")
        })?;
        // The path below this directory and its ancestors.
        let below = &parts[depth..];
        let candidates: Vec<String> = (1..=below.len()).map(|n| below[..n].join("/")).collect();
        let hit = rules
            .iter()
            .rev()
            .find(|(g, _)| candidates.iter().any(|c| g.is_match(c)));
        if let Some((_, owners)) = hit {
            return Ok(Some(owners.iter().filter_map(|o| user_login(o)).collect()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins() {
        assert_eq!(user_login("@Alice-1").as_deref(), Some("alice-1"));
        assert_eq!(user_login("bob").as_deref(), Some("bob"));
        for bad in [
            "<owner>",
            "org/team",
            "@org/team",
            "-x",
            "x-",
            "a--b",
            "",
            "@",
            "a b",
        ] {
            assert_eq!(user_login(bad), None, "{bad}");
        }
    }
}
