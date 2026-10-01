//! Trigger parsing (EXP-530; SLOP-2: an action carries its triggers in the
//! synced `actions.triggers` array). [`parse_trigger`] reads an element's
//! WHEN-part into a typed [`ParsedTrigger`]; [`parse_action_triggers`] reads
//! the whole element — the when-part plus its RUNNER (`id`, `enabled`,
//! `deviceId` and the optional launch pins beside it).
//!
//! Manual Value-walking, never derive — a malformed or FUTURE trigger (a
//! kind/event this build predates) must degrade to
//! [`TriggerKind::Unsupported`], so the bound host can still show "update the
//! app" instead of silently dropping the row. Only a non-object trigger
//! (`null`, a string, a missing column) parses to `None` — there is nothing
//! there to evaluate at all.

use serde_json::Value;

/// One automation's parsed trigger — the WHEN-part, nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTrigger {
    pub kind: TriggerKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    Schedule(Schedule),
    Event(EventSpec),
    /// Unknown kind/event or malformed fields — inert but visible.
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleInterval {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub interval: ScheduleInterval,
    /// 0..=1439 — the local wall-clock minute the run fires at.
    pub minute_of_day: u32,
    /// 1=Monday..7=Sunday; required (and only read) for weekly.
    pub weekday: Option<u32>,
    /// 1..=28 (always exists in every month); required for monthly.
    pub day_of_month: Option<u32>,
}

/// The 7 contract event kinds (`actionTrigger.eventValues`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Created,
    StatusChanged,
    AssigneeChanged,
    LabelAdded,
    PriorityChanged,
    PrOpened,
    PrMerged,
}

impl EventKind {
    pub fn wire(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::StatusChanged => "status_changed",
            Self::AssigneeChanged => "assignee_changed",
            Self::LabelAdded => "label_added",
            Self::PriorityChanged => "priority_changed",
            Self::PrOpened => "pr_opened",
            Self::PrMerged => "pr_merged",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "created" => Some(Self::Created),
            "status_changed" => Some(Self::StatusChanged),
            "assignee_changed" => Some(Self::AssigneeChanged),
            "label_added" => Some(Self::LabelAdded),
            "priority_changed" => Some(Self::PriorityChanged),
            "pr_opened" => Some(Self::PrOpened),
            "pr_merged" => Some(Self::PrMerged),
            _ => None,
        }
    }
}

/// An event trigger's kind + filters. An EMPTY vec = the filter is absent
/// (matches everything); the server caps list sizes, the client trusts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventSpec {
    pub event: EventKind,
    pub board_ids: Vec<String>,
    pub label_ids: Vec<String>,
    pub priorities: Vec<String>,
    pub to_status_ids: Vec<String>,
}

/// Parse the raw trigger Value. `None` = nothing there at all (null / a
/// non-object); every malformation inside an object degrades to
/// `Some(.. Unsupported ..)` so the device keeps a visible, inert row.
pub fn parse_trigger(value: &Value) -> Option<ParsedTrigger> {
    let object = value.as_object()?;
    let kind = match object.get("kind").and_then(Value::as_str) {
        Some("schedule") => parse_schedule(value),
        Some("event") => parse_event(value),
        _ => None,
    }
    .unwrap_or(TriggerKind::Unsupported);
    Some(ParsedTrigger { kind })
}

/// The TEXT-column path (native stores sync jsonb as its string form).
/// Unparseable JSON → `None` (nothing to evaluate, nothing to show).
pub fn parse_trigger_str(raw: &str) -> Option<ParsedTrigger> {
    let value = serde_json::from_str::<Value>(raw).ok()?;
    parse_trigger(&value)
}

fn parse_schedule(value: &Value) -> Option<TriggerKind> {
    let interval = match value.get("interval").and_then(Value::as_str)? {
        "daily" => ScheduleInterval::Daily,
        "weekly" => ScheduleInterval::Weekly,
        "monthly" => ScheduleInterval::Monthly,
        _ => return None,
    };
    let minute_of_day = read_u32(value.get("minuteOfDay"), 0, 1439)?;
    let weekday = match interval {
        ScheduleInterval::Weekly => Some(read_u32(value.get("weekday"), 1, 7)?),
        _ => None,
    };
    let day_of_month = match interval {
        ScheduleInterval::Monthly => Some(read_u32(value.get("dayOfMonth"), 1, 28)?),
        _ => None,
    };
    Some(TriggerKind::Schedule(Schedule {
        interval,
        minute_of_day,
        weekday,
        day_of_month,
    }))
}

fn parse_event(value: &Value) -> Option<TriggerKind> {
    // A FUTURE event source reads as "never fires" on this build; an absent
    // one is Exponential's (triggers from before sources existed).
    if value.get("source").is_some_and(|source| source.as_str() != Some("exponential")) {
        return None;
    }
    let event = EventKind::parse(value.get("event").and_then(Value::as_str)?)?;
    let filters = value.get("filters");
    Some(TriggerKind::Event(EventSpec {
        event,
        board_ids: read_id_list(filters, "boardIds")?,
        label_ids: read_id_list(filters, "labelIds")?,
        priorities: read_id_list(filters, "priorities")?,
        to_status_ids: read_id_list(filters, "toStatusIds")?,
    }))
}

fn read_u32(value: Option<&Value>, min: u32, max: u32) -> Option<u32> {
    let number = value?.as_u64()?;
    let number = u32::try_from(number).ok()?;
    (min..=max).contains(&number).then_some(number)
}

/// A missing filter list is EMPTY (absent = match all); a present one must
/// be an array of strings — anything else is malformed (`None`).
fn read_id_list(filters: Option<&Value>, key: &str) -> Option<Vec<String>> {
    let Some(list) = filters.and_then(|f| f.get(key)) else {
        return Some(Vec::new());
    };
    list.as_array()?
        .iter()
        .map(|entry| entry.as_str().map(str::to_string))
        .collect()
}

/// A trigger's optional agent/account/model/effort pins — every `None` falls
/// back to the bound device's own launch defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchPins {
    pub agent: Option<String>,
    /// EXP-995: the agent profile the run spends (belongs to `agent`).
    pub account: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// One READABLE element of an action's `triggers` (SLOP-2): the when-part
/// plus its runner.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionTrigger {
    /// Stable uuid — the device state's key AND the `automationId` a start
    /// stamps on the run it fires.
    pub id: String,
    /// Only an explicit `false` pauses: a missing flag is an enabled trigger.
    pub enabled: bool,
    /// The steer TEXT device id whose host evaluates and fires it.
    pub device_id: String,
    pub pins: LaunchPins,
    /// Never [`TriggerKind::Unsupported`] — an unreadable when-part drops
    /// the whole element.
    pub when: ParsedTrigger,
    /// The element exactly as synced — what a whole-array write sends back
    /// for the triggers it does not touch.
    pub raw: Value,
}

impl ActionTrigger {
    /// The engine's fingerprint — the when-part ONLY, so toggling, rebinding
    /// or re-pinning a trigger never reseeds its firing state.
    pub fn fingerprint(&self) -> String {
        trigger_fingerprint(&when_part(&self.raw))
    }

    pub fn is_schedule(&self) -> bool {
        matches!(self.when.kind, TriggerKind::Schedule(_))
    }
}

/// Tolerant read of ONE stored trigger. An element without a string `id` or
/// `deviceId`, or with an unreadable when-part, is `None`. Never panics.
pub fn parse_action_trigger(value: &Value) -> Option<ActionTrigger> {
    let when = parse_trigger(value)?;
    if when.kind == TriggerKind::Unsupported {
        return None;
    }
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    Some(ActionTrigger {
        id: text("id")?,
        enabled: value.get("enabled").and_then(Value::as_bool) != Some(false),
        device_id: text("deviceId")?,
        pins: LaunchPins {
            agent: text("agent"),
            account: text("account"),
            model: text("model"),
            effort: text("effort"),
        },
        when,
        raw: value.clone(),
    })
}

/// Tolerant read of an action's `triggers`: the readable ones, in order.
pub fn parse_action_triggers(value: Option<&Value>) -> Vec<ActionTrigger> {
    value
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(parse_action_trigger).collect())
        .unwrap_or_default()
}

/// The keys a trigger's WHEN-part may carry — exactly what the pre-SLOP-2
/// `automations.trigger` jsonb held.
const WHEN_KEYS: [&str; 7] = [
    "kind",
    "interval",
    "minuteOfDay",
    "weekday",
    "dayOfMonth",
    "event",
    "filters",
];

/// The when-part of a stored trigger element, as the old `automations.trigger`
/// jsonb spelled it: the runner fields (`id`, `enabled`, `deviceId`, the pins)
/// and the event `source` are dropped. [`trigger_fingerprint`] of this equals
/// the fingerprint the migrated automation row produced, so the persisted
/// device state (keyed by the same id) carries over without a reseed.
pub fn when_part(element: &Value) -> Value {
    let mut when = serde_json::Map::new();
    if let Some(object) = element.as_object() {
        for key in WHEN_KEYS {
            if let Some(value) = object.get(key) {
                when.insert(key.to_string(), value.clone());
            }
        }
    }
    Value::Object(when)
}

/// Canonical fingerprint of the raw trigger JSON — 16 lowercase hex chars.
/// The workspace's serde_json builds with `preserve_order` (feature-unified
/// via the gpui graph), so `Value::to_string` is INSERTION-ordered; the
/// canonical form key-sorts explicitly.
///
/// The digest is SHA-256 truncated to 8 bytes, NOT std's `DefaultHasher`
/// (EXP-562): a changed fingerprint re-seeds the automation state, and
/// `DefaultHasher`'s output is documented as unstable across Rust releases
/// AND across builds — a toolchain bump would silently reseed every
/// automation on every device. The persisted fingerprint must depend on the
/// trigger JSON alone, so it has to survive a recompile.
pub fn trigger_fingerprint(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    let mut canonical = String::new();
    write_canonical(value, &mut canonical);
    let digest = Sha256::digest(canonical.as_bytes());
    // 8 bytes keeps the exact string shape the old hasher wrote (16 hex),
    // so persisted states carry over in width if not in value.
    digest[..8].iter().map(|byte| format!("{byte:02x}")).collect::<String>()
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (position, key) in keys.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String((*key).clone()).to_string());
                out.push(':');
                write_canonical(&map[key.as_str()], out);
            }
            out.push('}');
        }
        Value::Array(entries) => {
            out.push('[');
            for (position, entry) in entries.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                write_canonical(entry, out);
            }
            out.push(']');
        }
        leaf => out.push_str(&leaf.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_wire_values_match_the_contract() {
        // Drift lock: the parser's vocabulary IS the generated contract's.
        let kinds = [
            EventKind::Created,
            EventKind::StatusChanged,
            EventKind::AssigneeChanged,
            EventKind::LabelAdded,
            EventKind::PriorityChanged,
            EventKind::PrOpened,
            EventKind::PrMerged,
        ];
        let wires: Vec<&str> = kinds.iter().map(EventKind::wire).collect();
        assert_eq!(wires, domain::contract::ACTION_TRIGGER_EVENT_VALUES);
        for wire in domain::contract::ACTION_TRIGGER_EVENT_VALUES {
            assert!(EventKind::parse(wire).is_some(), "{wire} must parse");
        }
        assert_eq!(
            domain::contract::ACTION_SCHEDULE_INTERVAL_VALUES,
            &["daily", "weekly", "monthly"]
        );
    }

    /// The parse truth table (the reconcile_truth_table idiom).
    #[test]
    fn parse_truth_table() {
        // Full valid weekly schedule (the when-part alone).
        let weekly = parse_trigger(&json!({
            "kind": "schedule", "interval": "weekly", "minuteOfDay": 540, "weekday": 1
        }))
        .expect("valid schedule parses");
        assert_eq!(
            weekly.kind,
            TriggerKind::Schedule(Schedule {
                interval: ScheduleInterval::Weekly,
                minute_of_day: 540,
                weekday: Some(1),
                day_of_month: None,
            })
        );

        // Full valid event with filters.
        let event = parse_trigger(&json!({
            "kind": "event",
            "event": "status_changed",
            "filters": {"boardIds": ["b-1"], "toStatusIds": ["s-1", "s-2"]}
        }))
        .expect("valid event parses");
        assert_eq!(
            event.kind,
            TriggerKind::Event(EventSpec {
                event: EventKind::StatusChanged,
                board_ids: vec!["b-1".to_string()],
                label_ids: Vec::new(),
                priorities: Vec::new(),
                to_status_ids: vec!["s-1".to_string(), "s-2".to_string()],
            })
        );

        // Unknown kind → Unsupported (a future build's trigger stays
        // visible-but-inert here).
        let future = parse_trigger(&json!({"kind": "cron"})).expect("an object parses");
        assert_eq!(future.kind, TriggerKind::Unsupported);
        // Unknown event → Unsupported.
        let unknown_event =
            parse_trigger(&json!({"kind": "event", "event": "issue_moved"})).unwrap();
        assert_eq!(unknown_event.kind, TriggerKind::Unsupported);
        // Malformed fields (weekly without weekday, minute out of range,
        // non-string filter ids) → Unsupported, never a panic or a fire.
        for bad in [
            json!({"kind": "schedule", "interval": "weekly", "minuteOfDay": 540}),
            json!({"kind": "schedule", "interval": "daily", "minuteOfDay": 1440}),
            json!({"kind": "schedule", "interval": "monthly",
                   "minuteOfDay": 0, "dayOfMonth": 29}),
            json!({"kind": "event", "event": "created", "filters": {"boardIds": [1, 2]}}),
        ] {
            assert_eq!(parse_trigger(&bad).unwrap().kind, TriggerKind::Unsupported, "{bad}");
        }

        // A stored trigger element (the runner beside the when-part) reads
        // as its when-part — the extra keys are simply ignored.
        let legacy = parse_trigger(&json!({
            "kind": "schedule", "deviceId": "dev-1", "enabled": false,
            "interval": "daily", "minuteOfDay": 420
        }))
        .unwrap();
        assert_eq!(
            legacy.kind,
            TriggerKind::Schedule(Schedule {
                interval: ScheduleInterval::Daily,
                minute_of_day: 420,
                weekday: None,
                day_of_month: None,
            })
        );

        // An event names its source: Exponential's (or none) parses, a
        // FUTURE provider's is inert here.
        for source in [json!("exponential"), Value::Null] {
            let mut event = json!({"kind": "event", "event": "created"});
            if !source.is_null() {
                event["source"] = source;
            }
            assert!(matches!(parse_trigger(&event).unwrap().kind, TriggerKind::Event(_)));
        }
        for source in [json!("github"), json!(7), json!(null)] {
            let foreign = json!({"kind": "event", "event": "created", "source": source});
            assert_eq!(parse_trigger(&foreign).unwrap().kind, TriggerKind::Unsupported);
        }

        // Nothing there at all → None.
        assert_eq!(parse_trigger(&json!(null)), None);
        assert_eq!(parse_trigger(&json!("schedule")), None);
    }

    #[test]
    fn parse_trigger_str_gates_on_valid_json() {
        assert!(parse_trigger_str(
            r#"{"kind":"schedule","interval":"daily","minuteOfDay":0}"#
        )
        .is_some());
        assert_eq!(parse_trigger_str("{not json"), None);
        assert_eq!(parse_trigger_str("null"), None);
    }

    #[test]
    fn fingerprint_is_reorder_stable_and_change_sensitive() {
        // The workspace serde_json preserves insertion order — the
        // canonical writer must key-sort at EVERY depth.
        let a: Value = serde_json::from_str(
            r#"{"kind":"event","deviceId":"d","event":"created",
                "filters":{"boardIds":["b-1"],"priorities":["urgent"]}}"#,
        )
        .unwrap();
        let b: Value = serde_json::from_str(
            r#"{"filters":{"priorities":["urgent"],"boardIds":["b-1"]},
                "event":"created","deviceId":"d","kind":"event"}"#,
        )
        .unwrap();
        assert_eq!(trigger_fingerprint(&a), trigger_fingerprint(&b));
        assert_eq!(trigger_fingerprint(&a), trigger_fingerprint(&a.clone()), "stable");

        // Any field change moves it — including inside filters, and array
        // ORDER (a reordered filter list is a genuine edit).
        let mut edited = a.clone();
        edited["filters"]["boardIds"] = json!(["b-2"]);
        assert_ne!(trigger_fingerprint(&a), trigger_fingerprint(&edited));
        let mut minute: Value = serde_json::from_str(
            r#"{"kind":"schedule","deviceId":"d","interval":"daily","minuteOfDay":420}"#,
        )
        .unwrap();
        let base = trigger_fingerprint(&minute);
        minute["minuteOfDay"] = json!(421);
        assert_ne!(base, trigger_fingerprint(&minute));
    }

    /// The tolerant element read (web `parseActionTriggers`): readable ones
    /// in order, everything else skipped.
    #[test]
    fn action_triggers_keep_the_readable_elements_in_order() {
        let triggers = json!([
            {"id": "t-1", "enabled": true, "deviceId": "d-1", "agent": "claude",
             "account": "prof-1", "model": "opus", "effort": "high",
             "kind": "schedule", "interval": "daily", "minuteOfDay": 540},
            // No `enabled` = enabled; an empty pin = unset.
            {"id": "t-2", "deviceId": "d-2", "agent": "",
             "kind": "event", "source": "exponential", "event": "created"},
            {"id": "t-3", "enabled": false, "deviceId": "d-1",
             "kind": "event", "event": "pr_merged"},
            // Skipped: no id, no device, an unreadable when-part, a foreign
            // source, a non-object.
            {"deviceId": "d-1", "kind": "schedule", "interval": "daily", "minuteOfDay": 1},
            {"id": "t-5", "kind": "schedule", "interval": "daily", "minuteOfDay": 1},
            {"id": "t-6", "deviceId": "d-1", "kind": "cron"},
            {"id": "t-7", "deviceId": "d-1", "kind": "event", "source": "github",
             "event": "created"},
            "schedule",
        ]);
        let parsed = parse_action_triggers(Some(&triggers));
        assert_eq!(
            parsed.iter().map(|trigger| trigger.id.as_str()).collect::<Vec<_>>(),
            vec!["t-1", "t-2", "t-3"]
        );
        assert!(parsed[0].enabled && parsed[1].enabled && !parsed[2].enabled);
        assert_eq!(parsed[0].device_id, "d-1");
        assert_eq!(
            parsed[0].pins,
            LaunchPins {
                agent: Some("claude".to_string()),
                account: Some("prof-1".to_string()),
                model: Some("opus".to_string()),
                effort: Some("high".to_string()),
            }
        );
        assert_eq!(parsed[1].pins, LaunchPins::default());
        assert!(parsed[0].is_schedule() && !parsed[1].is_schedule());
        assert_eq!(parsed[0].raw, triggers[0], "the raw element rides along for writes");

        // Not an array / absent → nothing.
        assert!(parse_action_triggers(None).is_empty());
        assert!(parse_action_triggers(Some(&json!({"kind": "schedule"}))).is_empty());
        assert!(parse_action_triggers(Some(&json!(null))).is_empty());
    }

    /// SLOP-2 carry-over lock: a migrated trigger element fingerprints
    /// EXACTLY like the old `automations.trigger` jsonb it was folded from
    /// (migration 0156: runner fields + the jsonb + an event `source`), so
    /// the device state keyed by the same id never reseeds on upgrade.
    #[test]
    fn migrated_element_fingerprint_equals_the_old_row_fingerprint() {
        let old_rows = [
            json!({"kind": "schedule", "interval": "daily", "minuteOfDay": 540}),
            json!({"kind": "schedule", "interval": "weekly", "minuteOfDay": 0, "weekday": 7}),
            json!({"kind": "schedule", "interval": "monthly", "minuteOfDay": 1439,
                   "dayOfMonth": 28}),
            json!({"kind": "event", "event": "created"}),
            json!({"kind": "event", "event": "status_changed",
                   "filters": {"boardIds": ["b-1", "b-2"], "toStatusIds": ["s-1"]}}),
            json!({"kind": "event", "event": "created",
                   "filters": {"priorities": ["urgent"]}}),
        ];
        for old in old_rows {
            let mut element = json!({
                "id": "8d8f2f0e-58a5-4ed1-9a4e-0f5b7f5a7c11",
                "enabled": false,
                "deviceId": "dev-1",
                "agent": "claude",
                "account": "prof-1",
                "model": "opus",
                "effort": "high",
            });
            for (key, value) in old.as_object().unwrap() {
                element[key.as_str()] = value.clone();
            }
            if old["kind"] == "event" {
                element["source"] = json!("exponential");
            }
            let trigger = parse_action_trigger(&element).expect("a migrated element reads");
            assert_eq!(trigger.fingerprint(), trigger_fingerprint(&old), "{old}");
            assert_eq!(when_part(&element), old, "the when-part IS the old jsonb");

            // The runner half never moves it: toggled, rebound, unpinned.
            let mut rebound = element.clone();
            rebound["enabled"] = json!(true);
            rebound["deviceId"] = json!("dev-2");
            rebound.as_object_mut().unwrap().remove("agent");
            assert_eq!(
                parse_action_trigger(&rebound).unwrap().fingerprint(),
                trigger.fingerprint()
            );
        }
        // Pinned against the EXP-562 literal below: same when-part, same hash.
        let pinned = json!({
            "id": "t-1", "deviceId": "dev-9", "source": "exponential",
            "kind": "event", "event": "created",
            "filters": {"boardIds": ["board-1"], "labelIds": [],
                        "priorities": ["urgent"], "toStatusIds": []}
        });
        assert_eq!(
            trigger_fingerprint(&when_part(&pinned)),
            trigger_fingerprint(&json!({
                "kind": "event", "event": "created",
                "filters": {"boardIds": ["board-1"], "labelIds": [],
                            "priorities": ["urgent"], "toStatusIds": []}
            }))
        );
    }

    /// EXP-562 pin: the fingerprint is a pure function of the trigger JSON,
    /// build- and toolchain-independent (hence SHA-256, not std's
    /// `DefaultHasher`). Changing this literal means the algorithm moved,
    /// which RESEEDS every automation on every device once — only ever do
    /// that deliberately.
    #[test]
    fn fingerprint_is_pinned_across_builds() {
        let trigger: Value = serde_json::from_str(
            r#"{"kind":"event","deviceId":"dev-1","event":"created",
                "filters":{"boardIds":["board-1"],"labelIds":[],
                           "priorities":["urgent"],"toStatusIds":[]}}"#,
        )
        .unwrap();
        assert_eq!(trigger_fingerprint(&trigger), "380c9203a0ba0d73");
    }
}
