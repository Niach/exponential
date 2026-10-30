//! Settings → one team agent's page (EXP-1294 — the Agents group lists every
//! agent root as its own nav entry; this pane is the selected one's page).
//!
//! The board page's layout, field for field (`fixtures/agent-settings.json`
//! `fields`, labels and copy through `domain::agent_settings`): ONE glass
//! group holding the identity row (icon picker, colour picker, name — the
//! board form's one field) and the Device row; a second group with the
//! Account row and the Schedule rows (one per trigger, a toggle each, the
//! trigger editor in its dialog); the Instructions card (server-only,
//! `agents.get`, saved on blur); the owner-only "Delete agent".
//!
//! Every write is `agents.update` / `agents.delete`; icon,
//! colour, device and account save at once, the name on blur/Enter. A failed
//! write is the "Couldn't …" toast (FEED-79).
//!
//! The pane is a singleton: it reads the selected `Agent(id)` from the
//! window's shared settings selection and re-points itself on agent switches,
//! flushing the previous agent's pending name and instructions first (blur
//! doesn't fire on unmount).

use std::rc::Rc;

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};
use serde_json::Value;
use sync::Store;

use domain::agent_settings::{
    agent_account_value, validate_agent_form, AgentAccountOption, AgentForm, AGENT_SETTINGS,
};
use domain::rows::AgentDefRow;

use crate::controls::{glass_input, WebControl as _};
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::sidebar::RailShared;
use crate::trigger_editor;

use super::{is_owner, section, SettingsSection};
use crate::icons::registry;

/// A fixture field's label (`agent-settings.json` `fields[key].label`).
pub(crate) fn field_label(key: &str) -> &'static str {
    AGENT_SETTINGS
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.label.as_str())
        .unwrap_or_default()
}

/// One machine an agent can run on: an own device or one shared with the
/// agent's team (the devices shape's scope). Offline rows stay pickable — a
/// trigger that missed its time fires once the machine comes back.
#[derive(Clone, Debug)]
pub(crate) struct AgentDevice {
    /// The steer TEXT id (`devices.device_id`), what `sessions` rows carry.
    pub(crate) device_id: String,
    pub(crate) label: String,
    /// The stored device glyph (`contract::DEVICE_ICON_VALUES`).
    pub(crate) icon: Option<String>,
    pub(crate) server: bool,
    /// The row's `kind` (`desktop` / `server` / ...; NEW-99 Runs on).
    pub(crate) kind: Option<String>,
    pub(crate) online: bool,
    /// The signed-in user's DEFAULT machine (EXP-622) — a new agent seeds to it.
    pub(crate) is_default: bool,
    /// The agent CLIs the machine advertises.
    agents: Vec<String>,
    /// Its `agent_accounts` / `agent_usage` payloads (the account picker).
    accounts: coding::agent_accounts::AgentAccounts,
    usage: coding::agent_usage::AgentUsageMap,
    /// Its published launch defaults clamped onto a default `Settings` — its
    /// `default_agent` names the machine's LAST USED login.
    settings: coding::Settings,
}

impl AgentDevice {
    /// A machine a run can start on NOW: online with a runnable agent (the
    /// relay refuses an offline start, a signed-out machine has nothing to
    /// run) — web/iOS's start-target rule.
    pub(crate) fn can_start_now(&self) -> bool {
        self.online && !self.agents.is_empty()
    }
}

/// The machines an agent of `team_id` can run on, online first, then by name.
pub(crate) fn agent_devices(team_id: &str, cx: &App) -> Vec<AgentDevice> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let me = queries::active_account(cx).map(|account| account.user_id);
    let mut devices: Vec<AgentDevice> = store
        .collections()
        .devices
        .read(cx)
        .iter()
        .filter(|row| {
            // #EXP-1440: an own machine shared only with OTHER teams is theirs.
            (me.as_deref().is_some_and(|me| row.user_id.as_deref() == Some(me))
                && row.is_usable_in_team(team_id))
                || row.is_shared_with(team_id)
        })
        .filter_map(|row| {
            let device_id = row.device_id.clone().filter(|id| !id.is_empty())?;
            let agents = row.agent_ids();
            // `launch_defaults` syncs as JSON that may itself be a JSON
            // string; the default agent only counts when runnable there.
            let launch_defaults = row.launch_defaults.as_ref().and_then(|value| match value {
                Value::String(raw) => serde_json::from_str::<Value>(raw).ok(),
                other => Some(other.clone()),
            });
            let mut settings = coding::Settings::default();
            if let Some(patch) = launch_defaults
                .as_ref()
                .and_then(|value| serde_json::from_value::<coding::DefaultsPatch>(value.clone()).ok())
            {
                coding::apply_defaults_patch(&mut settings, &patch);
            }
            if let Some(agent) = launch_defaults
                .as_ref()
                .and_then(|value| value.get("defaultAgent").and_then(Value::as_str))
                .filter(|agent| agents.iter().any(|id| id == agent))
                .and_then(coding::CodingAgent::parse)
            {
                settings.default_agent = agent;
            }
            Some(AgentDevice {
                label: row.label.clone().unwrap_or_else(|| device_id.clone()),
                icon: row.icon.clone(),
                server: row.is_server(),
                kind: row.kind.clone(),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                is_default: me
                    .as_deref()
                    .is_some_and(|me| row.user_id.as_deref() == Some(me))
                    && row.is_default.unwrap_or(false),
                agents,
                accounts: crate::device_settings::parse_agent_map(row.agent_accounts.as_ref()),
                usage: crate::device_settings::parse_agent_map(row.agent_usage.as_ref()),
                settings,
                device_id,
            })
        })
        .collect();
    devices.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.label.cmp(&b.label)));
    devices
}

/// The device a NEW agent seeds to: the caller's default machine, else the
/// only candidate. Several machines and no default leave the pick explicit.
pub(crate) fn seed_device(devices: &[AgentDevice]) -> Option<String> {
    devices
        .iter()
        .find(|device| device.is_default)
        .or(match devices {
            [only] => Some(only),
            _ => None,
        })
        .map(|device| device.device_id.clone())
}

/// Every signed-in login `device` reports, across its agents, its last used
/// login first; a machine that reports none offers one unpinned row per
/// runnable agent, so the row never goes empty.
pub(crate) fn account_options(device: &AgentDevice) -> Vec<coding::AccountOption> {
    let advertised: Vec<coding::CodingAgent> =
        device.agents.iter().filter_map(|id| coding::CodingAgent::parse(id)).collect();
    let available = if advertised.is_empty() {
        domain::contract::CODING_AGENT_VALUES
            .iter()
            .filter_map(|id| coding::CodingAgent::parse(id))
            .collect()
    } else {
        advertised
    };
    crate::launch_options::machine_account_options(
        &device.accounts,
        &device.usage,
        &device.settings,
        &available,
    )
}

/// The account row's current option through THE rule
/// (`domain::agent_settings::agent_account_value`, EXP-1306): the stored
/// profile (of the agent's CLI when it names one), else that CLI's first
/// login, else the machine's last used login, else the first. Only REPORTED
/// logins count — the agent-named unpinned rows of a machine that reports
/// none are no login, so the row reads "Last used" instead.
pub(crate) fn current_account<'a>(
    options: &'a [coding::AccountOption],
    account: Option<&str>,
    cli: Option<&str>,
) -> Option<&'a coding::AccountOption> {
    let logins: Vec<&coding::AccountOption> = options.iter().filter(|option| !option.id.is_empty()).collect();
    let rows: Vec<AgentAccountOption> = logins
        .iter()
        .map(|option| AgentAccountOption {
            id: option.id.clone(),
            agent: option.agent.id().to_string(),
            email: option.email.clone(),
            is_last_used: option.is_last_used,
        })
        .collect();
    let value = agent_account_value(&rows, account, cli)?;
    logins
        .into_iter()
        .find(|option| option.agent.id() == value.agent && option.email == value.email)
}

/// THE device picker over `devices` behind a picker-row trigger (the
/// composer's rows, each machine by its own glyph). Shared with the New agent
/// page ([`super::new_agent`]).
pub(crate) fn device_picker_control(
    prefix: &'static str,
    devices: &[AgentDevice],
    picked: Option<String>,
    on_pick: Rc<dyn Fn(String, &mut Window, &mut App)>,
    cx: &App,
) -> gpui::AnyElement {
    let label = picked
        .as_deref()
        .map(|id| {
            devices
                .iter()
                .find(|device| device.device_id == id)
                .map(|device| device.label.clone())
                // A bound device that isn't synced here (a teammate's
                // private machine) keeps its raw id rather than vanishing.
                .unwrap_or_else(|| id.to_string())
        })
        .unwrap_or_else(|| "Select device…".to_string());
    let trigger = trigger_editor::picker_trigger(format!("{prefix}-device").into(), label, cx)
        .into_any_element();
    let rows: Vec<crate::picker::device_picker::DevicePickerDevice> = devices
        .iter()
        .map(|device| crate::picker::device_picker::DevicePickerDevice {
            id: device.device_id.clone(),
            name: device.label.clone(),
            icon: device.icon.clone(),
            server: device.server,
            description: (!device.online).then(|| "Offline".to_string()),
            disabled: false,
        })
        .collect();
    crate::picker::deferred(move |window, cx| {
        crate::picker::device_picker::device_picker(
            &rows,
            picked,
            trigger,
            Rc::new(move |next: Vec<String>, window: &mut Window, cx: &mut App| {
                if let Some(device_id) = next.into_iter().next() {
                    on_pick(device_id, window, cx);
                }
            }),
        )
        .id(SharedString::from(format!("{prefix}-device-picker")))
        .render(window, cx)
    })
    .into_any_element()
}

/// Fire-and-forget agent write off the foreground; the synced row's echo
/// repaints, a rejection is the "Couldn't <verb> <thing>" toast.
fn spawn_agent_write<F, T>(cx: &mut App, verb: &'static str, thing: &'static str, call: F)
where
    F: FnOnce(&api::TrpcClient) -> Result<T, api::ApiError> + Send + 'static,
    T: Send + 'static,
{
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] agent write skipped: no signed-in account");
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { call(&trpc) }).await;
        if let Err(err) = result {
            let _ = cx.update(|cx| crate::action_error::failed_action_err(verb, thing, &err, cx));
        }
    })
    .detach();
}

fn update_agent(cx: &mut App, verb: &'static str, thing: &'static str, input: api::agents::UpdateInput) {
    spawn_agent_write(cx, verb, thing, move |trpc| api::agents::update(trpc, &input));
}

/// The server-only instructions of the selected agent.
enum InstructionsLoad {
    Loading,
    /// Loaded; the text the server holds (a blur saves only a change).
    Ready(String),
    /// The read failed (the section shows its Retry).
    Failed,
}

pub struct AgentDetailPane {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    name_input: Entity<InputState>,
    instructions: Entity<TextareaState>,
    /// The agent the inputs were last seeded from — a selection switch
    /// flushes the previous agent's pending edits, then re-seeds.
    loaded_agent: Option<String>,
    load: InstructionsLoad,
    /// The form's own validation message (INVALID INPUT, FEED-79).
    error: Option<SharedString>,
    /// A trigger toggle in flight — the next whole-array write starts from
    /// this one's array, so one at a time.
    trigger_writing: bool,
    /// Monotonic guard: a stale instructions read must not clobber a newer one.
    generation: u64,
    /// S-3: the Assistant page's Voice section = THE Voice settings block
    /// (`voice_prefs`), minus its Voice device and Agent pickers; built on
    /// the first Assistant shown.
    voice: Option<Entity<super::voice_prefs::VoicePrefsPane>>,
    _subscriptions: Vec<Subscription>,
}

impl AgentDetailPane {
    pub fn new(
        nav: Entity<Navigation>,
        shared: Entity<RailShared>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let copy = &AGENT_SETTINGS.copy;
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(copy.name_placeholder.clone()));
        let instructions = cx.new(|cx| {
            crate::controls::web_textarea(4, 16, window, cx)
                .placeholder(copy.instructions_placeholder.clone())
        });
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.agents, |_, _, cx| cx.notify()),
            cx.observe(&collections.devices, |_, _, cx| cx.notify()),
            cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Blur | InputEvent::PressEnter { .. } => this.commit_rename(cx),
                _ => {}
            }),
            cx.subscribe(&instructions, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.commit_instructions(cx);
                }
            }),
        ];
        Self {
            nav,
            shared,
            name_input,
            instructions,
            loaded_agent: None,
            load: InstructionsLoad::Loading,
            error: None,
            trigger_writing: false,
            generation: 0,
            voice: None,
            _subscriptions: subscriptions,
        }
    }

    /// The selected agent id from the shared settings selection. S-3: the
    /// Voice section (tray / meeting links) resolves to the viewer's
    /// Assistant, as `first_agent_resolved` does for the screen.
    fn selected_agent_id(&self, cx: &App) -> Option<String> {
        match self.shared.read(cx).settings_section() {
            SettingsSection::Agent(id) => Some(id),
            SettingsSection::Voice => active_team_id(&self.nav, cx)
                .and_then(|team_id| crate::session_data::my_assistant(&team_id, cx))
                .map(|assistant| assistant.id),
            _ => None,
        }
    }

    /// Re-seed the inputs when the selected agent changes, flushing the
    /// PREVIOUS agent's pending name and instructions first.
    fn sync_selected_agent(&mut self, agent: &AgentDefRow, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.loaded_agent.as_deref() == Some(agent.id.as_str()) {
            return;
        }
        if self.loaded_agent.is_some() {
            self.commit_rename(cx);
            self.commit_instructions(cx);
        }
        self.loaded_agent = Some(agent.id.clone());
        self.error = None;
        let name = agent.name.clone().unwrap_or_default();
        self.name_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.instructions
            .update(cx, |input, cx| input.set_value(String::new(), window, cx));
        self.fetch_instructions(window, cx);
    }

    /// `agents.get` — the ONLY instructions path (never synced).
    fn fetch_instructions(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(agent_id) = self.loaded_agent.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.load = InstructionsLoad::Loading;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::agents::get(&trpc, &agent_id) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                if this.generation != generation {
                    return; // superseded by a newer read
                }
                this.load = match result {
                    Ok(details) => {
                        let text = details.instructions;
                        this.instructions
                            .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
                        InstructionsLoad::Ready(text)
                    }
                    Err(err) => {
                        log::warn!("[ui] agents.get failed: {err}");
                        InstructionsLoad::Failed
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The form as it stands, for the fixture's validation.
    fn form(&self, agent: Option<&AgentDefRow>, cx: &App) -> AgentForm {
        AgentForm {
            name: self.name_input.read(cx).value().to_string(),
            device_id: agent.and_then(|agent| agent.device_id.clone()),
            instructions: self.instructions.read(cx).value().to_string(),
        }
    }

    /// The first fixture error of `field`, if the form has one.
    fn field_error(&self, field: &str, agent: Option<&AgentDefRow>, cx: &App) -> Option<String> {
        validate_agent_form(&self.form(agent, cx))
            .into_iter()
            .find(|error| error.field == field)
            .map(|error| error.message)
    }

    /// `agents.update({name})` when the drafted name is valid and
    /// differs from the synced row (keyed on the LOADED agent — a remote
    /// rename while editing doesn't stomp the input).
    fn commit_rename(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(agent_id) = self.loaded_agent.clone() else {
            return;
        };
        // A deleted agent (its row gone) has nothing left to rename.
        let Some(agent) = crate::session_data::agent_def(&agent_id, cx) else {
            return;
        };
        let agent = Some(agent);
        if let Some(message) = self.field_error("identity", agent.as_ref(), cx) {
            self.error = Some(message.into());
            cx.notify();
            return;
        }
        let drafted = self.name_input.read(cx).value().trim().to_string();
        if agent.as_ref().and_then(|agent| agent.name.as_deref()) == Some(drafted.as_str()) {
            return;
        }
        self.error = None;
        cx.notify();
        update_agent(
            cx,
            "rename",
            "the agent",
            api::agents::UpdateInput {
                name: Some(drafted),
                ..api::agents::UpdateInput::new(agent_id)
            },
        );
    }

    /// `agents.update({instructions})` when the text changed since the
    /// read (never before it landed: a blur then would blank them).
    fn commit_instructions(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(agent_id) = self.loaded_agent.clone() else {
            return;
        };
        let InstructionsLoad::Ready(saved) = &self.load else {
            return;
        };
        let Some(agent) = crate::session_data::agent_def(&agent_id, cx) else {
            return;
        };
        // #EXP-1439: an Assistant runs on the built-in prompt; `agents.update`
        // refuses its instructions (`copy.promptFixed`), so never send them.
        if agent.is_assistant() {
            return;
        }
        let agent = Some(agent);
        if let Some(message) = self.field_error("instructions", agent.as_ref(), cx) {
            self.error = Some(message.into());
            cx.notify();
            return;
        }
        let drafted = self.instructions.read(cx).value().to_string();
        if &drafted == saved {
            return;
        }
        self.load = InstructionsLoad::Ready(drafted.clone());
        self.error = None;
        cx.notify();
        update_agent(
            cx,
            "save",
            "the instructions",
            api::agents::UpdateInput {
                instructions: Some(drafted),
                ..api::agents::UpdateInput::new(agent_id)
            },
        );
    }

    /// A trigger row's switch: only that trigger's `enabled` moves.
    fn set_trigger_enabled(&mut self, agent_id: String, trigger_id: String, enabled: bool, cx: &mut gpui::Context<Self>) {
        if self.trigger_writing {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let triggers = trigger_editor::triggers_with_enabled(
            &trigger_editor::agent_triggers(&agent_id, cx),
            &trigger_id,
            enabled,
        );
        self.trigger_writing = true;
        cx.notify();
        let input = api::agents::UpdateInput {
            triggers: Some(triggers.clone()),
            ..api::agents::UpdateInput::new(agent_id.clone())
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::agents::update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.trigger_writing = false;
                match result {
                    Ok(_) => trigger_editor::note_triggers_written(&agent_id, triggers),
                    Err(err) => crate::action_error::failed_action_err(
                        if enabled { "resume" } else { "pause" },
                        "the trigger",
                        &err,
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The Delete confirm: the fixture's `deleteConfirm`, a Danger answer
    /// (Return lands on Cancel). The nav clamps the stale `Agent(id)` away on
    /// the sessions delta.
    fn open_delete_dialog(agent_id: String, window: &mut Window, cx: &mut App) {
        let spec = AlertSpec::new(
            field_label("delete"),
            AGENT_SETTINGS.copy.delete_confirm.clone(),
            "Delete",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let agent_id = agent_id.clone();
            spawn_agent_write(cx, "delete", "the agent", move |trpc| {
                api::agents::delete(trpc, &agent_id)
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// The Device row: picking a machine re-binds the agent and drops its
    /// account pin (a profile id is device-local, EXP-995).
    fn device_row(&self, agent: &AgentDefRow, devices: &[AgentDevice], cx: &App) -> gpui::Div {
        let agent_id = agent.id.clone();
        let bound = agent.device_id.clone();
        let control = device_picker_control(
            "agent-detail",
            devices,
            agent.device_id.clone(),
            Rc::new(move |device_id: String, _: &mut Window, cx: &mut App| {
                if bound.as_deref() == Some(device_id.as_str()) {
                    return;
                }
                update_agent(
                    cx,
                    "change",
                    "the agent's device",
                    api::agents::UpdateInput {
                        device_id: Some(device_id),
                        account: Some(None),
                        ..api::agents::UpdateInput::new(agent_id.clone())
                    },
                );
            }),
            cx,
        );
        crate::surface::glass_picker_row(field_label("device"), None, control, cx)
    }

    /// The Account row: THE account picker over the bound machine's logins
    /// (brand mark + email), its last used login when nothing is pinned.
    fn account_row(&self, agent: &AgentDefRow, devices: &[AgentDevice], cx: &App) -> gpui::Div {
        let options = agent
            .device_id
            .as_deref()
            .and_then(|id| devices.iter().find(|device| device.device_id == id))
            .map(account_options)
            .unwrap_or_default();
        let current = current_account(&options, agent.account.as_deref(), agent.cli.as_deref()).cloned();
        let control = match current {
            // No login reported (offline, another member's box): the launch
            // spends the machine's last used one, so the row says so, muted.
            None => div()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(AGENT_SETTINGS.copy.account_last_used.clone()))
                .into_any_element(),
            // The value = the composer chip's: the agent mark + the email,
            // always with the chevron (EXP-1307: one login too, like web and
            // Android).
            Some(current) => {
                let trigger = trigger_editor::picker_trigger_bare(
                    "agent-detail-account".into(),
                    SharedString::from(current.email.clone()),
                    cx,
                )
                .icon(crate::coding_selects::agent_mark(current.agent))
                .child(crate::surface::picker_row_chevron(cx))
                .into_any_element();
                let picked = current.account_option_key();
                let agent_id = agent.id.clone();
                crate::picker::deferred(move |window, cx| {
                    let rows = options.clone();
                    crate::picker::account_picker::account_picker(
                        &options,
                        Some(picked),
                        trigger,
                        Rc::new(move |next: Vec<String>, _window: &mut Window, cx: &mut App| {
                            let Some(option) = next.into_iter().next().and_then(|key| {
                                rows.iter().find(|option| option.account_option_key() == key).cloned()
                            }) else {
                                return;
                            };
                            update_agent(
                                cx,
                                "change",
                                "the agent's account",
                                api::agents::UpdateInput {
                                    account: Some(option.wire_account()),
                                    ..api::agents::UpdateInput::new(agent_id.clone())
                                },
                            );
                        }),
                    )
                    .id(SharedString::from("agent-detail-account-picker"))
                    .render(window, cx)
                })
                .into_any_element()
            }
        };
        crate::surface::glass_picker_row(field_label("account"), None, control, cx)
    }

    /// The Schedule rows (web/iOS/Android): the "Schedule" row carrying
    /// "+ Add", then one row per trigger, its summary leading (opens the
    /// editor) and its switch trailing (pauses it); none = ONE row reading
    /// "Waits for messages" that opens the editor on a new trigger.
    fn schedule_rows(&self, agent_id: &str, cx: &mut gpui::Context<Self>) -> Vec<gpui::Div> {
        let copy = &AGENT_SETTINGS.copy;
        let label = field_label("triggers");
        let triggers = trigger_editor::agent_triggers(agent_id, cx);
        if triggers.is_empty() {
            let agent_id = agent_id.to_string();
            let control = trigger_editor::picker_trigger(
                "agent-detail-schedule-new".into(),
                copy.no_schedule.clone(),
                cx,
            )
            .on_click(move |_, window, cx| {
                crate::trigger_dialog::open_new(window, cx, agent_id.clone())
            })
            .into_any_element();
            return vec![crate::surface::glass_picker_row(label, None, control, cx)];
        }
        let add: gpui::AnyElement = if triggers.len() < domain::contract::AGENT_TRIGGER_MAX_PER_AGENT {
            let agent_id = agent_id.to_string();
            Button::new("agent-detail-schedule-add")
                .outline()
                .cursor_pointer()
                .xsmall()
                .icon(registry::UI_ADD)
                .label(copy.add_trigger.clone())
                .on_click(move |_, window, cx| {
                    crate::trigger_dialog::open_new(window, cx, agent_id.clone())
                })
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let mut rows = vec![crate::surface::glass_picker_row(label, None, add, cx)];
        rows.extend(triggers.iter().enumerate().filter_map(|(index, trigger)| {
            let trigger_id = trigger_editor::trigger_id(trigger)?.to_string();
            let enabled = trigger_editor::trigger_enabled(trigger);
            let (edit_agent, edit_trigger) = (agent_id.to_string(), trigger_id.clone());
            let summary = trigger_editor::picker_trigger_bare(
                format!("agent-detail-trigger-{index}").into(),
                domain::agent_settings::trigger_summary(trigger),
                cx,
            )
            .text_color(cx.theme().foreground)
            .on_click(move |_, window, cx| {
                crate::trigger_dialog::open_edit(window, cx, edit_agent.clone(), edit_trigger.clone())
            });
            let toggle_agent = agent_id.to_string();
            let switch = crate::controls::web_switch(("agent-detail-trigger-enabled", index))
                .checked(enabled)
                .disabled(self.trigger_writing)
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    this.set_trigger_enabled(toggle_agent.clone(), trigger_id.clone(), *on, cx);
                }));
            Some(
                crate::surface::glass_row_shell()
                    .text_sm()
                    .child(h_flex().flex_1().min_w_0().justify_start().child(summary))
                    .child(switch),
            )
        }));
        rows
    }

    /// The Instructions card: the chrome-less textarea in a glass group, its
    /// label as the caption UNDER it (the draft); the read's skeleton, or its
    /// Retry when it failed.
    fn instructions_card(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let field: gpui::AnyElement = match &self.load {
            InstructionsLoad::Loading => div()
                .w_full()
                .px_4()
                .py_4()
                .child(crate::feedback::skeleton_text("agent-detail-instructions-placeholder", cx))
                .into_any_element(),
            InstructionsLoad::Failed => crate::action_error::load_failed_state(
                "agent-detail-instructions-retry",
                gpui_component::Icon::new(registry::SETTINGS_AGENTS),
                "Couldn't load the instructions",
                cx.listener(|this, _, window, cx| this.fetch_instructions(window, cx)),
                cx,
            )
            .into_any_element(),
            InstructionsLoad::Ready(_) => Textarea::new(&self.instructions)
                .appearance(false)
                .w_full()
                .px_4()
                .py_3()
                .into_any_element(),
        };
        v_flex()
            .w_full()
            .gap_1()
            .child(crate::surface::glass_group().child(field))
            .child(
                div()
                    .px_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(field_label("instructions")),
            )
    }
}

/// S-3: one single-select Picker row over `(value, label)` options (THE
/// Picker behind a picker-row trigger, the device row's recipe).
fn choice_row(
    id: &'static str,
    label: impl Into<SharedString>,
    options: Vec<(String, String)>,
    picked: Option<String>,
    on_pick: Rc<dyn Fn(String, &mut Window, &mut App)>,
    cx: &App,
) -> gpui::Div {
    let value = picked
        .as_deref()
        .and_then(|picked| options.iter().find(|(value, _)| value == picked))
        .map(|(_, label)| label.clone())
        .unwrap_or_default();
    let trigger = trigger_editor::picker_trigger(SharedString::from(id), value, cx).into_any_element();
    let items: Vec<crate::picker::PickerItem<String>> = options
        .into_iter()
        .map(|(value, label)| crate::picker::PickerItem::new(value, label))
        .collect();
    let control = crate::picker::deferred(move |window, cx| {
        let on_pick = on_pick.clone();
        crate::picker::Picker::single(
            items.clone(),
            picked.clone(),
            trigger,
            Rc::new(move |next: Vec<String>, window: &mut Window, cx: &mut App| {
                if let Some(value) = next.into_iter().next() {
                    on_pick(value, window, cx);
                }
            }),
        )
        .id(SharedString::from(format!("{id}-picker")))
        .render(window, cx)
    })
    .into_any_element();
    crate::surface::glass_picker_row(label.into(), None, control, cx)
}

impl AgentDetailPane {
    /// S-3 — THE ASSISTANT page (agent-settings.json `assistant.sections`):
    /// identity (the name, the brand mark instead of the icon picker), Runs
    /// on (THE device / account / model rows), Check in (THE Picker over
    /// `schedules`), Voice (the Voice settings block moved as one, minus
    /// its Voice device and Agent pickers). No Instructions (#EXP-1439: the
    /// prompt is built in), no Delete.
    fn render_assistant(&mut self, agent: &AgentDefRow, window: &mut Window, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        use domain::agent_settings::{assistant_schedule_of, assistant_schedule_triggers, ASSISTANT};
        let team_id = agent
            .team_id
            .clone()
            .or_else(|| active_team_id(&self.nav, cx))
            .unwrap_or_default();
        // NEW-99 (`runsOnCases`): the Assistant's device is the member's
        // voice device: desktops only, a bound non-desktop stays.
        let all = agent_devices(&team_id, cx);
        let kept = domain::agent_settings::runs_on_device_ids(
            all.iter().map(|device| (device.device_id.as_str(), device.kind.as_deref())),
            agent.device_id.as_deref(),
            true,
        )
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        let devices: Vec<AgentDevice> =
            all.iter().filter(|device| kept.contains(&device.device_id)).cloned().collect();
        let copy = &ASSISTANT.copy;
        let caption = |text: &str, cx: &App| {
            div()
                .px_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(text.to_string()))
        };
        // Identity: the brand mark (never a stored icon) + the name.
        let mark = div()
            .size(gpui::px(28.))
            .flex_shrink_0()
            .rounded_full()
            .bg(cx.theme().muted)
            .flex()
            .items_center()
            .justify_center()
            .child(
                gpui_component::Icon::from(crate::icons::ExpIcon::Logo)
                    .size(gpui::px(16.))
                    .text_color(cx.theme().foreground),
            );
        let identity_row = crate::surface::glass_row_shell().gap_2().child(mark).child(
            div()
                .flex_1()
                .min_w_0()
                .child(crate::surface::glass_row_input(glass_input(&self.name_input, window, cx)).text_left()),
        );
        // Runs on: THE device / account rows, then the model.
        let cli = agent.cli.clone().unwrap_or_else(|| "claude".to_string());
        let models: &[(&str, &str)] = if cli == "codex" {
            &crate::coding_selects::CODEX_MODEL_CHOICES
        } else {
            &crate::coding_selects::MODEL_CHOICES
        };
        // agent-settings.json `modelValueCases`: unpinned = the dispatcher
        // root's default for the login's CLI.
        let model_picked = Some(domain::agent_settings::assistant_model_value(agent.model.as_deref(), Some(&cli)));
        let model_agent = agent.id.clone();
        let model_row = choice_row(
            "assistant-model",
            crate::chat_screen::MENU_MODEL,
            models.iter().map(|(label, value)| ((*value).to_string(), (*label).to_string())).collect(),
            model_picked,
            Rc::new(move |model: String, _: &mut Window, cx: &mut App| {
                update_agent(
                    cx,
                    "change",
                    "the Assistant's model",
                    api::agents::UpdateInput { model: Some(Some(model)), ..api::agents::UpdateInput::new(model_agent.clone()) },
                );
            }),
            cx,
        );
        let runs_on = crate::surface::glass_group_rows(vec![
            self.device_row(agent, &devices, cx),
            self.account_row(agent, &devices, cx),
            model_row,
        ]);
        // Check in: THE Picker over the presets.
        let triggers = agent.trigger_list();
        let schedule = assistant_schedule_of(&triggers).unwrap_or_else(|| ASSISTANT.default_schedule.clone());
        let tz = triggers.first().and_then(|trigger| trigger["tz"].as_str()).map(str::to_string);
        let schedule_agent = agent.id.clone();
        let check_in = crate::surface::glass_group_rows(vec![choice_row(
            "assistant-check-in",
            copy.check_in.clone(),
            ASSISTANT.schedules.iter().map(|preset| (preset.id.clone(), preset.label.clone())).collect(),
            Some(schedule),
            Rc::new(move |id: String, _: &mut Window, cx: &mut App| {
                let triggers = assistant_schedule_triggers(&id, tz.as_deref());
                update_agent(
                    cx,
                    "change",
                    "the check-in",
                    api::agents::UpdateInput { triggers: Some(triggers), ..api::agents::UpdateInput::new(schedule_agent.clone()) },
                );
            }),
            cx,
        )]);
        let voice = self
            .voice
            .get_or_insert_with(|| cx.new(|cx| super::voice_prefs::VoicePrefsPane::new(cx).assistant_block()))
            .clone();
        let mut body = section(cx)
            .gap_4()
            .child(crate::surface::glass_group_rows(vec![identity_row]))
            .child(v_flex().w_full().gap_1().child(caption(&copy.runs_on, cx)).child(runs_on))
            .child(v_flex().w_full().gap_1().child(caption(&copy.check_in, cx)).child(check_in))
            .child(v_flex().w_full().gap_1().child(caption(&copy.voice, cx)).child(voice));
        if let Some(message) = self.error.clone() {
            body = body.child(crate::action_error::field_error(message, cx).px_1());
        }
        v_flex().child(body).into_any_element()
    }
}

impl Render for AgentDetailPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let agent = self
            .selected_agent_id(cx)
            .and_then(|id| crate::session_data::agent_def(&id, cx));
        // The nav clamps a stale Agent(id) selection away before this pane
        // shows, so a missing row is only a transient frame.
        let Some(agent) = agent else {
            return v_flex().into_any_element();
        };
        self.sync_selected_agent(&agent, window, cx);
        if agent.is_assistant() {
            return self.render_assistant(&agent, window, cx);
        }
        let team_id = agent
            .team_id
            .clone()
            .or_else(|| active_team_id(&self.nav, cx))
            .unwrap_or_default();
        let owner = is_owner(cx, &team_id);
        let devices = agent_devices(&team_id, cx);

        // The board form's ONE identity row: icon, colour, name — icon and
        // colour save at once, the name on blur/Enter.
        let defaults = &domain::session_tree::THREADS_LIST.agent_defaults;
        let agent_id = agent.id.clone();
        let icon_picker = crate::board_form::icon_picker(
            "agent-detail",
            registry::PICKABLE_ICONS,
            Some(agent.icon.as_deref().unwrap_or(defaults.icon.as_str())),
            Some(agent.color.as_deref().unwrap_or(defaults.color.as_str())),
            false,
            move |name, _, cx| {
                let Some(name) = name else { return };
                update_agent(
                    cx,
                    "change",
                    "the agent's icon",
                    api::agents::UpdateInput {
                        icon: Some(Some(name.to_string())),
                        ..api::agents::UpdateInput::new(agent_id.clone())
                    },
                );
            },
            cx,
        );
        let agent_id = agent.id.clone();
        let color_picker = crate::board_form::color_picker(
            "agent-detail",
            agent.color.as_deref().unwrap_or(defaults.color.as_str()),
            move |color, _, cx| {
                update_agent(
                    cx,
                    "change",
                    "the agent's color",
                    api::agents::UpdateInput {
                        color: Some(Some(color.to_string())),
                        ..api::agents::UpdateInput::new(agent_id.clone())
                    },
                );
            },
            cx,
        );
        let identity_row = crate::surface::glass_row_shell()
            .gap_2()
            .child(icon_picker)
            .child(color_picker)
            .child(
                div().flex_1().min_w_0().child(
                    crate::surface::glass_row_input(glass_input(&self.name_input, window, cx))
                        .text_left(),
                ),
            );

        let mut runner_rows = vec![self.account_row(&agent, &devices, cx)];
        runner_rows.extend(self.schedule_rows(&agent.id, cx));

        let mut body = section(cx)
            .gap_4()
            .child(crate::surface::glass_group_rows(vec![
                identity_row,
                self.device_row(&agent, &devices, cx),
            ]))
            .child(crate::surface::glass_group_rows(runner_rows))
            .child(self.instructions_card(cx));
        if let Some(message) = self.error.clone() {
            body = body.child(crate::action_error::field_error(message, cx).px_1());
        }
        // The fixture's one owner-only control: hidden from members.
        if owner {
            let agent_id = agent.id.clone();
            body = body.child(
                h_flex().child(
                    Button::new(SharedString::from(format!("agent-detail-delete-{}", agent.id)))
                        .danger()
                        .cursor_pointer()
                        .web_sm()
                        .label(field_label("delete"))
                        .on_click(move |_, window, cx| {
                            Self::open_delete_dialog(agent_id.clone(), window, cx)
                        }),
                ),
            );
        }

        v_flex().child(body).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(agent: &str, id: &str, is_last_used: bool) -> coding::AccountOption {
        coding::AccountOption {
            id: id.to_string(),
            agent: coding::CodingAgent::parse(agent).expect("contract agent"),
            email: format!("{id}@example.com"),
            is_last_used,
            health: coding::Health::Ok,
            limits: None,
        }
    }

    /// The labels come from the fixture's fields, in its words.
    #[test]
    fn field_labels_come_from_the_fixture() {
        assert_eq!(field_label("device"), "Device");
        assert_eq!(field_label("account"), "Account");
        assert_eq!(field_label("triggers"), "Triggers");
        assert_eq!(field_label("instructions"), "Instructions");
        assert_eq!(field_label("delete"), "Delete agent");
        assert_eq!(field_label("nope"), "");
    }

    /// The Account row reads THE rule: the stored profile (of the agent's
    /// CLI), else that CLI's first login, else the machine's last used one.
    #[test]
    fn the_account_row_reads_the_pin_then_the_last_used_login() {
        let options = vec![
            option("codex", "work", true),
            option("claude", "home", false),
            option("claude", "side", false),
        ];
        let pick = |account: Option<&str>, cli: Option<&str>| {
            current_account(&options, account, cli).map(|option| option.id.clone())
        };
        assert_eq!(pick(Some("side"), None), Some("side".to_string()));
        assert_eq!(pick(Some("side"), Some("claude")), Some("side".to_string()));
        // A pin of another CLI = that CLI's first login.
        assert_eq!(pick(Some("side"), Some("codex")), Some("work".to_string()));
        assert_eq!(pick(Some("work"), Some("claude")), Some("home".to_string()));
        // A pin the machine no longer reports, or none.
        assert_eq!(pick(Some("gone"), None), Some("work".to_string()));
        assert_eq!(pick(None, None), Some("work".to_string()));
        // No last used login: the first row.
        let none_last = vec![option("claude", "home", false)];
        assert_eq!(
            current_account(&none_last, None, None).map(|option| option.id.as_str()),
            Some("home")
        );
        assert!(current_account(&[], None, None).is_none());
        // The agent-named unpinned rows are no login: the row reads Last used.
        let unpinned = vec![option("claude", "", true)];
        assert!(current_account(&unpinned, None, Some("claude")).is_none());
    }
}
