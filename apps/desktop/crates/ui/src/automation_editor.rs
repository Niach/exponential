//! The shared **Automation** section (EXP-530) — one editor embedded by both
//! action dialogs: [`crate::action_editor_dialog`] (which saves the trigger
//! through `actions.update`) and [`crate::start_coding_dialog`]'s create mode
//! (which can't save anything — it appends the wire JSON to the creator run's
//! description so the agent sets it via `exponential_actions_create`).
//!
//! The section is a segmented **None · Schedule · On event** switch over one
//! contextual pane, plus the two fields every trigger needs: the DEVICE that
//! evaluates and fires it (automations are local-only — no server scheduler)
//! and the enabled toggle.
//!
//! [`AutomationEditorState::to_trigger`] is the only place the WIRE shape is
//! built, and it validates instead of clamping: a half-filled section returns
//! a readable message the host dialog renders in its own error slot, so a bad
//! trigger never reaches the server's BAD_REQUEST.
//!
//! The state lives as a plain field on the host view; [`render`] takes a
//! `fn(&mut V) -> &mut Self` accessor so the callbacks can reach back into it
//! without either dialog owning a second entity.
//!
//! [`render`]: AutomationEditorState::render

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, ClickEvent, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _,
};
use serde_json::{json, Value};

use coding::automations::{
    parse_trigger, EventKind, ParsedTrigger, ScheduleInterval, TriggerKind,
};

use crate::controls::WebControl as _;

/// Which pane the section shows. `None` = no automation (the saved trigger is
/// cleared).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomationMode {
    None,
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
/// run has no one to type them. The server refuses an ENABLED trigger on such
/// an action (`actions.update`), so every surface says the same sentence
/// instead of letting the user discover it as a failed save. Byte-shared with
/// the web `AutomationSection` copy.
pub(crate) const AUTOMATION_REQUIRED_INPUTS_HINT: &str =
    "This action has required inputs, and an automated run has none to fill them with. \
     Make the inputs optional to enable it.";

/// The cap the server enforces per filter list — the pickers stop offering
/// more instead of letting the save fail (`ACTION_TRIGGER_MAX_FILTER_IDS`).
fn filter_cap() -> usize {
    domain::contract::ACTION_TRIGGER_MAX_FILTER_IDS
}

/// One device the picker can bind a trigger to.
#[derive(Clone, Debug)]
struct DeviceOption {
    /// The steer TEXT id (`devices.device_id`), NOT the row uuid — the
    /// trigger's `deviceId` and what the host matches itself against.
    device_id: String,
    label: String,
    online: bool,
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
    device_id: Option<String>,
    /// Host-writable: [`crate::action_editor_dialog`] forces it off for an
    /// action with required inputs (see [`Self::has_required_inputs`]).
    pub(crate) enabled: bool,
    /// The hosted action has at least one REQUIRED input — the enabled switch
    /// is locked off (an automated run has nothing to fill them with, and the
    /// server refuses the save anyway). The create-action path leaves it
    /// false: a brand-new action has no inputs yet.
    pub(crate) has_required_inputs: bool,
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
            mode: AutomationMode::None,
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
            enabled: true,
            has_required_inputs: false,
        }
    }

    /// Seed from a synced row's `trigger` JSON. An UNSUPPORTED trigger (a kind
    /// this build predates) deliberately seeds as `None` but KEEPS its device
    /// binding, so re-saving from an old client can't silently retarget it —
    /// the host dialog warns via [`Self::unsupported`].
    pub(crate) fn seed(&mut self, trigger: Option<&Value>, window: &mut Window, cx: &mut App) {
        let Some(parsed) = trigger.and_then(parse_trigger) else {
            return;
        };
        let ParsedTrigger {
            device_id,
            enabled,
            kind,
        } = parsed;
        self.device_id = Some(device_id);
        self.enabled = enabled;
        match kind {
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
            // Inert-but-visible: the section shows "no automation" while the
            // stored trigger stays untouched unless the user picks a mode.
            TriggerKind::Unsupported => self.mode = AutomationMode::None,
        }
    }

    /// True when the row carries a trigger this build cannot represent — the
    /// host dialog blocks the save rather than overwriting it.
    pub(crate) fn unsupported(trigger: Option<&Value>) -> bool {
        trigger
            .and_then(parse_trigger)
            .is_some_and(|parsed| parsed.kind == TriggerKind::Unsupported)
    }

    /// Build the WIRE trigger. `Ok(None)` = no automation (the host sends
    /// `Patch::Null`); `Err` = a readable validation message.
    pub(crate) fn to_trigger(&self, cx: &App) -> Result<Option<Value>, SharedString> {
        if self.mode == AutomationMode::None {
            return Ok(None);
        }
        let Some(device_id) = self.device_id.clone().filter(|id| !id.is_empty()) else {
            return Err("Pick a device for the automation.".into());
        };
        match self.mode {
            AutomationMode::None => Ok(None),
            AutomationMode::Schedule => {
                let raw = self.time.read(cx).value();
                let minute_of_day = parse_minute_of_day(&raw)
                    .ok_or::<SharedString>("Enter a time like 07:00.".into())?;
                let mut trigger = json!({
                    "kind": "schedule",
                    "deviceId": device_id,
                    "enabled": self.enabled,
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
                Ok(Some(trigger))
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
                if matches!(
                    self.event,
                    EventKind::Created | EventKind::PriorityChanged
                ) {
                    put("priorities", &self.priorities);
                }
                if self.event == EventKind::StatusChanged {
                    put("toStatusIds", &self.to_status_ids);
                }
                let mut trigger = json!({
                    "kind": "event",
                    "deviceId": device_id,
                    "enabled": self.enabled,
                    "event": self.event.wire(),
                });
                // An all-empty `filters` object is omitted, not sent as `{}`.
                if !filters.is_empty() {
                    trigger["filters"] = Value::Object(filters);
                }
                Ok(Some(trigger))
            }
        }
    }

    // -- render ---------------------------------------------------------------

    /// The whole section. `prefix` namespaces the element ids (both dialogs
    /// can be open at once); `access` reaches the state on the host view.
    pub(crate) fn render<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut section = v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Automation (optional)"),
            )
            .child(self.render_mode_switch(prefix, access, cx));
        match self.mode {
            AutomationMode::None => {
                section = section.child(div().text_xs().text_color(muted.opacity(0.7)).child(
                    "Without a trigger the action only runs when someone starts it.",
                ));
            }
            AutomationMode::Schedule => {
                section = section
                    .child(self.render_schedule_pane(prefix, access, cx))
                    .child(div().text_xs().text_color(muted.opacity(0.7)).child(
                        "Runs on the selected device, in its local time. A run missed while \
                         the device was offline fires once when it comes back.",
                    ));
            }
            AutomationMode::Event => {
                section = section.child(self.render_event_pane(prefix, access, cx));
            }
        }
        if self.mode != AutomationMode::None {
            section = section
                .child(self.render_device_picker(prefix, access, cx))
                .child(self.render_enabled_switch(prefix, access, cx));
        }
        section.into_any_element()
    }

    fn render_mode_switch<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> impl IntoElement {
        let segment = |label: &'static str, mode: AutomationMode, id: SharedString| {
            crate::controls::segmented_item(self.mode == mode, cx)
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
        crate::controls::segmented(cx)
            .child(segment(
                "None",
                AutomationMode::None,
                format!("{prefix}-mode-none").into(),
            ))
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

    fn render_schedule_pane<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> impl IntoElement {
        let interval_label = INTERVAL_LABELS
            .iter()
            .find(|(interval, _)| *interval == self.interval)
            .map(|(_, label)| *label)
            .unwrap_or("Day");
        let view = cx.entity().downgrade();
        let every = Button::new(SharedString::from(format!("{prefix}-interval")))
            .outline()
            .cursor_pointer()
            .web_input_sm()
            .label(interval_label)
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
            });

        let mut row = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(field_label(cx, "Every"))
            .child(every);
        match self.interval {
            ScheduleInterval::Daily => {}
            ScheduleInterval::Weekly => {
                let current = WEEKDAY_LABELS
                    .iter()
                    .find(|(day, _)| *day == self.weekday)
                    .map(|(_, label)| *label)
                    .unwrap_or("Monday");
                let view = cx.entity().downgrade();
                row = row.child(
                    Button::new(SharedString::from(format!("{prefix}-weekday")))
                        .outline()
                        .cursor_pointer()
                        .web_input_sm()
                        .label(current)
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
                        }),
                );
            }
            ScheduleInterval::Monthly => {
                let current = format!("Day {}", self.day_of_month);
                let view = cx.entity().downgrade();
                row = row.child(
                    Button::new(SharedString::from(format!("{prefix}-day-of-month")))
                        .outline()
                        .cursor_pointer()
                        .web_input_sm()
                        .label(SharedString::from(current))
                        .dropdown_menu(move |mut menu, _window, _cx| {
                            // 1..=28 only — every month has those days, so a
                            // monthly schedule can never skip a month.
                            for day in 1..=28u32 {
                                let view = view.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(SharedString::from(format!("Day {day}")))
                                        .on_click(move |_, _, cx| {
                                            if let Some(view) = view.upgrade() {
                                                view.update(cx, |view, cx| {
                                                    access(view).day_of_month = day;
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                );
                            }
                            menu
                        }),
                );
            }
        }
        row.child(field_label(cx, "at"))
            .child(div().w(px(84.)).child(Input::new(&self.time).web_input_sm()))
    }

    fn render_event_pane<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> impl IntoElement {
        let current = EVENT_LABELS
            .iter()
            .find(|(event, _)| *event == self.event)
            .map(|(_, label)| *label)
            .unwrap_or("An issue is created");
        let view = cx.entity().downgrade();
        let when = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(field_label(cx, "When"))
            .child(
                Button::new(SharedString::from(format!("{prefix}-event")))
                    .outline()
                    .cursor_pointer()
                    .web_input_sm()
                    .label(current)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        for (event, label) in EVENT_LABELS {
                            let view = view.clone();
                            menu = menu.item(PopupMenuItem::new(label).on_click(
                                move |_, _, cx| {
                                    if let Some(view) = view.upgrade() {
                                        view.update(cx, |view, cx| {
                                            access(view).event = event;
                                            cx.notify();
                                        });
                                    }
                                },
                            ));
                        }
                        menu
                    }),
            );

        // The board filter applies to every event; the others are per-event
        // (the server's zod rejects a filter the event doesn't read).
        let boards: Vec<(String, String)> = sync::Store::global(cx)
            .collections()
            .boards_in_team(&self.team_id, cx)
            .into_iter()
            .map(|board| (board.id, board.name))
            .collect();
        let mut pane = v_flex().gap_2().child(when).child(self.render_filter(
            prefix,
            "board",
            "Board",
            "Any board",
            &boards,
            &self.board_ids,
            |state| &mut state.board_ids,
            access,
            cx,
        ));
        match self.event {
            EventKind::LabelAdded => {
                let labels: Vec<(String, String)> = crate::queries::team_labels(cx, &self.team_id)
                    .into_iter()
                    .map(|label| (label.id, label.name))
                    .collect();
                pane = pane.child(self.render_filter(
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
                pane = pane.child(self.render_filter(
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
                pane = pane.child(self.render_filter(
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
        pane
    }

    /// One multi-select filter row: a dropdown of toggleable options over
    /// `selected`, writing through the `pick` field accessor.
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
    ) -> impl IntoElement {
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
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(field_label(cx, label))
            .child(
                Button::new(SharedString::from(format!("{prefix}-filter-{key}")))
                    .outline()
                    .cursor_pointer()
                    .web_input_sm()
                    .label(button_label)
                    .dropdown_menu(move |mut menu, _window, _cx| {
                        if options.is_empty() {
                            return menu.item(PopupMenuItem::new("Nothing to filter on")
                                .disabled(true));
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
                                                    None if list.len() < filter_cap() => {
                                                        list.push(id)
                                                    }
                                                    None => {}
                                                }
                                                cx.notify();
                                            });
                                        }
                                    }),
                            );
                        }
                        menu
                    }),
            )
    }

    fn render_device_picker<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> impl IntoElement {
        let devices = automation_devices(cx);
        let muted = cx.theme().muted_foreground;
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
            return v_flex().gap_1().child(field_label(cx, "Device")).child(
                div().text_xs().text_color(muted).child(
                    "No automation-capable device. Run the desktop app or the exponential \
                     daemon and it will appear here.",
                ),
            );
        }
        let online = self
            .device_id
            .as_deref()
            .and_then(|id| devices.iter().find(|device| device.device_id == id))
            .is_some_and(|device| device.online);
        let view = cx.entity().downgrade();
        let menu_devices = devices.clone();
        let bound = self.device_id.clone();
        let trigger = Button::new(SharedString::from(format!("{prefix}-device")))
            .outline()
            .cursor_pointer()
            .web_input_sm()
            .label(picked.clone().unwrap_or_else(|| "Select device…".into()))
            .dropdown_menu(move |mut menu, _window, _cx| {
                for device in &menu_devices {
                    let view = view.clone();
                    let device_id = device.device_id.clone();
                    // Offline-but-capable stays pickable: the run fires when
                    // the machine comes back (the offline catch-up rule).
                    let label = if device.online {
                        device.label.clone()
                    } else {
                        format!("{} (offline)", device.label)
                    };
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(label))
                            .checked(bound.as_deref() == Some(device_id.as_str()))
                            .on_click(move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    let device_id = device_id.clone();
                                    view.update(cx, |view, cx| {
                                        access(view).device_id = Some(device_id);
                                        cx.notify();
                                    });
                                }
                            }),
                    );
                }
                menu
            });
        v_flex().gap_1().child(field_label(cx, "Device")).child(
            h_flex()
                .gap_2()
                .items_center()
                .child(trigger)
                .when(picked.is_some() && online, |this| {
                    this.child(
                        div()
                            .size_1p5()
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(theme::tokens::GREEN.to_hsla()),
                    )
                }),
        )
    }

    fn render_enabled_switch<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        cx: &mut Context<V>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let locked = self.has_required_inputs;
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_sm().child("Enabled"))
                    .child(
                        Switch::new(SharedString::from(format!("{prefix}-enabled")))
                            // Required inputs lock the switch OFF — the state
                            // shown is the one that will actually be saved.
                            .checked(self.enabled && !locked)
                            .disabled(locked)
                            .when(locked, |this| this.tooltip(AUTOMATION_REQUIRED_INPUTS_HINT))
                            .on_click(cx.listener(move |view: &mut V, on: &bool, _, cx| {
                                access(view).enabled = *on;
                                cx.notify();
                            })),
                    ),
            )
            .when(locked, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted.opacity(0.7))
                        .child(AUTOMATION_REQUIRED_INPUTS_HINT),
                )
            })
    }
}

fn field_label(cx: &App, label: &'static str) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(label)
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

/// The synced devices that advertise the `automations` cap — own rows plus
/// team-shared ones (the shape's scope). Offline rows are INCLUDED: a missed
/// schedule fires once when the machine comes back.
fn automation_devices(cx: &App) -> Vec<DeviceOption> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collection = store.collections().devices.clone();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut devices: Vec<DeviceOption> = collection
        .read(cx)
        .iter()
        .filter(|row| row.cap_ids().iter().any(|cap| cap == "automations"))
        .filter_map(|row| {
            let device_id = row.device_id.clone().filter(|id| !id.is_empty())?;
            Some(DeviceOption {
                label: row.label.clone().unwrap_or_else(|| device_id.clone()),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                device_id,
            })
        })
        .collect();
    // Online first, then by label — the machine most likely to run it leads.
    devices.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.label.cmp(&b.label)));
    devices
}

/// The one-line schedule/event sentence a card or row shows for `trigger`.
/// Thin wrapper over the shared [`coding::automations::trigger_summary`] so
/// callers never re-implement the parse.
pub(crate) fn trigger_summary_for(trigger: Option<&Value>) -> Option<String> {
    let parsed = trigger.and_then(parse_trigger)?;
    Some(coding::automations::trigger_summary(&parsed))
}

/// The parsed trigger of a synced row, for the surfaces that need its parts
/// (device binding, enabled flag, schedule shape) and not just the sentence.
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
    Some(format!(
        "{} (device time)",
        next.format("%b %-d, %H:%M")
    ))
}

/// Flip `enabled` on an existing trigger JSON without touching anything else
/// — the Automations tab's toggle (an edit here must not move the trigger's
/// fingerprint fields, only its on/off flag).
pub(crate) fn with_enabled(trigger: &Value, enabled: bool) -> Option<Value> {
    let mut next = trigger.clone();
    next.as_object_mut()?.insert("enabled".to_string(), json!(enabled));
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn enabled_flips_without_disturbing_the_trigger() {
        let trigger = json!({
            "kind": "schedule", "deviceId": "dev-1", "enabled": true,
            "interval": "daily", "minuteOfDay": 420
        });
        let off = with_enabled(&trigger, false).unwrap();
        assert_eq!(off["enabled"], json!(false));
        assert_eq!(off["minuteOfDay"], json!(420));
        assert_eq!(off["deviceId"], json!("dev-1"));
        // A non-object trigger has no flag to flip.
        assert_eq!(with_enabled(&json!("schedule"), false), None);
    }

    /// The section's parse→edit→serialize loop must round-trip: what
    /// [`AutomationEditorState::seed`] reads, [`to_trigger`] writes back.
    #[test]
    fn wire_shapes_round_trip_through_the_parser() {
        use coding::automations::{EventSpec, Schedule, ScheduleInterval};

        // Weekly schedule.
        let weekly = json!({
            "kind": "schedule", "deviceId": "dev-1", "enabled": true,
            "interval": "weekly", "minuteOfDay": 540, "weekday": 3
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
            trigger_summary_for(Some(&weekly)).as_deref(),
            Some("Weekly on Wednesday at 09:00")
        );

        // Event with filters — the empty lists are absent, not `[]`.
        let event = json!({
            "kind": "event", "deviceId": "dev-1", "enabled": false,
            "event": "status_changed",
            "filters": {"boardIds": ["b-1"], "toStatusIds": ["s-1"]}
        });
        let parsed = parse_trigger(&event).expect("event parses");
        assert!(!parsed.enabled);
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

        // No trigger / an untargetable one yields no sentence.
        assert_eq!(trigger_summary_for(None), None);
        assert_eq!(trigger_summary_for(Some(&json!({"kind": "schedule"}))), None);
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
}
