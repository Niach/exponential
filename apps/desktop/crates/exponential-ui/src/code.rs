//! Round 1 (docs/round-1-contract.md §3): CodeBlock's BUILT-IN tokenizer, a
//! line-by-line port of `src/code.ts` over `catalog/code.json` (embedded).
//! One forward scan, first matching rule wins; the tokens split into LINES
//! and adjacent tokens of one kind merge. `fixtures/code-tokens.json` locks
//! it. The scan walks `char`s where the TS walks UTF-16 units; the merge step
//! makes the two agree for any text (an astral char is one plain token
//! either way).

use std::collections::HashSet;
use std::sync::LazyLock;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::generated::catalog as g;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeToken {
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageSpec {
    #[serde(default)]
    pub plain: bool,
    #[serde(default)]
    pub line_kinds: Option<IndexMap<String, String>>,
    #[serde(default)]
    pub line_comment: Vec<String>,
    #[serde(default)]
    pub block_comment: Vec<(String, String)>,
    #[serde(default)]
    pub strings: Vec<String>,
    #[serde(default)]
    pub multiline: Vec<String>,
    #[serde(default)]
    pub key_strings: bool,
    #[serde(default)]
    pub markup: bool,
    #[serde(default)]
    pub numbers: Option<bool>,
    #[serde(default)]
    pub ident_extra: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub case_insensitive: bool,
    #[serde(default)]
    pub key_idents: bool,
    #[serde(default)]
    pub type_names: Vec<String>,
    #[serde(default)]
    pub types: Option<String>,
    #[serde(default)]
    pub operators: Option<String>,
    #[serde(default)]
    pub punctuation: Option<String>,
}

#[derive(Deserialize)]
struct Defaults {
    operators: String,
    punctuation: String,
}

#[derive(Deserialize)]
struct CodeFile {
    defaults: Defaults,
    languages: IndexMap<String, LanguageSpec>,
}

static CODE: LazyLock<CodeFile> = LazyLock::new(|| serde_json::from_str(g::CODE_JSON).expect("code.json"));

/// The languages, in `codeLanguage` enum order.
pub fn code_language_names() -> Vec<&'static str> {
    CODE.languages.keys().map(String::as_str).collect()
}

pub fn language(name: &str) -> Option<&'static LanguageSpec> {
    CODE.languages.get(name)
}

fn is_digit(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit())
}
fn is_alpha(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic())
}
fn is_space(c: Option<char>) -> bool {
    matches!(c, Some(' ') | Some('\t'))
}

struct Scanner<'a> {
    code: &'a [char],
    out: Vec<CodeToken>,
}

impl Scanner<'_> {
    fn at(&self, i: usize) -> Option<char> {
        self.code.get(i).copied()
    }
    fn starts_with(&self, p: &str, i: usize) -> bool {
        (i..).zip(p.chars()).all(|(k, c)| self.at(k) == Some(c))
    }
    fn slice(&self, a: usize, b: usize) -> String {
        self.code[a..b.min(self.code.len())].iter().collect()
    }
    fn push(&mut self, kind: &str, text: String) {
        self.out.push(CodeToken { kind: kind.to_string(), text });
    }
    fn next_non_space(&self, j: usize) -> Option<char> {
        let mut k = j;
        while is_space(self.at(k)) {
            k += 1;
        }
        self.at(k)
    }
    fn index_of(&self, needle: &str, from: usize) -> Option<usize> {
        let n: Vec<char> = needle.chars().collect();
        if n.is_empty() {
            return Some(from);
        }
        (from..self.code.len()).find(|&k| self.code[k..].starts_with(&n))
    }
    /// True when only spaces/tabs separate `i` from the start of its line.
    fn space_run_to_line_start(&self, i: usize) -> bool {
        let mut k = i as isize - 1;
        while k >= 0 && is_space(self.at(k as usize)) {
            k -= 1;
        }
        k < 0 || self.at(k as usize) == Some('\n')
    }
}

/// The source as flat tokens (text may span lines).
fn scan(code: &[char], spec: &LanguageSpec) -> Vec<CodeToken> {
    let mut s = Scanner { code, out: Vec::new() };
    let n = code.len();
    let operators: Vec<char> = spec.operators.as_deref().unwrap_or(&CODE.defaults.operators).chars().collect();
    let punctuation: Vec<char> = spec.punctuation.as_deref().unwrap_or(&CODE.defaults.punctuation).chars().collect();
    let extra: Vec<char> = spec.ident_extra.as_deref().unwrap_or("").chars().collect();
    let ident_start = |c: Option<char>| is_alpha(c) || c == Some('_') || c == Some('$') || c.is_some_and(|c| extra.contains(&c));
    let ident_part = |c: Option<char>| ident_start(c) || is_digit(c);
    let keywords: HashSet<&str> = spec.keywords.iter().map(String::as_str).collect();
    let type_names: HashSet<&str> = spec.type_names.iter().map(String::as_str).collect();
    if spec.plain {
        return if code.is_empty() { Vec::new() } else { vec![CodeToken { kind: "plain".into(), text: code.iter().collect() }] };
    }
    let mut i = 0usize;
    let mut in_tag = false;
    while i < n {
        let c = code[i];
        if c == '\n' {
            s.push("plain", "\n".into());
            i += 1;
            continue;
        }
        // (1) line kinds
        if let Some(kinds) = &spec.line_kinds {
            if i == 0 || code[i - 1] == '\n' || s.space_run_to_line_start(i) {
                let prefix = kinds.keys().find(|p| s.starts_with(p, i));
                if let Some(prefix) = prefix {
                    if !is_space(Some(c)) {
                        let mut j = i;
                        while j < n && code[j] != '\n' {
                            j += 1;
                        }
                        let kind = kinds[prefix].clone();
                        let text = s.slice(i, j);
                        s.push(&kind, text);
                        i = j;
                        continue;
                    }
                }
            }
        }
        // (2) line comments
        if !in_tag && spec.line_comment.iter().any(|p| s.starts_with(p, i)) {
            let mut j = i;
            while j < n && code[j] != '\n' {
                j += 1;
            }
            let text = s.slice(i, j);
            s.push("comment", text);
            i = j;
            continue;
        }
        // (3) block comments
        if !in_tag {
            if let Some((open, close)) = spec.block_comment.iter().find(|(open, _)| s.starts_with(open, i)) {
                let j = match s.index_of(close, i + open.chars().count()) {
                    Some(end) => end + close.chars().count(),
                    None => n,
                };
                let text = s.slice(i, j);
                s.push("comment", text);
                i = j;
                continue;
            }
        }
        // (4) strings
        let quote = c.to_string();
        if spec.strings.contains(&quote) && (!spec.markup || in_tag) {
            let multi = spec.multiline.contains(&quote);
            let mut j = i + 1;
            while j < n {
                if code[j] == '\\' {
                    j += 2;
                    continue;
                }
                if code[j] == c {
                    j += 1;
                    break;
                }
                if code[j] == '\n' && !multi {
                    break;
                }
                j += 1;
            }
            let j = j.min(n);
            let kind = if spec.key_strings && s.next_non_space(j) == Some(':') { "property" } else { "string" };
            let text = s.slice(i, j);
            s.push(kind, text);
            i = j;
            continue;
        }
        // (5) markup
        if spec.markup {
            if !in_tag {
                let next = s.at(i + 1);
                if c == '<' && (is_alpha(next) || next == Some('/') || next == Some('!')) {
                    let open = if next == Some('/') { "</" } else { "<" };
                    s.push("punctuation", open.into());
                    let mut j = i + open.len();
                    let start = j;
                    while j < n && (is_alpha(Some(code[j])) || is_digit(Some(code[j])) || matches!(code[j], '-' | ':' | '!')) {
                        j += 1;
                    }
                    if j > start {
                        let text = s.slice(start, j);
                        s.push("tag", text);
                    }
                    in_tag = true;
                    i = j;
                    continue;
                }
                let mut j = i;
                while j < n && code[j] != '<' && code[j] != '\n' {
                    j += 1;
                }
                if j == i {
                    j = i + 1;
                }
                let text = s.slice(i, j);
                s.push("plain", text);
                i = j;
                continue;
            }
            if c == '>' || s.starts_with("/>", i) {
                let close = if c == '>' { ">" } else { "/>" };
                s.push("punctuation", close.into());
                in_tag = false;
                i += close.len();
                continue;
            }
            if c == '=' {
                s.push("operator", "=".into());
                i += 1;
                continue;
            }
            if is_alpha(Some(c)) || matches!(c, '_' | ':' | '@') {
                let mut j = i;
                while j < n && (is_alpha(Some(code[j])) || is_digit(Some(code[j])) || "_:@.-".contains(code[j])) {
                    j += 1;
                }
                let text = s.slice(i, j);
                s.push("attribute", text);
                i = j;
                continue;
            }
            if is_space(Some(c)) {
                let mut j = i;
                while is_space(s.at(j)) {
                    j += 1;
                }
                let text = s.slice(i, j);
                s.push("plain", text);
                i = j;
                continue;
            }
            s.push("plain", c.to_string());
            i += 1;
            continue;
        }
        // (6) numbers
        if spec.numbers != Some(false) && (c.is_ascii_digit() || (c == '.' && is_digit(s.at(i + 1)))) {
            let mut j = i + 1;
            while j < n && (code[j].is_ascii_alphanumeric() || code[j] == '_' || code[j] == '.') {
                j += 1;
            }
            let text = s.slice(i, j);
            s.push("number", text);
            i = j;
            continue;
        }
        // (7) identifiers
        if ident_start(Some(c)) {
            let mut j = i + 1;
            while j < n && ident_part(Some(code[j])) {
                j += 1;
            }
            let word = s.slice(i, j);
            let key = if spec.case_insensitive { word.to_lowercase() } else { word.clone() };
            let next = s.next_non_space(j);
            let after = {
                let mut k = j;
                while is_space(s.at(k)) {
                    k += 1;
                }
                s.at(k + 1)
            };
            let prev = s.out.iter().rev().find(|t| !t.text.trim().is_empty());
            let kind = if keywords.contains(key.as_str()) {
                "keyword"
            } else if spec.key_idents && next == Some(':') && after != Some(':') {
                "property"
            } else if next == Some('(') {
                "function"
            } else if prev.is_some_and(|p| p.kind == "punctuation" && p.text.ends_with('.')) {
                "property"
            } else if type_names.contains(word.as_str()) || (spec.types.as_deref() == Some("capitalized") && word.starts_with(|ch: char| ch.is_ascii_uppercase())) {
                "type"
            } else {
                "plain"
            };
            s.push(kind, word);
            i = j;
            continue;
        }
        // (8) spaces
        if is_space(Some(c)) {
            let mut j = i;
            while is_space(s.at(j)) {
                j += 1;
            }
            let text = s.slice(i, j);
            s.push("plain", text);
            i = j;
            continue;
        }
        // (9) operators
        if operators.contains(&c) {
            let mut j = i;
            while j < n && operators.contains(&code[j]) {
                j += 1;
            }
            let text = s.slice(i, j);
            s.push("operator", text);
            i = j;
            continue;
        }
        // (10) punctuation, (11) anything else
        s.push(if punctuation.contains(&c) { "punctuation" } else { "plain" }, c.to_string());
        i += 1;
    }
    s.out
}

/// The source tokenized per LINE: each line's tokens concatenate to exactly
/// its text (no `\n`), adjacent tokens of one kind merged; an empty line is
/// an empty list. An unknown language tokenizes as `plain`.
pub fn tokenize_code(source: &str, language_name: &str) -> Vec<Vec<CodeToken>> {
    let code: String = source.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = code.chars().collect();
    let spec = language(language_name).or_else(|| language("plain")).expect("plain language");
    let mut lines: Vec<Vec<CodeToken>> = vec![Vec::new()];
    for token in scan(&chars, spec) {
        for (index, piece) in token.text.split('\n').enumerate() {
            if index > 0 {
                lines.push(Vec::new());
            }
            if piece.is_empty() {
                continue;
            }
            let line = lines.last_mut().expect("a line");
            match line.last_mut() {
                Some(last) if last.kind == token.kind => last.text.push_str(piece),
                _ => line.push(CodeToken { kind: token.kind.clone(), text: piece.to_string() }),
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_languages_are_the_generated_list() {
        assert_eq!(code_language_names(), g::CODE_LANGUAGE_NAMES);
    }

    #[test]
    fn lines_concatenate_to_their_text() {
        let src = "const x = 1 // one\nlet s = \"a\\\"b\"\n\nfn(x)";
        let lines = tokenize_code(src, "js");
        let joined: Vec<String> = lines.iter().map(|l| l.iter().map(|t| t.text.as_str()).collect()).collect();
        assert_eq!(joined, src.split('\n').collect::<Vec<_>>());
        assert_eq!(lines[0][0], CodeToken { kind: "keyword".into(), text: "const".into() });
        assert!(lines[2].is_empty());
        assert_eq!(tokenize_code("x", "nope"), vec![vec![CodeToken { kind: "plain".into(), text: "x".into() }]]);
    }
}
