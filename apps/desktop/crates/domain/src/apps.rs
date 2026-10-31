//! VAPP-110 — Exponential apps, the Rust twin of
//! `packages/domain-contract/src/apps.ts` (iOS `Apps.swift`, Android
//! `domain/Apps.kt`), byte-locked by `fixtures/apps.json` (`slugifyCases`,
//! `versionLabelCases`, `sortAppsCases`, `editorTabCases`, `mediaPolicy.cases`). The copy, the
//! pane ratio and the preview frame come FROM the fixture ([`APPS`]); the
//! caps from the contract (`apps`).

use std::sync::LazyLock;

use serde::Deserialize;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization as _;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsMenuCopy {
    pub versions: String,
    pub settings: String,
    pub export: String,
    pub delete: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppsTabsCopy {
    pub chat: String,
    pub preview: String,
    pub versions: String,
    /// VAPP-177: the editor's Settings tab.
    pub settings: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsCopy {
    pub nav: String,
    pub title: String,
    pub new_app: String,
    /// VAPP-176: a new draft's name.
    pub untitled: String,
    /// VAPP-176: the list band's Import pill (before New app).
    pub import: String,
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
    /// VAPP-176: the versions drawer with no version yet.
    pub versions_empty: String,
    pub composer_placeholder: String,
    /// VAPP-176: a draft's composer placeholder.
    pub draft_placeholder: String,
    /// VAPP-176: a draft's empty chat (page-size EmptyState).
    pub starter_title: String,
    /// VAPP-176: one action Pill each under [`Self::starter_title`]; a tap
    /// sends the text as the first message.
    pub starters: Vec<String>,
    pub no_device: String,
    pub name_placeholder: String,
    pub description_placeholder: String,
    /// `Delete {name}?`.
    pub delete_title: String,
    pub delete_body: String,
    pub delete: String,
    pub menu: AppsMenuCopy,
    pub tabs: AppsTabsCopy,
    /// Settings → Apps' title.
    pub settings_title: String,
    /// VAPP-122 (the settings page's records line): `{n} records`.
    pub records: String,
    pub records_one: String,
    /// `{n} of {max} records`.
    pub records_of: String,
    pub records_none: String,
    /// VAPP-177: a version row's ghost button (show that version).
    pub preview: String,
}

/// The editor's two panes (fr) and the chat's floor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsPanes {
    pub chat: f32,
    pub preview: f32,
    pub chat_min_width: f32,
    pub breakpoint: f32,
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
    /// VAPP-176: the list band's Import pill.
    pub import_app: String,
    /// VAPP-176: a draft's starter pills.
    pub starter: String,
    /// VAPP-177: the phones' `…` menu.
    pub menu: String,
    /// VAPP-177: the Settings tab.
    pub settings: String,
    pub version_preview: String,
    pub version_restore: String,
}

/// `fixtures/apps.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsSpec {
    pub copy: AppsCopy,
    pub panes: AppsPanes,
    pub preview: AppsPreview,
    pub test_ids: AppsTestIds,
    pub editor_tabs: AppsEditorTabs,
}

/// VAPP-177: the editor's tabs (web ≥ md and desktop: `wide`; phones:
/// `phone` + the `…` menu's sheets).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsEditorTabs {
    pub wide: Vec<String>,
    pub phone: Vec<String>,
    pub menu: Vec<String>,
    pub default: String,
    pub phone_default: String,
    pub settings_max_width: u32,
    pub search: String,
}

/// VAPP-177: one editor tab (web `AppEditorTab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    Chat,
    Preview,
    Versions,
    Settings,
}

impl EditorTab {
    pub fn as_str(self) -> &'static str {
        match self {
            EditorTab::Chat => "chat",
            EditorTab::Preview => "preview",
            EditorTab::Versions => "versions",
            EditorTab::Settings => "settings",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "chat" => Some(EditorTab::Chat),
            "preview" => Some(EditorTab::Preview),
            "versions" => Some(EditorTab::Versions),
            "settings" => Some(EditorTab::Settings),
            _ => None,
        }
    }

    /// The tab's label (`copy.tabs.*`).
    pub fn label(self) -> String {
        let tabs = &APPS.copy.tabs;
        match self {
            EditorTab::Chat => tabs.chat.clone(),
            EditorTab::Preview => tabs.preview.clone(),
            EditorTab::Versions => tabs.versions.clone(),
            EditorTab::Settings => tabs.settings.clone(),
        }
    }

    /// The wide tabs in order (`editorTabs.wide`).
    pub fn wide() -> Vec<EditorTab> {
        APPS.editor_tabs.wide.iter().filter_map(|id| EditorTab::parse(id)).collect()
    }
}

/// A requested tab (the desktop settings nav, web `?tab=`) → the tab on
/// screen and, on phones, the sheet over it (web `editorTab`,
/// `editorTabCases`). The sheet is only ever Versions or Settings.
pub fn editor_tab(requested: Option<&str>, phone: bool) -> (EditorTab, Option<EditorTab>) {
    let tabs = &APPS.editor_tabs;
    let parse = |id: &str| EditorTab::parse(id).expect("editorTabs ids are tabs");
    if phone {
        if let Some(sheet @ ("versions" | "settings")) = requested {
            return (parse(&tabs.phone_default), Some(parse(sheet)));
        }
        let tab = requested.filter(|r| tabs.phone.iter().any(|t| t == r)).unwrap_or(&tabs.phone_default);
        return (parse(tab), None);
    }
    let tab = requested.filter(|r| tabs.wide.iter().any(|t| t == r)).unwrap_or(&tabs.default);
    (parse(tab), None)
}

/// VAPP-176: a DRAFT = an app with no version yet (web `isDraftApp`).
pub fn is_draft(head_version: i64) -> bool {
    head_version == 0
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
    /// VAPP-122: a record kind's name.
    pub kind_pattern: String,
    /// VAPP-122: one record's `data` JSON, in bytes.
    pub record_max_bytes: usize,
    /// VAPP-122: the kinds one manifest may declare.
    pub record_kinds_max: usize,
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

/// VAPP-157: the fixture's `mediaPolicy` (the app hosts' media rule).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMediaPolicy {
    pub data_mime_prefixes: Vec<String>,
    pub denied_data_mimes: Vec<String>,
    pub attachment_path: String,
}

pub static APP_MEDIA_POLICY: LazyLock<AppMediaPolicy> = LazyLock::new(|| {
    let fixture: serde_json::Value = serde_json::from_str(APPS_JSON).expect("apps.json parses");
    serde_json::from_value(fixture["mediaPolicy"].clone()).expect("apps.json mediaPolicy parses")
});

/// JS `String.prototype.trim`'s whitespace (Rust's plus the BOM; Rust's
/// extra NEL only rejects more).
fn js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// `8-4-4-4-12` ASCII hex digits, any case (the TS reference's `/i` regex).
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// VAPP-157: the app hosts' media rule ×4 (`appMediaAllowed(src, origin)`,
/// a plain string rule): a `data:` image, audio or video (never svg), or
/// the instance's own `/api/attachments/<uuid>` (relative, or on `origin`
/// exactly) with nothing after the id. Applied to the RAW src, before any
/// resolution; a host narrows its media policy with it, never widens.
pub fn app_media_allowed(src: &str, origin: &str) -> bool {
    let policy = &*APP_MEDIA_POLICY;
    if src.is_empty() || src.trim_matches(js_space) != src {
        return false;
    }
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("data:") {
        let Some(comma) = lower.find(',') else { return false };
        let mime = lower[5..comma].split(';').next().unwrap_or("");
        return policy.data_mime_prefixes.iter().any(|p| mime.starts_with(p.as_str()) && mime.len() > p.len())
            && !policy.denied_data_mimes.iter().any(|d| d == mime);
    }
    let path = if src.starts_with('/') && !src.starts_with("//") {
        src
    } else {
        let base = origin.trim_end_matches('/').to_lowercase();
        if base.is_empty() || !lower.starts_with(&format!("{base}/")) {
            return false;
        }
        &src[base.len()..]
    };
    path.strip_prefix(policy.attachment_path.as_str()).is_some_and(is_uuid)
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
        assert_eq!(APPS_CAPS.kind_pattern, "^[a-z][a-z0-9_]{0,31}$");
        assert_eq!(APPS_CAPS.record_max_bytes, 16384);
        assert_eq!(APPS_CAPS.record_kinds_max, 16);
        assert_eq!(APPS.copy.records_of, "{n} of {max} records");
        assert_eq!(delete_title("Team dashboard"), "Delete Team dashboard?");
        assert_eq!(viewing_label(2), "Viewing v2");
        assert_eq!(restored_label(1, 4), "Restored v1 as v4");
    }

    #[test]
    fn editor_tab_replays_the_fixture() {
        let all = cases("editorTabCases");
        assert!(!all.is_empty());
        for case in all {
            let (tab, sheet) = editor_tab(case["requested"].as_str(), case["phone"].as_bool().unwrap());
            assert_eq!(tab.as_str(), case["tab"].as_str().unwrap(), "{case}");
            assert_eq!(sheet.map(EditorTab::as_str), case["sheet"].as_str(), "{case}");
        }
        assert_eq!(EditorTab::wide(), vec![EditorTab::Preview, EditorTab::Versions, EditorTab::Settings]);
        assert_eq!(APPS.editor_tabs.settings_max_width, 640);
        assert!(is_draft(0) && !is_draft(1));
        assert_eq!(APPS.copy.untitled, "Untitled app");
        assert_eq!(APPS.copy.starters.len(), 4);
        assert_eq!(APPS.test_ids.import_app, "apps-import");
    }

    #[test]
    fn copy_and_test_ids_match_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        let copy = &APPS.copy;
        assert_eq!(fixture["copy"]["tabs"]["settings"], copy.tabs.settings.as_str());
        assert_eq!(fixture["copy"]["preview"], copy.preview.as_str());
        for (key, value) in [
            ("versions", &copy.menu.versions),
            ("settings", &copy.menu.settings),
            ("export", &copy.menu.export),
            ("delete", &copy.menu.delete),
        ] {
            assert_eq!(fixture["copy"]["menu"][key], value.as_str(), "{key}");
        }
        let ids = &APPS.test_ids;
        for (key, value) in [
            ("tabs", &ids.tabs),
            ("menu", &ids.menu),
            ("settings", &ids.settings),
            ("versions", &ids.versions),
            ("versionRow", &ids.version_row),
            ("versionPreview", &ids.version_preview),
            ("versionRestore", &ids.version_restore),
        ] {
            assert_eq!(fixture["testIds"][key], value.as_str(), "{key}");
        }
        // VAPP-177: the floating layout is gone from the fixture.
        assert!(fixture.get("layout").is_none() && fixture.get("dockCard").is_none());
        assert!(fixture["panes"].get("headerHeight").is_none());
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

    #[test]
    fn app_media_allowed_replays_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        let policy = &fixture["mediaPolicy"];
        let origin = policy["origin"].as_str().unwrap();
        let cases = policy["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let src = case["src"].as_str().unwrap();
            assert_eq!(app_media_allowed(src, origin), case["expected"].as_bool().unwrap(), "{src:?}");
        }
        // A trailing slash on the origin changes nothing; no origin = relative only.
        let id = "/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90";
        assert!(app_media_allowed(&format!("{origin}{id}"), &format!("{origin}/")));
        assert!(app_media_allowed(id, ""));
        assert!(!app_media_allowed(&format!("{origin}{id}"), ""));
        assert!(!app_media_allowed(&format!("{id}\u{feff}"), origin));
    }
}
