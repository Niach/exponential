//! VAPP-110: Settings → Apps → one app (web `app-settings-page.tsx`, the
//! agent page's layout): the board form's identity row (icon + colour +
//! name: icon and colour save at once, the name on blur / Enter), the
//! description card (saved on blur), then Open in the editor, the version
//! pill and "Delete app" (the creator or an owner). Everyone else reads the
//! page. [`SettingsSection::NewApp`] = the same page in CREATE mode: every
//! field local until Create app, which opens the editor.

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use domain::apps::{APPS, APPS_CAPS};
use domain::rows::AppRow;

use super::{section, SettingsSection};
use crate::apps_view::AppEntry;
use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::sidebar::{select_settings_section, RailShared};

/// The web form's defaults (an app saved without an icon or a colour).
const DEFAULT_ICON: &str = "puzzle";
const DEFAULT_COLOR: &str = "#6366f1";

/// The synced app row, trashed rows excluded.
pub(crate) fn app_row(app_id: &str, cx: &App) -> Option<AppRow> {
    Store::try_global(cx)?
        .collections()
        .apps
        .read(cx)
        .get(app_id)
        .filter(|row| row.deleted_at.is_none())
        .cloned()
}

/// `apps.update` with the row's whole identity, `patch` applied.
fn update_app(cx: &mut App, verb: &'static str, row: &AppRow, patch: impl FnOnce(&mut api::apps::AppInput)) {
    let mut input = api::apps::AppInput {
        id: Some(row.id.clone()),
        name: row.name.clone(),
        icon: row.icon.clone().unwrap_or_else(|| DEFAULT_ICON.to_string()),
        color: row.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
        description: row.description.clone(),
        ..Default::default()
    };
    patch(&mut input);
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action(verb, "the app", None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::apps::update(&trpc, &input) })
            .await;
        if let Err(err) = result {
            let _ = cx.update(|cx| crate::action_error::failed_action_err(verb, "the app", &err, cx));
        }
    })
    .detach();
}

/// Which page the inputs were seeded for: one app, or the create page.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Loaded {
    App(String),
    New,
}

pub struct AppDetailPane {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    name_input: Entity<InputState>,
    description: Entity<TextareaState>,
    /// The create page's local icon and colour (an app's live in its row).
    icon: String,
    color: String,
    loaded: Option<Loaded>,
    pending: crate::feedback::Pending,
    _subscriptions: Vec<Subscription>,
}

impl AppDetailPane {
    pub fn new(
        nav: Entity<Navigation>,
        shared: Entity<RailShared>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let copy = &APPS.copy;
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(copy.name_placeholder.clone()));
        let description = cx.new(|cx| {
            crate::controls::web_textarea(3, 12, window, cx).placeholder(copy.description_placeholder.clone())
        });
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.apps, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Blur => this.commit_rename(cx),
                InputEvent::PressEnter { .. } => this.commit_rename(cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            cx.subscribe(&description, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.commit_description(cx);
                }
            }),
        ];
        Self {
            nav,
            shared,
            name_input,
            description,
            icon: DEFAULT_ICON.to_string(),
            color: DEFAULT_COLOR.to_string(),
            loaded: None,
            pending: crate::feedback::Pending::default(),
            _subscriptions: subscriptions,
        }
    }

    fn page(&self, cx: &App) -> Option<Loaded> {
        match self.shared.read(cx).settings_section() {
            SettingsSection::App(id) => Some(Loaded::App(id)),
            SettingsSection::NewApp => Some(Loaded::New),
            _ => None,
        }
    }

    fn loaded_row(&self, cx: &App) -> Option<AppRow> {
        match &self.loaded {
            Some(Loaded::App(id)) => app_row(id, cx),
            _ => None,
        }
    }

    /// Re-seed the inputs when the page changes (another app, the create
    /// page), flushing the previous app's pending edits first.
    fn sync_page(&mut self, page: &Loaded, row: Option<&AppRow>, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.loaded.as_ref() == Some(page) {
            return;
        }
        if matches!(self.loaded, Some(Loaded::App(_))) {
            self.commit_rename(cx);
            self.commit_description(cx);
        }
        self.loaded = Some(page.clone());
        let (name, description) = row
            .map(|row| (row.name.clone(), row.description.clone().unwrap_or_default()))
            .unwrap_or_default();
        self.icon = DEFAULT_ICON.to_string();
        self.color = DEFAULT_COLOR.to_string();
        self.name_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            if *page == Loaded::New {
                input.focus(window, cx);
            }
        });
        self.description.update(cx, |input, cx| input.set_value(description, window, cx));
    }

    fn can_manage(&self, row: &AppRow, cx: &App) -> bool {
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let owner = row.team_id.as_deref().is_some_and(|team| super::is_owner(cx, team));
        crate::apps_view::can_manage(&AppEntry::from_row(row), me.as_deref(), owner)
    }

    fn commit_rename(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_row(cx) else { return };
        if !self.can_manage(&row, cx) {
            return;
        }
        let drafted = self.name_input.read(cx).value().trim().to_string();
        // A blank name keeps the saved one (the web restores it).
        if drafted.is_empty() || drafted == row.name {
            return;
        }
        let drafted: String = drafted.chars().take(APPS_CAPS.name_max).collect();
        update_app(cx, "save", &row, |input| input.name = drafted);
    }

    fn commit_description(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_row(cx) else { return };
        if !self.can_manage(&row, cx) {
            return;
        }
        let drafted: String = self.description.read(cx).value().chars().take(APPS_CAPS.description_max).collect();
        if drafted == row.description.clone().unwrap_or_default() {
            return;
        }
        update_app(cx, "save", &row, |input| input.description = Some(drafted));
    }

    /// Create app: `apps.create`, then the editor opens on the synced row.
    fn create(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let name = self.name_input.read(cx).value().trim().to_string();
        if name.is_empty() || self.pending.active() {
            return;
        }
        let Some(team_id) = active_team_id(&self.nav, cx) else { return };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("create", "the app", None, cx);
            return;
        };
        let description = self.description.read(cx).value().trim().to_string();
        let input = api::apps::AppInput {
            team_id: Some(team_id),
            name: name.chars().take(APPS_CAPS.name_max).collect(),
            icon: self.icon.clone(),
            color: self.color.clone(),
            description: Some(description).filter(|d| !d.is_empty()),
            ..Default::default()
        };
        self.pending.start(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::apps::create(&trpc, &input) })
                .await;
            match result {
                Ok(output) => {
                    let app_id = output.app.id.clone();
                    let apps = window.update(|_, cx| Store::global(cx).collections().apps.clone()).ok();
                    if let Some(apps) = apps {
                        crate::queries::await_row_visible(&apps, &app_id, window).await;
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        this.pending.finish();
                        // The next visit to New app starts empty.
                        this.loaded = None;
                        navigate(window, cx, Screen::App { app: app_id });
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.pending.finish();
                        crate::action_error::failed_action_err("create", "the app", &err, cx);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// The identity row; on the create page the picks stay local.
    fn identity_row(&self, row: Option<&AppRow>, manage: bool, window: &Window, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let (icon, color) = match row {
            Some(row) => (
                row.icon.clone().unwrap_or_else(|| DEFAULT_ICON.to_string()),
                row.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            ),
            None => (self.icon.clone(), self.color.clone()),
        };
        let view = cx.entity().downgrade();
        let icon_row = row.cloned();
        let icon_picker = crate::board_form::icon_picker(
            "app-detail",
            registry::PICKABLE_ICONS,
            Some(icon.as_str()),
            Some(color.as_str()),
            false,
            move |name, _, cx| {
                let Some(name) = name else { return };
                match &icon_row {
                    Some(row) if manage => update_app(cx, "save", row, |input| input.icon = name.to_string()),
                    Some(_) => {}
                    None => {
                        let _ = view.update(cx, |this, cx| {
                            this.icon = name.to_string();
                            cx.notify();
                        });
                    }
                }
            },
            cx,
        );
        let view = cx.entity().downgrade();
        let color_row = row.cloned();
        let color_picker = crate::board_form::color_picker(
            "app-detail",
            &color,
            move |color, _, cx| match &color_row {
                Some(row) if manage => update_app(cx, "save", row, |input| input.color = color.to_string()),
                Some(_) => {}
                None => {
                    let _ = view.update(cx, |this, cx| {
                        this.color = color.to_string();
                        cx.notify();
                    });
                }
            },
            cx,
        );
        crate::surface::glass_row_shell()
            .gap_2()
            .child(icon_picker)
            .child(color_picker)
            .child(
                div().flex_1().min_w_0().child(
                    crate::surface::glass_row_input(glass_input(&self.name_input, window, cx).disabled(!manage))
                        .text_left(),
                ),
            )
    }
}

impl Render for AppDetailPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(page) = self.page(cx) else {
            return v_flex().into_any_element();
        };
        let row = match &page {
            Loaded::App(id) => match app_row(id, cx) {
                Some(row) => Some(row),
                // The nav clamps a stale App(id) selection away before this
                // pane shows, so a missing row is only a transient frame.
                None => return v_flex().into_any_element(),
            },
            Loaded::New => None,
        };
        self.sync_page(&page, row.as_ref(), window, cx);
        let manage = row.as_ref().is_none_or(|row| self.can_manage(row, cx));
        let copy = &APPS.copy;
        let actions = match &row {
            None => {
                let empty = self.name_input.read(cx).value().trim().is_empty();
                h_flex().pt_2().justify_end().child(self.pending.apply(
                    Button::new("app-create")
                        .primary()
                        .cursor_pointer()
                        .web_sm()
                        .label(copy.create.clone())
                        .disabled(empty && !self.pending.active())
                        .on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
                    &copy.create,
                    cx,
                ))
            }
            Some(row) => {
                let entry = AppEntry::from_row(row);
                let open = row.id.clone();
                let (delete_id, delete_name) = (row.id.clone(), row.name.clone());
                h_flex()
                    .pt_2()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("app-open-editor")
                            .outline()
                            .cursor_pointer()
                            .web_sm()
                            .icon(Icon::new(registry::NAV_APPS).small())
                            .label(copy.open_editor.clone())
                            .on_click(move |_, window, cx| navigate(window, cx, Screen::App { app: open.clone() })),
                    )
                    .child(crate::apps_view::version_pill("app-detail-version", &entry, cx))
                    .child(div().flex_1())
                    .when(manage, |actions| {
                        actions.child(
                            Button::new(SharedString::from(format!("app-detail-delete-{}", row.id)))
                                .danger()
                                .cursor_pointer()
                                .web_sm()
                                .icon(Icon::new(registry::UI_DELETE).small())
                                .label(copy.menu.delete.clone())
                                .on_click(move |_, window, cx| {
                                    crate::apps_view::confirm_delete(
                                        delete_id.clone(),
                                        delete_name.clone(),
                                        |window, cx| {
                                            select_settings_section(window, cx, SettingsSection::Apps);
                                            navigate(window, cx, Screen::Settings);
                                        },
                                        window,
                                        cx,
                                    )
                                }),
                        )
                    })
            }
        };
        let body = section(cx)
            .id("app-settings-page")
            .gap_4()
            .child(crate::surface::glass_group_rows(vec![self.identity_row(row.as_ref(), manage, window, cx)]))
            .child(
                crate::surface::glass_group().child(
                    Textarea::new(&self.description)
                        .appearance(false)
                        .disabled(!manage)
                        .w_full()
                        .px_4()
                        .py_3(),
                ),
            )
            .child(actions);
        v_flex().child(body).into_any_element()
    }
}

use gpui::prelude::FluentBuilder as _;
use gpui::InteractiveElement as _;
