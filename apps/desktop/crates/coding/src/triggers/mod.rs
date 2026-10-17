//! Agent triggers engine (EXP-530; EXP-1292 Threads v2: the triggers live
//! on their team AGENT, in the synced `sessions.triggers` array of a
//! `kind = agent` root): local-only schedules + event triggers. PURE — hosts
//! (the desktop GUI's trigger host, the CLI daemon's worker) snapshot their
//! inputs (the triggers bound to THIS device, persisted [`TriggerState`]s,
//! pre-fetched event rows, busy agent ids, the clock) and [`evaluate`]
//! returns [`Decision`]s; every side effect (state IO, the post) is the
//! host's.
//!
//! The runner is the agent row itself: [`bound_agent_triggers`] keeps the
//! enabled triggers of every live agent root whose `device_id` is THIS
//! device, BEFORE the engine sees one — everything here is per-trigger
//! bookkeeping, keyed by the trigger's id.
//!
//! Firing protocol (both hosts, in order): persist `new_state` via
//! [`write_states`] FIRST (the cursor/anchor advances at fire time — a
//! crash mid-post drops the firing rather than double-posting it) → post
//! [`firing_text`] into the agent's channel (`sessions.send({sessionId:
//! agentId, text})`, `api::session_device::post_message`). An agent never
//! ends: the message wakes it (or queues behind its current turn). A FAILED
//! post gets one more state write extending `cooldown_until` by
//! [`SEND_FAILURE_BACKOFF_MS`] — a failing trigger backs off instead of
//! hot-looping.
//!
//! The event cursor is NOT a bare high-water mark (EXP-562). Two issue
//! events whose transactions overlap can commit out of `created_at` order
//! and sync in that same wrong order, which strands the earlier row
//! permanently below a monotonic watermark. Instead a fire persists a
//! `seen_floor` trailing the snapshot's newest row by [`EVENT_GRACE_MS`]
//! plus the matching row ids at or above it ([`seen_window`], capped at
//! [`SEEN_IDS_CAP`]): the grace window is re-read every beat and the ids
//! dedupe it, so a late row fires exactly once. The floor is MONOTONIC —
//! the snapshot max can move DOWN when an issue deletion cascades its
//! events away, and a receding floor would re-admit fired-but-pruned rows.
//! `(watermark_created_at, watermark_id)` is still written for older
//! builds, and states written BEFORE this (`seen_floor: None`) keep
//! reading strictly off it until their first fire migrates them.

pub mod events;
pub mod firing;
pub mod schedule;
pub mod state;
pub mod summary;
pub mod trigger;

use std::collections::{HashMap, HashSet};

use domain::contract::SESSION_STATUS_ENDED;
use domain::rows::SessionRow;

pub use events::{
    event_matches, matching_events, seen_window, EventCursor, EventRow, EVENT_GRACE_MS,
    SEEN_IDS_CAP,
};
pub use firing::{firing_issue_ids, firing_text, EventLookups, FIRING_MAX_LINES};
pub use schedule::{latest_occurrence, next_occurrence};
pub use state::{read_states, write_states, TriggerState, TriggerStore};
pub use summary::{schedule_phrase, trigger_summary};
pub use trigger::{
    parse_agent_trigger, parse_agent_triggers, parse_trigger, parse_trigger_str,
    trigger_fingerprint, when_part, AgentTrigger, EventKind, EventSpec, ParsedTrigger, Schedule,
    ScheduleInterval, TriggerKind,
};

/// Backoff a failed firing post stamps onto `cooldown_until` (a trigger
/// whose agent the server refuses must not re-post every beat).
pub const SEND_FAILURE_BACKOFF_MS: i64 = 5 * 60_000;

/// One trigger bound to THIS device, pre-parsed by [`bound_agent_triggers`],
/// which has already dropped every other device's agent, every ended or
/// child row and every switched-off trigger.
#[derive(Clone, Debug)]
pub struct ArmedTrigger {
    /// The trigger's id — the state map's key.
    pub trigger_id: String,
    /// The agent root (`sessions.id`) this fires into — the post target and
    /// the busy-defer key.
    pub agent_id: String,
    /// The agent's team — it fences event matching against the shared
    /// events snapshot (hosts sync every member team's rows into one place).
    pub team_id: String,
    pub trigger: ParsedTrigger,
    /// [`AgentTrigger::fingerprint`] — the when-part alone.
    pub fingerprint: String,
}

/// The triggers `device_id` evaluates, over every synced `sessions` row:
/// agent ROOTS bound to this device, not ended, with a team, and ENABLED
/// triggers (≤ `AGENT_TRIGGER_MAX_PER_AGENT` each — the server's cap,
/// re-applied so a stale row can never arm more). The ONE resolution both
/// hosts use, so a GUI↔CLI hand-off reads the same fingerprints and never
/// re-seeds the shared state.
pub fn bound_agent_triggers<'a>(
    rows: impl Iterator<Item = &'a SessionRow>,
    device_id: &str,
) -> Vec<ArmedTrigger> {
    let mut bound = Vec::new();
    for row in rows {
        if !row.has_triggers()
            || !row.is_root()
            || !row.on_device(device_id)
            || row.status_is(SESSION_STATUS_ENDED)
        {
            continue; // a thread, a child, another machine's agent, or gone
        }
        // A team-less row has nowhere to post — and the team doubles as the
        // engine's event fence.
        let Some(team_id) = row.team_id.as_deref() else {
            continue;
        };
        for trigger in parse_agent_triggers(row.triggers.as_ref())
            .into_iter()
            .take(domain::contract::AGENT_TRIGGER_MAX_PER_AGENT)
        {
            if !trigger.enabled {
                continue;
            }
            bound.push(ArmedTrigger {
                fingerprint: trigger.fingerprint(),
                trigger_id: trigger.id,
                agent_id: row.id.clone(),
                team_id: team_id.to_string(),
                trigger: trigger.when,
            });
        }
    }
    bound
}

/// One evaluation pass's snapshot.
pub struct EvalInput<'a, Tz: chrono::TimeZone> {
    pub armed: &'a [ArmedTrigger],
    /// This device's persisted states ([`read_states`]).
    pub states: &'a HashMap<String, TriggerState>,
    /// Candidate event rows (host-fetched, ≤ the catch-up window is fine —
    /// the engine re-applies the cutoff).
    pub events: &'a [EventRow],
    /// Agents that must not get another firing yet — deferred, never
    /// piled up: the agent row is `busy` (mid-turn) OR a firing it was sent
    /// is still unconsumed (`messages.consumed_at` NULL). Keyed by AGENT id:
    /// two triggers on one agent must not both post while it works.
    pub busy_agent_ids: &'a HashSet<String>,
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
    /// Persist `new_state` FIRST, then post [`firing_text`] into the agent
    /// (see the firing protocol above).
    Fire {
        trigger_id: String,
        firing: Firing,
        new_state: TriggerState,
    },
    /// First observation of this trigger (or an edited one): persist the
    /// seeded state, fire NOTHING — a trigger never fires
    /// retroactively on creation or edit.
    Reseed {
        trigger_id: String,
        new_state: TriggerState,
    },
    /// Cooling down or the agent is busy — re-evaluate next beat.
    Defer { trigger_id: String },
}

/// Whether this pass needs an events snapshot at all: only an event trigger
/// reads it (the host already dropped the disabled rows; schedules never
/// look). Hosts skip the issue_events scan when this is false.
pub fn needs_events(armed: &[ArmedTrigger]) -> bool {
    armed
        .iter()
        .any(|armed| matches!(armed.trigger.kind, TriggerKind::Event(_)))
}

/// The per-trigger decision cascade: (1) unsupported → inert;
/// (2) unseen or edited trigger → [`Decision::Reseed`]; (3) cooldown →
/// defer; (4) busy agent → defer; (5) schedule fires on a new latest
/// occurrence (offline catch-up = exactly one run, anchored at the
/// occurrence — never `now` — so the anchor can't drift); (6) event fires
/// ONCE over all matches the cursor admits, then advances the cursor to
/// `(snapshot max − [`EVENT_GRACE_MS`], the matching ids inside that
/// window)` — see the firing protocol above for why the grace exists.
pub fn evaluate<Tz: chrono::TimeZone>(input: &EvalInput<Tz>) -> Vec<Decision> {
    let snapshot_watermark = input
        .events
        .iter()
        .map(|row| (row.created_at_ms, row.id.as_str()))
        .max()
        .unwrap_or((input.now_ms, ""));
    let mut decisions = Vec::new();
    for armed in input.armed {
        if armed.trigger.kind == TriggerKind::Unsupported {
            continue;
        }
        let state = input.states.get(&armed.trigger_id);
        // (2) Reseed: no state, an edited trigger, or a state missing the
        // stamps this kind needs (a schedule↔event edit keeps the
        // fingerprint moving, but an older writer could leave holes —
        // self-heal conservatively instead of firing blind).
        let needs_reseed = match state {
            None => true,
            Some(existing) if existing.fingerprint != armed.fingerprint => true,
            Some(existing) => match &armed.trigger.kind {
                TriggerKind::Schedule(_) => existing.last_fired_at.is_none(),
                TriggerKind::Event(_) => {
                    existing.watermark_created_at.is_none() || existing.watermark_id.is_none()
                }
                TriggerKind::Unsupported => false,
            },
        };
        if needs_reseed {
            // A reseed is NEVER retroactive, so it takes no grace: the
            // floor starts AT the snapshot max (raised monotonically over
            // any surviving floor), which leaves nothing in the snapshot
            // eligible to fire.
            let (seen_floor, seen_ids) = match &armed.trigger.kind {
                TriggerKind::Event(spec) => {
                    let base = state
                        .and_then(|existing| existing.seen_floor)
                        .unwrap_or(i64::MIN)
                        .max(snapshot_watermark.0);
                    let (floor, ids) =
                        seen_window(spec, &armed.team_id, input.events, base);
                    (Some(floor), ids)
                }
                TriggerKind::Schedule(_) | TriggerKind::Unsupported => (None, Vec::new()),
            };
            decisions.push(Decision::Reseed {
                trigger_id: armed.trigger_id.clone(),
                new_state: TriggerState {
                    fingerprint: armed.fingerprint.clone(),
                    last_fired_at: Some(input.now_ms),
                    watermark_created_at: Some(snapshot_watermark.0),
                    watermark_id: Some(snapshot_watermark.1.to_string()),
                    cooldown_until: None,
                    seen_floor,
                    seen_ids,
                },
            });
            continue;
        }
        let state = state.expect("reseed handled the missing-state arm");
        // (3) + (4): both defer — the trigger re-evaluates next beat.
        let cooling = state
            .cooldown_until
            .is_some_and(|until| input.now_ms < until);
        if cooling || input.busy_agent_ids.contains(&armed.agent_id) {
            decisions.push(Decision::Defer {
                trigger_id: armed.trigger_id.clone(),
            });
            continue;
        }
        match &armed.trigger.kind {
            TriggerKind::Schedule(shape) => {
                let Some(occurrence) = latest_occurrence(shape, input.now_local.clone()) else {
                    continue;
                };
                let occurrence_ms = occurrence.timestamp_millis();
                if occurrence_ms > state.last_fired_at.unwrap_or(input.now_ms) {
                    decisions.push(Decision::Fire {
                        trigger_id: armed.trigger_id.clone(),
                        firing: Firing::Schedule { occurrence_ms },
                        new_state: TriggerState {
                            fingerprint: state.fingerprint.clone(),
                            // Anchor AT the occurrence, not now — firing at
                            // 07:03 must not drift tomorrow's compare.
                            last_fired_at: Some(occurrence_ms),
                            watermark_created_at: state.watermark_created_at,
                            watermark_id: state.watermark_id.clone(),
                            cooldown_until: Some(
                                input.now_ms + domain::contract::TRIGGER_COOLDOWN_MS,
                            ),
                            // Carried through untouched, like the watermark
                            // — a schedule never consumes events.
                            seen_floor: state.seen_floor,
                            seen_ids: state.seen_ids.clone(),
                        },
                    });
                }
            }
            TriggerKind::Event(spec) => {
                let cursor = match state.seen_floor {
                    Some(floor) => EventCursor::Seen { floor, seen_ids: &state.seen_ids },
                    // Pre-EXP-562 state: stay strict until this fire
                    // migrates it, so an upgrade re-fires nothing.
                    None => EventCursor::Strict {
                        watermark: (
                            state.watermark_created_at.expect("reseed guaranteed the stamp"),
                            state.watermark_id.as_deref().expect("reseed guaranteed the id"),
                        ),
                    },
                };
                let matches =
                    matching_events(spec, &armed.team_id, input.events, cursor, input.now_ms);
                if matches.is_empty() {
                    continue;
                }
                // MONOTONIC: the snapshot max can recede (an issue delete
                // cascades its events away), and a receding floor would
                // re-admit rows already fired on and since pruned.
                let base = state
                    .seen_floor
                    .unwrap_or(i64::MIN)
                    .max(snapshot_watermark.0 - EVENT_GRACE_MS);
                let (seen_floor, seen_ids) =
                    seen_window(spec, &armed.team_id, input.events, base);
                decisions.push(Decision::Fire {
                    trigger_id: armed.trigger_id.clone(),
                    firing: Firing::Event {
                        matches: matches.into_iter().cloned().collect(),
                    },
                    new_state: TriggerState {
                        fingerprint: state.fingerprint.clone(),
                        last_fired_at: Some(input.now_ms),
                        // Still advanced over the WHOLE snapshot for older
                        // builds reading this state; the grace cursor below
                        // is what THIS build fires off.
                        watermark_created_at: Some(snapshot_watermark.0),
                        watermark_id: Some(snapshot_watermark.1.to_string()),
                        cooldown_until: Some(
                            input.now_ms + domain::contract::TRIGGER_COOLDOWN_MS,
                        ),
                        seen_floor: Some(seen_floor),
                        seen_ids,
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

    /// The ids are deliberately the same here: the state map keys on the
    /// TRIGGER and the busy set keys on the AGENT, and one id makes both
    /// readable in the asserts below.
    fn daily_trigger(id: &str, minute: u32) -> ArmedTrigger {
        ArmedTrigger {
            trigger_id: id.to_string(),
            agent_id: id.to_string(),
            team_id: "team-1".to_string(),
            trigger: ParsedTrigger {
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

    fn event_trigger(id: &str) -> ArmedTrigger {
        ArmedTrigger {
            trigger_id: id.to_string(),
            agent_id: id.to_string(),
            team_id: "team-1".to_string(),
            trigger: ParsedTrigger {
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
            team_id: Some("team-1".to_string()),
            created_at_ms,
            kind: kind.to_string(),
            payload: None,
            board_id: Some("board-1".to_string()),
        }
    }

    /// A LEGACY (pre-EXP-562) state: watermark only, no grace cursor.
    fn seeded(fingerprint: &str, fired_ms: i64, watermark: (i64, &str)) -> TriggerState {
        TriggerState {
            fingerprint: fingerprint.to_string(),
            last_fired_at: Some(fired_ms),
            watermark_created_at: Some(watermark.0),
            watermark_id: Some(watermark.1.to_string()),
            cooldown_until: None,
            seen_floor: None,
            seen_ids: Vec::new(),
        }
    }

    /// A current state: the grace cursor drives it, the watermark rides
    /// along for older builds.
    fn seeded_seen(
        fingerprint: &str,
        fired_ms: i64,
        watermark: (i64, &str),
        floor: i64,
        seen_ids: &[&str],
    ) -> TriggerState {
        TriggerState {
            seen_floor: Some(floor),
            seen_ids: seen_ids.iter().map(|id| id.to_string()).collect(),
            ..seeded(fingerprint, fired_ms, watermark)
        }
    }

    struct Snapshot {
        armed: Vec<ArmedTrigger>,
        states: HashMap<String, TriggerState>,
        events: Vec<EventRow>,
        busy: HashSet<String>,
        now_local: DateTime<FixedOffset>,
    }

    impl Snapshot {
        fn new(now_local: DateTime<FixedOffset>) -> Self {
            Self {
                armed: Vec::new(),
                states: HashMap::new(),
                events: Vec::new(),
                busy: HashSet::new(),
                now_local,
            }
        }

        fn evaluate(&self) -> Vec<Decision> {
            evaluate(&EvalInput {
                armed: &self.armed,
                states: &self.states,
                events: &self.events,
                busy_agent_ids: &self.busy,
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
        fresh.armed = vec![event_trigger("agt-e")];
        fresh.events = vec![
            event_row("evt-1", "created", now_ms - 5_000),
            event_row("evt-2", "created", now_ms - 1_000),
        ];
        assert_eq!(
            fresh.evaluate(),
            vec![Decision::Reseed {
                trigger_id: "agt-e".to_string(),
                // No grace on a reseed: the floor sits AT the snapshot max
                // and evt-2 (the only row there) is already marked seen.
                new_state: seeded_seen(
                    "fp-agt-e",
                    now_ms,
                    (now_ms - 1_000, "evt-2"),
                    now_ms - 1_000,
                    &["evt-2"],
                ),
            }],
            "first observation seeds the cursor, never fires retroactively"
        );

        // An edited trigger reseeds (fingerprint moved), even mid-steady-state.
        let mut edited = Snapshot::new(now.clone());
        edited.armed = vec![daily_trigger("agt-s", 420)];
        edited
            .states
            .insert("agt-s".to_string(), seeded("fp-STALE", yesterday_0700_ms, (0, "")));
        match &edited.evaluate()[..] {
            [Decision::Reseed { trigger_id, new_state }] => {
                assert_eq!(trigger_id, "agt-s", "edited trigger reseeds instead of firing");
                assert_eq!(new_state.fingerprint, "fp-agt-s");
                assert_eq!(new_state.last_fired_at, Some(now_ms));
            }
            other => panic!("expected one Reseed, got {other:?}"),
        }

        // Schedule offline catch-up: 3 missed days collapse into EXACTLY
        // one fire — the latest occurrence.
        let mut catchup = Snapshot::new(now.clone());
        catchup.armed = vec![daily_trigger("agt-s", 420)];
        let three_days_ago = at(2026, 8, 15, 7, 0).timestamp_millis();
        catchup
            .states
            .insert("agt-s".to_string(), seeded("fp-agt-s", three_days_ago, (0, "")));
        match &catchup.evaluate()[..] {
            [Decision::Fire { trigger_id, firing, new_state }] => {
                assert_eq!(trigger_id, "agt-s");
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
                    Some(now_ms + domain::contract::TRIGGER_COOLDOWN_MS)
                );
                assert_eq!(new_state.watermark_created_at, Some(0), "schedule keeps the watermark");
            }
            other => panic!("expected one Fire, got {other:?}"),
        }

        // Already anchored at today's occurrence → nothing (no Defer spam).
        let mut steady = Snapshot::new(now.clone());
        steady.armed = vec![daily_trigger("agt-s", 420)];
        steady
            .states
            .insert("agt-s".to_string(), seeded("fp-agt-s", today_0700_ms, (0, "")));
        assert_eq!(steady.evaluate(), vec![], "an anchored schedule is silent until the next tick");

        // Event coalescing: 3 matches → ONE Fire carrying all of them; the
        // watermark advances over a NEWER NON-MATCHING row too.
        let mut coalesce = Snapshot::new(now.clone());
        coalesce.armed = vec![event_trigger("agt-e")];
        coalesce
            .states
            .insert("agt-e".to_string(), seeded("fp-agt-e", 0, (now_ms - 10_000, "evt-0")));
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
                assert_eq!(
                    new_state.seen_floor,
                    Some(now_ms - 1_000 - EVENT_GRACE_MS),
                    "the grace cursor trails the snapshot max"
                );
                assert_eq!(
                    new_state.seen_ids,
                    vec!["evt-3".to_string(), "evt-2".to_string(), "evt-1".to_string()],
                    "only MATCHING rows are remembered — evt-4 is not this spec's"
                );
                assert_eq!(new_state.last_fired_at, Some(now_ms));
            }
            other => panic!("expected one event Fire, got {other:?}"),
        }

        // Another member team's events never fire this team's trigger — but
        // the watermark (max over the WHOLE snapshot) still advances past
        // them on the next real fire, so no Fire and no Reseed here.
        let mut cross_team = Snapshot::new(now.clone());
        cross_team.armed = vec![event_trigger("agt-e")];
        cross_team
            .states
            .insert("agt-e".to_string(), seeded("fp-agt-e", 0, (0, "")));
        let mut foreign = event_row("evt-f", "created", now_ms - 1_000);
        foreign.team_id = Some("team-OTHER".to_string());
        cross_team.events = vec![foreign];
        assert_eq!(
            cross_team.evaluate(),
            vec![],
            "cross-team events are fenced out of matching"
        );

        // Cooldown defers.
        let mut cooling = Snapshot::new(now.clone());
        cooling.armed = vec![event_trigger("agt-e")];
        let mut cooled = seeded("fp-agt-e", 0, (0, ""));
        cooled.cooldown_until = Some(now_ms + 30_000);
        cooling.states.insert("agt-e".to_string(), cooled);
        cooling.events = vec![event_row("evt-1", "created", now_ms - 1_000)];
        assert_eq!(
            cooling.evaluate(),
            vec![Decision::Defer { trigger_id: "agt-e".to_string() }],
            "a cooling trigger defers, keeping the pending events for the next beat"
        );

        // A busy agent (mid-turn, or an unconsumed firing) defers.
        let mut running = Snapshot::new(now.clone());
        running.armed = vec![daily_trigger("agt-s", 420)];
        running
            .states
            .insert("agt-s".to_string(), seeded("fp-agt-s", yesterday_0700_ms, (0, "")));
        running.busy.insert("agt-s".to_string());
        assert_eq!(
            running.evaluate(),
            vec![Decision::Defer { trigger_id: "agt-s".to_string() }],
            "a busy agent gets no second firing piled on"
        );

        // An Unsupported trigger is inert — not even a Reseed. (A DISABLED
        // trigger never reaches the engine at all: bound_agent_triggers
        // filters `enabled` off the row before snapshotting.)
        let mut inert = Snapshot::new(now.clone());
        inert.armed = vec![ArmedTrigger {
            trigger_id: "agt-u".to_string(),
            agent_id: "agt-u".to_string(),
            team_id: "team-1".to_string(),
            trigger: ParsedTrigger {
                kind: TriggerKind::Unsupported,
            },
            fingerprint: "fp-u".to_string(),
        }];
        assert_eq!(inert.evaluate(), vec![], "an unsupported trigger decides nothing");
    }

    /// A state written by a schedule trigger lacks event stamps — flipping
    /// the trigger kind moves the fingerprint, but a hole from an older
    /// writer must self-heal via Reseed, never fire blind.
    #[test]
    fn missing_stamps_reseed_instead_of_firing() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let mut holes = Snapshot::new(now);
        holes.armed = vec![event_trigger("agt-e")];
        holes.states.insert(
            "agt-e".to_string(),
            TriggerState {
                fingerprint: "fp-agt-e".to_string(),
                last_fired_at: Some(0),
                watermark_created_at: None,
                watermark_id: None,
                cooldown_until: None,
                seen_floor: None,
                seen_ids: Vec::new(),
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

    /// Only THIS device's live agent roots' ENABLED, readable triggers reach
    /// the engine — everything else belongs to another host (or to none).
    #[test]
    fn bound_agent_triggers_keep_only_this_devices_live_agents() {
        use serde_json::json;
        let row = |id: &str, extra: serde_json::Value| {
            let mut row = json!({"id": id, "kind": "agent", "team_id": "team-1",
                                 "device_id": "dev-gui", "status": "idle"});
            for (key, value) in extra.as_object().unwrap() {
                row[key.as_str()] = value.clone();
            }
            serde_json::from_value::<SessionRow>(row).expect("session row decodes")
        };
        let daily = |id: &str, enabled: bool| {
            json!({"id": id, "enabled": enabled,
                   "kind": "schedule", "interval": "daily", "minuteOfDay": 420})
        };
        let rows = vec![
            row(
                "agt-1",
                json!({"triggers": [
                    daily("mine", true),
                    daily("off", false),
                    // A FUTURE kind is unreadable: skipped, never evaluated.
                    {"id": "future", "kind": "moon_phase"},
                    {"id": "event", "source": "exponential", "kind": "event", "event": "created"},
                ]}),
            ),
            // Another machine's agent.
            row("agt-2", json!({"device_id": "dev-cli", "triggers": [daily("theirs", true)]})),
            // An ended agent, a thread root, a child row: never armed.
            row("agt-3", json!({"status": "ended", "triggers": [daily("ended", true)]})),
            row("thr-1", json!({"kind": "thread", "triggers": [daily("thread", true)]})),
            row("chd-1", json!({"parent_id": "agt-1", "triggers": [daily("child", true)]})),
            // A team-less agent has nowhere to post.
            row("agt-4", json!({"team_id": null, "triggers": [daily("teamless", true)]})),
            // TEXT-stored jsonb (the native stores' form) reads the same;
            // a running agent (mid-turn) still arms — the busy set defers it.
            row(
                "agt-5",
                json!({"team_id": "team-2", "status": "running",
                       "triggers": json!([daily("text", true)]).to_string()}),
            ),
            row("agt-6", json!({"triggers": null})),
        ];
        let bound = bound_agent_triggers(rows.iter(), "dev-gui");
        assert_eq!(
            bound.iter().map(|armed| armed.trigger_id.as_str()).collect::<Vec<_>>(),
            vec!["mine", "event", "text"]
        );
        assert_eq!(bound[0].agent_id, "agt-1", "the post target rides along");
        assert_eq!(bound[0].team_id, "team-1");
        assert_eq!(bound[2].agent_id, "agt-5");
        assert_eq!(bound[2].team_id, "team-2");
        // The fingerprint is the WHEN-part's, whatever `id`/`enabled` say.
        assert_eq!(
            bound[0].fingerprint,
            trigger_fingerprint(&json!({
                "kind": "schedule", "interval": "daily", "minuteOfDay": 420
            }))
        );

        // The per-agent cap holds even on a stale over-long array.
        let many: Vec<serde_json::Value> = (0..domain::contract::AGENT_TRIGGER_MAX_PER_AGENT + 3)
            .map(|index| daily(&format!("t-{index}"), true))
            .collect();
        let crowded = row("agt-7", json!({"triggers": many}));
        assert_eq!(
            bound_agent_triggers(std::iter::once(&crowded), "dev-gui").len(),
            domain::contract::AGENT_TRIGGER_MAX_PER_AGENT
        );
    }

    #[test]
    fn needs_events_only_for_event_triggers() {
        assert!(!needs_events(&[]), "no triggers, no scan");
        assert!(
            !needs_events(&[daily_trigger("agt-s", 420)]),
            "a schedule never reads the events snapshot"
        );
        let unsupported = ArmedTrigger {
            trigger_id: "agt-u".to_string(),
            agent_id: "agt-u".to_string(),
            team_id: "team-1".to_string(),
            trigger: ParsedTrigger {
                kind: TriggerKind::Unsupported,
            },
            fingerprint: "fp-u".to_string(),
        };
        assert!(!needs_events(&[unsupported]));
        assert!(needs_events(&[daily_trigger("agt-s", 420), event_trigger("agt-e")]));
    }

    /// EXP-562 the whole point: a row that COMMITTED before the snapshot's
    /// newest one but SYNCED after it (overlapping transactions) still
    /// fires — exactly once.
    #[test]
    fn late_row_inside_grace_fires_once() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let cooldown = chrono::Duration::milliseconds(domain::contract::TRIGGER_COOLDOWN_MS + 1);
        let mut first = Snapshot::new(now.clone());
        first.armed = vec![event_trigger("agt-e")];
        first.states.insert(
            "agt-e".to_string(),
            seeded_seen("fp-agt-e", 0, (now_ms - 10_000, "evt-0"), now_ms - 20_000, &["evt-0"]),
        );
        first.events = vec![event_row("evt-2", "created", now_ms - 5_000)];
        let fired_state = match &first.evaluate()[..] {
            [Decision::Fire { firing: Firing::Event { matches }, new_state, .. }] => {
                assert_eq!(matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["evt-2"]);
                new_state.clone()
            }
            other => panic!("expected one Fire, got {other:?}"),
        };

        // evt-1 lands late: BELOW the watermark evt-2 just set, but inside
        // the grace. A bare high-water mark would have lost it forever.
        let later = now + cooldown;
        let mut second = Snapshot::new(later.clone());
        second.armed = vec![event_trigger("agt-e")];
        second.states.insert("agt-e".to_string(), fired_state);
        second.events = vec![
            event_row("evt-1", "created", now_ms - 6_000),
            event_row("evt-2", "created", now_ms - 5_000),
        ];
        let second_state = match &second.evaluate()[..] {
            [Decision::Fire { firing: Firing::Event { matches }, new_state, .. }] => {
                assert_eq!(
                    matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
                    vec!["evt-1"],
                    "the late row fires; the already-seen evt-2 does not re-fire"
                );
                new_state.clone()
            }
            other => panic!("expected one Fire, got {other:?}"),
        };

        // Third pass over the SAME snapshot: nothing left to fire.
        let mut third = Snapshot::new(later + cooldown);
        third.armed = vec![event_trigger("agt-e")];
        third.states.insert("agt-e".to_string(), second_state);
        third.events = second.events.clone();
        assert_eq!(third.evaluate(), vec![], "a fired row is never re-litigated");
    }

    /// The grace is bounded: a row older than the floor is gone for good
    /// (the alternative is an unbounded seen set).
    #[test]
    fn late_row_below_floor_is_skipped() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let floor = now_ms - 10_000;
        let mut snapshot = Snapshot::new(now);
        snapshot.armed = vec![event_trigger("agt-e")];
        snapshot.states.insert(
            "agt-e".to_string(),
            seeded_seen("fp-agt-e", 0, (now_ms - 5_000, "evt-2"), floor, &["evt-2"]),
        );
        snapshot.events = vec![
            event_row("evt-ancient", "created", floor - 1),
            event_row("evt-2", "created", now_ms - 5_000),
        ];
        assert_eq!(snapshot.evaluate(), vec![], "below the floor stays below the floor");
    }

    /// An upgrade from a pre-EXP-562 state must not replay: the strict
    /// watermark still fences, and the first real fire migrates the state.
    #[test]
    fn legacy_state_without_seen_floor_stays_strict_and_migrates_on_fire() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let mut quiet = Snapshot::new(now);
        quiet.armed = vec![event_trigger("agt-e")];
        quiet
            .states
            .insert("agt-e".to_string(), seeded("fp-agt-e", 0, (now_ms - 5_000, "evt-b")));
        // Same ms, LOWER id — strictly below the watermark tuple. Under the
        // grace cursor it would fire; a legacy state must not replay it.
        quiet.events = vec![event_row("evt-a", "created", now_ms - 5_000)];
        assert_eq!(quiet.evaluate(), vec![], "a legacy state keeps its strict tuple fence");

        let mut fires = quiet;
        fires.events.push(event_row("evt-c", "created", now_ms - 1_000));
        match &fires.evaluate()[..] {
            [Decision::Fire { firing: Firing::Event { matches }, new_state, .. }] => {
                assert_eq!(matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["evt-c"]);
                assert!(
                    new_state.seen_floor.is_some(),
                    "the first fire migrates the state onto the grace cursor"
                );
                assert!(new_state.seen_ids.contains(&"evt-c".to_string()));
            }
            other => panic!("expected one Fire, got {other:?}"),
        }
    }

    /// The seen set is capped; overflow raises the floor instead of growing
    /// settings.json, and the narrowed window still never re-fires.
    #[test]
    fn seen_cap_truncation_raises_floor() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let mut burst = Snapshot::new(now.clone());
        burst.armed = vec![event_trigger("agt-e")];
        burst
            .states
            .insert("agt-e".to_string(), seeded_seen("fp-agt-e", 0, (0, ""), now_ms - 30_000, &[]));
        burst.events = (0..130)
            .map(|index| event_row(&format!("evt-{index:03}"), "created", now_ms - 20_000 + index))
            .collect();
        let fired_state = match &burst.evaluate()[..] {
            [Decision::Fire { firing: Firing::Event { matches }, new_state, .. }] => {
                assert_eq!(matches.len(), 130, "one coalesced fire carries the whole burst");
                assert!(
                    new_state.seen_ids.len() <= SEEN_IDS_CAP,
                    "the seen set is capped at {SEEN_IDS_CAP}, got {}",
                    new_state.seen_ids.len()
                );
                new_state.clone()
            }
            other => panic!("expected one Fire, got {other:?}"),
        };

        let cooldown = chrono::Duration::milliseconds(domain::contract::TRIGGER_COOLDOWN_MS + 1);
        let mut again = Snapshot::new(now + cooldown);
        again.armed = vec![event_trigger("agt-e")];
        again.states.insert("agt-e".to_string(), fired_state);
        again.events = burst.events.clone();
        assert_eq!(again.evaluate(), vec![], "the truncated rows fell BELOW the raised floor");
    }

    #[test]
    fn reseed_on_empty_snapshot_seeds_floor_at_now() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let mut empty = Snapshot::new(now);
        empty.armed = vec![event_trigger("agt-e")];
        match &empty.evaluate()[..] {
            [Decision::Reseed { new_state, .. }] => {
                assert_eq!(
                    new_state.seen_floor,
                    Some(now_ms),
                    "no events → the cursor anchors at now, not at i64::MIN"
                );
                assert!(new_state.seen_ids.is_empty());
            }
            other => panic!("expected Reseed, got {other:?}"),
        }
    }

    /// An issue deletion cascades its events away, so the snapshot max can
    /// RECEDE. The floor must not follow it down — that would re-admit rows
    /// this device already fired on and has since forgotten.
    #[test]
    fn floor_never_moves_backwards() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let floor = now_ms - 10_000;
        let mut shrunk = Snapshot::new(now);
        shrunk.armed = vec![event_trigger("agt-e")];
        shrunk.states.insert(
            "agt-e".to_string(),
            seeded_seen("fp-agt-e", 0, (now_ms, "evt-gone"), floor, &[]),
        );
        // The only survivor is far below `floor + EVENT_GRACE_MS`, so the
        // naive candidate floor (max − grace) sits below the persisted one.
        shrunk.events = vec![event_row("evt-1", "created", now_ms - 5_000)];
        assert!(
            now_ms - 5_000 - EVENT_GRACE_MS < floor,
            "fixture sanity: the candidate floor really is lower"
        );
        match &shrunk.evaluate()[..] {
            [Decision::Fire { new_state, .. }] => {
                assert_eq!(new_state.seen_floor, Some(floor), "the floor only ever rises");
            }
            other => panic!("expected one Fire, got {other:?}"),
        }
    }
}
