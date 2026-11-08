//! #EXP-1488 — no unnamed interactive element: every button the desktop
//! clients build carries an accessible name, so VoiceOver and
//! accessibility-driven automation can find it.
//!
//! The pinned gpui exposes an AccessKit tree (`.role()`, `.aria_label()`),
//! but its headless test window never activates it (`TestWindow` has no
//! `a11y_init`), so no frame here produces a tree to walk. The check is the
//! structural one instead, the same honest grep the IDE's
//! `only_controls_constructs_*` rules use: every construction of a
//! gpui-component `Button` (and the eUI recipes that return one, and the
//! checkbox) in this crate and in the IDE (`crates/ui`) must name itself in
//! its own builder chain with one of [`NAMES`]. The vendored Button resolves
//! the name as `accessibility_label`, else `label`, else the tooltip text.
//!
//! A recipe that returns an unnamed button for its CALLER to name says so on
//! the line above the construction: `// a11y: named by the caller`.

use std::path::{Path, PathBuf};

/// The constructions that make an interactive AccessKit node.
const NEEDLES: &[&str] = &[
    "Button::new(",
    "ghost_icon_button(",
    "glass_icon_button(",
    "call_button(",
    "glass_pill_button(",
    "glass_pill_button_primary(",
    "checkbox(",
    "chip_button(",
];

/// What names one inside its chain.
const NAMES: &[&str] = &[
    ".label(",
    ".tooltip(",
    ".tooltip_with_action(",
    ".accessibility_label(",
    ".aria_label(",
];

const CALLER_NAMES: &str = "a11y: named by the caller";

/// The builder chain starting at `start`: up to the `;` or `,` that ends it
/// at its own depth, or the bracket that closes its enclosing call. Line
/// comments and string literals inside it are skipped.
fn chain(text: &str, start: usize) -> &str {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
                continue;
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth < 0 {
                    return &text[start..i];
                }
            }
            b';' | b',' if depth == 0 => return &text[start..i],
            _ => {}
        }
        i += 1;
    }
    &text[start..]
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a readable source dir") {
        let path = entry.expect("a readable dir entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// `file:line` of every unnamed construction in `text`.
fn unnamed(name: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for needle in NEEDLES {
        for (at, _) in text.match_indices(needle) {
            let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let before = &text[line_start..at];
            // A definition (`fn checkbox(`), a comment, or inside a string.
            if before.contains("fn ") || before.trim_start().starts_with("//") || before.matches('"').count() % 2 == 1 {
                continue;
            }
            // `WebSwitch::new(` and friends are other types; `Button::new(`
            // must be the type itself (`button::Button::new(` included).
            if *needle == "Button::new(" && before.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            if *needle == "checkbox(" && before.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let above = text[..line_start.saturating_sub(1)].rsplit('\n').take(2).collect::<Vec<_>>();
            if above.iter().any(|line| line.contains(CALLER_NAMES)) {
                continue;
            }
            let built = chain(text, at);
            if !NAMES.iter().any(|named| built.contains(named)) {
                let line = text[..at].matches('\n').count() + 1;
                found.push(format!("{name}:{line}"));
            }
        }
    }
    found
}

#[test]
fn every_button_has_an_accessible_name() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = manifest.parent().expect("crates/");
    let mut offenders = Vec::new();
    for root in [manifest.join("src"), crates.join("ui").join("src")] {
        let mut files = Vec::new();
        sources(&root, &mut files);
        for path in files {
            let text = std::fs::read_to_string(&path).expect("a readable source file");
            let name = path.strip_prefix(crates).unwrap_or(&path).to_string_lossy().to_string();
            offenders.extend(unnamed(&name, &text));
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "#EXP-1488: {} interactive element(s) without an accessible name; chain \
         `.accessibility_label(…)` with the string web/iOS/Android use (or a \
         `.tooltip(…)`, which names it too):\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

#[test]
fn the_scan_sees_an_unnamed_icon_button() {
    let named = r#"ghost_icon_button("x", icon, cx).tooltip("Close").on_click(f);"#;
    let unnamed_button = r#"let b = Button::new("x").ghost().icon(icon).on_click(move |_, _, cx| { go(cx); });"#;
    let caller = "// a11y: named by the caller\n    Button::new(id).custom(variant)";
    let commented = "Button::new(\"go\")\n    // one, two; three\n    .label(\"Go\")";
    let nested = r#"row.child(Button::new("go").child("Go")).child(checkbox("c", s, false, cx).aria_label("Select"))"#;
    assert!(unnamed("t", named).is_empty());
    assert_eq!(unnamed("t", unnamed_button), vec!["t:1".to_string()]);
    assert!(unnamed("t", caller).is_empty());
    assert!(unnamed("t", commented).is_empty(), "a comment in the chain does not end it");
    assert_eq!(unnamed("t", nested), vec!["t:1".to_string()], "the Go button, not the named checkbox");
}
