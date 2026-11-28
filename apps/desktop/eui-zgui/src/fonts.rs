//! The faces both renderers paint with: the system collection plus the
//! desktop's bundled Inter + JetBrains Mono (`apps/desktop/assets/fonts`)
//! and eUI's `Inter Symbols` key-symbol fallback (as eui-gpui registers).

use std::sync::Arc;

use zgui::app::Fonts;

/// The family stack every sans run is shaped with (theme family first, the
/// key-symbol fallback right after it, as on every eUI renderer).
pub const SANS_STACK: &str = "\"Inter\", \"Inter Symbols\", sans-serif";
pub const MONO_STACK: &str = "\"JetBrains Mono\", \"Inter Symbols\", monospace";

pub fn font_files() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let dirs = [crate::repo().join("apps/desktop/assets/fonts"), crate::repo().join("apps/desktop/crates/eui-gpui/assets/fonts")];
    for dir in dirs {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "ttf") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// System faces + the bundled ones.
pub fn fonts() -> Fonts {
    let fonts = Fonts::system();
    for p in font_files() {
        let bytes: Vec<u8> = std::fs::read(&p).expect("font");
        let _ = fonts.register(Arc::new(bytes), None);
    }
    fonts
}

/// A CSS family stack for a theme family name.
pub fn stack(family: Option<&str>) -> String {
    match family.map(str::trim) {
        None | Some("") | Some("Inter") | Some("system-ui") | Some("sans-serif") | Some("ui-sans-serif") => SANS_STACK.to_string(),
        Some(f) if is_mono(f) => MONO_STACK.to_string(),
        Some(f) => format!("\"{}\", {SANS_STACK}", f.split(',').next().unwrap_or(f).trim().trim_matches('"')),
    }
}

/// The family list as names (for the measurer's `TextStyle`).
pub fn stack_names(family: Option<&str>) -> Vec<String> {
    stack(family).split(',').map(|s| s.trim().trim_matches('"').to_string()).collect()
}

pub fn is_mono(f: &str) -> bool {
    let f = f.to_ascii_lowercase();
    f.contains("mono") || f.contains("code") || f.contains("consol") || f.contains("menlo") || f.contains("courier")
}
