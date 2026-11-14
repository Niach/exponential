//! Settings → New agent (EXP-1319): the agent page in CREATE mode, the web's
//! `settings/agents/new` — #EXP-1846 `page`'s order: identity; Runs on
//! (Device, Account, Model); Triggers (THE list, [`super::agent_triggers`],
//! over the local draft: Check in first); Instructions; Create. All LOCAL
//! until "Create agent" sends `agents.create`; then the page becomes the new
//! agent's (`SettingsSection::Agent`), where Skills and MCP servers are
//! picked. Owner-only.
//!
//! Validation is the fixture's `validateAgentForm`, shown inline; a failed
//! create is the "Couldn't create the agent" toast (FEED-79).
//!
//! #EXP-1629: the page is an eUI document on the native binding (the agent
//! pages' pieces, [`super::agent_doc`]); the pane keeps the draft and the
//! create.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, WeakEntity, Window};
use serde_json::{json, Value};
use sync::Store;

use domain::agent_settings::{validate_agent_form, AgentForm, AGENT_SETTINGS};
use eui_gpui::binding::Binding;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;

use super::agent_detail::{account_options, agent_devices, current_account, seed_device, AgentDevice};
use super::agent_doc::{account_rows, agent_mark_icon, device_row, divided, field_error, identity, instructions_caption, instructions_field, select_row, Chrome};
use super::agent_triggers::{self, AgentTriggers, ListTarget};
use super::doc::{path, Ink, SM};
use super::SettingsSection;
use crate::navigation::{navigate, Screen};
use crate::queries;
use crate::trigger_dialog::DraftTriggers;

/// The page's create button label (web `agents-section.tsx`).
const CREATE_AGENT: &str = "Create agent";

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

pub struct NewAgentPane {
    team_id: String,
    /// The name and instructions as typed (the document's fields).
    name: String,
    instructions: String,
    /// Curated icon concept (contract `boardIcon`), sent as `icon`.
    icon: String,
    color: String,
    /// The picked machine's steer id; seeded to the caller's default machine,
    /// else the first online one, once the devices shape has it.
    device_id: Option<String>,
    /// The picked login (`agent`, profile id); `None` = the machine's last
    /// used one, the Account row's own default.
    account: Option<(String, String)>,
    /// #EXP-1846: the pinned model; `None` = the device default for the
    /// login's CLI (the Model row's own default).
    model: Option<String>,
    /// THE trigger list's state (the draft never writes, so never busy).
    trigger_list: AgentTriggers,
    /// The Triggers' draft — shared with the trigger dialog's save.
    triggers: Rc<RefCell<Vec<Value>>>,
    /// A create in flight.
    pending: bool,
    /// The fixture's validation messages (INVALID INPUT), by field.
    errors: Vec<domain::agent_settings::AgentFormError>,
    focused_once: bool,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's frames left to run a pressed popup's fade on.
    #[cfg(test)]
    shot_settle: u8,
    _subscriptions: Vec<Subscription>,
}

impl NewAgentPane {
    pub fn new(team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The devices shape can land AFTER the page opened: the seed re-runs
        // on every delta (a pick the person made is never disturbed).
        let mut subscriptions = Vec::new();
        if let Some(store) = Store::try_global(cx) {
            let devices = store.collections().devices.clone();
            subscriptions.push(cx.observe(&devices, |this: &mut Self, _, cx| {
                this.seed_device(cx);
                cx.notify();
            }));
        }
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(super::doc::IdeToasts));
            for name in ["icon", "color", "device", "account", "model", "create"].into_iter().chain(agent_triggers::ACTIONS) {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = event.context.clone();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
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
        let plugin: Rc<dyn HostPlugin> = Rc::new(PanePlugin { pane: cx.weak_entity(), inner: super::doc::ide_plugin() });
        let view = super::doc::mount_with("settings-new-agent", document(&Ink::of(cx), &Chrome::of(cx)), &binding, plugin, true, window, cx);
        let defaults = &domain::session_tree::CHATS_LIST.agent_defaults;
        let mut this = Self {
            team_id,
            name: String::new(),
            instructions: String::new(),
            icon: defaults.icon.clone(),
            color: defaults.color.clone(),
            device_id: None,
            account: None,
            model: None,
            trigger_list: AgentTriggers::default(),
            triggers: Rc::default(),
            pending: false,
            errors: Vec::new(),
            focused_once: false,
            binding,
            view,
            #[cfg(test)]
            shot_settle: 0,
            _subscriptions: subscriptions,
        };
        this.seed_device(cx);
        this
    }

    /// The bench's page of the shots team (unfocused: the caret blinks).
    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut pane = Self::new("team-shots".into(), window, cx);
        pane.focused_once = true;
        pane
    }

    /// The page in a pinned state.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: NewAgentShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.name = shot.name.into();
        self.instructions = shot.instructions.into();
        if let Some(device) = shot.device {
            self.device_id = Some(device.into());
        }
        *self.triggers.borrow_mut() = shot.triggers;
        if shot.validate {
            self.errors = validate_agent_form(&self.form());
        }
        self.pending = shot.pending;
        cx.notify();
    }

    /// The bench's press on a document node (a headless click never
    /// reaches an eUI popup), its popup's enter fade run out.
    #[cfg(test)]
    pub(crate) fn press_for_shots(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            // The bench's click (today's pane's) may already have opened it.
            if view.layers().is_empty() {
                let index = view.index_of(id).expect("the pressed node");
                view.press(index, window, cx);
            }
            view.advance_clock(std::time::Duration::from_millis(500), cx);
        });
        self.shot_settle = 4;
        cx.notify();
    }

    /// The caller's default machine, else the only one, else the first online.
    fn seed_device(&mut self, cx: &App) {
        if self.device_id.is_none() {
            let devices = agent_devices(&self.team_id, cx);
            self.device_id = seed_device(&devices).or_else(|| devices.iter().find(|device| device.online).map(|device| device.device_id.clone()));
        }
    }

    fn clear_error(&mut self, field: &str, cx: &mut gpui::Context<Self>) {
        let before = self.errors.len();
        self.errors.retain(|error| error.field != field);
        if self.errors.len() != before {
            cx.notify();
        }
    }

    fn error(&self, field: &str) -> Option<String> {
        self.errors.iter().find(|error| error.field == field).map(|error| error.message.clone())
    }

    fn form(&self) -> AgentForm {
        AgentForm { name: self.name.clone(), device_id: self.device_id.clone(), instructions: self.instructions.clone() }
    }

    /// The Account row's login on the picked machine (THE rule,
    /// `agent_account_value`): the pick, else the machine's last used one.
    fn picked_account(&self, devices: &[AgentDevice]) -> (Vec<coding::AccountOption>, Option<coding::AccountOption>) {
        let options = self.device_id.as_deref().and_then(|id| devices.iter().find(|device| device.device_id == id)).map(account_options).unwrap_or_default();
        let (cli, account) = match &self.account {
            Some((cli, account)) => (Some(cli.as_str()), Some(account.as_str())),
            None => (None, None),
        };
        let current = current_account(&options, account, cli).cloned();
        (options, current)
    }

    /// A field's edit (the plugin's): the live text clears its error.
    fn edited(&mut self, field: &str, value: String, cx: &mut gpui::Context<Self>) {
        match field {
            "name" => {
                self.name = value;
                self.clear_error("identity", cx);
            }
            "instructions" => {
                self.instructions = value;
                self.clear_error("instructions", cx);
            }
            _ => {}
        }
    }

    /// The trigger dialog's handle on this page's draft array.
    fn draft(&self, view: WeakEntity<Self>) -> DraftTriggers {
        let read = self.triggers.clone();
        let write = self.triggers.clone();
        DraftTriggers {
            team_id: self.team_id.clone(),
            read: Rc::new(move |_| read.borrow().clone()),
            write: Rc::new(move |next, cx| {
                *write.borrow_mut() = next;
                let _ = view.update(cx, |_, cx| cx.notify());
            }),
        }
    }

    /// The CLI the Model row's choices are for: the picked login's, else claude.
    fn cli(&self, devices: &[AgentDevice]) -> String {
        let (_, current) = self.picked_account(devices);
        current.map(|login| login.agent.id().to_string()).unwrap_or_else(|| "claude".to_string())
    }

    /// A document action.
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // #EXP-1846: THE trigger list's actions, over the draft.
        if agent_triggers::ACTIONS.contains(&name) {
            let target = ListTarget::Draft(self.draft(cx.entity().downgrade()));
            agent_triggers::handle(self, |pane| &mut pane.trigger_list, name, target, true, context, payload, window, cx);
            cx.notify();
            return;
        }
        let value = payload.get("value").and_then(Value::as_str).unwrap_or_default().to_string();
        match name {
            "icon" if !value.is_empty() => self.icon = value,
            "color" if !value.is_empty() => self.color = value,
            // A pick drops the account pick (a profile id is device-local,
            // EXP-995).
            "device" if !value.is_empty() => {
                if self.device_id.as_deref() != Some(value.as_str()) {
                    self.account = None;
                }
                self.device_id = Some(value);
                self.clear_error("device", cx);
            }
            "account" => {
                let devices = agent_devices(&self.team_id, cx);
                let before = self.cli(&devices);
                let (options, _) = self.picked_account(&devices);
                if let Some(option) = options.iter().find(|option| option.account_option_key() == value) {
                    self.account = Some((option.agent.id().to_string(), option.id.clone()));
                }
                // A pinned model is its CLI's: another CLI's login drops it.
                if self.cli(&devices) != before {
                    self.model = None;
                }
            }
            "model" if !value.is_empty() => self.model = Some(value),
            "create" => self.submit(window, cx),
            _ => {}
        }
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.pending {
            return;
        }
        let form = self.form();
        self.errors = validate_agent_form(&form);
        if !self.errors.is_empty() {
            cx.notify();
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            crate::action_error::failed_action("create", "the agent", Some("Not signed in.".into()), cx);
            return;
        };
        let (_, login) = self.picked_account(&agent_devices(&self.team_id, cx));
        self.pending = true;
        cx.notify();

        // NEW-2: an agent is a DEFINITION row (`agents.create`), never a
        // session; it runs as conversation sessions on first message.
        let input = api::agents::CreateInput {
            team_id: self.team_id.clone(),
            name: form.name.trim().to_string(),
            icon: Some(self.icon.clone()),
            color: Some(self.color.clone()),
            instructions: form.instructions,
            triggers: self.triggers.borrow().clone(),
            device_id: form.device_id.unwrap_or_default(),
            cli: login.as_ref().map(|login| login.agent.id().to_string()),
            account: login.as_ref().and_then(coding::AccountOption::wire_account),
            model: self.model.clone(),
        };
        cx.spawn_in(window, async move |this, window| {
            let result = window.background_executor().spawn(async move { api::agents::create(&trpc, &input) }).await;
            match result {
                Ok(agent) => {
                    // Land on the agent's page once its row has synced (the
                    // nav clamps an unknown `Agent(id)` away).
                    let agent_id = agent.id;
                    let agents = window.update(|_, cx| Store::global(cx).collections().agents.clone()).ok();
                    if let Some(agents) = agents {
                        queries::await_row_visible(&agents, &agent_id, window).await;
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        this.pending = false;
                        crate::sidebar::select_settings_section(window, cx, SettingsSection::Agent(agent_id));
                        navigate(window, cx, Screen::Settings);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.pending = false;
                        // A create can duplicate, so no Retry.
                        crate::action_error::failed_action_err("create", "the agent", &err, cx);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// The page as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let devices = agent_devices(&self.team_id, cx);
        // THE device picker: each machine by its glyph, offline ones say so.
        let device_options: Vec<Value> = devices
            .iter()
            .map(|device| {
                let mut option = json!({"label": device.label, "value": device.device_id, "icon": crate::icons::device_icon_name(device.icon.as_deref(), device.server)});
                if !device.online {
                    option["description"] = json!("Offline");
                }
                option
            })
            .collect();
        // The Account row: THE account picker over the picked machine's
        // logins (brand mark + email), "Last used" when it reports none.
        let (options, current) = self.picked_account(&devices);
        let logins: Vec<Value> = options
            .iter()
            .filter(|option| !option.id.is_empty())
            .map(|option| {
                let mut row = json!({"label": option.email, "value": option.account_option_key(), "icon": agent_mark_icon(option.agent)});
                if let Some(badge) = option.health.badge_label() {
                    row["description"] = json!(badge);
                }
                row
            })
            .collect();
        // #EXP-1846: the Model row (unpinned = the device default for the
        // login's CLI).
        let cli = self.cli(&devices);
        let models: &[(&str, &str)] = if cli == "codex" { &crate::coding_selects::CODEX_MODEL_CHOICES } else { &crate::coding_selects::MODEL_CHOICES };
        let identity_error = self.error("identity");
        let device_error = self.error("device");
        let instructions_error = self.error("instructions");
        json!({
            "icon": self.icon,
            "color": self.color,
            "fields": {"name": self.name, "instructions": self.instructions},
            "device": {"value": self.device_id.clone().unwrap_or_default(), "options": device_options},
            "account": {"has": current.is_some(), "none": current.is_none(), "value": current.as_ref().map(|option| option.account_option_key()).unwrap_or_default(), "options": logins},
            "model": {"value": domain::agent_settings::assistant_model_value(self.model.as_deref(), Some(&cli)), "options": models.iter().map(|(label, value)| json!({"label": label, "value": value})).collect::<Vec<_>>()},
            "triggerList": agent_triggers::project_list(&self.triggers.borrow(), true, false),
            "deviceError": device_error.clone().unwrap_or_default(),
            "hasDeviceError": device_error.is_some(),
            "identityError": identity_error.clone().unwrap_or_default(),
            "hasIdentityError": identity_error.is_some(),
            "instructionsError": instructions_error.clone().unwrap_or_default(),
            "hasInstructionsError": instructions_error.is_some(),
            "pending": self.pending,
        })
    }
}

impl Render for NewAgentPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "NewAgentPane").entered();
        #[cfg(test)]
        if self.shot_settle > 0 {
            self.shot_settle -= 1;
            self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            cx.notify();
        }
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        if !self.focused_once {
            self.focused_once = true;
            self.view.update(cx, |view, cx| {
                view.focus_field("name", window, cx);
            });
        }
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The pane's plugin: the IDE's, and the name / instructions edits for
/// the pane.
struct PanePlugin {
    pane: WeakEntity<NewAgentPane>,
    inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        self.inner.icon(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        self.inner.icon_element(name, size, color, cx)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn on_input(&self, event: &eui_gpui::host::InputEvent, cx: &mut App) {
        let field = event.name.clone();
        let value = event.value.as_str().unwrap_or_default().to_string();
        let pane = self.pane.clone();
        cx.defer(move |cx| {
            let _ = pane.update(cx, |pane, cx| pane.edited(&field, value, cx));
        });
    }
}

/// The page's tree (`gap_4`, #EXP-1846 `page`): the identity group and its
/// message; Runs on (Device, Account, Model) and the device's message; THE
/// Triggers list over the draft; the Instructions card and its message; the
/// right-aligned "Create agent".
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let identity_block = json!({"id": "identity_block", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [ink.group("agent_group", vec![identity(ink)]), field_error(ink, chrome, "identity_error", path("/pane/identityError"), path("/pane/hasIdentityError"))]});
    let [account_pick, account_none] = account_rows(ink);
    let mut runs_on = vec![device_row()];
    runs_on.extend(divided(ink, account_pick));
    runs_on.extend(divided(ink, account_none));
    runs_on.extend(divided(ink, select_row("model", crate::chat_screen::MENU_MODEL, "model", "model", json!({}))));
    let runs_on_block = json!({"id": "runs_on_block", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [ink.band("runs_on_band", &AGENT_SETTINGS.copy.runs_on, None), ink.group("runs_on_group", runs_on),
            field_error(ink, chrome, "device_error", path("/pane/deviceError"), path("/pane/hasDeviceError"))]});
    let instructions = json!({"id": "instructions_card", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25), "width": "100%"},
        "children": [ink.group("instructions_group", vec![instructions_field(json!(true))]), instructions_caption(ink),
            field_error(ink, chrome, "instructions_error", path("/pane/instructionsError"), path("/pane/hasInstructionsError"))]});
    // `Button::primary().web_sm()`: h 32, px 12, the theme radius, the
    // Small label; pending = gpui-component's `loading` face (disabled).
    let size = ink.r(SM);
    let draw = |dim: bool| {
        let id = if dim { "create_dim" } else { "create" };
        let mut button = json!({"id": id, "component": "Box", "visible": if dim { path("/pane/pending") } else { super::doc::not(path("/pane/pending")) }, "accessibility": {"label": CREATE_AGENT},
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "height": 32, "paddingHorizontal": 12, "borderRadius": chrome.radius, "backgroundColor": chrome.primary},
            "children": [{"id": format!("{id}_text"), "component": "Text", "props": {"text": CREATE_AGENT}, "style": {"fontSize": size, "lineHeight": size, "color": chrome.primary_fg, "flexShrink": 0}}]});
        if dim {
            // gpui-component's disabled primary: the fill at half, the
            // label solid. The half fill as a layer at half opacity (a
            // `#rrggbbaa` alpha byte can't say 0.5).
            button["style"]["backgroundColor"] = json!("#00000000");
            button["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": "create_dim_fill", "component": "Box",
                "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": chrome.radius, "backgroundColor": chrome.primary, "opacity": 0.5}}));
        } else {
            button["props"] = json!({"pressable": true});
            button["on"] = json!({"press": {"event": {"name": "create"}}});
            button["style"]["cursor"] = json!("pointer");
            button["style"][":hover"] = json!({"backgroundColor": chrome.primary_hover});
        }
        button
    };
    let create = json!({"id": "create_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "width": "100%"}, "children": [draw(false), draw(true)]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.), "width": "100%", "minWidth": 0},
        "children": [identity_block, runs_on_block, agent_triggers::section(ink, chrome), instructions, create]})
}

/// One bench state of the page.
#[cfg(test)]
pub(crate) struct NewAgentShot {
    pub name: &'static str,
    pub instructions: &'static str,
    /// A machine other than the seeded default.
    pub device: Option<&'static str>,
    pub triggers: Vec<Value>,
    /// Run the form's validation (its inline errors).
    pub validate: bool,
    pub pending: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// refuses (#EXP-1767's class).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let tree = document(&super::super::readiness::tests::test_ink(), &Chrome::test());
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-new-agent", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }
}
