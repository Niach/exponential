//! VAPP-110 — the team's apps for the IDE (fixtures/apps.json). VAPP-242:
//! no Apps list page on desktop (wide); the sidebar's APPS group lists the
//! apps (a row RUNS the app, [`Screen::AppRun`]) and its `+` opens THE NEW
//! MENU ([`new_menu_button`]: New app = a draft and its editor at once,
//! [`new_app`]; Import… = the import dialog). Here: the entries (the team's
//! synced apps, by name), the version pill, the delete prompt.

use crate::controls::{PointerMenu, PointerMenuButtonExt as _};
use gpui::{App, ClickEvent, IntoElement, ParentElement, SharedString, Styled, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::apps::{sort_apps, AppListLike, APPS};
use domain::prompts::{Action, Prompt, Role};
use domain::rows::AppRow;

use crate::icons::registry;
use crate::navigation::Screen;

/// One list entry: a synced app.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppEntry {
    /// The `apps` row id.
    pub(crate) id: String,
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) head_version: i64,
    /// eApps V1 (#VAPP-210): the published version (0 = never published).
    pub(crate) published_version: i64,
    pub(crate) row: Option<AppRow>,
}

impl AppListLike for AppEntry {
    fn name(&self) -> &str {
        &self.name
    }
}

impl AppEntry {
    pub(crate) fn from_row(row: &AppRow) -> Self {
        Self {
            id: row.id.clone(),
            slug: row.slug.clone(),
            name: row.name.clone(),
            icon: row.icon.clone(),
            color: row.color.clone(),
            head_version: row.head_version.unwrap_or(0),
            published_version: row.published_version.unwrap_or(0),
            row: Some(row.clone()),
        }
    }

    /// The list's glyph (web `BoardGlyph`).
    pub(crate) fn icon(&self) -> Icon {
        crate::icons::app_icon(self.icon.as_deref(), self.color.as_deref())
    }

    /// The rail row's glyph (web `AppIcon`): the curated set with the
    /// action fallback, in the app's colour.
    pub(crate) fn rail_icon(&self) -> Icon {
        let icon = crate::icons::action_icon(self.icon.as_deref());
        match self.color.as_deref().and_then(crate::settings::parse_hex_color) {
            Some(tint) => icon.text_color(tint),
            None => icon,
        }
    }

    /// eApps V1: the two pointers (fixture `publishCases`).
    pub(crate) fn versions(&self) -> domain::apps::AppVersions {
        domain::apps::AppVersions { head: self.head_version, published: self.published_version }
    }

    /// The version pill's text (`listPill`): the published `v{n}` or `Draft`.
    pub(crate) fn pill_label(&self) -> String {
        domain::apps::list_pill(self.versions())
    }

    /// The head's package: the draft while ahead, else the published one.
    pub(crate) fn head_package(&self) -> Option<serde_json::Value> {
        let row = self.row.as_ref()?;
        row.draft_package.clone().filter(serde_json::Value::is_object).or_else(|| row.package.clone().filter(serde_json::Value::is_object))
    }
}

/// The team's apps in list order (by name), trashed rows hidden.
pub(crate) fn app_entries(team_id: &str, cx: &App) -> Vec<AppEntry> {
    let mut entries = Vec::new();
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

/// One app by `apps` row id, or by slug in `team_id`; `None` while the
/// shape has no such row.
pub(crate) fn find_app(app: &str, team_id: Option<&str>, cx: &App) -> Option<AppEntry> {
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
    if entry.published_version > 0 {
        pill.into_any_element()
    } else {
        pill.text_color(cx.theme().muted_foreground).into_any_element()
    }
}

/// VAPP-176 (fixtures/apps.json NEW APP): EVERY New app entry (the band
/// pill, the sidebar group's `+`, the rail's `+`, the settings nav) calls
/// `apps.create {teamId}` with no other field = a DRAFT ("Untitled app")
/// and opens its editor on the synced row; a failure = the failed-action
/// toast.
pub(crate) fn new_app(team_id: String, window: &mut Window, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action("create", "the app", None, cx);
        return;
    };
    window
        .spawn(cx, async move |window| {
            let result = window
                .background_executor()
                .spawn(async move { api::apps::create_draft(&trpc, &team_id) })
                .await;
            match result {
                Ok(output) => {
                    let app_id = output.app.id;
                    let apps = window.update(|_, cx| Store::global(cx).collections().apps.clone()).ok();
                    if let Some(apps) = apps {
                        crate::queries::await_row_visible(&apps, &app_id, window).await;
                    }
                    let _ = window.update(|window, cx| {
                        crate::navigation::navigate(window, cx, Screen::App { app: app_id });
                    });
                }
                Err(err) => {
                    let _ = window.update(|_, cx| crate::action_error::failed_action_err("create", "the app", &err, cx));
                }
            }
        })
        .detach();
}

/// VAPP-176: `apps.discardDraft {id}`, fire-and-forget (leaving an EMPTY
/// draft's editor); the server deletes it only while it is still empty and
/// the caller created it.
pub(crate) fn discard_draft(app_id: String, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else { return };
    cx.background_executor()
        .spawn(async move {
            let _ = api::apps::discard_draft(&trpc, &app_id);
        })
        .detach();
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

/// VAPP-242 (apps.json THE NEW MENU, testId `newMenu`): the apps `+`
/// opening THE existing menu with `run.newMenu` in order: New app (ui-add)
/// = a draft and its editor, Import… (ui-file) = the import dialog.
pub(crate) fn new_menu_button(team_id: String) -> PointerMenu<Button> {
    Button::new(SharedString::from(APPS.test_ids.new_menu.clone()))
        .ghost()
        .cursor_pointer()
        .xsmall()
        .icon(registry::UI_ADD)
        .tooltip(APPS.copy.new_app.clone())
        .pointer_menu(move |mut menu, _window, _cx| {
            for key in &APPS.run.new_menu {
                let team_id = team_id.clone();
                menu = match key.as_str() {
                    "newApp" => menu.item(
                        crate::controls::pointer_label_item(APPS.copy.new_app.clone(), false)
                            .icon(Icon::new(registry::UI_ADD))
                            .on_click(move |_: &ClickEvent, window, cx| new_app(team_id.clone(), window, cx)),
                    ),
                    "import" => menu.item(
                        crate::controls::pointer_label_item(APPS.copy.import.clone(), false)
                            .icon(Icon::new(registry::UI_FILE))
                            .on_click(move |_: &ClickEvent, window, cx| {
                                crate::app_import_dialog::open(window, cx, team_id.clone())
                            }),
                    ),
                    _ => menu,
                };
            }
            menu
        })
}

/// Whether the viewer manages `entry`: its creator, or a team owner.
pub(crate) fn can_manage(entry: &AppEntry, me: Option<&str>, owner: bool) -> bool {
    owner || entry.row.as_ref().and_then(|row| row.created_by.as_deref()).is_some_and(|by| Some(by) == me)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: serde_json::Value) -> AppRow {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn apps_sort_by_name_and_pills_read_the_fixture() {
        let zeta = AppEntry::from_row(&row(serde_json::json!({"id": "z", "slug": "zeta", "name": "Zeta", "head_version": 4, "published_version": 3})));
        let devices = AppEntry::from_row(&row(serde_json::json!({"id": "d", "slug": "devices", "name": "Devices", "head_version": 1, "published_version": 1})));
        let alpha = AppEntry::from_row(&row(serde_json::json!({"id": "a", "slug": "alpha", "name": "alpha"})));
        let mut entries = vec![zeta, devices, alpha];
        sort_apps(&mut entries);
        let order: Vec<&str> = entries.iter().map(|e| e.slug.as_str()).collect();
        assert_eq!(order, ["alpha", "devices", "zeta"]);
        let pills: Vec<String> = entries.iter().map(AppEntry::pill_label).collect();
        assert_eq!(pills, ["Draft", "v1", "v3"]);
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
