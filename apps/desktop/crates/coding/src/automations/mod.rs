//! Action automations engine (EXP-530): local-only schedules + event
//! triggers over the synced `actions.trigger` column. PURE — hosts (the
//! desktop GUI's automation host, the CLI daemon's worker) snapshot their
//! inputs (parsed triggers, persisted [`AutomationState`]s, pre-fetched
//! event rows, live-run ids, the clock) and [`evaluate`] returns
//! [`Decision`]s; every side effect (settings IO, the launch) is the
//! host's.
//!
//! Firing protocol (both hosts, in order): claim the `action:{id}`
//! reservation → persist `new_state` via [`write_states`] FIRST (the
//! watermark/anchor advances at launch START — a crash mid-launch drops
//! the run rather than double-firing) → prepare/launch. A FAILED prepare
//! gets one more state write extending `cooldown_until` by
//! [`PREPARE_FAILURE_BACKOFF_MS`] — a poison-pill trigger backs off
//! instead of hot-looping.

pub mod events;
pub mod schedule;
pub mod state;
pub mod summary;
pub mod trigger;

use std::collections::{HashMap, HashSet};

pub use events::{event_matches, matching_events, EventRow};
pub use schedule::{latest_occurrence, next_occurrence};
pub use state::{read_states, write_states, AutomationState, AUTOMATIONS_KEY};
pub use summary::{schedule_phrase, trigger_summary};
pub use trigger::{
    parse_trigger, parse_trigger_str, trigger_fingerprint, EventKind, EventSpec, ParsedTrigger,
    Schedule, ScheduleInterval, TriggerKind,
};

/// Backoff a failed prepare stamps onto `cooldown_until` (poison-pill).
pub const PREPARE_FAILURE_BACKOFF_MS: i64 = 5 * 60_000;
/// Cap on the issue lines an event firing renders into the trigger prompt
/// section — the overflow becomes one "…and N more." line.
pub const TRIGGER_PROMPT_MAX_LINES: usize = 50;

/// One action this device's trigger targets, pre-parsed by the host.
#[derive(Clone, Debug)]
pub struct TriggeredAction {
    pub action_id: String,
    pub trigger: ParsedTrigger,
    /// [`trigger_fingerprint`] of the RAW trigger JSON.
    pub fingerprint: String,
}

/// One evaluation pass's snapshot.
pub struct EvalInput<'a, Tz: chrono::TimeZone> {
    pub actions: &'a [TriggeredAction],
    /// This device's persisted states ([`read_states`]).
    pub states: &'a HashMap<String, AutomationState>,
    /// Candidate event rows (host-fetched, ≤ the catch-up window is fine —
    /// the engine re-applies the cutoff).
    pub events: &'a [EventRow],
    /// Actions with a LIVE local run — deferred, never double-launched.
    pub live_action_ids: &'a HashSet<String>,
    pub now_ms: i64,
    /// The same instant in the device's zone (tests pin `FixedOffset`).
    pub now_local: chrono::DateTime<Tz>,
}

/// What a [`Decision::Fire`] fires on (owned — outlives the snapshot).
#[derive(Clone, Debug, PartialEq)]
pub enum Firing {
    Schedule { occurrence_ms: i64 },
    Event { matches: Vec<EventRow> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// Launch the action and persist `new_state` FIRST (see the firing
    /// protocol above).
    Fire {
        action_id: String,
        firing: Firing,
        new_state: AutomationState,
    },
    /// First observation of this trigger (or an edited one): persist the
    /// seeded state, fire NOTHING — an automation never fires
    /// retroactively on creation or edit.
    Reseed {
        action_id: String,
        new_state: AutomationState,
    },
    /// Cooling down or already running — re-evaluate next beat.
    Defer { action_id: String },
}

/// The per-action decision cascade: (1) disabled/unsupported → inert;
/// (2) unseen or edited trigger → [`Decision::Reseed`]; (3) cooldown →
/// defer; (4) live run → defer; (5) schedule fires on a new latest
/// occurrence (offline catch-up = exactly one run, anchored at the
/// occurrence — never `now` — so the anchor can't drift); (6) event fires
/// ONCE over all matches past the watermark, advancing it over the WHOLE
/// snapshot.
pub fn evaluate<Tz: chrono::TimeZone>(input: &EvalInput<Tz>) -> Vec<Decision> {
    let snapshot_watermark = input
        .events
        .iter()
        .map(|row| (row.created_at_ms, row.id.as_str()))
        .max()
        .unwrap_or((input.now_ms, ""));
    let mut decisions = Vec::new();
    for action in input.actions {
        if !action.trigger.enabled || action.trigger.kind == TriggerKind::Unsupported {
            continue;
        }
        let state = input.states.get(&action.action_id);
        // (2) Reseed: no state, an edited trigger, or a state missing the
        // stamps this kind needs (a schedule↔event edit keeps the
        // fingerprint moving, but an older writer could leave holes —
        // self-heal conservatively instead of firing blind).
        let needs_reseed = match state {
            None => true,
            Some(existing) if existing.fingerprint != action.fingerprint => true,
            Some(existing) => match &action.trigger.kind {
                TriggerKind::Schedule(_) => existing.last_fired_at.is_none(),
                TriggerKind::Event(_) => {
                    existing.watermark_created_at.is_none() || existing.watermark_id.is_none()
                }
                TriggerKind::Unsupported => false,
            },
        };
        if needs_reseed {
            decisions.push(Decision::Reseed {
                action_id: action.action_id.clone(),
                new_state: AutomationState {
                    fingerprint: action.fingerprint.clone(),
                    last_fired_at: Some(input.now_ms),
                    watermark_created_at: Some(snapshot_watermark.0),
                    watermark_id: Some(snapshot_watermark.1.to_string()),
                    cooldown_until: None,
                },
            });
            continue;
        }
        let state = state.expect("reseed handled the missing-state arm");
        // (3) + (4): both defer — the trigger re-evaluates next beat.
        let cooling = state
            .cooldown_until
            .is_some_and(|until| input.now_ms < until);
        if cooling || input.live_action_ids.contains(&action.action_id) {
            decisions.push(Decision::Defer {
                action_id: action.action_id.clone(),
            });
            continue;
        }
        match &action.trigger.kind {
            TriggerKind::Schedule(shape) => {
                let Some(occurrence) = latest_occurrence(shape, input.now_local.clone()) else {
                    continue;
                };
                let occurrence_ms = occurrence.timestamp_millis();
                if occurrence_ms > state.last_fired_at.unwrap_or(input.now_ms) {
                    decisions.push(Decision::Fire {
                        action_id: action.action_id.clone(),
                        firing: Firing::Schedule { occurrence_ms },
                        new_state: AutomationState {
                            fingerprint: state.fingerprint.clone(),
                            // Anchor AT the occurrence, not now — firing at
                            // 07:03 must not drift tomorrow's compare.
                            last_fired_at: Some(occurrence_ms),
                            watermark_created_at: state.watermark_created_at,
                            watermark_id: state.watermark_id.clone(),
                            cooldown_until: Some(
                                input.now_ms + domain::contract::AUTOMATION_COOLDOWN_MS,
                            ),
                        },
                    });
                }
            }
            TriggerKind::Event(spec) => {
                let watermark = (
                    state.watermark_created_at.expect("reseed guaranteed the stamp"),
                    state.watermark_id.as_deref().expect("reseed guaranteed the id"),
                );
                let matches = matching_events(spec, input.events, watermark, input.now_ms);
                if matches.is_empty() {
                    continue;
                }
                decisions.push(Decision::Fire {
                    action_id: action.action_id.clone(),
                    firing: Firing::Event {
                        matches: matches.into_iter().cloned().collect(),
                    },
                    new_state: AutomationState {
                        fingerprint: state.fingerprint.clone(),
                        last_fired_at: Some(input.now_ms),
                        // Advance over the WHOLE snapshot (non-matching
                        // rows included) — they were seen and rejected,
                        // never to be re-litigated.
                        watermark_created_at: Some(snapshot_watermark.0),
                        watermark_id: Some(snapshot_watermark.1.to_string()),
                        cooldown_until: Some(
                            input.now_ms + domain::contract::AUTOMATION_COOLDOWN_MS,
                        ),
                    },
                });
            }
            TriggerKind::Unsupported => {}
        }
    }
    decisions
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, TimeZone};

    fn tz() -> FixedOffset {
        FixedOffset::east_opt(3600).unwrap()
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<FixedOffset> {
        tz().with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    fn daily_action(id: &str, minute: u32, enabled: bool) -> TriggeredAction {
        TriggeredAction {
            action_id: id.to_string(),
            trigger: ParsedTrigger {
                device_id: "dev-1".to_string(),
                enabled,
                kind: TriggerKind::Schedule(Schedule {
                    interval: ScheduleInterval::Daily,
                    minute_of_day: minute,
                    weekday: None,
                    day_of_month: None,
                }),
            },
            fingerprint: format!("fp-{id}"),
        }
    }

    fn event_action(id: &str) -> TriggeredAction {
        TriggeredAction {
            action_id: id.to_string(),
            trigger: ParsedTrigger {
                device_id: "dev-1".to_string(),
                enabled: true,
                kind: TriggerKind::Event(EventSpec {
                    event: EventKind::Created,
                    board_ids: Vec::new(),
                    label_ids: Vec::new(),
                    priorities: Vec::new(),
                    to_status_ids: Vec::new(),
                }),
            },
            fingerprint: format!("fp-{id}"),
        }
    }

    fn event_row(id: &str, kind: &str, created_at_ms: i64) -> EventRow {
        EventRow {
            id: id.to_string(),
            issue_id: "issue-1".to_string(),
            created_at_ms,
            kind: kind.to_string(),
            payload: None,
            board_id: Some("board-1".to_string()),
        }
    }

    fn seeded(fingerprint: &str, fired_ms: i64, watermark: (i64, &str)) -> AutomationState {
        AutomationState {
            fingerprint: fingerprint.to_string(),
            last_fired_at: Some(fired_ms),
            watermark_created_at: Some(watermark.0),
            watermark_id: Some(watermark.1.to_string()),
            cooldown_until: None,
        }
    }

    struct Snapshot {
        actions: Vec<TriggeredAction>,
        states: HashMap<String, AutomationState>,
        events: Vec<EventRow>,
        live: HashSet<String>,
        now_local: DateTime<FixedOffset>,
    }

    impl Snapshot {
        fn new(now_local: DateTime<FixedOffset>) -> Self {
            Self {
                actions: Vec::new(),
                states: HashMap::new(),
                events: Vec::new(),
                live: HashSet::new(),
                now_local,
            }
        }

        fn evaluate(&self) -> Vec<Decision> {
            evaluate(&EvalInput {
                actions: &self.actions,
                states: &self.states,
                events: &self.events,
                live_action_ids: &self.live,
                now_ms: self.now_local.timestamp_millis(),
                now_local: self.now_local.clone(),
            })
        }
    }

    /// The evaluation truth table (the reconcile_truth_table idiom): one
    /// scenario + rationale per assert.
    #[test]
    fn evaluate_truth_table() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let today_0700_ms = at(2026, 8, 18, 7, 0).timestamp_millis();
        let yesterday_0700_ms = at(2026, 8, 17, 7, 0).timestamp_millis();

        // Reseed never fires: a NEW event trigger with matching past
        // events seeds lastFiredAt=now + watermark=max event — no Fire.
        let mut fresh = Snapshot::new(now.clone());
        fresh.actions = vec![event_action("act-e")];
        fresh.events = vec![
            event_row("evt-1", "created", now_ms - 5_000),
            event_row("evt-2", "created", now_ms - 1_000),
        ];
        assert_eq!(
            fresh.evaluate(),
            vec![Decision::Reseed {
                action_id: "act-e".to_string(),
                new_state: seeded("fp-act-e", now_ms, (now_ms - 1_000, "evt-2")),
            }],
            "first observation seeds the watermark, never fires retroactively"
        );

        // An edited trigger reseeds (fingerprint moved), even mid-steady-state.
        let mut edited = Snapshot::new(now.clone());
        edited.actions = vec![daily_action("act-s", 420, true)];
        edited
            .states
            .insert("act-s".to_string(), seeded("fp-STALE", yesterday_0700_ms, (0, "")));
        match &edited.evaluate()[..] {
            [Decision::Reseed { action_id, new_state }] => {
                assert_eq!(action_id, "act-s", "edited trigger reseeds instead of firing");
                assert_eq!(new_state.fingerprint, "fp-act-s");
                assert_eq!(new_state.last_fired_at, Some(now_ms));
            }
            other => panic!("expected one Reseed, got {other:?}"),
        }

        // Schedule offline catch-up: 3 missed days collapse into EXACTLY
        // one fire — the latest occurrence.
        let mut catchup = Snapshot::new(now.clone());
        catchup.actions = vec![daily_action("act-s", 420, true)];
        let three_days_ago = at(2026, 8, 15, 7, 0).timestamp_millis();
        catchup
            .states
            .insert("act-s".to_string(), seeded("fp-act-s", three_days_ago, (0, "")));
        match &catchup.evaluate()[..] {
            [Decision::Fire { action_id, firing, new_state }] => {
                assert_eq!(action_id, "act-s");
                assert_eq!(
                    firing,
                    &Firing::Schedule { occurrence_ms: today_0700_ms },
                    "missed occurrences collapse into the latest one"
                );
                // Anchor no-drift: fired at 07:03 → the anchor is the
                // 07:00 occurrence, and the cooldown stamps from now.
                assert_eq!(new_state.last_fired_at, Some(today_0700_ms));
                assert_eq!(
                    new_state.cooldown_until,
                    Some(now_ms + domain::contract::AUTOMATION_COOLDOWN_MS)
                );
                assert_eq!(new_state.watermark_created_at, Some(0), "schedule keeps the watermark");
            }
            other => panic!("expected one Fire, got {other:?}"),
        }

        // Already anchored at today's occurrence → nothing (no Defer spam).
        let mut steady = Snapshot::new(now.clone());
        steady.actions = vec![daily_action("act-s", 420, true)];
        steady
            .states
            .insert("act-s".to_string(), seeded("fp-act-s", today_0700_ms, (0, "")));
        assert_eq!(steady.evaluate(), vec![], "an anchored schedule is silent until the next tick");

        // Event coalescing: 3 matches → ONE Fire carrying all of them; the
        // watermark advances over a NEWER NON-MATCHING row too.
        let mut coalesce = Snapshot::new(now.clone());
        coalesce.actions = vec![event_action("act-e")];
        coalesce
            .states
            .insert("act-e".to_string(), seeded("fp-act-e", 0, (now_ms - 10_000, "evt-0")));
        coalesce.events = vec![
            event_row("evt-1", "created", now_ms - 5_000),
            event_row("evt-2", "created", now_ms - 4_000),
            event_row("evt-3", "created", now_ms - 3_000),
            event_row("evt-4", "status_changed", now_ms - 1_000), // non-matching, newest
        ];
        match &coalesce.evaluate()[..] {
            [Decision::Fire { firing: Firing::Event { matches }, new_state, .. }] => {
                assert_eq!(
                    matches.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
                    vec!["evt-1", "evt-2", "evt-3"],
                    "one coalesced fire carries every match"
                );
                assert_eq!(
                    (new_state.watermark_created_at, new_state.watermark_id.as_deref()),
                    (Some(now_ms - 1_000), Some("evt-4")),
                    "the watermark advances over non-matching rows too"
                );
                assert_eq!(new_state.last_fired_at, Some(now_ms));
            }
            other => panic!("expected one event Fire, got {other:?}"),
        }

        // Cooldown defers.
        let mut cooling = Snapshot::new(now.clone());
        cooling.actions = vec![event_action("act-e")];
        let mut cooled = seeded("fp-act-e", 0, (0, ""));
        cooled.cooldown_until = Some(now_ms + 30_000);
        cooling.states.insert("act-e".to_string(), cooled);
        cooling.events = vec![event_row("evt-1", "created", now_ms - 1_000)];
        assert_eq!(
            cooling.evaluate(),
            vec![Decision::Defer { action_id: "act-e".to_string() }],
            "a cooling trigger defers, keeping the pending events for the next beat"
        );

        // A live run for the action defers.
        let mut running = Snapshot::new(now.clone());
        running.actions = vec![daily_action("act-s", 420, true)];
        running
            .states
            .insert("act-s".to_string(), seeded("fp-act-s", yesterday_0700_ms, (0, "")));
        running.live.insert("act-s".to_string());
        assert_eq!(
            running.evaluate(),
            vec![Decision::Defer { action_id: "act-s".to_string() }],
            "a live run blocks a second launch of the same action"
        );

        // Disabled and Unsupported triggers are inert — not even a Reseed.
        let mut inert = Snapshot::new(now.clone());
        inert.actions = vec![
            daily_action("act-off", 420, false),
            TriggeredAction {
                action_id: "act-u".to_string(),
                trigger: ParsedTrigger {
                    device_id: "dev-1".to_string(),
                    enabled: true,
                    kind: TriggerKind::Unsupported,
                },
                fingerprint: "fp-u".to_string(),
            },
        ];
        assert_eq!(inert.evaluate(), vec![], "disabled/unsupported triggers decide nothing");
    }

    /// A state written by a schedule trigger lacks event stamps — flipping
    /// the trigger kind moves the fingerprint, but a hole from an older
    /// writer must self-heal via Reseed, never fire blind.
    #[test]
    fn missing_stamps_reseed_instead_of_firing() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let mut holes = Snapshot::new(now);
        holes.actions = vec![event_action("act-e")];
        holes.states.insert(
            "act-e".to_string(),
            AutomationState {
                fingerprint: "fp-act-e".to_string(),
                last_fired_at: Some(0),
                watermark_created_at: None,
                watermark_id: None,
                cooldown_until: None,
            },
        );
        holes.events = vec![event_row("evt-1", "created", now_ms - 1_000)];
        match &holes.evaluate()[..] {
            [Decision::Reseed { new_state, .. }] => {
                assert_eq!(new_state.watermark_id.as_deref(), Some("evt-1"));
            }
            other => panic!("expected Reseed, got {other:?}"),
        }
    }
}
