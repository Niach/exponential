//! VAPP-110 — Exponential apps, the Rust twin of
//! `packages/domain-contract/src/apps.ts` (iOS `Apps.swift`, Android
//! `domain/Apps.kt`), byte-locked by `fixtures/apps.json` (`slugifyCases`,
//! `versionLabelCases`, `sortAppsCases`, `editorTabCases`, `mediaPolicy.cases`, `screenShellCases`,
//! `screenPathCases`, `screenQueryCases`). The copy, the
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
    /// VAPP-242: `Edit app` (the phones' `…` menus; THE EDITOR).
    pub edit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppsTabsCopy {
    pub chat: String,
    pub preview: String,
    /// VAPP-178: the editor's Data tab.
    pub data: String,
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
    /// eApps V1 (#VAPP-210): Publish, the published badge, its toast.
    pub publish: String,
    pub published: String,
    /// `Published v{n}`.
    pub published_toast: String,
    pub publish_nothing: String,
    /// eApps R8 (#VAPP-233, `migrationCases`): a version row's record sweep.
    pub migrating: String,
    /// `Migrating records {done} of {total}`.
    pub migrating_of: String,
    /// `Records not migrated: {error}`.
    pub migration_failed: String,
    /// #VAPP-256 (`rollbackCases`): `Roll back to v{n}?`.
    pub rollback_title: String,
    pub rollback_body: String,
    pub empty: String,
    pub chat_empty: String,
    pub preview_empty: String,
    /// VAPP-242: THE RUN VIEW of an app never published (page-size EmptyState).
    pub run_empty: String,
    pub needs_update: String,
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
    /// VAPP-179: the preview toolbar's frame segments.
    pub frame: AppsFrameCopy,
    /// VAPP-198: Run as.
    pub run_as: AppsRunAsCopy,
    /// #VAPP-220: the Settings tab's Automations group.
    pub automations: AppsAutomationsCopy,
    /// #VAPP-238: the Settings tab's Webhooks group.
    pub webhooks: AppsWebhooksCopy,
    /// #VAPP-243: the Settings tab's Record access group.
    pub policies: AppsPoliciesCopy,
}

/// #VAPP-243: `copy.policies.*` (`policyRowCases`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsPoliciesCopy {
    pub title: String,
    pub ops: std::collections::BTreeMap<String, String>,
    pub levels: std::collections::BTreeMap<String, String>,
}

/// #VAPP-238: `copy.webhooks.*`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsWebhooksCopy {
    pub title: String,
    pub ready: String,
    pub unsigned: String,
    pub copy_url: String,
    pub copied: String,
    pub mint: String,
    pub rotate: String,
    /// The shown-once secret row's label.
    pub secret: String,
    pub copy_secret: String,
}

/// #VAPP-220: `copy.automations.*`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsAutomationsCopy {
    pub title: String,
    /// `Schedule {cron}`.
    pub schedule: String,
    /// `Schedule {cron} ({tz})`.
    pub schedule_zone: String,
    /// `When a {kind} is created` (and updated, deleted).
    pub create: String,
    pub update: String,
    pub delete: String,
    /// `On {event}`.
    pub event: String,
    /// #VAPP-237: `On webhook {name}`.
    pub webhook: String,
    pub paused: String,
    pub failed: String,
    pub ok: String,
    pub never: String,
    /// #VAPP-243: `Runs as {name}`.
    pub runs_as: String,
    /// #VAPP-255: `{n} runs dropped in 24 h`.
    pub dropped: String,
    /// #VAPP-255: `1 run dropped in 24 h`.
    pub dropped_one: String,
    /// #VAPP-243: a pause's reason, by `sponsorStates` word.
    pub paused_reasons: std::collections::BTreeMap<String, String>,
}

/// VAPP-198: `copy.runAs.*`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsRunAsCopy {
    pub me: String,
    pub writes_off: String,
}

/// VAPP-198: one Run as option (`value` "" = me).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunAsOption {
    pub value: String,
    pub label: String,
}

/// VAPP-198: the preview toolbar's Run as options (`runAsCases`): `None`
/// unless the viewer is a team owner with another member; else Me, then
/// every other member by name (case-insensitive, then id; no name = id).
/// `members` = (user id, name, role).
pub fn run_as_options(members: &[(String, Option<String>, String)], me: Option<&str>, role: Option<&str>) -> Option<Vec<RunAsOption>> {
    if role != Some("owner") {
        return None;
    }
    let mut others: Vec<RunAsOption> = members
        .iter()
        .filter(|(id, _, _)| Some(id.as_str()) != me)
        .map(|(id, name, _)| RunAsOption {
            value: id.clone(),
            label: name.as_deref().filter(|n| !n.trim().is_empty()).unwrap_or(id).to_string(),
        })
        .collect();
    if others.is_empty() {
        return None;
    }
    others.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.value.cmp(&b.value)));
    let mut out = vec![RunAsOption { value: String::new(), label: APPS.copy.run_as.me.clone() }];
    out.extend(others);
    Some(out)
}

/// VAPP-179: `copy.frame.*`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppsFrameCopy {
    pub desktop: String,
    pub phone: String,
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
    /// VAPP-179: the toolbar's frames in order, the one it opens on and the
    /// Phone frame's width (px, centred).
    pub frames: Vec<String>,
    pub default_frame: String,
    pub phone_width: f32,
}

/// VAPP-179: the preview frame's width (`preview.frames`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewFrame {
    /// The pane's full width.
    Desktop,
    /// `preview.phoneWidth` px, centred.
    Phone,
}

impl PreviewFrame {
    pub fn as_str(self) -> &'static str {
        match self {
            PreviewFrame::Desktop => "desktop",
            PreviewFrame::Phone => "phone",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "desktop" => Some(PreviewFrame::Desktop),
            "phone" => Some(PreviewFrame::Phone),
            _ => None,
        }
    }

    /// The segment's label (`copy.frame.*`).
    pub fn label(self) -> String {
        let frame = &APPS.copy.frame;
        match self {
            PreviewFrame::Desktop => frame.desktop.clone(),
            PreviewFrame::Phone => frame.phone.clone(),
        }
    }

    /// The frames in order (`preview.frames`).
    pub fn all() -> Vec<PreviewFrame> {
        APPS.preview.frames.iter().filter_map(|id| PreviewFrame::parse(id)).collect()
    }

    /// The frame the toolbar opens on (`preview.defaultFrame`).
    pub fn default_frame() -> PreviewFrame {
        PreviewFrame::parse(&APPS.preview.default_frame).expect("defaultFrame is a frame")
    }

    /// The frame's width in px; `None` = the pane's full width.
    pub fn width(self) -> Option<f32> {
        match self {
            PreviewFrame::Desktop => None,
            PreviewFrame::Phone => Some(APPS.preview.phone_width),
        }
    }
}

/// VAPP-179: the template ids of a package, sorted (web `previewTemplates`;
/// a host's JSON layer does not keep the declared order).
pub fn preview_templates(pkg: Option<&serde_json::Value>) -> Vec<String> {
    let mut ids: Vec<String> = pkg
        .and_then(|pkg| pkg.get("templates"))
        .and_then(serde_json::Value::as_object)
        .map(|templates| templates.keys().cloned().collect())
        .unwrap_or_default();
    ids.sort();
    ids
}

/// VAPP-179: the template the preview shows: the picked one while the
/// package has it, else the first (web `previewTemplate`, `templateCases`).
pub fn preview_template(pkg: Option<&serde_json::Value>, picked: Option<&str>) -> Option<String> {
    let list = preview_templates(pkg);
    match picked {
        Some(picked) if list.iter().any(|id| id == picked) => Some(picked.to_string()),
        _ => list.into_iter().next(),
    }
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
    /// eApps V1: the header's (phones: the menu's) Publish, a row's Publish.
    pub publish: String,
    pub version_publish: String,
    /// #VAPP-254: one Accounts row (its `-picker` Select), the import step's
    /// Accounts group and one of its rows.
    pub account_row: String,
    pub import_upstreams: String,
    pub import_upstream: String,
    /// #VAPP-220: the Settings tab's Automations group and one row.
    pub automations: String,
    pub automation_row: String,
    /// #VAPP-238: the Webhooks group, one row, the shown-once secret row.
    pub webhooks: String,
    pub webhook_row: String,
    pub webhook_secret: String,
    /// #VAPP-243: the Record access group and one row.
    pub policies: String,
    pub policy_row: String,
    /// VAPP-178: the Data tab.
    pub data: String,
    /// VAPP-179: the preview toolbar, its frame control and template picker.
    pub preview_toolbar: String,
    pub preview_frame: String,
    pub preview_template: String,
    /// VAPP-198: Run as (owners only).
    pub preview_run_as: String,
    /// VAPP-242: THE RUN VIEW, its phones' `…` menu, THE NEW MENU.
    pub run: String,
    pub run_menu: String,
    pub new_menu: String,
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
    /// VAPP-218: the screens shell (`screens`).
    pub screens: AppsScreens,
    /// VAPP-242: THE RUN VIEW's menu and THE NEW MENU (`run`).
    pub run: AppsRun,
}

/// VAPP-242 (`run`): the menus' item keys in order and the editor's route.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsRun {
    /// THE RUN VIEW's `…` menu (phones): `copy.menu.<key>` in order.
    pub menu: Vec<String>,
    /// THE NEW MENU: `copy.<key>` in order (`newApp`, `import`).
    pub new_menu: Vec<String>,
    /// The editor's web route.
    pub editor_path: String,
}

/// VAPP-218 (eApps U1): the screens contract (`screens.$comment`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsScreens {
    /// The sidebar column's width (px) in the wide frame.
    pub sidebar_width: f32,
    /// The editor's own query keys, never screen params.
    pub reserved: Vec<String>,
}

/// VAPP-218: the navigation the preview frame paints for a package's screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenShell {
    /// The wide frame: `sidebar-rows` in a `sidebarWidth` column.
    Sidebar,
    /// A phone or the Phone frame: the `phone-tab-bar` under the surface.
    Bar,
}

impl ScreenShell {
    pub fn as_str(self) -> &'static str {
        match self {
            ScreenShell::Sidebar => "sidebar",
            ScreenShell::Bar => "bar",
        }
    }
}

/// VAPP-218: none without screens, the bar on a phone (or the Phone frame),
/// else the sidebar (web `screenShell`, `screenShellCases`).
pub fn screen_shell(screen_count: usize, phone: bool) -> Option<ScreenShell> {
    if screen_count == 0 {
        return None;
    }
    Some(if phone { ScreenShell::Bar } else { ScreenShell::Sidebar })
}

/// VAPP-218: the web deep link to an app screen, `/t/<team>/apps/<app>/<screen>`
/// then the params sorted by key (web `screenPath`, `screenPathCases`).
pub fn screen_path(
    team_slug: &str,
    app_slug: &str,
    screen: &str,
    params: &std::collections::BTreeMap<String, String>,
) -> String {
    let path = format!(
        "/t/{}/apps/{}/{}",
        encode_uri_component(team_slug),
        encode_uri_component(app_slug),
        encode_uri_component(screen)
    );
    if params.is_empty() {
        return path;
    }
    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", encode_uri_component(k), encode_uri_component(v)))
        .collect();
    format!("{path}?{}", query.join("&"))
}

/// VAPP-218: a screen route's query → its params: every string key but the
/// editor's own (`screens.reserved`), sorted (web `screenParamsFromQuery`,
/// `screenQueryCases`).
pub fn screen_params_from_query(
    query: &serde_json::Map<String, serde_json::Value>,
) -> std::collections::BTreeMap<String, String> {
    query
        .iter()
        .filter(|(k, _)| !APPS.screens.reserved.iter().any(|r| r == *k))
        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
        .collect()
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
    /// VAPP-178: the app's records.
    Data,
    Versions,
    Settings,
}

impl EditorTab {
    pub fn as_str(self) -> &'static str {
        match self {
            EditorTab::Chat => "chat",
            EditorTab::Preview => "preview",
            EditorTab::Data => "data",
            EditorTab::Versions => "versions",
            EditorTab::Settings => "settings",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "chat" => Some(EditorTab::Chat),
            "preview" => Some(EditorTab::Preview),
            "data" => Some(EditorTab::Data),
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
            EditorTab::Data => tabs.data.clone(),
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
/// #FEED-204 (fixture `editorDeviceCases`, EDITOR DEVICE): the device an app
/// editor's composer shows and sends on (`device_id`; `None` = none online
/// and no root to fall back on), `moves` when an ended root's next send goes
/// to another device. A live root keeps its device: only a pick moves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDeviceChoice {
    pub device_id: Option<String>,
    pub moves: bool,
}

/// `root` = the editor root's (status, device); `candidates` = the member's
/// online devices as (id, is default), with a root the targets a move accepts.
pub fn editor_device(
    root: Option<(&str, Option<&str>)>,
    candidates: &[(&str, bool)],
    remembered: Option<&str>,
) -> EditorDeviceChoice {
    if let Some((status, device_id)) = root {
        if status != crate::contract::SESSION_STATUS_ENDED {
            return EditorDeviceChoice { device_id: device_id.map(str::to_string), moves: false };
        }
    }
    let root_device = root.and_then(|(_, device_id)| device_id);
    let known = |id: &&str| candidates.iter().any(|(candidate, _)| candidate == id);
    let device_id = remembered
        .filter(known)
        .or(root_device.filter(known))
        .or(candidates.iter().find(|(_, is_default)| *is_default).map(|(id, _)| *id))
        .or(candidates.first().map(|(id, _)| *id))
        .or(root_device);
    EditorDeviceChoice {
        device_id: device_id.map(str::to_string),
        moves: root.is_some() && device_id != root_device,
    }
}

/// #VAPP-251 (fixture `surfaceStateCases`): what an app's pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceState {
    Empty,
    Surface,
    NeedsUpdate,
}

/// Nothing to run, the surface, or once the host had its chance and holds no
/// surface (it refused the package), `copy.needsUpdate`.
pub fn surface_state(has_package: bool, applied: bool, has_surface: bool) -> SurfaceState {
    if !has_package {
        SurfaceState::Empty
    } else if applied && !has_surface {
        SurfaceState::NeedsUpdate
    } else {
        SurfaceState::Surface
    }
}

/// eApps V1 (#VAPP-210, fixture `publishCases`): an app's two pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppVersions {
    pub head: i64,
    pub published: i64,
}

/// The version the editor's preview shows (web `shownVersion`): an
/// explicitly viewed one, else nothing (0), else the HEAD while the viewer's
/// own editor session is live or nothing is published, else the PUBLISHED.
pub fn shown_version(app: AppVersions, editing: bool, viewing: Option<i64>) -> i64 {
    if let Some(viewing) = viewing {
        return viewing;
    }
    if app.head == 0 {
        return 0;
    }
    if editing || app.published == 0 {
        app.head
    } else {
        app.published
    }
}

/// Publish is offered while the head is not the published version.
pub fn can_publish(app: AppVersions) -> bool {
    app.head > 0 && app.head != app.published
}

/// The list's and the header's version pill: the PUBLISHED version.
pub fn list_pill(app: AppVersions) -> String {
    version_label(app.published)
}

/// The version a grant is made on (`apps.update {headVersion}` carries it).
pub fn grant_version(app: AppVersions) -> i64 {
    if app.published > 0 {
        app.published
    } else {
        app.head
    }
}

/// A Versions row's trailing (web `versionRow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionRowKind {
    Published,
    Head,
    Version,
}

impl VersionRowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Head => "head",
            Self::Version => "version",
        }
    }
}

/// eApps R8 (#VAPP-233, fixture `migrationCases`): a version row's sweep
/// line after its subtitle, or `None` (no sweep, done, superseded).
pub fn version_migration(state: Option<&str>, total: i64, done: i64, error: Option<&str>) -> Option<String> {
    match state {
        Some("running") if total > 0 => Some(APPS.copy.migrating_of.replace("{done}", &done.to_string()).replace("{total}", &total.to_string())),
        Some("running") => Some(APPS.copy.migrating.clone()),
        Some("failed") => Some(APPS.copy.migration_failed.replace("{error}", error.unwrap_or(""))),
        _ => None,
    }
}

/// #VAPP-256 (fixture `rollbackCases`): whether publishing `target` rolls back
/// past a version whose sweep ran; `rows` = `(number, migrationState)`.
pub fn rollback_warns(published: i64, target: i64, rows: &[(i64, Option<&str>)]) -> bool {
    target < published && rows.iter().any(|(number, state)| *number > target && state.is_some())
}

pub fn rollback_title(n: i64) -> String {
    APPS.copy.rollback_title.replace("{n}", &n.to_string())
}

pub fn rollback_body(n: i64) -> String {
    APPS.copy.rollback_body.replace("{n}", &n.to_string())
}

pub fn version_row(app: AppVersions, number: i64) -> VersionRowKind {
    if number == app.published {
        VersionRowKind::Published
    } else if number == app.head {
        VersionRowKind::Head
    } else {
        VersionRowKind::Version
    }
}

/// `Published v{n}`.
pub fn published_label(number: i64) -> String {
    APPS.copy.published_toast.replace("{n}", &number.to_string())
}

pub fn restored_label(restored: i64, version: i64) -> String {
    APPS.copy
        .restored
        .replace("{n}", &restored.to_string())
        .replace("{m}", &version.to_string())
}

/// #VAPP-220: a schedule automation's trigger.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AutomationSchedule {
    pub cron: String,
    #[serde(default)]
    pub tz: Option<String>,
}

/// #VAPP-220: a record automation's trigger (`op` = create | update | delete).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AutomationRecord {
    pub kind: String,
    pub op: String,
}

/// #VAPP-237: a webhook automation's trigger.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AutomationWebhook {
    pub name: String,
}

/// #VAPP-220: an automation's `on` (`{schedule}` | `{record}` | `{event}` | `{webhook}`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutomationOn {
    Schedule(AutomationSchedule),
    Record(AutomationRecord),
    Event(String),
    Webhook(AutomationWebhook),
}

/// #VAPP-220: an automation's state on its Settings row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationStatus {
    Paused,
    Failed,
    Ok,
    Never,
}

impl AutomationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::Ok => "ok",
            Self::Never => "never",
        }
    }
}

/// #VAPP-220 (`automationRowCases`): one row of the Settings tab's
/// Automations group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationRowView {
    pub trigger: String,
    pub status: AutomationStatus,
    pub label: String,
}

/// #VAPP-220: the trigger line, the status and its label (the TS
/// `automationRow`).
pub fn automation_row(on: &AutomationOn, paused: Option<&str>, last_status: Option<&str>) -> AutomationRowView {
    let copy = &APPS.copy.automations;
    let trigger = match on {
        AutomationOn::Schedule(s) => match s.tz.as_deref().filter(|tz| !tz.is_empty()) {
            Some(tz) => copy.schedule_zone.replace("{cron}", &s.cron).replace("{tz}", tz),
            None => copy.schedule.replace("{cron}", &s.cron),
        },
        AutomationOn::Record(r) => {
            let text = match r.op.as_str() {
                "create" => &copy.create,
                "update" => &copy.update,
                _ => &copy.delete,
            };
            text.replace("{kind}", &r.kind)
        }
        AutomationOn::Event(event) => copy.event.replace("{event}", event),
        AutomationOn::Webhook(hook) => copy.webhook.replace("{name}", &hook.name),
    };
    let status = if paused.is_some_and(|p| !p.is_empty()) {
        AutomationStatus::Paused
    } else {
        match last_status {
            None => AutomationStatus::Never,
            Some("ok") => AutomationStatus::Ok,
            Some(_) => AutomationStatus::Failed,
        }
    };
    let label = match status {
        AutomationStatus::Paused => &copy.paused,
        AutomationStatus::Failed => &copy.failed,
        AutomationStatus::Ok => &copy.ok,
        AutomationStatus::Never => &copy.never,
    }
    .clone();
    AutomationRowView { trigger, status, label }
}

/// #VAPP-238: a webhook's state on its Settings row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookStatus {
    Ready,
    Unsigned,
}

impl WebhookStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Unsigned => "unsigned",
        }
    }
}

/// #VAPP-238: an owner's action pill on a webhook row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookAction {
    /// No secret yet: `appAccess.rotateHook` at once.
    Mint,
    /// Asks first (`rotate-webhook-secret`), then the same call.
    Rotate,
}

impl WebhookAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mint => "mint",
            Self::Rotate => "rotate",
        }
    }
}

/// #VAPP-238 (`webhookRowCases`): one row of the Settings tab's Webhooks
/// group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookRowView {
    pub status: WebhookStatus,
    pub label: String,
    pub action: Option<WebhookAction>,
    pub action_label: Option<String>,
}

/// #VAPP-238: the status, its label and the owner's action (the TS
/// `webhookRow`); `is_owner` = `appTools.canToggle`.
pub fn webhook_row(has_secret: bool, is_owner: bool) -> WebhookRowView {
    let copy = &APPS.copy.webhooks;
    let (status, label) = if has_secret {
        (WebhookStatus::Ready, &copy.ready)
    } else {
        (WebhookStatus::Unsigned, &copy.unsigned)
    };
    let action = is_owner.then_some(if has_secret { WebhookAction::Rotate } else { WebhookAction::Mint });
    let action_label = action.map(|a| match a {
        WebhookAction::Mint => copy.mint.clone(),
        WebhookAction::Rotate => copy.rotate.clone(),
    });
    WebhookRowView { status, label: label.clone(), action, action_label }
}

/// #VAPP-254 (`upstreamPickerCases`): an Accounts or import row's Select.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamPicker {
    pub value: String,
    /// `(value, label)` in order.
    pub options: Vec<(String, String)>,
}

/// The Select's value when no server fills the upstream.
pub const UPSTREAM_PICKER_NONE: &str = "none";

/// `servers` = `(id, name)` in name order; empty = no Select. The clear
/// option only when no server has exactly the upstream's name.
pub fn upstream_picker(name: &str, server_id: Option<&str>, servers: &[(String, String)], not_set_up: &str) -> Option<UpstreamPicker> {
    if servers.is_empty() {
        return None;
    }
    let mut options = Vec::with_capacity(servers.len() + 1);
    if !servers.iter().any(|(_, server)| server == name) {
        options.push((UPSTREAM_PICKER_NONE.to_string(), not_set_up.to_string()));
    }
    options.extend(servers.iter().cloned());
    Some(UpstreamPicker { value: server_id.unwrap_or(UPSTREAM_PICKER_NONE).to_string(), options })
}

/// The row's second line naming the real server, when it is not the name.
pub fn account_caption(name: &str, server_name: Option<&str>, uses_server: &str) -> Option<String> {
    server_name.filter(|server| *server != name).map(|server| uses_server.replace("{server}", server))
}

/// #VAPP-243 (`policyRowCases`): the record-policy ops in order.
pub const POLICY_OPS: [&str; 4] = ["read", "create", "update", "delete"];

/// #VAPP-243: a kind's EFFECTIVE policy (`levels` = the level of each of
/// [`POLICY_OPS`], in order) as the Record access row's caption.
pub fn policy_caption(levels: [&str; 4]) -> String {
    let copy = &APPS.copy.policies;
    POLICY_OPS
        .iter()
        .zip(levels)
        .map(|(op, level)| {
            let op = copy.ops.get(*op).map(String::as_str).unwrap_or(op);
            let level = copy.levels.get(level).map(String::as_str).unwrap_or(level);
            format!("{op}: {level}")
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// #VAPP-243: the declared kinds of a package's `records`, code-point order.
pub fn policy_kinds(records: Option<&serde_json::Value>) -> Vec<String> {
    let mut kinds: Vec<String> = records.and_then(|r| r.as_object()).map(|o| o.keys().cloned().collect()).unwrap_or_default();
    kinds.sort();
    kinds
}

/// #VAPP-243 (`automationSponsorCases`): who an automation runs as, or why
/// it is paused (a pause wins); `None` = the trigger line alone.
pub fn automation_sponsor(paused: Option<&str>, sponsor_name: Option<&str>) -> Option<String> {
    let copy = &APPS.copy.automations;
    if let Some(paused) = paused.filter(|p| !p.is_empty()) {
        return Some(copy.paused_reasons.get(paused).cloned().unwrap_or_else(|| copy.paused.clone()));
    }
    sponsor_name.filter(|n| !n.is_empty()).map(|name| copy.runs_as.replace("{name}", name))
}

/// #VAPP-255 (`automationDroppedCases`): the runs the rate cap dropped in the
/// last 24 h; `None` = nothing to say.
pub fn automation_dropped(n: i64) -> Option<String> {
    let copy = &APPS.copy.automations;
    match n {
        n if n <= 0 => None,
        1 => Some(copy.dropped_one.clone()),
        n => Some(copy.dropped.replace("{n}", &n.to_string())),
    }
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

/// VAPP-221 (eApps R7): the fixture's `appFiles` (the app hosts' FileUpload
/// uploads): every picked file POSTs to `upload_path` as one multipart part
/// `part_name`; the answer's `url` lands in the FileUpload's bound list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFiles {
    pub upload_path: String,
    pub part_name: String,
    pub failed_verb: String,
    pub failed_thing: String,
}

pub static APP_FILES: LazyLock<AppFiles> = LazyLock::new(|| {
    let fixture: serde_json::Value = serde_json::from_str(APPS_JSON).expect("apps.json parses");
    serde_json::from_value(fixture["appFiles"].clone()).expect("apps.json appFiles parses")
});

/// JS `encodeURIComponent`.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The upload url of an app (`uploadPath` with its id).
pub fn app_upload_path(app_id: &str) -> String {
    APP_FILES.upload_path.replace("{appId}", &encode_uri_component(app_id))
}

/// The absolute `files` path of the FIRST FileUpload (component order)
/// named `name` (its `name`, else its id); None when it has no absolute
/// `{path}` binding (`targetCases`).
pub fn app_upload_target(components: &[serde_json::Value], name: &str) -> Option<String> {
    let upload = components.iter().find(|c| {
        c["component"].as_str() == Some("FileUpload")
            && match c.get("name").and_then(serde_json::Value::as_str) {
                Some(n) => n == name,
                None => c["id"].as_str() == Some(name),
            }
    })?;
    let path = upload.get("files")?.as_object()?.get("path")?.as_str()?;
    path.starts_with('/').then(|| path.to_string())
}

/// `list` with `url` on every object entry named `file_name`; None = nothing
/// to write (no list, no such entry) (`urlCases`).
pub fn with_uploaded_url(list: Option<&serde_json::Value>, file_name: &str, url: &str) -> Option<Vec<serde_json::Value>> {
    let list = list?.as_array()?;
    let mut hit = false;
    let next = list
        .iter()
        .map(|entry| match entry.as_object() {
            Some(obj) if obj.get("name").and_then(serde_json::Value::as_str) == Some(file_name) => {
                hit = true;
                let mut obj = obj.clone();
                obj.insert("url".into(), serde_json::Value::String(url.into()));
                serde_json::Value::Object(obj)
            }
            _ => entry.clone(),
        })
        .collect();
    hit.then_some(next)
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
        assert_eq!(EditorTab::wide(), vec![EditorTab::Preview, EditorTab::Data, EditorTab::Versions, EditorTab::Settings]);
        assert_eq!(APPS.editor_tabs.settings_max_width, 640);
        assert!(is_draft(0) && !is_draft(1));
        assert_eq!(APPS.copy.untitled, "Untitled app");
        assert_eq!(APPS.copy.starters.len(), 4);
        assert_eq!(APPS.test_ids.import_app, "apps-import");
    }

    /// VAPP-242: THE RUN VIEW and THE NEW MENU read the fixture.
    #[test]
    fn run_view_and_new_menu_read_the_fixture() {
        assert_eq!(APPS.run.new_menu, ["newApp", "import"]);
        assert_eq!(APPS.run.menu, ["edit", "export", "delete"]);
        assert_eq!(APPS.test_ids.run, "app-run");
        assert_eq!(APPS.test_ids.new_menu, "apps-new-menu");
        assert_eq!(APPS.copy.run_empty, "Not published yet");
        assert_eq!(APPS.copy.needs_update, "This app needs an update");
        assert_eq!(APPS.copy.menu.edit, "Edit app");
    }

    #[test]
    fn copy_and_test_ids_match_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        let copy = &APPS.copy;
        assert_eq!(fixture["copy"]["tabs"]["settings"], copy.tabs.settings.as_str());
        assert_eq!(fixture["copy"]["tabs"]["data"], copy.tabs.data.as_str());
        assert_eq!(EditorTab::Data.label(), copy.tabs.data);
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
            ("data", &ids.data),
        ] {
            assert_eq!(fixture["testIds"][key], value.as_str(), "{key}");
        }
        // VAPP-177: the floating layout is gone from the fixture.
        assert!(fixture.get("layout").is_none() && fixture.get("dockCard").is_none());
        assert!(fixture["panes"].get("headerHeight").is_none());
    }

    #[test]
    fn run_as_options_replay_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(APPS_JSON).unwrap();
        for case in fixture["runAsCases"].as_array().unwrap() {
            let members: Vec<(String, Option<String>, String)> = case["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| (m["userId"].as_str().unwrap().into(), m["name"].as_str().map(String::from), m["role"].as_str().unwrap().into()))
                .collect();
            let got = run_as_options(&members, case["me"].as_str(), case["role"].as_str())
                .map(|list| list.into_iter().map(|o| serde_json::json!({"value": o.value, "label": o.label})).collect::<Vec<_>>());
            assert_eq!(got.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null), case["options"], "{case}");
        }
        assert_eq!(APPS.test_ids.preview_run_as, fixture["testIds"]["previewRunAs"].as_str().unwrap());
        assert_eq!(APPS.copy.run_as.writes_off, fixture["copy"]["runAs"]["writesOff"].as_str().unwrap());
    }

    #[test]
    fn preview_template_replays_the_fixture() {
        let all = cases("templateCases");
        assert!(!all.is_empty());
        for case in all {
            let templates: serde_json::Map<String, Value> = case["templates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| (id.as_str().unwrap().to_string(), serde_json::json!({})))
                .collect();
            let pkg = serde_json::json!({ "templates": templates });
            let want: Vec<&str> = case["list"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            assert_eq!(preview_templates(Some(&pkg)), want, "{case}");
            assert_eq!(preview_template(Some(&pkg), case["picked"].as_str()).as_deref(), case["shown"].as_str(), "{case}");
        }
        assert!(preview_templates(None).is_empty());
        assert_eq!(preview_template(None, Some("week")), None);
    }

    #[test]
    fn preview_toolbar_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        let preview = &fixture["preview"];
        let frames: Vec<&str> = PreviewFrame::all().into_iter().map(PreviewFrame::as_str).collect();
        let want: Vec<&str> = preview["frames"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(frames, want);
        assert_eq!(PreviewFrame::default_frame().as_str(), preview["defaultFrame"].as_str().unwrap());
        assert_eq!(PreviewFrame::Phone.width(), Some(preview["phoneWidth"].as_f64().unwrap() as f32));
        assert_eq!(PreviewFrame::Desktop.width(), None);
        for frame in PreviewFrame::all() {
            assert_eq!(fixture["copy"]["frame"][frame.as_str()], frame.label().as_str());
        }
        let ids = &APPS.test_ids;
        for (key, value) in [
            ("previewToolbar", &ids.preview_toolbar),
            ("previewFrame", &ids.preview_frame),
            ("previewTemplate", &ids.preview_template),
        ] {
            assert_eq!(fixture["testIds"][key], value.as_str(), "{key}");
        }
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
    fn automation_row_replays_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        let all = fixture["automationRowCases"]["cases"].as_array().unwrap();
        assert!(!all.is_empty());
        for case in all {
            let name = case["name"].as_str().unwrap();
            let row = &case["row"];
            let on: AutomationOn = serde_json::from_value(row["on"].clone()).unwrap();
            let got = automation_row(&on, row["paused"].as_str(), row["lastStatus"].as_str());
            let want = &case["expected"];
            assert_eq!(got.trigger, want["trigger"].as_str().unwrap(), "{name}");
            assert_eq!(got.status.as_str(), want["status"].as_str().unwrap(), "{name}");
            assert_eq!(got.label, want["label"].as_str().unwrap(), "{name}");
        }
        assert_eq!(APPS.copy.automations.title, fixture["copy"]["automations"]["title"].as_str().unwrap());
        assert_eq!(APPS.test_ids.automations, fixture["testIds"]["automations"].as_str().unwrap());
        assert_eq!(APPS.test_ids.automation_row, fixture["testIds"]["automationRow"].as_str().unwrap());
    }

    #[test]
    fn webhook_row_replays_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        let all = fixture["webhookRowCases"]["cases"].as_array().unwrap();
        assert!(!all.is_empty());
        for case in all {
            let name = case["name"].as_str().unwrap();
            let got = webhook_row(case["row"]["hasSecret"].as_bool().unwrap(), case["isOwner"].as_bool().unwrap());
            let want = &case["expected"];
            assert_eq!(got.status.as_str(), want["status"].as_str().unwrap(), "{name}");
            assert_eq!(got.label, want["label"].as_str().unwrap(), "{name}");
            assert_eq!(got.action.map(WebhookAction::as_str), want["action"].as_str(), "{name}");
            assert_eq!(got.action_label.as_deref(), want["actionLabel"].as_str(), "{name}");
        }
        let copy = &APPS.copy.webhooks;
        for (key, value) in [
            ("title", &copy.title),
            ("ready", &copy.ready),
            ("unsigned", &copy.unsigned),
            ("copyUrl", &copy.copy_url),
            ("copied", &copy.copied),
            ("mint", &copy.mint),
            ("rotate", &copy.rotate),
            ("secret", &copy.secret),
            ("copySecret", &copy.copy_secret),
        ] {
            assert_eq!(fixture["copy"]["webhooks"][key], value.as_str(), "{key}");
        }
        assert_eq!(APPS.test_ids.webhooks, fixture["testIds"]["webhooks"].as_str().unwrap());
        assert_eq!(APPS.test_ids.webhook_row, fixture["testIds"]["webhookRow"].as_str().unwrap());
        assert_eq!(APPS.test_ids.webhook_secret, fixture["testIds"]["webhookSecret"].as_str().unwrap());
    }

    #[test]
    fn automation_sponsor_cases_replay() {
        let fixture: serde_json::Value = serde_json::from_str(APPS_JSON).unwrap();
        let all = fixture["automationSponsorCases"]["cases"].as_array().unwrap();
        assert!(!all.is_empty());
        for case in all {
            let name = case["name"].as_str().unwrap();
            let got = automation_sponsor(case["paused"].as_str(), case["sponsorName"].as_str());
            assert_eq!(got.as_deref(), case["expected"].as_str(), "{name}");
        }
    }

    #[test]
    fn automation_dropped_cases_replay() {
        let fixture: serde_json::Value = serde_json::from_str(APPS_JSON).unwrap();
        let all = fixture["automationDroppedCases"]["cases"].as_array().unwrap();
        assert!(!all.is_empty());
        for case in all {
            let got = automation_dropped(case["n"].as_i64().unwrap());
            assert_eq!(got.as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn upstream_picker_cases_replay() {
        let fixture: serde_json::Value = serde_json::from_str(APPS_JSON).unwrap();
        let block = &fixture["upstreamPickerCases"];
        assert_eq!(block["none"].as_str().unwrap(), UPSTREAM_PICKER_NONE);
        let accounts = &fixture["copy"]["accounts"];
        let (not_set_up, uses) = (accounts["notSetUp"].as_str().unwrap(), accounts["usesServer"].as_str().unwrap());
        assert_eq!(APPS.test_ids.account_row, fixture["testIds"]["accountRow"].as_str().unwrap());
        assert_eq!(APPS.test_ids.import_upstreams, fixture["testIds"]["importUpstreams"].as_str().unwrap());
        assert_eq!(APPS.test_ids.import_upstream, fixture["testIds"]["importUpstream"].as_str().unwrap());
        for case in block["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let row = &case["row"];
            let servers: Vec<(String, String)> = case["servers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s["id"].as_str().unwrap().to_string(), s["name"].as_str().unwrap().to_string()))
                .collect();
            let got = upstream_picker(row["name"].as_str().unwrap(), row["serverId"].as_str(), &servers, not_set_up);
            let want = case["expected"].as_object().map(|e| UpstreamPicker {
                value: e["value"].as_str().unwrap().to_string(),
                options: e["options"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|o| (o["value"].as_str().unwrap().to_string(), o["label"].as_str().unwrap().to_string()))
                    .collect(),
            });
            assert_eq!(got, want, "{name}");
            assert_eq!(account_caption(row["name"].as_str().unwrap(), row["serverName"].as_str(), uses).as_deref(), case["caption"].as_str(), "{name}");
        }
    }

    #[test]
    fn policy_row_cases_replay() {
        let fixture: serde_json::Value = serde_json::from_str(APPS_JSON).unwrap();
        let all = fixture["policyRowCases"]["cases"].as_array().unwrap();
        assert!(!all.is_empty());
        for case in all {
            let name = case["name"].as_str().unwrap();
            let levels = POLICY_OPS.map(|op| case["policy"][op].as_str().unwrap());
            assert_eq!(policy_caption(levels), case["expected"].as_str().unwrap(), "{name}");
        }
        assert_eq!(APPS.test_ids.policies, fixture["testIds"]["policies"].as_str().unwrap());
        assert_eq!(APPS.test_ids.policy_row, fixture["testIds"]["policyRow"].as_str().unwrap());
        assert_eq!(policy_kinds(Some(&serde_json::json!({"note": {}, "entry": {}}))), vec!["entry", "note"]);
    }

    #[test]
    fn publish_replays_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        for case in fixture["publishCases"]["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let app = AppVersions {
                head: case["app"]["headVersion"].as_i64().unwrap(),
                published: case["app"]["publishedVersion"].as_i64().unwrap(),
            };
            let editing = case["editing"].as_bool().unwrap();
            assert_eq!(shown_version(app, editing, case["viewing"].as_i64()), case["shown"].as_i64().unwrap(), "{name}");
            assert_eq!(can_publish(app), case["canPublish"].as_bool().unwrap(), "{name}");
            assert_eq!(list_pill(app), case["pill"].as_str().unwrap(), "{name}");
            assert_eq!(grant_version(app), case["grant"].as_i64().unwrap(), "{name}");
            for row in case["rows"].as_array().unwrap() {
                let number = row[0].as_i64().unwrap();
                assert_eq!(version_row(app, number).as_str(), row[1].as_str().unwrap(), "{name} v{number}");
            }
        }
    }

    #[test]
    fn version_migration_replays_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        for case in fixture["migrationCases"]["cases"].as_array().unwrap() {
            let r = &case["row"];
            let got = version_migration(r["migrationState"].as_str(), r["migrationTotal"].as_i64().unwrap(), r["migrationDone"].as_i64().unwrap(), r["migrationError"].as_str());
            assert_eq!(got.as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn rollback_cases_replay() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        for case in fixture["rollbackCases"]["cases"].as_array().unwrap() {
            let rows: Vec<(i64, Option<&str>)> =
                case["rows"].as_array().unwrap().iter().map(|r| (r["number"].as_i64().unwrap(), r["migrationState"].as_str())).collect();
            let got = rollback_warns(case["published"].as_i64().unwrap(), case["target"].as_i64().unwrap(), &rows);
            assert_eq!(got, case["warns"].as_bool().unwrap(), "{}", case["name"]);
        }
        assert_eq!(rollback_title(2), "Roll back to v2?");
        assert!(rollback_body(2).contains("v2 can refuse"));
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
    fn surface_state_replays_the_fixture() {
        for case in cases("surfaceStateCases") {
            let got = surface_state(case["hasPackage"].as_bool().unwrap(), case["applied"].as_bool().unwrap(), case["hasSurface"].as_bool().unwrap());
            let expected = match case["state"].as_str().unwrap() {
                "empty" => SurfaceState::Empty,
                "surface" => SurfaceState::Surface,
                _ => SurfaceState::NeedsUpdate,
            };
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn screen_shell_replays_the_fixture() {
        let all = cases("screenShellCases");
        assert!(!all.is_empty());
        for case in all {
            let shell = screen_shell(case["screens"].as_u64().unwrap() as usize, case["phone"].as_bool().unwrap());
            assert_eq!(shell.map(ScreenShell::as_str), case["shell"].as_str(), "{case}");
        }
        assert_eq!(APPS.screens.sidebar_width, 200.0);
        assert_eq!(APPS.screens.reserved, vec!["tab".to_string(), "frame".to_string()]);
    }

    #[test]
    fn screen_path_replays_the_fixture() {
        let all = cases("screenPathCases");
        assert!(!all.is_empty());
        for case in all {
            let params = case["params"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                .collect();
            let path = screen_path(
                case["teamSlug"].as_str().unwrap(),
                case["appSlug"].as_str().unwrap(),
                case["screen"].as_str().unwrap(),
                &params,
            );
            assert_eq!(path, case["path"].as_str().unwrap(), "{case}");
        }
    }

    #[test]
    fn screen_params_from_query_replays_the_fixture() {
        let all = cases("screenQueryCases");
        assert!(!all.is_empty());
        for case in all {
            let params = screen_params_from_query(case["query"].as_object().unwrap());
            let want: std::collections::BTreeMap<String, String> = case["params"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                .collect();
            assert_eq!(params, want, "{case}");
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

    #[test]
    fn editor_device_replays_the_fixture() {
        let cases = cases("editorDeviceCases");
        assert!(!cases.is_empty());
        for case in cases {
            let root = case["root"].as_object().map(|root| (root["status"].as_str().unwrap(), root["deviceId"].as_str()));
            let candidates: Vec<(&str, bool)> = case["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| (row["deviceId"].as_str().unwrap(), row["isDefault"].as_bool().unwrap()))
                .collect();
            let expected = &case["expected"];
            assert_eq!(
                editor_device(root, &candidates, case["remembered"].as_str()),
                EditorDeviceChoice {
                    device_id: expected["deviceId"].as_str().map(str::to_string),
                    moves: expected["move"].as_bool().unwrap(),
                },
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn app_files_replay_the_fixture() {
        let fixture: Value = serde_json::from_str(APPS_JSON).unwrap();
        let files = &fixture["appFiles"];
        assert_eq!(APP_FILES.part_name, files["partName"].as_str().unwrap());
        assert_eq!(APP_FILES.failed_verb, files["failedVerb"].as_str().unwrap());
        assert_eq!(APP_FILES.failed_thing, files["failedThing"].as_str().unwrap());
        assert_eq!(app_upload_path("abc"), "/api/apps/abc/files");
        assert_eq!(app_upload_path("a/b c"), "/api/apps/a%2Fb%20c/files");
        let targets = files["targetCases"].as_array().unwrap();
        assert!(!targets.is_empty());
        for case in targets {
            let components = case["components"].as_array().unwrap();
            let want = case["expected"].as_str().map(str::to_string);
            assert_eq!(app_upload_target(components, case["name"].as_str().unwrap()), want, "{}", case["$why"]);
        }
        let urls = files["urlCases"].as_array().unwrap();
        assert!(!urls.is_empty());
        for case in urls {
            let got = with_uploaded_url(Some(&case["list"]), case["fileName"].as_str().unwrap(), case["url"].as_str().unwrap());
            let want = case["expected"].as_array().cloned();
            assert_eq!(got, want, "{}", case["$why"]);
        }
        assert_eq!(with_uploaded_url(None, "a", "/u"), None);
    }
}
