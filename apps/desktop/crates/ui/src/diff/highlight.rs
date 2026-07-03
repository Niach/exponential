//! Filename → language detection + per-line syntax-highlight computation for
//! the diff view (masterplan-v3 §7.8 bullet 3).
//!
//! Highlighting runs through gpui-component's Tree-sitter `highlighter`
//! (`SyntaxHighlighter` / `LanguageRegistry` / `HighlightTheme`) with the
//! grammars enabled via the `tree-sitter-languages` feature. A patch is not a
//! full file, so each file side (old = context+removed, new = context+added)
//! is joined into one fragment document and parsed once; Tree-sitter degrades
//! gracefully on fragments — token-level captures (keywords, strings,
//! comments) survive, which is exactly what a diff view needs (same approach
//! GitHub takes). Unknown languages fall back to plain text (no styles), never
//! an error.

use std::ops::Range;

use gpui::HighlightStyle;
use gpui_component::highlighter::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};
use ropey::Rope;

/// Map a PR file path to a gpui-component `LanguageRegistry` key (§7.8:
/// "keyed by file extension → language"). Returns `"text"` when no bundled
/// grammar matches — `SyntaxHighlighter::new` treats that as plain text.
///
/// The registry itself resolves short aliases (`rs`, `ts`, `py`, …), but the
/// mapping is explicit here so (a) multi-alias extensions the registry does
/// NOT know (`mjs`, `h`, `htm`, `gql`, …) still highlight, and (b) the set is
/// unit-testable against the registry.
pub fn language_for_filename(filename: &str) -> &'static str {
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    // Extension-less well-known files first.
    match basename {
        "Makefile" | "makefile" | "GNUmakefile" => return "make",
        "CMakeLists.txt" => return "cmake",
        _ => {}
    }
    let ext = match basename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext,
        // ".env"-style dotfiles and extension-less paths.
        _ => return "text",
    };
    match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        // No dedicated jsx grammar is bundled; the tsx grammar is a superset.
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "json" | "jsonc" => "json",
        "py" => "python",
        "rb" => "ruby",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" | "ktm" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "cs" => "csharp",
        "php" | "phtml" => "php",
        "lua" => "lua",
        "scala" => "scala",
        "ex" | "exs" => "elixir",
        "erb" => "erb",
        "ejs" => "ejs",
        "astro" => "astro",
        "svelte" => "svelte",
        "html" | "htm" => "html",
        "css" | "scss" => "css",
        "md" | "mdx" | "markdown" => "markdown",
        "sh" | "bash" | "zsh" => "bash",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "sql" => "sql",
        "zig" => "zig",
        "proto" => "proto",
        "graphql" | "gql" => "graphql",
        "cmake" => "cmake",
        "mk" => "make",
        "diff" | "patch" => "diff",
        _ => "text",
    }
}

/// Whether a language name resolves to a real registered grammar (used to
/// skip the parse entirely for plain text).
fn has_grammar(lang: &str) -> bool {
    lang != "text" && LanguageRegistry::singleton().language(lang).is_some()
}

/// Syntax-highlight one side of a file's patch. `lines` are the side's lines
/// in patch order (a concatenation of hunk fragments). Returns one style-run
/// vector per input line, with byte ranges LOCAL to that line — ready to hand
/// to `StyledText::with_highlights`.
///
/// Plain text / unknown languages return all-empty runs (cheap no-op).
pub fn highlight_lines(
    lang: &str,
    lines: &[&str],
    theme: &HighlightTheme,
) -> Vec<Vec<(Range<usize>, HighlightStyle)>> {
    if lines.is_empty() {
        return Vec::new();
    }
    if !has_grammar(lang) {
        return lines.iter().map(|_| Vec::new()).collect();
    }

    let doc = lines.join("\n");
    let mut highlighter = SyntaxHighlighter::new(lang);
    let rope = Rope::from_str(&doc);
    // No timeout: parse to completion. Patch fragments are bounded (GitHub
    // omits `patch` beyond its size cap), and this runs once per set_files,
    // not per frame.
    highlighter.update(None, &rope, None);
    let styles = highlighter.styles(&(0..doc.len()), theme);

    // Split the document-wide style runs at line boundaries.
    let mut per_line: Vec<Vec<(Range<usize>, HighlightStyle)>> = Vec::with_capacity(lines.len());
    let mut line_start = 0usize;
    let mut run_ix = 0usize;
    for line in lines {
        let line_end = line_start + line.len();
        let mut runs = Vec::new();
        // Advance past runs that end before this line.
        while run_ix < styles.len() && styles[run_ix].0.end <= line_start {
            run_ix += 1;
        }
        let mut ix = run_ix;
        while ix < styles.len() && styles[ix].0.start < line_end {
            let (range, style) = &styles[ix];
            let start = range.start.max(line_start) - line_start;
            let end = range.end.min(line_end) - line_start;
            if start < end && *style != HighlightStyle::default() {
                runs.push((start..end, *style));
            }
            if range.end > line_end {
                break; // run continues into the next line; re-visit it there
            }
            ix += 1;
        }
        per_line.push(runs);
        line_start = line_end + 1; // + the '\n' separator
    }
    per_line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_languages_from_paths() {
        assert_eq!(language_for_filename("apps/desktop/crates/ui/src/diff.rs"), "rust");
        assert_eq!(language_for_filename("apps/web/src/lib/trpc/issues.ts"), "typescript");
        assert_eq!(language_for_filename("apps/web/src/components/diff-view.tsx"), "tsx");
        assert_eq!(language_for_filename("src/App.jsx"), "javascript");
        assert_eq!(language_for_filename("lib.mjs"), "javascript");
        assert_eq!(language_for_filename("include/foo.h"), "c");
        assert_eq!(language_for_filename("src/engine.hpp"), "cpp");
        assert_eq!(language_for_filename("app/build.gradle.kts"), "kotlin");
        assert_eq!(language_for_filename("Sources/App/Model.swift"), "swift");
        assert_eq!(language_for_filename("Makefile"), "make");
        assert_eq!(language_for_filename("third_party/CMakeLists.txt"), "cmake");
        assert_eq!(language_for_filename("docker-compose.yaml"), "yaml");
        assert_eq!(language_for_filename(".github/workflows/ci.yml"), "yaml");
        assert_eq!(language_for_filename("docs/masterplan-v3.md"), "markdown");
        assert_eq!(language_for_filename("schema.graphql"), "graphql");
        assert_eq!(language_for_filename("query.gql"), "graphql");
        // Unknown / binary / dotfile → plain text, never a panic.
        assert_eq!(language_for_filename("logo.png"), "text");
        assert_eq!(language_for_filename(".env"), "text");
        assert_eq!(language_for_filename("LICENSE"), "text");
        assert_eq!(language_for_filename(""), "text");
    }

    #[test]
    fn mapped_languages_resolve_in_the_registry() {
        // Every non-text mapping must point at a real bundled grammar —
        // otherwise SyntaxHighlighter falls back to text silently and the
        // mapping entry is dead.
        for (file, expect) in [
            ("a.rs", "rust"),
            ("a.ts", "typescript"),
            ("a.tsx", "tsx"),
            ("a.js", "javascript"),
            ("a.json", "json"),
            ("a.py", "python"),
            ("a.rb", "ruby"),
            ("a.go", "go"),
            ("a.java", "java"),
            ("a.kt", "kotlin"),
            ("a.swift", "swift"),
            ("a.c", "c"),
            ("a.cpp", "cpp"),
            ("a.cs", "csharp"),
            ("a.php", "php"),
            ("a.lua", "lua"),
            ("a.scala", "scala"),
            ("a.ex", "elixir"),
            ("a.erb", "erb"),
            ("a.ejs", "ejs"),
            ("a.astro", "astro"),
            ("a.svelte", "svelte"),
            ("a.html", "html"),
            ("a.css", "css"),
            ("a.md", "markdown"),
            ("a.sh", "bash"),
            ("a.yaml", "yaml"),
            ("a.toml", "toml"),
            ("a.sql", "sql"),
            ("a.zig", "zig"),
            ("a.proto", "proto"),
            ("a.graphql", "graphql"),
            ("a.cmake", "cmake"),
            ("a.mk", "make"),
            ("a.patch", "diff"),
        ] {
            assert_eq!(language_for_filename(file), expect, "{file}");
            assert!(
                LanguageRegistry::singleton().language(expect).is_some(),
                "{expect} must be a registered grammar"
            );
        }
    }

    #[test]
    fn highlights_rust_keywords_with_line_local_ranges() {
        let theme = HighlightTheme::default_dark();
        let lines = ["fn main() {", "    let answer = 42;", "}"];
        let per_line = highlight_lines("rust", &lines, &theme);
        assert_eq!(per_line.len(), 3);
        // `fn` on line 0 must be a styled run starting at byte 0.
        assert!(
            per_line[0].iter().any(|(range, style)| {
                range.start == 0 && range.end >= 2 && *style != HighlightStyle::default()
            }),
            "expected a styled `fn` keyword run, got {:?}",
            per_line[0]
        );
        // `let` on line 1 is at bytes 4..7 — proves ranges are line-local,
        // not document offsets.
        assert!(
            per_line[1]
                .iter()
                .any(|(range, _)| range.start == 4 && range.end >= 7),
            "expected a line-local `let` run, got {:?}",
            per_line[1]
        );
        // Every emitted range stays inside its line.
        for (line, runs) in lines.iter().zip(&per_line) {
            for (range, _) in runs {
                assert!(range.end <= line.len(), "range {range:?} escapes line {line:?}");
            }
        }
    }

    #[test]
    fn plain_text_and_empty_input_are_cheap_no_ops() {
        let theme = HighlightTheme::default_dark();
        assert!(highlight_lines("text", &["hello world"], &theme)[0].is_empty());
        assert!(highlight_lines("no-such-lang", &["x"], &theme)[0].is_empty());
        assert!(highlight_lines("rust", &[], &theme).is_empty());
    }
}
