//! VAPP-110: Settings → Apps → one app (web `app-settings-page.tsx`, the
//! agent page's layout): the board form's identity row (icon + colour +
//! name: icon and colour save at once, the name on blur / Enter), the
//! description card (saved on blur), then Open in the editor, the version
//! pill, Export (VAPP-121, once there is a version) and "Delete app" (the creator or an owner). Everyone else reads the
//! page. VAPP-120: under the records line, the Tools group: one row per
//! typed action the head package declares, its title, the function name
//! muted and a switch = the app's grant (`apps.granted_tools`). [`SettingsSection::NewApp`] = the same page in CREATE mode: every
//! field local until Create app, which opens the editor.

use gpui::{
    div, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use domain::app_actions::{action_grantable, action_title, can_grant_actions, declared_actions, APP_ACTIONS_COPY};
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

/// VAPP-120: the typed actions a head package's `functions` declares
/// (patterns expanded, the contract's order); no package = none.
pub(crate) fn declared_tools(package: Option<&serde_json::Value>) -> Vec<String> {
    let functions: Vec<String> = package
        .and_then(|p| p.get("functions"))
        .and_then(serde_json::Value::as_array)
        .map(|list| list.iter().filter_map(|f| f.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    declared_actions(&functions)
}

/// VAPP-120: the whole grant list after one switch (the web's `toggleTool`):
/// on = appended once, off = removed.
pub(crate) fn toggled_grants(granted: &[String], name: &str, on: bool) -> Vec<String> {
    let mut next: Vec<String> = granted.iter().filter(|g| g.as_str() != name).cloned().collect();
    if on {
        next.push(name.to_string());
    }
    next
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
    /// VAPP-122: the team's records quota (`None` = unlimited, or the plan
    /// still loading) and the team it was read for.
    records_max: Option<u64>,
    records_plan_team: Option<String>,
    /// VAPP-120: the tool whose grant is being written (its switch waits).
    tool_busy: Option<String>,
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
            cx.observe(&collections.app_records, |_, _, cx| cx.notify()),
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
            records_max: None,
            records_plan_team: None,
            tool_busy: None,
            _subscriptions: subscriptions,
        }
    }

    fn page(&self, cx: &App) -> Option<Loaded> {
        match self.shared.read(cx).settings_section() {
            // The dev `app:<slug>` seed names a slug: resolve it to the id.
            SettingsSection::App(id) => Some(Loaded::App(
                crate::apps_view::find_app(&id, active_team_id(&self.nav, cx).as_deref(), cx)
                    .map(|app| app.id)
                    .unwrap_or(id),
            )),
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
        if let Some(team_id) = row.and_then(|row| row.team_id.clone()) {
            self.load_records_max(team_id, cx);
        }
    }

    /// VAPP-122: the team's `appRecords` quota off `billing.teamPlan` (the
    /// web's; a failure keeps the count without a max).
    fn load_records_max(&mut self, team_id: String, cx: &mut gpui::Context<Self>) {
        if self.records_plan_team.as_deref() == Some(team_id.as_str()) {
            return;
        }
        self.records_plan_team = Some(team_id.clone());
        self.records_max = None;
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let plan_team = team_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::billing::billing_team_plan(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.records_plan_team.as_deref() != Some(plan_team.as_str()) {
                    return; // superseded
                }
                match result {
                    Ok(plan) => {
                        this.records_max = plan.limits.app_records.filter(|max| max.is_finite()).map(|max| max as u64);
                        cx.notify();
                    }
                    Err(err) => log::warn!("[ui] billing.teamPlan failed: {err}"),
                }
            });
        })
        .detach();
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

    /// VAPP-120 (the 2026-10-24 ruling): only a team OWNER sets the app's
    /// grants; the creator gets no grant power from creating it.
    fn can_grant(&self, row: &AppRow, cx: &App) -> bool {
        row.team_id
            .as_deref()
            .is_some_and(|team| can_grant_actions(super::is_owner(cx, team).then_some(domain::app_actions::GRANT_ROLE)))
    }

    /// VAPP-120: one switch of the Tools group: `apps.update` with the whole
    /// new `granted_tools` list (owners only, never a destructive action); a
    /// refusal toasts.
    fn toggle_tool(&mut self, name: String, on: bool, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.loaded_row(cx) else { return };
        if !self.can_grant(&row, cx) || !action_grantable(&name) || self.tool_busy.is_some() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the app", None, cx);
            return;
        };
        let input = api::apps::AppInput {
            id: Some(row.id.clone()),
            name: row.name.clone(),
            icon: row.icon.clone().unwrap_or_else(|| DEFAULT_ICON.to_string()),
            color: row.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            description: row.description.clone(),
            granted_tools: Some(toggled_grants(&row.granted_tools, &name, on)),
            ..Default::default()
        };
        self.tool_busy = Some(name);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::apps::update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.tool_busy = None;
                if let Err(err) = result {
                    crate::action_error::failed_action_err("save", "the app", &err, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// VAPP-120: the Tools group (the web's `app-settings-tools`): the title
    /// and hint, then one row per declared action with its switch, or the
    /// none line.
    fn tools_group(&self, row: &AppRow, _manage: bool, cx: &mut gpui::Context<Self>) -> gpui::Stateful<gpui::Div> {
        let copy = *APP_ACTIONS_COPY;
        let muted = cx.theme().muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        let declared = declared_tools(row.package.as_ref());
        // The ruling: owners grant; a destructive or owner-only action is never grantable.
        let grant = self.can_grant(row, cx);
        let group = if declared.is_empty() {
            crate::surface::glass_group().child(
                div().px_4().py_3().text_sm().text_color(muted).child(copy.none.clone()),
            )
        } else {
            let rows = declared
                .into_iter()
                .map(|name| {
                    let checked = row.granted_tools.iter().any(|g| g == &name);
                    let busy = self.tool_busy.as_deref() == Some(name.as_str());
                    let toggle = name.clone();
                    crate::surface::glass_row_shell()
                        .justify_between()
                        .gap_3()
                        .child(
                            v_flex()
                                .min_w_0()
                                .child(div().text_sm().truncate().child(action_title(&name)))
                                .child(div().text_xs().font_family(mono.clone()).text_color(muted).truncate().child(name.clone())),
                        )
                        .child(if action_grantable(&name) {
                            crate::controls::web_switch(SharedString::from(format!("app-tool-switch-{name}")))
                                .checked(checked)
                                .disabled(!grant || busy)
                                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                    this.toggle_tool(toggle.clone(), *checked, cx)
                                }))
                                .into_any_element()
                        } else {
                            div().flex_shrink_0().text_sm().text_color(muted).child(copy.always.clone()).into_any_element()
                        })
                })
                .collect::<Vec<_>>();
            crate::surface::glass_group_rows(rows)
        };
        v_flex()
            .id("app-settings-tools")
            .gap_1()
            .child(
                v_flex()
                    .px_1()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(copy.title.clone()))
                    .child(div().text_sm().text_color(muted).child(copy.hint.clone())),
            )
            .child(group)
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
                let export_id = row.id.clone();
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
                    // VAPP-121: any member exports an app that has a version.
                    .when(entry.head_version > 0, |actions| {
                        actions.child(
                            Button::new("app-export")
                                .outline()
                                .cursor_pointer()
                                .web_sm()
                                .icon(Icon::new(registry::UI_SHARE).small())
                                .label(domain::app_export::APP_EXPORT_COPY.export.clone())
                                .on_click(move |_, window, cx| {
                                    crate::app_export_dialog::open(window, cx, export_id.clone(), false)
                                }),
                        )
                    })
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
            .when_some(row.as_ref(), |body, row| {
                // VAPP-122: the app's live records against the quota.
                let count = Store::global(cx)
                    .collections()
                    .app_records
                    .read(cx)
                    .iter()
                    .filter(|record| record.app_id == row.id)
                    .count() as u64;
                body.child(
                    div()
                        .id("app-settings-records")
                        .px_1()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(domain::app_records::records_count_label(count, self.records_max)),
                )
                .child(self.tools_group(row, manage, cx))
            })
            .child(actions);
        v_flex().child(body).into_any_element()
    }
}

use gpui::prelude::FluentBuilder as _;
use gpui::InteractiveElement as _;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_tools_group_lists_the_declared_actions() {
        // VAPP-120: patterns expand, non-actions drop, the contract's order.
        let package = json!({"functions": ["harness.toast", "exponential.issues.update_status", "exponential.comments.create"]});
        assert_eq!(
            declared_tools(Some(&package)),
            vec!["exponential.comments.create".to_string(), "exponential.issues.update_status".to_string()]
        );
        assert!(declared_tools(None).is_empty());
        assert!(declared_tools(Some(&json!({"functions": "nope"}))).is_empty());
        assert_eq!(action_title("exponential.issues.update_status"), "Set issue status");
        assert!(!APP_ACTIONS_COPY.none.is_empty());
    }

    #[test]
    fn a_switch_writes_the_whole_grant_list() {
        let granted = vec!["exponential.comments.create".to_string()];
        assert_eq!(
            toggled_grants(&granted, "exponential.issues.update", true),
            vec!["exponential.comments.create".to_string(), "exponential.issues.update".to_string()]
        );
        assert_eq!(toggled_grants(&granted, "exponential.comments.create", true), granted);
        assert!(toggled_grants(&granted, "exponential.comments.create", false).is_empty());
    }

    #[test]
    fn the_app_row_reads_its_grants_tolerantly() {
        let row: AppRow = serde_json::from_value(json!({"id": "a", "granted_tools": ["exponential.issues.update"]})).unwrap();
        assert_eq!(row.granted_tools, vec!["exponential.issues.update".to_string()]);
        let text: AppRow = serde_json::from_value(json!({"id": "a", "granted_tools": "[\"exponential.issues.get\"]"})).unwrap();
        assert_eq!(text.granted_tools, vec!["exponential.issues.get".to_string()]);
        for garbage in [json!(null), json!(7), json!({"x": 1})] {
            let row: AppRow = serde_json::from_value(json!({"id": "a", "granted_tools": garbage})).unwrap();
            assert!(row.granted_tools.is_empty());
        }
        let absent: AppRow = serde_json::from_value(json!({"id": "a"})).unwrap();
        assert!(absent.granted_tools.is_empty());
    }
}
