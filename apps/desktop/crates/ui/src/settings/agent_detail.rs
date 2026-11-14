//! Settings → one agent's page (EXP-1294 — the Agents group lists every
//! agent root as its own nav entry; this pane is the selected one's page).
//!
//! #EXP-1846 (`fixtures/agent-settings.json` `page`): ONE page for the
//! Assistant and every team agent, each section a band over one glass
//! group: identity (icon picker, colour picker, name; the Assistant: its
//! brand mark + name); Runs on (Device, Account, Model, Skills, MCP
//! servers); Triggers (THE reusable list, [`super::agent_triggers`]: Check
//! in, then the other triggers); Voice (the Assistant only); the
//! Instructions card (team agents only; server-only, `agents.get`, saved on
//! blur); the owner-only "Delete agent" (team agents only). A team agent's
//! editors are its team's owners, the Assistant's its member.
//!
//! Every write is `agents.update` / `agents.delete`; icon,
//! colour, device and account save at once, the name on blur/Enter. A failed
//! write is the "Couldn't …" toast (FEED-79).
//!
//! #EXP-1629: the page is an eUI document on the native binding (genUI
//! IconPicker / ColorPicker, the row Selects with option glyphs and
//! descriptions, Textarea `row`); the pane keeps its state and writes.
//!
//! The pane is a singleton: it reads the selected `Agent(id)` from the
//! window's shared settings selection and re-points itself on agent switches,
//! flushing the previous agent's pending name and instructions first (blur
//! doesn't fire on unmount).

use std::rc::Rc;

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window};
use gpui_component::{button::ButtonVariant, v_flex, ActiveTheme as _};
use serde_json::{json, Value};
use sync::Store;

use domain::agent_settings::{agent_account_value, validate_agent_form, AgentAccountOption, AgentForm, AGENT_SETTINGS};
use domain::rows::AgentDefRow;
use eui_gpui::binding::Binding;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::sidebar::RailShared;
use crate::trigger_editor;

use super::agent_doc::{account_rows, agent_mark_icon, device_row, divided, field_error, identity, instructions_caption, instructions_field, select_row, value_row, Chrome};
use super::agent_triggers::{self, AgentTriggers};
use super::doc::{failed_state, path, Ink, SM};
use super::SettingsSection;

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

/// #EXP-1629: the page is an eUI document on the native binding; the pane
/// keeps its state and writes, the document draws it (`/pane`).
pub struct AgentDetailPane {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    /// The name field's live text (the document's); `None` = untouched since
    /// the agent was loaded.
    name_draft: Option<String>,
    /// The instructions field's live text while it differs from the read.
    instructions_draft: Option<String>,
    /// The agent the fields were last seeded from — a selection switch
    /// flushes the previous agent's pending edits, then re-seeds.
    loaded_agent: Option<String>,
    load: InstructionsLoad,
    /// The form's own validation message (INVALID INPUT, FEED-79).
    error: Option<SharedString>,
    /// #EXP-1846: THE trigger list's state (a whole-array write in flight).
    triggers: AgentTriggers,
    /// Monotonic guard: a stale instructions read must not clobber a newer one.
    generation: u64,
    /// S-3: the Assistant page's Voice section = THE Voice settings block
    /// (`voice_prefs`, its own document), drawn after this page's.
    voice: Option<Entity<super::voice_prefs::VoicePrefsPane>>,
    /// #EXP-1732: THE gear's page in a session screen's pane: this agent,
    /// whatever the settings selection says.
    fixed_agent: Option<String>,
    /// #EXP-1793 (agent-settings.json `mcpRow`): the agent's team's MCP
    /// servers (`mcpServers.list`, server-only: never a shape) and the team
    /// they were asked for.
    mcp: Option<(String, Vec<api::mcp_servers::McpServerListEntry>)>,
    mcp_requested: Option<String>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's owner flag (headless has no signed-in member).
    #[cfg(test)]
    shot_owner: Option<bool>,
    /// The bench's frames left to run a pressed popup's fade on.
    #[cfg(test)]
    shot_settle: u8,
    _subscriptions: Vec<Subscription>,
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

impl AgentDetailPane {
    pub fn new(nav: Entity<Navigation>, shared: Entity<RailShared>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.agents, |_, _, cx| cx.notify()),
            cx.observe(&collections.devices, |_, _, cx| cx.notify()),
            // #EXP-1660: the Skills row names the team's skills.
            cx.observe(&collections.skills, |_, _, cx| cx.notify()),
        ];
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(super::doc::IdeToasts));
            for name in ["icon", "color", "device", "account", "skills", "mcpServers", "model", "retry", "delete"].into_iter().chain(agent_triggers::ACTIONS) {
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
        let view = super::doc::mount_with("settings-agent", document(&Ink::of(cx), &Chrome::of(cx)), &binding, plugin, true, window, cx);
        Self {
            nav,
            shared,
            name_draft: None,
            instructions_draft: None,
            loaded_agent: None,
            load: InstructionsLoad::Loading,
            error: None,
            triggers: AgentTriggers::default(),
            generation: 0,
            voice: None,
            fixed_agent: None,
            mcp: None,
            mcp_requested: None,
            binding,
            view,
            #[cfg(test)]
            shot_owner: None,
            #[cfg(test)]
            shot_settle: 0,
            _subscriptions: subscriptions,
        }
    }

    /// #EXP-1732: the page of `agent_id` as THE gear opens it in a session
    /// screen's pane (an agent's, the Assistant's conversation).
    pub(crate) fn for_agent(agent_id: String, nav: Entity<Navigation>, shared: Entity<RailShared>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut pane = Self::new(nav, shared, window, cx);
        pane.fixed_agent = Some(agent_id);
        pane
    }

    /// The bench's page of `agent_id` (its row seeded into the store).
    #[cfg(test)]
    pub(crate) fn for_shots(agent_id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        let shared = crate::sidebar::rail_shared_for_window(window, cx);
        Self::for_agent(agent_id.to_string(), nav, shared, window, cx)
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

    /// The page in a pinned state: `owner`, and the instructions read
    /// (`None` = still loading, `Some(Err)` = failed).
    #[cfg(test)]
    pub(crate) fn show(&mut self, owner: bool, instructions: Option<Result<String, ()>>, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.shot_owner = Some(owner);
        self.loaded_agent = self.fixed_agent.clone();
        self.load = match instructions {
            None => InstructionsLoad::Loading,
            Some(Ok(text)) => InstructionsLoad::Ready(text),
            Some(Err(())) => InstructionsLoad::Failed,
        };
        cx.notify();
    }

    /// Whether the viewer edits `agent` (`page` EDITORS: a team agent's
    /// team owners, the Assistant's member; the bench pins it).
    fn editor(&self, agent: &AgentDefRow, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(owner) = self.shot_owner {
            return owner;
        }
        agent_triggers::editor_of(agent, team_id, cx)
    }

    /// The selected agent id from the shared settings selection. S-3: the
    /// Voice section (tray / meeting links) resolves to the viewer's
    /// Assistant, as `first_agent_resolved` does for the screen.
    fn selected_agent_id(&self, cx: &App) -> Option<String> {
        if let Some(agent_id) = &self.fixed_agent {
            return Some(agent_id.clone());
        }
        match self.shared.read(cx).settings_section() {
            SettingsSection::Agent(id) => Some(id),
            SettingsSection::Voice => active_team_id(&self.nav, cx)
                .and_then(|team_id| crate::session_data::my_assistant(&team_id, cx))
                .map(|assistant| assistant.id),
            _ => None,
        }
    }

    /// Re-seed the fields when the selected agent changes, flushing the
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
        self.name_draft = None;
        self.instructions_draft = None;
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
            let result = window.background_executor().spawn(async move { api::agents::get(&trpc, &agent_id) }).await;
            let _ = this.update_in(window, |this, _, cx| {
                if this.generation != generation {
                    return; // superseded by a newer read
                }
                this.load = match result {
                    Ok(details) => InstructionsLoad::Ready(details.instructions),
                    Err(err) => {
                        log::warn!("[ui] agents.get failed: {err}");
                        InstructionsLoad::Failed
                    }
                };
                this.instructions_draft = None;
                cx.notify();
            });
        })
        .detach();
    }

    /// #EXP-1793: one `mcpServers.list` per team (it carries the viewer's
    /// own connection to each server, which greys the rows).
    fn ensure_mcp_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if team_id.is_empty() || self.mcp_requested.as_deref() == Some(team_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.mcp_requested = Some(team_id.to_string());
        let team = team_id.to_string();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn({
                    let team = team.clone();
                    async move { api::mcp_servers::list(&trpc, &team) }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.mcp_requested.as_deref() != Some(team.as_str()) {
                    return;
                }
                match loaded {
                    Ok(entries) => this.mcp = Some((team, entries)),
                    Err(err) => log::debug!("[ui] mcpServers.list for the agent page: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The bench's team MCP servers (headless has no server).
    #[cfg(test)]
    pub(crate) fn seed_mcp_for_shots(&mut self, team_id: &str, entries: Vec<api::mcp_servers::McpServerListEntry>, cx: &mut gpui::Context<Self>) {
        self.mcp_requested = Some(team_id.to_string());
        self.mcp = Some((team_id.to_string(), entries));
        cx.notify();
    }

    /// #EXP-1793: the team's MCP servers as the page offers them (team list
    /// order), `[]` until the list landed.
    fn team_mcp(&self, team_id: &str) -> &[api::mcp_servers::McpServerListEntry] {
        match &self.mcp {
            Some((team, entries)) if team == team_id => entries,
            _ => &[],
        }
    }

    /// The form as it stands, for the fixture's validation.
    fn form(&self, agent: Option<&AgentDefRow>) -> AgentForm {
        let saved = match &self.load {
            InstructionsLoad::Ready(text) => text.clone(),
            _ => String::new(),
        };
        AgentForm {
            name: self.name_draft.clone().unwrap_or_else(|| agent.and_then(|agent| agent.name.clone()).unwrap_or_default()),
            device_id: agent.and_then(|agent| agent.device_id.clone()),
            instructions: self.instructions_draft.clone().unwrap_or(saved),
        }
    }

    /// The first fixture error of `field`, if the form has one.
    fn field_error(&self, field: &str, agent: Option<&AgentDefRow>) -> Option<String> {
        validate_agent_form(&self.form(agent)).into_iter().find(|error| error.field == field).map(|error| error.message)
    }

    /// `agents.update({name})` when the drafted name is valid and differs
    /// from the synced row (keyed on the LOADED agent).
    fn commit_rename(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(agent_id) = self.loaded_agent.clone() else {
            return;
        };
        let Some(agent) = crate::session_data::agent_def(&agent_id, cx) else {
            return;
        };
        let Some(drafted) = self.name_draft.as_deref().map(str::trim).map(str::to_string) else {
            return;
        };
        if let Some(message) = self.field_error("identity", Some(&agent)) {
            self.error = Some(message.into());
            cx.notify();
            return;
        }
        if agent.name.as_deref() == Some(drafted.as_str()) {
            return;
        }
        self.error = None;
        cx.notify();
        update_agent(cx, "rename", "the agent", api::agents::UpdateInput { name: Some(drafted), ..api::agents::UpdateInput::new(agent_id) });
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
        let Some(drafted) = self.instructions_draft.clone() else {
            return;
        };
        if let Some(message) = self.field_error("instructions", Some(&agent)) {
            self.error = Some(message.into());
            cx.notify();
            return;
        }
        if &drafted == saved {
            return;
        }
        self.load = InstructionsLoad::Ready(drafted.clone());
        self.instructions_draft = None;
        self.error = None;
        cx.notify();
        update_agent(cx, "save", "the instructions", api::agents::UpdateInput { instructions: Some(drafted), ..api::agents::UpdateInput::new(agent_id) });
    }

    /// A field's edit (the plugin's): the live text, and on commit (blur /
    /// Enter) its save.
    fn edited(&mut self, field: &str, value: String, commit: bool, cx: &mut gpui::Context<Self>) {
        match field {
            "name" => {
                self.name_draft = Some(value);
                if commit {
                    self.commit_rename(cx);
                }
            }
            "instructions" => {
                self.instructions_draft = Some(value);
                if commit {
                    self.commit_instructions(cx);
                }
            }
            _ => {}
        }
    }

    /// The Delete confirm: the fixture's `deleteConfirm`, a Danger answer
    /// (Return lands on Cancel). The nav clamps the stale `Agent(id)` away on
    /// the sessions delta.
    fn open_delete_dialog(agent_id: String, window: &mut Window, cx: &mut App) {
        let spec = AlertSpec::new(field_label("delete"), AGENT_SETTINGS.copy.delete_confirm.clone(), "Delete").ok_variant(ButtonVariant::Danger).on_ok(move |_, cx| {
            let agent_id = agent_id.clone();
            spawn_agent_write(cx, "delete", "the agent", move |trpc| api::agents::delete(trpc, &agent_id));
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// The machines the page offers: every team machine; the Assistant's
    /// (NEW-99 `runsOnCases`) desktops only, a bound non-desktop kept.
    fn devices(&self, agent: &AgentDefRow, team_id: &str, cx: &App) -> Vec<AgentDevice> {
        let all = agent_devices(team_id, cx);
        if !agent.is_assistant() {
            return all;
        }
        let kept = domain::agent_settings::runs_on_device_ids(all.iter().map(|device| (device.device_id.as_str(), device.kind.as_deref())), agent.device_id.as_deref(), true)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        all.iter().filter(|device| kept.contains(&device.device_id)).cloned().collect()
    }

    /// A document action.
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(agent) = self.selected_agent_id(cx).and_then(|id| crate::session_data::agent_def(&id, cx)) else {
            return;
        };
        let agent_id = agent.id.clone();
        let team_id = agent.team_id.clone().or_else(|| active_team_id(&self.nav, cx)).unwrap_or_default();
        let editor = self.editor(&agent, &team_id, cx);
        // #EXP-1846: THE trigger list's own actions.
        if agent_triggers::ACTIONS.contains(&name) {
            agent_triggers::handle(self, |pane| &mut pane.triggers, name, agent_triggers::ListTarget::Agent(agent_id), editor, context, payload, window, cx);
            return;
        }
        // A member reads every row.
        if !editor && name != "retry" {
            return;
        }
        // #FEED-247: a popup opening or closing fires `change` too, with no
        // value: never a pick.
        let picked = super::doc::picked_value(payload).is_some();
        let value = super::doc::picked_value(payload).unwrap_or_default().to_string();
        match name {
            "icon" if !value.is_empty() => update_agent(cx, "change", "the agent's icon", api::agents::UpdateInput { icon: Some(Some(value)), ..api::agents::UpdateInput::new(agent_id) }),
            "color" if !value.is_empty() => update_agent(cx, "change", "the agent's color", api::agents::UpdateInput { color: Some(Some(value)), ..api::agents::UpdateInput::new(agent_id) }),
            // A machine re-binds the agent and drops its account pin (a
            // profile id is device-local, EXP-995).
            "device" if !value.is_empty() && agent.device_id.as_deref() != Some(value.as_str()) => {
                update_agent(cx, "change", "the agent's device", api::agents::UpdateInput { device_id: Some(value), account: Some(None), ..api::agents::UpdateInput::new(agent_id) })
            }
            "account" if picked => {
                let devices = self.devices(&agent, &team_id, cx);
                let options = agent.device_id.as_deref().and_then(|id| devices.iter().find(|device| device.device_id == id)).map(account_options).unwrap_or_default();
                if let Some(option) = options.iter().find(|option| option.account_option_key() == value) {
                    update_agent(cx, "change", "the agent's account", api::agents::UpdateInput { account: Some(option.wire_account()), ..api::agents::UpdateInput::new(agent_id) });
                }
            }
            "skills" if picked => {
                let ids: Vec<String> = value.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect();
                update_agent(cx, "change", "the agent's skills", api::agents::UpdateInput { skill_ids: Some(ids), ..api::agents::UpdateInput::new(agent_id) });
            }
            // #EXP-1793 (agent-settings.json `mcpRow`): THE composer's MCP
            // servers rows: a server the viewer has not connected opens
            // Settings > MCP servers to connect instead of toggling.
            "mcpServers" if picked => {
                let entries = self.team_mcp(&team_id);
                let ids: Vec<String> = value.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect();
                let blocked_new = ids.iter().any(|id| {
                    !agent.mcp_server_ids.contains(id)
                        && entries.iter().any(|entry| &entry.config.id == id && crate::launch_options::mcp_block_reason(&entry.connection).is_some())
                });
                if blocked_new {
                    crate::navigation::navigate(window, cx, crate::navigation::Screen::Settings);
                    crate::sidebar::select_settings_section(window, cx, SettingsSection::Mcp);
                    cx.notify();
                    return;
                }
                // Team list order; at most `mcpRow.max`.
                let mut picked: Vec<String> = entries.iter().filter(|entry| ids.contains(&entry.config.id)).map(|entry| entry.config.id.clone()).collect();
                picked.truncate(domain::agent_settings::MCP_ROW_MAX);
                update_agent(cx, "change", "the agent's MCP servers", api::agents::UpdateInput { mcp_server_ids: Some(picked), ..api::agents::UpdateInput::new(agent_id) });
            }
            "model" if !value.is_empty() && agent.model.as_deref() != Some(value.as_str()) => {
                update_agent(cx, "change", "the agent's model", api::agents::UpdateInput { model: Some(Some(value)), ..api::agents::UpdateInput::new(agent_id) })
            }
            "retry" => self.fetch_instructions(window, cx),
            "delete" => Self::open_delete_dialog(agent_id, window, cx),
            _ => {}
        }
    }

    /// The page as the document binds it.
    fn project(&self, agent: &AgentDefRow, team_id: &str, cx: &App) -> Value {
        let owner = self.editor(agent, team_id, cx);
        let assistant = agent.is_assistant();
        let defaults = &domain::session_tree::CHATS_LIST.agent_defaults;
        let devices = self.devices(agent, team_id, cx);
        // THE device picker: each machine by its glyph, offline ones say so;
        // a bound device that isn't synced here keeps its raw id.
        let mut device_options: Vec<Value> = devices
            .iter()
            .map(|device| {
                let mut option = json!({"label": device.label, "value": device.device_id, "icon": crate::icons::device_icon_name(device.icon.as_deref(), device.server)});
                if !device.online {
                    option["description"] = json!("Offline");
                }
                option
            })
            .collect();
        if let Some(bound) = agent.device_id.as_deref() {
            if !devices.iter().any(|device| device.device_id == bound) {
                device_options.push(json!({"label": bound, "value": bound}));
            }
        }
        // The Account row: THE account picker over the bound machine's
        // logins (brand mark + email), "Last used" when it reports none.
        let account_options_list = agent.device_id.as_deref().and_then(|id| devices.iter().find(|device| device.device_id == id)).map(account_options).unwrap_or_default();
        let current = current_account(&account_options_list, agent.account.as_deref(), agent.cli.as_deref()).cloned();
        let logins: Vec<Value> = account_options_list
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
        // #EXP-1660 (skills.json `agentRow`): the agent's default skills,
        // list order; an id whose skill is gone is not shown.
        let (rows, _) = queries::team_skills(cx, team_id);
        let picked: Vec<&domain::rows::SkillRow> = rows.iter().filter(|skill| agent.skill_ids.iter().any(|id| id == &skill.id)).collect();
        let names: Vec<&str> = picked.iter().map(|skill| skill.name.as_str()).collect();
        // #EXP-1793 (agent-settings.json `mcpRow`): the agent's MCP servers,
        // team list order; shown while the team has one or the agent names one.
        let mcp_entries = self.team_mcp(team_id);
        let mcp_picked: Vec<&api::mcp_servers::McpServerListEntry> = mcp_entries.iter().filter(|entry| agent.mcp_server_ids.contains(&entry.config.id)).collect();
        let mcp_names: Vec<&str> = mcp_picked.iter().map(|entry| entry.config.name.as_str()).collect();
        let mcp_shown = !mcp_entries.is_empty() || !agent.mcp_server_ids.is_empty();
        // #EXP-1846: every agent's Model row (unpinned = the device default
        // for the login's CLI).
        let cli = agent.cli.clone().unwrap_or_else(|| "claude".to_string());
        let models: &[(&str, &str)] = if cli == "codex" { &crate::coding_selects::CODEX_MODEL_CHOICES } else { &crate::coding_selects::MODEL_CHOICES };
        let instructions_text = match &self.load {
            InstructionsLoad::Ready(text) => self.instructions_draft.clone().unwrap_or_else(|| text.clone()),
            _ => String::new(),
        };
        json!({
            "assistant": assistant,
            "agentPage": !assistant,
            "owner": owner,
            "member": !owner,
            "icon": agent.icon.clone().unwrap_or_else(|| defaults.icon.clone()),
            "color": agent.color.clone().unwrap_or_else(|| defaults.color.clone()),
            "fields": {"name": self.name_draft.clone().unwrap_or_else(|| agent.name.clone().unwrap_or_default()), "instructions": instructions_text},
            "device": {"value": agent.device_id.clone().unwrap_or_default(), "options": device_options},
            "account": {"has": current.is_some(), "none": current.is_none(), "value": current.as_ref().map(|option| option.account_option_key()).unwrap_or_default(), "options": logins},
            "skills": {"value": picked.iter().map(|skill| skill.id.as_str()).collect::<Vec<_>>().join(","), "summary": domain::skills::agent_skills_value(&names), "empty": names.is_empty(), "some": !names.is_empty(),
                "options": rows.iter().map(|skill| {
                    // THE skills picker's row: the docs glyph, the name, the description caption.
                    let mut option = json!({"label": skill.name, "value": skill.id, "icon": "ui-docs"});
                    if !skill.description.is_empty() {
                        option["description"] = json!(skill.description);
                    }
                    option
                }).collect::<Vec<_>>()},
            "mcpServers": {"value": mcp_picked.iter().map(|entry| entry.config.id.as_str()).collect::<Vec<_>>().join(","), "summary": domain::agent_settings::agent_mcp_servers_value(&mcp_names), "empty": mcp_names.is_empty(),
                "pick": owner && mcp_shown, "read": !owner && mcp_shown,
                "options": mcp_entries.iter().map(mcp_option).collect::<Vec<_>>()},
            "model": {"value": domain::agent_settings::assistant_model_value(agent.model.as_deref(), Some(&cli)), "options": models.iter().map(|(label, value)| json!({"label": label, "value": value})).collect::<Vec<_>>()},
            "triggerList": agent_triggers::project(&self.triggers, &agent.id, owner, cx),
            "instructionsLoading": matches!(self.load, InstructionsLoad::Loading),
            "instructionsFailed": matches!(self.load, InstructionsLoad::Failed),
            "instructionsReady": matches!(self.load, InstructionsLoad::Ready(_)),
            "error": self.error.as_ref().map(|message| message.to_string()).unwrap_or_default(),
            // The Assistant's message follows its Voice block (`render`).
            "hasError": self.error.is_some() && !assistant,
            "canDelete": owner && !assistant,
        })
    }
}

impl Render for AgentDetailPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AgentDetailPane").entered();
        #[cfg(test)]
        if self.shot_settle > 0 {
            self.shot_settle -= 1;
            self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            cx.notify();
        }
        let agent = self.selected_agent_id(cx).and_then(|id| crate::session_data::agent_def(&id, cx));
        // The nav clamps a stale Agent(id) selection away before this pane
        // shows, so a missing row is only a transient frame.
        let Some(agent) = agent else {
            return div().into_any_element();
        };
        self.sync_selected_agent(&agent, window, cx);
        let team_id = agent.team_id.clone().or_else(|| active_team_id(&self.nav, cx)).unwrap_or_default();
        self.ensure_mcp_loaded(&team_id, cx);
        let projection = self.project(&agent, &team_id, cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        let page = div().w_full().min_w_0().child(self.view.clone());
        if !agent.is_assistant() {
            return page.into_any_element();
        }
        // S-3: the Voice block (its own document) right under the page's
        // Voice band; the form's error after it.
        let voice = self.voice.get_or_insert_with(|| cx.new(|cx| super::voice_prefs::VoicePrefsPane::assistant(window, cx))).clone();
        let mut column = v_flex().w_full().min_w_0().child(page).child(voice);
        if let Some(message) = self.error.clone() {
            column = column.child(div().pt_4().child(crate::action_error::field_error(message, cx).px_1()));
        }
        column.into_any_element()
    }
}

/// The pane's plugin: the IDE's, and the name / instructions edits for
/// the pane.
struct PanePlugin {
    pane: gpui::WeakEntity<AgentDetailPane>,
    inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        // #EXP-1793: an MCP server's catalog brand mark (THE picker's glyph).
        if let Some(slug) = name.strip_prefix(MCP_BRAND_PREFIX) {
            use gpui_component::IconNamed as _;
            return crate::icons::registry::brand::by_slug(slug).map(|icon| icon.path());
        }
        self.inner.icon(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        // The Assistant's mark is the IDE's own logo (the rail's
        // `ExpIcon::Logo`, full bleed) at three quarters of its 36 px disc
        // (web `AssistantMark size-9`), past the theme's icon rung.
        if name == "brand-exponential" {
            return Some(gpui_component::Icon::from(crate::icons::ExpIcon::Logo).size(gpui::px(27.)).text_color(color).into_any_element());
        }
        self.inner.icon_element(name, size, color, cx)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn on_input(&self, event: &eui_gpui::host::InputEvent, cx: &mut App) {
        let commit = event.kind == eui_gpui::host::InputKind::Commit;
        let field = event.name.clone();
        let value = event.value.as_str().unwrap_or_default().to_string();
        let pane = self.pane.clone();
        cx.defer(move |cx| {
            let _ = pane.update(cx, |pane, cx| pane.edited(&field, value, commit, cx));
        });
    }
}

/// The Skills row (owners pick, members read).
fn skills_rows(ink: &Ink) -> [Value; 2] {
    let label = domain::skills::AGENT_ROW_LABEL;
    let mut pick = select_row("skills", label, "skills", "skills", json!({"multiple": true, "searchable": true, "placeholder": domain::skills::agent_skills_value(&[])}));
    pick["visible"] = path("/pane/owner");
    [pick, value_row(ink, "skills_read", path("/pane/member"), label, path("/pane/skills/summary"), path("/pane/skills/empty"))]
}

/// #EXP-1793: an MCP server's catalog brand as a document icon name (the
/// plugin resolves it).
const MCP_BRAND_PREFIX: &str = "mcp-brand:";

/// #EXP-1793: one MCP servers option as THE composer's picker draws it (the
/// server's mark, its name, the host or command; a server the viewer has
/// not connected says so, `launch_options::mcp_block_reason`).
fn mcp_option(entry: &api::mcp_servers::McpServerListEntry) -> Value {
    let config = &entry.config;
    let url = config.url.as_deref().filter(|url| !url.trim().is_empty());
    let icon = match crate::picker::mcp_server_picker::mcp_catalog_entry_for(url).filter(|catalog| crate::icons::registry::brand::by_slug(catalog.mark).is_some()) {
        Some(catalog) => format!("{MCP_BRAND_PREFIX}{}", catalog.mark),
        None if url.is_none() && config.command.as_deref().is_some_and(|command| !command.trim().is_empty()) => "session-shell".to_string(),
        None => "ui-mcp".to_string(),
    };
    let description = crate::launch_options::mcp_block_reason(&entry.connection)
        .or_else(|| crate::picker::mcp_server_picker::mcp_host(url))
        .or_else(|| config.command.clone().filter(|command| !command.is_empty()));
    let mut option = json!({"label": config.name, "value": config.id, "icon": icon});
    if let Some(description) = description {
        option["description"] = json!(description);
    }
    option
}

/// #EXP-1793 (agent-settings.json `mcpRow`): the MCP servers row (owners
/// pick, members read), shown while the team has a server or the agent
/// names one.
fn mcp_rows(ink: &Ink) -> [Value; 2] {
    let label = domain::agent_settings::MCP_ROW_LABEL;
    let mut pick = select_row("mcp_servers", label, "mcpServers", "mcpServers", json!({"multiple": true, "searchable": true, "placeholder": domain::agent_settings::agent_mcp_servers_value(&[])}));
    pick["visible"] = path("/pane/mcpServers/pick");
    [pick, value_row(ink, "mcp_servers_read", path("/pane/mcpServers/read"), label, path("/pane/mcpServers/summary"), path("/pane/mcpServers/empty"))]
}

/// The Instructions card: the chrome-less field in a glass group, its label
/// as the caption UNDER it; the read's skeleton, or its Retry when it failed.
fn instructions(ink: &Ink, chrome: &Chrome) -> Value {
    let field = instructions_field(path("/pane/instructionsReady"));
    // `feedback::skeleton_text`: three bars (280, 220, 160) `gap_3`, `p_4`.
    let bars: Vec<Value> = [280., 220., 160.].iter().enumerate().map(|(ix, w): (usize, &f32)| json!({"id": format!("instructions_bar_{ix}"), "component": "Skeleton", "props": {"width": w.to_string(), "height": "8"}, "style": {"width": w, "height": 8, "flexShrink": 0, "borderRadius": 4, "backgroundColor": chrome.skeleton}})).collect();
    let loading = json!({"id": "instructions_loading", "component": "Box", "visible": path("/pane/instructionsLoading"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "width": "100%", "padding": ink.r(1.)}, "children": bars});
    let failed = failed_state(ink, "instructions_failed", "settings-agents", "Couldn't load the instructions", path("/pane/instructionsFailed"));
    let caption = instructions_caption(ink);
    json!({"id": "instructions_card", "component": "Box", "visible": path("/pane/agentPage"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25), "width": "100%"},
        "children": [ink.group("instructions_group", vec![loading, failed, field]), caption]})
}

/// The Assistant's identity: the brand mark in a 36 px muted disc (never a
/// stored icon, web `AssistantMark size-9`) + the name.
fn assistant_identity(ink: &Ink, chrome: &Chrome) -> Value {
    let mark = json!({"id": "assistant_mark", "component": "Box", "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "width": 36, "height": 36, "flexShrink": 0, "borderRadius": 9999, "backgroundColor": chrome.disc},
        "children": [{"id": "assistant_mark_icon", "component": "Icon", "props": {"name": "brand-exponential"}, "style": {"width": 27, "height": 27, "color": ink.foreground}}]});
    let name = json!({"id": "assistant_name", "component": "Input", "props": {"variant": "bare", "label": "Name", "name": "name", "placeholder": AGENT_SETTINGS.copy.name_placeholder.clone(), "value": path("/pane/fields/name")}, "style": {"flexGrow": 1, "minWidth": 0}});
    let mut row = ink.shell("assistant_identity", false, json!(true), vec![mark, name]);
    row["style"]["gap"] = json!(ink.r(0.5));
    row
}

/// A band over its group (EXP-1557: each Assistant group heads with THE band).
fn banded(ink: &Ink, id: &str, label: &str, group: Value) -> Value {
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [ink.band(&format!("{id}_band"), label, None), group]})
}

/// The pane's tree (`gap_4`, #EXP-1846 `page`): identity; Runs on; the
/// Triggers list; the Voice band (the Assistant's; its block follows the
/// document); Instructions, the error and the owner's Delete (team agents).
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let copy = &AGENT_SETTINGS.copy;
    let assistant_copy = &domain::agent_settings::ASSISTANT.copy;
    // Identity: the team agent's icon + colour + name, or the Assistant's mark + name.
    let mut agent_identity = identity(ink);
    agent_identity["visible"] = path("/pane/agentPage");
    let mut mark_identity = assistant_identity(ink, chrome);
    mark_identity["visible"] = path("/pane/assistant");
    let identity_group = ink.group("identity_group", vec![agent_identity, mark_identity]);
    // Runs on: device, account, model, skills, MCP servers (members read).
    let read_only = |mut row: Value| {
        row["props"]["disabled"] = path("/pane/member");
        row
    };
    let [skills_pick, skills_read] = skills_rows(ink);
    let [mcp_pick, mcp_read] = mcp_rows(ink);
    let [account_pick, account_none] = account_rows(ink);
    let mut runs_on = vec![read_only(device_row())];
    runs_on.extend(divided(ink, read_only(account_pick)));
    runs_on.extend(divided(ink, account_none));
    runs_on.extend(divided(ink, read_only(select_row("model", crate::chat_screen::MENU_MODEL, "model", "model", json!({})))));
    runs_on.extend(divided(ink, skills_pick));
    runs_on.extend(divided(ink, skills_read));
    runs_on.extend(divided(ink, mcp_pick));
    runs_on.extend(divided(ink, mcp_read));
    let mut voice = ink.band("voice_band", &assistant_copy.voice, None);
    voice["visible"] = path("/pane/assistant");
    let error = field_error(ink, chrome, "error", path("/pane/error"), path("/pane/hasError"));
    let size = ink.r(SM);
    let delete = json!({"id": "delete", "component": "Box", "visible": path("/pane/canDelete"), "props": {"pressable": true}, "accessibility": {"label": field_label("delete")}, "on": {"press": {"event": {"name": "delete"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "alignSelf": "flex-start", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": chrome.radius, "backgroundColor": chrome.danger_bg, "cursor": "pointer", ":hover": {"backgroundColor": chrome.danger_hover}},
        "children": [
            {"id": "delete_icon", "component": "Icon", "props": {"name": "ui-delete"}, "style": {"width": ink.r(SM), "height": ink.r(SM), "color": chrome.danger_fg, "flexShrink": 0}},
            {"id": "delete_text", "component": "Text", "props": {"text": field_label("delete")}, "style": {"fontSize": size, "lineHeight": size, "color": chrome.danger_fg, "flexShrink": 0}},
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.), "width": "100%", "minWidth": 0},
        "children": [
            identity_group,
            banded(ink, "runs_on", &copy.runs_on, ink.group("runs_on_group", runs_on)),
            agent_triggers::section(ink, chrome),
            voice,
            instructions(ink, chrome),
            error,
            delete,
        ]})
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
        let mut surface = eui_core::surface::Surface::new("settings-agent", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

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
        let options = vec![option("codex", "work", true), option("claude", "home", false), option("claude", "side", false)];
        let pick = |account: Option<&str>, cli: Option<&str>| current_account(&options, account, cli).map(|option| option.id.clone());
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
        assert_eq!(current_account(&none_last, None, None).map(|option| option.id.as_str()), Some("home"));
        assert!(current_account(&[], None, None).is_none());
        // The agent-named unpinned rows are no login: the row reads Last used.
        let unpinned = vec![option("claude", "", true)];
        assert!(current_account(&unpinned, None, Some("claude")).is_none());
    }
}
