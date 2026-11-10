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
//! The `ui` crate compiles this same file as a lib test module
//! (`ui/src/lib.rs`), so the desktop job's `gate` runs it whenever the IDE
//! changes, not only on eUI PRs; the roots are absolute for that reason.
//!
//! A recipe that returns an unnamed button for its CALLER to name says so on
//! the line above the construction: `// a11y: named by the caller`.
//!
//! #EXP-1744: a clickable `div()` / `h_flex()` / `v_flex()` (a row, a tab, a
//! card) is no AccessKit node at all until it has a role, so every such chain
//! with an `.on_click(` carries a `.role(…)` and an `.aria_label(…)` in its
//! own builder chain (its `.child(…)` arguments are other elements and do not
//! count), and so does the `let` chain of a variable that gets one later
//! (`row = row.on_click(…)`). A container that only swallows clicks says so:
//! `// a11y: a click guard, not a control`.

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
const CLICK_GUARD: &str = "a11y: a click guard, not a control";

/// The plain containers that only become a control by their `.on_click(`.
const CONTAINERS: &[&str] = &["div()", "h_flex()", "v_flex()"];

/// What else makes an interactive node out of a variable's `let` chain.
const CONTROLS: &[&str] = &["Button::new(", "_button(", "Switch::new(", "web_switch(", "checkbox(", "entity_chip(", "PopupMenuItem::", ".role("];

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

/// The end of the bracket group opening at `open`.
fn close(text: &str, open: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
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
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// The calls whose arguments are other elements (children, menus).
const SKIPPED: &[&str] = &[".child(", ".children(", ".context_menu(", ".pointer_context_menu(", ".pointer_menu("];

/// [`chain`] without the arguments of its [`SKIPPED`] calls: what the
/// element at `start` itself is built with.
fn own_chain(text: &str, start: usize) -> String {
    let built = chain(text, start);
    let mut own = String::with_capacity(built.len());
    let mut i = 0;
    while i < built.len() {
        let rest = &built[i..];
        if let Some(call) = SKIPPED.iter().find(|call| rest.starts_with(**call)) {
            own.push_str(call);
            i += close(rest, call.len() - 1).max(call.len() - 1) + 1;
            continue;
        }
        let c = rest.chars().next().expect("a char");
        own.push(c);
        i += c.len_utf8();
    }
    own
}

/// The two lines above the line holding `at`.
fn above(text: &str, at: usize) -> Vec<&str> {
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    text[..line_start.saturating_sub(1)].rsplit('\n').take(2).collect()
}

/// `file:line` of every clickable container in `text` without a role and a
/// name (#EXP-1744).
fn roleless(name: &str, text: &str) -> Vec<String> {
    let line_of = |at: usize| format!("{name}:{}", text[..at].matches('\n').count() + 1);
    let mut found = Vec::new();
    for needle in CONTAINERS {
        for (at, _) in text.match_indices(needle) {
            let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let before = &text[line_start..at];
            if before.trim_start().starts_with("//") || before.matches('"').count() % 2 == 1 {
                continue;
            }
            if before.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let notes = above(text, at);
            if notes.iter().any(|line| line.contains(CLICK_GUARD)) {
                continue;
            }
            let own = own_chain(text, at);
            if !own.contains(".on_click(") {
                continue;
            }
            let named = own.contains(".aria_label(") || notes.iter().any(|line| line.contains(CALLER_NAMES));
            if !own.contains(".role(") || !named {
                found.push(line_of(at));
            }
        }
    }
    // `row = row.on_click(…)` / `card.on_click(…)`: the variable's `let`.
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices(".on_click(") {
        let ident_start = text[..at].rfind(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(0, |i| i + 1);
        let ident = &text[ident_start..at];
        if ident.is_empty() || !ident.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
            continue;
        }
        // A method chain's `.on_click(` (`foo().on_click(`, `x.y.on_click(`).
        if ident_start > 0 && matches!(bytes[ident_start - 1], b'.' | b')') {
            continue;
        }
        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        if text[line_start..at].trim_start().starts_with("//") {
            continue;
        }
        // The last few bindings: `let switch = match … { … switch.on_click(…) }`
        // shadows the one that built it.
        let mut bindings: Vec<usize> = [format!("let {ident} "), format!("let mut {ident} "), format!("let {ident}:"), format!("let mut {ident}:")]
            .iter()
            .flat_map(|binding| text[..at].match_indices(binding.as_str()).map(|(i, _)| i).collect::<Vec<_>>())
            .collect();
        bindings.sort_unstable();
        // A closure's `|this| this.on_click(…)`: its chain is the container's.
        let Some(&last) = bindings.last() else { continue };
        if text[last..at].contains(&format!("|{ident}|")) {
            continue;
        }
        let bound = bindings[bindings.len().saturating_sub(3)];
        let span = &text[bound..at];
        if above(text, bound).iter().any(|line| line.contains(CLICK_GUARD)) {
            continue;
        }
        if !CONTROLS.iter().any(|control| span.contains(control)) && !NEEDLES.iter().any(|needle| span.contains(needle)) {
            found.push(line_of(at));
        }
    }
    found
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
    for root in [crates.join("eui-gpui").join("src"), crates.join("ui").join("src")] {
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
fn every_clickable_container_has_a_role() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = manifest.parent().expect("crates/");
    let mut offenders = Vec::new();
    for root in [crates.join("eui-gpui").join("src"), crates.join("ui").join("src")] {
        let mut files = Vec::new();
        sources(&root, &mut files);
        for path in files {
            let text = std::fs::read_to_string(&path).expect("a readable source file");
            let name = path.strip_prefix(crates).unwrap_or(&path).to_string_lossy().to_string();
            offenders.extend(roleless(&name, &text));
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "#EXP-1744: {} clickable div(s) without a role and an accessible name; chain \
         `.role(gpui::Role::Button | ListItem | Tab)` and `.aria_label(…)` after its \
         `.id(…)`, with the string web/iOS/Android use:\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

#[test]
fn the_scan_sees_a_roleless_clickable_div() {
    let row = r#"div().id("r").role(gpui::Role::ListItem).aria_label(title).on_click(f)"#;
    let bare = r#"h_flex().id("r").child(div().role(gpui::Role::Button)).on_click(move |_, _, cx| go(cx));"#;
    let when = "v_flex().id(\"t\").when(open, |this| this.on_click(f))";
    let guard = "// a11y: a click guard, not a control\n    div().id(id).on_click(|_, _, cx| cx.stop_propagation())";
    let variable = "let mut row = div().id(\"r\").child(\"x\");\n    row = row.on_click(f);";
    let button = "let mut b = Button::new(\"b\").label(\"Go\");\n    b = b.on_click(f);";
    let closure = "let x = 1;\n    div().when(true, |row| row.on_click(f))";
    assert!(roleless("t", row).is_empty());
    assert_eq!(roleless("t", bare), vec!["t:1".to_string()], "a child's role is not the row's");
    assert_eq!(roleless("t", when), vec!["t:1".to_string()]);
    assert!(roleless("t", guard).is_empty());
    assert_eq!(roleless("t", variable), vec!["t:2".to_string()]);
    assert!(roleless("t", button).is_empty());
    assert_eq!(roleless("t", closure), vec!["t:2".to_string()], "the container, not the closure");
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
