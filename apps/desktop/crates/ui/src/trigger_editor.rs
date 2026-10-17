//! The shared **trigger** form (EXP-530; reshaped by EXP-583 and SLOP-2;
//! EXP-1294: a trigger lives on its AGENT) — the when-part embedded by
//! [`crate::trigger_dialog`], which adds/edits one element of an agent's
//! `triggers`.
//!
//! A trigger carries only its WHEN: the runner is the agent itself (its
//! device, account, model — the agent page's own rows). The section is ONE
//! glass group whose first row is the embedded Schedule · On event strip,
//! over that kind's own field rows (EXP-698).
//!
//! It also owns the pure WRITE shapes ([`agent_trigger_element`] and the
//! whole-array helpers beside it): `sessions.updateAgent({triggers})`
//! replaces the array, so every add/edit/toggle/delete is "the readable
//! existing elements, with one changed".
//!
//! [`TriggerEditorState::to_trigger`] is the only place the WIRE when-part is
//! built.
//!
//! The state lives as a plain field on the host view; [`render`] takes a
//! `fn(&mut V) -> &mut Self` accessor so the callbacks can reach back into it
//! without the dialog owning a second entity.
//!
//! [`render`]: TriggerEditorState::render

use gpui::{
    App, ClickEvent, Context, Div, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    menu::DropdownMenu as _,
    v_flex, ActiveTheme as _,
};
use serde_json::{json, Value};

use coding::triggers::{parse_trigger, EventKind, ScheduleInterval, TriggerKind};

use crate::surface;

/// Which pane the section shows. EXP-583 dropped the `None` mode: a stored
/// trigger exists to fire, so its when-part is never absent — an agent that
/// only waits for messages simply has no trigger at all.
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

/// The cap the server enforces per filter list — the pickers stop offering
/// more instead of letting the save fail.
fn filter_cap() -> usize {
    domain::contract::AGENT_TRIGGER_MAX_FILTER_IDS
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
    /// The local wall-clock time, minutes past midnight (0..1440), picked
    /// as a 24h `HH:mm` hour + minute pair (the natives' time row).
    minute_of_day: u32,
    event: EventKind,
    board_ids: Vec<String>,
    label_ids: Vec<String>,
    priorities: Vec<String>,
    to_status_ids: Vec<String>,
}

/// ONE agent trigger element as written (`AgentTrigger`): `id`, `enabled`,
/// then the when-part, and an event names its source. `id` is absent on a
/// NEW trigger (the server mints it).
pub(crate) fn agent_trigger_element(when: &Value, id: Option<&str>, enabled: bool) -> Value {
    let mut element = serde_json::Map::new();
    if let Some(id) = id {
        element.insert("id".to_string(), json!(id));
    }
    element.insert("enabled".to_string(), json!(enabled));
    let is_event = when.get("kind").and_then(Value::as_str) == Some("event");
    if let Some(when) = when.as_object() {
        for (key, value) in when {
            element.insert(key.clone(), value.clone());
        }
    }
    if is_event {
        element.insert("source".to_string(), json!("exponential"));
    }
    Value::Object(element)
}

/// A stored element's id (`None` = unreadable, never written back).
pub(crate) fn trigger_id(trigger: &Value) -> Option<&str> {
    trigger.get("id").and_then(Value::as_str).filter(|id| !id.is_empty())
}

/// A stored element's on/off flag (absent = on, the server's default).
pub(crate) fn trigger_enabled(trigger: &Value) -> bool {
    trigger.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

/// Whether this build can read (and so may write back) a stored element: an
/// id and a when-part it understands. Everything else is dropped by the read,
/// so it never rides a write.
fn is_readable(trigger: &Value) -> bool {
    trigger_id(trigger).is_some()
        && parse_trigger(trigger).is_some_and(|parsed| !matches!(parsed.kind, TriggerKind::Unsupported))
}

/// The array an ADD sends: every readable existing trigger, then the new one.
pub(crate) fn triggers_with_added(existing: &[Value], element: Value) -> Vec<Value> {
    let mut next = existing.to_vec();
    next.push(element);
    next
}

/// The array an EDIT sends: `element` in the place of the trigger `id` (the
/// order is the list's order, so it never moves).
pub(crate) fn triggers_with_replaced(existing: &[Value], id: &str, element: Value) -> Vec<Value> {
    existing
        .iter()
        .map(|trigger| {
            if trigger_id(trigger) == Some(id) {
                element.clone()
            } else {
                trigger.clone()
            }
        })
        .collect()
}

/// The array a TOGGLE sends: only that trigger's `enabled` moves, so the
/// when-part — and with it the host's firing state — is untouched.
pub(crate) fn triggers_with_enabled(existing: &[Value], id: &str, enabled: bool) -> Vec<Value> {
    existing
        .iter()
        .map(|trigger| {
            let mut element = trigger.clone();
            if trigger_id(trigger) == Some(id) {
                element["enabled"] = json!(enabled);
            }
            element
        })
        .collect()
}

/// The array a DELETE sends: everything but the trigger `id`.
pub(crate) fn triggers_without(existing: &[Value], id: &str) -> Vec<Value> {
    existing
        .iter()
        .filter(|trigger| trigger_id(trigger) != Some(id))
        .cloned()
        .collect()
}

/// The triggers the last `sessions.updateAgent` SENT, per agent, kept until
/// the synced row shows them. Every write replaces the WHOLE array, so a
/// second write built off a row the Electric echo has not reached yet would
/// carry the first one's change undone (switch A off, then B off → A back on).
static WRITTEN_TRIGGERS: std::sync::Mutex<Option<std::collections::HashMap<String, WrittenTriggers>>> =
    std::sync::Mutex::new(None);

/// How long a sent array stays the base without its echo — past it (a
/// teammate's write raced ours, so the row will never equal it) the synced
/// row is the authority again.
const WRITTEN_TRIGGERS_TTL: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Clone)]
struct WrittenTriggers {
    triggers: Vec<Value>,
    at: std::time::Instant,
}

/// Record what a successful `sessions.updateAgent({triggers})` sent — the
/// base of the next write until the synced row shows it ([`agent_triggers`]).
pub(crate) fn note_triggers_written(agent_id: &str, triggers: Vec<Value>) {
    let mut written = WRITTEN_TRIGGERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    written.get_or_insert_with(Default::default).insert(
        agent_id.to_string(),
        WrittenTriggers {
            triggers,
            at: std::time::Instant::now(),
        },
    );
}

/// The array without its server-minted ids: a NEW element is sent id-less,
/// so the echo of the same write only matches once ids are ignored.
fn without_ids(triggers: &[Value]) -> Vec<Value> {
    triggers
        .iter()
        .map(|trigger| {
            let mut element = trigger.clone();
            if let Some(object) = element.as_object_mut() {
                object.remove("id");
            }
            element
        })
        .collect()
}

/// The array the next write starts from: the last write's while the synced
/// row does not show it yet (and the write is fresh), else `None` = the
/// synced row's own.
fn write_base(
    synced: &[Value],
    written: Option<&WrittenTriggers>,
    now: std::time::Instant,
) -> Option<Vec<Value>> {
    let written = written.filter(|written| now.duration_since(written.at) < WRITTEN_TRIGGERS_TTL)?;
    (without_ids(synced) != without_ids(&written.triggers)).then(|| written.triggers.clone())
}

/// An agent's readable triggers, in stored order — what every write above
/// starts from: the synced row's, or the last write's while its echo is still
/// on the way. Empty when the row is not synced.
pub(crate) fn agent_triggers(agent_id: &str, cx: &App) -> Vec<Value> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let Some(row) = store.collections().sessions.read(cx).get(agent_id).cloned() else {
        return Vec::new();
    };
    let synced: Vec<Value> = row.trigger_list().into_iter().filter(is_readable).collect();
    let mut written = WRITTEN_TRIGGERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let pending = written.as_ref().and_then(|written| written.get(agent_id));
    match write_base(&synced, pending, std::time::Instant::now()) {
        Some(base) => base,
        None => {
            // Caught up (or never written): the base has served its purpose.
            if let Some(written) = written.as_mut() {
                written.remove(agent_id);
            }
            synced
        }
    }
}

impl TriggerEditorState {
    pub(crate) fn new(team_id: String) -> Self {
        Self {
            team_id,
            mode: TriggerMode::Schedule,
            interval: ScheduleInterval::Daily,
            weekday: 1,
            day_of_month: 1,
            minute_of_day: DEFAULT_MINUTE_OF_DAY,
            event: EventKind::Created,
            board_ids: Vec::new(),
            label_ids: Vec::new(),
            priorities: Vec::new(),
            to_status_ids: Vec::new(),
        }
    }

    /// Seed the form from a stored trigger element. An element this build
    /// cannot read never gets here — the tolerant read drops it before any
    /// row, so there is nothing to open.
    pub(crate) fn seed_trigger(&mut self, trigger: &Value) {
        let Some(parsed) = parse_trigger(trigger) else {
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
                self.minute_of_day = schedule.minute_of_day.min(MINUTES_PER_DAY - 1);
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

    /// Build the WIRE when-part — nothing else ([`agent_trigger_element`]
    /// adds the id, the flag and an event's source around it).
    pub(crate) fn to_trigger(&self) -> Value {
        match self.mode {
            TriggerMode::Schedule => {
                let minute_of_day = self.minute_of_day.min(MINUTES_PER_DAY - 1);
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
                trigger
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
                trigger
            }
        }
    }

    // -- render ---------------------------------------------------------------

    /// The whole section (EXP-698): ONE inset-grouped card — the Schedule ·
    /// On event strip as its FIRST ROW, a hairline, then the chosen kind's
    /// fields as grouped picker rows.
    ///
    /// `prefix` namespaces the element ids; `access` reaches the state on the
    /// host view.
    pub(crate) fn render<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> gpui::AnyElement {
        let mut rows = vec![self.render_mode_strip(prefix, access, cx)];
        rows.extend(match self.mode {
            TriggerMode::Schedule => self.schedule_rows(prefix, access, cx),
            TriggerMode::Event => self.event_rows(prefix, access, cx),
        });
        v_flex()
            .gap_2()
            .child(surface::glass_group_rows(rows))
            .into_any_element()
    }

    /// The trigger card's first row: the Schedule · On event segments, drawn
    /// with the SAME [`surface::glass_tabs_row`] / [`surface::glass_tab_item`]
    /// pair as every embedded strip — the group draws the hairline under it, so
    /// the strip carries no capsule of its own.
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
                    menu = menu.item(crate::controls::pointer_label_item(label, false).on_click(move |_, _, cx| {
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
                            menu = menu.item(crate::controls::pointer_label_item(label, false).on_click(
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
                            crate::controls::pointer_label_item(SharedString::from(format!("Day {day}")), false).on_click(
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
        rows.push(surface::glass_picker_row(
            "Time",
            None,
            self.time_picker(prefix, access, cx),
            cx,
        ));
        rows
    }

    /// The Time row's trailing value ×4: the 24h `HH:mm`, its hour and its
    /// minute each a picker (every hour, every minute — the natives' time
    /// wheel), then the row's chevron.
    fn time_picker<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> gpui::AnyElement {
        let label = time_label(self.minute_of_day);
        let (hour_label, minute_label) = label.split_once(':').unwrap_or(("09", "00"));
        let current = self.minute_of_day;
        let part = |id: String, text: String, count: u32, is_hour: bool, cx: &mut Context<V>| {
            let view = cx.entity().downgrade();
            picker_trigger_bare(id.into(), text, cx).dropdown_menu(move |menu, _window, _cx| {
                let mut menu = crate::pickers::single_select_menu(menu.scrollable(true).max_h(gpui::px(320.)));
                for value in 0..count {
                    let view = view.clone();
                    let picked = if is_hour { current / 60 } else { current % 60 } == value;
                    menu = menu.item(
                        crate::controls::pointer_label_item(format!("{value:02}"), false)
                            .checked(picked)
                            .on_click(move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |view, cx| {
                                        let state = access(view);
                                        state.minute_of_day = if is_hour {
                                            with_time_part(state.minute_of_day, Some(value), None)
                                        } else {
                                            with_time_part(state.minute_of_day, None, Some(value))
                                        };
                                        cx.notify();
                                    });
                                }
                            }),
                    );
                }
                menu
            })
        };
        let hour = part(format!("{prefix}-time-hour"), hour_label.to_string(), 24, true, cx);
        let minute = part(format!("{prefix}-time-minute"), minute_label.to_string(), 60, false, cx);
        gpui_component::h_flex()
            .items_center()
            .child(hour)
            .child(":")
            .child(minute)
            .child(surface::picker_row_chevron(cx))
            .into_any_element()
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
                    menu = menu.item(crate::controls::pointer_label_item(label, false).on_click(move |_, _, cx| {
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
}

/// The trailing control of a trigger-card row: a caret-ed dropdown trigger
/// stripped of field chrome, exactly like [`crate::launch_options`]'s pins —
/// the GROUP is the field, the row's 16/12 is the padding.
/// EXP-810: `pub(crate)` — the MCP server editor's Transport/Auth rows are
/// the same closed-vocabulary picker trigger.
pub(crate) fn picker_trigger(id: SharedString, label: impl Into<SharedString>, cx: &App) -> Button {
    // A grouped picker row ends in chevron-right ×4, not a caret.
    picker_trigger_bare(id, label, cx).child(surface::picker_row_chevron(cx))
}

/// [`picker_trigger`] without its trailing chevron (a row with nothing else
/// to pick, or one that appends the chevron after its own children).
pub(crate) fn picker_trigger_bare(
    id: SharedString,
    label: impl Into<SharedString>,
    cx: &App,
) -> Button {
    Button::new(id)
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_0()
        .py_0()
        .text_color(cx.theme().foreground.opacity(0.7))
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

const MINUTES_PER_DAY: u32 = 24 * 60;

/// A new schedule's time: 09:00.
const DEFAULT_MINUTE_OF_DAY: u32 = 9 * 60;

/// Minute of day → the 24h `HH:mm` the time row shows ×4.
fn time_label(minute_of_day: u32) -> String {
    let minute_of_day = minute_of_day.min(MINUTES_PER_DAY - 1);
    format!("{:02}:{:02}", minute_of_day / 60, minute_of_day % 60)
}

/// Replace the hour (`Some(h)`) or the minute (`Some(m)`) of a minute of day.
fn with_time_part(minute_of_day: u32, hour: Option<u32>, minute: Option<u32>) -> u32 {
    let hour = hour.unwrap_or(minute_of_day / 60).min(23);
    let minute = minute.unwrap_or(minute_of_day % 60).min(59);
    hour * 60 + minute
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second whole-array write before the Electric echo must start from
    /// what the FIRST write sent — else switching A off and then B off sends
    /// A enabled again. The echo of the same write (ids minted for new
    /// elements) catches up; a stale base expires.
    #[test]
    fn the_next_write_starts_from_the_last_write_until_the_synced_row_catches_up() {
        let element = |id: &str, enabled: bool| {
            json!({"id": id, "enabled": enabled,
                   "kind": "schedule", "interval": "daily", "minuteOfDay": 540})
        };
        let now = std::time::Instant::now();
        let written = WrittenTriggers {
            triggers: vec![element("a", false), element("b", true)],
            at: now,
        };
        // The synced row still reads [a on, b on]: the written array is the
        // base, so B's toggle keeps A off.
        let synced = vec![element("a", true), element("b", true)];
        let base = write_base(&synced, Some(&written), now).expect("the synced row lags");
        let next = triggers_with_enabled(&base, "b", false);
        assert_eq!(next, vec![element("a", false), element("b", false)]);
        // The echo of the same write is caught up.
        assert!(write_base(&written.triggers, Some(&written), now).is_none());
        // A new element is sent id-less; its echo carries the minted id.
        let sent = WrittenTriggers {
            triggers: vec![agent_trigger_element(
                &json!({"kind": "event", "event": "created"}),
                None,
                true,
            )],
            at: now,
        };
        let echoed = vec![json!({"id": "t-9", "enabled": true, "kind": "event",
                                 "event": "created", "source": "exponential"})];
        assert!(write_base(&echoed, Some(&sent), now).is_none());
        // Nothing written, or a write that never echoed: the synced row rules.
        assert!(write_base(&synced, None, now).is_none());
        assert!(write_base(&synced, Some(&written), now + WRITTEN_TRIGGERS_TTL).is_none());
    }

    /// The time row reads 24h `HH:mm` ×4 and edits one half at a time.
    #[test]
    fn time_row_is_24h_hh_mm() {
        assert_eq!(time_label(420), "07:00");
        assert_eq!(time_label(425), "07:05");
        assert_eq!(time_label(1439), "23:59");
        assert_eq!(time_label(0), "00:00");
        assert_eq!(time_label(5000), "23:59", "clamped, never past midnight");
        assert_eq!(with_time_part(425, Some(18), None), 18 * 60 + 5);
        assert_eq!(with_time_part(425, None, Some(30)), 7 * 60 + 30);
        assert_eq!(time_label(DEFAULT_MINUTE_OF_DAY), "09:00");
    }

    /// The form's parse→edit→serialize loop round-trips: what
    /// [`TriggerEditorState::seed_trigger`] reads off a stored element,
    /// [`TriggerEditorState::to_trigger`] writes back as the when-part alone
    /// (no `id`, no `enabled`, no `source`).
    #[test]
    fn when_parts_round_trip_through_the_form() {
        let mut state = TriggerEditorState::new("team-1".to_string());
        state.seed_trigger(&json!({
            "id": "t-1", "enabled": false,
            "kind": "schedule", "interval": "weekly", "minuteOfDay": 540, "weekday": 3
        }));
        assert_eq!(
            state.to_trigger(),
            json!({"kind": "schedule", "interval": "weekly", "minuteOfDay": 540, "weekday": 3})
        );

        // Filters only the event reads ride along; empty lists are absent.
        state.seed_trigger(&json!({
            "id": "t-2", "enabled": true, "source": "exponential",
            "kind": "event", "event": "status_changed",
            "filters": {"boardIds": ["b-1"], "toStatusIds": ["s-1"]}
        }));
        assert_eq!(
            state.to_trigger(),
            json!({"kind": "event", "event": "status_changed",
                   "filters": {"boardIds": ["b-1"], "toStatusIds": ["s-1"]}})
        );
        state.event = EventKind::PrOpened;
        assert_eq!(
            state.to_trigger(),
            json!({"kind": "event", "event": "pr_opened", "filters": {"boardIds": ["b-1"]}})
        );
    }

    /// The event picker's copy is cross-client — lock the 7 labels and their
    /// contract order.
    #[test]
    fn event_labels_cover_the_contract_in_order() {
        let wires: Vec<&str> = EVENT_LABELS.iter().map(|(kind, _)| kind.wire()).collect();
        assert_eq!(wires, domain::contract::AGENT_TRIGGER_EVENT_VALUES);
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
        assert_eq!(intervals, domain::contract::AGENT_SCHEDULE_INTERVAL_VALUES);
    }

    fn stored() -> Vec<Value> {
        vec![
            json!({"id": "t-1", "enabled": true,
                   "kind": "schedule", "interval": "daily", "minuteOfDay": 540}),
            json!({"id": "t-2", "enabled": false,
                   "kind": "event", "source": "exponential", "event": "created"}),
        ]
    }

    /// Unreadable elements (no id, an unknown kind) are dropped by the read,
    /// so they never ride a write.
    #[test]
    fn only_readable_triggers_ride_a_write() {
        assert!(stored().iter().all(is_readable));
        assert!(!is_readable(&json!({"id": "t-3", "kind": "moon_phase"})));
        assert!(!is_readable(&json!({"kind": "schedule", "interval": "daily", "minuteOfDay": 1})));
    }

    /// EXP-1294: every trigger write is the WHOLE `AgentTrigger[]` — the
    /// readable existing elements with exactly one changed, no runner fields.
    #[test]
    fn trigger_writes_replace_the_whole_array() {
        let existing = stored();

        // A new element: no `id` (the server mints it), born on, an event
        // names its source.
        let new_event = agent_trigger_element(&json!({"kind": "event", "event": "pr_merged"}), None, true);
        assert_eq!(
            new_event,
            json!({"enabled": true, "kind": "event", "event": "pr_merged", "source": "exponential"})
        );
        let added = triggers_with_added(&existing, new_event.clone());
        assert_eq!(added, vec![existing[0].clone(), existing[1].clone(), new_event]);

        // An edit keeps the id, the flag and the position; a schedule carries
        // no source.
        let edited = agent_trigger_element(
            &json!({"kind": "schedule", "interval": "weekly", "minuteOfDay": 60, "weekday": 2}),
            Some("t-2"),
            false,
        );
        assert_eq!(
            edited,
            json!({"id": "t-2", "enabled": false, "kind": "schedule",
                   "interval": "weekly", "minuteOfDay": 60, "weekday": 2})
        );
        let replaced = triggers_with_replaced(&existing, "t-2", edited.clone());
        assert_eq!(replaced, vec![existing[0].clone(), edited]);

        // A toggle moves `enabled` and nothing else.
        let toggled = triggers_with_enabled(&existing, "t-1", false);
        assert_eq!(toggled[0]["enabled"], json!(false));
        assert!(!trigger_enabled(&toggled[0]));
        assert_eq!(toggled[1], existing[1]);

        // A delete sends the array without it.
        assert_eq!(triggers_without(&existing, "t-1"), vec![existing[1].clone()]);
    }
}
