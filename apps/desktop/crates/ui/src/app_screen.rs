//! VAPP-110 — one app's EDITOR ([`Screen::App`], web `app-editor.tsx`,
//! fixtures/apps.json). Header = the app's glyph + name + the version pill +
//! `Export` (VAPP-121, once there is a version) + `Versions` (a count pill); body = two panes `5fr : 7fr` with a hairline
//! between: the CHAT (the member's editor session,
//! [`crate::agent_screen::AgentScreenView::for_app`]: the agent channel's
//! rows and THE composer; the first send `sessions.send({appId, text,
//! deviceId})`) beside the live PREVIEW: the IDE's Exponential UI host
//! ([`crate::exponential_ui_host::IdeHost`]) installs the head package and
//! applies its first template (`applyTemplate`, surface `app`) in a 12px
//! frame, repainting on every synced upsert. Versions = a right sheet, rows
//! newest first; a row PREVIEWS that version under the `Viewing v{n}` banner
//! with Restore (`apps.restore` = a NEW head, toast `Restored v{n} as v{m}`).
//!
//! VAPP-129 (apps.json `layout`, kept per device in the settings'
//! `appEditorLayout`): FLOATING = the preview fills the body and the chat is
//! THE Dock's chat card over its bottom edge (the channel view's floating
//! mode, layered over the preview); the card opens the chat as the Dock
//! sheet whose "Move to the side" docks it = DOCKED, the two panes, the
//! chat pane led by a header strip with Minimize, which floats it back.
//!
//! LIFETIME: entering starts nothing; leaving (a navigation off the screen,
//! the window closing) ENDS the member's live editor session
//! (`sessions.end` on the root); the next send continues it.

use eui_core::theme::Mode;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, relative, AnyElement, App, AppContext as _, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};
use serde_json::{json, Value};
use sync::Store;

use domain::apps::{editor_layout, version_label, viewing_label, EditorLayout, APPS};
use domain::rows::AppVersionRow;

use crate::apps_view::{find_app, version_pill, AppEntry};
use crate::controls::WebControl as _;
use crate::exponential_ui_host::{IdeHost, THEME_ID};
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation};

/// The header glyph (the web's `size-5`).
const HEADER_GLYPH_PX: f32 = 20.;

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

/// The first template of a package (`Object.keys(pkg.templates)[0]`).
pub(crate) fn first_template(pkg: &Value) -> Option<String> {
    pkg.get("templates")?.as_object()?.keys().next().cloned()
}

/// The `applyTemplate` message the preview sends (the web's, byte for byte).
pub(crate) fn apply_template_message(pkg: &Value) -> Option<Value> {
    let template = first_template(pkg)?;
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
    /// VAPP-129: floating (the chat in THE Dock) or docked (two panes).
    layout: EditorLayout,
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
        // Leaving ends the live editor session: the screens panel drops this
        // view on a navigation off it, the window drops it on close.
        cx.on_release(|this, cx| this.end_live_session(cx)).detach();
        let mut this = Self {
            requested,
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
            layout: editor_layout(
                crate::coding_flow::CodingHub::global(cx).read(cx).settings.app_editor_layout.as_deref(),
            ),
            _subscriptions: subscriptions,
        };
        this.refresh(window, cx);
        this
    }

    /// VAPP-129: switch the layout, keep it per device (the settings'
    /// `appEditorLayout`), and tell the chat.
    fn set_layout(&mut self, layout: EditorLayout, cx: &mut gpui::Context<Self>) {
        if self.layout == layout {
            return;
        }
        self.layout = layout;
        let hub = crate::coding_flow::CodingHub::global(cx);
        let mut settings = hub.read(cx).settings.clone();
        settings.app_editor_layout = Some(layout.as_str().to_string());
        if let Err(err) = crate::coding_flow::CodingHub::save_settings(&hub, settings, cx) {
            log::warn!("[ui] persisting the app editor layout failed: {err}");
        }
        self.sync_chat_layout(cx);
        cx.notify();
    }

    /// The chat view's mode follows the layout; its sheet's "Move to the
    /// side" docks it.
    fn sync_chat_layout(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(chat) = self.chat.clone() else { return };
        let floating = self.layout == EditorLayout::Floating;
        let this = cx.entity().downgrade();
        let dock_side: std::rc::Rc<dyn Fn(&mut Window, &mut App)> = std::rc::Rc::new(move |_, cx| {
            let _ = this.update(cx, |this, cx| this.set_layout(EditorLayout::Docked, cx));
        });
        chat.update(cx, |chat, cx| chat.set_floating(floating, Some(dock_side), cx));
    }

    /// The id or slug this view was opened with.
    pub(crate) fn requested(&self) -> &str {
        &self.requested
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let team_id = active_team_id(&self.nav, cx);
        self.app = find_app(&self.requested, team_id.as_deref(), cx);
        let app_id = self.app.as_ref().map(|app| app.id.clone());
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
                self.chat = Some(cx.new(|cx| crate::agent_screen::AgentScreenView::for_app(app_id, window, cx)));
                self.sync_chat_layout(cx);
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
        let key = format!("{}:{}", self.viewing.is_some(), pkg.as_ref().map(Value::to_string).unwrap_or_default());
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
        if let Some(message) = apply_template_message(&pkg) {
            ide.host.update(cx, |host, cx| {
                host.receive(&message, cx);
            });
        }
        let observe = cx.observe(&ide.host, |_, _, cx| cx.notify());
        self.preview = Some(Preview { key, ide, _observe: observe });
    }

    /// `sessions.end` on the member's live editor root (the tree).
    fn end_live_session(&mut self, cx: &mut App) {
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
    fn pick_version(&mut self, number: i64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        window.close_sheet(cx);
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

    fn restore(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(number), Some(app)) = (self.viewing, self.app.clone()) else { return };
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

    fn open_versions(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let view = cx.entity().downgrade();
        window.open_sheet(cx, move |sheet, _window, cx| {
            let body = view
                .upgrade()
                .map(|view| {
                    let this = view.read(cx);
                    let head = this.app.as_ref().map(|app| app.head_version).unwrap_or(0);
                    let (versions, viewing) = (this.versions.clone(), this.viewing);
                    version_rows(&versions, head, viewing, view.downgrade(), cx)
                })
                .unwrap_or_else(|| div().into_any_element());
            sheet
                .size(px(448.))
                .title(div().text_base().font_weight(FontWeight::SEMIBOLD).child(APPS.copy.versions.clone()))
                .child(div().id("app-versions-scroll").size_full().overflow_y_scroll().p_4().child(body))
        });
    }

    fn render_header(&self, app: &AppEntry, cx: &mut gpui::Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let versions = {
            let count = self.versions.len();
            Button::new("app-editor-versions-button")
                .outline()
                .web_sm()
                .cursor_pointer()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Icon::new(registry::UI_HISTORY).small())
                        .child(APPS.copy.versions.clone())
                        .when(count > 0, |this| {
                            this.child(
                                crate::surface::glass_pill(
                                    "app-versions-count",
                                    crate::surface::PillSize::Sm,
                                    crate::surface::PillMode::Readonly,
                                    cx,
                                )
                                .child(SharedString::from(count.to_string())),
                            )
                        }),
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_versions(window, cx)))
        };
        // VAPP-121: Export beside Versions, once the app has a version.
        let export = (app.head_version > 0).then(|| {
            let app_id = app.id.clone();
            Button::new("app-editor-export")
                .outline()
                .web_sm()
                .cursor_pointer()
                .icon(Icon::new(registry::UI_SHARE).small())
                .label(domain::app_export::APP_EXPORT_COPY.export.clone())
                .on_click(move |_, window, cx| crate::app_export_dialog::open(window, cx, app_id.clone(), false))
        });
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .px_6()
            .py_4()
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
            .children(export)
            .child(versions)
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
                        .on_click(cx.listener(|this, _, _, cx| this.restore(cx))),
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
            .flex_1()
            .min_h_0()
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
            .children(banner)
            .child(frame)
            .into_any_element()
    }
}

/// The Versions rows (web `VersionRows`): newest first, `v{n}` + the note
/// (muted, `No note`) + `Current` on the head + who · when; the row being
/// previewed (else the head) is lit; a press previews that version.
fn version_rows(
    versions: &[AppVersionRow],
    head: i64,
    viewing: Option<i64>,
    view: WeakEntity<AppScreenView>,
    cx: &mut App,
) -> AnyElement {
    let copy = &APPS.copy;
    if versions.is_empty() {
        return crate::controls::empty_state_list(Icon::new(registry::UI_HISTORY), copy.no_version.clone(), cx)
            .into_any_element();
    }
    let theme = cx.theme();
    let (muted, hover, lit) = (theme.muted_foreground, theme.list_hover, theme.accent.opacity(0.4));
    let users = Store::try_global(cx).map(|store| store.collections().users.clone());
    let rows: Vec<AnyElement> = versions
        .iter()
        .map(|version| {
            let number = version.number.unwrap_or(0);
            let author: Option<(String, Option<String>, Option<String>)> =
                version.created_by.as_deref().and_then(|id| {
                    let user = users.as_ref()?.read(cx).get(id)?.clone();
                    Some((id.to_string(), user.name, user.image))
                });
            let active = viewing.map_or(number == head, |viewing| viewing == number);
            let name = author
                .as_ref()
                .and_then(|(_, name, _)| name.clone())
                .unwrap_or_else(|| copy.by_agent.clone());
            let avatar = author.as_ref().map(|(id, _, image)| {
                crate::user_avatar::user_avatar(id, &name, image.as_deref(), gpui_component::Size::XSmall, cx)
            });
            let note = version
                .note
                .as_deref()
                .map(str::trim)
                .filter(|note| !note.is_empty())
                .unwrap_or(copy.no_note.as_str())
                .to_string();
            let view = view.clone();
            crate::surface::list_row(
                crate::surface::flat_row()
                    .id(SharedString::from(format!("{}-{number}", APPS.test_ids.version_row)))
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .cursor_pointer()
                    .when(active, |row| row.bg(lit))
                    .hover(move |row| row.bg(hover))
                    .on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| this.pick_version(number, window, cx));
                    })
                    .child(div().w(px(40.)).flex_shrink_0().font_weight(FontWeight::MEDIUM).child(version_label(number)))
                    .child(div().flex_1().min_w_0().truncate().text_color(muted).child(note))
                    .when(number == head, |row| {
                        row.child(
                            crate::surface::glass_pill(
                                ("app-version-head", number as usize),
                                crate::surface::PillSize::Sm,
                                crate::surface::PillMode::Readonly,
                                cx,
                            )
                            .child(copy.head.clone()),
                        )
                    })
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_1p5()
                            .text_xs()
                            .text_color(muted)
                            .children(avatar)
                            .child(div().max_w(px(96.)).truncate().child(name))
                            .child("·")
                            .child(crate::thread_screen::clock(version.created_at.as_deref())),
                    ),
            )
            .into_any_element()
        })
        .collect();
    v_flex()
        .id(SharedString::from(APPS.test_ids.versions.clone()))
        .w_full()
        .children(rows)
        .into_any_element()
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
        let body = match (&self.chat, self.layout) {
            // The chat is built with the app (refresh): the preview alone
            // until then.
            (None, _) => div().flex_1().min_h_0().w_full().child(self.render_preview(cx)).into_any_element(),
            (Some(chat), EditorLayout::Floating) => {
                let band = chat.read(cx).band_height(cx);
                div()
                    .id("app-editor-floating")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(div().size_full().pb(px(band + crate::dock::DOCK_GAP)).child(self.render_preview(cx)))
                    .child(div().absolute().top_0().left_0().size_full().child(chat.clone()))
                    .into_any_element()
            }
            (Some(chat), EditorLayout::Docked) => {
                let chat = chat.clone();
                let header = h_flex()
                    .flex_shrink_0()
                    .h(px(panes.header_height))
                    .w_full()
                    .px_4()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(APPS.copy.tabs.chat.clone()))
                    .child(
                        Button::new(SharedString::from(APPS.test_ids.float.clone()))
                            .ghost()
                            .web_icon_sm()
                            .cursor_pointer()
                            .icon(Icon::new(
                                registry::concept_by_name(&APPS.dock_card.float_icon).unwrap_or(registry::UI_INFO),
                            ))
                            .tooltip(APPS.copy.float.clone())
                            .on_click(cx.listener(|this, _, _, cx| this.set_layout(EditorLayout::Floating, cx))),
                    );
                h_flex()
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
                            .child(header)
                            .child(div().flex_1().min_h_0().child(self.render_chat(&chat))),
                    )
                    .child(div().h_full().min_h_0().min_w_0().flex_1().child(self.render_preview(cx)))
                    .into_any_element()
            }
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
        let message = apply_template_message(&pkg).expect("the Devices package has a template");
        assert_eq!(message["applyTemplate"]["surfaceId"], "app");
        assert_eq!(message["applyTemplate"]["templateId"], "devices");
        assert_eq!(message["applyTemplate"]["packageId"], "exponential.devices");
        assert!(apply_template_message(&json!({"id": "x", "templates": {}})).is_none());
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
            let ops = host.receive(&apply_template_message(&pkg).unwrap(), cx);
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
