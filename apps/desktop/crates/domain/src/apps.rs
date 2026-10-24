//! VAPP-110 — Exponential apps, the Rust twin of
//! `packages/domain-contract/src/apps.ts` (iOS `Apps.swift`, Android
//! `domain/Apps.kt`), byte-locked by `fixtures/apps.json` (`slugifyCases`,
//! `versionLabelCases`, `sortAppsCases`). The copy, the
//! pane ratio and the preview frame come FROM the fixture ([`APPS`]); the
//! caps from the contract (`apps`).

use std::sync::LazyLock;

use serde::Deserialize;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization as _;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsMenuCopy {
    pub rename: String,
    pub delete: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppsTabsCopy {
    pub chat: String,
    pub preview: String,
    pub versions: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsCopy {
    pub nav: String,
    pub title: String,
    pub new_app: String,
    /// `v{n}`.
    pub version: String,
    pub no_version: String,
    pub versions: String,
    pub head: String,
    pub no_note: String,
    pub by_agent: String,
    /// `Viewing v{n}`.
    pub viewing: String,
    pub restore: String,
    /// `Restored v{n} as v{m}`.
    pub restored: String,
    pub restored_note: String,
    pub empty: String,
    pub chat_empty: String,
    pub preview_empty: String,
    pub composer_placeholder: String,
    pub no_device: String,
    pub name_placeholder: String,
    pub description_placeholder: String,
    pub create: String,
    pub rename: String,
    /// `Delete {name}?`.
    pub delete_title: String,
    pub delete_body: String,
    pub delete: String,
    pub menu: AppsMenuCopy,
    pub tabs: AppsTabsCopy,
    /// The app settings page's button to the editor.
    pub open_editor: String,
    /// Settings → Apps' title.
    pub settings_title: String,
    /// VAPP-129: the floating chat's dock-to-side button.
    pub dock_to_side: String,
    /// VAPP-129: the docked chat's float-back button.
    pub float: String,
    /// VAPP-129: the chat card's caption while the session works.
    pub working: String,
}

/// The editor's two panes (fr) and the chat's floor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsPanes {
    pub chat: f32,
    pub preview: f32,
    pub chat_min_width: f32,
    pub breakpoint: f32,
    /// VAPP-129: the docked chat pane's header strip.
    pub header_height: f32,
}

/// The preview frame.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsPreview {
    pub radius: f32,
    pub padding: f32,
    pub surface_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsTestIds {
    pub list: String,
    pub row: String,
    pub new_app: String,
    pub editor: String,
    pub chat: String,
    pub preview: String,
    pub versions: String,
    pub version_row: String,
    pub tabs: String,
    pub composer: String,
    pub viewing: String,
    pub chat_card: String,
    pub dock_to_side: String,
    pub float: String,
}

/// `fixtures/apps.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsSpec {
    pub copy: AppsCopy,
    pub panes: AppsPanes,
    pub preview: AppsPreview,
    pub test_ids: AppsTestIds,
    pub layout: AppsLayout,
    pub dock_card: AppsDockCard,
}

/// VAPP-129: the editor's layouts on web ≥ md and the IDE.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsLayout {
    pub default: String,
    pub values: Vec<String>,
    pub storage_key: String,
}

/// VAPP-129: the floating chat's ONE DockCard.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsDockCard {
    pub id: String,
    pub kind: String,
    pub tint: String,
    pub icon: String,
    pub dock_icon: String,
    pub float_icon: String,
}

/// VAPP-129: the editor's layout (web `AppEditorLayout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorLayout {
    Floating,
    Docked,
}

impl EditorLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            EditorLayout::Floating => "floating",
            EditorLayout::Docked => "docked",
        }
    }
}

/// A stored layout, else the fixture's default (web `editorLayout`).
pub fn editor_layout(stored: Option<&str>) -> EditorLayout {
    match stored.unwrap_or(APPS.layout.default.as_str()) {
        "docked" => EditorLayout::Docked,
        "floating" => EditorLayout::Floating,
        _ if APPS.layout.default == "docked" => EditorLayout::Docked,
        _ => EditorLayout::Floating,
    }
}

/// The floating editor's ONE DockCard (web `chatCard`): the newest message's
/// first plain line (else the empty chat copy), `Working` while the editor
/// session is busy. `messages` = the bodies, oldest first. It opens (the
/// chevron), so no action word: "Move to the side" is the sheet's.
pub fn chat_card(messages: &[&str], busy: bool) -> crate::dock::DockRow {
    let title = messages
        .iter()
        .rev()
        .map(|body| crate::ask_dock::ask_dock_title(body))
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| APPS.copy.chat_empty.clone());
    crate::dock::DockRow {
        id: APPS.dock_card.id.clone(),
        kind: crate::dock::DockKind::Chat,
        tint: crate::dock::DockTint::Muted,
        icon: APPS.dock_card.icon.clone(),
        label: APPS.copy.tabs.chat.clone(),
        caption: busy.then(|| APPS.copy.working.clone()),
        meta: None,
        title,
        expandable: true,
        action: None,
    }
}

const APPS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/apps.json");
const CONTRACT_JSON: &str = include_str!("../../../../../packages/domain-contract/contract.json");

/// The apps fixture: copy, built-in app, panes, preview frame, test ids.
pub static APPS: LazyLock<AppsSpec> =
    LazyLock::new(|| serde_json::from_str(APPS_JSON).expect("apps.json parses"));

/// Contract `apps` (the caps).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsCaps {
    pub slug_max: usize,
    pub name_max: usize,
    pub description_max: usize,
    pub note_max: usize,
    pub package_max_bytes: usize,
    pub template_max_components: usize,
}

pub static APPS_CAPS: LazyLock<AppsCaps> = LazyLock::new(|| {
    let contract: serde_json::Value = serde_json::from_str(CONTRACT_JSON).expect("contract.json parses");
    serde_json::from_value(contract["apps"].clone()).expect("contract apps parses")
});

/// `v{n}`, or the "no version" copy while an app has no head.
pub fn version_label(head_version: i64) -> String {
    let copy = &APPS.copy;
    if head_version > 0 {
        copy.version.replace("{n}", &head_version.to_string())
    } else {
        copy.no_version.clone()
    }
}

/// `Viewing v{n}`.
pub fn viewing_label(number: i64) -> String {
    APPS.copy.viewing.replace("{n}", &number.to_string())
}

/// `Restored v{n} as v{m}` (the restore toast).
pub fn restored_label(restored: i64, version: i64) -> String {
    APPS.copy
        .restored
        .replace("{n}", &restored.to_string())
        .replace("{m}", &version.to_string())
}

/// `Delete {name}?`.
pub fn delete_title(name: &str) -> String {
    APPS.copy.delete_title.replace("{name}", name)
}

/// Diacritics off (NFKD, combining marks dropped).
fn fold(text: &str) -> String {
    text.nfkd().filter(|c| !is_combining_mark(*c)).collect()
}

/// Cut at `max` bytes (the slug is ASCII) and drop trailing dashes.
fn cut(slug: &str, max: usize) -> String {
    slug[..slug.len().min(max)].trim_end_matches('-').to_string()
}

/// A name → its slug: lowercase ASCII letters, digits and single dashes,
/// diacritics stripped, cut at the cap, `app` when nothing is left; a taken
/// slug gets `-2`, `-3`, …
pub fn slugify(name: &str, taken: &[&str]) -> String {
    let max = APPS_CAPS.slug_max;
    let mut dashed = String::new();
    for c in fold(name).to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            dashed.push(c);
        } else if !dashed.ends_with('-') {
            dashed.push('-');
        }
    }
    let mut base = cut(dashed.trim_matches('-'), max);
    if base.is_empty() {
        base = "app".to_string();
    }
    if !taken.contains(&base.as_str()) {
        return base;
    }
    (2..)
        .map(|n| {
            let suffix = format!("-{n}");
            format!("{}{suffix}", cut(&base, max - suffix.len()))
        })
        .find(|candidate| !taken.contains(&candidate.as_str()))
        .expect("an unbounded range finds a free slug")
}

/// What [`sort_apps`] orders.
pub trait AppListLike {
    fn name(&self) -> &str;
}

/// The list order: by name (case- and accent-insensitive, the web's
/// `localeCompare` at `base`), stable. Exponential ships no built-in apps.
pub fn sort_apps<T: AppListLike>(apps: &mut [T]) {
    apps.sort_by(|a, b| fold(a.name()).to_lowercase().cmp(&fold(b.name()).to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases(key: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        fixture[key].as_array().unwrap().clone()
    }

    #[test]
    fn the_fixture_parses() {
        assert_eq!(APPS.copy.nav, "Apps");
        assert_eq!(APPS.preview.surface_id, "app");
        assert_eq!(APPS_CAPS.slug_max, 64);
        assert_eq!(delete_title("Team dashboard"), "Delete Team dashboard?");
        assert_eq!(viewing_label(2), "Viewing v2");
        assert_eq!(restored_label(1, 4), "Restored v1 as v4");
    }

    #[test]
    fn chat_card_replays_the_fixture() {
        for case in cases("chatCardCases") {
            let messages: Vec<&str> = case["messages"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            let row = chat_card(&messages, case["busy"].as_bool().unwrap());
            assert_eq!(row.title, case["title"].as_str().unwrap());
            assert_eq!(row.caption.as_deref(), case["caption"].as_str());
            assert_eq!(row.id, APPS.dock_card.id);
            assert_eq!(row.kind, crate::dock::DockKind::Chat);
            assert!(row.expandable && row.action.is_none());
        }
        assert_eq!(editor_layout(None).as_str(), APPS.layout.default);
        assert_eq!(editor_layout(Some("docked")), EditorLayout::Docked);
        assert_eq!(editor_layout(Some("sideways")).as_str(), APPS.layout.default);
    }

    #[test]
    fn slugify_replays_the_fixture() {
        for case in cases("slugifyCases") {
            let taken: Vec<&str> = case["taken"]
                .as_array()
                .map(|t| t.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let name = case["name"].as_str().unwrap();
            assert_eq!(slugify(name, &taken), case["slug"].as_str().unwrap(), "{name}");
        }
    }

    #[test]
    fn version_label_replays_the_fixture() {
        for case in cases("versionLabelCases") {
            let head = case["headVersion"].as_i64().unwrap();
            assert_eq!(version_label(head), case["label"].as_str().unwrap());
        }
    }

    struct Row {
        slug: String,
        name: String,
    }

    impl AppListLike for Row {
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn sort_apps_replays_the_fixture() {
        for case in cases("sortAppsCases") {
            let mut rows: Vec<Row> = case["apps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| Row {
                    slug: a["slug"].as_str().unwrap().into(),
                    name: a["name"].as_str().unwrap().into(),
                })
                .collect();
            sort_apps(&mut rows);
            let order: Vec<&str> = rows.iter().map(|r| r.slug.as_str()).collect();
            let want: Vec<&str> = case["order"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            assert_eq!(order, want);
        }
    }
}
