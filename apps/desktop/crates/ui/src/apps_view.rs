//! VAPP-110 — the APPS list (web `apps-list.tsx`, fixtures/apps.json), drawn
//! as Settings → Apps' index (the Agents index's twin; the rail lists the
//! apps as a group like the Agents): the band `Apps` with `New app` as
//! its trailing pill, then one row per app = its glyph (the board set) +
//! name + the version pill (`v{n}`, `No version yet` muted, `Built-in` on
//! the built-in Devices app) + the `…` menu (Rename → the New app form
//! prefilled, Delete → the confirm). The built-in app sorts first and has no
//! menu; members manage only their own apps, owners every app. A row opens
//! the editor ([`Screen::App`]).

use gpui::{
    div, App, ClickEvent, Entity, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement as _,
    Styled, Subscription, Window,
};
use gpui_component::{menu::DropdownMenu as _, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::apps::{sort_apps, version_label, AppListLike, APPS};
use domain::prompts::{Action, Prompt, Role};
use domain::rows::AppRow;

use crate::icons::registry;
use crate::navigation::{active_team_id, Navigation, Screen};

/// One list entry: a synced app, or the BUILT-IN one every client ships
/// (the Devices package: no row, no versions, no menu).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppEntry {
    /// The `apps` row id; the built-in app's slug.
    pub(crate) id: String,
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) head_version: i64,
    pub(crate) builtin: bool,
    pub(crate) row: Option<AppRow>,
}

impl AppListLike for AppEntry {
    fn name(&self) -> &str {
        &self.name
    }
    fn builtin(&self) -> bool {
        self.builtin
    }
}

impl AppEntry {
    pub(crate) fn builtin() -> Self {
        let builtin = &APPS.builtin;
        Self {
            id: builtin.slug.clone(),
            slug: builtin.slug.clone(),
            name: builtin.name.clone(),
            icon: Some(builtin.icon.clone()),
            color: None,
            head_version: 0,
            builtin: true,
            row: None,
        }
    }

    pub(crate) fn from_row(row: &AppRow) -> Self {
        Self {
            id: row.id.clone(),
            slug: row.slug.clone(),
            name: row.name.clone(),
            icon: row.icon.clone(),
            color: row.color.clone(),
            head_version: row.head_version.unwrap_or(0),
            builtin: false,
            row: Some(row.clone()),
        }
    }

    /// The list's glyph (web `BoardGlyph`); the built-in Devices app wears
    /// the Devices nav glyph.
    pub(crate) fn icon(&self) -> Icon {
        if self.builtin {
            return Icon::new(registry::NAV_DEVICES);
        }
        crate::icons::app_icon(self.icon.as_deref(), self.color.as_deref())
    }

    /// The rail row's glyph (web `AppIcon`): the curated set with the
    /// action fallback, in the app's colour; Devices for the built-in app.
    pub(crate) fn rail_icon(&self) -> Icon {
        if self.builtin {
            return Icon::new(registry::NAV_DEVICES);
        }
        let icon = crate::icons::action_icon(self.icon.as_deref());
        match self.color.as_deref().and_then(crate::settings::parse_hex_color) {
            Some(tint) => icon.text_color(tint),
            None => icon,
        }
    }

    /// The version pill's text: `Built-in`, `v{n}` or `No version yet`.
    pub(crate) fn pill_label(&self) -> String {
        if self.builtin {
            APPS.copy.builtin.clone()
        } else {
            version_label(self.head_version)
        }
    }
}

/// The team's apps in list order (the built-in one first, then by name),
/// trashed rows hidden.
pub(crate) fn app_entries(team_id: &str, cx: &App) -> Vec<AppEntry> {
    let mut entries = vec![AppEntry::builtin()];
    if let Some(store) = Store::try_global(cx) {
        entries.extend(
            store
                .collections()
                .apps
                .read(cx)
                .iter()
                .filter(|row| row.team_id.as_deref() == Some(team_id) && row.deleted_at.is_none())
                .map(AppEntry::from_row),
        );
    }
    sort_apps(&mut entries);
    entries
}

/// One app by `apps` row id, or by slug in `team_id` (the built-in slug
/// included); `None` while the shape has no such row.
pub(crate) fn find_app(app: &str, team_id: Option<&str>, cx: &App) -> Option<AppEntry> {
    if app == APPS.builtin.slug {
        return Some(AppEntry::builtin());
    }
    let store = Store::try_global(cx)?;
    let apps = store.collections().apps.read(cx);
    apps.get(app)
        .or_else(|| {
            apps.iter()
                .find(|row| row.slug == app && team_id.is_none_or(|team| row.team_id.as_deref() == Some(team)))
        })
        .filter(|row| row.deleted_at.is_none())
        .map(AppEntry::from_row)
}

/// The version pill (web `<Pill size="sm">`): muted while there is no
/// version.
pub(crate) fn version_pill(id: impl Into<gpui::ElementId>, entry: &AppEntry, cx: &App) -> gpui::AnyElement {
    let pill = crate::surface::glass_pill(id, crate::surface::PillSize::Sm, crate::surface::PillMode::Readonly, cx)
        .child(SharedString::from(entry.pill_label()));
    if entry.builtin || entry.head_version > 0 {
        pill.into_any_element()
    } else {
        pill.text_color(cx.theme().muted_foreground).into_any_element()
    }
}

/// The delete confirm's two answers (web `Prompt`: Cancel, Delete).
static DELETE_ACTIONS: [Action; 2] = [
    Action { id: "cancel", label: "Cancel", role: Role::Cancel },
    Action { id: "delete", label: "Delete", role: Role::Destructive },
];

/// `Delete {name}?` / `Its versions go with it. …`, Return cancels.
pub(crate) fn delete_prompt(name: &str) -> Prompt {
    Prompt {
        id: "delete-app",
        title: domain::apps::delete_title(name),
        body: Some(APPS.copy.delete_body.clone()),
        actions: &DELETE_ACTIONS,
        focus: "cancel",
    }
}

pub(crate) fn confirm_delete(
    app_id: String,
    name: String,
    then: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let opener = window.window_handle();
    let then = std::rc::Rc::new(then);
    let spec = crate::native_dialog::AlertSpec::from_prompt(APPS.copy.menu.delete.clone(), &delete_prompt(&name))
        .on_ok(move |_, cx| {
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                crate::action_error::failed_action("delete", "the app", None, cx);
                return true;
            };
            let app_id = app_id.clone();
            let then = then.clone();
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::apps::delete(&trpc, &app_id) })
                    .await;
                let _ = cx.update(|cx| match result {
                    Ok(_) => {
                        let _ = opener.update(cx, |_, window, cx| then(window, cx));
                    }
                    Err(err) => crate::action_error::failed_action_err("delete", "the app", &err, cx),
                });
            })
            .detach();
            true
        });
    crate::native_dialog::open_alert(window, cx, spec);
}

/// The row's `…` menu: Rename, Delete app.
fn row_menu(index: usize, row: AppRow, cx: &App) -> gpui::AnyElement {
    let copy = &APPS.copy;
    let (rename, delete) = (copy.menu.rename.clone(), copy.menu.delete.clone());
    div()
        .flex_shrink_0()
        // The menu is not the row: a press here never opens the editor.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            crate::controls::ghost_icon_button(("apps-row-menu", index), Icon::new(registry::UI_MORE), cx)
                .dropdown_menu(move |menu, _window, cx| {
                    let rename_row = row.clone();
                    let (delete_id, delete_name) = (row.id.clone(), row.name.clone());
                    menu.item(
                        crate::controls::pointer_label_item(rename.clone(), false)
                            .icon(Icon::new(registry::UI_EDIT))
                            .on_click(move |_: &ClickEvent, window, cx| {
                                crate::app_form_dialog::open(
                                    window,
                                    cx,
                                    crate::app_form_dialog::AppFormTarget::Rename { app: rename_row.clone() },
                                )
                            }),
                    )
                    .item(
                        crate::controls::danger_menu_item(delete.clone(), Icon::new(registry::UI_DELETE), cx)
                            .on_click(move |_: &ClickEvent, window, cx| {
                                confirm_delete(delete_id.clone(), delete_name.clone(), |_, _| {}, window, cx)
                            }),
                    )
                }),
        )
        .into_any_element()
}

/// One app row (web `ListRow`): glyph · name · the version pill · `…`.
fn app_row(index: usize, entry: &AppEntry, manage: bool, to_settings: bool, cx: &App) -> gpui::AnyElement {
    let row_hover = cx.theme().list_hover;
    let target = entry.id.clone();
    // Settings → Apps opens an app's settings page (the built-in app has
    // none: its editor).
    let to_settings = to_settings && !entry.builtin;
    crate::surface::list_row(
        crate::surface::flat_row()
            .id(SharedString::from(format!("{}-{}", APPS.test_ids.row, entry.slug)))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .text_sm()
            .cursor_pointer()
            .hover(move |this| this.bg(row_hover))
            .on_click(move |_: &ClickEvent, window, cx| {
                if to_settings {
                    crate::sidebar::select_settings_section(
                        window,
                        cx,
                        crate::settings::SettingsSection::App(target.clone()),
                    );
                    crate::navigation::navigate(window, cx, Screen::Settings);
                } else {
                    crate::navigation::navigate(window, cx, Screen::App { app: target.clone() });
                }
            })
            .child(entry.icon().small().flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(entry.name.clone())),
            )
            .child(version_pill(("apps-row-pill", index), entry, cx))
            .children(entry.row.clone().filter(|_| manage).map(|row| row_menu(index, row, cx))),
    )
    .into_any_element()
}

/// The page body (the band + the rows, or the empty state); the styleguide
/// draws it too.
pub(crate) fn apps_list(
    team_id: Option<&str>,
    entries: &[AppEntry],
    me: Option<&str>,
    owner: bool,
    to_settings: bool,
    cx: &App,
) -> gpui::Div {
    let new_team = team_id.map(str::to_string);
    let new_app = crate::surface::glass_pill_button(APPS.test_ids.new_app.clone(), crate::surface::PillSize::Sm, cx)
        .icon(registry::UI_ADD)
        .label(APPS.copy.new_app.clone())
        .on_click(move |_, window, cx| {
            if let Some(team_id) = new_team.clone() {
                crate::app_form_dialog::open(window, cx, crate::app_form_dialog::AppFormTarget::New { team_id });
            }
        });
    let band = crate::surface::glass_section_header(APPS.copy.title.clone(), Some(new_app.into_any_element()), cx);
    let column = v_flex().id(SharedString::from(APPS.test_ids.list.clone())).w_full().min_w_0().child(band);
    let body = if entries.is_empty() {
        crate::controls::empty_state_list(Icon::new(registry::NAV_APPS), APPS.copy.empty.clone(), cx).into_any_element()
    } else {
        v_flex()
            .w_full()
            .min_w_0()
            .children(entries.iter().enumerate().map(|(index, entry)| {
                let manage = can_manage(entry, me, owner);
                app_row(index, entry, manage, to_settings, cx)
            }))
            .into_any_element()
    };
    v_flex().w_full().min_w_0().child(column.child(body))
}

/// Whether the viewer manages `entry`: its creator, or a team owner (never
/// the built-in app).
pub(crate) fn can_manage(entry: &AppEntry, me: Option<&str>, owner: bool) -> bool {
    !entry.builtin && (owner || entry.row.as_ref().and_then(|row| row.created_by.as_deref()).is_some_and(|by| Some(by) == me))
}

/// The team's own apps (the built-in one has no settings page): the
/// settings group's rows.
pub(crate) fn team_apps(team_id: &str, cx: &App) -> Vec<AppEntry> {
    app_entries(team_id, cx).into_iter().filter(|entry| !entry.builtin).collect()
}

/// Settings → Apps, the index (the Agents index's twin): the list band.
pub struct AppsIndexPane {
    nav: Entity<Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl AppsIndexPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        if let Some(collections) = Store::try_global(cx).map(|store| store.collections().clone()) {
            subscriptions.push(cx.observe(&collections.apps, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.team_members, |_, _, cx| cx.notify()));
        }
        Self { nav, _subscriptions: subscriptions }
    }
}

impl Render for AppsIndexPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let ready = Store::try_global(cx).is_some_and(|store| store.collections().apps.read(cx).is_ready());
        let team_id = active_team_id(&self.nav, cx);
        match team_id.filter(|_| ready) {
            // Skeletons, never "Loading…" (feedback-states.json).
            None => v_flex().min_w_0().child(crate::feedback::skeleton_rows(
                "apps-skeleton",
                crate::feedback::SKELETON_ROWS,
                44.,
                cx,
            )),
            Some(team_id) => {
                let me = crate::queries::active_account(cx).map(|account| account.user_id);
                let owner = crate::settings::is_owner(cx, &team_id);
                let entries = app_entries(&team_id, cx);
                apps_list(Some(&team_id), &entries, me.as_deref(), owner, true, cx)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: serde_json::Value) -> AppRow {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_builtin_app_leads_and_pills_read_the_fixture() {
        let zeta = AppEntry::from_row(&row(serde_json::json!({"id": "z", "slug": "zeta", "name": "Zeta", "head_version": 3})));
        let alpha = AppEntry::from_row(&row(serde_json::json!({"id": "a", "slug": "alpha", "name": "alpha"})));
        let mut entries = vec![zeta, AppEntry::builtin(), alpha];
        sort_apps(&mut entries);
        let order: Vec<&str> = entries.iter().map(|e| e.slug.as_str()).collect();
        assert_eq!(order, ["devices", "alpha", "zeta"]);
        let pills: Vec<String> = entries.iter().map(AppEntry::pill_label).collect();
        assert_eq!(pills, ["Built-in", "No version yet", "v3"]);
    }

    #[test]
    fn the_delete_prompt_reads_the_fixture() {
        let prompt = delete_prompt("Team dashboard");
        assert_eq!(prompt.title, "Delete Team dashboard?");
        assert_eq!(prompt.body.as_deref(), Some(APPS.copy.delete_body.as_str()));
        assert_eq!(prompt.action("delete").label, APPS.copy.delete);
        assert_eq!(prompt.focused().role, Role::Cancel);
    }
}
