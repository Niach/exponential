//! NEW-6: Settings → Prompts — the web `settings/prompts` pages
//! (`components/team/prompts-section.tsx` + `components/prompt-form.tsx`).
//!
//! Prompts replaced actions (NEW-2): a name, a curated icon and a markdown
//! body, nothing else. They RUN from the composer's `/` menu; this is the one
//! place they are managed. Built like web: the list ([`PromptsPane`]) is the
//! "Prompts" band ("New prompt" its owner-only trailing pill) over one row per
//! prompt (icon + name); a row opens the prompt's page ([`PromptPage`]: icon +
//! name in ONE row over the body, Save, and the owner's Delete); "New prompt"
//! opens the same page in create mode. Members read every page; the
//! owner-only controls are HIDDEN from them (the server enforces the writes).
//!
//! The list is LIVE off the synced `prompts` shape (body-less rows); the body
//! comes from `prompts.get` when a page opens.

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use gpui_component::ActiveTheme as _;

use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::queries;
use crate::sidebar::select_settings_section;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use serde_json::{json, Value};

use super::doc::{empty_state, failed_state, hex, mount, not, path, IdeToasts, Ink, SM};
use super::SettingsSection;

/// The band's label and the nav entry (web `PROMPTS_TITLE`).
pub(crate) const PROMPTS_TITLE: &str = "Prompts";
/// The band's owner-only pill and the create page's header (web
/// `NEW_PROMPT_LABEL`).
pub(crate) const NEW_PROMPT_LABEL: &str = "New prompt";
/// The list's empty state (web `prompts-section.tsx`).
pub(crate) const PROMPTS_EMPTY: &str = "No prompts yet";
/// The body field when `prompts.get` failed (web `prompt-form.tsx`).
const PROMPT_UNAVAILABLE: &str = "Prompt unavailable";
/// The name field's required error (web `prompt-form.tsx`).
const NAME_REQUIRED: &str = "Enter a name";
/// The server's `MAX_PROMPT_BODY` — a zod `.max`, so CHARACTERS (web
/// `PROMPT_BODY_MAX_CHARS`): over it is an input error under the body.
pub(crate) const PROMPT_BODY_MAX_CHARS: usize = 64 * 1024;

/// The body-too-long error (web `prompt-form.tsx`, verbatim).
pub(crate) fn body_too_long(chars: usize) -> Option<String> {
    (chars > PROMPT_BODY_MAX_CHARS)
        .then(|| format!("Too long: {chars} of {PROMPT_BODY_MAX_CHARS} characters"))
}

/// The submit label (web: `Create prompt` / `Save changes`).
pub(crate) fn submit_label(creating: bool) -> &'static str {
    if creating {
        "Create prompt"
    } else {
        "Save changes"
    }
}

/// The confirm before Delete: the prompts fixture's `delete-action` TITLE
/// alone (`Delete "{name}"?`), web `prompts-section.tsx` parity — a prompt
/// has no triggers or sessions, so the action entry's body does not apply.
fn delete_prompt_confirm(name: &str) -> domain::prompts::Prompt {
    domain::prompts::DELETE_ACTION.render("title", None, &[("name", name)])
}

/// Open one prompt's settings page — the list rows, an entity chip naming a
/// prompt, a pinned prompt's "Edit".
pub(crate) fn open_prompt_page(window: &mut Window, cx: &mut App, prompt_id: String) {
    select_settings_section(window, cx, SettingsSection::Prompt(prompt_id));
    navigate(window, cx, Screen::Settings);
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// The Prompts band (web `PromptsListBand`).
///
/// #EXP-1629: ONE document on eUI's native host binding: the band with the
/// owner's `New prompt` pill over the flat prompt rows, the skeleton until
/// the shape is ready, the empty and no-team states; a row press opens its
/// page.
pub struct PromptsPane {
    nav: Entity<Navigation>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's state (headless has no team or synced rows).
    #[cfg(test)]
    shot: Option<PromptsShot>,
    _subscriptions: Vec<Subscription>,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct PromptsShot {
    /// `None` = no team selected.
    pub team: Option<String>,
    pub owner: bool,
    pub prompts: Vec<api::prompts::Prompt>,
    pub ready: bool,
}

/// The data-model path the list's projection is written at.
const PATH: &str = "/pane";

impl PromptsPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.prompts.clone(), collections.team_members.clone())
        });
        if let Some((prompts, members)) = watched {
            subscriptions.push(cx.observe(&prompts, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }
        // `new` opens the create page, `open` a prompt's page.
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(std::rc::Rc::new(IdeToasts));
            for name in ["new", "open"] {
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| match name {
                            "new" => {
                                select_settings_section(window, cx, SettingsSection::NewPrompt);
                                navigate(window, cx, Screen::Settings);
                            }
                            _ => open_prompt_page(window, cx, key),
                        });
                    });
                    None
                });
            }
            binding
        });
        let (list_hover, skeleton_fill) = (hex(cx.theme().list_hover), hex(cx.theme().muted_foreground.opacity(0.18)));
        let view = mount("settings-prompts", document(&Ink::of(cx), &list_hover, &skeleton_fill), &binding, window, cx);
        Self {
            nav,
            binding,
            view,
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: PromptsShot, cx: &mut gpui::Context<Self>) {
        self.shot = Some(shot);
        cx.notify();
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.clone();
        }
        active_team_id(&self.nav, cx)
    }

    /// The team's prompts, whether the shape is ready, and owner.
    fn rows(&self, team_id: &str, cx: &App) -> (Vec<api::prompts::Prompt>, bool, bool) {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return (shot.prompts.clone(), shot.ready, shot.owner);
        }
        let (prompts, ready) = queries::team_prompts(cx, team_id);
        (prompts, ready, super::is_owner(cx, team_id))
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let Some(team_id) = self.team_id(cx) else {
            return json!({"noTeam": true, "rows": []});
        };
        let (prompts, ready, owner) = self.rows(&team_id, cx);
        let rows: Vec<Value> = prompts
            .iter()
            .map(|prompt| {
                // `icons::action_icon`: a curated name, else the generic glyph.
                let icon = prompt.icon.as_deref().filter(|name| registry::icon_by_name(name).is_some()).unwrap_or("action-default");
                json!({"key": prompt.id, "name": prompt.name, "icon": icon})
            })
            .collect();
        let empty = rows.is_empty();
        json!({"noTeam": false, "owner": owner, "loading": empty && !ready, "empty": empty && ready, "hasRows": !empty, "rows": rows})
    }
}

impl Render for PromptsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "PromptsPane").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        // An ordinary section pane: `SettingsView` owns the scroll.
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The list's tree: the Prompts band (`New prompt` for owners) over the
/// flat rows (EXP-1076: a prompt is an ENTITY: `px_3` / `py_2`, `gap_3`,
/// the small muted glyph, the MEDIUM name on one line, the list hover),
/// the skeleton, the empty state; no team = the no-team state.
fn document(ink: &Ink, list_hover: &str, skeleton_fill: &str) -> Value {
    // Owners only: the pill's own `visible` (it is never disabled).
    let [mut new_prompt, _] = ink.pill_button("new", Some("ui-add"), json!(NEW_PROMPT_LABEL), json!(false), json!({"name": "new"}));
    new_prompt["visible"] = path("/pane/owner");
    // gpui centres the label's line half a point lower than the flex centre.
    new_prompt["children"][1]["style"]["paddingTop"] = json!(1);
    let glyph = ink.r(SM);
    let mut name = ink.text("row_name", path("name"), SM, &ink.foreground);
    name["props"]["lines"] = json!(1);
    name["style"]["fontWeight"] = json!(500);
    name["style"]["minWidth"] = json!(0);
    name["style"]["flexGrow"] = json!(1);
    name["style"]["flexShrink"] = json!(1);
    let row = json!({"id": "row", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": path("name")}, "on": {"press": {"event": {"name": "open", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md, "cursor": "pointer", ":hover": {"backgroundColor": list_hover}},
        "children": [
            {"id": "row_icon", "component": "Icon", "props": {"name": path("icon")}, "style": {"width": glyph, "height": glyph, "color": ink.muted, "flexShrink": 0}},
            name,
        ]});
    let rows = json!({"id": "rows", "component": "List", "visible": path("/pane/hasRows"), "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]});
    let list = json!({"id": "list", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            ink.band("band", PROMPTS_TITLE, Some(new_prompt)),
            empty_state(ink, "empty", "nav-actions", PROMPTS_EMPTY, path("/pane/empty")),
            ink.skeleton_rows("skeleton", skeleton_fill, path("/pane/loading")),
            rows,
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [list, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

// ---------------------------------------------------------------------------
// One prompt's page (web `PromptSettingsPage` + `PromptForm`)
// ---------------------------------------------------------------------------

/// One prompt's page, or "New prompt" (`prompt_id` None until created).
pub struct PromptPage {
    team_id: String,
    /// The prompt on the page; `None` = create mode. A create flips it to the
    /// new id and the page carries on as that prompt's.
    prompt_id: Option<String>,
    /// The header label: the live synced name (an edit mode page), else
    /// "New prompt".
    header: String,
    /// Whether the prompt's synced row has been seen — after that, its
    /// absence from a READY shape means it was deleted.
    seen_synced: bool,
    /// The form as typed (the fields are the document's; edits arrive as
    /// `edit` actions).
    name: String,
    /// The curated registry glyph (the boards set).
    icon: String,
    body: String,
    /// The body's text the field was last given (the read): its bound value.
    shown_body: String,
    /// What the form was seeded with / last saved — the dirty check.
    saved_name: String,
    saved_icon: String,
    saved_body: String,
    /// `prompts.get` in flight — the body field shows its skeleton and Save
    /// stays off, so a save can never blank the body.
    body_loading: bool,
    /// FEED-79: `prompts.get` failed (the field's empty state + Retry);
    /// `body_retry_pressed` = the load in flight is the Retry press, whose
    /// failure toasts.
    body_failed: bool,
    body_retry_pressed: bool,
    submitting: bool,
    /// Save's pending face (it shows after `PENDING_DELAY`).
    pending: crate::feedback::Pending,
    /// Submit was pressed once — the create page shows the required error
    /// only from then on (web `touched`).
    touched: bool,
    /// The duplicate-name CONFLICT — invalid input, under the name.
    name_error: Option<String>,
    /// The bench's owner flag (headless has no membership).
    #[cfg(test)]
    shot_owner: Option<bool>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// A page state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct PageShot {
    pub owner: bool,
    /// The read's outcome: `None` = still loading, `Some(Err)` = failed.
    pub body: Option<Result<String, ()>>,
    /// The Name field's typed text (differs = dirty).
    pub name: Option<String>,
    pub touched: bool,
    pub name_error: Option<String>,
}

/// The data-model path the page's projection is written at.
const PAGE_PATH: &str = "/pane";

impl PromptPage {
    /// The page's document (its focus, for tests).
    #[cfg(test)]
    pub(crate) fn surface(&self) -> Entity<SurfaceView> {
        self.view.clone()
    }

    /// The page over `prompt` (the synced row), or create mode in `team_id`.
    pub(crate) fn new(
        team_id: String,
        prompt: Option<api::prompts::Prompt>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let creating = prompt.is_none();
        let default_icon = domain::contract::BOARD_ICON_VALUES[0].to_string();
        let (prompt_id, seed_name, seed_icon) = match &prompt {
            Some(prompt) => (
                Some(prompt.id.clone()),
                prompt.name.clone(),
                prompt.icon.clone().unwrap_or_else(|| default_icon.clone()),
            ),
            None => (None, String::new(), default_icon),
        };

        let mut subscriptions = Vec::new();
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.prompts.clone(), collections.team_members.clone())
        });
        if let Some((prompts, members)) = watched {
            subscriptions.push(cx.observe(&prompts, |this, _, cx| this.on_rows_changed(cx)));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }
        // Every action leaves the binding for the page (the confirm and the
        // create's navigation need its window).
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PAGE_PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(std::rc::Rc::new(IdeToasts));
            for name in ["edit", "submit", "save", "delete", "retry", "icon"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = event.context.clone();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &context, &payload, window, cx));
                        });
                    });
                    None
                });
            }
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
        let plugin: std::rc::Rc<dyn HostPlugin> = std::rc::Rc::new(super::doc::ide_plugin());
        let view = super::doc::mount_with("settings-prompt", page_document(&Ink::of(cx), &chrome), &binding, plugin, true, window, cx);

        let mut this = Self {
            team_id,
            header: if creating { NEW_PROMPT_LABEL.to_string() } else { seed_name.clone() },
            prompt_id,
            seen_synced: !creating,
            name: seed_name.clone(),
            icon: seed_icon.clone(),
            body: String::new(),
            shown_body: String::new(),
            saved_name: seed_name,
            saved_icon: seed_icon,
            saved_body: String::new(),
            body_loading: !creating,
            body_failed: false,
            body_retry_pressed: false,
            submitting: false,
            pending: crate::feedback::Pending::default(),
            touched: false,
            name_error: None,
            #[cfg(test)]
            shot_owner: None,
            binding,
            view,
            _subscriptions: subscriptions,
        };
        if creating {
            // The name field takes the focus on a NEW page, once the
            // projection has made it.
            this.push(cx);
            super::doc::focus_input(this.view.clone(), "name", window, cx);
        } else {
            this.fetch_body(window, cx);
        }
        this
    }

    /// The prompt this page edits (`None` while it is "New prompt").
    pub(crate) fn prompt_id(&self) -> Option<&str> {
        self.prompt_id.as_deref()
    }

    /// The page's prompt was deleted (by its own Delete or a teammate): its
    /// synced row was seen and a READY shape no longer has it.
    pub(crate) fn is_gone(&self, cx: &App) -> bool {
        let Some(id) = self.prompt_id.as_deref() else {
            return false;
        };
        let Some(store) = sync::Store::try_global(cx) else {
            return false;
        };
        let prompts = store.collections().prompts.read(cx);
        self.seen_synced && prompts.is_ready() && prompts.get(id).is_none()
    }

    /// The synced rows moved: the header follows the live name.
    fn on_rows_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if let (Some(id), Some(store)) = (self.prompt_id.as_deref(), sync::Store::try_global(cx)) {
            if let Some(row) = store.collections().prompts.read(cx).get(id) {
                self.seen_synced = true;
                if let Some(name) = row.name.clone() {
                    self.header = name;
                }
            }
        }
        cx.notify();
    }

    fn read_only(&self, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(owner) = self.shot_owner {
            return !owner;
        }
        !super::is_owner(cx, &self.team_id)
    }

    /// The page in a pinned state.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: PageShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.shot_owner = Some(shot.owner);
        if self.prompt_id.is_some() {
            match shot.body {
                None => self.body_loading = true,
                Some(Ok(body)) => {
                    self.body_loading = false;
                    self.saved_body = body.clone();
                    self.shown_body = body.clone();
                    self.body = body;
                }
                Some(Err(())) => {
                    self.body_loading = false;
                    self.body_failed = true;
                }
            }
        }
        if let Some(name) = shot.name {
            self.name = name;
        }
        self.touched = shot.touched;
        self.name_error = shot.name_error;
        self.push(cx);
        cx.notify();
    }

    /// The ONLY body path: synced rows exclude it.
    fn fetch_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (Some(trpc), Some(prompt_id)) = (queries::trpc_client(cx), self.prompt_id.clone())
        else {
            self.body_loading = false;
            return;
        };
        self.body_loading = true;
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::prompts::get(&trpc, &prompt_id) })
                .await;
            let _ = this.update_in(window, |view, _, cx| {
                let attended = std::mem::take(&mut view.body_retry_pressed);
                match result {
                    Ok(prompt) => {
                        view.body_failed = false;
                        view.saved_body = prompt.body.clone();
                        view.shown_body = prompt.body.clone();
                        view.body = prompt.body;
                    }
                    // FEED-79: the load on open fails quietly (the field's
                    // empty state + Retry); only a failed Retry press toasts.
                    Err(err) => {
                        view.body_failed = true;
                        crate::action_error::failed_load(
                            "the prompt",
                            attended,
                            crate::action_error::action_error_description(&err),
                            cx,
                        );
                    }
                }
                view.body_loading = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// A document action: `edit` (a field's typed text, `field` names it),
    /// `submit` (Enter in the name), `save`, `delete`, `retry` (the body
    /// read).
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "edit" => {
                let text = payload.get("value").and_then(Value::as_str).unwrap_or_default().to_string();
                match context.get("field").and_then(Value::as_str) {
                    Some("name") => {
                        self.name = text;
                        self.name_error = None;
                    }
                    Some("body") => self.body = text,
                    _ => return,
                }
                cx.notify();
            }
            "icon" => {
                if let Some(icon) = payload.get("value").and_then(Value::as_str).filter(|icon| !icon.is_empty()) {
                    self.pick_icon(icon, cx);
                }
            }
            "submit" | "save" => self.submit(window, cx),
            "delete" => self.open_delete(window, cx),
            "retry" => {
                self.body_retry_pressed = true;
                self.fetch_body(window, cx);
                cx.notify();
            }
            _ => {}
        }
    }

    /// The icon picker's pick (the IDE's own picker, `host_leaf`).
    fn pick_icon(&mut self, icon: &str, cx: &mut gpui::Context<Self>) {
        self.icon = icon.to_string();
        cx.notify();
    }

    /// A name is one logical line; pasted newlines collapse (EXP-230).
    fn typed_name(&self) -> String {
        self.name.replace(['\r', '\n'], " ").trim().to_string()
    }

    fn can_submit(&self) -> bool {
        let name = self.typed_name();
        let dirty = self.prompt_id.is_none()
            || name != self.saved_name
            || self.icon != self.saved_icon
            || self.body != self.saved_body;
        dirty
            && !name.is_empty()
            && !self.body.trim().is_empty()
            && body_too_long(self.body.chars().count()).is_none()
            && !self.body_loading
            && !self.body_failed
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
        let name = self.typed_name();
        let icon = self.icon.clone();
        let body = self.body.clone();
        self.submitting = true;
        self.pending.start(cx);
        self.name_error = None;

        let team_id = self.team_id.clone();
        let prompt_id = self.prompt_id.clone();
        let (sent_name, sent_icon, sent_body) = (name.clone(), icon.clone(), body.clone());
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    match prompt_id {
                        Some(id) => {
                            let input = api::prompts::PromptUpdate {
                                name: Some(name),
                                icon: Some(Some(icon)),
                                body: Some(body),
                                ..api::prompts::PromptUpdate::new(id)
                            };
                            api::prompts::update(&trpc, &input)
                        }
                        None => api::prompts::create(&trpc, &team_id, &name, Some(&icon), &body),
                    }
                })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                view.submitting = false;
                view.pending.finish();
                let creating = view.prompt_id.is_none();
                match result {
                    Ok(saved) => {
                        view.saved_name = sent_name;
                        view.saved_icon = sent_icon;
                        view.saved_body = sent_body;
                        // "Create prompt" → the page becomes the new prompt's
                        // (web: navigate to `/settings/prompts/$id`).
                        if creating {
                            view.header = saved.name.clone();
                            view.prompt_id = Some(saved.id.clone());
                            select_settings_section(window, cx, SettingsSection::Prompt(saved.id));
                        }
                    }
                    Err(err) if crate::action_error::is_name_conflict(&err) => {
                        if let api::ApiError::Http { message, .. } = err {
                            view.name_error = Some(message);
                        }
                    }
                    // FEED-79: anything else is the error toast.
                    Err(err) => {
                        let description = crate::action_error::action_error_description(&err);
                        if creating {
                            crate::action_error::failed_action("create", "the prompt", description, cx);
                        } else {
                            let form = cx.entity().downgrade();
                            crate::action_error::failed_action_with_retry(
                                "save",
                                "the prompt",
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

    /// Delete behind the shared confirm; the page leaves for the list once
    /// the write lands.
    fn open_delete(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(prompt_id) = self.prompt_id.clone() else {
            return;
        };
        let spec = AlertSpec::from_prompt("Delete prompt", &delete_prompt_confirm(&self.header))
            .on_ok(move |_, cx| {
                spawn_prompt_delete(cx, prompt_id.clone());
                true
            });
        native_dialog::open_alert(window, cx, spec);
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let read_only = self.read_only(cx);
        let creating = self.prompt_id.is_none();
        let required = (!read_only && (self.touched || !creating) && self.typed_name().is_empty()).then_some(NAME_REQUIRED.to_string());
        let name_error = self.name_error.clone().or(required);
        let too_long = body_too_long(self.body.chars().count());
        let off = !self.can_submit() && !self.submitting;
        let saving = self.pending.shown();
        json!({
            "header": self.header,
            "readOnly": read_only,
            "editable": !read_only,
            "icon": icon_name(&self.icon),
            "pickedIcon": self.icon,
            "fields": {"name": self.name, "body": self.shown_body},
            "hasNameError": name_error.is_some(),
            "nameError": name_error.unwrap_or_default(),
            "hasBodyError": too_long.is_some(),
            "bodyError": too_long.unwrap_or_default(),
            "bodyLoading": self.body_loading,
            "bodyFailed": self.body_failed && !self.body_loading,
            "bodyReady": !self.body_loading && !self.body_failed,
            "submit": submit_label(creating),
            "saveOn": !off && !saving,
            "saveOff": off,
            "saving": saving,
            "canDelete": !creating && !read_only,
        })
    }

    /// Write the projection now.
    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
    }
}

/// `prompts.delete` over tRPC; a failure toasts (the person's own action).
/// The synced collection drops the row and the page falls back to the list.
fn spawn_prompt_delete(cx: &mut App, prompt_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::prompts::delete(&trpc, &prompt_id) })
            .await;
        let _ = cx.update(|cx| {
            if let Err(err) = result {
                crate::action_error::failed_action_err("delete", "the prompt", &err, cx);
            }
        });
    })
    .detach();
}

impl Render for PromptPage {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "PromptPage").entered();
        self.push(cx);
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// `icons::action_icon`'s name: a curated glyph, else the generic one.
fn icon_name(icon: &str) -> &str {
    if registry::icon_by_name(icon).is_some() {
        icon
    } else {
        "action-default"
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
fn sm_button(ink: &Ink, id: &str, icon: Option<Value>, label: Value, fill: &str, hover: &str, fg: &str, event: &str, radius: f32) -> Value {
    let text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": label.clone()}, "style": {"fontSize": ink.r(SM), "lineHeight": ink.r(SM), "color": fg, "flexShrink": 0}});
    json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": {"name": event}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": radius, "backgroundColor": fill, "cursor": "pointer", ":hover": {"backgroundColor": hover}},
        "children": icon.into_iter().chain([text]).collect::<Vec<_>>()})
}

/// The page's tree (`section` `gap_4`): the header band, the form
/// (`gap_3`: the icon + name group with its inline error, the body group
/// with the cap's line, Save), then the owner's Delete.
fn page_document(ink: &Ink, chrome: &PageChrome) -> Value {
    let mut band = ink.band("band", "", None);
    band["children"][0]["props"]["text"] = path("/pane/header");

    // Icon and name are ONE row (web parity): the glyph picker leads, the
    // name types straight into the row; a reader sees the glyph.
    let glyph = ink.r(1.);
    let picker = json!({"id": "icon_picker", "component": "IconPicker", "visible": path("/pane/editable"), "props": {"value": path("/pane/pickedIcon")}, "style": {"flexShrink": 0}, "on": {"change": {"event": {"name": "icon"}}}});
    let reader = json!({"id": "icon_glyph", "component": "Icon", "visible": path("/pane/readOnly"), "props": {"name": path("/pane/icon")}, "style": {"width": glyph, "height": glyph, "color": ink.muted, "flexShrink": 0}});
    let name = json!({"id": "name", "component": "Input", "props": {"label": "Name", "name": "name", "variant": "bare", "placeholder": "Name", "value": path("/pane/fields/name"), "disabled": path("/pane/readOnly")},
        "style": {"flexGrow": 1, "flexShrink": 1, "minWidth": 0},
        "on": {"change": {"event": {"name": "edit", "context": {"field": "name"}}}, "submit": {"event": {"name": "submit"}}}});
    let name_row = ink.shell("name_row", false, json!(true), vec![picker, reader, name]);
    let name_group = json!({"id": "name_group", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [ink.group("name_box", vec![name_row]), field_error(chrome, "name_error", path("/pane/nameError"), path("/pane/hasNameError"))]});

    // The body: one group, one field; a failed load swaps it for its empty
    // state + Retry (FEED-79), a load in flight for the skeleton.
    let body = json!({"id": "body", "component": "Textarea", "props": {"label": "Prompt", "name": "body", "variant": "code", "rows": 14, "autosize": true, "maxRows": 40, "placeholder": "Prompt", "value": path("/pane/fields/body"), "disabled": path("/pane/readOnly")},
        // A definite width: the core measures the autosize rows at it.
        "style": {"width": "100%"},
        "on": {"change": {"event": {"name": "edit", "context": {"field": "body"}}}}});
    let body = json!({"id": "body_ready", "component": "Box", "visible": path("/pane/bodyReady"), "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [body]});
    // `feedback::skeleton_text`: three bars (280, 220, 160) `gap_3`.
    let bars: Vec<Value> = [280., 220., 160.].iter().enumerate().map(|(ix, w): (usize, &f32)| json!({"id": format!("body_bar_{ix}"), "component": "Skeleton", "props": {"width": w.to_string(), "height": "8"}, "style": {"width": w, "height": 8, "flexShrink": 0, "borderRadius": 4, "backgroundColor": chrome.skeleton}})).collect();
    let loading = json!({"id": "body_loading", "component": "Box", "visible": path("/pane/bodyLoading"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "width": "100%", "minHeight": 256, "padding": ink.r(1.)}, "children": bars});
    let failed = failed_state(ink, "body_failed", "nav-actions", PROMPT_UNAVAILABLE, path("/pane/bodyFailed"));
    let body_group = json!({"id": "body_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [ink.group("body_group", vec![loading, failed, body]), field_error(chrome, "body_error", path("/pane/bodyError"), path("/pane/hasBodyError"))]});

    // Save: the primary `web_sm` Button; off = the fill at half (the label
    // solid); while the write is out past `PENDING_DELAY`, the spinner and
    // `Saving`.
    let mut save = sm_button(ink, "save", None, path("/pane/submit"), &chrome.primary, &chrome.primary_hover, &chrome.primary_fg, "save", chrome.radius);
    save["visible"] = path("/pane/saveOn");
    let mut save_off = sm_button(ink, "save_off", None, path("/pane/submit"), "#00000000", "#00000000", &chrome.primary_fg, "save", chrome.radius);
    save_off["visible"] = path("/pane/saveOff");
    save_off["props"]["pressable"] = json!(false);
    save_off["style"]["cursor"] = json!("default");
    save_off["style"]["position"] = json!("relative");
    save_off["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": "save_off_fill", "component": "Box",
        "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": chrome.radius, "backgroundColor": chrome.primary, "opacity": 0.5}}));
    let spinner = json!({"id": "saving_spinner", "component": "Spinner", "style": {"width": ink.r(SM), "height": ink.r(SM), "color": chrome.primary_fg, "flexShrink": 0}});
    let mut saving = sm_button(ink, "saving", Some(spinner), json!(crate::feedback::PENDING_SAVE), &chrome.primary, &chrome.primary, &chrome.primary_fg, "save", chrome.radius);
    saving["visible"] = path("/pane/saving");
    saving["props"]["pressable"] = json!(false);
    let actions = json!({"id": "actions", "component": "Box", "visible": path("/pane/editable"), "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "gap": ink.r(0.5), "minWidth": 0}, "children": [save, save_off, saving]});
    let form = json!({"id": "form", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "minWidth": 0, "width": "100%"},
        "children": [name_group, body_group, actions]});

    // The one owner-only control below the form: hidden from members.
    let icon = json!({"id": "delete_icon", "component": "Icon", "props": {"name": "ui-delete"}, "style": {"width": ink.r(SM), "height": ink.r(SM), "color": chrome.danger_fg, "flexShrink": 0}});
    let delete = sm_button(ink, "delete", Some(icon), json!("Delete prompt"), &chrome.danger_bg, &chrome.danger_hover, &chrome.danger_fg, "delete", chrome.radius);
    let delete = json!({"id": "delete_row", "component": "Box", "visible": path("/pane/canDelete"), "style": {"display": "flex", "flexDirection": "row"}, "children": [delete]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.), "width": "100%", "minWidth": 0}, "children": [band, form, delete]})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1629: the list and page documents raise no issue a debug
    /// build's mount check refuses (#EXP-1767's class).
    #[test]
    fn the_documents_raise_no_unexpected_issue() {
        let ink = Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        };
        let chrome = PageChrome {
            danger: "#f87171ff".into(),
            primary: "#f4f4f5ff".into(),
            primary_fg: "#18181bff".into(),
            primary_hover: "#e4e4e7ff".into(),
            danger_bg: "#dc2626ff".into(),
            danger_fg: "#ffffffff".into(),
            danger_hover: "#b91c1cff".into(),
            radius: 6.,
            skeleton: "#a1a1aa2e".into(),
        };
        for tree in [document(&ink, "#ffffff0a", "#a1a1aa2e"), page_document(&ink, &chrome)] {
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new("settings-prompts", Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{issues:?}");
        }
    }

    /// The copy is the web page's, word for word.
    #[test]
    fn copy_matches_the_web_page() {
        assert_eq!(PROMPTS_TITLE, "Prompts");
        assert_eq!(NEW_PROMPT_LABEL, "New prompt");
        assert_eq!(PROMPTS_EMPTY, "No prompts yet");
        assert_eq!(submit_label(true), "Create prompt");
        assert_eq!(submit_label(false), "Save changes");
        assert_eq!(body_too_long(PROMPT_BODY_MAX_CHARS), None);
        assert_eq!(
            body_too_long(PROMPT_BODY_MAX_CHARS + 1).as_deref(),
            Some("Too long: 65537 of 65536 characters")
        );
    }

    /// Delete confirms with the fixture title alone, Cancel + a destructive
    /// Delete (web `prompts-section.tsx`).
    #[test]
    fn delete_confirm_is_the_title_alone() {
        let prompt = delete_prompt_confirm("Release train");
        assert_eq!(prompt.title, "Delete \"Release train\"?");
        assert_eq!(prompt.body, None);
        let labels: Vec<&str> = prompt.actions.iter().map(|a| a.label).collect();
        assert_eq!(labels, vec!["Cancel", "Delete"]);
    }
}
