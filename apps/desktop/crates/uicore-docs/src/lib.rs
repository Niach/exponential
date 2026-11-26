//! `uicore-docs` (#EDE-50): the shell's factory genUI documents, the files
//! in `apps/ede/ui` (Express, #EDE-216) compiled in. Exponential Desktop installs that directory
//! as the factory copy `exponential-ui` serves (people and agents edit their
//! own copies, #EDE-9); the IDE embeds the same documents when it hosts the
//! shell. One source, two hosts: a document that only one host can draw is
//! a bug here, not a fork there.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// One factory document: its name (the file stem) and its source: Express
/// (`<name>.express`, #EDE-216), or JSON for the IDE's own tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Doc {
    pub name: &'static str,
    pub source: &'static str,
}

macro_rules! docs {
    ($($name:literal),* $(,)?) => {
        /// Every factory document, in file-name order.
        pub const DOCS: &[Doc] = &[$(Doc { name: $name, source: include_str!(concat!("../../../../ede/ui/", $name, ".express")) }),*];
    };
}

docs!["applet-battery", "applet-bluetooth", "applet-calendar", "applet-drives", "applet-network", "applet-printers", "applet-sound", "chat", "connect", "expo", "ground", "history", "needs-you", "notification-center", "notifications", "osd", "scale", "settings", "sidebar", "space-view", "start-menu", "switcher", "taskbar"];

/// #EDE-235: eDE's factory theme (`apps/ede/ui/theme.json`: the Rerusted
/// palette and eDE's spacing scale), installed beside the documents.
pub const THEME: &str = include_str!("../../../../ede/ui/theme.json");

/// [`THEME`] resolved over the built-ins (once per process): what the
/// sealed surfaces paint in (never the person's editable copy) and the
/// shell's goldens and tests check against.
pub fn factory_theme() -> std::sync::Arc<ede_ui::ResolvedTheme> {
    static RESOLVED: OnceLock<std::sync::Arc<ede_ui::ResolvedTheme>> = OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            let source: Value = serde_json::from_str(THEME).expect("the factory theme parses");
            ede_ui::validate_theme(&source).unwrap_or_else(|issues| panic!("the factory theme: {issues:?}"))
        })
        .clone()
}

/// The tab bar only the IDE draws (eDE has Mint's taskbar).
pub const TABBAR: Doc = Doc { name: "tabbar", source: include_str!("../../../../../packages/uicore/docs/tabbar.json") };

impl Doc {
    /// The document's JSON form (`{surface, root, data}`): an Express
    /// source compiled by ede-ui (once per process), a JSON one parsed.
    /// The sources are checked by the tests.
    pub fn json(&self) -> Value {
        static COMPILED: OnceLock<Mutex<HashMap<&'static str, Value>>> = OnceLock::new();
        if self.source.trim_start().starts_with('{') {
            let _span = tracing::debug_span!("uicore_docs.parse", doc = self.name).entered();
            return serde_json::from_str(self.source).expect("a factory document parses");
        }
        let mut compiled = COMPILED.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
        compiled
            .entry(self.name)
            .or_insert_with(|| {
                let _span = tracing::debug_span!("uicore_docs.parse", doc = self.name).entered();
                ede_ui::compile_document(self.name, self.source).unwrap_or_else(|issues| panic!("{}: {issues:?}", self.name))
            })
            .clone()
    }
}

/// The document called `name`.
pub fn doc(name: &str) -> Option<&'static Doc> {
    DOCS.iter().find(|d| d.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_factory_file_is_listed_and_parses() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../ede/ui");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "express"))
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let listed: Vec<&str> = DOCS.iter().map(|d| d.name).collect();
        assert_eq!(on_disk, listed, "apps/ede/ui and DOCS name the same documents");
        for d in DOCS {
            let v = d.json();
            assert!(v.get("surface").is_some(), "{}: a shell document has a surface header", d.name);
            assert!(v.get("root").is_some() || v.get("components").is_some(), "{}: a body", d.name);
        }
    }

    #[test]
    fn the_tabbar_is_a_document() {
        let v = TABBAR.json();
        assert!(v.get("surface").is_some() && v.get("root").is_some());
    }

    /// #EDE-60: a layer surface floats over windows, so its root must be
    /// opaque in the dark theme (`$color.card` is a white glaze at 6%).
    #[test]
    fn layer_documents_have_an_opaque_root() {
        for theme in [eui_core::themes::builtin_theme("exponential").expect("the exponential theme"), factory_theme()] {
        for d in DOCS {
            let v = d.json();
            if v["surface"].get("layer").is_none() && v["surface"].get("popup").is_some() {
                continue;
            }
            let Some(bg) = v["root"]["style"].get("backgroundColor") else {
                assert_eq!(d.name, "notifications", "{}: a layer document's root paints a background", d.name);
                continue;
            };
            let resolved = eui_core::theme::resolve_token(&theme, bg, eui_core::theme::Mode::Dark).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| bg.to_string());
            let hex = resolved.trim_start_matches('#');
            assert!(hex.len() == 6 || (hex.len() == 8 && hex.ends_with("ff")), "{}: root background {bg} resolves to {resolved} in {}, not opaque", d.name, theme.id);
        }
        }
    }

    /// #EDE-235: the factory theme is the Rerusted palette over the
    /// exponential theme, with eDE's spacing scale.
    #[test]
    fn the_factory_theme_is_rerusted_with_room() {
        let theme = factory_theme();
        let dark = &theme.modes.dark.color;
        let base = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
        assert_eq!(dark.get("success"), base.modes.dark.color.get("success"), "the rest is the exponential theme's");
        assert_eq!(dark.get("background").map(String::as_str), Some("#0f0807"));
        assert_eq!(dark.get("accent"), dark.get("muted"), "eUI's accent is the hover surface");
        assert_eq!(dark.get("ring").map(String::as_str), Some("#ba4f1f"));
        let spacing = theme.tokens.numbers("spacing").expect("spacing");
        assert_eq!((spacing["xs"], spacing["sm"], spacing["md"], spacing["lg"]), (6.0, 10.0, 16.0, 20.0));
        let control = theme.tokens.numbers("control").expect("control");
        assert_eq!((control["row"], control["input"], control["buttonMd"]), (40.0, 38.0, 34.0));
    }
}
