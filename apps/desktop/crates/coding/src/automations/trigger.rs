//! Trigger parsing (EXP-530): the synced `actions.trigger` JSON → a typed
//! [`ParsedTrigger`]. Manual Value-walking, never derive — a malformed or
//! FUTURE trigger (a kind/event this build predates) must degrade to
//! [`TriggerKind::Unsupported`] keeping its `device_id`, so the bound host
//! can still show "update the app" instead of silently dropping the row.
//! Only an untargetable trigger (no `deviceId`) parses to `None` — no host
//! could ever evaluate it.

use serde_json::Value;

/// One action's parsed automation trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTrigger {
    /// The steer device id (TEXT, not the row uuid) that evaluates and
    /// fires this trigger — every other device ignores it.
    pub device_id: String,
    /// Default TRUE when absent (a trigger exists to run).
    pub enabled: bool,
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

/// Parse the raw trigger Value. `None` = no evaluable trigger at all (null
/// / non-object / missing `deviceId`); every other malformation degrades to
/// `Some(.. Unsupported ..)` so the device keeps a visible, inert row.
pub fn parse_trigger(value: &Value) -> Option<ParsedTrigger> {
    let object = value.as_object()?;
    let device_id = object
        .get("deviceId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?
        .to_string();
    let enabled = object.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let kind = match object.get("kind").and_then(Value::as_str) {
        Some("schedule") => parse_schedule(value),
        Some("event") => parse_event(value),
        _ => None,
    }
    .unwrap_or(TriggerKind::Unsupported);
    Some(ParsedTrigger {
        device_id,
        enabled,
        kind,
    })
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
        // Full valid weekly schedule.
        let weekly = parse_trigger(&json!({
            "kind": "schedule", "deviceId": "dev-1", "enabled": true,
            "interval": "weekly", "minuteOfDay": 540, "weekday": 1
        }))
        .expect("valid schedule parses");
        assert_eq!(weekly.device_id, "dev-1");
        assert!(weekly.enabled);
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
            "kind": "event", "deviceId": "dev-1", "enabled": false,
            "event": "status_changed",
            "filters": {"boardIds": ["b-1"], "toStatusIds": ["s-1", "s-2"]}
        }))
        .expect("valid event parses");
        assert!(!event.enabled);
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
        let future = parse_trigger(&json!({"kind": "cron", "deviceId": "dev-1"}))
            .expect("device-targeted trigger parses");
        assert_eq!(future.kind, TriggerKind::Unsupported);
        // Unknown event → Unsupported.
        let unknown_event =
            parse_trigger(&json!({"kind": "event", "deviceId": "dev-1", "event": "issue_moved"}))
                .unwrap();
        assert_eq!(unknown_event.kind, TriggerKind::Unsupported);
        // Malformed fields (weekly without weekday, minute out of range,
        // non-string filter ids) → Unsupported, never a panic or a fire.
        for bad in [
            json!({"kind": "schedule", "deviceId": "d", "interval": "weekly", "minuteOfDay": 540}),
            json!({"kind": "schedule", "deviceId": "d", "interval": "daily", "minuteOfDay": 1440}),
            json!({"kind": "schedule", "deviceId": "d", "interval": "monthly",
                   "minuteOfDay": 0, "dayOfMonth": 29}),
            json!({"kind": "event", "deviceId": "d", "event": "created",
                   "filters": {"boardIds": [1, 2]}}),
        ] {
            assert_eq!(parse_trigger(&bad).unwrap().kind, TriggerKind::Unsupported, "{bad}");
        }

        // Missing/empty/invalid deviceId → None (untargetable: no host
        // would ever evaluate it).
        assert_eq!(parse_trigger(&json!({"kind": "schedule"})), None);
        assert_eq!(parse_trigger(&json!({"kind": "event", "deviceId": ""})), None);
        assert_eq!(parse_trigger(&json!(null)), None);
        assert_eq!(parse_trigger(&json!("schedule")), None);

        // enabled defaults true.
        let default_enabled = parse_trigger(&json!({
            "kind": "schedule", "deviceId": "dev-1",
            "interval": "daily", "minuteOfDay": 420
        }))
        .unwrap();
        assert!(default_enabled.enabled);
    }

    #[test]
    fn parse_trigger_str_gates_on_valid_json() {
        assert!(parse_trigger_str(
            r#"{"kind":"schedule","deviceId":"d","interval":"daily","minuteOfDay":0}"#
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
