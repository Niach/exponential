//! VAPP-110: Settings → Apps → one app (the agent page's twin): the board
//! form's identity row (icon + colour + name: icon and colour save at once,
//! the name on blur / Enter), the description (saved on blur), and "Delete
//! app" behind the apps fixture's confirm. The creator's and the owners'
//! page (the Apps group lists only the apps the viewer manages).

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex,
};
use sync::Store;

use domain::apps::{APPS, APPS_CAPS};
use domain::rows::AppRow;

use super::{section, SettingsSection};
use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::sidebar::RailShared;

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

pub struct AppDetailPane {
    shared: Entity<RailShared>,
    name_input: Entity<InputState>,
    description: Entity<TextareaState>,
    /// The app the inputs were last seeded from.
    loaded_app: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl AppDetailPane {
    pub fn new(shared: Entity<RailShared>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let copy = &APPS.copy;
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(copy.name_placeholder.clone()));
        let description = cx.new(|cx| {
            crate::controls::web_textarea(3, 12, window, cx).placeholder(copy.description_placeholder.clone())
        });
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.apps, |_, _, cx| cx.notify()),
            cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.commit_rename(cx);
                }
            }),
            cx.subscribe(&description, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.commit_description(cx);
                }
            }),
        ];
        Self { shared, name_input, description, loaded_app: None, _subscriptions: subscriptions }
    }

    fn selected_app_id(&self, cx: &App) -> Option<String> {
        match self.shared.read(cx).settings_section() {
            SettingsSection::App(id) => Some(id),
            _ => None,
        }
    }

    /// Re-seed the inputs when the selected app changes, flushing the
    /// previous app's pending edits first.
    fn sync_selected_app(&mut self, app: &AppRow, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.loaded_app.as_deref() == Some(app.id.as_str()) {
            return;
        }
        if self.loaded_app.is_some() {
            self.commit_rename(cx);
            self.commit_description(cx);
        }
        self.loaded_app = Some(app.id.clone());
        let (name, description) = (app.name.clone(), app.description.clone().unwrap_or_default());
        self.name_input.update(cx, |input, cx| input.set_value(name, window, cx));
        self.description.update(cx, |input, cx| input.set_value(description, window, cx));
    }

    fn commit_rename(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_app.as_deref().and_then(|id| app_row(id, cx)) else { return };
        let drafted = self.name_input.read(cx).value().trim().to_string();
        // A blank name keeps the saved one (the web form refuses it).
        if drafted.is_empty() || drafted == row.name {
            return;
        }
        let drafted: String = drafted.chars().take(APPS_CAPS.name_max).collect();
        update_app(cx, "rename", &row, |input| input.name = drafted);
    }

    fn commit_description(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_app.as_deref().and_then(|id| app_row(id, cx)) else { return };
        let drafted: String = self.description.read(cx).value().chars().take(APPS_CAPS.description_max).collect();
        if drafted == row.description.clone().unwrap_or_default() {
            return;
        }
        update_app(cx, "save", &row, |input| input.description = Some(drafted));
    }
}

impl Render for AppDetailPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The nav clamps a stale App(id) selection away before this pane
        // shows, so a missing row is only a transient frame.
        let Some(row) = self.selected_app_id(cx).and_then(|id| app_row(&id, cx)) else {
            return v_flex().into_any_element();
        };
        self.sync_selected_app(&row, window, cx);
        let icon_row = row.clone();
        let icon_picker = crate::board_form::icon_picker(
            "app-detail",
            registry::PICKABLE_ICONS,
            Some(row.icon.as_deref().unwrap_or(DEFAULT_ICON)),
            Some(row.color.as_deref().unwrap_or(DEFAULT_COLOR)),
            false,
            move |name, _, cx| {
                let Some(name) = name else { return };
                update_app(cx, "change", &icon_row, |input| input.icon = name.to_string());
            },
            cx,
        );
        let color_row = row.clone();
        let color_picker = crate::board_form::color_picker(
            "app-detail",
            row.color.as_deref().unwrap_or(DEFAULT_COLOR),
            move |color, _, cx| {
                update_app(cx, "change", &color_row, |input| input.color = color.to_string());
            },
            cx,
        );
        let identity_row = crate::surface::glass_row_shell()
            .gap_2()
            .child(icon_picker)
            .child(color_picker)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(crate::surface::glass_row_input(glass_input(&self.name_input, window, cx)).text_left()),
            );
        let (delete_id, delete_name) = (row.id.clone(), row.name.clone());
        let body = section(cx)
            .gap_4()
            .child(crate::surface::glass_group_rows(vec![identity_row]))
            .child(
                crate::surface::glass_group().child(
                    Textarea::new(&self.description).appearance(false).w_full().px_4().py_3(),
                ),
            )
            .child(
                h_flex().child(
                    Button::new(SharedString::from(format!("app-detail-delete-{}", row.id)))
                        .danger()
                        .cursor_pointer()
                        .web_sm()
                        .label(APPS.copy.menu.delete.clone())
                        .on_click(move |_, window, cx| {
                            crate::apps_view::confirm_delete(delete_id.clone(), delete_name.clone(), window, cx)
                        }),
                ),
            );
        v_flex().child(body).into_any_element()
    }
}
