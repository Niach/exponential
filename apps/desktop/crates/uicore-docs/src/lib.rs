//! `uicore-docs` (#EDE-50): the shell's factory genUI documents, the files
//! in `apps/ede/ui` compiled in. Exponential Desktop installs that directory
//! as the factory copy `exponential-ui` serves (people and agents edit their
//! own copies, #EDE-9); the IDE embeds the same documents when it hosts the
//! shell. One source, two hosts: a document that only one host can draw is
//! a bug here, not a fork there.

use serde_json::Value;

/// One factory document: its name (the file stem) and its JSON source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Doc {
    pub name: &'static str,
    pub source: &'static str,
}

macro_rules! docs {
    ($($name:literal),* $(,)?) => {
        /// Every factory document, in file-name order.
        pub const DOCS: &[Doc] = &[$(Doc { name: $name, source: include_str!(concat!("../../../../ede/ui/", $name, ".json")) }),*];
    };
}

docs!["applet-battery", "applet-bluetooth", "applet-calendar", "applet-network", "applet-sound", "chat", "connect", "expo", "ground", "history", "needs-you", "notification-center", "notifications", "osd", "scale", "settings", "sidebar", "start-menu", "switcher", "taskbar", "tray-menu"];

/// The tab bar only the IDE draws (eDE has Mint's taskbar).
pub const TABBAR: Doc = Doc { name: "tabbar", source: include_str!("../../../../../packages/uicore/docs/tabbar.json") };

impl Doc {
    /// The parsed document (the sources are checked by the tests).
    pub fn json(&self) -> Value {
        let _span = tracing::debug_span!("uicore_docs.parse", doc = self.name).entered();
        serde_json::from_str(self.source).expect("a factory document parses")
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
            .filter(|p| p.extension().is_some_and(|e| e == "json") && p.file_name().is_some_and(|n| n != "theme.json"))
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
        let theme = eui_core::themes::builtin_theme("exponential").expect("the exponential theme");
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
            assert!(hex.len() == 6 || (hex.len() == 8 && hex.ends_with("ff")), "{}: root background {bg} resolves to {resolved}, not opaque", d.name);
        }
    }
}
