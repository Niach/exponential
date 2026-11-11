//! #EXP-1660: Settings › Skills — the desktop twin of the web
//! `settings/skills` pages. The spec is
//! `packages/domain-contract/fixtures/skills.json` (`list`, `page`), its copy
//! and rules through `domain::skills`.
//!
//! #EXP-1629: both pages are eUI documents on the native binding. The list
//! ([`SkillsPane`]) is the "Skills" band ("New skill" its owner-only
//! trailing pill) over one row per skill (the `ui-docs` glyph, the name, the
//! Default / Built-in pill, the description as a one-line caption, the
//! owner's ⋯ with Edit and Delete); a row opens the skill's page
//! ([`SkillPage`]: name + description in one group, the SKILL.md body in
//! the code editor, the "On by default" switch, Save, and the owner's Delete
//! behind THE Prompt). Members read every page; the owner-only controls are
//! hidden or disabled for them (the server enforces the writes).
//!
//! The list is LIVE off the synced `skills` shape (body-less rows); the body
//! comes from `skills.get` when a page opens.

use std::rc::Rc;

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use domain::skills::{self as spec, SkillField};
use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::queries;
use crate::sidebar::select_settings_section;

use super::doc::{empty_state, failed_state, hex, mount, not, path, IdeToasts, Ink, SM, XS};
use super::SettingsSection;

/// The body field when `skills.get` failed.
const SKILL_UNAVAILABLE: &str = "Skill unavailable";

/// The data-model path the panes' projections are written at.
const PATH: &str = "/pane";

/// Open one skill's settings page.
pub(crate) fn open_skill_page(window: &mut Window, cx: &mut App, skill_id: String) {
    select_settings_section(window, cx, SettingsSection::Skill(skill_id));
    navigate(window, cx, Screen::Settings);
}

/// Delete behind THE Prompt (skills.json `page.deleteConfirm`).
fn confirm_delete(skill_id: String, window: &mut Window, cx: &mut App) {
    let alert = AlertSpec::from_prompt(spec::DELETE_LABEL, &spec::delete_confirm()).on_ok(move |_, cx| {
        spawn_skill_delete(cx, skill_id.clone());
        true
    });
    native_dialog::open_alert(window, cx, alert);
}

/// `skills.delete` over tRPC; a failure toasts. The synced collection drops
/// the row and the page falls back to the list.
fn spawn_skill_delete(cx: &mut App, skill_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { api::skills::delete(&trpc, &skill_id) }).await;
        let _ = cx.update(|cx| {
            if let Err(err) = result {
                crate::action_error::failed_action_err("delete", "the skill", &err, cx);
            }
        });
    })
    .detach();
}

/// A pane's handler of a document action: its name, context and payload.
type Act<P> = fn(&mut P, &str, &Value, &Value, &mut Window, &mut gpui::Context<P>);

/// Routes the document actions `names` to the pane, deferred (the binding
/// is mid-update and the pane writes it).
fn route<P: 'static>(binding: &mut Binding<Value>, names: &[&'static str], pane: gpui::WeakEntity<P>, handle: gpui::AnyWindowHandle, act: Act<P>) {
    for &name in names {
        let pane = pane.clone();
        binding.on_action(name, move |_, event, cx| {
            let context = event.context.clone();
            let payload = event.payload.clone().unwrap_or(Value::Null);
            let pane = pane.clone();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    let _ = pane.update(cx, |pane, cx| act(pane, name, &context, &payload, window, cx));
                });
            });
            None
        });
    }
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// The Skills band (web `SkillsListBand`) over the rows.
pub struct SkillsPane {
    nav: Entity<Navigation>,
    #[cfg(test)]
    shot: Option<ListShot>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// The list in a pinned state (the headless store syncs no team).
#[cfg(test)]
pub(crate) struct ListShot {
    pub(crate) team: bool,
    pub(crate) ready: bool,
    pub(crate) owner: bool,
    pub(crate) rows: Vec<domain::rows::SkillRow>,
}

impl SkillsPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.skills.clone(), collections.team_members.clone())
        });
        if let Some((skills, members)) = watched {
            subscriptions.push(cx.observe(&skills, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            route(&mut binding, &["open", "new", "menu"], pane, handle, |pane: &mut Self, name, context, payload, window, cx| pane.action(name, context, payload, window, cx));
            binding
        });
        let (list_hover, skeleton_fill) = (hex(cx.theme().list_hover), hex(cx.theme().muted_foreground.opacity(0.18)));
        let view = mount("settings-skills", list_document(&Ink::of(cx), &list_hover, &skeleton_fill), &binding, window, cx);
        Self {
            nav,
            #[cfg(test)]
            shot: None,
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        Self::new(nav, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: ListShot, cx: &mut gpui::Context<Self>) {
        self.shot = Some(shot);
        cx.notify();
    }

    /// `open` (a row), `new` (New skill), `menu` (the ⋯ pick on `key`).
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let key = context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
        match name {
            "open" if !key.is_empty() => open_skill_page(window, cx, key),
            "new" => {
                select_settings_section(window, cx, SettingsSection::NewSkill);
                navigate(window, cx, Screen::Settings);
            }
            "menu" => match payload.get("value").and_then(Value::as_str) {
                Some("edit") => open_skill_page(window, cx, key),
                Some("delete") => confirm_delete(key, window, cx),
                _ => {}
            },
            _ => {}
        }
    }

    /// The team's rows, whether the shape is ready, and the owner flag.
    fn read(&self, cx: &App) -> Option<(Vec<domain::rows::SkillRow>, bool, bool)> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.then(|| (shot.rows.clone(), shot.ready, shot.owner));
        }
        let team_id = active_team_id(&self.nav, cx)?;
        let (rows, ready) = queries::team_skills(cx, &team_id);
        Some((rows, ready, super::is_owner(cx, &team_id)))
    }

    fn project(&self, cx: &App) -> Value {
        let Some((rows, ready, owner)) = self.read(cx) else {
            return json!({"noTeam": true, "loading": false, "empty": false, "ready": false, "owner": false, "rows": []});
        };
        json!({
            "noTeam": false,
            "loading": rows.is_empty() && !ready,
            "empty": rows.is_empty() && ready,
            "ready": !rows.is_empty(),
            "owner": owner,
            "rows": rows.iter().map(|skill| {
                let pill = spec::skill_pill(skill.builtin, skill.enabled_by_default);
                json!({
                    "key": skill.id,
                    "name": skill.name,
                    // An empty caption keeps its line, as the IDE's empty run does.
                    "description": if skill.description.is_empty() { "\u{a0}".to_string() } else { skill.description.clone() },
                    "hasPill": pill.is_some(),
                    "pill": pill.unwrap_or_default(),
                    "canEdit": spec::can_edit_skill(skill.builtin, owner),
                    "menuLabel": format!("Skill menu for {}", skill.name),
                    "items": [
                        {"label": "Edit", "value": "edit", "icon": "ui-edit"},
                        {"kind": "separator"},
                        {"label": spec::DELETE_CONFIRM, "value": "delete", "icon": "ui-delete", "destructive": true},
                    ],
                })
            }).collect::<Vec<_>>(),
        })
    }
}

/// One list row (the MCP server row's layout): the 14 px muted glyph, the
/// name (`text_sm` MEDIUM, one line) and its pill over the `text_xs` muted
/// one-line description; the owner's ⋯ trailing (never on a built-in).
fn list_row(ink: &Ink, list_hover: &str) -> Value {
    let mut name = ink.text("row_name", path("name"), SM, &ink.foreground);
    name["props"]["lines"] = json!(1);
    name["style"]["fontWeight"] = json!(500);
    name["style"]["minWidth"] = json!(0);
    name["style"]["flexShrink"] = json!(1);
    let mut pill = ink.pill("row_pill", path("pill"), false, None);
    pill["visible"] = path("hasPill");
    let title = json!({"id": "row_title", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5)}, "children": [name, pill]});
    let mut description = ink.text("row_description", path("description"), XS, &ink.muted);
    description["props"]["lines"] = json!(1);
    description["style"]["minWidth"] = json!(0);
    // a11y: named by the caller ("Skill menu for {name}", the node's accessibility label).
    let mut more = ink.ghost_icon_button("row_more", "ui-more", path("menuLabel"), Value::Null, list_hover);
    more.as_object_mut().expect("the trigger").remove("on");
    let menu = json!({"id": "row_menu", "component": "Menu", "visible": path("canEdit"), "props": {"items": path("items"), "align": "start"}, "on": {"select": {"event": {"name": "menu", "context": {"key": path("key")}}}}, "children": [more]});
    json!({"id": "row", "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "open", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md, "cursor": "pointer", ":hover": {"backgroundColor": list_hover}},
        "children": [
            {"id": "row_icon", "component": "Icon", "props": {"name": "ui-docs"}, "style": {"width": 14, "height": 14, "color": ink.muted_glyph, "flexShrink": 0}},
            {"id": "row_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "gap": ink.r(0.25)}, "children": [title, description]},
            menu,
        ]})
}

/// The list's tree: the band (New skill for owners) over the rows, the
/// skeleton or the empty state; no team = the no-team state.
fn list_document(ink: &Ink, list_hover: &str, skeleton_fill: &str) -> Value {
    let [mut new_skill, _] = ink.pill_button("new", Some("ui-add"), json!(spec::NEW_BUTTON), json!(false), json!({"name": "new"}));
    // Beside a glyph the IDE's pill label sits on the lower device pixel.
    new_skill["children"][1]["style"]["paddingTop"] = json!(1);
    let new_skill = json!({"id": "new_slot", "component": "Box", "visible": path("/pane/owner"), "style": {"display": "flex", "flexShrink": 0}, "children": [new_skill]});
    let band = ink.band("band", spec::TITLE, Some(new_skill));
    let rows = json!({"id": "rows", "component": "List", "visible": path("/pane/ready"), "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [list_row(ink, list_hover)]}]});
    let section = json!({"id": "section", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            band,
            ink.skeleton_rows("skeleton", skeleton_fill, path("/pane/loading")),
            empty_state(ink, "empty", "ui-docs", spec::EMPTY_TITLE, path("/pane/empty")),
            rows,
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [section, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

impl Render for SkillsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SkillsPane").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

// ---------------------------------------------------------------------------
// One skill's page
// ---------------------------------------------------------------------------

/// The body read (`skills.get`; synced rows carry none).
#[derive(Clone, Copy, PartialEq)]
enum BodyLoad {
    Loading,
    Ready,
    Failed,
}

/// One skill's page, or "New skill" (`skill_id` None until created).
pub struct SkillPage {
    team_id: String,
    /// `None` = create mode; a create flips it to the new id.
    skill_id: Option<String>,
    /// The header: the live synced name, else "New skill".
    header: String,
    seen_synced: bool,
    /// The form as typed (the fields are the document's; edits arrive as
    /// `edit` actions).
    name: String,
    description: String,
    body: String,
    enabled_by_default: bool,
    /// What the form was seeded with / last saved — the dirty check.
    saved_name: String,
    saved_description: String,
    saved_body: String,
    saved_default: bool,
    load: BodyLoad,
    body_retry_pressed: bool,
    submitting: bool,
    /// Save was pressed once — the create page shows its errors from then on.
    touched: bool,
    /// The server's CONFLICT (the name is taken), under the name.
    name_taken: bool,
    /// #VAPP-213: a built-in: read-only for everyone (`can_edit_skill`).
    builtin: bool,
    #[cfg(test)]
    shot_owner: Option<bool>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

impl SkillPage {
    /// The page's document (its focus, for tests).
    #[cfg(test)]
    pub(crate) fn surface(&self) -> Entity<SurfaceView> {
        self.view.clone()
    }

    pub(crate) fn new(team_id: String, skill: Option<api::skills::Skill>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let creating = skill.is_none();
        let builtin = skill.as_ref().is_some_and(|skill| skill.builtin);
        let (skill_id, seed_name, seed_description, seed_default) = match &skill {
            Some(skill) => (Some(skill.id.clone()), skill.name.clone(), skill.description.clone(), skill.enabled_by_default),
            None => (None, String::new(), String::new(), false),
        };
        let mut subscriptions = Vec::new();
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.skills.clone(), collections.team_members.clone())
        });
        if let Some((skills, members)) = watched {
            subscriptions.push(cx.observe(&skills, |this, _, cx| this.on_rows_changed(cx)));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            route(&mut binding, &["edit", "submit", "toggle", "save", "delete", "retry"], pane, handle, |pane: &mut Self, name, context, payload, window, cx| pane.action(name, context, payload, window, cx));
            binding
        });
        let theme = cx.theme();
        let chrome = PageChrome {
            danger: hex(theme.danger),
            primary: hex(theme.button_primary),
            primary_fg: hex(theme.button_primary_foreground),
            primary_hover: hex(theme.button_primary_hover),
            danger_bg: hex(theme.button_danger),
            danger_fg: hex(theme.button_danger_foreground),
            danger_hover: hex(theme.button_danger_hover),
            radius: f32::from(theme.radius),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
        };
        let view = mount("settings-skill", page_document(&Ink::of(cx), &chrome), &binding, window, cx);
        let mut this = Self {
            team_id,
            header: if creating { spec::NEW_BUTTON.to_string() } else { seed_name.clone() },
            skill_id,
            seen_synced: !creating,
            name: seed_name.clone(),
            description: seed_description.clone(),
            body: String::new(),
            enabled_by_default: seed_default,
            saved_name: seed_name,
            saved_description: seed_description,
            saved_body: String::new(),
            saved_default: seed_default,
            load: if creating { BodyLoad::Ready } else { BodyLoad::Loading },
            body_retry_pressed: false,
            submitting: false,
            touched: false,
            name_taken: false,
            builtin,
            #[cfg(test)]
            shot_owner: None,
            binding,
            view,
            _subscriptions: subscriptions,
        };
        if creating {
            // The name field takes the focus on a NEW page, once the
            // projection has made it.
            let projection = this.project(cx);
            this.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
            super::doc::focus_input(this.view.clone(), "name", window, cx);
        } else {
            this.fetch_body(window, cx);
        }
        this
    }

    /// The page in a pinned state: `body` = the read's outcome (`None` =
    /// still loading, `Some(Err)` = failed).
    #[cfg(test)]
    pub(crate) fn show(&mut self, owner: bool, body: Option<Result<String, ()>>, touched: bool, name_taken: bool, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.shot_owner = Some(owner);
        if self.skill_id.is_some() {
            match body {
                None => self.load = BodyLoad::Loading,
                Some(Ok(body)) => {
                    self.load = BodyLoad::Ready;
                    self.saved_body = body.clone();
                    self.body = body;
                }
                Some(Err(())) => self.load = BodyLoad::Failed,
            }
        }
        self.touched = touched;
        self.name_taken = name_taken;
        cx.notify();
    }

    pub(crate) fn skill_id(&self) -> Option<&str> {
        self.skill_id.as_deref()
    }

    /// The page's skill was deleted: its synced row was seen and a READY
    /// shape no longer has it.
    pub(crate) fn is_gone(&self, cx: &App) -> bool {
        let (Some(id), Some(store)) = (self.skill_id.as_deref(), sync::Store::try_global(cx)) else {
            return false;
        };
        let skills = store.collections().skills.read(cx);
        self.seen_synced && skills.is_ready() && skills.get(id).is_none()
    }

    fn on_rows_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if let (Some(id), Some(store)) = (self.skill_id.as_deref(), sync::Store::try_global(cx)) {
            if let Some(row) = store.collections().skills.read(cx).get(id) {
                self.seen_synced = true;
                self.header = row.name.clone();
            }
        }
        cx.notify();
    }

    fn owner(&self, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(owner) = self.shot_owner {
            return owner;
        }
        super::is_owner(cx, &self.team_id)
    }

    fn read_only(&self, cx: &App) -> bool {
        !spec::can_edit_skill(self.builtin, self.owner(cx))
    }

    /// The ONLY body path: synced rows exclude it.
    fn fetch_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (Some(trpc), Some(skill_id)) = (queries::trpc_client(cx), self.skill_id.clone()) else {
            self.load = BodyLoad::Ready;
            return;
        };
        self.load = BodyLoad::Loading;
        cx.spawn_in(window, async move |this, window| {
            let result = window.background_executor().spawn(async move { api::skills::get(&trpc, &skill_id) }).await;
            let _ = this.update_in(window, |view, _, cx| {
                let attended = std::mem::take(&mut view.body_retry_pressed);
                match result {
                    Ok(skill) => {
                        view.load = BodyLoad::Ready;
                        view.saved_body = skill.body.clone();
                        view.body = skill.body;
                    }
                    Err(err) => {
                        view.load = BodyLoad::Failed;
                        crate::action_error::failed_load("the skill", attended, crate::action_error::action_error_description(&err), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A document action: `edit` (a field's typed text, `field` names it),
    /// `submit` (Enter in the name), `toggle` (On by default), `save`,
    /// `delete`, `retry` (the body read).
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "edit" => {
                let text = payload.get("value").and_then(Value::as_str).unwrap_or_default().to_string();
                match context.get("field").and_then(Value::as_str) {
                    Some("name") => {
                        self.name = text;
                        self.name_taken = false;
                    }
                    Some("description") => self.description = text,
                    Some("body") => self.body = text,
                    _ => return,
                }
                cx.notify();
            }
            "toggle" => {
                if let Some(on) = payload.get("checked").and_then(Value::as_bool) {
                    self.enabled_by_default = on;
                    cx.notify();
                }
            }
            "submit" | "save" => self.submit(window, cx),
            "delete" => {
                if let Some(skill_id) = self.skill_id.clone() {
                    confirm_delete(skill_id, window, cx);
                }
            }
            "retry" => {
                self.body_retry_pressed = true;
                self.fetch_body(window, cx);
                cx.notify();
            }
            _ => {}
        }
    }

    /// The form as typed: the name is one logical line.
    fn typed(&self) -> (String, String, String) {
        (self.name.replace(['\r', '\n'], " ").trim().to_string(), self.description.trim().to_string(), self.body.clone())
    }

    fn errors(&self) -> Vec<spec::SkillFormError> {
        let (name, description, body) = self.typed();
        spec::validate_skill_form(&name, &description, &body)
    }

    fn dirty(&self) -> bool {
        let (name, description, body) = self.typed();
        self.skill_id.is_none() || name != self.saved_name || description != self.saved_description || body != self.saved_body || self.enabled_by_default != self.saved_default
    }

    fn can_submit(&self) -> bool {
        self.dirty() && self.errors().is_empty() && self.load == BodyLoad::Ready
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.touched = true;
        cx.notify();
        if self.read_only(cx) || self.submitting || !self.can_submit() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let (name, description, body) = self.typed();
        let enabled_by_default = self.enabled_by_default;
        self.submitting = true;
        self.name_taken = false;
        let team_id = self.team_id.clone();
        let skill_id = self.skill_id.clone();
        let sent = (name.clone(), description.clone(), body.clone(), enabled_by_default);
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    match skill_id {
                        Some(id) => api::skills::update(&trpc, &api::skills::SkillUpdate { name: Some(name), description: Some(description), body: Some(body), enabled_by_default: Some(enabled_by_default), ..api::skills::SkillUpdate::new(id) }),
                        None => api::skills::create(&trpc, &api::skills::SkillCreate { team_id, name, description, body, enabled_by_default: Some(enabled_by_default) }),
                    }
                })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                view.submitting = false;
                let creating = view.skill_id.is_none();
                match result {
                    Ok(saved) => {
                        (view.saved_name, view.saved_description, view.saved_body, view.saved_default) = sent;
                        if creating {
                            view.header = saved.name.clone();
                            view.skill_id = Some(saved.id.clone());
                            select_settings_section(window, cx, SettingsSection::Skill(saved.id));
                        }
                    }
                    // skills.json: a CONFLICT reads errors.nameTaken inline.
                    Err(err) if crate::action_error::is_name_conflict(&err) => view.name_taken = true,
                    Err(err) => {
                        let description = crate::action_error::action_error_description(&err);
                        if creating {
                            crate::action_error::failed_action("create", "the skill", description, cx);
                        } else {
                            let form = cx.entity().downgrade();
                            crate::action_error::failed_action_with_retry(
                                "save",
                                "the skill",
                                description,
                                move |window, cx| {
                                    let _ = form.update(cx, |view, cx| view.submit(window, cx));
                                },
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The inline error under `field`, once it may show.
    fn field_error(&self, field: SkillField, errors: &[spec::SkillFormError]) -> Option<&'static str> {
        if field == SkillField::Name && self.name_taken {
            return Some(spec::ERR_NAME_TAKEN);
        }
        (self.touched || self.skill_id.is_some()).then(|| errors.iter().find(|error| error.field == field).map(|error| error.message)).flatten()
    }

    fn project(&self, cx: &App) -> Value {
        let read_only = self.read_only(cx);
        let errors = if read_only { Vec::new() } else { self.errors() };
        let identity_error = self.field_error(SkillField::Name, &errors).or(self.field_error(SkillField::Description, &errors));
        let body_error = self.field_error(SkillField::Body, &errors);
        json!({
            "header": self.header,
            "readOnly": read_only,
            "editable": !read_only,
            "fields": {"name": self.name, "description": self.description, "body": self.body},
            "enabledByDefault": self.enabled_by_default,
            "identityError": identity_error.unwrap_or_default(),
            "hasIdentityError": identity_error.is_some(),
            "bodyError": body_error.unwrap_or_default(),
            "hasBodyError": body_error.is_some(),
            "bodyLoading": self.load == BodyLoad::Loading,
            "bodyFailed": self.load == BodyLoad::Failed,
            "bodyReady": self.load == BodyLoad::Ready,
            "saveOff": !self.can_submit() && !self.submitting,
            "canDelete": self.skill_id.is_some() && !read_only,
        })
    }
}

/// The colours the page needs beyond [`Ink`]: the error ink and the
/// primary / danger `Button`s' paints.
struct PageChrome {
    danger: String,
    primary: String,
    primary_fg: String,
    primary_hover: String,
    danger_bg: String,
    danger_fg: String,
    danger_hover: String,
    radius: f32,
    skeleton: String,
}

/// `action_error::field_error` under a group (`px_1`).
fn field_error(chrome: &PageChrome, id: &str, message: Value, visible: Value) -> Value {
    let size = crate::action_error::FIELD_ERROR_PX;
    json!({"id": id, "component": "Text", "visible": visible, "props": {"text": message}, "style": {"fontSize": size, "lineHeight": 20, "color": chrome.danger, "paddingHorizontal": 3.5}})
}

/// A `web_sm` `Button` (32 px, `px 12`, `text_sm`, `gap_1`, a `size_3p5`
/// glyph, the theme radius) in a solid variant, its glyph optional.
#[allow(clippy::too_many_arguments)]
fn sm_button(ink: &Ink, id: &str, icon: Option<&str>, label: &str, fill: &str, hover: &str, fg: &str, event: &str, radius: f32) -> Value {
    let glyph = icon.map(|icon| json!({"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": ink.r(SM), "height": ink.r(SM), "color": fg, "flexShrink": 0}}));
    let text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": label}, "style": {"fontSize": ink.r(SM), "lineHeight": ink.r(SM), "color": fg, "flexShrink": 0}});
    json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": {"name": event}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": radius, "backgroundColor": fill, "cursor": "pointer", ":hover": {"backgroundColor": hover}},
        "children": glyph.into_iter().chain([text]).collect::<Vec<_>>()})
}

/// The page's tree: the header band, then (`gap_3`) the name + description
/// group, the body group, the On by default row and Save; `gap_4` before the
/// owner's Delete.
fn page_document(ink: &Ink, chrome: &PageChrome) -> Value {
    let mut band = ink.band("band", "", None);
    band["children"][0]["props"]["text"] = path("/pane/header");
    let glyph = ink.r(1.);
    let name = json!({"id": "name", "component": "Input", "props": {"label": spec::NAME_LABEL, "name": "name", "variant": "code", "placeholder": spec::NAME_PLACEHOLDER, "value": path("/pane/fields/name"), "disabled": path("/pane/readOnly")},
        "style": {"flexGrow": 1, "flexShrink": 1, "minWidth": 0},
        "on": {"change": {"event": {"name": "edit", "context": {"field": "name"}}}, "submit": {"event": {"name": "submit"}}}});
    let name_row = json!({"id": "name_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.75), "paddingHorizontal": ink.r(1.), "paddingVertical": ink.r(0.75)},
        "children": [{"id": "name_icon", "component": "Icon", "props": {"name": "ui-docs"}, "style": {"width": glyph, "height": glyph, "color": ink.muted_glyph, "flexShrink": 0}}, name]});
    let description = json!({"id": "description", "component": "Textarea", "props": {"label": spec::DESCRIPTION_LABEL, "name": "description", "variant": "row", "rows": 2, "autosize": true, "maxRows": 6, "placeholder": spec::DESCRIPTION_PLACEHOLDER, "value": path("/pane/fields/description"), "disabled": path("/pane/readOnly")},
        "on": {"change": {"event": {"name": "edit", "context": {"field": "description"}}}}});
    let identity = ink.group("identity", vec![name_row, ink.divider("identity_divider", json!(true)), description]);
    let identity = json!({"id": "identity_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [identity, field_error(chrome, "identity_error", path("/pane/identityError"), path("/pane/hasIdentityError"))]});
    let body = json!({"id": "body", "component": "Textarea", "visible": path("/pane/bodyReady"), "props": {"label": spec::BODY_LABEL, "name": "body", "variant": "code", "rows": 14, "autosize": true, "maxRows": 40, "placeholder": spec::BODY_PLACEHOLDER, "value": path("/pane/fields/body"), "disabled": path("/pane/readOnly")},
        "on": {"change": {"event": {"name": "edit", "context": {"field": "body"}}}}});
    // `feedback::skeleton_text`: three bars (280, 220, 160) `gap_3`.
    let bars: Vec<Value> = [280., 220., 160.].iter().enumerate().map(|(ix, w): (usize, &f32)| json!({"id": format!("body_bar_{ix}"), "component": "Skeleton", "props": {"width": w.to_string(), "height": "8"}, "style": {"width": w, "height": 8, "flexShrink": 0, "borderRadius": 4, "backgroundColor": chrome.skeleton}})).collect();
    let loading = json!({"id": "body_loading", "component": "Box", "visible": path("/pane/bodyLoading"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "width": "100%", "minHeight": 256, "padding": ink.r(1.)}, "children": bars});
    let failed = failed_state(ink, "body_failed", "ui-docs", SKILL_UNAVAILABLE, path("/pane/bodyFailed"));
    let body = json!({"id": "body_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [ink.group("body_group", vec![loading, failed, body]), field_error(chrome, "body_error", path("/pane/bodyError"), path("/pane/hasBodyError"))]});
    let toggle = ink.shell("default", false, json!(true), vec![
        json!({"id": "default_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "minWidth": 0}, "children": [ink.text("default_label", json!(spec::DEFAULT_LABEL), SM, &ink.foreground)]}),
        json!({"id": "default_switch", "component": "Switch", "props": {"label": "", "name": "enabledByDefault", "checked": path("/pane/enabledByDefault"), "disabled": path("/pane/readOnly")}, "on": {"change": {"event": {"name": "toggle"}}}}),
    ]);
    let mut save = sm_button(ink, "save", None, spec::SAVE, &chrome.primary, &chrome.primary_hover, &chrome.primary_fg, "save", chrome.radius);
    save["visible"] = not(path("/pane/saveOff"));
    // Disabled (`disabled:opacity-50` on the fill only, the label solid):
    // the solid fill under node opacity, which fades by exactly half.
    let mut save_off = sm_button(ink, "save_off", None, spec::SAVE, "#00000000", "#00000000", &chrome.primary_fg, "save", chrome.radius);
    save_off["visible"] = path("/pane/saveOff");
    save_off["props"]["pressable"] = json!(false);
    save_off["style"]["cursor"] = json!("default");
    save_off["style"]["position"] = json!("relative");
    save_off["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": "save_off_fill", "component": "Box",
        "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": chrome.radius, "backgroundColor": chrome.primary, "opacity": 0.5}}));
    let actions = json!({"id": "actions", "component": "Box", "visible": path("/pane/editable"), "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "gap": ink.r(0.5), "minWidth": 0}, "children": [save, save_off]});
    let form = json!({"id": "form", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "minWidth": 0, "width": "100%"},
        "children": [identity, body, ink.group("default_group", vec![toggle]), actions]});
    let delete = sm_button(ink, "delete", Some("ui-delete"), spec::DELETE_LABEL, &chrome.danger_bg, &chrome.danger_hover, &chrome.danger_fg, "delete", chrome.radius);
    let delete = json!({"id": "delete_row", "component": "Box", "visible": path("/pane/canDelete"), "style": {"display": "flex", "flexDirection": "row"}, "children": [delete]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.), "width": "100%", "minWidth": 0}, "children": [band, form, delete]})
}

impl Render for SkillPage {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SkillPage").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn delete_confirms_with_the_fixture_copy() {
        let prompt = domain::skills::delete_confirm();
        assert_eq!(prompt.title, "Delete this skill?");
        assert_eq!(prompt.body.as_deref(), Some("Sessions that picked it run without it from their next start."));
        let labels: Vec<&str> = prompt.actions.iter().map(|a| a.label).collect();
        assert_eq!(labels, vec!["Cancel", "Delete"]);
        assert_eq!(prompt.focus, "cancel");
    }
}
