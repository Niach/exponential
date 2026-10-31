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
//!   (`applyTemplate`, surface `app`) in a 12px frame, repainting on every
//!   synced upsert; a previewed older version sits under the `Viewing v{n}`
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

use eui_core::theme::Mode;
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
use sync::Store;

use domain::apps::{
    editor_tab, is_draft, preview_template, preview_templates, version_label, viewing_label, EditorTab, PreviewFrame,
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
pub(crate) fn apply_template_message(pkg: &Value, picked: Option<&str>) -> Option<Value> {
    let template = preview_template(Some(pkg), picked)?;
    Some(json!({
        "version": "v0.9",
        "applyTemplate": {
            "surfaceId": APPS.preview.surface_id,
            "templateId": template,
            "packageId": pkg.get("id").cloned().unwrap_or(Value::Null),
        }
    }))
}

/// The preview's live host: the package it renders (its JSON = the key a
/// new version replaces it on) and the host itself.
struct Preview {
    key: String,
    ide: IdeHost,
    _observe: Subscription,
}

pub struct AppScreenView {
    /// The id or slug the screen was opened with.
    requested: String,
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
    preview: Option<Preview>,
    /// The version being previewed (`None` = the head) and its package.
    viewing: Option<i64>,
    viewing_package: Option<Value>,
    restoring: bool,
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
    _subscriptions: Vec<Subscription>,
}

impl AppScreenView {
    pub fn new(requested: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
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
        // VAPP-177: the settings nav asking this editor for a tab.
        subscriptions.push(cx.observe_global::<AppTabRequest>(|this, cx| {
            let shows = |app: &str| this.shows(app);
            if let Some(tab) = take_tab_request(shows, cx) {
                this.tab = tab;
                cx.notify();
            }
        }));
        // Leaving ends the live editor session: the screens panel drops this
        // view on a navigation off it, the window drops it on close.
        cx.on_release(|this, cx| this.end_live_session(cx)).detach();
        let mut this = Self {
            requested,
            app_id: None,
            left: false,
            nav,
            app: None,
            versions: Vec::new(),
            chat: None,
            preview: None,
            viewing: None,
            viewing_package: None,
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
            _subscriptions: subscriptions,
        };
        this.refresh(window, cx);
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

    fn set_frame(&mut self, frame: PreviewFrame, cx: &mut gpui::Context<Self>) {
        if self.frame != frame {
            self.frame = frame;
            cx.notify();
        }
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
            let requested = crate::navigation::Screen::App { app: self.requested.clone() };
            if id != self.requested && resolved_screen(&self.nav, cx).as_ref() == Some(&requested) {
                cx.defer_in(window, move |_, window, cx| {
                    crate::navigation::navigate_replace(window, cx, crate::navigation::Screen::App { app: id });
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
            if self.chat.is_none() {
                let app_id = app_id.clone();
                self.chat = Some(cx.new(|cx| crate::agent_screen::AgentScreenView::for_app(app_id.clone(), window, cx)));
                self.data = Some(cx.new(|cx| crate::app_data_tab::AppDataTab::new(app_id.clone(), cx)));
                self.settings =
                    Some(cx.new(|cx| crate::settings::app_detail::AppDetailPane::new(app_id, window, cx)));
            }
            if let Some(root) = crate::session_data::my_editor_session(app_id, cx) {
                self.live_root = Some(root.id);
            }
        }
        // The head moved past the version being viewed: it is the head now.
        if self.viewing.is_some() && self.viewing == self.app.as_ref().map(|app| app.head_version) {
            self.viewing = None;
            self.viewing_package = None;
        }
        self.sync_preview(cx);
        cx.notify();
    }

    /// The package the preview shows: the picked version's, else the head's.
    fn shown_package(&self) -> Option<Value> {
        if self.viewing.is_some() {
            return self.viewing_package.clone();
        }
        let app = self.app.as_ref()?;
        app.row.as_ref()?.package.clone().filter(Value::is_object)
    }

    /// A new package = a fresh host (the web keys its `PackageSurface` on
    /// the package), so every synced upsert repaints.
    fn sync_preview(&mut self, cx: &mut gpui::Context<Self>) {
        let pkg = self.shown_package();
        // The key carries the viewing flag: the head's host reads the grants,
        // an older version's never does (a grant is about the HEAD package).
        // VAPP-179: and the template shown (a pick = a fresh host).
        let template = preview_template(pkg.as_ref(), self.picked_template.as_deref()).unwrap_or_default();
        let key = format!(
            "{}:{template}:{}",
            self.viewing.is_some(),
            pkg.as_ref().map(Value::to_string).unwrap_or_default()
        );
        if self.preview.as_ref().map(|p| p.key.as_str()) == Some(key.as_str()) {
            return;
        }
        if let Some(old) = self.preview.take() {
            old.ide.host.update(cx, |host, cx| host.close(cx));
        }
        let Some(pkg) = pkg else { return };
        let theme = pkg.get("theme").and_then(Value::as_str).unwrap_or(THEME_ID).to_string();
        // VAPP-122: the editor's app keeps its records (`exp:records`,
        // `harness.records.*`).
        let scope = self.app.as_ref().map(|app| crate::exponential_ui_host::HostScope {
            app_id: app.id.clone(),
            team_id: app.row.as_ref().and_then(|row| row.team_id.clone()),
            grants: self.viewing.is_none(),
        });
        let ide = IdeHost::with_packages(ide_mode(cx), std::slice::from_ref(&pkg), Some(&theme), scope, cx);
        if let Some(message) = apply_template_message(&pkg, self.picked_template.as_deref()) {
            ide.host.update(cx, |host, cx| {
                host.receive(&message, cx);
            });
        }
        let observe = cx.observe(&ide.host, |_, _, cx| cx.notify());
        self.preview = Some(Preview { key, ide, _observe: observe });
    }

    /// `sessions.end` on the member's live editor root (the tree);
    /// VAPP-176: an EMPTY draft (no editor session) is discarded instead.
    fn end_live_session(&mut self, cx: &mut App) {
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

    /// Preview version `number` (the head clears it); its package loads on
    /// demand.
    fn pick_version(&mut self, number: i64, cx: &mut gpui::Context<Self>) {
        let Some(app) = self.app.clone() else { return };
        if number == app.head_version {
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

    fn render_header(&self, app: &AppEntry, cx: &mut gpui::Context<Self>) -> AnyElement {
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
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child(SharedString::from(app.name.clone())),
            )
            .child(version_pill("app-editor-version", app, cx))
            .child(div().flex_1())
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
        let pkg = self.shown_package();
        let templates = preview_templates(pkg.as_ref());
        let picker = (templates.len() > 1).then(|| {
            let current = preview_template(pkg.as_ref(), self.picked_template.as_deref()).unwrap_or_default();
            let view = cx.entity().downgrade();
            Button::new(SharedString::from(APPS.test_ids.preview_template.clone()))
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
            .child(div().flex_1())
            .children(picker)
            .into_any_element()
    }

    fn render_preview(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
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
                .child(
                    Button::new("app-restore")
                        .primary()
                        .web_sm()
                        .cursor_pointer()
                        .label(APPS.copy.restore.clone())
                        .loading(self.restoring)
                        .on_click(cx.listener(move |this, _, _, cx| this.restore(number, cx))),
                )
                .child(
                    Button::new("app-viewing-head")
                        .ghost()
                        .web_sm()
                        .cursor_pointer()
                        .label(APPS.copy.head.clone())
                        .on_click(cx.listener(|this, _, _, cx| this.clear_viewing(cx))),
                )
        });
        let surface_id = &APPS.preview.surface_id;
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
        let surface = host.and_then(|host| host.surface(surface_id));
        let frame_body = match surface {
            Some(surface) => v_flex().w_full().gap_3().children(host_banner).child(surface).into_any_element(),
            None if self.viewing.is_some() => div().into_any_element(),
            None => div()
                .size_full()
                .min_h(px(192.))
                .flex()
                .items_center()
                .justify_center()
                .child(crate::controls::empty_state(
                    Icon::new(registry::NAV_APPS),
                    APPS.copy.preview_empty.clone(),
                    None,
                    cx,
                ))
                .into_any_element(),
        };
        let frame = div()
            .id(SharedString::from(APPS.test_ids.preview.clone()))
            .relative()
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .map(|frame| match self.frame.width() {
                // VAPP-179: Phone = `phoneWidth` px, centred (never wider
                // than the pane); no flex_1 here, its 0 basis would win
                // over the width in the row.
                Some(width) => frame.w(px(width)).max_w_full(),
                None => frame.flex_1().min_w_0(),
            })
            .rounded(px(APPS.preview.radius))
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(crate::scroll_pane::v_scroll_pane(
                "app-preview-scroll",
                &self.preview_scroll,
                div().w_full().p(px(APPS.preview.padding)).child(frame_body),
            ));
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
}

impl AppScreenView {
    /// VAPP-177: the Versions tab (web `VersionsTab`): ONE grouped list,
    /// newest first; a row = the `v{n}` pill + the note (`No note` muted)
    /// over a muted `who · relative time`; trailing: the head row's
    /// `Current` badge, every other row's Preview (ghost) + Restore
    /// (outline). None yet = the list-size EmptyState.
    fn render_versions(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let copy = &APPS.copy;
        let head = self.app.as_ref().map_or(0, |app| app.head_version);
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
                    let note = version.note.as_deref().map(str::trim).filter(|note| !note.is_empty());
                    let trailing = if number == head {
                        crate::surface::glass_pill(
                            ("app-version-head", number as usize),
                            crate::surface::PillSize::Sm,
                            crate::surface::PillMode::Readonly,
                            cx,
                        )
                        .child(copy.head.clone())
                        .into_any_element()
                    } else {
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .child(
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
                            .child(
                                Button::new(SharedString::from(format!("{}-{number}", APPS.test_ids.version_restore)))
                                    .outline()
                                    .web_sm()
                                    .cursor_pointer()
                                    .label(copy.restore.clone())
                                    .disabled(self.restoring)
                                    .on_click(cx.listener(move |this, _, _, cx| this.restore(number, cx))),
                            )
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
        let message = apply_template_message(&pkg, None).expect("the Devices package has a template");
        assert_eq!(message["applyTemplate"]["surfaceId"], "app");
        assert_eq!(message["applyTemplate"]["templateId"], "devices");
        assert_eq!(message["applyTemplate"]["packageId"], "exponential.devices");
        assert!(apply_template_message(&json!({"id": "x", "templates": {}}), None).is_none());
    }

    /// VAPP-179: the picked template while the package has it, else the
    /// first of the SORTED ids (`templateCases`).
    #[test]
    fn the_preview_applies_the_picked_template() {
        let pkg = json!({"id": "x", "templates": {"week": {}, "dashboard": {}, "month": {}}});
        let id = |picked| apply_template_message(&pkg, picked).unwrap()["applyTemplate"]["templateId"].clone();
        assert_eq!(id(None), "dashboard");
        assert_eq!(id(Some("week")), "week");
        assert_eq!(id(Some("missing")), "dashboard");
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
            let ops = host.receive(&apply_template_message(&pkg, None).unwrap(), cx);
            (ops.len(), format!("{:?}", host.issues()))
        });
        cx.run_until_parked();
        let surface = host.read_with(cx, |host, _| host.surface(&APPS.preview.surface_id));
        assert!(surface.is_some(), "no surface; ops/issues: {issues:?}");
        drop((surface, host));
        cx.update(|_| drop(ide));
        cx.run_until_parked();
    }
}
