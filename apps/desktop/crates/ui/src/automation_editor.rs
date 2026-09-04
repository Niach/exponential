//! The shared **Automation** section (EXP-530; reshaped by EXP-583) — the
//! trigger + runner form embedded by [`crate::automation_dialog`] (which
//! creates/edits a real `automations` row) and by
//! [`crate::start_coding_dialog`]'s suggestion-prefilled create mode (which
//! can't save anything — it appends the wire JSON to the creator run's
//! description so the agent sets it via `exponential_automations_create`).
//!
//! Since EXP-583 an automation is its own row, so this section owns FOUR
//! things: the **trigger** (EXP-698 — ONE glass group whose first row is the
//! embedded Schedule · On event strip, over that kind's own field rows), the
//! **device** that evaluates and fires it (automations
//! are local-only — no server scheduler), and the optional **agent / model /
//! effort** pins (every unpinned field falls back to that device's launch
//! defaults). The device picker is ALWAYS shown: an automation can target any
//! automation-capable machine, not just the one authoring it.
//!
//! [`AutomationEditorState::to_trigger`] is the only place the WIRE trigger is
//! built, and it validates instead of clamping: a half-filled section returns
//! a readable message the host dialog renders in its own error slot, so a bad
//! trigger never reaches the server's BAD_REQUEST.
//!
//! The state lives as a plain field on the host view; [`render`] takes a
//! `fn(&mut V) -> &mut Self` accessor so the callbacks can reach back into it
//! without either dialog owning a second entity.
//!
//! [`render`]: AutomationEditorState::render

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
    parse_trigger, EventKind, ParsedTrigger, ScheduleInterval, TriggerKind,
};

use crate::coding_selects::{effort_choices_for, model_choices_for};
use crate::surface;
// EXP-615: the agent/model/effort pins render through the ONE shared launch
// cluster (its Automation variant leads with the "Device default" pill).
use crate::launch_options;
use crate::controls::glass_input;

/// Which pane the section shows. EXP-583 dropped the `None` mode: an
/// automation row exists to fire, so the trigger is never absent — a manual
/// action simply has no automation row at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomationMode {
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

/// Why an action with required inputs can never be automated: an automated
/// run has no one to type them. The server refuses such a target on an
/// ENABLED automation (`automations.create/update`), so every surface says the
/// same sentence instead of letting the user discover it as a failed save.
/// Byte-shared with the web copy.
pub(crate) const AUTOMATION_REQUIRED_INPUTS_HINT: &str =
    "Automations can't run actions with required inputs. Make the inputs optional first.";

/// The cap the server enforces per filter list — the pickers stop offering
/// more instead of letting the save fail (`ACTION_TRIGGER_MAX_FILTER_IDS`).
fn filter_cap() -> usize {
    domain::contract::ACTION_TRIGGER_MAX_FILTER_IDS
}

/// One device the picker can bind an automation to.
#[derive(Clone, Debug)]
pub(crate) struct DeviceOption {
    /// The steer TEXT id (`devices.device_id`), NOT the row uuid — the
    /// automation's `device_id` and what the host matches itself against.
    pub(crate) device_id: String,
    pub(crate) label: String,
    pub(crate) online: bool,
    /// The agent CLIs the machine advertises — the Agent picker offers
    /// exactly these (the server re-checks the pin against the same list).
    pub(crate) agents: Vec<String>,
    /// The machine's configured default launch agent (EXP-437), clamped to
    /// [`Self::agents`] — the strip seeds to it (EXP-615: no "Device
    /// default" pill, same tabs as the launch dialogs).
    pub(crate) default_agent: Option<String>,
    /// EXP-622: this is the signed-in user's DEFAULT machine — the binding
    /// seeds to it. False on a teammate's shared row (their preference).
    pub(crate) is_default: bool,
}

pub(crate) struct AutomationEditorState {
    /// Scopes the board/label/status filter pickers.
    team_id: String,
    pub(crate) mode: AutomationMode,
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
    /// The bound machine's steer id. Always picked here (EXP-583) — an
    /// automation may target any automation-capable device, not only this one.
    pub(crate) device_id: Option<String>,
    /// The pinned launch overrides; every `None` = the device's own defaults.
    pub(crate) agent: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
}

/// Everything a saved (or agent-authored) automation needs beyond its action.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutomationSpec {
    pub(crate) trigger: Value,
    pub(crate) device_id: String,
    pub(crate) agent: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
}

impl AutomationEditorState {
    pub(crate) fn new<V: 'static>(
        team_id: String,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Self {
        let time = cx.new(|cx| InputState::new(window, cx).placeholder("07:00"));
        time.update(cx, |state, cx| state.set_value("09:00", window, cx));
        Self {
            team_id,
            mode: AutomationMode::Schedule,
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
            model: None,
            effort: None,
        }
    }

    /// Preselect the caller's DEFAULT machine (EXP-622) when it is one of the
    /// automation-capable candidates, else the only candidate when there is
    /// exactly one — the common single-machine case, so "New automation" is
    /// one click less. Several machines and no default leaves the pick
    /// explicit.
    pub(crate) fn seed_default_device(&mut self, cx: &mut App) {
        if self.device_id.is_none() {
            let devices = automation_devices(cx);
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
            automation_devices(cx)
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
            // A model/effort belongs to ONE agent — they never survive it.
            self.model = None;
            self.effort = None;
            self.agent = next;
        }
    }

    /// Seed the TRIGGER half from a row's (or a suggestion's) `trigger` JSON.
    /// An UNSUPPORTED trigger (a kind this build predates) leaves the panes on
    /// their defaults; the host dialog blocks the save via [`Self::unsupported`]
    /// so an old client can't silently rewrite it.
    pub(crate) fn seed_trigger(&mut self, trigger: Option<&Value>, window: &mut Window, cx: &mut App) {
        let Some(parsed) = trigger.and_then(parse_trigger) else {
            return;
        };
        match parsed.kind {
            TriggerKind::Schedule(schedule) => {
                self.mode = AutomationMode::Schedule;
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
                self.mode = AutomationMode::Event;
                self.event = spec.event;
                self.board_ids = spec.board_ids;
                self.label_ids = spec.label_ids;
                self.priorities = spec.priorities;
                self.to_status_ids = spec.to_status_ids;
            }
            // Inert-but-visible: the panes stay on their defaults while the
            // stored trigger is untouched (the host blocks the save).
            TriggerKind::Unsupported => {}
        }
    }

    /// Seed the RUNNER half from an existing automation row.
    pub(crate) fn seed_runner(
        &mut self,
        device_id: Option<&str>,
        agent: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
    ) {
        self.device_id = device_id.filter(|id| !id.is_empty()).map(str::to_string);
        self.agent = agent.filter(|value| !value.is_empty()).map(str::to_string);
        self.model = model.filter(|value| !value.is_empty()).map(str::to_string);
        self.effort = effort.filter(|value| !value.is_empty()).map(str::to_string);
    }

    /// True when the row carries a trigger this build cannot represent — the
    /// host dialog blocks the save rather than overwriting it.
    pub(crate) fn unsupported(trigger: Option<&Value>) -> bool {
        trigger
            .and_then(parse_trigger)
            .is_some_and(|parsed| parsed.kind == TriggerKind::Unsupported)
    }

    /// Build the WIRE trigger — the WHEN-part only (EXP-583: the device, the
    /// enabled flag and the launch pins are COLUMNS of the automations row).
    /// `Err` = a readable validation message.
    pub(crate) fn to_trigger(&self, cx: &App) -> Result<Value, SharedString> {
        match self.mode {
            AutomationMode::Schedule => {
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
            AutomationMode::Event => {
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

    /// The whole automation, validated: trigger + the runner binding.
    pub(crate) fn to_spec(&self, cx: &App) -> Result<AutomationSpec, SharedString> {
        let trigger = self.to_trigger(cx)?;
        let Some(device_id) = self.device_id.clone().filter(|id| !id.is_empty()) else {
            return Err("Pick a device for the automation.".into());
        };
        Ok(AutomationSpec {
            trigger,
            device_id,
            agent: self.agent.clone(),
            // A model/effort is only meaningful against a pinned agent (the
            // server validates the pair) — drop them with the agent.
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
            AutomationMode::Schedule => self.schedule_rows(prefix, access, window, cx),
            AutomationMode::Event => self.event_rows(prefix, access, cx),
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
        let segment = |label: &'static str, mode: AutomationMode, id: SharedString| {
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
                AutomationMode::Schedule,
                format!("{prefix}-mode-schedule").into(),
            ))
            .child(segment(
                "On event",
                AutomationMode::Event,
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

        let boards: Vec<(String, String)> = sync::Store::global(cx)
            .collections()
            .boards_in_team(&self.team_id, cx)
            .into_iter()
            .map(|board| (board.id, board.name))
            .collect();
        let mut rows = vec![
            surface::glass_picker_row("When", None, when, cx),
            self.render_filter(
                prefix,
                "board",
                "Board",
                "Any board",
                &boards,
                &self.board_ids,
                |state| &mut state.board_ids,
                access,
                cx,
            ),
        ];
        match self.event {
            EventKind::LabelAdded => {
                let labels: Vec<(String, String)> = crate::queries::team_labels(cx, &self.team_id)
                    .into_iter()
                    .map(|label| (label.id, label.name))
                    .collect();
                rows.push(self.render_filter(
                    prefix,
                    "label",
                    "Label",
                    "Any label",
                    &labels,
                    &self.label_ids,
                    |state| &mut state.label_ids,
                    access,
                    cx,
                ));
            }
            EventKind::Created | EventKind::PriorityChanged => {
                let priorities: Vec<(String, String)> = domain::contract::ISSUE_PRIORITY_VALUES
                    .iter()
                    .map(|value| ((*value).to_string(), capitalize(value)))
                    .collect();
                rows.push(self.render_filter(
                    prefix,
                    "priority",
                    "Priority",
                    "Any priority",
                    &priorities,
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
                let statuses: Vec<(String, String)> =
                    crate::queries::team_statuses(cx, &self.team_id)
                        .into_iter()
                        .filter(|row| row.category != "duplicate")
                        .map(|row| (row.id, row.name))
                        .collect();
                rows.push(self.render_filter(
                    prefix,
                    "status",
                    "To status",
                    "Any status",
                    &statuses,
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
        options: &[(String, String)],
        selected: &[String],
        pick: fn(&mut Self) -> &mut Vec<String>,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> Div {
        let button_label: SharedString = match selected.len() {
            0 => empty_label.into(),
            1 => options
                .iter()
                .find(|(id, _)| id == &selected[0])
                .map(|(_, name)| SharedString::from(name.clone()))
                .unwrap_or_else(|| "1 selected".into()),
            count => format!("{count} selected").into(),
        };
        let options = options.to_vec();
        let picked = selected.to_vec();
        let at_cap = selected.len() >= filter_cap();
        let view = cx.entity().downgrade();
        let control = picker_trigger(
            format!("{prefix}-filter-{key}").into(),
            button_label,
            cx,
        )
        .dropdown_menu(move |mut menu, _window, _cx| {
            if options.is_empty() {
                return menu.item(PopupMenuItem::new("Nothing to filter on").disabled(true));
            }
            for (id, name) in &options {
                let on = picked.iter().any(|entry| entry == id);
                let view = view.clone();
                let id = id.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(name.clone()))
                        .checked(on)
                        // At the cap only DEselection stays live —
                        // the server rejects a longer list.
                        .disabled(at_cap && !on)
                        .on_click(move |_, _, cx| {
                            if let Some(view) = view.upgrade() {
                                let id = id.clone();
                                view.update(cx, |view, cx| {
                                    let list = pick(access(view));
                                    match list.iter().position(|e| e == &id) {
                                        Some(ix) => {
                                            list.remove(ix);
                                        }
                                        None if list.len() < filter_cap() => list.push(id),
                                        None => {}
                                    }
                                    cx.notify();
                                });
                            }
                        }),
                );
            }
            menu
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
        let devices = automation_devices(cx);
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
                                "No automation-capable device. Run the desktop app or the \
                                 exponential daemon and it will appear here.",
                            ),
                    ),
            )]);
        }
        let view = cx.entity().downgrade();
        let menu_devices = devices.clone();
        let bound = self.device_id.clone();
        let trigger = picker_trigger(
            format!("{prefix}-device").into(),
            picked.clone().unwrap_or_else(|| "Select device…".into()),
            cx,
        )
        .dropdown_menu(move |mut menu, _window, _cx| {
            for device in &menu_devices {
                let view = view.clone();
                let device_id = device.device_id.clone();
                // EXP-615: every automation-capable machine reads the
                // same. Offline-but-capable is not a lesser choice — the
                // run fires when the machine comes back (the offline
                // catch-up rule) — so the picker carries no online
                // decoration at all; the Automations LIST shows presence.
                let label = device.label.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(label))
                        .checked(bound.as_deref() == Some(device_id.as_str()))
                        .on_click(move |_, _, cx| {
                            if let Some(view) = view.upgrade() {
                                let device_id = device_id.clone();
                                view.update(cx, |view, cx| {
                                    let state = access(view);
                                    state.device_id = Some(device_id);
                                    // A pin the NEW machine cannot run
                                    // would be refused server-side —
                                    // re-seed to its default agent.
                                    state.ensure_agent_seeded(cx);
                                    cx.notify();
                                });
                            }
                        }),
                );
            }
            menu
        });
        surface::glass_group_rows(vec![surface::glass_picker_row(
            "Runs on",
            None,
            trigger.into_any_element(),
            cx,
        )])
    }

    /// Agent / Model / Effort — the optional pins, rendered by the SHARED
    /// [`crate::launch_options`] cluster in its Automation variant (EXP-615):
    /// the same pill strip the launch dialogs use, led by a "Device default"
    /// pill. The agent list is exactly what the BOUND device advertises (the
    /// server re-checks it), and model/effort only appear once an agent is
    /// pinned: they are validated per agent, and "the device's default agent
    /// with a foreign model" is not a state the server accepts.
    fn render_launch_pins<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> Div {
        let agents = self.device_agents(cx);
        // EXP-721: the strip is a RADIO — one segment is always lit. An
        // unseeded (or no-longer-runnable) pick falls back to the first pill
        // exactly like the launch dialogs' `active_ix`, never to "nothing
        // selected".
        let active = Some(
            self.agent
                .as_deref()
                .and_then(|picked| agents.iter().position(|id| id == picked))
                .unwrap_or(0),
        );
        let click_agents = agents.clone();
        // EXP-694: the strip is the group's FIRST ROW, not a capsule above a
        // labeled column — the same embedded tabs the launch and device
        // pickers lead with.
        let strip = launch_options::agent_tabs_row(
            prefix,
            launch_options::agent_id_pills(&agents),
            active,
            move |view: &mut V, ix, _window, cx| {
                let picked = click_agents.get(ix).cloned();
                let state = access(view);
                // A model/effort belongs to ONE agent — switching agents
                // clears both back to the CLI defaults.
                if state.agent != picked {
                    state.model = None;
                    state.effort = None;
                }
                state.agent = picked;
                cx.notify();
            },
            cx,
        );

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
            automation_devices(cx)
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
fn picker_trigger(id: SharedString, label: impl Into<SharedString>, cx: &App) -> Button {
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
/// [`AutomationEditorState::ensure_agent_seeded`], so the fallback ladder is
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

/// The synced devices that advertise the `automations` cap — own rows plus
/// team-shared ones (the shape's scope). Offline rows are INCLUDED: a missed
/// schedule fires once when the machine comes back.
pub(crate) fn automation_devices(cx: &App) -> Vec<DeviceOption> {
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
            let default_agent = row
                .launch_defaults
                .as_ref()
                .and_then(|value| match value {
                    Value::String(raw) => serde_json::from_str::<Value>(raw).ok(),
                    other => Some(other.clone()),
                })
                .and_then(|value| {
                    value
                        .get("defaultAgent")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .filter(|agent| agents.contains(agent));
            Some(DeviceOption {
                label: row.label.clone().unwrap_or_else(|| device_id.clone()),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                agents,
                default_agent,
                is_default: me
                    .as_deref()
                    .is_some_and(|me| row.user_id.as_deref() == Some(me))
                    && row.is_default.unwrap_or(false),
                device_id,
            })
        })
        .collect();
    // Online first, then by label — the machine most likely to run it leads.
    devices.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.label.cmp(&b.label)));
    devices
}

/// The parsed trigger of a synced row, for the surfaces that need its shape
/// (the next-run label) and not just the sentence.
pub(crate) fn parsed_trigger(trigger: Option<&Value>) -> Option<ParsedTrigger> {
    trigger.and_then(parse_trigger)
}

/// A schedule's next local occurrence, formatted for the Automations list.
/// `None` for event triggers (they have no next time) and for the malformed
/// shapes the parser degrades.
pub(crate) fn next_run_label(parsed: &ParsedTrigger) -> Option<String> {
    let TriggerKind::Schedule(schedule) = &parsed.kind else {
        return None;
    };
    let next = coding::automations::next_occurrence(schedule, chrono::Local::now())?;
    Some(format!("{} (device time)", next.format("%b %-d, %H:%M")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-721: the agent strip is a radio — the ladder must always name a
    /// segment. The three-pill "no device bound yet" state (the `devices`
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
            settle_seed_agent(None, Some("pi"), &all, "codex"),
            Some("pi".to_string())
        );
        // A still-runnable pick is never disturbed.
        assert_eq!(
            settle_seed_agent(Some("claude"), Some("pi"), &all, "codex"),
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
    /// [`AutomationEditorState::seed_trigger`] reads, [`to_trigger`] writes
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

        // Nothing to parse yields nothing; a kind this build predates stays
        // visible-but-inert so the row can say "update the app".
        assert_eq!(parsed_trigger(None), None);
        assert!(AutomationEditorState::unsupported(Some(&json!({"kind": "cron"}))));
        assert!(!AutomationEditorState::unsupported(Some(&weekly)));
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

    /// The required-inputs sentence is byte-shared with the server's refusal
    /// (`automations.create/update`) — the two must never drift.
    #[test]
    fn required_inputs_hint_matches_the_server_message() {
        assert_eq!(
            AUTOMATION_REQUIRED_INPUTS_HINT,
            "Automations can't run actions with required inputs. \
             Make the inputs optional first."
        );
    }
}
