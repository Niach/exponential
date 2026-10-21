//! The shared **trigger** form (EXP-530; reshaped by EXP-583, SLOP-2 and
//! NEW-2: a trigger lives on its AGENT definition and the SERVER fires it) —
//! the when-part embedded by [`crate::trigger_dialog`], which adds/edits one
//! element of an agent's `triggers`.
//!
//! A trigger carries only its WHEN: a schedule = a 5-field `cron` and an
//! optional IANA `tz`; an event = the `name` of an `events` row (a builtin
//! like `issue.created`, or anything `events_emit` posts). The section is
//! ONE glass group whose first row is the Schedule · On event strip, over
//! that kind's own field rows. (NEW-6 redoes these screens.)
//!
//! It also owns the pure WRITE shapes ([`agent_trigger_element`] and the
//! whole-array helpers beside it): `agents.update({triggers})` replaces the
//! array, so every add/edit/toggle/delete is "the readable existing
//! elements, with one changed".
//!
//! [`TriggerEditorState::to_trigger`] is the only place the WIRE when-part is
//! built.

use gpui::{
    App, AppContext as _, ClickEvent, Context, Div, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::InputState,
    menu::DropdownMenu as _,
    v_flex, ActiveTheme as _,
};
use serde_json::{json, Value};

use domain::agent_settings::AGENT_SETTINGS;

use crate::controls::glass_input;
use crate::surface;

/// Which pane the section shows. A stored trigger exists to fire, so its
/// when-part is never absent — an agent that only waits for messages simply
/// has no trigger at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TriggerMode {
    Schedule,
    Event,
}

/// A new schedule's cron: weekdays at 09:00.
const DEFAULT_CRON: &str = "0 9 * * 1-5";

pub(crate) struct TriggerEditorState {
    pub(crate) mode: TriggerMode,
    /// The 5-field cron (`m h dom mon dow`).
    cron: Entity<InputState>,
    /// The optional IANA zone (blank = the server's UTC).
    tz: Entity<InputState>,
    /// The event name the trigger listens on.
    name: Entity<InputState>,
}

/// ONE agent trigger element as written (`AgentTrigger`): `id`, `enabled`,
/// then the when-part. `id` is absent on a NEW trigger (the server mints
/// it). The server's schema is STRICT: nothing else rides along.
pub(crate) fn agent_trigger_element(when: &Value, id: Option<&str>, enabled: bool) -> Value {
    let mut element = serde_json::Map::new();
    if let Some(id) = id {
        element.insert("id".to_string(), json!(id));
    }
    element.insert("enabled".to_string(), json!(enabled));
    if let Some(when) = when.as_object() {
        for (key, value) in when {
            element.insert(key.clone(), value.clone());
        }
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
/// id and a when-part it understands (NEW-2: a schedule's `cron`, an
/// event's `name`). Everything else is dropped by the read, so it never
/// rides a write.
fn is_readable(trigger: &Value) -> bool {
    let text = |key: &str| trigger.get(key).and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty());
    trigger_id(trigger).is_some()
        && match trigger.get("kind").and_then(Value::as_str) {
            Some("schedule") => text("cron"),
            Some("event") => text("name"),
            _ => false,
        }
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

/// The triggers the last `agents.update` SENT, per agent, kept until
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

/// Record what a successful `agents.update({triggers})` sent — the
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
    let Some(row) = store.collections().agents.read(cx).get(agent_id).cloned() else {
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
    pub(crate) fn new<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Self {
        let input = |placeholder: &str, value: &str, window: &mut Window, cx: &mut Context<V>| {
            let (placeholder, value) = (placeholder.to_string(), value.to_string());
            cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                state.set_value(value, window, cx);
                state
            })
        };
        Self {
            mode: TriggerMode::Schedule,
            cron: input(DEFAULT_CRON, DEFAULT_CRON, window, cx),
            tz: input("Europe/Vienna", "", window, cx),
            name: input(domain::contract::EVENT_BUILTIN_NAMES[0], domain::contract::EVENT_BUILTIN_NAMES[0], window, cx),
        }
    }

    /// Seed the form from a stored trigger element. An element this build
    /// cannot read never gets here — the tolerant read drops it first.
    pub(crate) fn seed_trigger(&mut self, trigger: &Value, window: &mut Window, cx: &mut App) {
        let text = |key: &str| trigger.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        match trigger.get("kind").and_then(Value::as_str) {
            Some("schedule") => {
                self.mode = TriggerMode::Schedule;
                let (cron, tz) = (text("cron"), text("tz"));
                self.cron.update(cx, |input, cx| input.set_value(cron, window, cx));
                self.tz.update(cx, |input, cx| input.set_value(tz, window, cx));
            }
            Some("event") => {
                self.mode = TriggerMode::Event;
                let name = text("name");
                self.name.update(cx, |input, cx| input.set_value(name, window, cx));
            }
            _ => {}
        }
    }

    /// Build the WIRE when-part — nothing else ([`agent_trigger_element`]
    /// adds the id and the flag around it).
    pub(crate) fn to_trigger(&self, cx: &App) -> Value {
        match self.mode {
            TriggerMode::Schedule => when_schedule(
                self.cron.read(cx).value().as_ref(),
                self.tz.read(cx).value().as_ref(),
            ),
            TriggerMode::Event => when_event(self.name.read(cx).value().as_ref()),
        }
    }

    // -- render ---------------------------------------------------------------

    /// The whole section: ONE inset-grouped card — the Schedule · On event
    /// strip as its FIRST ROW, then the chosen kind's fields.
    ///
    /// `prefix` namespaces the element ids; `access` reaches the state on the
    /// host view.
    pub(crate) fn render<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> gpui::AnyElement {
        let mut rows = vec![self.render_mode_strip(prefix, access, cx)];
        match self.mode {
            TriggerMode::Schedule => {
                rows.push(surface::glass_input_row(
                    "Cron",
                    surface::glass_row_input(glass_input(&self.cron, window, cx)).into_any_element(),
                    cx,
                ));
                rows.push(surface::glass_input_row(
                    "Time zone",
                    surface::glass_row_input(glass_input(&self.tz, window, cx)).into_any_element(),
                    cx,
                ));
            }
            TriggerMode::Event => {
                rows.push(self.builtin_event_row(prefix, cx));
                rows.push(surface::glass_input_row(
                    "Event",
                    surface::glass_row_input(glass_input(&self.name, window, cx)).into_any_element(),
                    cx,
                ));
            }
        }
        v_flex()
            .gap_2()
            .child(surface::glass_group_rows(rows))
            .into_any_element()
    }

    /// The trigger card's first row: the Schedule · On event segments.
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

    /// The `When` row: the builtin events by their copy
    /// (`agent-settings.json` `copy.events`); a pick fills the name field.
    fn builtin_event_row<V: Render>(&self, prefix: &'static str, cx: &mut Context<V>) -> Div {
        let current = self.name.read(cx).value().to_string();
        let label = AGENT_SETTINGS
            .copy
            .events
            .get(current.trim())
            .cloned()
            .unwrap_or_else(|| AGENT_SETTINGS.copy.event.replacen("{name}", current.trim(), 1));
        let name = self.name.clone();
        let control = picker_trigger(format!("{prefix}-event").into(), label, cx)
            .dropdown_menu(move |mut menu, _window, _cx| {
                for builtin in domain::contract::EVENT_BUILTIN_NAMES {
                    let label = AGENT_SETTINGS.copy.events.get(*builtin).cloned().unwrap_or_default();
                    let name = name.clone();
                    menu = menu.item(crate::controls::pointer_label_item(label, false).on_click(
                        move |_, window, cx| {
                            name.update(cx, |input, cx| input.set_value(*builtin, window, cx));
                        },
                    ));
                }
                menu
            })
            .into_any_element();
        surface::glass_picker_row("When", None, control, cx)
    }
}

/// A schedule's when-part: the trimmed cron, the zone only when set.
fn when_schedule(cron: &str, tz: &str) -> Value {
    let mut trigger = json!({"kind": "schedule", "cron": cron.trim()});
    if !tz.trim().is_empty() {
        trigger["tz"] = json!(tz.trim());
    }
    trigger
}

/// An event's when-part: the trimmed name.
fn when_event(name: &str) -> Value {
    json!({"kind": "event", "name": name.trim()})
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
            json!({"id": id, "enabled": enabled, "kind": "schedule", "cron": "0 9 * * *"})
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
                &json!({"kind": "event", "name": "issue.created"}),
                None,
                true,
            )],
            at: now,
        };
        let echoed = vec![json!({"id": "t-9", "enabled": true, "kind": "event", "name": "issue.created"})];
        assert!(write_base(&echoed, Some(&sent), now).is_none());
        // Nothing written, or a write that never echoed: the synced row rules.
        assert!(write_base(&synced, None, now).is_none());
        assert!(write_base(&synced, Some(&written), now + WRITTEN_TRIGGERS_TTL).is_none());
    }

    /// NEW-2: the form's when-parts are the server's strict shapes: a
    /// schedule = cron (+ tz only when set), an event = its name.
    #[test]
    fn when_parts_are_the_strict_wire_shapes() {
        assert_eq!(when_schedule(" 0 9 * * 1-5 ", ""), json!({"kind": "schedule", "cron": "0 9 * * 1-5"}));
        assert_eq!(
            when_schedule("*/10 * * * *", " Europe/Vienna "),
            json!({"kind": "schedule", "cron": "*/10 * * * *", "tz": "Europe/Vienna"})
        );
        assert_eq!(when_event(" ui-changed "), json!({"kind": "event", "name": "ui-changed"}));
        // The builtin event picker reads the fixture's copy for every name.
        for name in domain::contract::EVENT_BUILTIN_NAMES {
            assert!(AGENT_SETTINGS.copy.events.contains_key(*name), "{name}");
        }
    }

    fn stored() -> Vec<Value> {
        vec![
            json!({"id": "t-1", "enabled": true, "kind": "schedule", "cron": "0 9 * * *"}),
            json!({"id": "t-2", "enabled": false, "kind": "event", "name": "issue.created"}),
        ]
    }

    /// Unreadable elements (no id, an unknown kind, a pre-NEW-2 interval
    /// schedule or `event` key) are dropped by the read, so they never ride
    /// a write.
    #[test]
    fn only_readable_triggers_ride_a_write() {
        assert!(stored().iter().all(is_readable));
        assert!(!is_readable(&json!({"id": "t-3", "kind": "moon_phase"})));
        assert!(!is_readable(&json!({"kind": "schedule", "cron": "0 9 * * *"})));
        assert!(!is_readable(&json!({"id": "t-4", "kind": "schedule", "interval": "daily", "minuteOfDay": 1})));
        assert!(!is_readable(&json!({"id": "t-5", "kind": "event", "event": "created"})));
    }

    /// EXP-1294: every trigger write is the WHOLE `AgentTrigger[]` — the
    /// readable existing elements with exactly one changed, no runner fields.
    #[test]
    fn trigger_writes_replace_the_whole_array() {
        let existing = stored();

        // A new element: no `id` (the server mints it), born on, nothing
        // but the strict shape.
        let new_event = agent_trigger_element(&json!({"kind": "event", "name": "pr.merged"}), None, true);
        assert_eq!(new_event, json!({"enabled": true, "kind": "event", "name": "pr.merged"}));
        let added = triggers_with_added(&existing, new_event.clone());
        assert_eq!(added, vec![existing[0].clone(), existing[1].clone(), new_event]);

        // An edit keeps the id, the flag and the position.
        let edited = agent_trigger_element(
            &json!({"kind": "schedule", "cron": "0 1 * * 2", "tz": "Europe/Vienna"}),
            Some("t-2"),
            false,
        );
        assert_eq!(
            edited,
            json!({"id": "t-2", "enabled": false, "kind": "schedule", "cron": "0 1 * * 2", "tz": "Europe/Vienna"})
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
