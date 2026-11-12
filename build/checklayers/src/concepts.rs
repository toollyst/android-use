//! Forbidden-concept lint (`LAYERS.toml [forbidden]` + `LAYER.toml forbid_concepts`).
//!
//! Checked: identifiers, string literals, file and directory names, dependency crate names
//! (the latter in `deps.rs`); in `*.patch` / `*.diff` files only the lines the patch adds.
//! Not checked: comments and `*.md` documents. Case-insensitive, word-prefix matching with
//! per-concept exceptions (see `words.rs`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::config::{Config, DirConfig, Layer, concept_key};
use crate::lexer::{self, ChunkKind};
use crate::words::{contains_prefix_run, split_words};
use crate::{Entry, Rule, Stats, Violation};

pub struct Concept {
    pub raw: String,
    words: Vec<String>,
    /// Exact words exempt from this concept's prefix match (`[forbidden_exceptions]`).
    exceptions: BTreeSet<String>,
    /// Where the concept is declared, for messages.
    pub sources: String,
}

pub struct Hit<'a> {
    pub concept: &'a str,
    pub sources: &'a str,
}

/// The concepts forbidden for an item inside `layer`, governed by the `LAYER.toml` `chain`.
pub fn concepts_for(cfg: &Config, layer: &Layer, chain: &[&DirConfig]) -> Vec<Concept> {
    let mut list: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |raw: &str, source: String| {
        let key = concept_key(raw);
        if key.is_empty() {
            return;
        }
        match list.iter_mut().find(|(k, _)| *k == key) {
            Some((_, s)) => {
                if !s.contains(&source) {
                    s.push(source);
                }
            }
            None => list.push((key, vec![source])),
        }
    };
    for key in ["*", layer.name.as_str()] {
        for c in cfg.forbidden.get(key).into_iter().flatten() {
            add(c, format!("LAYERS.toml [forbidden].\"{key}\""));
        }
    }
    for d in chain {
        for c in &d.forbid_concepts {
            add(c, crate::slash(&d.file));
        }
    }
    list.into_iter()
        .map(|(key, sources)| Concept {
            raw: key.replace(' ', "_"),
            words: key.split(' ').map(str::to_string).collect(),
            exceptions: cfg
                .concept_exceptions
                .get(&key)
                .cloned()
                .unwrap_or_default(),
            sources: sources.join(", "),
        })
        .collect()
}

pub fn matches<'a>(concepts: &'a [Concept], text: &str) -> Vec<Hit<'a>> {
    let words = split_words(text);
    concepts
        .iter()
        .filter(|c| contains_prefix_run(&words, &c.words, &c.exceptions))
        .map(|c| Hit {
            concept: &c.raw,
            sources: &c.sources,
        })
        .collect()
}

fn snippet(s: &str) -> String {
    let one_line: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 80 {
        let cut: String = one_line.chars().take(77).collect();
        format!("{cut}...")
    } else {
        one_line
    }
}

/// Checks one walked entry inside `layer`.
pub fn check_entry(
    root: &Path,
    cfg: &Config,
    layer: &Layer,
    entry: &Entry,
    stats: &mut Stats,
    out: &mut Vec<Violation>,
) {
    let (rel, is_dir, is_symlink) = (entry.rel.as_path(), entry.is_dir, entry.is_symlink);
    let name = rel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Documents are excluded entirely (LAYERS.toml: "Not checked: comments and documents (*.md)").
    if !is_dir && name.to_ascii_lowercase().ends_with(".md") {
        return;
    }
    let parent = rel.parent().unwrap_or(Path::new(""));
    let chain = cfg.chain(parent);
    let concepts = concepts_for(cfg, layer, &chain);
    let loc = crate::slash(rel);

    let what = if is_dir {
        "directory name"
    } else {
        "file name"
    };
    for hit in matches(&concepts, &name) {
        out.push(Violation::new(
            Rule::ForbiddenConcept,
            loc.clone(),
            format!(
                "\"{}\" in {what} `{name}` (forbidden here by {})",
                hit.concept, hit.sources
            ),
        ));
    }
    // LAYER.toml is the checker's own configuration: it lists the forbidden words.
    if is_dir || is_symlink || name == "LAYER.toml" {
        return;
    }
    let bytes = match fs::read(root.join(rel)) {
        Ok(b) => b,
        Err(e) => {
            out.push(Violation::new(
                Rule::Unreadable,
                loc,
                format!("cannot read file: {e}"),
            ));
            return;
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(t) if !t.contains('\0') => t,
        _ => {
            stats.skipped_binary.push(loc);
            return;
        }
    };
    stats.files_scanned += 1;
    for chunk in lexer::lex(&text, lexer::syntax_for(&name)) {
        for hit in matches(&concepts, &chunk.text) {
            let kind = chunk.kind.describe();
            let shown = match chunk.kind {
                ChunkKind::Identifier => format!("`{}`", chunk.text),
                _ => format!("\"{}\"", snippet(&chunk.text)),
            };
            out.push(Violation::new(
                Rule::ForbiddenConcept,
                format!("{loc}:{}", chunk.line),
                format!(
                    "\"{}\" in {kind} {shown} (forbidden here by {})",
                    hit.concept, hit.sources
                ),
            ));
        }
    }
}
