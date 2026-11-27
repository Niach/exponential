//! Settings → one agent's page (EXP-1294 — the Agents group lists every
//! agent root as its own nav entry; this pane is the selected one's page).
//!
//! #EXP-1846 (`fixtures/agent-settings.json` `page`): ONE page for the
//! Assistant and every team agent, each section a band over one glass
//! group: identity (icon picker, colour picker, name; the Assistant: its
//! brand mark + name); Runs on (Device, Account, Model, Skills, MCP
//! servers); Triggers (THE reusable list, [`super::agent_triggers`]: Check
//! in, then the other triggers); Voice (#EXP-1847: EVERY agent's own
//! voice, `agent-settings.json` `voice`: Language, the clip, Transcript,
//! Wake phrase; #EXP-1859: a team agent's Wake word switch, the member's
//! own `wakeAgentIds`; the Assistant's member rows follow it); the
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
use domain::apps::APPS;
use domain::rows::AgentDefRow;
use eui_gpui::binding::Binding;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::sidebar::RailShared;
use crate::trigger_editor;

use super::agent_doc::{account_rows, agent_mark_icon, device_row, divided, field_error, identity, instructions_caption, instructions_field, select_row, value_row, voice_rows, Chrome};
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
    /// #EXP-1849: shared with the agent's team (an app agent's slot).
    pub(crate) team_shared: bool,
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
                team_shared: row.is_shared_with(team_id),
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
    /// #EXP-1847: the Voice rows' typed text (`None` = the resolved value),
    /// the wake phrase's inline error, a clip upload / removal in flight and
    /// the wake placeholder the mounted tree was built with.
    transcript_draft: Option<String>,
    wake_draft: Option<String>,
    wake_invalid: Option<String>,
    clip_busy: bool,
    wake_placeholder: String,
    /// #EXP-1847: the member's voice settings (`voice.get`, once per page),
    /// every agent's voice fallback, as on web; `None` until they landed
    /// (the settings this IDE last received stand in).
    member_voice: Option<etalk::voice::VoiceSettings>,
    member_voice_requested: bool,
    /// #EXP-1859: the Wake word row's refusal (`voice.wakeAgent`'s
    /// BAD_REQUEST, or `wake_toggle`'s own), inline until the next flip, and
    /// the newest flip (last write wins).
    wake_word_error: Option<String>,
    wake_word_write: u64,
    /// #EXP-1849 (apps.json `agentSlot`): THE app agent's slot, the App
    /// settings' Agent group (`Some(editor)`): only the Runs on rows, the
    /// device = `Each member's own device` or a team-shared one.
    slot: Option<bool>,
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
        Self::mount(nav, shared, false, window, cx)
    }

    fn mount(nav: Entity<Navigation>, shared: Entity<RailShared>, slot: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
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
            for name in ["icon", "color", "device", "account", "skills", "mcpServers", "model", "retry", "delete", "voiceLanguage", "voiceClip", "voiceClipRemove", "voiceWakeWord"].into_iter().chain(agent_triggers::ACTIONS) {
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
        let wake_placeholder = domain::agent_settings::agent_wake_phrase_placeholder("");
        let view = if slot {
            super::doc::mount_with("settings-app-agent", slot_document(&Ink::of(cx)), &binding, plugin, true, window, cx)
        } else {
            super::doc::mount_with("settings-agent", document(&Ink::of(cx), &Chrome::of(cx), &wake_placeholder), &binding, plugin, true, window, cx)
        };
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
            transcript_draft: None,
            wake_draft: None,
            wake_invalid: None,
            clip_busy: false,
            wake_placeholder,
            member_voice: None,
            member_voice_requested: false,
            wake_word_error: None,
            wake_word_write: 0,
            slot: slot.then_some(false),
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

    /// #EXP-1849 (apps.json `agentSlot`): the App settings' Agent group over
    /// the app's agent `agent_id`; read-only until [`Self::set_slot_editor`].
    pub(crate) fn for_app_slot(agent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        let shared = crate::sidebar::rail_shared_for_window(window, cx);
        let mut pane = Self::mount(nav, shared, true, window, cx);
        pane.fixed_agent = Some(agent_id);
        pane
    }

    /// #EXP-1849: the slot's editors = the app's creator and the team's
    /// owners (the app pane's `can_manage`).
    pub(crate) fn set_slot_editor(&mut self, editor: bool, cx: &mut gpui::Context<Self>) {
        if self.slot.is_some() && self.slot != Some(editor) {
            self.slot = Some(editor);
            cx.notify();
        }
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

    /// The bench's Assistant page with its Voice block loaded.
    #[cfg(test)]
    pub(crate) fn show_voice(&mut self, shot: super::voice_prefs::VoiceShot, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let voice = self.voice.get_or_insert_with(|| cx.new(|cx| super::voice_prefs::VoicePrefsPane::assistant(window, cx))).clone();
        voice.update(cx, |voice, cx| voice.show(shot, cx));
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
        if let Some(editor) = self.slot {
            return editor;
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
        self.transcript_draft = None;
        self.wake_draft = None;
        self.wake_invalid = None;
        self.wake_word_error = None;
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
            "voiceTranscript" | "voiceWakePhrase" => self.edited_voice(field, value, commit, cx),
            _ => {}
        }
    }

    /// #EXP-1847: `voice.get`, once per page (a failed read keeps the
    /// settings this IDE last received; the next page asks again).
    fn ensure_member_voice(&mut self, cx: &mut gpui::Context<Self>) {
        if self.member_voice_requested {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.member_voice_requested = true;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::voice::voice_get(&trpc) }).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(settings) => {
                    this.member_voice = Some(settings);
                    cx.notify();
                }
                Err(err) => log::debug!("[ui] voice.get for the agent page: {err}"),
            });
        })
        .detach();
    }

    /// The bench's member voice settings (headless has no server).
    #[cfg(test)]
    pub(crate) fn seed_member_voice_for_shots(&mut self, settings: etalk::voice::VoiceSettings, cx: &mut gpui::Context<Self>) {
        self.member_voice_requested = true;
        self.member_voice = Some(settings);
        cx.notify();
    }

    /// #EXP-1847: the agent's resolved voice (`agentVoiceOf`) against the
    /// member's voice settings (`voice.get`; until it landed, the settings
    /// this IDE last received).
    fn resolved_voice(&self, agent: &AgentDefRow, cx: &App) -> domain::agent_settings::ResolvedAgentVoice {
        let member = self.member_voice.clone().unwrap_or_else(|| etalk_gpui::speak_replies::settings(cx));
        let member = domain::agent_settings::MemberVoice {
            language: member.language(),
            voice_file: &member.voice_file,
            voice_transcript: &member.voice_transcript,
            wake_phrase: &member.wake_phrase,
        };
        domain::agent_settings::agent_voice_of(Some(&agent.role), agent.name.as_deref().unwrap_or_default(), &agent.voice, &member)
    }

    /// The member's voice settings (`voice.get`; until it landed, the
    /// settings this IDE last received).
    fn member_settings(&self, cx: &App) -> etalk::voice::VoiceSettings {
        self.member_voice.clone().unwrap_or_else(|| etalk_gpui::speak_replies::settings(cx))
    }

    /// #EXP-1859 (`voice.wake` `wakeRow`): the `wake-word` row of `agent`
    /// and its detail while it listens (the Assistant's row's `wakeDetail`).
    fn wake_word_row(&self, agent: &AgentDefRow, cx: &App) -> (domain::agent_settings::WakeRow, Option<String>) {
        let settings = self.member_settings(cx);
        let rows = synced_agents(cx);
        let agents: Vec<domain::agent_settings::WakeAgent<'_>> = rows.iter().map(domain::agent_settings::WakeAgent::of).collect();
        let input = wake_input(&settings);
        let row = domain::agent_settings::wake_row(&input, &agents, &agent.id);
        let listening = row.on && domain::agent_settings::wake_listeners(&input, &agents).iter().any(|listener| listener.agent_id == agent.id);
        let detail = listening.then(|| super::voice_prefs::wake_word_detail(&settings, cx)).flatten();
        (row, detail)
    }

    /// #EXP-1859: a flip of the `wake-word` row = `voice.wakeAgent`, through
    /// `wake_toggle` first: its refusal shows inline and is never sent; else
    /// the flip shows at once, the stored settings adopt (and reach this
    /// IDE's voice), a BAD_REQUEST reverts with its copy inline, any other
    /// failure reverts and toasts.
    fn flip_wake_word(&mut self, agent_id: String, on: bool, cx: &mut gpui::Context<Self>) {
        use domain::agent_settings::{wake_toggle, WakeAgent, WakeRefusal};
        let previous = self.member_settings(cx);
        let rows = synced_agents(cx);
        let agents: Vec<WakeAgent<'_>> = rows.iter().map(WakeAgent::of).collect();
        let toggle = wake_toggle(&wake_input(&previous), &agents, &agent_id, on);
        match toggle.error {
            Some(WakeRefusal::Invalid(message)) => {
                self.wake_word_error = Some(message);
                cx.notify();
                return;
            }
            Some(WakeRefusal::NotFound) => {
                crate::action_error::failed_action("change", "the wake word", None, cx);
                return;
            }
            None => {}
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let mut next = previous.clone();
        next.wake_word = toggle.wake_word;
        next.wake_agent_ids = toggle.wake_agent_ids;
        self.member_voice = Some(next);
        self.wake_word_error = None;
        self.wake_word_write += 1;
        let write = self.wake_word_write;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::voice::voice_wake_agent(&trpc, &agent_id, on) }).await;
            let _ = this.update(cx, |this, cx| {
                let newest = this.wake_word_write == write;
                match result {
                    Ok(stored) => {
                        if newest {
                            this.member_voice = Some(stored.clone());
                            crate::talk_host::set_voice_settings(stored, cx);
                        }
                    }
                    Err(err) => {
                        if newest {
                            this.member_voice = Some(previous);
                        }
                        if err.trpc_code() == Some("BAD_REQUEST") {
                            this.wake_word_error = Some(err.user_message());
                        } else {
                            crate::action_error::failed_action_err("change", "the wake word", &err, cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #EXP-1847: a Transcript / Wake phrase edit (THE input rule ×4 as it
    /// is typed); a commit saves a change (`agents.update {voice}`, blank =
    /// null), a too-short wake phrase stays inline and is never sent.
    fn edited_voice(&mut self, field: &str, value: String, commit: bool, cx: &mut gpui::Context<Self>) {
        let wake = field == "voiceWakePhrase";
        let limits = &domain::agent_settings::AGENT_VOICE.limits;
        let value = etalk::voice::voice_text_input(&value, if wake { limits.wake_phrase } else { limits.transcript });
        if wake {
            self.wake_invalid = None;
            self.wake_draft = Some(value.clone());
        } else {
            self.transcript_draft = Some(value.clone());
        }
        cx.notify();
        if !commit {
            return;
        }
        let Some(agent) = self.loaded_agent.clone().and_then(|id| crate::session_data::agent_def(&id, cx)) else {
            return;
        };
        let team_id = agent.team_id.clone().or_else(|| active_team_id(&self.nav, cx)).unwrap_or_default();
        if !self.editor(&agent, &team_id, cx) {
            return;
        }
        let trimmed = value.trim().to_string();
        if wake {
            if let Some(invalid) = etalk::voice::wake_phrase_error(&trimmed) {
                self.wake_invalid = Some(invalid);
                return;
            }
        }
        let resolved = self.resolved_voice(&agent, cx);
        let current = if wake { &resolved.wake_phrase } else { &resolved.transcript };
        if trimmed == current.trim() {
            return;
        }
        let next = (!trimmed.is_empty()).then_some(trimmed);
        let patch = if wake {
            api::agents::VoicePatch { wake_phrase: Some(next), ..Default::default() }
        } else {
            api::agents::VoicePatch { transcript: Some(next), ..Default::default() }
        };
        update_agent(cx, "save", "the agent's voice", api::agents::UpdateInput { voice: Some(patch), ..api::agents::UpdateInput::new(agent.id) });
    }

    /// #EXP-1847: the clip row's press: the host's file picker (one file),
    /// then the upload.
    fn pick_voice_clip(&mut self, agent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.clip_busy {
            return;
        }
        let receiver = crate::file_picker::prompt_for_paths(cx, gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(domain::agent_settings::AGENT_VOICE.copy.clip_upload.clone().into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            // Receiver error = dialog dismissed/unsupported; None = cancelled.
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, _, cx| this.upload_voice_clip(agent_id, path, cx));
        })
        .detach();
    }

    /// One multipart `file` (a WAV, `clipUpload`) to the agent's clip
    /// route; the shape echoes the new clip. A refused file or a failed
    /// upload toasts (`copy.errors`), the latter with a Retry.
    fn upload_voice_clip(&mut self, agent_id: String, path: std::path::PathBuf, cx: &mut gpui::Context<Self>) {
        if self.clip_busy {
            return;
        }
        let Some(transport) = queries::http_attachment_transport(cx) else {
            crate::action_error::failed_action("upload", "the voice clip", None, cx);
            return;
        };
        self.clip_busy = true;
        cx.notify();
        let (send_agent, send_path) = (agent_id.clone(), path.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (filename, content_type, bytes) = crate::markdown::image_paste::read_any_file(&send_path)?;
                    if let Some(refusal) = domain::agent_settings::agent_voice_clip_refusal(&filename, &content_type, bytes.len() as u64) {
                        return Err(anyhow::anyhow!(refusal));
                    }
                    let accept = &domain::agent_settings::AGENT_VOICE.clip_upload.accept;
                    let content_type = if accept.iter().any(|accepted| accepted.eq_ignore_ascii_case(&content_type)) { content_type } else { accept[0].clone() };
                    transport.upload_agent_voice_clip(&send_agent, &filename, &content_type, &bytes).map(|_| ())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.clip_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] voice clip upload failed: {err}");
                    let message = err.to_string();
                    let errors = &domain::agent_settings::AGENT_VOICE.copy.errors;
                    if message == errors.clip_type || message == errors.clip_size {
                        crate::action_error::failed_action("upload", "the voice clip", Some(message), cx);
                    } else {
                        let pane = cx.entity().downgrade();
                        crate::action_error::failed_action_with_retry(
                            "upload",
                            "the voice clip",
                            crate::action_error::description_of(&message),
                            move |_, cx| {
                                let _ = pane.update(cx, |this, cx| this.upload_voice_clip(agent_id.clone(), path.clone(), cx));
                            },
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The clip's Remove: `DELETE` on its route; the echo clears the row.
    fn remove_voice_clip(&mut self, agent_id: String, cx: &mut gpui::Context<Self>) {
        if self.clip_busy {
            return;
        }
        let Some(transport) = queries::http_attachment_transport(cx) else {
            crate::action_error::failed_action("remove", "the voice clip", None, cx);
            return;
        };
        self.clip_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let send_agent = agent_id.clone();
            let result = cx.background_executor().spawn(async move { transport.delete_agent_voice_clip(&send_agent) }).await;
            let _ = this.update(cx, |this, cx| {
                this.clip_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] voice clip removal failed: {err}");
                    let pane = cx.entity().downgrade();
                    crate::action_error::failed_action_with_retry(
                        "remove",
                        "the voice clip",
                        crate::action_error::description_of(&err.to_string()),
                        move |_, cx| {
                            let _ = pane.update(cx, |this, cx| this.remove_voice_clip(agent_id.clone(), cx));
                        },
                        cx,
                    );
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
        // #EXP-1849: an app agent's slot = a device shared with the team.
        if self.slot.is_some() {
            return all.into_iter().filter(|device| device.team_shared).collect();
        }
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
        // #EXP-1859: the Wake word row is the MEMBER's own setting: every
        // member flips it, on a team agent's page.
        if name == "voiceWakeWord" {
            if let Some(on) = payload.get("checked").and_then(Value::as_bool).filter(|_| !agent.is_assistant()) {
                self.flip_wake_word(agent_id, on, cx);
            }
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
            // #EXP-1849: `Each member's own device` empties an app agent's slot.
            "device" if self.slot.is_some() && value == EACH_MEMBER => {
                if agent.device_id.as_deref().is_some_and(|id| !id.is_empty()) {
                    update_agent(cx, "change", "the agent's device", api::agents::UpdateInput { device_id: Some(String::new()), ..api::agents::UpdateInput::new(agent_id) })
                }
            }
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
            // #EXP-1847: the agent's own voice.
            "voiceLanguage" if picked && !value.is_empty() && value != self.resolved_voice(&agent, cx).language => {
                let patch = api::agents::VoicePatch { language: Some(Some(value)), ..Default::default() };
                update_agent(cx, "save", "the agent's voice", api::agents::UpdateInput { voice: Some(patch), ..api::agents::UpdateInput::new(agent_id) })
            }
            "voiceClip" => self.pick_voice_clip(agent_id, window, cx),
            "voiceClipRemove" if agent.voice.clip_id.is_some() => self.remove_voice_clip(agent_id, cx),
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
        let bound_device = agent.device_id.as_deref().filter(|id| !id.is_empty());
        if let Some(bound) = bound_device {
            if !devices.iter().any(|device| device.device_id == bound) {
                device_options.push(json!({"label": bound, "value": bound}));
            }
        }
        // #EXP-1849 (apps.json `agentSlot`): the slot picker = `Each member's
        // own device`, then the team-shared devices (offline ones disabled);
        // Account and Model only while a device is set.
        let slot = self.slot.is_some();
        if slot {
            for (option, device) in device_options.iter_mut().zip(devices.iter()) {
                if !device.online {
                    option["disabled"] = json!(true);
                }
            }
            device_options.insert(0, json!({"label": APPS.agent_slot.copy.each_member, "value": EACH_MEMBER}));
        }
        let device_value = match bound_device {
            Some(id) => id.to_string(),
            None if slot => EACH_MEMBER.to_string(),
            None => String::new(),
        };
        let has_device = !slot || bound_device.is_some();
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
        // #EXP-1847: the agent's own voice, resolved against the member's.
        let voice = self.resolved_voice(agent, cx);
        let voice_copy = &domain::agent_settings::AGENT_VOICE.copy;
        let transcript = self.transcript_draft.clone().unwrap_or_else(|| voice.transcript.clone());
        let wake_phrase = self.wake_draft.clone().unwrap_or_else(|| voice.wake_phrase.clone());
        let read_text = |text: &str, placeholder: String| if text.is_empty() { json!({"shown": placeholder, "empty": true}) } else { json!({"shown": text, "empty": false}) };
        let agent_name = agent.name.clone().unwrap_or_default();
        // #EXP-1859: the team agent's Wake word (the Assistant's page keeps
        // its member row); off and disabled while the member has no voice
        // device or voice agent, as the Assistant's row.
        let (wake_row, wake_detail) = if assistant { (domain::agent_settings::WakeRow { on: false, error: None }, None) } else { self.wake_word_row(agent, cx) };
        let member = self.member_settings(cx);
        let wake_ready = member.voice_device_id.is_some() && member.agent_id.is_some();
        let wake_error = self.wake_word_error.clone().or(wake_row.error);
        json!({
            "assistant": assistant,
            "agentPage": !assistant,
            "owner": owner,
            "member": !owner,
            "icon": agent.icon.clone().unwrap_or_else(|| defaults.icon.clone()),
            "color": agent.color.clone().unwrap_or_else(|| defaults.color.clone()),
            "fields": {"name": self.name_draft.clone().unwrap_or_else(|| agent.name.clone().unwrap_or_default()), "instructions": instructions_text, "voiceTranscript": transcript, "voiceWakePhrase": wake_phrase},
            "voiceLanguage": {"value": voice.language, "options": etalk::voice::VOICE.languages.iter().map(|language| json!({"label": etalk::voice::language_label(&language.value, None), "value": language.value})).collect::<Vec<_>>()},
            "voice": {
                "clip": {"label": voice.clip_label, "pick": owner && !self.clip_busy, "read": !owner || self.clip_busy, "removable": owner && !self.clip_busy && agent.voice.clip_id.is_some()},
                "voiceTranscript": read_text(&transcript, voice_copy.transcript_placeholder.clone()),
                "voiceWakePhrase": read_text(&wake_phrase, domain::agent_settings::agent_wake_phrase_placeholder(&agent_name)),
                "wakeInvalid": self.wake_invalid.clone().unwrap_or_default(),
                "hasWakeInvalid": self.wake_invalid.is_some(),
                "wakeWord": {"checked": wake_row.on && wake_ready, "disabled": !wake_ready, "detail": wake_detail, "hint": wake_detail.is_some(),
                    "error": wake_error.clone().unwrap_or_default(), "hasError": !assistant && wake_error.is_some()},
            },
            "device": {"value": device_value, "options": device_options},
            "hasDevice": has_device,
            "account": {"has": has_device && current.is_some(), "none": has_device && current.is_none(), "value": current.as_ref().map(|option| option.account_option_key()).unwrap_or_default(), "options": logins},
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
        // #EXP-1849: the slot shows no name or instructions to seed.
        if self.slot.is_none() {
            self.sync_selected_agent(&agent, window, cx);
        }
        let team_id = agent.team_id.clone().or_else(|| active_team_id(&self.nav, cx)).unwrap_or_default();
        self.ensure_mcp_loaded(&team_id, cx);
        self.ensure_member_voice(cx);
        // #EXP-1847: the wake phrase's placeholder names the agent (a
        // static prop): a renamed agent rebuilds the tree.
        let placeholder = domain::agent_settings::agent_wake_phrase_placeholder(agent.name.as_deref().unwrap_or_default());
        if self.slot.is_none() && placeholder != self.wake_placeholder {
            let tree: eui_core::NestedNode = serde_json::from_value(document(&Ink::of(cx), &Chrome::of(cx), &placeholder)).expect("the agent document parses");
            self.view.update(cx, |view, cx| {
                view.set_nested(tree, cx);
            });
            self.wake_placeholder = placeholder;
        }
        let projection = self.project(&agent, &team_id, cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        let page = div().w_full().min_w_0().child(self.view.clone());
        if self.slot.is_some() {
            return gpui::InteractiveElement::id(page, SharedString::from(APPS.agent_slot.test_id.clone())).into_any_element();
        }
        if !agent.is_assistant() {
            return page.into_any_element();
        }
        // S-3: the Assistant's member voice block (its own document: push to
        // talk, wake word, meetings) right under the page's Voice group; the
        // form's error after it.
        let voice = self.voice.get_or_insert_with(|| cx.new(|cx| super::voice_prefs::VoicePrefsPane::assistant(window, cx))).clone();
        // The page's own gap (`gap_4`) above the member block.
        let gap = gpui::px(Ink::of(cx).r(1.));
        let mut column = v_flex().w_full().min_w_0().child(page).child(div().w_full().min_w_0().pt(gap).child(voice));
        if let Some(message) = self.error.clone() {
            column = column.child(div().pt_4().child(crate::action_error::field_error(message, cx).px_1()));
        }
        column.into_any_element()
    }
}

/// #EXP-1859: the agents of the member's teams as synced (the agents shape
/// is member-only), the wake rules' `agents`.
fn synced_agents(cx: &App) -> Vec<AgentDefRow> {
    Store::try_global(cx).map(|store| store.collections().agents.read(cx).iter().cloned().collect()).unwrap_or_default()
}

/// #EXP-1859: the member's voice settings as the wake rules read them.
fn wake_input(settings: &etalk::voice::VoiceSettings) -> domain::agent_settings::WakeSettings<'_> {
    domain::agent_settings::WakeSettings {
        voice_device_id: settings.voice_device_id.as_deref(),
        agent_id: settings.agent_id.as_deref(),
        wake_word: settings.wake_word,
        wake_agent_ids: &settings.wake_agent_ids,
        member: domain::agent_settings::MemberVoice {
            language: settings.language(),
            voice_file: &settings.voice_file,
            voice_transcript: &settings.voice_transcript,
            wake_phrase: &settings.wake_phrase,
        },
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
/// #EXP-1849: the slot picker's `Each member's own device` value (writes
/// `deviceId: ""`).
const EACH_MEMBER: &str = "__each-member__";

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

/// The Runs on rows: device, account, model, skills, MCP servers (members
/// read; account and model only while `/pane/hasDevice`).
fn runs_on_rows(ink: &Ink) -> Vec<Value> {
    let read_only = |mut row: Value| {
        row["props"]["disabled"] = path("/pane/member");
        row
    };
    let [skills_pick, skills_read] = skills_rows(ink);
    let [mcp_pick, mcp_read] = mcp_rows(ink);
    let [account_pick, account_none] = account_rows(ink);
    let mut model = read_only(select_row("model", crate::chat_screen::MENU_MODEL, "model", "model", json!({})));
    model["visible"] = path("/pane/hasDevice");
    let mut runs_on = vec![read_only(device_row())];
    runs_on.extend(divided(ink, read_only(account_pick)));
    runs_on.extend(divided(ink, account_none));
    runs_on.extend(divided(ink, model));
    runs_on.extend(divided(ink, skills_pick));
    runs_on.extend(divided(ink, skills_read));
    runs_on.extend(divided(ink, mcp_pick));
    runs_on.extend(divided(ink, mcp_read));
    runs_on
}

/// #EXP-1849 (apps.json `agentSlot`): the App settings' Agent group: the
/// section header `copy.title` over THE Runs on rows.
fn slot_document(ink: &Ink) -> Value {
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "maxWidth": APPS.editor_tabs.settings_max_width},
        "children": [banded(ink, "agent_slot", &APPS.agent_slot.copy.title, ink.group("agent_slot_group", runs_on_rows(ink)))]})
}

/// The pane's tree (`gap_4`, #EXP-1846 `page`): identity; Runs on; the
/// Triggers list; Voice (#EXP-1847: every agent's rows; the Assistant's
/// member block follows the document); Instructions, the error and the
/// owner's Delete (team agents).
fn document(ink: &Ink, chrome: &Chrome, wake_placeholder: &str) -> Value {
    let copy = &AGENT_SETTINGS.copy;
    // Identity: the team agent's icon + colour + name, or the Assistant's mark + name.
    let mut agent_identity = identity(ink);
    agent_identity["visible"] = path("/pane/agentPage");
    let mut mark_identity = assistant_identity(ink, chrome);
    mark_identity["visible"] = path("/pane/assistant");
    let identity_group = ink.group("identity_group", vec![agent_identity, mark_identity]);
    // Runs on: device, account, model, skills, MCP servers (members read).
    let runs_on = runs_on_rows(ink);
    let voice_copy = &domain::agent_settings::AGENT_VOICE.copy;
    let wake_invalid = field_error(ink, chrome, "voice_wake_invalid", path("/pane/voice/wakeInvalid"), path("/pane/voice/hasWakeInvalid"));
    // #EXP-1859: why a team agent that is on does not listen, inline.
    let wake_word_error = field_error(ink, chrome, "voice_wake_word_error", path("/pane/voice/wakeWord/error"), path("/pane/voice/wakeWord/hasError"));
    let voice = json!({"id": "voice", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [ink.band("voice_band", &voice_copy.label, None), ink.group("voice_group", voice_rows(ink, chrome, wake_placeholder)), wake_invalid, wake_word_error]});
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
        let tree = document(&super::super::readiness::tests::test_ink(), &Chrome::test(), "Hey Sloppy");
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-agent", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

    /// Every id in a document tree.
    fn ids(node: &Value, out: &mut Vec<String>) {
        if let Some(id) = node.get("id").and_then(Value::as_str) {
            out.push(id.to_string());
        }
        for child in node.get("children").and_then(Value::as_array).into_iter().flatten() {
            ids(child, out);
        }
    }

    /// #EXP-1847: every agent's page carries the Voice band with its four
    /// rows in the fixture's order (no longer Assistant-only), the wake
    /// phrase's placeholder naming the agent.
    #[test]
    fn every_agent_page_has_the_voice_rows() {
        let tree = document(&super::super::readiness::tests::test_ink(), &Chrome::test(), "Hey Sloppy");
        let mut found = Vec::new();
        ids(&tree, &mut found);
        let voice = tree["children"].as_array().unwrap().iter().find(|node| node["id"] == "voice").expect("the voice section");
        assert!(voice.get("visible").is_none(), "shown for every agent");
        let order: Vec<usize> = ["voice_band", "voice_language", "voice_clip", "voice_transcript", "voice_wake_phrase", "voice_wake_word", "voice_wake_word_error"]
            .iter()
            .map(|id| found.iter().position(|found| found == id).unwrap_or_else(|| panic!("{id}")))
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        assert!(found.iter().any(|id| id == "voice_clip_remove"));
        let wake = serde_json::to_string(&tree).unwrap();
        assert!(wake.contains("\"placeholder\":\"Hey Sloppy\""));
        assert!(wake.contains(&domain::agent_settings::AGENT_VOICE.copy.transcript_placeholder));
        // #EXP-1859: the Wake word switch shows on a team agent's page only.
        let wake_word = voice["children"][1]["children"].as_array().unwrap().iter().find(|node| node["id"] == "voice_wake_word").expect("the wake word row");
        assert!(wake_word["visible"].to_string().contains("/pane/agentPage"), "{}", wake_word["visible"]);
        assert!(wake.contains(&format!("\"{}\"", domain::agent_settings::AGENT_VOICE.wake.copy.wake_word)));
    }

    /// #EXP-1849: the App settings' Agent group raises none either, and
    /// heads with the fixture's title.
    #[test]
    fn the_slot_document_raises_no_unexpected_issue() {
        let tree = slot_document(&super::super::readiness::tests::test_ink());
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-app-agent", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree.clone()).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
        assert!(tree.to_string().contains(&format!("\"{}\"", APPS.agent_slot.copy.title)));
        assert_eq!(APPS.agent_slot.copy.title, "Agent");
        assert_eq!(APPS.agent_slot.copy.each_member, "Each member's own device");
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
