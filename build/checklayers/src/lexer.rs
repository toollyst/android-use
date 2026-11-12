//! Minimal comment-aware lexers for the forbidden-concept lint.
//!
//! The lint checks identifiers and string literals but not comments
//! (`LAYERS.toml [forbidden]`). The lexers here only need to tell those three apart:
//! they are not full language lexers. Where a lexer is unsure, it errs towards
//! treating text as code (checked), never towards treating code as a comment.
//!
//! Patch files (`*.patch`, `*.diff`) are a special case: only the lines the patch adds
//! are checked (see [`lex_patch`]).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    Identifier,
    StringLiteral,
    /// Text in a file whose syntax is unknown or has no identifiers (checked as a whole).
    Text,
    /// A line a patch adds, without its leading `+` (checked as a whole).
    AddedLine,
}

impl ChunkKind {
    pub fn describe(self) -> &'static str {
        match self {
            ChunkKind::Identifier => "identifier",
            ChunkKind::StringLiteral => "string literal",
            ChunkKind::Text => "text",
            ChunkKind::AddedLine => "added line",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub kind: ChunkKind,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    /// `//`, nested `/* */`, raw strings, char literals vs lifetimes.
    Rust,
    /// `//`, `/* */`, `"`/`'`/`` ` `` strings, `"""` text blocks.
    CLike,
    /// `#` line comments, `"`/`'` strings, `"""`/`'''` strings.
    Hash,
    /// `<!-- -->` comments; the rest is text.
    Xml,
    /// No comment syntax known: every line is checked.
    Plain,
    /// Unified diff: only added lines are checked.
    Patch,
}

/// Picks the syntax from the file name. Unknown files are `Plain` (fully checked).
pub fn syntax_for(file_name: &str) -> Syntax {
    let lower = file_name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "rs" => return Syntax::Rust,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "inl" | "java" | "kt" | "kts"
        | "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" | "cts" | "go" | "proto" | "aidl"
        | "hal" | "bp" | "swift" | "scala" | "gradle" | "groovy" | "dart" | "cs" | "m" | "mm"
        | "css" | "scss" | "less" | "json" | "json5" | "jsonc" | "wgsl" | "glsl" | "vert"
        | "frag" | "comp" | "ld" | "dts" | "dtsi" => return Syntax::CLike,
        "py" | "pyi" | "toml" | "sh" | "bash" | "zsh" | "yaml" | "yml" | "cfg" | "conf" | "te"
        | "rc" | "mk" | "cmake" | "properties" | "bzl" | "star" | "nix" | "r" => {
            return Syntax::Hash;
        }
        "xml" | "html" | "htm" | "svg" | "xsd" | "xsl" => return Syntax::Xml,
        "patch" | "diff" => return Syntax::Patch,
        _ => {}
    }
    match lower.as_str() {
        "makefile" | "gnumakefile" | "dockerfile" | "containerfile" | "build" | "workspace"
        | ".gitignore" | ".gitattributes" | ".dockerignore" | "cargo.lock" | "series"
        | "file_contexts" | "property_contexts" | "service_contexts" | "seapp_contexts"
        | "hwservice_contexts" | "genfs_contexts" | "fs_config" => Syntax::Hash,
        _ => Syntax::Plain,
    }
}

pub fn lex(src: &str, syntax: Syntax) -> Vec<Chunk> {
    match syntax {
        Syntax::Plain => lex_plain(src),
        Syntax::Xml => lex_xml(src),
        Syntax::Patch => lex_patch(src),
        _ => Lexer::new(src, syntax).run(),
    }
}

/// `@@ -a[,b] +c[,d] @@...` -> (old line count, new line count). Counts default to 1.
fn hunk_header(line: &str) -> Option<(u64, u64)> {
    fn count(range: &str) -> Option<u64> {
        let mut parts = range.splitn(2, ',');
        parts.next()?.parse::<u64>().ok()?;
        match parts.next() {
            Some(n) => n.parse().ok(),
            None => Some(1),
        }
    }
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix('+')?;
    let (new, rest) = rest.split_once(' ')?;
    if !rest.starts_with("@@") {
        return None;
    }
    Some((count(old)?, count(new)?))
}

fn is_new_file_header(line: &str) -> bool {
    line == "+++" || line.starts_with("+++ ") || line.starts_with("+++\t")
}

/// Unified diff: emits only the lines the patch adds (`+`), without the `+`.
///
/// Hunks are tracked by the counts in their `@@` headers, so inside a hunk every `+` line is
/// an added line, even one whose content starts with `++`. Outside hunks the `+++` file
/// header is skipped; context (` `), removed (`-`), `\ No newline` and all other header or
/// message lines are not checked. Fail-closed: a `+` line outside any recognized hunk (a
/// malformed or hand-edited patch) that is not a `+++` header is still checked.
fn lex_patch(src: &str) -> Vec<Chunk> {
    let mut out = Vec::new();
    // Remaining (old, new) lines of the current hunk.
    let mut hunk: Option<(u64, u64)> = None;
    for (i, line) in src.lines().enumerate() {
        let mut emit = |text: &str| {
            let t = text.trim();
            if !t.is_empty() {
                out.push(Chunk {
                    kind: ChunkKind::AddedLine,
                    text: t.to_string(),
                    line: i + 1,
                });
            }
        };
        if let Some((old, new)) = hunk.as_mut() {
            let consumed = match line.chars().next() {
                Some('+') if *new > 0 => {
                    *new -= 1;
                    emit(&line[1..]);
                    true
                }
                Some('-') if *old > 0 => {
                    *old -= 1;
                    true
                }
                // Some tools strip the space of an empty context line.
                Some(' ') | None if *old > 0 && *new > 0 => {
                    *old -= 1;
                    *new -= 1;
                    true
                }
                Some('\\') => true,
                // Does not fit the hunk: treat it as outside any hunk.
                _ => false,
            };
            if !consumed || (*old == 0 && *new == 0) {
                hunk = None;
            }
            if consumed {
                continue;
            }
        }
        if let Some(counts) = hunk_header(line) {
            hunk = (counts != (0, 0)).then_some(counts);
        } else if !is_new_file_header(line)
            && let Some(rest) = line.strip_prefix('+')
        {
            emit(rest);
        }
    }
    out
}

fn lex_plain(src: &str) -> Vec<Chunk> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| Chunk {
            kind: ChunkKind::Text,
            text: l.trim().to_string(),
            line: i + 1,
        })
        .collect()
}

fn lex_xml(src: &str) -> Vec<Chunk> {
    // Blank out comments (keeping newlines so line numbers stay right), then check lines.
    let mut kept = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(start) = rest.find("<!--") {
        kept.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let (comment, tail) = match after.find("-->") {
            Some(end) => (&after[..end], &after[end + 3..]),
            None => (after, ""),
        };
        kept.extend(comment.chars().filter(|&c| c == '\n'));
        rest = tail;
    }
    kept.push_str(rest);
    lex_plain(&kept)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct Lexer {
    s: Vec<char>,
    i: usize,
    line: usize,
    syntax: Syntax,
    out: Vec<Chunk>,
}

impl Lexer {
    fn new(src: &str, syntax: Syntax) -> Self {
        Lexer {
            s: src.chars().collect(),
            i: 0,
            line: 1,
            syntax,
            out: Vec::new(),
        }
    }

    fn at(&self, pat: &str) -> bool {
        (self.i..)
            .zip(pat.chars())
            .all(|(j, p)| self.s.get(j) == Some(&p))
    }

    fn peek(&self, off: usize) -> Option<char> {
        self.s.get(self.i + off).copied()
    }

    /// Advances one char, counting lines.
    fn bump(&mut self) {
        if self.s[self.i] == '\n' {
            self.line += 1;
        }
        self.i += 1;
    }

    fn emit(&mut self, kind: ChunkKind, text: String, line: usize) {
        if !text.trim().is_empty() {
            self.out.push(Chunk { kind, text, line });
        }
    }

    fn run(mut self) -> Vec<Chunk> {
        let c_comments = matches!(self.syntax, Syntax::Rust | Syntax::CLike);
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c_comments && self.at("//") {
                self.skip_line();
            } else if c_comments && self.at("/*") {
                self.skip_block(self.syntax == Syntax::Rust);
            } else if self.syntax == Syntax::Hash && c == '#' {
                self.skip_line();
            } else if is_word(c) {
                self.word();
            } else if c == '"' {
                self.string_start('"');
            } else if c == '\'' {
                if self.syntax == Syntax::Rust {
                    self.rust_quote();
                } else {
                    self.string_start('\'');
                }
            } else if c == '`' && self.syntax == Syntax::CLike {
                self.i += 1;
                self.string_body("`", true, true);
            } else {
                self.bump();
            }
        }
        self.out
    }

    fn skip_line(&mut self) {
        while self.i < self.s.len() && self.s[self.i] != '\n' {
            self.i += 1;
        }
    }

    fn skip_block(&mut self, nested: bool) {
        self.i += 2;
        let mut depth = 1usize;
        while self.i < self.s.len() {
            if nested && self.at("/*") {
                depth += 1;
                self.i += 2;
            } else if self.at("*/") {
                depth -= 1;
                self.i += 2;
                if depth == 0 {
                    return;
                }
            } else {
                self.bump();
            }
        }
    }

    fn word(&mut self) {
        let start = self.i;
        let line = self.line;
        while self.i < self.s.len() && is_word(self.s[self.i]) {
            self.i += 1;
        }
        let text: String = self.s[start..self.i].iter().collect();
        if self.syntax == Syntax::Rust {
            // Raw strings: r"..", r#".."#, br#".."#, cr#".."#.
            if matches!(text.as_str(), "r" | "br" | "cr") {
                let mut j = self.i;
                while self.s.get(j) == Some(&'#') {
                    j += 1;
                }
                if self.s.get(j) == Some(&'"') {
                    let hashes = j - self.i;
                    self.i = j + 1;
                    let close = format!("\"{}", "#".repeat(hashes));
                    self.string_body(&close, false, true);
                    return;
                }
            }
            // Prefixed strings b"..", c"..": the prefix is not a word of its own.
            if matches!(text.as_str(), "b" | "c") && self.peek(0) == Some('"') {
                return;
            }
        }
        self.emit(ChunkKind::Identifier, text, line);
    }

    fn string_start(&mut self, q: char) {
        let triple: String = [q, q, q].iter().collect();
        let triple_ok = match self.syntax {
            Syntax::Hash => true,
            Syntax::CLike => q == '"',
            _ => false,
        };
        if triple_ok && self.at(&triple) {
            self.i += 3;
            self.string_body(&triple, true, true);
        } else {
            self.i += 1;
            // Rust strings may span lines; elsewhere an unterminated quote ends at the line
            // end so a stray apostrophe cannot swallow (and hide) the rest of the file.
            let multiline = self.syntax == Syntax::Rust;
            self.string_body(&q.to_string(), true, multiline);
        }
    }

    /// Reads a string body up to `close` (already past the opening quote) and emits it.
    fn string_body(&mut self, close: &str, escapes: bool, multiline: bool) {
        let line = self.line;
        let mut text = String::new();
        while self.i < self.s.len() {
            if self.at(close) {
                self.i += close.chars().count();
                break;
            }
            let c = self.s[self.i];
            if c == '\n' && !multiline {
                break;
            }
            if escapes && c == '\\' && self.i + 1 < self.s.len() {
                self.bump();
                text.push(' ');
                self.bump();
                continue;
            }
            text.push(c);
            self.bump();
        }
        self.emit(ChunkKind::StringLiteral, text, line);
    }

    /// Rust `'`: a char literal (`'a'`, `'\n'`, `'"'`) or a lifetime/label (`'a`).
    fn rust_quote(&mut self) {
        if self.peek(1) == Some('\\') {
            // Escaped char literal: skip to the closing quote on this line.
            self.i += 2;
            while self.i < self.s.len() && self.s[self.i] != '\'' && self.s[self.i] != '\n' {
                self.i += 1;
            }
            if self.i < self.s.len() && self.s[self.i] == '\'' {
                self.i += 1;
            }
        } else if self.peek(2) == Some('\'') && self.peek(1) != Some('\n') {
            self.i += 3;
        } else {
            // Lifetime or label: the name that follows is lexed as an identifier.
            self.i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str, syn: Syntax) -> Vec<(ChunkKind, String, usize)> {
        lex(src, syn)
            .into_iter()
            .map(|c| (c.kind, c.text, c.line))
            .collect()
    }

    #[test]
    fn rust_comments_are_dropped() {
        let src = "// a1\n/// a2\n//! a3\n/* a4 /* nested */ still */ fn f() {}\n";
        let t = texts(src, Syntax::Rust);
        assert_eq!(
            t,
            vec![
                (ChunkKind::Identifier, "fn".into(), 4),
                (ChunkKind::Identifier, "f".into(), 4),
            ]
        );
    }

    #[test]
    fn rust_strings_and_quotes() {
        let src =
            "let u = \"http://x\"; let r = r#\"a \"q\" b\"#; let c = '\"'; fn g<'tenant>() {}";
        let t = texts(src, Syntax::Rust);
        assert!(t.contains(&(ChunkKind::StringLiteral, "http://x".into(), 1)));
        assert!(t.contains(&(ChunkKind::StringLiteral, "a \"q\" b".into(), 1)));
        assert!(t.contains(&(ChunkKind::Identifier, "tenant".into(), 1)));
        // The char literal '"' must not open a string that swallows `fn g`.
        assert!(t.contains(&(ChunkKind::Identifier, "g".into(), 1)));
    }

    #[test]
    fn hash_comments_and_strings() {
        let src = "name = \"a#b\" # comment word\n'''x\ny''' z\n";
        let t = texts(src, Syntax::Hash);
        assert_eq!(
            t,
            vec![
                (ChunkKind::Identifier, "name".into(), 1),
                (ChunkKind::StringLiteral, "a#b".into(), 1),
                (ChunkKind::StringLiteral, "x\ny".into(), 2),
                (ChunkKind::Identifier, "z".into(), 3),
            ]
        );
    }

    #[test]
    fn xml_comments_dropped_lines_kept() {
        let t = texts("<a>\n<!-- x\ny -->\n<b/>\n", Syntax::Xml);
        assert_eq!(
            t,
            vec![
                (ChunkKind::Text, "<a>".into(), 1),
                (ChunkKind::Text, "<b/>".into(), 4),
            ]
        );
    }

    #[test]
    fn syntax_detection() {
        assert_eq!(syntax_for("lib.rs"), Syntax::Rust);
        assert_eq!(syntax_for("Cargo.toml"), Syntax::Hash);
        assert_eq!(syntax_for("Android.bp"), Syntax::CLike);
        assert_eq!(syntax_for("Makefile"), Syntax::Hash);
        assert_eq!(syntax_for("0001-x.patch"), Syntax::Patch);
        assert_eq!(syntax_for("fix.DIFF"), Syntax::Patch);
        assert_eq!(syntax_for("notes.txt"), Syntax::Plain);
    }

    #[test]
    fn patch_only_added_lines() {
        let src = "Subject: a1\n--- a/x\n+++ b/x\n@@ -1,3 +1,4 @@ a2\n a3\n-a4\n+b1\n+++b2\n\n\\ No newline\n+++ b/y\n@@ -0,0 +1 @@\n+b3\n";
        let t = texts(src, Syntax::Patch);
        assert_eq!(
            t,
            vec![
                (ChunkKind::AddedLine, "b1".into(), 7),
                (ChunkKind::AddedLine, "++b2".into(), 8),
                (ChunkKind::AddedLine, "b3".into(), 13),
            ]
        );
    }

    #[test]
    fn patch_outside_hunk_plus_lines_are_checked() {
        // Hunk too short for its lines: the extra `+` line is still checked.
        let src = "@@ -1 +1 @@\n-a\n+b1\n+b2\n+++ b/z\n";
        let t = texts(src, Syntax::Patch);
        assert_eq!(
            t,
            vec![
                (ChunkKind::AddedLine, "b1".into(), 3),
                (ChunkKind::AddedLine, "b2".into(), 4),
            ]
        );
    }
}
