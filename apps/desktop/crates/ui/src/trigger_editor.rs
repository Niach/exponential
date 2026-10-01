//! The shared **trigger** form (EXP-530; reshaped by EXP-583 and SLOP-2) —
//! the when-part + runner section embedded by [`crate::trigger_dialog`]
//! (which adds/edits one element of an action's `triggers`). EXP-825: the
//! suggestion-seeded creator flow can't save anything —
//! [`format_trigger_block`] appends the wire JSON to the composer's seed text
//! so the creator agent sets it via `exponential_actions_update` once the
//! action exists.
//!
//! The section owns FOUR things: the **when-part** (EXP-698 — ONE glass group
//! whose first row is the embedded Schedule · On event strip, over that
//! kind's own field rows), the **device** that evaluates and fires it
//! (triggers are local-only — no server scheduler), and the optional
//! **account / model / effort** pins (every unpinned field falls back to that
//! device's launch defaults). The device picker is ALWAYS shown: a trigger
//! can target any trigger-capable machine, not just the one authoring it.
//!
//! It also owns the pure WRITE shapes ([`trigger_element`] and the
//! whole-array helpers beside it): `actions.update({triggers})` replaces the
//! array, so every add/edit/toggle/delete is "the readable existing elements,
//! with one changed".
//!
//! [`TriggerEditorState::to_trigger`] is the only place the WIRE trigger is
//! built, and it validates instead of clamping: a half-filled section returns
//! a readable message the host dialog renders in its own error slot, so a bad
//! trigger never reaches the server's BAD_REQUEST.
//!
//! The state lives as a plain field on the host view; [`render`] takes a
//! `fn(&mut V) -> &mut Self` accessor so the callbacks can reach back into it
//! without either dialog owning a second entity.
//!
//! [`render`]: TriggerEditorState::render

use gpui::{
    div, App, AppContext as _, ClickEvent, Context, Div, Entity, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _,
};
use serde_json::{json, Value};

use coding::automations::{
    parse_action_triggers, parse_trigger, ActionTrigger, EventKind, ParsedTrigger,
    ScheduleInterval, TriggerKind,
};

use crate::coding_selects::{effort_choices_for, model_choices_for};
use crate::surface;
// EXP-615: the model/effort pins render through the ONE shared launch
// cluster; EXP-995: the agent strip above them became THE account picker
// (`picker::account_picker` since EXP-1021), fed by the bound machine's
// logins.
use crate::launch_options;
use crate::controls::glass_input;

/// Which pane the section shows. EXP-583 dropped the `None` mode: a stored
/// trigger exists to fire, so its when-part is never absent — a manual action
/// simply has no trigger at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TriggerMode {
    Schedule,
    Event,
}

/// The 7 contract events in picker order, with the copy every client shows.
/// Byte-shared with the web select — a rename is a cross-client change.
const EVENT_LABELS: [(EventKind, &str); 7] = [
    (EventKind::Created, "An issue is created"),
    (EventKind::StatusChanged, "Status changes"),
    (EventKind::AssigneeChanged, "The assignee changes"),
    (EventKind::LabelAdded, "A label is added"),
    (EventKind::PriorityChanged, "Priority changes"),
    (EventKind::PrOpened, "A pull request is opened"),
    (EventKind::PrMerged, "A pull request is merged"),
];

const WEEKDAY_LABELS: [(u32, &str); 7] = [
    (1, "Monday"),
    (2, "Tuesday"),
    (3, "Wednesday"),
    (4, "Thursday"),
    (5, "Friday"),
    (6, "Saturday"),
    (7, "Sunday"),
];

const INTERVAL_LABELS: [(ScheduleInterval, &str); 3] = [
    (ScheduleInterval::Daily, "Day"),
    (ScheduleInterval::Weekly, "Week"),
    (ScheduleInterval::Monthly, "Month"),
];

/// Why a trigger on an action with required inputs cannot be switched on:
/// a triggered run has no one to fill them. The server refuses to ENABLE
/// such a trigger (`actions.update`), so the Triggers row says so up front
/// instead of letting the owner discover it as a failed toggle. Byte-shared
/// ×4 (web `REQUIRED_INPUTS_HINT`).
pub(crate) const TRIGGER_REQUIRED_INPUTS_HINT: &str =
    "This action has required inputs, and a triggered run has none to fill them with. \
     Make the inputs optional to enable it.";

/// The cap the server enforces per filter list — the pickers stop offering
/// more instead of letting the save fail (`ACTION_TRIGGER_MAX_FILTER_IDS`).
fn filter_cap() -> usize {
    domain::contract::ACTION_TRIGGER_MAX_FILTER_IDS
}

/// One device the picker can bind a trigger to.
#[derive(Clone, Debug)]
pub(crate) struct DeviceOption {
    /// The steer TEXT id (`devices.device_id`), NOT the row uuid — the
    /// trigger's `deviceId` and what the host matches itself against.
    pub(crate) device_id: String,
    pub(crate) label: String,
    /// EXP-924's stored device glyph (`contract::DEVICE_ICON_VALUES`), read
    /// only through `icons::device_icon` — with [`Self::server`] for the
    /// kind default when the row names none.
    pub(crate) icon: Option<String>,
    pub(crate) server: bool,
    pub(crate) online: bool,
    /// The agent CLIs the machine advertises — the Agent picker offers
    /// exactly these (the server re-checks the pin against the same list).
    pub(crate) agents: Vec<String>,
    /// The machine's last used launch agent (EXP-437/1158), clamped to
    /// [`Self::agents`] — the account row seeds to its last used login
    /// (EXP-615: no "Device default" pill; EXP-995: no agent strip at all).
    pub(crate) default_agent: Option<String>,
    /// EXP-622: this is the signed-in user's DEFAULT machine — the binding
    /// seeds to it. False on a teammate's shared row (their preference).
    pub(crate) is_default: bool,
    /// EXP-995: the machine's `agent_accounts` payload off its synced row —
    /// which login each agent CLI runs as there and its profiles. Empty for
    /// a row that never reported: the account picker then offers one ambient
    /// row per runnable agent.
    pub(crate) accounts: coding::agent_accounts::AgentAccounts,
    /// EXP-992: its `agent_usage` payload, the picker's limit bars.
    pub(crate) usage: coding::agent_usage::AgentUsageMap,
    /// Its published launch defaults clamped onto a default `Settings` — its
    /// `default_agent` (the last used agent) names the machine's LAST USED
    /// login, the first row of the picker.
    pub(crate) settings: coding::Settings,
}

pub(crate) struct TriggerEditorState {
    /// Scopes the board/label/status filter pickers.
    team_id: String,
    pub(crate) mode: TriggerMode,
    interval: ScheduleInterval,
    /// 1=Monday..7=Sunday (weekly only).
    weekday: u32,
    /// 1..=28 — every month has one (monthly only).
    day_of_month: u32,
    /// "HH:MM" local wall clock, parse-validated in [`Self::to_trigger`].
    time: Entity<InputState>,
    event: EventKind,
    board_ids: Vec<String>,
    label_ids: Vec<String>,
    priorities: Vec<String>,
    to_status_ids: Vec<String>,
    /// The bound machine's steer id. Always picked here (EXP-583) — a
    /// trigger may target any trigger-capable device, not only this one.
    pub(crate) device_id: Option<String>,
    /// The pinned launch overrides; every `None` = the device's own defaults.
    pub(crate) agent: Option<String>,
    /// EXP-995: the agent PROFILE the run spends, picked with its agent off
    /// the account row; `None` = the machine's last used login for `agent`.
    pub(crate) account: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
}

/// Everything a saved (or agent-authored) trigger carries beyond its id and
/// its on/off flag: the when-part and the runner.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TriggerSpec {
    /// The WHEN-part alone (`kind` + its fields, no `source`).
    pub(crate) trigger: Value,
    pub(crate) device_id: String,
    pub(crate) agent: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
}

/// The machine-readable block appended to a creator run's request when a
/// suggestion carries a trigger (EXP-530/583/825; SLOP-2 reshaped it for
/// `actions.triggers`). Byte-identical to web `formatTriggerBlock`
/// (`lib/action-triggers.ts`), key order included: the when-part's keys in
/// their stored order, then `deviceId`, then the launch pins only when they
/// are set. The composer never talks to the server — the creator agent passes
/// this JSON verbatim as `triggers` to `exponential_actions_update` once the
/// action exists.
pub(crate) fn format_trigger_block(spec: &TriggerSpec) -> String {
    // The workspace serde_json preserves insertion order (the fingerprint's
    // doc has the why), so this map IS the key order on the wire.
    let mut payload = spec.trigger.as_object().cloned().unwrap_or_default();
    payload.insert("deviceId".to_string(), json!(spec.device_id));
    for (key, value) in [
        ("agent", spec.agent.as_deref()),
        ("model", spec.model.as_deref()),
        ("effort", spec.effort.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            payload.insert(key.to_string(), json!(value));
        }
    }
    let json = serde_json::to_string(&Value::Array(vec![Value::Object(payload)]))
        .unwrap_or_else(|_| "[]".to_string());
    format!(
        "\n\nTrigger — after creating the action, call exponential_actions_update \
         with its id and `triggers` set to exactly this array: `{json}`. A triggered \
         run fills no inputs, so declare none as required."
    )
}

/// EXP-825: the block a getting-started suggestion that carries a trigger
/// appends to its composer seed — web `action-suggestions-list.tsx` parity:
/// the runner is the caller's DEFAULT trigger-capable machine, else the
/// first one; without any the block is simply left off (the old dialog did
/// the same). No launch pins: the creator agent sets the trigger, its
/// agent/model/effort stay the machine's defaults.
pub(crate) fn suggestion_trigger_block(trigger: Value, cx: &App) -> String {
    let devices = trigger_devices(cx);
    let device_id = devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first())
        .map(|device| device.device_id.clone());
    match device_id {
        Some(device_id) => format_trigger_block(&TriggerSpec {
            trigger,
            device_id,
            agent: None,
            account: None,
            model: None,
            effort: None,
        }),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// The write shapes (SLOP-2: `actions.update({triggers})`, a whole-array replace)
// ---------------------------------------------------------------------------

/// ONE trigger element as written: the runner, then the when-part, and an
/// event names its source. `id` is absent on a NEW trigger (the server mints
/// it); unset pins are omitted.
pub(crate) fn trigger_element(spec: &TriggerSpec, id: Option<&str>, enabled: bool) -> Value {
    let mut element = serde_json::Map::new();
    if let Some(id) = id {
        element.insert("id".to_string(), json!(id));
    }
    element.insert("enabled".to_string(), json!(enabled));
    element.insert("deviceId".to_string(), json!(spec.device_id));
    for (key, value) in [
        ("agent", spec.agent.as_deref()),
        ("account", spec.account.as_deref()),
        ("model", spec.model.as_deref()),
        ("effort", spec.effort.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            element.insert(key.to_string(), json!(value));
        }
    }
    let is_event = spec.trigger.get("kind").and_then(Value::as_str) == Some("event");
    if let Some(when) = spec.trigger.as_object() {
        for (key, value) in when {
            element.insert(key.clone(), value.clone());
        }
    }
    if is_event {
        element.insert("source".to_string(), json!("exponential"));
    }
    Value::Object(element)
}

/// The array an ADD sends: every readable existing trigger, then the new one.
pub(crate) fn triggers_with_added(existing: &[ActionTrigger], element: Value) -> Vec<Value> {
    let mut next: Vec<Value> = existing.iter().map(|trigger| trigger.raw.clone()).collect();
    next.push(element);
    next
}

/// The array an EDIT sends: `element` in the place of the trigger `id` (the
/// order is the list's order, so it never moves).
pub(crate) fn triggers_with_replaced(
    existing: &[ActionTrigger],
    id: &str,
    element: Value,
) -> Vec<Value> {
    existing
        .iter()
        .map(|trigger| {
            if trigger.id == id {
                element.clone()
            } else {
                trigger.raw.clone()
            }
        })
        .collect()
}

/// The array a TOGGLE sends: only that trigger's `enabled` moves, so the
/// when-part — and with it the host's firing state — is untouched.
pub(crate) fn triggers_with_enabled(
    existing: &[ActionTrigger],
    id: &str,
    enabled: bool,
) -> Vec<Value> {
    existing
        .iter()
        .map(|trigger| {
            let mut element = trigger.raw.clone();
            if trigger.id == id {
                element["enabled"] = json!(enabled);
            }
            element
        })
        .collect()
}

/// The array a DELETE sends: everything but the trigger `id`.
pub(crate) fn triggers_without(existing: &[ActionTrigger], id: &str) -> Vec<Value> {
    existing
        .iter()
        .filter(|trigger| trigger.id != id)
        .map(|trigger| trigger.raw.clone())
        .collect()
}

/// The triggers the last `actions.update` RETURNED, per action, kept until
/// the synced row has caught up. Every write replaces the WHOLE array, so a
/// second write built off a row the Electric echo has not reached yet would
/// carry the first one's change undone (switch A off, then B off → A back on).
static WRITTEN_TRIGGERS: std::sync::Mutex<Option<std::collections::HashMap<String, WrittenTriggers>>> =
    std::sync::Mutex::new(None);

#[derive(Clone)]
struct WrittenTriggers {
    triggers: Vec<Value>,
    /// The written row's `updated_at` — the synced row replaces this base
    /// once its own is at least as new.
    updated_at: Option<String>,
}

/// Record what a successful `actions.update({triggers})` returned — the base
/// of the next write until the synced row shows it ([`action_triggers`]).
pub(crate) fn note_triggers_written(action: &api::actions::Action) {
    let mut written = WRITTEN_TRIGGERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    written.get_or_insert_with(Default::default).insert(
        action.id.clone(),
        WrittenTriggers {
            triggers: action.triggers.clone(),
            updated_at: action.updated_at.clone(),
        },
    );
}

/// Whether the synced row (`synced`) already shows the write stamped
/// `written`. Electric forwards microseconds, tRPC echoes milliseconds, so
/// the row of the SAME write compares at-least-as-new. An unreadable stamp
/// on either side reads as caught up — the synced row stays the authority.
fn synced_caught_up(synced: Option<&str>, written: Option<&str>) -> bool {
    match (
        synced.and_then(crate::inbox::parse_timestamp),
        written.and_then(crate::inbox::parse_timestamp),
    ) {
        (Some(synced), Some(written)) => synced >= written,
        _ => true,
    }
}

/// The array the next write starts from: the last write's returned triggers
/// while the synced row lags behind it, else the synced row's own.
fn write_base(
    synced_updated_at: Option<&str>,
    written: Option<&WrittenTriggers>,
) -> Option<Vec<ActionTrigger>> {
    let written = written
        .filter(|written| !synced_caught_up(synced_updated_at, written.updated_at.as_deref()))?;
    Some(
        written
            .triggers
            .iter()
            .filter_map(coding::automations::parse_action_trigger)
            .collect(),
    )
}

/// An action's readable triggers, in stored order — what every write above
/// starts from: the synced row's, or the last write's while its echo is
/// still on the way. Empty when the row is not synced.
pub(crate) fn action_triggers(action_id: &str, cx: &App) -> Vec<ActionTrigger> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let actions = store.collections().actions.read(cx);
    let Some(row) = actions.get(action_id) else {
        return Vec::new();
    };
    let mut written = WRITTEN_TRIGGERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let pending = written.as_ref().and_then(|written| written.get(action_id));
    match write_base(row.updated_at.as_deref(), pending) {
        Some(base) => base,
        None => {
            // Caught up (or never written): the base has served its purpose.
            if let Some(written) = written.as_mut() {
                written.remove(action_id);
            }
            parse_action_triggers(row.triggers.as_ref())
        }
    }
}

/// What an action row's trigger glyphs draw (web `triggerBadges`): each kind
/// is `Some` when the action has a trigger of it, and `true` while at least
/// one of them is enabled (a paused kind draws muted).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TriggerBadges {
    pub(crate) schedule: Option<bool>,
    pub(crate) event: Option<bool>,
}

impl TriggerBadges {
    pub(crate) fn of(triggers: &[ActionTrigger]) -> Self {
        let badge = |schedule: bool| {
            let mut of_kind = triggers
                .iter()
                .filter(|trigger| trigger.is_schedule() == schedule)
                .peekable();
            of_kind.peek()?;
            Some(of_kind.any(|trigger| trigger.enabled))
        };
        Self {
            schedule: badge(true),
            event: badge(false),
        }
    }

    /// A suggestion seed's glyph: its one suggested trigger, drawn active.
    pub(crate) fn of_when(when: &ParsedTrigger) -> Self {
        match when.kind {
            TriggerKind::Schedule(_) => Self { schedule: Some(true), event: None },
            TriggerKind::Event(_) => Self { schedule: None, event: Some(true) },
            TriggerKind::Unsupported => Self::default(),
        }
    }

    /// The glyphs beside a name: `trigger-schedule` then `trigger-event`,
    /// each muted to the disabled tone while no trigger of its kind is on.
    /// No text, no count.
    pub(crate) fn render(self, cx: &App) -> Vec<gpui::AnyElement> {
        use gpui_component::Sizable as _;
        let foreground = cx.theme().foreground;
        [
            (self.schedule, crate::icons::registry::TRIGGER_SCHEDULE),
            (self.event, crate::icons::registry::TRIGGER_EVENT),
        ]
        .into_iter()
        .filter_map(|(badge, icon)| {
            let active = badge?;
            Some(
                gpui_component::Icon::from(icon)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(foreground.opacity(if active { 0.7 } else { 0.3 }))
                    .into_any_element(),
            )
        })
        .collect()
    }
}

/// The Triggers row's sentence. A schedule fires on the BOUND MACHINE's wall
/// clock, so the recurrence carries that caveat (EXP-812: an absolute
/// next-run date moved under every screenshot, and the recurrence says the
/// same thing).
pub(crate) fn trigger_sentence(when: &ParsedTrigger) -> String {
    let sentence = coding::automations::trigger_summary(when);
    if matches!(when.kind, TriggerKind::Schedule(_)) {
        format!("{sentence} (device time)")
    } else {
        sentence
    }
}

impl TriggerEditorState {
    pub(crate) fn new<V: 'static>(
        team_id: String,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Self {
        let time = cx.new(|cx| InputState::new(window, cx).placeholder("07:00"));
        time.update(cx, |state, cx| state.set_value("09:00", window, cx));
        Self {
            team_id,
            mode: TriggerMode::Schedule,
            interval: ScheduleInterval::Daily,
            weekday: 1,
            day_of_month: 1,
            time,
            event: EventKind::Created,
            board_ids: Vec::new(),
            label_ids: Vec::new(),
            priorities: Vec::new(),
            to_status_ids: Vec::new(),
            device_id: None,
            agent: None,
            account: None,
            model: None,
            effort: None,
        }
    }

    /// Preselect the caller's DEFAULT machine (EXP-622) when it is one of the
    /// trigger-capable candidates, else the only candidate when there is
    /// exactly one — the common single-machine case, so "Add trigger" is
    /// one click less. Several machines and no default leaves the pick
    /// explicit.
    pub(crate) fn seed_default_device(&mut self, cx: &mut App) {
        if self.device_id.is_none() {
            let devices = trigger_devices(cx);
            if let Some(default) = devices.iter().find(|device| device.is_default) {
                self.device_id = Some(default.device_id.clone());
            } else if let [only] = &devices[..] {
                self.device_id = Some(only.device_id.clone());
            }
        }
        self.ensure_agent_seeded(cx);
    }

    /// EXP-615: the strip has no "Device default" pill — once a device is
    /// bound, an unset (or no-longer-runnable) agent seeds to that machine's
    /// default launch agent, exactly like the start-coding dialog.
    ///
    /// EXP-721: it seeds even when NO device is bound yet (the `devices` shape
    /// may still be landing) or when the bound machine advertises no agents.
    /// [`Self::device_agents`] offers the whole contract list in both cases,
    /// so leaving `agent` unset painted three pills with none lit — this
    /// falls back to THIS install's default launch agent instead.
    pub(crate) fn ensure_agent_seeded(&mut self, cx: &mut App) {
        let available = self.device_agents(cx);
        let device_default = self.device_id.as_deref().and_then(|device_id| {
            trigger_devices(cx)
                .into_iter()
                .find(|device| device.device_id == device_id)
                .and_then(|device| device.default_agent)
        });
        let global_default = crate::coding_flow::CodingHub::global(cx)
            .read(cx)
            .settings
            .default_agent
            .id();
        let next = settle_seed_agent(
            self.agent.as_deref(),
            device_default.as_deref(),
            &available,
            global_default,
        );
        if self.agent != next {
            // A model/effort belongs to ONE agent — they never survive it,
            // and neither does an account (EXP-995: a profile is ONE agent's).
            self.model = None;
            self.effort = None;
            self.agent = next;
            // The seeded agent takes the machine's LAST USED login when that
            // is its login — the composer's seed, `settle_account`.
            let options = self.account_options(cx);
            let seeded = coding::last_used_account_option(&options)
                .filter(|option| Some(option.agent.id()) == self.agent.as_deref())
                .and_then(|option| option.wire_account());
            self.account = seeded;
        }
    }

    /// EXP-995: the bound device CHANGED. A pin the new machine cannot run
    /// would be refused server-side, so the agent settles first
    /// ([`Self::ensure_agent_seeded`]); and a profile id is DEVICE-LOCAL
    /// (`agent_profiles` dirs live on one machine), so even a still-runnable
    /// agent's pin named the OLD machine's login — the account re-seeds from
    /// the new device ([`settle_device_account`]) to the row the picker
    /// displays, so Save stores what is shown. Re-picking the bound device
    /// disturbs nothing.
    pub(crate) fn rebind_device(&mut self, device_id: String, cx: &mut App) {
        if self.device_id.as_deref() == Some(device_id.as_str()) {
            return;
        }
        self.device_id = Some(device_id);
        self.ensure_agent_seeded(cx);
        let options = self.account_options(cx);
        self.account = settle_device_account(self.agent.as_deref(), &options);
    }

    /// EXP-995: every signed-in login the BOUND machine reports, across
    /// agents, its last used login first
    /// ([`launch_options::machine_account_options`]);
    /// a machine that reports none (or none bound yet) offers one ambient row
    /// per runnable agent, named by the agent, so the row never goes empty.
    fn account_options(&self, cx: &mut App) -> Vec<coding::AccountOption> {
        let available: Vec<coding::CodingAgent> = self
            .device_agents(cx)
            .iter()
            .filter_map(|id| coding::CodingAgent::parse(id))
            .collect();
        let device = self.device_id.as_deref().and_then(|device_id| {
            trigger_devices(cx)
                .into_iter()
                .find(|device| device.device_id == device_id)
        });
        match device {
            Some(device) => launch_options::machine_account_options(
                &device.accounts,
                &device.usage,
                &device.settings,
                &available,
            ),
            None => launch_options::machine_account_options(
                &Default::default(),
                &Default::default(),
                &crate::coding_flow::CodingHub::global(cx).read(cx).settings,
                &available,
            ),
        }
    }

    /// EXP-995: a pick off the account row — the agent rides the option, so
    /// this IS the agent switch too (model/effort re-clamp on a different
    /// agent; another login of the same agent keeps them).
    fn set_account(&mut self, option: &coding::AccountOption) {
        let agent = option.agent.id().to_string();
        if self.agent.as_deref() != Some(agent.as_str()) {
            self.model = None;
            self.effort = None;
            self.agent = Some(agent);
        }
        self.account = option.wire_account();
    }

    /// Seed the WHEN half from a stored trigger element (or a suggestion's
    /// when-part). An element this build cannot read never gets here — the
    /// tolerant read drops it before any row, so there is nothing to open.
    pub(crate) fn seed_trigger(&mut self, trigger: Option<&Value>, window: &mut Window, cx: &mut App) {
        let Some(parsed) = trigger.and_then(parse_trigger) else {
            return;
        };
        match parsed.kind {
            TriggerKind::Schedule(schedule) => {
                self.mode = TriggerMode::Schedule;
                self.interval = schedule.interval;
                if let Some(weekday) = schedule.weekday {
                    self.weekday = weekday;
                }
                if let Some(day) = schedule.day_of_month {
                    self.day_of_month = day;
                }
                let time = format!(
                    "{:02}:{:02}",
                    schedule.minute_of_day / 60,
                    schedule.minute_of_day % 60
                );
                self.time
                    .update(cx, |state, cx| state.set_value(time, window, cx));
            }
            TriggerKind::Event(spec) => {
                self.mode = TriggerMode::Event;
                self.event = spec.event;
                self.board_ids = spec.board_ids;
                self.label_ids = spec.label_ids;
                self.priorities = spec.priorities;
                self.to_status_ids = spec.to_status_ids;
            }
            // Unreachable from a stored trigger (see above); a hand-built
            // value leaves the panes on their defaults.
            TriggerKind::Unsupported => {}
        }
    }

    /// Seed the RUNNER half from an existing trigger.
    pub(crate) fn seed_runner(
        &mut self,
        device_id: Option<&str>,
        agent: Option<&str>,
        account: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
    ) {
        self.device_id = device_id.filter(|id| !id.is_empty()).map(str::to_string);
        self.agent = agent.filter(|value| !value.is_empty()).map(str::to_string);
        self.account = account.filter(|value| !value.is_empty()).map(str::to_string);
        self.model = model.filter(|value| !value.is_empty()).map(str::to_string);
        self.effort = effort.filter(|value| !value.is_empty()).map(str::to_string);
    }

    /// Build the WIRE when-part — nothing else ([`trigger_element`] adds the
    /// runner, the flag and an event's source around it).
    /// `Err` = a readable validation message.
    pub(crate) fn to_trigger(&self, cx: &App) -> Result<Value, SharedString> {
        match self.mode {
            TriggerMode::Schedule => {
                let raw = self.time.read(cx).value();
                let minute_of_day = parse_minute_of_day(&raw)
                    .ok_or::<SharedString>("Enter a time like 07:00.".into())?;
                let mut trigger = json!({
                    "kind": "schedule",
                    "interval": interval_wire(self.interval),
                    "minuteOfDay": minute_of_day,
                });
                match self.interval {
                    ScheduleInterval::Daily => {}
                    ScheduleInterval::Weekly => {
                        trigger["weekday"] = json!(self.weekday.clamp(1, 7));
                    }
                    ScheduleInterval::Monthly => {
                        trigger["dayOfMonth"] = json!(self.day_of_month.clamp(1, 28));
                    }
                }
                Ok(trigger)
            }
            TriggerMode::Event => {
                let mut filters = serde_json::Map::new();
                // Only the filters this event actually reads ride along — the
                // server's zod rejects a labelIds on a pr_opened trigger.
                let mut put = |key: &str, ids: &[String]| {
                    if !ids.is_empty() {
                        let capped: Vec<&String> = ids.iter().take(filter_cap()).collect();
                        filters.insert(key.to_string(), json!(capped));
                    }
                };
                put("boardIds", &self.board_ids);
                if self.event == EventKind::LabelAdded {
                    put("labelIds", &self.label_ids);
                }
                if matches!(self.event, EventKind::Created | EventKind::PriorityChanged) {
                    put("priorities", &self.priorities);
                }
                if self.event == EventKind::StatusChanged {
                    put("toStatusIds", &self.to_status_ids);
                }
                let mut trigger = json!({
                    "kind": "event",
                    "event": self.event.wire(),
                });
                // An all-empty `filters` object is omitted, not sent as `{}`.
                if !filters.is_empty() {
                    trigger["filters"] = Value::Object(filters);
                }
                Ok(trigger)
            }
        }
    }

    /// The whole trigger, validated: the when-part + the runner binding.
    pub(crate) fn to_spec(&self, cx: &App) -> Result<TriggerSpec, SharedString> {
        let trigger = self.to_trigger(cx)?;
        let Some(device_id) = self.device_id.clone().filter(|id| !id.is_empty()) else {
            return Err("Pick a device for the trigger.".into());
        };
        Ok(TriggerSpec {
            trigger,
            device_id,
            agent: self.agent.clone(),
            // A model/effort (and EXP-995 the account) is only meaningful
            // against a pinned agent (the server validates the pair) — drop
            // them with the agent.
            account: self.agent.as_ref().and(self.account.clone()),
            model: self.agent.as_ref().and(self.model.clone()),
            effort: self.agent.as_ref().and(self.effort.clone()),
        })
    }

    // -- render ---------------------------------------------------------------

    /// The whole section (EXP-698). The trigger is ONE inset-grouped card —
    /// the Schedule · On event strip as its FIRST ROW (the embedded,
    /// container-less variant the agent tabs of the same dialog wear), a
    /// hairline, then the chosen kind's fields as grouped picker rows. The old
    /// "Trigger" heading is gone with it: a labelled column above a
    /// free-floating capsule was the last surface in the dialog that wasn't
    /// one of these cards, and the card's own first row already names the
    /// choice. "Runs on" and the launch pins stay their own groups.
    ///
    /// `prefix` namespaces the element ids (both dialogs can be open at once);
    /// `access` reaches the state on the host view.
    pub(crate) fn render<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        window: &Window,
        cx: &mut Context<V>,
    ) -> gpui::AnyElement {
        let mut rows = vec![self.render_mode_strip(prefix, access, cx)];
        rows.extend(match self.mode {
            TriggerMode::Schedule => self.schedule_rows(prefix, access, window, cx),
            TriggerMode::Event => self.event_rows(prefix, access, cx),
        });
        v_flex()
            .gap_2()
            .child(surface::glass_group_rows(rows))
            .child(self.render_device_picker(prefix, access, cx))
            .child(self.render_launch_pins(prefix, access, cx))
            .into_any_element()
    }

    /// The trigger card's first row: the Schedule · On event segments, drawn
    /// with the SAME [`surface::glass_tabs_row`] / [`surface::glass_tab_item`]
    /// pair as the agent strip below it — the group draws the hairline under
    /// it, so the strip carries no capsule of its own.
    fn render_mode_strip<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> Div {
        let segment = |label: &'static str, mode: TriggerMode, id: SharedString| {
            surface::glass_tab_item(self.mode == mode, cx)
                .id(id)
                .child(label)
                .on_click(cx.listener(move |view: &mut V, _: &ClickEvent, _, cx| {
                    let state = access(view);
                    if state.mode != mode {
                        state.mode = mode;
                        cx.notify();
                    }
                }))
        };
        surface::glass_tabs_row()
            .child(segment(
                "Schedule",
                TriggerMode::Schedule,
                format!("{prefix}-mode-schedule").into(),
            ))
            .child(segment(
                "On event",
                TriggerMode::Event,
                format!("{prefix}-mode-event").into(),
            ))
    }

    /// The Schedule kind's rows: `Every`, the interval's own qualifier
    /// (`Weekday` weekly / `Day of month` monthly) and `Time`.
    fn schedule_rows<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        window: &Window,
        cx: &mut Context<V>,
    ) -> Vec<Div> {
        let interval_label = INTERVAL_LABELS
            .iter()
            .find(|(interval, _)| *interval == self.interval)
            .map(|(_, label)| *label)
            .unwrap_or("Day");
        let view = cx.entity().downgrade();
        let every = picker_trigger(format!("{prefix}-interval").into(), interval_label, cx)
            .dropdown_menu(move |mut menu, _window, _cx| {
                for (interval, label) in INTERVAL_LABELS {
                    let view = view.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        if let Some(view) = view.upgrade() {
                            view.update(cx, |view, cx| {
                                access(view).interval = interval;
                                cx.notify();
                            });
                        }
                    }));
                }
                menu
            })
            .into_any_element();
        let mut rows = vec![surface::glass_picker_row("Every", None, every, cx)];
        match self.interval {
            ScheduleInterval::Daily => {}
            ScheduleInterval::Weekly => {
                let current = WEEKDAY_LABELS
                    .iter()
                    .find(|(day, _)| *day == self.weekday)
                    .map(|(_, label)| *label)
                    .unwrap_or("Monday");
                let view = cx.entity().downgrade();
                let control = picker_trigger(format!("{prefix}-weekday").into(), current, cx)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for (day, label) in WEEKDAY_LABELS {
                            let view = view.clone();
                            menu = menu.item(PopupMenuItem::new(label).on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            access(view).weekday = day;
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        menu
                    })
                    .into_any_element();
                rows.push(surface::glass_picker_row("Weekday", None, control, cx));
            }
            ScheduleInterval::Monthly => {
                let current = format!("Day {}", self.day_of_month);
                let view = cx.entity().downgrade();
                let control = picker_trigger(
                    format!("{prefix}-day-of-month").into(),
                    SharedString::from(current),
                    cx,
                )
                .dropdown_menu(move |mut menu, _window, _cx| {
                    // 1..=28 only — every month has those days, so a
                    // monthly schedule can never skip a month.
                    for day in 1..=28u32 {
                        let view = view.clone();
                        menu = menu.item(
                            PopupMenuItem::new(SharedString::from(format!("Day {day}"))).on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            access(view).day_of_month = day;
                                            cx.notify();
                                        });
                                    }
                                },
                            ),
                        );
                    }
                    menu
                })
                .into_any_element();
                rows.push(surface::glass_picker_row("Day of month", None, control, cx));
            }
        }
        rows.push(surface::glass_input_row(
            "Time",
            surface::glass_row_input(glass_input(&self.time, window, cx)).into_any_element(),
            cx,
        ));
        rows
    }

    /// The On-event kind's rows: `When`, the always-applicable `Board` filter
    /// and whatever else THIS event reads (the server's zod rejects a filter
    /// the event ignores).
    fn event_rows<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> Vec<Div> {
        let current = EVENT_LABELS
            .iter()
            .find(|(event, _)| *event == self.event)
            .map(|(_, label)| *label)
            .unwrap_or("An issue is created");
        let view = cx.entity().downgrade();
        let when = picker_trigger(format!("{prefix}-event").into(), current, cx)
            .dropdown_menu(move |mut menu, _window, _cx| {
                for (event, label) in EVENT_LABELS {
                    let view = view.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        if let Some(view) = view.upgrade() {
                            view.update(cx, |view, cx| {
                                access(view).event = event;
                                cx.notify();
                            });
                        }
                    }));
                }
                menu
            })
            .into_any_element();

        // The filter rows are the TYPED pickers' own rows: a board wears its
        // glyph in its colour, a label its coloured dot.
        let boards = crate::picker::board_picker::board_items(
            &sync::Store::global(cx)
                .collections()
                .boards_in_team(&self.team_id, cx),
        );
        let mut rows = vec![
            surface::glass_picker_row("When", None, when, cx),
            self.render_filter(
                prefix,
                "board",
                "Board",
                "Any board",
                boards,
                &self.board_ids,
                |state| &mut state.board_ids,
                access,
                cx,
            ),
        ];
        match self.event {
            EventKind::LabelAdded => {
                let labels = crate::picker::label_picker::label_items(
                    &crate::queries::team_labels(cx, &self.team_id),
                );
                rows.push(self.render_filter(
                    prefix,
                    "label",
                    "Label",
                    "Any label",
                    labels,
                    &self.label_ids,
                    |state| &mut state.label_ids,
                    access,
                    cx,
                ));
            }
            EventKind::Created | EventKind::PriorityChanged => {
                let priorities: Vec<crate::picker::PickerItem<String>> =
                    domain::contract::ISSUE_PRIORITY_VALUES
                        .iter()
                        .map(|value| {
                            crate::picker::PickerItem::new((*value).to_string(), capitalize(value))
                        })
                        .collect();
                rows.push(self.render_filter(
                    prefix,
                    "priority",
                    "Priority",
                    "Any priority",
                    priorities,
                    &self.priorities,
                    |state| &mut state.priorities,
                    access,
                    cx,
                ));
            }
            EventKind::StatusChanged => {
                // EXP-314: the team's own status rows. The duplicate category
                // is excluded like every other picker — a duplicate needs its
                // canonical pairing, so nothing "changes to" it in isolation.
                let statuses: Vec<crate::picker::PickerItem<String>> =
                    crate::queries::team_statuses(cx, &self.team_id)
                        .into_iter()
                        .filter(|row| row.category != "duplicate")
                        .map(|row| crate::picker::PickerItem::new(row.id, row.name))
                        .collect();
                rows.push(self.render_filter(
                    prefix,
                    "status",
                    "To status",
                    "Any status",
                    statuses,
                    &self.to_status_ids,
                    |state| &mut state.to_status_ids,
                    access,
                    cx,
                ));
            }
            EventKind::AssigneeChanged | EventKind::PrOpened | EventKind::PrMerged => {}
        }
        rows
    }

    /// One multi-select filter row of the trigger card: a grouped picker row
    /// whose menu toggles `selected`, writing through the `pick` accessor.
    #[allow(clippy::too_many_arguments)] // one row, one call site per filter
    fn render_filter<V: Render>(
        &self,
        prefix: &'static str,
        key: &'static str,
        label: &'static str,
        empty_label: &'static str,
        items: Vec<crate::picker::PickerItem<String>>,
        selected: &[String],
        pick: fn(&mut Self) -> &mut Vec<String>,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> Div {
        let button_label: SharedString = match selected.len() {
            0 => empty_label.into(),
            1 => items
                .iter()
                .find(|item| item.value == selected[0])
                .map(|item| item.label.clone())
                .unwrap_or_else(|| "1 selected".into()),
            count => format!("{count} selected").into(),
        };
        let picked = selected.to_vec();
        let view = cx.entity().downgrade();
        let trigger =
            picker_trigger(format!("{prefix}-filter-{key}").into(), button_label, cx)
                .into_any_element();
        // EXP-1021: a filter is a MULTI pick, so it rides the shared picker —
        // its rows carry the subject's own glyph and a picked one reads as
        // the row's highlight, the same language every other multi picker
        // speaks. At the cap only DEselection stays live (the primitive's
        // `max`): the server rejects a longer list.
        let control = crate::picker::deferred(move |window, cx| {
            crate::picker::Picker::multi(
                items,
                picked,
                trigger,
                std::rc::Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |view, cx| {
                            let list = pick(access(view));
                            *list = next.clone();
                            list.truncate(filter_cap());
                            cx.notify();
                        });
                    }
                }),
            )
            .search(true)
            .max(filter_cap())
            .empty_text("Nothing to filter on")
            .id(SharedString::from(format!("{prefix}-filter-{key}-picker")))
            .render(window, cx)
        })
        .into_any_element();
        surface::glass_picker_row(label, None, control, cx)
    }

    /// EXP-694: the runner picker is its own grouped row ("Runs on" leading,
    /// the machine trailing) — the device group every client shows above the
    /// agent group.
    fn render_device_picker<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> Div {
        let devices = trigger_devices(cx);
        let foreground = cx.theme().foreground;
        let picked = self
            .device_id
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
            .map(SharedString::from);
        if devices.is_empty() && picked.is_none() {
            return surface::glass_group_rows(vec![surface::glass_row_shell().child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().text_sm().text_color(foreground).child("Runs on"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(foreground.opacity(0.5))
                            .child(
                                "No device can run triggers. Run the desktop app or the \
                                 exponential daemon and it will appear here.",
                            ),
                    ),
            )]);
        }
        let view = cx.entity().downgrade();
        let bound = self.device_id.clone();
        let trigger = picker_trigger(
            format!("{prefix}-device").into(),
            picked.clone().unwrap_or_else(|| "Select device…".into()),
            cx,
        )
        .into_any_element();
        // EXP-1021: THE device picker — the same rows the composer and the
        // workflow runner row draw, each machine by its own glyph.
        // EXP-615: every trigger-capable machine reads the same.
        // Offline-but-capable is not a lesser choice — the run fires when the
        // machine comes back (the offline catch-up rule) — so no row is
        // disabled and none carries an online decoration; the Triggers
        // rows show presence.
        let rows: Vec<crate::picker::device_picker::DevicePickerDevice> = devices
            .iter()
            .map(|device| crate::picker::device_picker::DevicePickerDevice {
                id: device.device_id.clone(),
                name: device.label.clone(),
                icon: device.icon.clone(),
                server: device.server,
                description: None,
                disabled: false,
            })
            .collect();
        let control = crate::picker::deferred(move |window, cx| {
            crate::picker::device_picker::device_picker(
                &rows,
                bound,
                trigger,
                std::rc::Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
                    let (Some(view), Some(device_id)) = (view.upgrade(), next.into_iter().next())
                    else {
                        return;
                    };
                    view.update(cx, |view, cx| {
                        access(view).rebind_device(device_id, cx);
                        cx.notify();
                    });
                }),
            )
            .id(SharedString::from(format!("{prefix}-device-picker")))
            .render(window, cx)
        })
        .into_any_element();
        surface::glass_group_rows(vec![surface::glass_picker_row("Runs on", None, control, cx)])
    }

    /// Account / Model / Effort — the optional pins. EXP-995: the agent strip
    /// is gone — the group's FIRST ROW is THE account picker
    /// ([`crate::picker::account_picker`], the shared picker since EXP-1021):
    /// brand mark + email over every login the BOUND machine reports, its
    /// default first, and a pick implies the agent (the server re-checks it
    /// against what the machine advertises). Model/effort below it are the
    /// same rows the launch dialogs draw and only unlock once an agent is
    /// pinned: they are validated per agent, and "the device's default agent
    /// with a foreign model" is not a state the server accepts.
    fn render_launch_pins<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> Div {
        let options = self.account_options(cx);
        // EXP-1158: an UNPINNED row (no account) runs on the machine's last
        // used login of the agent, so it reads as that agent's first login.
        let current = self
            .agent
            .as_deref()
            .zip(self.account.as_deref())
            .map(|(agent, account)| format!("{agent}:{account}"));
        // The stored pair, else that agent's first login (unpinned, or a
        // profile the machine no longer reports), else the picker's first row.
        let current_key = current
            .filter(|key| options.iter().any(|option| &option.account_option_key() == key))
            .or_else(|| {
                let agent = self.agent.as_deref()?;
                options
                    .iter()
                    .find(|option| option.agent.id() == agent)
                    .map(|option| option.account_option_key())
            });
        let view = cx.entity().downgrade();
        // EXP-1021: THE account picker — the shared picker's rows (brand mark
        // + login email, a dead credential's health as the muted line) behind
        // the trigger the other pin rows in this group wear. What the trigger
        // READS as: the stored pair, else the machine's last used login, which
        // is the picker's first row.
        let current_option = current_key
            .as_deref()
            .and_then(|key| options.iter().find(|option| option.account_option_key() == key))
            .or_else(|| coding::last_used_account_option(&options))
            .cloned();
        let picker = match current_option {
            // No login to offer: the row keeps its label and nothing else —
            // a caller with nothing to pick builds its own fallback rows.
            None => div().into_any_element(),
            Some(current) => {
                // Nothing to PICK either, with exactly one login: the same
                // line without the affordance (the mark and the email still
                // say which login the run spends).
                let alone = options.len() < 2;
                let trigger = picker_trigger(
                    format!("{prefix}-account").into(),
                    SharedString::from(current.email.clone()),
                    cx,
                )
                .icon(crate::coding_selects::agent_mark(current.agent))
                .dropdown_caret(!alone)
                .into_any_element();
                let picked = current.account_option_key();
                let rows = options.clone();
                crate::picker::deferred(move |window, cx| {
                    let options = rows.clone();
                    crate::picker::account_picker::account_picker(
                        &rows,
                        Some(picked),
                        trigger,
                        std::rc::Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
                            let (Some(view), Some(key)) = (view.upgrade(), next.into_iter().next())
                            else {
                                return;
                            };
                            let Some(option) = options
                                .iter()
                                .find(|option| option.account_option_key() == key)
                                .cloned()
                            else {
                                return;
                            };
                            view.update(cx, |view, cx| {
                                access(view).set_account(&option);
                                cx.notify();
                            });
                        }),
                    )
                    .disabled(alone)
                    .id(SharedString::from(format!("{prefix}-account-picker")))
                    .render(window, cx)
                })
                .into_any_element()
            }
        };
        let strip = surface::glass_picker_row("Account", None, picker, cx);

        // Model/Effort stay VISIBLE while nothing is pinned (web parity,
        // EXP-615): dimmed rows reading "CLI default" — a model belongs to
        // ONE agent, so they only unlock once an agent is picked.
        let agent = self.agent.as_deref().and_then(coding::CodingAgent::parse);
        let (model_row, effort_row): (Div, Div) = match agent {
            Some(agent) => (
                launch_options::choice_pin_row(
                    "Model",
                    prefix,
                    "model",
                    model_choices_for(agent),
                    self.model.as_deref(),
                    |state: &mut Self| &mut state.model,
                    access,
                    cx,
                ),
                launch_options::choice_pin_row(
                    "Effort",
                    prefix,
                    "effort",
                    effort_choices_for(agent),
                    self.effort.as_deref(),
                    |state: &mut Self| &mut state.effort,
                    access,
                    cx,
                ),
            ),
            None => {
                let foreground = cx.theme().foreground;
                let placeholder = |label: &'static str, key: &'static str| {
                    use gpui_component::Disableable as _;
                    let control = Button::new(SharedString::from(format!("{prefix}-pin-{key}-off")))
                        .ghost()
                        .h_auto()
                        .px_0()
                        .py_0()
                        .text_color(foreground.opacity(0.7))
                        .dropdown_caret(true)
                        .label(launch_options::CLI_DEFAULT_LABEL)
                        .disabled(true)
                        .into_any_element();
                    // `appearance`-free buttons lose the component's own
                    // disabled dimming, so the row carries it.
                    surface::glass_picker_row(label, None, control, cx).opacity(0.5)
                };
                (placeholder("Model", "model"), placeholder("Effort", "effort"))
            }
        };
        surface::glass_group_rows(vec![strip, model_row, effort_row])
    }

    /// The agent ids the bound device advertises; a device that advertises
    /// none (or none picked yet) offers the whole contract list — the server
    /// still refuses a pin the machine can't run.
    fn device_agents(&self, cx: &App) -> Vec<String> {
        let advertised = self.device_id.as_deref().and_then(|id| {
            trigger_devices(cx)
                .into_iter()
                .find(|device| device.device_id == id)
                .map(|device| device.agents)
        });
        match advertised {
            Some(agents) if !agents.is_empty() => agents,
            _ => domain::contract::CODING_AGENT_VALUES
                .iter()
                .map(|id| (*id).to_string())
                .collect(),
        }
    }
}

/// The trailing control of a trigger-card row: a caret-ed dropdown trigger
/// stripped of field chrome, exactly like [`crate::launch_options`]'s pins —
/// the GROUP is the field, the row's 16/12 is the padding.
/// EXP-810: `pub(crate)` — the MCP server editor's Transport/Auth rows are
/// the same closed-vocabulary picker trigger.
pub(crate) fn picker_trigger(id: SharedString, label: impl Into<SharedString>, cx: &App) -> Button {
    Button::new(id)
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_0()
        .py_0()
        .text_color(cx.theme().foreground.opacity(0.7))
        .dropdown_caret(true)
        // EXP-697: NOT `.label()` — upstream draws that in a `flex_none` box,
        // so a long option wraps onto a second line.
        .child(surface::picker_value_label(label))
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn interval_wire(interval: ScheduleInterval) -> &'static str {
    match interval {
        ScheduleInterval::Daily => "daily",
        ScheduleInterval::Weekly => "weekly",
        ScheduleInterval::Monthly => "monthly",
    }
}

/// "HH:MM" → minute of day. Strict: two fields, in range, no stray text — a
/// typo must surface as the validation message, never as a 09:00 default.
fn parse_minute_of_day(raw: &str) -> Option<u32> {
    let (hours, minutes) = raw.trim().split_once(':')?;
    let hours: u32 = hours.trim().parse().ok()?;
    let minutes: u32 = minutes.trim().parse().ok()?;
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

/// Which agent the strip settles on (EXP-721) — the pure half of
/// [`TriggerEditorState::ensure_agent_seeded`], so the fallback ladder is
/// testable without an `App`.
///
/// A still-runnable pick is never disturbed; otherwise the bound machine's own
/// default wins, then THIS install's default launch agent, then whatever the
/// list offers first. `None` only when nothing is runnable at all — which
/// `device_agents` never actually reports (it falls back to the contract
/// list), so the strip always has a lit segment.
fn settle_seed_agent(
    picked: Option<&str>,
    device_default: Option<&str>,
    available: &[String],
    global_default: &str,
) -> Option<String> {
    let runnable = |candidate: &str| available.iter().any(|id| id == candidate);
    if let Some(picked) = picked.filter(|picked| runnable(picked)) {
        return Some(picked.to_string());
    }
    if let Some(device_default) = device_default.filter(|agent| runnable(agent)) {
        return Some(device_default.to_string());
    }
    if runnable(global_default) {
        return Some(global_default.to_string());
    }
    available.first().cloned()
}

/// EXP-995: the account pin for `agent` on a NEWLY bound device: the
/// machine's last used login when it is `agent`'s, else `agent`'s first login
/// there — exactly the row the account picker would DISPLAY for that agent
/// ([`TriggerEditorState::render_launch_pins`]'s fallback), its id
/// verbatim on the wire (EXP-1158: `system` names the ambient login). `None`
/// when nothing of `agent`'s is runnable there.
fn settle_device_account(
    agent: Option<&str>,
    options: &[coding::AccountOption],
) -> Option<String> {
    let agent = agent?;
    let of_agent = |option: &&coding::AccountOption| option.agent.id() == agent;
    coding::last_used_account_option(options)
        .filter(of_agent)
        .or_else(|| options.iter().find(of_agent))
        .and_then(|option| option.wire_account())
}

/// The synced devices that advertise the `automations` cap (the wire name
/// SLOP-2 deliberately kept) — own rows plus
/// team-shared ones (the shape's scope). Offline rows are INCLUDED: a missed
/// schedule fires once when the machine comes back.
pub(crate) fn trigger_devices(cx: &App) -> Vec<DeviceOption> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collection = store.collections().devices.clone();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let me = crate::queries::active_account(cx).map(|account| account.id);
    let mut devices: Vec<DeviceOption> = collection
        .read(cx)
        .iter()
        .filter(|row| row.cap_ids().iter().any(|cap| cap == "automations"))
        .filter_map(|row| {
            let device_id = row.device_id.clone().filter(|id| !id.is_empty())?;
            let agents = row.agent_ids();
            // `launch_defaults` syncs as JSON that may itself be a JSON
            // string; the default agent only counts when runnable there.
            let launch_defaults = row.launch_defaults.as_ref().and_then(|value| match value {
                Value::String(raw) => serde_json::from_str::<Value>(raw).ok(),
                other => Some(other.clone()),
            });
            let default_agent = launch_defaults
                .as_ref()
                .and_then(|value| {
                    value
                        .get("defaultAgent")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .filter(|agent| agents.contains(agent));
            // EXP-995: the published defaults clamped onto a default
            // Settings (the same clamp `device_settings::baseline_for` runs
            // for a remote row) — what names the machine's last used login.
            let mut settings = coding::Settings::default();
            if let Some(patch) = launch_defaults
                .as_ref()
                .and_then(|value| serde_json::from_value::<coding::DefaultsPatch>(value.clone()).ok())
            {
                coding::apply_defaults_patch(&mut settings, &patch);
            }
            if let Some(agent) = default_agent.as_deref().and_then(coding::CodingAgent::parse) {
                settings.default_agent = agent;
            }
            Some(DeviceOption {
                label: row.label.clone().unwrap_or_else(|| device_id.clone()),
                icon: row.icon.clone(),
                server: row.is_server(),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                agents,
                default_agent,
                is_default: me
                    .as_deref()
                    .is_some_and(|me| row.user_id.as_deref() == Some(me))
                    && row.is_default.unwrap_or(false),
                accounts: crate::device_settings::parse_agent_map(row.agent_accounts.as_ref()),
                usage: crate::device_settings::parse_agent_map(row.agent_usage.as_ref()),
                settings,
                device_id,
            })
        })
        .collect();
    // Online first, then by label — the machine most likely to run it leads.
    devices.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.label.cmp(&b.label)));
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SLOP-2: a second whole-array write before the Electric echo must
    /// start from what the FIRST write returned — else switching A off and
    /// then B off sends A enabled again.
    #[test]
    fn the_next_write_starts_from_the_last_write_until_the_synced_row_catches_up() {
        let element = |id: &str, enabled: bool| {
            json!({"id": id, "deviceId": "d-1", "enabled": enabled,
                   "kind": "schedule", "interval": "daily", "minuteOfDay": 540})
        };
        // The synced row still reads [a on, b on].
        let written = WrittenTriggers {
            triggers: vec![element("a", false), element("b", true)],
            updated_at: Some("2026-10-01T10:00:00.123Z".to_string()),
        };
        // The echo has not landed: the written array is the base, so B's
        // toggle keeps A off.
        let base = write_base(Some("2026-10-01 09:00:00.5+00"), Some(&written))
            .expect("the synced row lags");
        let next = triggers_with_enabled(&base, "b", false);
        assert_eq!(next, vec![element("a", false), element("b", false)]);
        // The same write's row (microseconds) — or a later one — is caught up.
        assert!(write_base(Some("2026-10-01 10:00:00.123456+00"), Some(&written)).is_none());
        assert!(write_base(Some("2026-10-01 11:00:00+00"), Some(&written)).is_none());
        // Nothing written, or a stamp nobody can read: the synced row rules.
        assert!(write_base(Some("2026-10-01 09:00:00+00"), None).is_none());
        assert!(write_base(None, Some(&written)).is_none());
    }

    /// EXP-721: the agent strip is a radio — the ladder must always name a
    /// segment. The "no device bound yet" state (the `devices`
    /// shape still landing, so `device_agents` offers the whole contract
    /// list) used to leave `agent` NULL and every pill dark.
    #[test]
    fn the_agent_seed_always_settles_on_a_runnable_agent() {
        let all: Vec<String> = domain::contract::CODING_AGENT_VALUES
            .iter()
            .map(|id| (*id).to_string())
            .collect();

        // No device, nothing picked: this install's default agent.
        assert_eq!(
            settle_seed_agent(None, None, &all, "codex"),
            Some("codex".to_string())
        );
        // A bound machine's own default outranks the global one.
        assert_eq!(
            settle_seed_agent(None, Some("claude"), &all, "codex"),
            Some("claude".to_string())
        );
        // A still-runnable pick is never disturbed.
        assert_eq!(
            settle_seed_agent(Some("claude"), Some("codex"), &all, "codex"),
            Some("claude".to_string())
        );
        // A pick the machine cannot run falls through the ladder.
        let only_claude = vec!["claude".to_string()];
        assert_eq!(
            settle_seed_agent(Some("codex"), None, &only_claude, "codex"),
            Some("claude".to_string())
        );
        // ... and so does a global default it cannot run.
        assert_eq!(
            settle_seed_agent(None, None, &only_claude, "codex"),
            Some("claude".to_string())
        );
        // Nothing runnable is the only NULL — unreachable through
        // `device_agents`, which always offers the contract list.
        assert_eq!(settle_seed_agent(None, None, &[], "codex"), None);
    }

    /// EXP-995: a profile id is device-local — a device switch re-seeds the
    /// pin from the NEW machine, to the row the picker would display.
    #[test]
    fn a_device_switch_reseeds_the_account_from_the_new_machine() {
        let option = |agent: &str, id: &str, is_last_used: bool| coding::AccountOption {
            id: id.to_string(),
            agent: coding::CodingAgent::parse(agent).expect("contract agent"),
            email: id.to_string(),
            is_last_used,
            health: coding::Health::Ok,
            limits: None,
        };
        let options = vec![
            option("codex", "work", true),
            option("claude", "home", false),
            option("claude", "side", false),
        ];
        // The new machine's last used login is the agent's own: the pin.
        assert_eq!(settle_device_account(Some("codex"), &options), Some("work".to_string()));
        // Another agent: its FIRST login there — what the picker shows.
        assert_eq!(settle_device_account(Some("claude"), &options), Some("home".to_string()));
        // EXP-1158: the ambient login is NAMED on the wire.
        let ambient = vec![option("claude", coding::SYSTEM_PROFILE, true)];
        assert_eq!(
            settle_device_account(Some("claude"), &ambient),
            Some(coding::SYSTEM_PROFILE.to_string())
        );
        // Nothing of the agent's runnable there, or no agent: no pin.
        assert_eq!(settle_device_account(Some("codex"), &ambient), None);
        assert_eq!(settle_device_account(None, &options), None);
    }

    #[test]
    fn minute_of_day_parsing_is_strict() {
        assert_eq!(parse_minute_of_day("07:00"), Some(420));
        assert_eq!(parse_minute_of_day("7:5"), Some(425));
        assert_eq!(parse_minute_of_day(" 23:59 "), Some(1439));
        assert_eq!(parse_minute_of_day("00:00"), Some(0));
        // A typo must reach the user as the validation message.
        for bad in ["24:00", "07:60", "0700", "", "aa:bb", "7"] {
            assert_eq!(parse_minute_of_day(bad), None, "{bad} must not parse");
        }
    }

    /// The section's parse→edit→serialize loop must round-trip: what
    /// [`TriggerEditorState::seed_trigger`] reads, [`to_trigger`] writes
    /// back. EXP-583: the WHEN-part only — no `deviceId`, no `enabled`.
    #[test]
    fn wire_shapes_round_trip_through_the_parser() {
        use coding::automations::{EventSpec, Schedule, ScheduleInterval};

        // Weekly schedule.
        let weekly = json!({
            "kind": "schedule", "interval": "weekly", "minuteOfDay": 540, "weekday": 3
        });
        let parsed = parse_trigger(&weekly).expect("weekly parses");
        assert_eq!(
            parsed.kind,
            TriggerKind::Schedule(Schedule {
                interval: ScheduleInterval::Weekly,
                minute_of_day: 540,
                weekday: Some(3),
                day_of_month: None,
            })
        );
        assert_eq!(
            coding::automations::trigger_summary(&parsed),
            "Weekly on Wednesday at 09:00"
        );

        // Event with filters — the empty lists are absent, not `[]`.
        let event = json!({
            "kind": "event",
            "event": "status_changed",
            "filters": {"boardIds": ["b-1"], "toStatusIds": ["s-1"]}
        });
        let parsed = parse_trigger(&event).expect("event parses");
        assert_eq!(
            parsed.kind,
            TriggerKind::Event(EventSpec {
                event: EventKind::StatusChanged,
                board_ids: vec!["b-1".to_string()],
                label_ids: Vec::new(),
                priorities: Vec::new(),
                to_status_ids: vec!["s-1".to_string()],
            })
        );
    }

    /// The event picker's copy is cross-client — lock the 7 labels and their
    /// contract order.
    #[test]
    fn event_labels_cover_the_contract_in_order() {
        let wires: Vec<&str> = EVENT_LABELS.iter().map(|(kind, _)| kind.wire()).collect();
        assert_eq!(wires, domain::contract::ACTION_TRIGGER_EVENT_VALUES);
        let labels: Vec<&str> = EVENT_LABELS.iter().map(|(_, label)| *label).collect();
        assert_eq!(
            labels,
            vec![
                "An issue is created",
                "Status changes",
                "The assignee changes",
                "A label is added",
                "Priority changes",
                "A pull request is opened",
                "A pull request is merged",
            ]
        );
        let intervals: Vec<&str> = INTERVAL_LABELS
            .iter()
            .map(|(interval, _)| interval_wire(*interval))
            .collect();
        assert_eq!(intervals, domain::contract::ACTION_SCHEDULE_INTERVAL_VALUES);
    }

    /// The required-inputs sentence is cross-client copy (web
    /// `REQUIRED_INPUTS_HINT`) — the four clients must never drift.
    #[test]
    fn required_inputs_hint_matches_the_shared_copy() {
        assert_eq!(
            TRIGGER_REQUIRED_INPUTS_HINT,
            "This action has required inputs, and a triggered run has none to fill them \
             with. Make the inputs optional to enable it."
        );
    }

    fn spec(trigger: Value, device_id: &str) -> TriggerSpec {
        TriggerSpec {
            trigger,
            device_id: device_id.to_string(),
            agent: None,
            account: None,
            model: None,
            effort: None,
        }
    }

    /// SLOP-2: the machine-readable trigger block is cross-client copy —
    /// byte-locked against web `formatTriggerBlock` (`lib/action-triggers.ts`)
    /// and the mobile mirrors, including the compact `JSON.stringify` form
    /// and its key order (the when-part's keys, `deviceId`, then only the
    /// pins that are set).
    #[test]
    fn trigger_block_matches_the_shared_example() {
        // The brief's example: daily 09:00 on device d-1.
        let block = format_trigger_block(&spec(
            json!({"kind": "schedule", "interval": "daily", "minuteOfDay": 540}),
            "d-1",
        ));
        assert_eq!(
            block,
            "\n\nTrigger — after creating the action, call exponential_actions_update with its id and `triggers` set to exactly this array: `[{\"kind\":\"schedule\",\"interval\":\"daily\",\"minuteOfDay\":540,\"deviceId\":\"d-1\"}]`. A triggered run fills no inputs, so declare none as required."
        );

        // The pins ride AFTER the device, in agent/model/effort order, and
        // only when set — an empty pin is omitted, never sent as "". The
        // account never rides (a profile id is device-local).
        let pinned = format_trigger_block(&TriggerSpec {
            agent: Some("codex".to_string()),
            account: Some("prof-1".to_string()),
            model: Some(String::new()),
            effort: Some("high".to_string()),
            ..spec(json!({"kind": "event", "event": "created"}), "d")
        });
        assert!(
            pinned.contains(
                "`[{\"kind\":\"event\",\"event\":\"created\",\"deviceId\":\"d\",\
                 \"agent\":\"codex\",\"effort\":\"high\"}]`"
            ),
            "{pinned}"
        );
    }

    fn stored() -> Vec<ActionTrigger> {
        parse_action_triggers(Some(&json!([
            {"id": "t-1", "enabled": true, "deviceId": "d-1",
             "kind": "schedule", "interval": "daily", "minuteOfDay": 540},
            {"id": "t-2", "enabled": false, "deviceId": "d-2", "agent": "codex",
             "kind": "event", "source": "exponential", "event": "created"},
            // Unreadable: dropped by the read, so it never rides a write.
            {"id": "t-3", "deviceId": "d-1", "kind": "moon_phase"},
        ])))
    }

    /// SLOP-2: every trigger write is the WHOLE array — the readable existing
    /// elements with exactly one changed.
    #[test]
    fn trigger_writes_replace_the_whole_array() {
        let existing = stored();
        assert_eq!(existing.len(), 2);

        // A new element: no `id` (the server mints it), born on, an event
        // names its source, unset pins are absent.
        let new_event = trigger_element(
            &spec(json!({"kind": "event", "event": "pr_merged"}), "d-9"),
            None,
            true,
        );
        assert_eq!(
            new_event,
            json!({"enabled": true, "deviceId": "d-9", "kind": "event",
                   "event": "pr_merged", "source": "exponential"})
        );
        let added = triggers_with_added(&existing, new_event.clone());
        assert_eq!(added.len(), 3);
        assert_eq!(added[0], existing[0].raw);
        assert_eq!(added[2], new_event);

        // An edit keeps the id, the flag and the position; a schedule carries
        // no source; the pins ride when set.
        let edited = trigger_element(
            &TriggerSpec {
                agent: Some("claude".to_string()),
                account: Some("prof-1".to_string()),
                model: Some("opus".to_string()),
                effort: None,
                ..spec(
                    json!({"kind": "schedule", "interval": "weekly", "minuteOfDay": 60,
                           "weekday": 2}),
                    "d-2",
                )
            },
            Some("t-2"),
            false,
        );
        assert_eq!(
            edited,
            json!({"id": "t-2", "enabled": false, "deviceId": "d-2", "agent": "claude",
                   "account": "prof-1", "model": "opus", "kind": "schedule",
                   "interval": "weekly", "minuteOfDay": 60, "weekday": 2})
        );
        let replaced = triggers_with_replaced(&existing, "t-2", edited.clone());
        assert_eq!(replaced, vec![existing[0].raw.clone(), edited]);

        // A toggle moves `enabled` and nothing else — the fingerprint (the
        // when-part) is untouched, so the host's state survives.
        let toggled = triggers_with_enabled(&existing, "t-1", false);
        assert_eq!(toggled[0]["enabled"], json!(false));
        assert_eq!(toggled[1], existing[1].raw);
        assert_eq!(
            coding::automations::parse_action_trigger(&toggled[0]).unwrap().fingerprint(),
            existing[0].fingerprint()
        );

        // A delete sends the array without it.
        assert_eq!(triggers_without(&existing, "t-1"), vec![existing[1].raw.clone()]);
    }

    /// Web `triggerBadges`: a kind is present with any trigger of it, active
    /// while one of them is enabled.
    #[test]
    fn trigger_badges_follow_the_kinds_and_their_enabled_flags() {
        assert_eq!(TriggerBadges::of(&[]), TriggerBadges::default());
        // t-1 = an enabled schedule, t-2 = a paused event.
        assert_eq!(
            TriggerBadges::of(&stored()),
            TriggerBadges { schedule: Some(true), event: Some(false) }
        );
        let schedules = parse_action_triggers(Some(&json!([
            {"id": "a", "enabled": false, "deviceId": "d",
             "kind": "schedule", "interval": "daily", "minuteOfDay": 1},
            {"id": "b", "deviceId": "d",
             "kind": "schedule", "interval": "daily", "minuteOfDay": 2},
        ])));
        assert_eq!(
            TriggerBadges::of(&schedules),
            TriggerBadges { schedule: Some(true), event: None },
            "one enabled trigger of a kind lights it"
        );
        assert_eq!(
            TriggerBadges::of(&schedules[..1]),
            TriggerBadges { schedule: Some(false), event: None }
        );
    }

    /// The Triggers row's sentence: a schedule names the device clock.
    #[test]
    fn trigger_sentence_marks_schedules_as_device_time() {
        let triggers = stored();
        assert_eq!(trigger_sentence(&triggers[0].when), "Daily at 09:00 (device time)");
        assert_eq!(trigger_sentence(&triggers[1].when), "When an issue is created");
    }
}
