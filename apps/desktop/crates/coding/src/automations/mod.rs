//! Action automations engine (EXP-530; EXP-583 moved the rows into their own
//! `automations` shape): local-only schedules + event triggers. PURE — hosts
//! (the desktop GUI's automation host, the CLI daemon's worker) snapshot
//! their inputs (the automations bound to THIS device, persisted
//! [`AutomationState`]s, pre-fetched event rows, live-run ids, the clock) and
//! [`evaluate`] returns [`Decision`]s; every side effect (settings IO, the
//! launch) is the host's.
//!
//! The device binding and the on/off flag are COLUMNS of the automations row,
//! so the hosts filter (`device_id == mine && enabled`) BEFORE the engine
//! sees a row — everything here is per-automation bookkeeping, keyed by the
//! automation's id. The launch reservation still keys on the ACTION
//! (`action:{id}`), because that is what a remote start holds.
//!
//! Firing protocol (both hosts, in order): claim the `action:{id}`
//! reservation → persist `new_state` via [`write_states`] FIRST (the
//! cursor/anchor advances at launch START — a crash mid-launch drops the
//! run rather than double-firing) → prepare/launch. A FAILED prepare gets
//! one more state write extending `cooldown_until` by
//! [`PREPARE_FAILURE_BACKOFF_MS`] — a poison-pill trigger backs off
//! instead of hot-looping.
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
pub mod schedule;
pub mod state;
pub mod summary;
pub mod trigger;

use std::collections::{HashMap, HashSet};

pub use events::{
    event_matches, matching_events, seen_window, EventCursor, EventRow, EVENT_GRACE_MS,
    SEEN_IDS_CAP,
};
pub use schedule::{latest_occurrence, next_occurrence};
pub use state::{read_states, write_states, AutomationState, AUTOMATIONS_KEY};
pub use summary::{schedule_phrase, trigger_summary};
pub use trigger::{
    parse_trigger, parse_trigger_str, trigger_fingerprint, EventKind, EventSpec, ParsedTrigger,
    Schedule, ScheduleInterval, TriggerKind,
};

/// The launch options an automation runs with — the ONE resolution both
/// hosts use (EXP-583). An automation may PIN an agent/model/effort; every
/// unpinned field falls back to the device's own launch defaults, exactly
/// like a dialog start with untouched options. Plan mode is forced OFF (F7):
/// an unattended run must never park at the plan-approval card waiting for a
/// human who is not watching.
pub fn launch_options(
    settings: &crate::Settings,
    agent: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> crate::LaunchOptions {
    // An agent this build does not know (a newer contract value) falls back
    // to the device default rather than refusing to run.
    let agent = agent
        .and_then(crate::CodingAgent::parse)
        .unwrap_or(settings.default_agent);
    let mut options = crate::LaunchOptions::defaults_for(settings, agent);
    if let Some(model) = model.filter(|value| !value.is_empty()) {
        options.model = model.to_string();
    }
    if let Some(effort) = effort.filter(|value| !value.is_empty()) {
        options.effort = effort.to_string();
    }
    options.plan_mode = false;
    options
}

/// Backoff a failed prepare stamps onto `cooldown_until` (poison-pill).
pub const PREPARE_FAILURE_BACKOFF_MS: i64 = 5 * 60_000;
/// Cap on the issue lines an event firing renders into the trigger prompt
/// section — the overflow becomes one "…and N more." line.
pub const TRIGGER_PROMPT_MAX_LINES: usize = 50;

/// One automation bound to THIS device, pre-parsed by the host. The host has
/// already dropped every row that targets another device or is switched off.
#[derive(Clone, Debug)]
pub struct TriggeredAutomation {
    /// The `automations` row id — the state map's key AND the id stamped on
    /// the run it fires.
    pub automation_id: String,
    /// The action this fires (the reservation key and the launch target).
    pub action_id: String,
    /// The automation's team — it fences event matching against the shared
    /// events snapshot (hosts sync every member team's rows into one place).
    pub team_id: String,
    pub trigger: ParsedTrigger,
    /// [`trigger_fingerprint`] of the RAW trigger JSON.
    pub fingerprint: String,
}

/// One evaluation pass's snapshot.
pub struct EvalInput<'a, Tz: chrono::TimeZone> {
    pub automations: &'a [TriggeredAutomation],
    /// This device's persisted states ([`read_states`]).
    pub states: &'a HashMap<String, AutomationState>,
    /// Candidate event rows (host-fetched, ≤ the catch-up window is fine —
    /// the engine re-applies the cutoff).
    pub events: &'a [EventRow],
    /// Actions with a LIVE local run — deferred, never double-launched.
    /// Keyed by ACTION id: two automations on one action must not both
    /// launch it while a run is up.
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
        automation_id: String,
        firing: Firing,
        new_state: AutomationState,
    },
    /// First observation of this trigger (or an edited one): persist the
    /// seeded state, fire NOTHING — an automation never fires
    /// retroactively on creation or edit.
    Reseed {
        automation_id: String,
        new_state: AutomationState,
    },
    /// Cooling down or already running — re-evaluate next beat.
    Defer { automation_id: String },
}

/// Whether this pass needs an events snapshot at all: only an event trigger
/// reads it (the host already dropped the disabled rows; schedules never
/// look). Hosts skip the issue_events scan when this is false.
pub fn needs_events(automations: &[TriggeredAutomation]) -> bool {
    automations
        .iter()
        .any(|automation| matches!(automation.trigger.kind, TriggerKind::Event(_)))
}

/// The per-automation decision cascade: (1) unsupported → inert;
/// (2) unseen or edited trigger → [`Decision::Reseed`]; (3) cooldown →
/// defer; (4) live run → defer; (5) schedule fires on a new latest
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
    for automation in input.automations {
        if automation.trigger.kind == TriggerKind::Unsupported {
            continue;
        }
        let state = input.states.get(&automation.automation_id);
        // (2) Reseed: no state, an edited trigger, or a state missing the
        // stamps this kind needs (a schedule↔event edit keeps the
        // fingerprint moving, but an older writer could leave holes —
        // self-heal conservatively instead of firing blind).
        let needs_reseed = match state {
            None => true,
            Some(existing) if existing.fingerprint != automation.fingerprint => true,
            Some(existing) => match &automation.trigger.kind {
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
            let (seen_floor, seen_ids) = match &automation.trigger.kind {
                TriggerKind::Event(spec) => {
                    let base = state
                        .and_then(|existing| existing.seen_floor)
                        .unwrap_or(i64::MIN)
                        .max(snapshot_watermark.0);
                    let (floor, ids) =
                        seen_window(spec, &automation.team_id, input.events, base);
                    (Some(floor), ids)
                }
                TriggerKind::Schedule(_) | TriggerKind::Unsupported => (None, Vec::new()),
            };
            decisions.push(Decision::Reseed {
                automation_id: automation.automation_id.clone(),
                new_state: AutomationState {
                    fingerprint: automation.fingerprint.clone(),
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
        if cooling || input.live_action_ids.contains(&automation.action_id) {
            decisions.push(Decision::Defer {
                automation_id: automation.automation_id.clone(),
            });
            continue;
        }
        match &automation.trigger.kind {
            TriggerKind::Schedule(shape) => {
                let Some(occurrence) = latest_occurrence(shape, input.now_local.clone()) else {
                    continue;
                };
                let occurrence_ms = occurrence.timestamp_millis();
                if occurrence_ms > state.last_fired_at.unwrap_or(input.now_ms) {
                    decisions.push(Decision::Fire {
                        automation_id: automation.automation_id.clone(),
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
                    matching_events(spec, &automation.team_id, input.events, cursor, input.now_ms);
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
                    seen_window(spec, &automation.team_id, input.events, base);
                decisions.push(Decision::Fire {
                    automation_id: automation.automation_id.clone(),
                    firing: Firing::Event {
                        matches: matches.into_iter().cloned().collect(),
                    },
                    new_state: AutomationState {
                        fingerprint: state.fingerprint.clone(),
                        last_fired_at: Some(input.now_ms),
                        // Still advanced over the WHOLE snapshot for older
                        // builds reading this state; the grace cursor below
                        // is what THIS build fires off.
                        watermark_created_at: Some(snapshot_watermark.0),
                        watermark_id: Some(snapshot_watermark.1.to_string()),
                        cooldown_until: Some(
                            input.now_ms + domain::contract::AUTOMATION_COOLDOWN_MS,
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
    /// AUTOMATION and the live-run set keys on the ACTION, and one id makes
    /// both readable in the asserts below.
    fn daily_action(id: &str, minute: u32) -> TriggeredAutomation {
        TriggeredAutomation {
            automation_id: id.to_string(),
            action_id: id.to_string(),
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

    fn event_action(id: &str) -> TriggeredAutomation {
        TriggeredAutomation {
            automation_id: id.to_string(),
            action_id: id.to_string(),
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
    fn seeded(fingerprint: &str, fired_ms: i64, watermark: (i64, &str)) -> AutomationState {
        AutomationState {
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
    ) -> AutomationState {
        AutomationState {
            seen_floor: Some(floor),
            seen_ids: seen_ids.iter().map(|id| id.to_string()).collect(),
            ..seeded(fingerprint, fired_ms, watermark)
        }
    }

    struct Snapshot {
        automations: Vec<TriggeredAutomation>,
        states: HashMap<String, AutomationState>,
        events: Vec<EventRow>,
        live: HashSet<String>,
        now_local: DateTime<FixedOffset>,
    }

    impl Snapshot {
        fn new(now_local: DateTime<FixedOffset>) -> Self {
            Self {
                automations: Vec::new(),
                states: HashMap::new(),
                events: Vec::new(),
                live: HashSet::new(),
                now_local,
            }
        }

        fn evaluate(&self) -> Vec<Decision> {
            evaluate(&EvalInput {
                automations: &self.automations,
                states: &self.states,
                events: &self.events,
                live_action_ids: &self.live,
                now_ms: self.now_local.timestamp_millis(),
                now_local: self.now_local.clone(),
            })
        }
    }

    /// An automation's pins win over the device defaults; everything it
    /// leaves NULL follows the machine, and plan mode is always off.
    #[test]
    fn launch_options_layer_pins_over_the_device_defaults() {
        let settings = crate::Settings::default();
        assert!(
            settings.plan_mode_for(settings.default_agent),
            "the shipped default parks claude/pi in plan mode"
        );

        // Nothing pinned = the device's own defaults, plan mode off.
        let bare = launch_options(&settings, None, None, None);
        let device = crate::LaunchOptions::defaults(&settings);
        assert_eq!(bare.agent, device.agent);
        assert_eq!(bare.model, device.model);
        assert_eq!(bare.effort, device.effort);
        assert!(!bare.plan_mode);

        // A pinned agent brings ITS defaults, then the explicit overrides.
        let pinned = launch_options(&settings, Some("codex"), Some("gpt-5.1-codex"), None);
        assert_eq!(pinned.agent, crate::CodingAgent::Codex);
        assert_eq!(pinned.model, "gpt-5.1-codex");
        assert_eq!(
            pinned.effort,
            crate::LaunchOptions::defaults_for(&settings, crate::CodingAgent::Codex).effort,
            "an unpinned effort still follows the device"
        );

        // An agent this build predates degrades to the device default —
        // never a refused run.
        assert_eq!(
            launch_options(&settings, Some("moonshot"), None, None).agent,
            settings.default_agent
        );
        // Empty strings are "unset", not a blank model.
        assert_eq!(launch_options(&settings, None, Some(""), Some("")).model, device.model);
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
        fresh.automations = vec![event_action("act-e")];
        fresh.events = vec![
            event_row("evt-1", "created", now_ms - 5_000),
            event_row("evt-2", "created", now_ms - 1_000),
        ];
        assert_eq!(
            fresh.evaluate(),
            vec![Decision::Reseed {
                automation_id: "act-e".to_string(),
                // No grace on a reseed: the floor sits AT the snapshot max
                // and evt-2 (the only row there) is already marked seen.
                new_state: seeded_seen(
                    "fp-act-e",
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
        edited.automations = vec![daily_action("act-s", 420)];
        edited
            .states
            .insert("act-s".to_string(), seeded("fp-STALE", yesterday_0700_ms, (0, "")));
        match &edited.evaluate()[..] {
            [Decision::Reseed { automation_id, new_state }] => {
                assert_eq!(automation_id, "act-s", "edited trigger reseeds instead of firing");
                assert_eq!(new_state.fingerprint, "fp-act-s");
                assert_eq!(new_state.last_fired_at, Some(now_ms));
            }
            other => panic!("expected one Reseed, got {other:?}"),
        }

        // Schedule offline catch-up: 3 missed days collapse into EXACTLY
        // one fire — the latest occurrence.
        let mut catchup = Snapshot::new(now.clone());
        catchup.automations = vec![daily_action("act-s", 420)];
        let three_days_ago = at(2026, 8, 15, 7, 0).timestamp_millis();
        catchup
            .states
            .insert("act-s".to_string(), seeded("fp-act-s", three_days_ago, (0, "")));
        match &catchup.evaluate()[..] {
            [Decision::Fire { automation_id, firing, new_state }] => {
                assert_eq!(automation_id, "act-s");
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
        steady.automations = vec![daily_action("act-s", 420)];
        steady
            .states
            .insert("act-s".to_string(), seeded("fp-act-s", today_0700_ms, (0, "")));
        assert_eq!(steady.evaluate(), vec![], "an anchored schedule is silent until the next tick");

        // Event coalescing: 3 matches → ONE Fire carrying all of them; the
        // watermark advances over a NEWER NON-MATCHING row too.
        let mut coalesce = Snapshot::new(now.clone());
        coalesce.automations = vec![event_action("act-e")];
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
        cross_team.automations = vec![event_action("act-e")];
        cross_team
            .states
            .insert("act-e".to_string(), seeded("fp-act-e", 0, (0, "")));
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
        cooling.automations = vec![event_action("act-e")];
        let mut cooled = seeded("fp-act-e", 0, (0, ""));
        cooled.cooldown_until = Some(now_ms + 30_000);
        cooling.states.insert("act-e".to_string(), cooled);
        cooling.events = vec![event_row("evt-1", "created", now_ms - 1_000)];
        assert_eq!(
            cooling.evaluate(),
            vec![Decision::Defer { automation_id: "act-e".to_string() }],
            "a cooling trigger defers, keeping the pending events for the next beat"
        );

        // A live run for the action defers.
        let mut running = Snapshot::new(now.clone());
        running.automations = vec![daily_action("act-s", 420)];
        running
            .states
            .insert("act-s".to_string(), seeded("fp-act-s", yesterday_0700_ms, (0, "")));
        running.live.insert("act-s".to_string());
        assert_eq!(
            running.evaluate(),
            vec![Decision::Defer { automation_id: "act-s".to_string() }],
            "a live run blocks a second launch of the same action"
        );

        // An Unsupported trigger is inert — not even a Reseed. (A DISABLED
        // automation never reaches the engine at all: the host filters
        // `enabled` off the row before snapshotting.)
        let mut inert = Snapshot::new(now.clone());
        inert.automations = vec![TriggeredAutomation {
            automation_id: "act-u".to_string(),
            action_id: "act-u".to_string(),
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
        holes.automations = vec![event_action("act-e")];
        holes.states.insert(
            "act-e".to_string(),
            AutomationState {
                fingerprint: "fp-act-e".to_string(),
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

    #[test]
    fn needs_events_only_for_event_triggers() {
        assert!(!needs_events(&[]), "no automations, no scan");
        assert!(
            !needs_events(&[daily_action("act-s", 420)]),
            "a schedule never reads the events snapshot"
        );
        let unsupported = TriggeredAutomation {
            automation_id: "act-u".to_string(),
            action_id: "act-u".to_string(),
            team_id: "team-1".to_string(),
            trigger: ParsedTrigger {
                kind: TriggerKind::Unsupported,
            },
            fingerprint: "fp-u".to_string(),
        };
        assert!(!needs_events(&[unsupported]));
        assert!(needs_events(&[daily_action("act-s", 420), event_action("act-e")]));
    }

    /// EXP-562 the whole point: a row that COMMITTED before the snapshot's
    /// newest one but SYNCED after it (overlapping transactions) still
    /// fires — exactly once.
    #[test]
    fn late_row_inside_grace_fires_once() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let cooldown = chrono::Duration::milliseconds(domain::contract::AUTOMATION_COOLDOWN_MS + 1);
        let mut first = Snapshot::new(now.clone());
        first.automations = vec![event_action("act-e")];
        first.states.insert(
            "act-e".to_string(),
            seeded_seen("fp-act-e", 0, (now_ms - 10_000, "evt-0"), now_ms - 20_000, &["evt-0"]),
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
        second.automations = vec![event_action("act-e")];
        second.states.insert("act-e".to_string(), fired_state);
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
        third.automations = vec![event_action("act-e")];
        third.states.insert("act-e".to_string(), second_state);
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
        snapshot.automations = vec![event_action("act-e")];
        snapshot.states.insert(
            "act-e".to_string(),
            seeded_seen("fp-act-e", 0, (now_ms - 5_000, "evt-2"), floor, &["evt-2"]),
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
        quiet.automations = vec![event_action("act-e")];
        quiet
            .states
            .insert("act-e".to_string(), seeded("fp-act-e", 0, (now_ms - 5_000, "evt-b")));
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
        burst.automations = vec![event_action("act-e")];
        burst
            .states
            .insert("act-e".to_string(), seeded_seen("fp-act-e", 0, (0, ""), now_ms - 30_000, &[]));
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

        let cooldown = chrono::Duration::milliseconds(domain::contract::AUTOMATION_COOLDOWN_MS + 1);
        let mut again = Snapshot::new(now + cooldown);
        again.automations = vec![event_action("act-e")];
        again.states.insert("act-e".to_string(), fired_state);
        again.events = burst.events.clone();
        assert_eq!(again.evaluate(), vec![], "the truncated rows fell BELOW the raised floor");
    }

    #[test]
    fn reseed_on_empty_snapshot_seeds_floor_at_now() {
        let now = at(2026, 8, 18, 7, 3);
        let now_ms = now.timestamp_millis();
        let mut empty = Snapshot::new(now);
        empty.automations = vec![event_action("act-e")];
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
        shrunk.automations = vec![event_action("act-e")];
        shrunk.states.insert(
            "act-e".to_string(),
            seeded_seen("fp-act-e", 0, (now_ms, "evt-gone"), floor, &[]),
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
