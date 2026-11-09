//! VAPP-110 — one app's EDITOR ([`Screen::App`], web `app-editor.tsx`,
//! fixtures/apps.json THE EDITOR). VAPP-177 (`editorTabs`): header = the
//! app's glyph + name + the version pill, then the SegmentedControl of
//! `editorTabs.wide` (Preview | Data | Versions | Settings, `copy.tabs.*`); body =
//! two panes `5fr : 7fr` with a hairline between: the CHAT (the member's
//! editor session, [`crate::agent_screen::AgentScreenView::for_app`]: the
//! agent channel's rows and THE composer; the first send
//! `sessions.send({appId, text, deviceId})`; no header strip of its own)
//! beside the TAB pane:
//! - PREVIEW: the IDE's Exponential UI host ([`crate::exponential_ui_host::IdeHost`])
//!   installs the head package and applies its first template
//!   (`applyTemplate`, a surface minted under `app`) in a 12px frame, repainting
//!   on every synced upsert; a previewed older version sits under the `Viewing v{n}`
//!   banner with Restore and Current. VAPP-179 (`preview.$comment`): a
//!   toolbar row above both = the Desktop | Phone segmented control (the
//!   frame's width: the pane's, or `preview.phoneWidth` px centred) and,
//!   only when the shown package has more than one template, the template
//!   picker (`previewTemplates`, sorted ids); both kept per editor only.
//! - DATA (VAPP-178): the head package's records, browsed and edited
//!   ([`crate::app_data_tab::AppDataTab`]).
//! - VERSIONS: one grouped list, newest first: `v{n}` + the note over
//!   `who · relative time`; the head row = the `Current` badge, every other
//!   row = Preview (ghost, → the Preview tab on that version) + Restore
//!   (outline, `apps.restore` = a NEW head, toast `Restored v{n} as v{m}`).
//! - SETTINGS: [`crate::settings::app_detail::AppDetailPane`], ≤
//!   `editorTabs.settingsMaxWidth` px.
//!
//! The editor opens on `editorTabs.default` (no tab is kept); the settings
//! nav (Settings → Apps, the sidebar's hover gear, the dev `app:<slug>`
//! settings drive) opens it on Settings ([`open_app_tab`]), the dev screen
//! `app:<slug>:<tab>` on that tab (`app:<slug>:preview:phone` = the
//! Phone frame). VAPP-129's floating layout is gone.
//!
//! VAPP-218 (apps.json `screens`): a package with `screens` gets its
//! navigation painted BY THE HOST in the frame (`screen_shell`): the
//! sidebar-rows compact rows in a `sidebarWidth` column + a hairline (the
//! Desktop frame), or the phone tab bar under the surface (the Phone
//! frame); the shown screen = `resolve_screen` of the request (a row/tab
//! tap = that screen with its own params, `app.navigate` = screen + params,
//! a tapped `appScreen` link = [`open_app_screen`]); the template picker is
//! hidden then. The request stays in memory (no route).
//!
//! LIFETIME: entering starts nothing; leaving (a navigation off the screen,
//! the window closing) ENDS the member's live editor session
//! (`sessions.end` on the root); the next send continues it.
//!
//! VAPP-176 (apps.json NEW APP): a DRAFT's empty chat shows the starters
//! ([`crate::agent_screen`]). The editor keys the app by ID: a draft's slug
//! follows the name the agent gives it, so the screen resolves the
//! requested slug once, then follows the id (the navigation entry is
//! replaced by the id, in place). Leaving an EMPTY draft (no editor
//! session) fires `apps.discardDraft`.
//!
//! VAPP-242 (apps.json THE RUN VIEW, [`Screen::AppRun`]): the SAME view in
//! RUN mode ([`AppScreenView::run`]): the PUBLISHED package (never the
//! draft, never Run as) through the same preview host (records, grants, the
//! read-only access, the HostBanner, the consent card) and the screens
//! shell, FULL SCREEN: no header, no toolbar, no frame border or radius;
//! never published = the page-size `copy.runEmpty` with the app glyph. A
//! sidebar row and an `appScreen` link ([`open_app_screen`]) land here;
//! `app_navigate` keeps the screen in memory. Leaving it ends nothing.

use eui_core::theme::Mode;
use eui_gpui::runtime::ExponentialHost;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, relative, AnyElement, App, AppContext as _, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::rc::Rc;

use eui_core::host::{nav_screens, package_screens, resolve_screen, screen_data, ResolvedScreen};
use sync::Store;

use domain::apps::{
    editor_tab, is_draft, preview_template, preview_templates, screen_shell, surface_state, version_label, version_row, viewing_label, EditorTab,
    PreviewFrame, ScreenShell, SurfaceState, VersionRowKind,
    APPS,
};
use domain::rows::AppVersionRow;

use crate::apps_view::{find_app, version_pill, AppEntry};
use crate::controls::{PointerMenuButtonExt as _, WebControl as _};
use crate::exponential_ui_host::{IdeHost, THEME_ID};
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, resolved_screen, Navigation, Screen};

/// The header glyph (the web's `size-5`).
const HEADER_GLYPH_PX: f32 = 20.;
/// One header segment's width (the segmented control sizes to its tabs).
const TAB_SEGMENT_PX: f32 = 96.;
/// VAPP-179: one frame segment's width in the preview toolbar.
const FRAME_SEGMENT_PX: f32 = 88.;

/// VAPP-177: the tab the next editor of `app` opens on (the settings nav's
/// request), consumed by the editor that shows that app.
#[derive(Default)]
struct AppTabRequest(Option<(String, EditorTab)>);

impl gpui::Global for AppTabRequest {}

/// VAPP-177: open `app`'s editor (id or slug) on `tab` (Settings → Apps,
/// the sidebar's hover gear, the `app:<slug>` settings drive); `replace` =
/// the web's redirect (the navigation entry replaced in place).
pub(crate) fn open_app_tab(window: &Window, cx: &mut App, app: String, tab: EditorTab, replace: bool) {
    cx.set_global(AppTabRequest(Some((app.clone(), tab))));
    let screen = Screen::App { app };
    if replace {
        crate::navigation::navigate_replace(window, cx, screen);
    } else {
        crate::navigation::navigate(window, cx, screen);
    }
}

/// VAPP-218: the screen (+ params) the next editor of an app opens its
/// Preview on (a tapped `appScreen` link), consumed like [`AppTabRequest`].
#[derive(Default)]
struct AppScreenRequest(Option<(String, String, serde_json::Map<String, Value>)>);

impl gpui::Global for AppScreenRequest {}

/// VAPP-218 (app-link.json `appScreen`): VAPP-242 open `app`'s RUN VIEW
/// (id or slug) showing `screen` with `params`.
pub(crate) fn open_app_screen(
    window: &Window,
    cx: &mut App,
    app: String,
    screen: String,
    params: serde_json::Map<String, Value>,
) {
    cx.set_global(AppScreenRequest(Some((app.clone(), screen, params))));
    crate::navigation::navigate(window, cx, Screen::AppRun { app });
}

/// The pending screen request for the editor `shows` answers for, consumed.
fn take_screen_request(shows: impl Fn(&str) -> bool, cx: &mut App) -> Option<(String, serde_json::Map<String, Value>)> {
    let request = cx.try_global::<AppScreenRequest>()?.0.as_ref()?;
    if !shows(&request.0) {
        return None;
    }
    let (_, screen, params) = request.clone();
    cx.set_global(AppScreenRequest(None));
    Some((screen, params))
}

/// The pending tab request for the editor `shows` answers for, consumed.
fn take_tab_request(shows: impl Fn(&str) -> bool, cx: &mut App) -> Option<EditorTab> {
    let request = cx.try_global::<AppTabRequest>()?.0.as_ref()?;
    if !shows(&request.0) {
        return None;
    }
    let tab = request.1;
    cx.set_global(AppTabRequest(None));
    Some(tab)
}

/// VAPP-177: a dev-screen app spec `<slug>[:<tab>[:<frame>]]` → the app,
/// the tab and (VAPP-179) the preview frame asked for
/// (`app:working-hours:versions`, `app:team-dashboard:preview:phone`;
/// slugs and ids carry no `:`).
pub(crate) fn split_app_spec(spec: &str) -> (&str, Option<&str>, Option<&str>) {
    let mut parts = spec.splitn(3, ':');
    let app = parts.next().unwrap_or(spec);
    (app, parts.next(), parts.next())
}

/// DEV-ONLY: the tab `EXP_DEV_SCREEN=app:<slug>:<tab>` asks of the first
/// editor that shows that app (once).
fn dev_tab(shows: impl Fn(&str) -> bool) -> Option<EditorTab> {
    static TAKEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let spec = std::env::var("EXP_DEV_SCREEN").ok()?;
    let (app, tab, _) = split_app_spec(spec.trim().strip_prefix("app:")?);
    let tab = tab?;
    if !shows(app) || TAKEN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    Some(editor_tab(Some(tab), false).0)
}

/// DEV-ONLY (VAPP-179): the preview frame `EXP_DEV_SCREEN=app:<slug>:<tab>:<frame>`
/// asks of the first editor that shows that app (once).
fn dev_frame(shows: impl Fn(&str) -> bool) -> Option<PreviewFrame> {
    static TAKEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let spec = std::env::var("EXP_DEV_SCREEN").ok()?;
    let (app, _, frame) = split_app_spec(spec.trim().strip_prefix("app:")?);
    let frame = PreviewFrame::parse(frame?)?;
    if !shows(app) || TAKEN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    Some(frame)
}

/// DEV-ONLY: split a run drive's `<slug>[/<screen>]`
/// (`run:crm/contacts`, `run:leave-planner/timeline`; slugs carry no `/`).
pub(crate) fn split_run_spec(spec: &str) -> (&str, Option<&str>) {
    match spec.split_once('/') {
        Some((app, screen)) if !screen.is_empty() => (app, Some(screen)),
        Some((app, _)) => (app, None),
        None => (spec, None),
    }
}

/// DEV-ONLY: the screen `EXP_DEV_SCREEN=run:<slug>/<screen>` asks of the first
/// run view that shows that app (once).
fn dev_run_screen(shows: impl Fn(&str) -> bool) -> Option<String> {
    static TAKEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let spec = std::env::var("EXP_DEV_SCREEN").ok()?;
    let (app, screen) = split_run_spec(spec.trim().strip_prefix("run:")?);
    let screen = screen?;
    if !shows(app) || TAKEN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    Some(screen.to_string())
}

fn ide_mode(cx: &App) -> Mode {
    if cx.theme().mode.is_dark() {
        Mode::Dark
    } else {
        Mode::Light
    }
}

/// The tab / window title of [`crate::navigation::Screen::App`]: the app's
/// name, the page's title while it syncs.
pub(crate) fn app_tab_title(app: &str, cx: &App) -> SharedString {
    find_app(app, None, cx)
        .map(|entry| SharedString::from(entry.name))
        .unwrap_or_else(|| APPS.copy.title.clone().into())
}

/// The `applyTemplate` message the preview sends (the web's, byte for
/// byte): VAPP-179 the `picked` template while the package has it, else the
/// first of the sorted ids (`previewTemplate`, `templateCases`).
pub(crate) fn apply_template_message(pkg: &Value, picked: Option<&str>, surface_id: &str) -> Option<Value> {
    let template = preview_template(Some(pkg), picked)?;
    Some(json!({
        "version": eui_core::catalog::A2UI_VERSION,
        "applyTemplate": {
            "surfaceId": surface_id,
            "templateId": template,
            "packageId": pkg.get("id").cloned().unwrap_or(Value::Null),
        }
    }))
}

/// VAPP-218: the `applyTemplate` of a resolved screen: its template with
/// the data `{params}` (eUI `screenData`).
pub(crate) fn apply_screen_message(pkg: &Value, screen: &ResolvedScreen, surface_id: &str) -> Value {
    json!({
        "version": eui_core::catalog::A2UI_VERSION,
        "applyTemplate": {
            "surfaceId": surface_id,
            "templateId": screen.screen,
            "packageId": pkg.get("id").cloned().unwrap_or(Value::Null),
            "data": screen_data(screen),
        }
    })
}

/// Round 9 §1.4: the shown screen (else the `picked` template) onto a fresh
/// surface: a minted id, the template on it, and only then the `old` surface
/// goes, so it paints until the new one exists. Surface ids are unique for the
/// router's lifetime, so the old id can never come back. The new id.
fn open_surface(host: &Entity<ExponentialHost>, pkg: &Value, screen: Option<&ResolvedScreen>, picked: Option<&str>, old: Option<&str>, cx: &mut App) -> String {
    host.update(cx, |host, cx| {
        let surface_id = host.mint_surface_id(&APPS.preview.surface_id);
        let message = match screen {
            Some(screen) => Some(apply_screen_message(pkg, screen, &surface_id)),
            None => apply_template_message(pkg, picked, &surface_id),
        };
        if let Some(message) = message {
            host.receive_trusted(&message, cx);
        }
        if let Some(old) = old {
            host.receive(&json!({"version": eui_core::catalog::A2UI_VERSION, "deleteSurface": {"surfaceId": old}}), cx);
        }
        surface_id
    })
}

/// VAPP-218: a screen's eUI icon by name, unknown or none = the
/// placeholder.
fn screen_icon(name: Option<&str>) -> crate::icons::ExpIcon {
    name.and_then(|name| registry::concept_by_name(name).or_else(|| registry::icon_by_name(name)))
        .unwrap_or(registry::UI_ICON_PLACEHOLDER)
}

/// The preview's live host: the package it renders (its JSON = the key a
/// new version replaces it on) and the host itself.
struct Preview {
    key: String,
    /// The shown template (+ the screen's params): a new one re-opens the
    /// surface on the same host.
    template: String,
    /// The painted surface: minted per open (round 9 §1.4).
    surface_id: String,
    ide: IdeHost,
    _observe: Subscription,
}

pub struct AppScreenView {
    /// The id or slug the screen was opened with.
    requested: String,
    /// VAPP-242: THE RUN VIEW (the published package, full screen), not
    /// the editor.
    run: bool,
    /// VAPP-176: the `apps` row id once resolved; the screen follows it
    /// (a draft's slug changes when the agent names it).
    app_id: Option<String>,
    /// The leave already ran (the panel's `leave`, then the release).
    left: bool,
    nav: Entity<Navigation>,
    app: Option<AppEntry>,
    versions: Vec<AppVersionRow>,
    /// The chat pane, built once the app resolves.
    chat: Option<Entity<crate::agent_screen::AgentScreenView>>,
    /// #EXP-1729 (apps.json `editorHeader`): the header's `status · device`
    /// subtitle off the chat (its root and the composer's device),
    /// repainted only when it changes.
    editor_header: Option<domain::apps::EditorHeader>,
    preview: Option<Preview>,
    /// The version being previewed (`None` = the head) and its package.
    viewing: Option<i64>,
    viewing_package: Option<Value>,
    restoring: bool,
    /// eApps V1 (#VAPP-210): the version a Publish is running for.
    publishing: Option<i64>,
    /// The live editor root this screen saw last (ended on leave).
    live_root: Option<String>,
    preview_scroll: ScrollHandle,
    versions_scroll: ScrollHandle,
    settings_scroll: ScrollHandle,
    /// VAPP-177: the tab on screen (`editorTabs.wide`).
    tab: EditorTab,
    /// VAPP-177: the Settings tab's body, built once the app resolves.
    settings: Option<Entity<crate::settings::app_detail::AppDetailPane>>,
    /// VAPP-178: the Data tab's body, built once the app resolves.
    data: Option<Entity<crate::app_data_tab::AppDataTab>>,
    /// VAPP-179: the preview frame (`preview.defaultFrame` on open).
    frame: PreviewFrame,
    /// VAPP-179: the picked template (`None` = the default; an id the shown
    /// package lacks falls back to it).
    picked_template: Option<String>,
    /// VAPP-218: the requested screen (`None` = the first) and its params;
    /// `resolve_screen` falls back to the first when the package lacks it.
    screen: Option<String>,
    screen_params: serde_json::Map<String, Value>,
    /// VAPP-198: Run as (`None` = me) and the app's non-Full access levels
    /// (`appAccess.levels`, owners) for that app.
    run_as: Option<String>,
    access_levels: Option<(String, HashMap<String, String>)>,
    _subscriptions: Vec<Subscription>,
}

impl AppScreenView {
    pub fn new(requested: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::build(requested, false, window, cx)
    }

    /// VAPP-242: THE RUN VIEW of `requested` (id or slug).
    pub fn run(requested: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::build(requested, true, window, cx)
    }

    /// VAPP-242: whether this is THE RUN VIEW (else the editor).
    pub(crate) fn is_run(&self) -> bool {
        self.run
    }

    fn build(requested: String, run: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe_global::<gpui_component::Theme>(|this, cx| {
            let mode = ide_mode(cx);
            if let Some(preview) = &this.preview {
                preview.ide.host.update(cx, |host, cx| host.set_mode(mode, cx));
            }
        })];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe_in(&collections.apps, window, |this, _, window, cx| this.refresh(window, cx)));
            subscriptions.push(cx.observe_in(&collections.app_versions, window, |this, _, window, cx| {
                this.refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.sessions, window, |this, _, window, cx| {
                this.refresh(window, cx)
            }));
            subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));
        }
        // VAPP-235: the header's pending glyph follows the record outbox.
        if let Some(outbox) = crate::app_outbox::record_outbox(cx) {
            subscriptions.push(cx.observe(&outbox, |_, _, cx| cx.notify()));
        }
        // VAPP-177: the settings nav asking this editor for a tab.
        subscriptions.push(cx.observe_global::<AppTabRequest>(|this, cx| {
            if this.run {
                return;
            }
            let shows = |app: &str| this.shows(app);
            if let Some(tab) = take_tab_request(shows, cx) {
                this.tab = tab;
                cx.notify();
            }
        }));
        // VAPP-218 + VAPP-242: a tapped `appScreen` link while this run view
        // is open.
        subscriptions.push(cx.observe_global::<AppScreenRequest>(|this, cx| {
            if !this.run {
                return;
            }
            if let Some((screen, params)) = take_screen_request(|app| this.shows(app), cx) {
                this.request_screen(screen, params, cx);
            }
        }));
        // Leaving ends the live editor session: the screens panel drops this
        // view on a navigation off it, the window drops it on close.
        cx.on_release(|this, cx| this.end_live_session(cx)).detach();
        let mut this = Self {
            requested,
            run,
            app_id: None,
            left: false,
            nav,
            app: None,
            versions: Vec::new(),
            chat: None,
            editor_header: None,
            preview: None,
            viewing: None,
            viewing_package: None,
            publishing: None,
            restoring: false,
            live_root: None,
            preview_scroll: ScrollHandle::new(),
            versions_scroll: ScrollHandle::new(),
            settings_scroll: ScrollHandle::new(),
            tab: editor_tab(None, false).0,
            settings: None,
            data: None,
            frame: PreviewFrame::default_frame(),
            picked_template: None,
            screen: None,
            screen_params: serde_json::Map::new(),
            run_as: None,
            access_levels: None,
            _subscriptions: subscriptions,
        };
        this.refresh(window, cx);
        if run {
            // VAPP-242: the run view is wide (`screenShell(n, phone: false)`).
            this.frame = PreviewFrame::Desktop;
            if let Some((screen, params)) = take_screen_request(|app| this.shows(app), cx) {
                this.request_screen(screen, params, cx);
            } else if let Some(screen) = dev_run_screen(|app| this.shows(app)) {
                this.request_screen(screen, serde_json::Map::new(), cx);
            }
            return this;
        }
        // VAPP-177: the editor opens on the default tab unless the settings
        // nav (or the dev screen) asked for one.
        let requested = take_tab_request(|app| this.shows(app), cx).or_else(|| dev_tab(|app| this.shows(app)));
        if let Some(tab) = requested {
            this.tab = editor_tab(Some(tab.as_str()), false).0;
        }
        if let Some(frame) = dev_frame(|app| this.shows(app)) {
            this.frame = frame;
        }
        this
    }

    /// VAPP-218: open `screen` with `params` (a row/tab tap = no params, an
    /// `app.navigate`, a link); a new screen = a fresh host.
    fn request_screen(&mut self, screen: String, params: serde_json::Map<String, Value>, cx: &mut gpui::Context<Self>) {
        self.screen = Some(screen);
        self.screen_params = params;
        self.sync_preview(cx);
        cx.notify();
    }

    /// VAPP-218: the screen the preview shows for `pkg` (`None` = the
    /// package declares no screens).
    fn resolved_screen(&self, pkg: &Value) -> Option<ResolvedScreen> {
        resolve_screen(pkg, self.screen.as_deref(), Some(&Value::Object(self.screen_params.clone())))
    }

    fn set_frame(&mut self, frame: PreviewFrame, cx: &mut gpui::Context<Self>) {
        if self.frame != frame {
            self.frame = frame;
            cx.notify();
        }
    }

    /// VAPP-198: the Run as options (`runAsCases`) for the app's team: the
    /// members with their names and roles; `None` = no picker.
    fn run_as_choices(&self, cx: &App) -> Option<Vec<domain::apps::RunAsOption>> {
        let team_id = self.app.as_ref()?.row.as_ref()?.team_id.clone()?;
        let store = Store::try_global(cx)?;
        let me = crate::queries::active_account(cx).map(|a| a.user_id);
        let users = store.collections().users.read(cx);
        let members: Vec<(String, Option<String>, String)> = store
            .collections()
            .team_members
            .read(cx)
            .iter()
            .filter(|m| m.team_id == team_id)
            .map(|m| (m.user_id.clone(), users.get(&m.user_id).and_then(|u| u.name.clone()), m.role.clone().unwrap_or_else(|| "member".into())))
            .collect();
        let role = members.iter().find(|(id, _, _)| Some(id) == me.as_ref()).map(|(_, _, role)| role.clone());
        domain::apps::run_as_options(&members, me.as_deref(), role.as_deref())
    }

    /// VAPP-198: the member the preview runs as (`None` = me).
    fn run_as(&self, pkg: &Value, cx: &App) -> Option<crate::exponential_ui_host::RunAs> {
        let id = self.run_as.clone()?;
        let app_id = self.app.as_ref()?.id.clone();
        let label = self.run_as_choices(cx)?.into_iter().find(|o| o.value == id)?.label;
        let team_id = self.app.as_ref()?.row.as_ref()?.team_id.clone()?;
        let role = Store::try_global(cx)?
            .collections()
            .team_members
            .read(cx)
            .iter()
            .find(|m| m.team_id == team_id && m.user_id == id)
            .and_then(|m| m.role.clone())
            .unwrap_or_else(|| "member".into());
        let access = self.access_levels.as_ref().filter(|(app, _)| *app == app_id).and_then(|(_, levels)| levels.get(&id).cloned());
        Some(crate::exponential_ui_host::RunAs { id, name: label, role, access, kinds: pkg.get("records").map(Value::to_string).unwrap_or_default() })
    }

    /// VAPP-198: run the preview as `member` (`None` = me); the first pick
    /// fetches the app's access levels (owners only).
    fn pick_run_as(&mut self, member: Option<String>, cx: &mut gpui::Context<Self>) {
        self.run_as = member;
        let app_id = self.app.as_ref().map(|a| a.id.clone());
        if let (Some(app_id), Some(trpc)) = (app_id.filter(|id| self.access_levels.as_ref().map(|(a, _)| a) != Some(id)), crate::queries::trpc_client(cx)) {
            cx.spawn(async move |this, cx| {
                let id = app_id.clone();
                let result = cx.background_executor().spawn(async move { api::app_access::levels(&trpc, &id) }).await;
                let _ = this.update(cx, |this, cx| {
                    this.access_levels = Some((app_id, result.map(|r| r.levels).unwrap_or_default()));
                    this.sync_preview(cx);
                    cx.notify();
                });
            })
            .detach();
        }
        self.sync_preview(cx);
        cx.notify();
    }

    /// VAPP-179: show `template` (the host re-syncs on the new key).
    fn pick_template(&mut self, template: String, cx: &mut gpui::Context<Self>) {
        self.picked_template = Some(template);
        self.sync_preview(cx);
        cx.notify();
    }

    fn set_tab(&mut self, tab: EditorTab, cx: &mut gpui::Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            cx.notify();
        }
    }

    /// Whether this view shows `app` (the id or slug it was opened with,
    /// the resolved id, or the app's current slug).
    pub(crate) fn shows(&self, app: &str) -> bool {
        self.requested == app
            || self.app_id.as_deref() == Some(app)
            || self.app.as_ref().is_some_and(|entry| entry.slug == app)
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let team_id = active_team_id(&self.nav, cx);
        // VAPP-176: by id once resolved, so a slug change never loses it.
        let key = self.app_id.clone().unwrap_or_else(|| self.requested.clone());
        self.app = find_app(&key, team_id.as_deref(), cx);
        let app_id = self.app.as_ref().map(|app| app.id.clone());
        if let Some(id) = app_id.clone().filter(|id| self.app_id.as_ref() != Some(id)) {
            self.app_id = Some(id.clone());
            // The navigation entry names the app by id from now on (the
            // tab title, the rail's highlight), replaced in place.
            let run = self.run;
            let screen = move |app: String| if run { Screen::AppRun { app } } else { Screen::App { app } };
            let requested = screen(self.requested.clone());
            if id != self.requested && resolved_screen(&self.nav, cx).as_ref() == Some(&requested) {
                cx.defer_in(window, move |_, window, cx| {
                    crate::navigation::navigate_replace(window, cx, screen(id));
                });
            }
        }
        self.versions = app_id
            .as_deref()
            .and_then(|app_id| {
                Store::try_global(cx).map(|store| {
                    let mut rows: Vec<AppVersionRow> = store
                        .collections()
                        .app_versions
                        .read(cx)
                        .iter()
                        .filter(|row| row.app_id == app_id)
                        .cloned()
                        .collect();
                    rows.sort_by(|a, b| b.number.cmp(&a.number));
                    rows
                })
            })
            .unwrap_or_default();
        if let Some(app_id) = &app_id {
            if self.data.is_none() {
                let app_id = app_id.clone();
                let data = cx.new(|cx| crate::app_data_tab::AppDataTab::new(app_id.clone(), cx));
                // VAPP-200: the access it fetches decides the preview's read-only.
                self._subscriptions.push(cx.observe(&data, |this, _, cx| this.sync_preview(cx)));
                self.data = Some(data);
                // VAPP-242: the run view is the host alone (no chat, no tabs).
                if !self.run {
                    let chat = cx.new(|cx| crate::agent_screen::AgentScreenView::for_app(app_id.clone(), window, cx));
                    self.editor_header = chat.read(cx).editor_header();
                    self._subscriptions.push(cx.observe(&chat, |this, chat, cx| {
                        let header = chat.read(cx).editor_header();
                        if header != this.editor_header {
                            this.editor_header = header;
                            cx.notify();
                        }
                    }));
                    self.chat = Some(chat);
                    self.settings =
                        Some(cx.new(|cx| crate::settings::app_detail::AppDetailPane::new(app_id, window, cx)));
                }
            }
            if !self.run {
                if let Some(root) = crate::session_data::my_editor_session(app_id, cx) {
                    self.live_root = Some(root.id);
                }
            }
        }
        // The version being viewed became the preview's default: drop the pick.
        if self.viewing.is_some() && self.viewing == Some(self.default_shown(cx)) {
            self.viewing = None;
            self.viewing_package = None;
        }
        self.sync_preview(cx);
        cx.notify();
    }

    /// eApps V1: my editor session for this app is live (not ended).
    fn editing(&self, cx: &App) -> bool {
        self.app
            .as_ref()
            .and_then(|app| crate::session_data::my_editor_session(&app.id, cx))
            .is_some_and(|root| root.status.as_deref() != Some(domain::contract::SESSION_STATUS_ENDED))
    }

    /// eApps V1 (`shownVersion`): the version the preview shows unpicked.
    fn default_shown(&self, cx: &App) -> i64 {
        self.app.as_ref().map_or(0, |app| domain::apps::shown_version(app.versions(), self.editing(cx), None))
    }

    /// eApps V1: the viewer may write the app (access `write`): Publish.
    fn can_write(&self, cx: &App) -> bool {
        self.data.as_ref().is_none_or(|data| data.read(cx).access().is_none_or(|level| level == "write"))
    }

    /// The package the preview shows (`shownVersion`): a picked version's,
    /// else the head's (the draft) or the published one's, off the row.
    fn shown_package(&self, cx: &App) -> Option<Value> {
        let app = self.app.as_ref()?;
        // VAPP-242: the run view runs the PUBLISHED package only.
        if self.run {
            return (app.published_version > 0).then(|| app.row.as_ref()?.package.clone().filter(Value::is_object)).flatten();
        }
        let shown = domain::apps::shown_version(app.versions(), self.editing(cx), self.viewing);
        if shown == 0 {
            return None;
        }
        if shown == app.head_version {
            return app.head_package();
        }
        if shown == app.published_version {
            return app.row.as_ref()?.package.clone().filter(Value::is_object);
        }
        self.viewing_package.clone()
    }

    /// eApps R8 (apps.json `migrationCases`): previewing a version other than
    /// the published one: the published package's `version` the records are
    /// read through (the draft's migrations, copies only); `None` = as stored.
    fn migrate_from(&self, cx: &App) -> Option<String> {
        if self.run {
            return None;
        }
        let app = self.app.as_ref()?;
        let shown = domain::apps::shown_version(app.versions(), self.editing(cx), self.viewing);
        if app.published_version == 0 || shown == app.published_version {
            return None;
        }
        app.row.as_ref()?.package.as_ref()?.get("version")?.as_str().map(str::to_string)
    }

    /// eApps V1: the shown version is the one the grants were made on.
    fn shows_grant_version(&self, cx: &App) -> bool {
        // VAPP-242: the run view shows the published version = the grants'.
        if self.run {
            return self.app.as_ref().is_some_and(|app| {
                app.published_version > 0 && domain::apps::grant_version(app.versions()) == app.published_version
            });
        }
        self.app.as_ref().is_some_and(|app| {
            domain::apps::shown_version(app.versions(), self.editing(cx), self.viewing) == domain::apps::grant_version(app.versions())
        })
    }

    /// A new package = a fresh host (the web keys its `PackageSurface` on
    /// the package), so every synced upsert repaints.
    fn sync_preview(&mut self, cx: &mut gpui::Context<Self>) {
        let pkg = self.shown_package(cx);
        let grants = self.shows_grant_version(cx);
        // The key carries the grants flag: only the version the grants were
        // made on reads them (`grantVersion`).
        // VAPP-179: and the template shown (a pick = a fresh host).
        // VAPP-218: with screens, the resolved screen and its params instead.
        let resolved = pkg.as_ref().and_then(|pkg| self.resolved_screen(pkg));
        let template = match &resolved {
            Some(screen) => format!("{}{}", screen.screen, Value::Object(screen.params.clone())),
            None => preview_template(pkg.as_ref(), self.picked_template.as_deref()).unwrap_or_default(),
        };
        let run_as = pkg.as_ref().and_then(|pkg| self.run_as(pkg, cx));
        // VAPP-200: at app access `read` (the Run as member's, else mine)
        // the app's own writing controls paint disabled.
        let access = match &run_as {
            Some(r) => r.access.clone(),
            None => self.data.as_ref().and_then(|d| d.read(cx).access().map(str::to_string)),
        };
        let read_only = access.as_deref() == Some("read");
        let key = format!(
            "{}:{template}:{:?}:{read_only}:{}",
            grants,
            run_as,
            pkg.as_ref().map(Value::to_string).unwrap_or_default()
        );
        if let Some(preview) = self.preview.as_mut().filter(|p| p.key == key) {
            if preview.template != template {
                if let Some(pkg) = &pkg {
                    let opened = open_surface(&preview.ide.host, pkg, resolved.as_ref(), self.picked_template.as_deref(), Some(&preview.surface_id), cx);
                    preview.surface_id = opened;
                    preview.template = template;
                    cx.notify();
                }
            }
            return;
        }
        if let Some(old) = self.preview.take() {
            old.ide.host.update(cx, |host, cx| host.close(cx));
        }
        let Some(pkg) = pkg else { return };
        let theme = pkg.get("theme").and_then(Value::as_str).unwrap_or(THEME_ID).to_string();
        let migrate_from = self.migrate_from(cx);
        // VAPP-122: the editor's app keeps its records (`records:<kind>`,
        // `records.*`).
        let scope = self.app.as_ref().map(|app| crate::exponential_ui_host::HostScope {
            app_id: app.id.clone(),
            team_id: app.row.as_ref().and_then(|row| row.team_id.clone()),
            grants,
            run_as: run_as.clone(),
            read_only,
            records: pkg.get("records").cloned(),
            migrate_from: migrate_from.clone().filter(|_| pkg.get("migrations").is_some_and(|m| m.as_array().is_some_and(|l| !l.is_empty()))).map(|from| (from, serde_json::json!({ "migrations": pkg["migrations"].clone() }))),
        });
        let ide = IdeHost::with_packages(ide_mode(cx), std::slice::from_ref(&pkg), Some(&theme), scope, cx);
        // VAPP-218: `app_navigate` replaces the request; deferred, since
        // the new screen re-opens the surface that is running the call.
        let view = cx.entity().downgrade();
        let navigate: eui_gpui::runtime::NavigateHook = Rc::new(move |target: &eui_gpui::runtime::HostNavigate, cx: &mut App| {
            let Some(view) = view.upgrade() else { return false };
            let (screen, params) = (target.screen.clone(), target.params.clone());
            cx.defer(move |cx| view.update(cx, |this, cx| this.request_screen(screen, params, cx)));
            true
        });
        ide.host.update(cx, |host, _| host.set_navigate(Some(navigate)));
        let surface_id = open_surface(&ide.host, &pkg, resolved.as_ref(), self.picked_template.as_deref(), None, cx);
        let observe = cx.observe(&ide.host, |_, _, cx| cx.notify());
        self.preview = Some(Preview { key, template, surface_id, ide, _observe: observe });
    }

    /// `sessions.end` on the member's live editor root (the tree);
    /// VAPP-176: an EMPTY draft (no editor session) is discarded instead.
    fn end_live_session(&mut self, cx: &mut App) {
        // VAPP-242: leaving a run view ends nothing (and discards nothing).
        if self.run {
            return;
        }
        if !self.left {
            self.left = true;
            if let Some(app) = self.app.as_ref().filter(|app| is_draft(app.head_version)) {
                if self.live_root.is_none() && crate::session_data::my_editor_session(&app.id, cx).is_none() {
                    crate::apps_view::discard_draft(app.id.clone(), cx);
                }
            }
        }
        let Some(root_id) = self.live_root.take() else { return };
        let live = crate::session_data::session(&root_id, cx)
            .is_some_and(|row| row.status.as_deref() != Some(domain::contract::SESSION_STATUS_ENDED));
        if !live {
            return;
        }
        crate::settings::spawn_trpc(cx, "sessions.end", move |trpc| api::sessions::end(trpc, &root_id));
    }

    /// The screens panel calls this when a navigation leaves the editor.
    pub(crate) fn leave(&mut self, cx: &mut gpui::Context<Self>) {
        self.end_live_session(cx);
    }

    /// Preview version `number` (the preview's default clears it); its
    /// package loads on demand unless it rides on the row.
    fn pick_version(&mut self, number: i64, cx: &mut gpui::Context<Self>) {
        let Some(app) = self.app.clone() else { return };
        if number == self.default_shown(cx) {
            self.viewing = None;
            self.viewing_package = None;
            self.sync_preview(cx);
            cx.notify();
            return;
        }
        self.viewing = Some(number);
        self.viewing_package = None;
        self.sync_preview(cx);
        cx.notify();
        if number == app.head_version || number == app.published_version {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::apps::get_version(&trpc, &app.id, number) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.viewing != Some(number) {
                    return;
                }
                match result {
                    Ok(output) => this.viewing_package = output.package,
                    Err(_) => this.viewing = None,
                }
                this.sync_preview(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn clear_viewing(&mut self, cx: &mut gpui::Context<Self>) {
        self.viewing = None;
        self.viewing_package = None;
        self.sync_preview(cx);
        cx.notify();
    }

    /// Restore version `number` (a Versions row's button, or the viewing
    /// banner's) = a NEW head; the preview back on the head.
    fn restore(&mut self, number: i64, cx: &mut gpui::Context<Self>) {
        let Some(app) = self.app.clone() else { return };
        if self.restoring {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("restore", "the version", None, cx);
            return;
        };
        self.restoring = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::apps::restore(&trpc, &app.id, number) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.restoring = false;
                match result {
                    Ok(output) => {
                        crate::toast::show_in_active_window(
                            crate::toast::Toast::info(domain::apps::restored_label(output.restored, output.version)),
                            cx,
                        );
                        this.clear_viewing(cx);
                    }
                    Err(err) => crate::action_error::failed_action_err("restore", "the version", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// eApps V1 (#VAPP-210): PUBLISH version `number` (the header's button
    /// = the head, a Versions row's, the viewing banner's); the preview back
    /// on its default.
    fn publish(&mut self, number: i64, cx: &mut gpui::Context<Self>) {
        let Some(app) = self.app.clone() else { return };
        if self.publishing.is_some() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("publish", "the version", None, cx);
            return;
        };
        self.publishing = Some(number);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::apps::publish(&trpc, &app.id, number) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.publishing = None;
                match result {
                    Ok(output) => {
                        crate::toast::show_in_active_window(
                            crate::toast::Toast::info(domain::apps::published_label(output.published)),
                            cx,
                        );
                        this.clear_viewing(cx);
                    }
                    Err(err) => crate::action_error::failed_action_err("publish", "the version", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #VAPP-256 (apps.json `rollbackCases`): a rollback past a migrated
    /// version asks first; every other publish goes through at once.
    fn request_publish(&mut self, number: i64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let published = self.app.as_ref().map(|app| app.versions().published).unwrap_or(0);
        let rows: Vec<(i64, Option<&str>)> =
            self.versions.iter().map(|v| (v.number.unwrap_or(0), v.migration_state.as_deref())).collect();
        if !domain::apps::rollback_warns(published, number, &rows) {
            self.publish(number, cx);
            return;
        }
        let this = cx.entity().downgrade();
        let spec = crate::native_dialog::AlertSpec::from_prompt(APPS.copy.publish.clone(), &crate::apps_view::rollback_prompt(number))
            .on_ok(move |_, cx| {
                let _ = this.update(cx, |this, cx| this.publish(number, cx));
                true
            });
        crate::native_dialog::open_alert(window, cx, spec);
    }

    fn render_header(&self, app: &AppEntry, cx: &mut gpui::Context<Self>) -> AnyElement {
        let show_publish = self.can_write(cx) && domain::apps::can_publish(app.versions());
        let head = app.head_version;
        let theme = cx.theme();
        let tabs = EditorTab::wide();
        let segments: Vec<_> = tabs
            .iter()
            .map(|&tab| {
                crate::controls::segmented_item(self.tab == tab, cx)
                    .id(SharedString::from(format!("{}-{}", APPS.test_ids.tabs, tab.as_str())))
                    .child(tab.label())
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| this.set_tab(tab, cx)))
            })
            .collect();
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .px_6()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .child(app.icon().with_size(px(HEADER_GLYPH_PX)))
            // #EXP-1729: the chat header's title block, the name over the
            // `status · device` subtitle in its tone.
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(SharedString::from(app.name.clone())),
                    )
                    .when_some(self.editor_header.clone(), |block, header| {
                        let tone = match header.tone {
                            domain::apps::EditorHeaderTone::Attention => theme::tokens::YELLOW.to_hsla(),
                            domain::apps::EditorHeaderTone::Active => theme.foreground,
                            domain::apps::EditorHeaderTone::Muted => theme.muted_foreground,
                        };
                        block.child(
                            div()
                                .id("app-editor-subtitle")
                                .min_w_0()
                                .truncate()
                                .text_size(px(domain::chats::CHATS.shell.chat_header.subtitle))
                                .text_color(tone)
                                .child(SharedString::from(header.subtitle)),
                        )
                    }),
            )
            .child(version_pill("app-editor-version", app, cx))
            // VAPP-235 (app-records.json `outbox`): while the running app has
            // record writes waiting to sync, the muted `ui-offline` glyph.
            .when_some(crate::app_outbox::pending_label_for(&app.id, cx), |row, label| {
                let tip = SharedString::from(label);
                row.child(
                    div()
                        .id("app-outbox-pending")
                        .flex_shrink_0()
                        .child(Icon::new(registry::UI_OFFLINE).size_4().text_color(theme.muted_foreground))
                        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)),
                )
            })
            .child(div().flex_1())
            .when(show_publish, |row| {
                row.child(
                    Button::new(SharedString::from(APPS.test_ids.publish.clone()))
                        .primary()
                        .web_sm()
                        .cursor_pointer()
                        .label(APPS.copy.publish.clone())
                        .loading(self.publishing == Some(head))
                        .disabled(self.publishing.is_some())
                        .on_click(cx.listener(move |this, _, _, cx| this.publish(head, cx))),
                )
            })
            .child(
                crate::controls::segmented(cx)
                    .id(SharedString::from(APPS.test_ids.tabs.clone()))
                    .flex_shrink_0()
                    .w(px(TAB_SEGMENT_PX * tabs.len() as f32))
                    .h(px(32.))
                    .children(segments),
            )
            .into_any_element()
    }

    fn render_chat(&self, chat: &Entity<crate::agent_screen::AgentScreenView>) -> AnyElement {
        div()
            .id(SharedString::from(APPS.test_ids.chat.clone()))
            .size_full()
            .min_h_0()
            .child(chat.clone())
            .into_any_element()
    }

    /// VAPP-179: the preview toolbar (testId `previewToolbar`): the frame's
    /// segmented control at the start; at the end, only when the shown
    /// package has more than one template, the template picker.
    fn render_preview_toolbar(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let frames = PreviewFrame::all();
        let segments: Vec<_> = frames
            .iter()
            .map(|&frame| {
                crate::controls::segmented_item(self.frame == frame, cx)
                    .id(SharedString::from(format!("{}-{}", APPS.test_ids.preview_frame, frame.as_str())))
                    .child(frame.label())
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| this.set_frame(frame, cx)))
            })
            .collect();
        let pkg = self.shown_package(cx);
        let templates = preview_templates(pkg.as_ref());
        // VAPP-218: with screens the screens ARE the picker.
        let has_screens = pkg.as_ref().is_some_and(|pkg| !package_screens(pkg).is_empty());
        let picker = (templates.len() > 1 && !has_screens).then(|| {
            let current = preview_template(pkg.as_ref(), self.picked_template.as_deref()).unwrap_or_default();
            let view = cx.entity().downgrade();
            Button::new(SharedString::from(APPS.test_ids.preview_template.clone()))
                .accessibility_label(current.clone())
                .outline()
                .web_sm()
                .cursor_pointer()
                .child(div().min_w_0().truncate().child(current.clone()))
                .child(Icon::new(registry::UI_CHEVRON_DOWN).size(px(14.)).text_color(cx.theme().muted_foreground))
                .pointer_menu(move |mut menu, _window, _cx| {
                    menu = crate::pickers::single_select_menu(menu);
                    for id in &templates {
                        let view = view.clone();
                        let value = id.clone();
                        menu = menu.item(
                            crate::controls::pointer_label_item(id.clone(), false)
                                .checked(*id == current)
                                .on_click(move |_, _, cx| {
                                    let value = value.clone();
                                    let _ = view.update(cx, |this, cx| this.pick_template(value, cx));
                                }),
                        );
                    }
                    menu
                })
        });
        // VAPP-198: Run as (owners with another member), after the frame.
        let run_as_picker = self.run_as_choices(cx).map(|choices| {
            let current = self.run_as.clone().unwrap_or_default();
            let label = choices.iter().find(|o| o.value == current).map(|o| o.label.clone()).unwrap_or_default();
            let view = cx.entity().downgrade();
            Button::new(SharedString::from(APPS.test_ids.preview_run_as.clone()))
                .accessibility_label(label.clone())
                .outline()
                .web_sm()
                .cursor_pointer()
                .child(div().min_w_0().truncate().child(label))
                .child(Icon::new(registry::UI_CHEVRON_DOWN).size(px(14.)).text_color(cx.theme().muted_foreground))
                .pointer_menu(move |mut menu, _window, _cx| {
                    menu = crate::pickers::single_select_menu(menu);
                    for option in &choices {
                        let view = view.clone();
                        let value = option.value.clone();
                        menu = menu.item(
                            crate::controls::pointer_label_item(option.label.clone(), false)
                                .checked(option.value == current)
                                .on_click(move |_, _, cx| {
                                    let value = value.clone();
                                    let _ = view.update(cx, |this, cx| this.pick_run_as(Some(value).filter(|v| !v.is_empty()), cx));
                                }),
                        );
                    }
                    menu
                })
        });
        h_flex()
            .id(SharedString::from(APPS.test_ids.preview_toolbar.clone()))
            .flex_shrink_0()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                crate::controls::segmented(cx)
                    .id(SharedString::from(APPS.test_ids.preview_frame.clone()))
                    .flex_shrink_0()
                    .w(px(FRAME_SEGMENT_PX * frames.len() as f32))
                    .h(px(32.))
                    .children(segments),
            )
            .children(run_as_picker)
            .child(div().flex_1())
            .children(picker)
            .into_any_element()
    }

    /// VAPP-218 (apps.json `screens`): the shell of the shown package's
    /// screens (`screen_shell`: the Phone frame = the bar), testId
    /// `previewScreens`: the sidebar-rows compact rows in a `sidebarWidth`
    /// column, or the phone tab bar; the shown screen active. `None` = no
    /// screens (or nothing painted yet).
    fn render_screens(&self, preview_empty: bool, cx: &mut gpui::Context<Self>) -> Option<(ScreenShell, AnyElement)> {
        if preview_empty {
            return None;
        }
        let pkg = self.shown_package(cx)?;
        // #EXP-1674: the navigation's screens; a `nav: false` one opens by link.
        let screens: Vec<(String, String, Option<String>)> = nav_screens(&pkg)
            .into_iter()
            .map(|s| {
                let text = |key: &str| s.get(key).and_then(Value::as_str).map(str::to_string);
                (text("template").unwrap_or_default(), text("title").unwrap_or_default(), text("icon"))
            })
            .collect();
        let shell = screen_shell(screens.len(), self.frame == PreviewFrame::Phone)?;
        let shown = self.resolved_screen(&pkg).map(|r| r.screen).unwrap_or_default();
        let id = SharedString::from("previewScreens");
        let nav = match shell {
            ScreenShell::Sidebar => v_flex()
                .id(id)
                .w(px(APPS.screens.sidebar_width))
                .flex_shrink_0()
                .h_full()
                .p_2()
                .gap_1()
                .children(screens.into_iter().map(|(template, title, icon)| {
                    let lead = Icon::new(screen_icon(icon.as_deref()))
                        .with_size(gpui_component::Size::Medium)
                        .flex_shrink_0()
                        .into_any_element();
                    let active = template == shown;
                    crate::sidebar::rail_row_lead(SharedString::from(format!("preview-screen-{template}")), lead, title, active, None, cx)
                        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                            this.request_screen(template.clone(), serde_json::Map::new(), cx)
                        }))
                }))
                .into_any_element(),
            ScreenShell::Bar => h_flex()
                .id(id)
                .flex_shrink_0()
                .w_full()
                .justify_center()
                .p_2()
                .child(crate::styleguide::entries::phone_tab_bar::capsule().children(screens.into_iter().map(
                    |(template, title, icon)| {
                        let selected = template == shown;
                        crate::styleguide::entries::phone_tab_bar::screen_tab(screen_icon(icon.as_deref()), title, selected, cx)
                            .id(SharedString::from(format!("preview-screen-{template}")))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                                this.request_screen(template.clone(), serde_json::Map::new(), cx)
                            }))
                    },
                )))
                .into_any_element(),
        };
        Some((shell, nav))
    }

    fn render_preview(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let can_write = self.can_write(cx);
        let (head, published) = self.app.as_ref().map_or((0, 0), |app| (app.head_version, app.published_version));
        let banner = self.viewing.map(|number| {
            h_flex()
                .id(SharedString::from(APPS.test_ids.viewing.clone()))
                .items_center()
                .gap_3()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .px_3()
                .py_2()
                .text_sm()
                .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(viewing_label(number)))
                .when(can_write && number != published, |row| {
                    row.child(
                        Button::new("app-viewing-publish")
                            .primary()
                            .web_sm()
                            .cursor_pointer()
                            .label(APPS.copy.publish.clone())
                            .loading(self.publishing == Some(number))
                            .disabled(self.publishing.is_some())
                            .on_click(cx.listener(move |this, _, window, cx| this.request_publish(number, window, cx))),
                    )
                })
                .when(can_write && number != head, |row| {
                    row.child(
                        Button::new("app-restore")
                            .outline()
                            .web_sm()
                            .cursor_pointer()
                            .label(APPS.copy.restore.clone())
                            .loading(self.restoring)
                            .on_click(cx.listener(move |this, _, _, cx| this.restore(number, cx))),
                    )
                })
                .child(
                    Button::new("app-viewing-head")
                        .ghost()
                        .web_sm()
                        .cursor_pointer()
                        .label(APPS.copy.head.clone())
                        .on_click(cx.listener(|this, _, _, cx| this.clear_viewing(cx))),
                )
        });
        let frame = self.render_surface(true, cx);
        v_flex()
            .size_full()
            .min_h_0()
            .gap_3()
            .p_4()
            .child(self.render_preview_toolbar(cx))
            .children(banner)
            // The frame's row: a flex parent so the frame keeps its height
            // (flex_1) and centres at the Phone width.
            .child(h_flex().flex_1().min_h_0().w_full().items_stretch().justify_center().child(frame))
            .into_any_element()
    }

    /// The preview's surface pane: the HostBanner + the surface (scrolling,
    /// `preview.padding` inside) with the screens shell. `framed` = the
    /// editor's frame (border, radius, the Phone width); VAPP-242 unframed =
    /// THE RUN VIEW, edge to edge, its empty state `copy.runEmpty` with the
    /// app glyph.
    fn render_surface(&self, framed: bool, cx: &mut gpui::Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let host = self.preview.as_ref().map(|preview| preview.ide.host.read(cx));
        // The catalog-update banner (the web's `HostBanner`); a local host
        // has no transport, so no offline state.
        let host_banner = host.and_then(|host| host.unsupported_catalog()).map(|_| {
            let amber = theme::tokens::YELLOW.to_hsla();
            div()
                .rounded_md()
                .border_1()
                .border_color(amber.opacity(0.4))
                .bg(amber.opacity(0.1))
                .px_3()
                .py_2()
                .text_sm()
                .text_color(amber)
                .child("This surface needs a newer Exponential. Update the app to show it.")
        });
        // VAPP-230 (the web's `HostBanner`, first; upstream.json `binding`
        // ONE LOOK): one amber notice per upstream a binding reads without
        // the viewer's connection (the shell's offline banner's tone and
        // `ui-warning`), its ghost `Connect` opening Settings › MCP.
        let upstream_banners = self.preview.as_ref().map(|p| p.ide.upstream_banners()).unwrap_or_default();
        let host_banner = if upstream_banners.is_empty() {
            host_banner.map(IntoElement::into_any_element)
        } else {
            Some(
                v_flex()
                    .gap_2()
                    .children(upstream_banners.into_iter().enumerate().map(|(i, banner)| {
                        let connect = banner.connect;
                        h_flex()
                            .id(("exponential-ui-upstream-connect", i))
                            .w_full()
                            .items_center()
                            .gap_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.warning.opacity(0.4))
                            .px_3()
                            .py_1()
                            .bg(theme.warning.opacity(0.10))
                            .text_color(theme.warning)
                            .child(Icon::new(registry::UI_WARNING).size_4().text_color(theme.warning))
                            .child(div().flex_1().text_sm().child(banner.text))
                            .when(connect, |row| row.child(
                                Button::new(("app-upstream-connect", i))
                                    .ghost()
                                    .small()
                                    .cursor_pointer()
                                    .text_color(theme.warning)
                                    .label(api::apps::upstream_binding::CONNECT)
                                    .on_click(|_, window, cx| {
                                        crate::navigation::navigate(window, cx, crate::navigation::Screen::Settings);
                                        crate::sidebar::select_settings_section(
                                            window,
                                            cx,
                                            crate::settings::SettingsSection::Mcp,
                                        );
                                    }),
                            ))
                    }))
                    .into_any_element(),
            )
        };
        let surface = self.preview.as_ref().and_then(|preview| preview.ide.host.read(cx).surface(&preview.surface_id));
        // EXP-1557 (web parity): `No version yet` is CENTRED in the frame,
        // so it fills the frame itself (a scroll pane's content has no
        // height to centre in).
        // #VAPP-251: a preview exists and its host holds no surface = the host
        // refused the package (its issues are in the log): `copy.needsUpdate`.
        let has_preview = self.preview.is_some();
        let needs_update = surface_state(has_preview, has_preview, surface.is_some()) == SurfaceState::NeedsUpdate;
        let preview_empty = surface.is_none() && (needs_update || self.viewing.is_none());
        let frame_body = match surface {
            Some(surface) => v_flex().w_full().gap_3().children(host_banner).child(surface).into_any_element(),
            None if !needs_update && self.viewing.is_some() => div().into_any_element(),
            None => div()
                .flex_1()
                .w_full()
                .min_h(px(192.))
                .flex()
                .items_center()
                .justify_center()
                // A sized cell: the state's own `w_full` + `max_w` + `mx_auto`
                // lands at the row's end in a flex row wider than its cap.
                .child(div().w(px(448.)).max_w_full().child(if needs_update {
                    let glyph = self.app.as_ref().map_or_else(|| Icon::new(registry::NAV_APPS), AppEntry::icon);
                    crate::controls::empty_state(glyph, APPS.copy.needs_update.clone(), None, cx)
                } else if framed {
                    crate::controls::empty_state(
                        Icon::new(registry::NAV_APPS),
                        APPS.copy.preview_empty.clone(),
                        None,
                        cx,
                    )
                } else {
                    // VAPP-242: never published = `copy.runEmpty`, the app glyph.
                    let glyph = self.app.as_ref().map_or_else(|| Icon::new(registry::NAV_APPS), AppEntry::icon);
                    crate::controls::empty_state(glyph, APPS.copy.run_empty.clone(), None, cx)
                }))
                .into_any_element(),
        };
        let frame = div()
            .id(SharedString::from(if framed { APPS.test_ids.preview.clone() } else { APPS.test_ids.run.clone() }))
            .relative()
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .bg(theme.background)
            .map(|frame| {
                if !framed {
                    // VAPP-242: edge to edge, no frame of its own. gpui's content
                    // mask is rectangular and a child paints over its parent's
                    // border, so the fill keeps the cutout panel's arc and stroke
                    // (shell.rs, the panel), or it squares the panel off.
                    return frame
                        .size_full()
                        .min_w_0()
                        .rounded(px(theme::tokens::radius::LG))
                        .border_1()
                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla());
                }
                let frame = match self.frame.width() {
                    // VAPP-179: Phone = `phoneWidth` px, centred (never wider
                    // than the pane); no flex_1 here, its 0 basis would win
                    // over the width in the row.
                    Some(width) => frame.w(px(width)).max_w_full(),
                    None => frame.flex_1().min_w_0(),
                };
                frame.rounded(px(APPS.preview.radius)).border_1().border_color(theme.border)
            });
        let content = if preview_empty {
            frame_body
        } else {
            crate::scroll_pane::v_scroll_pane(
                "app-preview-scroll",
                &self.preview_scroll,
                div().w_full().p(px(APPS.preview.padding)).child(frame_body),
            )
            .into_any_element()
        };
        // VAPP-218: the package's screens, painted by the host in the frame.
        let frame = match self.render_screens(preview_empty, cx) {
            Some((ScreenShell::Sidebar, nav)) => frame.flex_row().child(nav).child(div().w(px(1.)).h_full().flex_shrink_0().bg(theme.border)).child(
                div().flex_1().min_w_0().h_full().min_h_0().flex().flex_col().child(content),
            ),
            Some((ScreenShell::Bar, nav)) => frame.child(content).child(nav),
            None => frame.child(content),
        };
        frame.into_any_element()
    }
}

impl AppScreenView {
    /// VAPP-177: the Versions tab (web `VersionsTab`): ONE grouped list,
    /// newest first; a row = the `v{n}` pill + the note (`No note` muted)
    /// over a muted `who · relative time`; trailing: the head row's
    /// `Current` badge, every other row's Preview (ghost) + Restore
    /// (outline). None yet = the list-size EmptyState.
    fn render_versions(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let copy = &APPS.copy;
        let pointers = self.app.as_ref().map(AppEntry::versions).unwrap_or(domain::apps::AppVersions { head: 0, published: 0 });
        let can_write = self.can_write(cx);
        let body = if self.versions.is_empty() {
            crate::controls::empty_state_list(Icon::new(registry::UI_HISTORY), copy.versions_empty.clone(), cx)
                .into_any_element()
        } else {
            let muted = cx.theme().muted_foreground;
            let users = Store::try_global(cx).map(|store| store.collections().users.clone());
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let rows: Vec<gpui::Div> = self
                .versions
                .iter()
                .map(|version| {
                    let number = version.number.unwrap_or(0);
                    let author = version
                        .created_by
                        .as_deref()
                        .and_then(|id| users.as_ref()?.read(cx).get(id)?.name.clone());
                    let who = author.unwrap_or_else(|| copy.by_agent.clone());
                    let when = version
                        .created_at
                        .as_deref()
                        .map(|at| crate::comments::relative_time(at, now))
                        .unwrap_or_default();
                    let caption = if when.is_empty() { who } else { format!("{who} · {when}") };
                    // eApps R8 (apps.json `migrationCases`): the record sweep's line.
                    let caption = match domain::apps::version_migration(version.migration_state.as_deref(), version.migration_total.unwrap_or(0), version.migration_done.unwrap_or(0), version.migration_error.as_deref()) {
                        Some(line) => format!("{caption} · {line}"),
                        None => caption,
                    };
                    let note = version.note.as_deref().map(str::trim).filter(|note| !note.is_empty());
                    // eApps V1 (`versionRow`): Published | Current + Publish | Preview + Publish.
                    let kind = version_row(pointers, number);
                    let trailing = if kind == VersionRowKind::Published {
                        crate::surface::glass_pill(
                            ("app-version-published", number as usize),
                            crate::surface::PillSize::Sm,
                            crate::surface::PillMode::Readonly,
                            cx,
                        )
                        .child(copy.published.clone())
                        .into_any_element()
                    } else {
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .map(|row| {
                                if kind == VersionRowKind::Head {
                                    row.child(
                                        crate::surface::glass_pill(
                                            ("app-version-head", number as usize),
                                            crate::surface::PillSize::Sm,
                                            crate::surface::PillMode::Readonly,
                                            cx,
                                        )
                                        .child(copy.head.clone()),
                                    )
                                } else {
                                    row.child(
                                        Button::new(SharedString::from(format!("{}-{number}", APPS.test_ids.version_preview)))
                                            .ghost()
                                            .web_sm()
                                            .cursor_pointer()
                                            .label(copy.preview.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.pick_version(number, cx);
                                                this.set_tab(EditorTab::Preview, cx);
                                            })),
                                    )
                                }
                            })
                            .when(can_write, |row| {
                                row.child(
                                    Button::new(SharedString::from(format!("{}-{number}", APPS.test_ids.version_publish)))
                                        .outline()
                                        .web_sm()
                                        .cursor_pointer()
                                        .label(copy.publish.clone())
                                        .loading(self.publishing == Some(number))
                                        .disabled(self.publishing.is_some())
                                        .on_click(cx.listener(move |this, _, window, cx| this.request_publish(number, window, cx))),
                                )
                            })
                            .into_any_element()
                    };
                    crate::surface::glass_row_shell().child(
                        h_flex()
                            .id(SharedString::from(format!("{}-{number}", APPS.test_ids.version_row)))
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_3()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        h_flex()
                                            .min_w_0()
                                            .items_center()
                                            .gap_2()
                                            .text_sm()
                                            .child(
                                                crate::surface::glass_pill(
                                                    ("app-version-pill", number as usize),
                                                    crate::surface::PillSize::Sm,
                                                    crate::surface::PillMode::Readonly,
                                                    cx,
                                                )
                                                .flex_shrink_0()
                                                .child(version_label(number)),
                                            )
                                            .child(match note {
                                                Some(note) => div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(note.to_string()),
                                                None => div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_color(muted)
                                                    .child(copy.no_note.clone()),
                                            }),
                                    )
                                    .child(div().min_w_0().truncate().text_xs().text_color(muted).child(caption)),
                            )
                            .child(trailing),
                    )
                })
                .collect();
            crate::surface::glass_group_rows(rows).into_any_element()
        };
        crate::scroll_pane::v_scroll_pane(
            "app-versions-scroll",
            &self.versions_scroll,
            div()
                .id(SharedString::from(APPS.test_ids.versions.clone()))
                .w_full()
                .p_4()
                .child(body),
        )
        .into_any_element()
    }

    /// VAPP-177: the Settings tab: the app's settings column (≤
    /// `editorTabs.settingsMaxWidth`, left-aligned), scrollable.
    fn render_settings(&self) -> AnyElement {
        let Some(settings) = self.settings.clone() else {
            return div().into_any_element();
        };
        crate::scroll_pane::v_scroll_pane(
            "app-settings-scroll",
            &self.settings_scroll,
            div().w_full().px_6().py_4().child(settings),
        )
        .into_any_element()
    }

    /// The tab pane on the right of the chat.
    fn render_tab(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        match self.tab {
            EditorTab::Versions => self.render_versions(cx),
            EditorTab::Settings => self.render_settings(),
            EditorTab::Data => self.data.clone().map(|data| v_flex().size_full().min_h_0().child(data).into_any_element()).unwrap_or_else(|| div().into_any_element()),
            EditorTab::Preview | EditorTab::Chat => self.render_preview(cx),
        }
    }
}

impl Render for AppScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AppScreenView").entered();
        let _ = window;
        let Some(app) = self.app.clone() else {
            let ready = Store::try_global(cx).is_some_and(|store| store.collections().apps.read(cx).is_ready());
            if ready {
                return v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .child(crate::controls::empty_state(
                        Icon::new(registry::NAV_APPS),
                        "Not found",
                        None,
                        cx,
                    ))
                    .into_any_element();
            }
            // Skeletons, never "Loading…" (feedback-states.json).
            return v_flex()
                .size_full()
                .px_6()
                .pt_6()
                .child(crate::feedback::skeleton_rows("app-loading", 3, 36., cx))
                .into_any_element();
        };
        // VAPP-242: THE RUN VIEW = the surface pane alone, full screen.
        if self.run {
            return self.render_surface(false, cx);
        }
        let panes = &APPS.panes;
        let chat_share = panes.chat / (panes.chat + panes.preview);
        let tab_pane = v_flex().h_full().min_h_0().min_w_0().flex_1().child(self.render_tab(cx));
        // The chat is built with the app (refresh): the tab pane alone until
        // then. The chat pane has no header strip of its own.
        let body = match self.chat.clone() {
            None => div().flex_1().min_h_0().w_full().child(tab_pane).into_any_element(),
            Some(chat) => h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_stretch()
                .child(
                    v_flex()
                        .h_full()
                        .min_h_0()
                        .w(relative(chat_share))
                        .min_w(px(panes.chat_min_width))
                        .flex_shrink_0()
                        .border_r_1()
                        .border_color(cx.theme().border)
                        .child(div().flex_1().min_h_0().child(self.render_chat(&chat))),
                )
                .child(tab_pane)
                .into_any_element(),
        };
        v_flex()
            .id(SharedString::from(APPS.test_ids.editor.clone()))
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(self.render_header(&app, cx))
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_applies_the_first_template_of_the_package() {
        let pkg: Value = serde_json::from_str(crate::exponential_ui_host::DEVICES_PACKAGE_JSON).unwrap();
        let message = apply_template_message(&pkg, None, "app#1").expect("the Devices package has a template");
        assert_eq!(message["applyTemplate"]["surfaceId"], "app#1");
        assert_eq!(message["applyTemplate"]["templateId"], "devices");
        assert_eq!(message["applyTemplate"]["packageId"], "exponential.devices");
        assert!(apply_template_message(&json!({"id": "x", "templates": {}}), None, "app#1").is_none());
    }

    /// VAPP-179: the picked template while the package has it, else the
    /// first of the SORTED ids (`templateCases`).
    #[test]
    fn the_preview_applies_the_picked_template() {
        let pkg = json!({"id": "x", "templates": {"week": {}, "dashboard": {}, "month": {}}});
        let id = |picked| apply_template_message(&pkg, picked, "app#1").unwrap()["applyTemplate"]["templateId"].clone();
        assert_eq!(id(None), "dashboard");
        assert_eq!(id(Some("week")), "week");
        assert_eq!(id(Some("missing")), "dashboard");
    }

    /// VAPP-218: a screen opens its template with `{params}`; a request
    /// the package lacks falls back to the first screen; icons by eUI name.
    #[test]
    fn the_preview_applies_the_requested_screen() {
        // P2: the samples are signed app FILES; the package rides under `package`.
        let file: Value = serde_json::from_str(include_str!("../../../../../samples/apps/crm.app.json")).unwrap();
        let pkg = file.get("package").cloned().unwrap_or(file);
        let params = json!({"q": "acme"});
        let screen = resolve_screen(&pkg, Some("companies"), Some(&params)).unwrap();
        let message = apply_screen_message(&pkg, &screen, "app#1");
        assert_eq!(message["applyTemplate"]["templateId"], "companies");
        assert_eq!(message["applyTemplate"]["data"], json!({"params": {"q": "acme"}}));
        assert_eq!(resolve_screen(&pkg, Some("gone"), None).unwrap().screen, "pipeline");
        assert_eq!(screen_shell(nav_screens(&pkg).len(), true), Some(ScreenShell::Bar));
        // `ui-icon-placeholder` = CircleDashed (ExpIcon has no PartialEq).
        assert!(matches!(screen_icon(None), crate::icons::ExpIcon::CircleDashed));
        assert!(matches!(screen_icon(Some("no-such-icon")), crate::icons::ExpIcon::CircleDashed));
        assert!(!matches!(screen_icon(Some("users")), crate::icons::ExpIcon::CircleDashed));
    }

    #[test]
    fn a_dev_app_spec_names_its_tab() {
        assert_eq!(split_app_spec("working-hours:versions"), ("working-hours", Some("versions"), None));
        assert_eq!(split_app_spec("working-hours"), ("working-hours", None, None));
        // An unknown or phone-only tab = the default (`editorTabCases`).
        assert_eq!(editor_tab(Some("chat"), false).0, EditorTab::Preview);
        assert_eq!(editor_tab(Some("settings"), false).0, EditorTab::Settings);
        // VAPP-178: `app:<slug>:data` opens the Data tab.
        assert_eq!(split_app_spec("working-hours:data"), ("working-hours", Some("data"), None));
        assert_eq!(editor_tab(Some("data"), false).0, EditorTab::Data);
    }

    /// VAPP-179: `app:<slug>:<tab>:<frame>` (the catalog's
    /// `app-preview-phone` drive = `app:team-dashboard:preview:phone`).
    #[test]
    fn a_dev_app_spec_names_its_frame() {
        let (app, tab, frame) = split_app_spec("team-dashboard:preview:phone");
        assert_eq!((app, tab, frame), ("team-dashboard", Some("preview"), Some("phone")));
        assert_eq!(editor_tab(tab, false).0, EditorTab::Preview);
        assert_eq!(frame.and_then(PreviewFrame::parse), Some(PreviewFrame::Phone));
        assert_eq!(PreviewFrame::default_frame(), PreviewFrame::Desktop);
        assert_eq!(PreviewFrame::parse("tablet"), None);
    }

    /// The seeded Team dashboard package (the app catalog's Stack, Grid,
    /// Card, Progress, Chart, List) installs and paints a surface.
    #[gpui::test]
    fn a_team_package_installs_and_applies(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let pkg: Value = serde_json::from_str(include_str!("app_screen.test-package.json")).unwrap();
        let ide = cx.update(|cx| IdeHost::with_packages(Mode::Dark, std::slice::from_ref(&pkg), None, None, cx));
        let host = ide.host.clone();
        let issues = host.update(cx, |host, cx| {
            let ops = host.receive_trusted(&apply_template_message(&pkg, None, "app#1").unwrap(), cx);
            (ops.len(), format!("{:?}", host.issues()))
        });
        cx.run_until_parked();
        let surface = host.read_with(cx, |host, _| host.surface("app#1"));
        assert!(surface.is_some(), "no surface; ops/issues: {issues:?}");
        drop((surface, host));
        cx.update(|_| drop(ide));
        cx.run_until_parked();
    }

    /// Round 9 §1.4: every re-open mints, and the old surface goes only once
    /// the new one exists, so navigating twice paints the second screen.
    #[gpui::test]
    fn navigating_twice_paints_the_second_screen(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let pkg: Value = serde_json::from_str(include_str!("app_screen.test-package.json")).unwrap();
        let ide = cx.update(|cx| IdeHost::with_packages(Mode::Dark, std::slice::from_ref(&pkg), None, None, cx));
        let host = ide.host.clone();
        let first = cx.update(|cx| open_surface(&host, &pkg, None, None, None, cx));
        cx.run_until_parked();
        assert!(host.read_with(cx, |host, _| host.surface(&first)).is_some());
        let second = cx.update(|cx| open_surface(&host, &pkg, None, None, Some(&first), cx));
        cx.run_until_parked();
        let third = cx.update(|cx| open_surface(&host, &pkg, None, None, Some(&second), cx));
        cx.run_until_parked();
        assert_ne!(first, second);
        assert_ne!(second, third);
        host.read_with(cx, |host, _| {
            assert!(host.surface(&first).is_none());
            assert!(host.surface(&second).is_none());
            assert!(host.surface(&third).is_some(), "the second navigation paints");
        });
        cx.update(|_| drop(ide));
        cx.run_until_parked();
    }
}
