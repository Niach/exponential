//! EXP-530: the desktop GUI's action-automation host — the CLI daemon
//! worker's twin, per signed-in account and shaped exactly like
//! [`crate::device_sync`].
//!
//! The engine ([`coding::automations`]) is PURE: this host snapshots its
//! inputs on the foreground (the parsed triggers bound to THIS device, the
//! candidate `issue_events` rows with `board_id` joined from the synced
//! issues, the actions with a live local run, the clock) and runs
//! `read_states` → `evaluate` → `write_states` on the background executor,
//! because every one of those touches settings.json.
//!
//! Cadence mirrors the device-sync beat: a 1s tick gated to a 30s
//! evaluation, plus `cx.observe` watches on the `issue_events` and `actions`
//! collections that flip `eval_soon` so a freshly synced event (or an edited
//! trigger) is acted on within a tick instead of waiting out the beat.
//!
//! EXP-562: the foreground half is the part that must stay cheap, so it is
//! gated twice — a device with no ENABLED event trigger never looks at the
//! `issue_events` collection at all, and one that does reuses its last
//! [`EventCache`] until the collection's revision moves.
//!
//! Firing protocol (the module doc of [`coding::automations`] is the
//! authority, both hosts obey it): claim the `action:{id}` reservation →
//! persist the new state FIRST → launch. A failed claim skips WITHOUT a
//! state write (the run that holds it is the one that advances the
//! watermark); a failed prepare extends `cooldown_until` by
//! [`coding::automations::PREPARE_FAILURE_BACKOFF_MS`] through the
//! `on_failed` hook, so a poison-pill trigger backs off instead of
//! re-firing every beat.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local};
use gpui::{App, AppContext as _, Global};

use coding::automations::{
    self, AutomationState, Decision, EvalInput, EventRow, Firing, TriggeredAction, TriggerKind,
};
use coding::{LaunchOptions, LaunchOrigin, TriggerNote, TriggerNoteKind};
use domain::rows::ActionRow;

use crate::action_run::{self, ActionRepo, StartActionArgs};
use crate::coding_flow::{CodingHub, LocalSessions};
use crate::queries;
use crate::steer_wiring::ReservationGuard;

/// The evaluation cadence — a schedule's finest granularity is one minute,
/// so a half-minute beat can never miss an occurrence, and the collection
/// watches cover event triggers in realtime anyway.
const BEAT: Duration = Duration::from_secs(30);
/// The nudge-poll granularity inside the beat loop (device_sync's TICK).
const TICK: Duration = Duration::from_secs(1);

#[derive(Default)]
struct AutomationHostState {
    /// Stop flag per account (sign-out flips it; the loop retires itself).
    by_account: HashMap<String, Arc<AtomicBool>>,
    /// The issue_events + actions shape watches per account.
    watch_by_account: HashMap<String, Vec<gpui::Subscription>>,
    /// A synced-row change asked for an off-cadence evaluation.
    eval_soon: Arc<AtomicBool>,
}

struct AutomationHostGlobal(gpui::Entity<AutomationHostState>);
impl Global for AutomationHostGlobal {}

fn state(cx: &mut App) -> gpui::Entity<AutomationHostState> {
    if let Some(global) = cx.try_global::<AutomationHostGlobal>() {
        return global.0.clone();
    }
    let entity = cx.new(|_| AutomationHostState::default());
    cx.set_global(AutomationHostGlobal(entity.clone()));
    entity
}

/// Start the host for `account` (from `session::connect_account`, beside
/// [`crate::device_sync::start_device_sync`]). Restarting for the same
/// account replaces the old loop.
pub fn start_automation_host(account: &api::Account, cx: &mut App) {
    let state_entity = state(cx);
    let account_id = account.id.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let eval_soon = state_entity.update(cx, |state, _| {
        if let Some(previous) = state.by_account.insert(account_id.clone(), stop.clone()) {
            previous.store(true, Ordering::SeqCst);
        }
        state.eval_soon.clone()
    });

    let watches = watch_collections(stop.clone(), eval_soon.clone(), cx);
    state_entity.update(cx, |state, _| {
        if watches.is_empty() {
            state.watch_by_account.remove(&account_id);
        } else {
            state.watch_by_account.insert(account_id.clone(), watches);
        }
    });

    cx.spawn(async move |cx| {
        // Evaluate on the first tick: a trigger authored while this device
        // was offline must seed its state (never fire retroactively) before
        // any beat elapses.
        let mut ticks_since_beat = u32::MAX / 2;
        // EXP-562: the event snapshot survives between beats — see
        // [`EventCache`]. It lives in the loop, so a stopped host (sign-out)
        // drops it with everything else.
        let mut event_cache = EventCache::default();
        loop {
            cx.background_executor().timer(TICK).await;
            if stop.load(Ordering::SeqCst) {
                return;
            }
            ticks_since_beat = ticks_since_beat.saturating_add(1);
            let nudged = eval_soon.swap(false, Ordering::SeqCst);
            if !nudged && Duration::from_secs(ticks_since_beat as u64) < BEAT {
                continue;
            }
            ticks_since_beat = 0;

            // ONE foreground snapshot — every entity read happens here, the
            // whole evaluation below runs off the main thread.
            let Some(snapshot) = cx.update(|cx| snapshot_for(&account_id, &mut event_cache, cx))
            else {
                // Account switched away — retire; connect_account restarts.
                return;
            };
            if snapshot.actions.is_empty() {
                continue;
            }
            let options = snapshot.options.clone();
            let settings_path = snapshot.settings_path.clone();
            let device_id = snapshot.device_id.clone();

            let outcome = cx
                .background_executor()
                .spawn(async move { evaluate_pass(snapshot) })
                .await;
            if outcome.fires.is_empty() {
                continue;
            }

            // Claim first: a remote start (or a duplicate relay frame) already
            // preparing this action owns the launch, and a firing that loses
            // the claim writes NO state — the watermark stays put so the next
            // beat re-decides once the holder is done.
            let claimed: Vec<(FireOrder, ReservationGuard)> = outcome
                .fires
                .into_iter()
                .filter_map(|fire| {
                    match crate::steer_wiring::action_start_reservations()
                        .claim(format!("action:{}", fire.action_id))
                    {
                        Some(guard) => Some((fire, guard)),
                        None => {
                            log::info!(
                                "[automations] {} skipped — a start holding it is already in flight",
                                fire.action_id
                            );
                            None
                        }
                    }
                })
                .collect();
            if claimed.is_empty() {
                continue;
            }

            // Persist BEFORE launching (the firing protocol): a crash between
            // here and the spawn drops the run instead of double-firing it.
            // A FAILED write skips the launch too (the daemon's rule):
            // without a durable watermark the same occurrence would re-fire
            // every beat for as long as settings.json stays unwritable.
            let mut states = outcome.states;
            let advanced: Vec<(String, AutomationState)> = claimed
                .iter()
                .map(|(fire, _)| (fire.action_id.clone(), fire.new_state.clone()))
                .collect();
            let persisted = {
                let settings_path = settings_path.clone();
                let device_id = device_id.clone();
                cx.background_executor()
                    .spawn(async move {
                        for (action_id, new_state) in advanced {
                            states.insert(action_id, new_state);
                        }
                        write_states(&settings_path, &device_id, &states)
                    })
                    .await
            };
            if !persisted {
                continue;
            }

            cx.update(|cx| {
                for (fire, reservation) in claimed {
                    launch(
                        fire,
                        reservation,
                        options.clone(),
                        settings_path.clone(),
                        device_id.clone(),
                        cx,
                    );
                }
            });
        }
    })
    .detach();
}

/// Stop `account_id`'s host (from the sign-out paths, beside
/// [`crate::device_sync::stop_device_sync`]).
pub fn stop_automation_host(account_id: &str, cx: &mut App) {
    let state = state(cx);
    state.update(cx, |state, _| {
        if let Some(stop) = state.by_account.remove(account_id) {
            stop.store(true, Ordering::SeqCst);
        }
        // Dropping the subscriptions detaches the watches; the stop flag
        // covers a same-tick notification already in flight.
        state.watch_by_account.remove(account_id);
    });
}

/// Watch the two collections an evaluation reads: a new `issue_events` row is
/// the whole point of an event trigger, and an edited/created `actions` row
/// must re-seed within a tick instead of a beat. Unlike the device-sync
/// stamp latch there is nothing to de-bounce — evaluation is idempotent, and
/// firing is gated by the cooldown + the watermark, so an eager pass costs
/// one settings read.
fn watch_collections(
    stop: Arc<AtomicBool>,
    eval_soon: Arc<AtomicBool>,
    cx: &mut App,
) -> Vec<gpui::Subscription> {
    let Some(collections) = sync::Store::try_global(cx).map(|store| store.collections().clone())
    else {
        return Vec::new(); // headless tests — the beat cadence still runs
    };
    vec![
        watch_flag(&collections.issue_events, &stop, &eval_soon, cx),
        watch_flag(&collections.actions, &stop, &eval_soon, cx),
    ]
}

fn watch_flag<T: 'static>(
    entity: &gpui::Entity<T>,
    stop: &Arc<AtomicBool>,
    eval_soon: &Arc<AtomicBool>,
    cx: &mut App,
) -> gpui::Subscription {
    let stop = stop.clone();
    let eval_soon = eval_soon.clone();
    cx.observe(entity, move |_, _| {
        if !stop.load(Ordering::SeqCst) {
            eval_soon.store(true, Ordering::SeqCst);
        }
    })
}

// ---------------------------------------------------------------------------
// The foreground snapshot
// ---------------------------------------------------------------------------

/// An issue's display strings, for the trigger prompt's event lines.
#[derive(Clone, Debug)]
struct IssueDisplay {
    identifier: String,
    title: String,
}

/// The last built event snapshot, kept between beats (EXP-562). Rebuilding
/// it walks the WHOLE `issue_events` collection on the foreground thread, so
/// it happens only when the collection actually changed:
/// [`sync::Collection::revision`] is a monotonic counter bumped by every
/// upsert/remove/refetch, so an unchanged value means byte-identical rows.
/// The `Arc` hands the pass to the background executor without a copy.
#[derive(Default)]
struct EventCache {
    /// The revision `rows` were built from; `None` = nothing cached (first
    /// beat, or a pass that needs no events at all dropped it).
    revision: Option<u64>,
    rows: Arc<Vec<EventRow>>,
}

impl EventCache {
    /// The ONE rebuild condition. Kept pure so the cheap path is testable
    /// without a gpui app.
    fn needs_rebuild(&self, revision: u64) -> bool {
        self.revision != Some(revision)
    }
}

/// Everything one evaluation pass needs, read on the foreground.
struct EvalSnapshot {
    settings_path: PathBuf,
    /// The GUI's steer device id — triggers bound to the CLI daemon's
    /// sibling row (or another machine) belong to that host, never this one.
    device_id: String,
    /// This device's parseable triggers, each carrying its action's team
    /// (the event fence AND the runner's up-front team).
    actions: Vec<TriggeredAction>,
    /// Shared with [`EventCache`] — the pass only reads it.
    events: Arc<Vec<EventRow>>,
    live_action_ids: HashSet<String>,
    issue_display: HashMap<String, IssueDisplay>,
    label_names: HashMap<String, String>,
    now_local: DateTime<Local>,
    /// The device's own launch defaults for its default agent — an automated
    /// run is this machine's run, exactly like a dialog start with untouched
    /// options.
    options: LaunchOptions,
}

fn snapshot_for(account_id: &str, cache: &mut EventCache, cx: &mut App) -> Option<EvalSnapshot> {
    let account = queries::active_account(cx)?;
    if account.id != account_id {
        return None;
    }
    let auth = crate::session::AuthContext::global(cx);
    let device_id = steer::persistent_device_id(&auth.data_dir);
    let settings_path = coding::Settings::default_path(&auth.data_dir);
    let collections = sync::Store::try_global(cx)?.collections().clone();
    // The `&mut App` borrows first — everything below only reads.
    let sessions = LocalSessions::global(cx);
    let hub = CodingHub::global(cx);

    let options = automated_options(&hub.read(cx).settings);
    let live_action_ids = sessions.read(cx).live_action_ids();
    let actions = triggered_actions(collections.actions.read(cx).iter(), &device_id);
    if actions.is_empty() || !automations::needs_events(&actions) {
        // Nothing bound to this device, or nothing that READS events (a
        // schedule-only device never scans) — skip the potentially large
        // event collection entirely, and let the cache go with it.
        *cache = EventCache::default();
        return Some(EvalSnapshot {
            settings_path,
            device_id,
            actions,
            events: Arc::default(),
            live_action_ids,
            issue_display: HashMap::new(),
            label_names: HashMap::new(),
            now_local: Local::now(),
            options,
        });
    }

    let now_local = Local::now();
    let cutoff = now_local.timestamp_millis() - domain::contract::AUTOMATION_EVENT_CATCHUP_MS;
    let issues = collections.issues.read(cx);
    let synced_events = collections.issue_events.read(cx);
    let revision = synced_events.revision();
    if cache.needs_rebuild(revision) {
        let mut rows = Vec::new();
        for row in synced_events.iter() {
            let Some(created_at_ms) = row
                .created_at
                .as_deref()
                .and_then(crate::inbox::parse_timestamp)
                .map(|parsed| parsed.timestamp_millis())
            else {
                continue;
            };
            // The engine re-applies the cutoff, but pruning here is what
            // keeps the CACHE from growing without bound between rebuilds.
            if created_at_ms < cutoff {
                continue;
            }
            rows.push(EventRow {
                id: row.id.clone(),
                issue_id: row.issue_id.clone(),
                // The denormalized team — the engine fences matching to the
                // action's own team (a missing value conservatively never
                // matches).
                team_id: row.team_id.clone(),
                created_at_ms,
                // A row without a type can never match a spec, but it still
                // belongs in the snapshot — the watermark advances over it.
                kind: row.kind.clone().unwrap_or_default(),
                payload: row.payload.clone(),
                // Pre-joined: the issue_events shape carries no board_id, and
                // an unknown issue must FAIL a board filter (never silently
                // pass).
                board_id: issues
                    .get(&row.issue_id)
                    .map(|issue| issue.board_id.clone()),
            });
        }
        cache.rows = Arc::new(rows);
        cache.revision = Some(revision);
    } else {
        patch_missing_boards(&mut cache.rows, |issue_id| {
            issues.get(issue_id).map(|issue| issue.board_id.clone())
        });
    }
    let events = cache.rows.clone();

    // The two display maps are rebuilt every pass from the cached rows —
    // distinct issues/labels inside the catch-up window, so this is a small
    // map over an already-narrowed list, not a second scan.
    let mut issue_display: HashMap<String, IssueDisplay> = HashMap::new();
    for row in events.iter() {
        if issue_display.contains_key(&row.issue_id) {
            continue;
        }
        if let Some(issue) = issues.get(&row.issue_id) {
            issue_display.insert(
                row.issue_id.clone(),
                IssueDisplay {
                    identifier: issue.identifier.clone(),
                    title: issue.title.clone(),
                },
            );
        }
    }

    let referenced_labels: HashSet<String> = events
        .iter()
        .filter(|row| row.kind == "label_added")
        .filter_map(|row| payload_str(row, "labelId"))
        .collect();
    let labels = collections.labels.read(cx);
    let label_names = referenced_labels
        .into_iter()
        .filter_map(|id| labels.get(&id).map(|label| (id, label.name.clone())))
        .collect();

    Some(EvalSnapshot {
        settings_path,
        device_id,
        actions,
        events,
        live_action_ids,
        issue_display,
        label_names,
        now_local,
        options,
    })
}

/// Heal the board join of CACHED rows that still have none: `issue_events`
/// routinely syncs BEFORE the issue it belongs to, and a missing board FAILS
/// a board filter — so a miss frozen into the cache would never match, even
/// once the issue lands. The rows are copied (`Arc::make_mut`) only when a
/// lookup actually resolves, so the steady state — every issue synced, or
/// none of them yet — allocates nothing. Returns whether anything changed.
fn patch_missing_boards(
    rows: &mut Arc<Vec<EventRow>>,
    board_of: impl Fn(&str) -> Option<String>,
) -> bool {
    let resolved: Vec<(usize, String)> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.board_id.is_none())
        .filter_map(|(index, row)| board_of(&row.issue_id).map(|board_id| (index, board_id)))
        .collect();
    if resolved.is_empty() {
        return false;
    }
    let rows = Arc::make_mut(rows);
    for (index, board_id) in resolved {
        rows[index].board_id = Some(board_id);
    }
    true
}

/// This device's launch defaults for an AUTOMATED run: the saved default
/// agent with its model/effort/permission toggles, plan mode forced OFF.
/// The CLI daemon's rule (`launch::agent_options` with `interactive =
/// false`, F7): an unattended run must never park at the plan-approval card
/// waiting for a human who is not watching.
fn automated_options(settings: &coding::Settings) -> LaunchOptions {
    let mut options = LaunchOptions::defaults(settings);
    options.plan_mode = false;
    options
}

/// The synced `actions` rows this device evaluates: a parseable trigger bound
/// to `device_id`, with a team to run in. The fingerprint hashes the RAW
/// trigger JSON as synced — the canonical input BOTH hosts must use, or a
/// GUI/CLI hand-off would re-seed the shared state on every switch.
fn triggered_actions<'a>(
    rows: impl Iterator<Item = &'a ActionRow>,
    device_id: &str,
) -> Vec<TriggeredAction> {
    let mut actions = Vec::new();
    for row in rows {
        let Some(raw) = row.trigger.as_ref() else {
            continue; // manual-only action
        };
        let Some(trigger) = automations::parse_trigger(raw) else {
            continue; // untargetable (no deviceId) — no host can evaluate it
        };
        if trigger.device_id != device_id {
            continue; // another machine's (or the CLI daemon's) trigger
        }
        let Some(team_id) = row.team_id.clone() else {
            continue; // no team to run in
        };
        actions.push(TriggeredAction {
            action_id: row.id.clone(),
            team_id,
            trigger,
            fingerprint: automations::trigger_fingerprint(raw),
        });
    }
    actions
}

// ---------------------------------------------------------------------------
// The evaluation pass (background executor)
// ---------------------------------------------------------------------------

/// One action the engine decided to fire, with its prompt note pre-rendered
/// (the display lookups live in the snapshot, not the launcher).
struct FireOrder {
    action_id: String,
    team_id: String,
    note: TriggerNote,
    new_state: AutomationState,
}

struct PassOutcome {
    /// The device's whole state map INCLUDING the applied reseeds — the fire
    /// writes replace it wholesale, so it must stay complete.
    states: HashMap<String, AutomationState>,
    fires: Vec<FireOrder>,
}

fn evaluate_pass(snapshot: EvalSnapshot) -> PassOutcome {
    let now_ms = snapshot.now_local.timestamp_millis();
    let mut states = automations::read_states(&snapshot.settings_path, &snapshot.device_id);
    let decisions = automations::evaluate(&EvalInput {
        actions: &snapshot.actions,
        states: &states,
        events: &snapshot.events[..],
        live_action_ids: &snapshot.live_action_ids,
        now_ms,
        now_local: snapshot.now_local.clone(),
    });
    let mut reseeded = false;
    let mut fires = Vec::new();
    for decision in decisions {
        match decision {
            // Cooling down or already running — nothing to persist.
            Decision::Defer { .. } => {}
            Decision::Reseed {
                action_id,
                new_state,
            } => {
                states.insert(action_id, new_state);
                reseeded = true;
            }
            Decision::Fire {
                action_id,
                firing,
                new_state,
            } => {
                let Some(action) = snapshot
                    .actions
                    .iter()
                    .find(|candidate| candidate.action_id == action_id)
                else {
                    continue;
                };
                fires.push(FireOrder {
                    action_id,
                    team_id: action.team_id.clone(),
                    note: trigger_note(action, &firing, &snapshot),
                    new_state,
                });
            }
        }
    }
    if reseeded {
        write_states(&snapshot.settings_path, &snapshot.device_id, &states);
    }
    PassOutcome { states, fires }
}

fn write_states(
    settings_path: &Path,
    device_id: &str,
    states: &HashMap<String, AutomationState>,
) -> bool {
    match automations::write_states(settings_path, device_id, states) {
        Ok(()) => true,
        Err(err) => {
            log::warn!("[automations] state write failed: {err}");
            false
        }
    }
}

/// The prompt's `## Trigger` note: a schedule renders the shared phrase, an
/// event the per-issue lines (capped — the overflow becomes the note's
/// `omitted` count).
fn trigger_note(action: &TriggeredAction, firing: &Firing, snapshot: &EvalSnapshot) -> TriggerNote {
    match firing {
        Firing::Schedule { .. } => {
            let phrase = match &action.trigger.kind {
                TriggerKind::Schedule(schedule) => automations::schedule_phrase(schedule),
                // Unreachable — the engine only schedules a Schedule trigger.
                _ => String::new(),
            };
            TriggerNote {
                kind: TriggerNoteKind::Schedule { phrase },
            }
        }
        Firing::Event { matches } => {
            let lines = matches
                .iter()
                .take(automations::TRIGGER_PROMPT_MAX_LINES)
                .map(|row| event_line(row, &snapshot.issue_display, &snapshot.label_names))
                .collect();
            TriggerNote {
                kind: TriggerNoteKind::Event {
                    lines,
                    omitted: matches
                        .len()
                        .saturating_sub(automations::TRIGGER_PROMPT_MAX_LINES),
                },
            }
        }
    }
}

/// One event's prompt line: `EXP-142 "Ship it" status Todo → In Progress`.
/// The agent gets the identifier first (its MCP handle), then the human
/// title, then what changed.
fn event_line(
    row: &EventRow,
    issues: &HashMap<String, IssueDisplay>,
    labels: &HashMap<String, String>,
) -> String {
    let display = issues.get(&row.issue_id);
    // An unsynced issue still names its event — the agent can look the id up.
    let identifier = display
        .map(|display| display.identifier.clone())
        .unwrap_or_else(|| format!("issue {}", row.issue_id));
    let tail = event_tail(row, labels);
    match display.map(|display| display.title.trim()).unwrap_or("") {
        "" => format!("{identifier} {tail}"),
        title => format!("{identifier} \"{title}\" {tail}"),
    }
}

/// The per-kind tail of an [`event_line`], mirroring the timeline's phrases
/// in the compact form a prompt list wants.
fn event_tail(row: &EventRow, labels: &HashMap<String, String>) -> String {
    match row.kind.as_str() {
        "status_changed" => {
            // EXP-314: the payload carries the human NAMES alongside the
            // legacy enum anchors — a custom status has no enum to munge.
            let to = payload_str(row, "toName")
                .filter(|name| !name.is_empty())
                .or_else(|| payload_str(row, "to").map(|wire| status_label(&wire)))
                .unwrap_or_default();
            let from = payload_str(row, "fromName")
                .filter(|name| !name.is_empty())
                .or_else(|| payload_str(row, "from").map(|wire| status_label(&wire)))
                .filter(|name| !name.is_empty());
            match (from, to.is_empty()) {
                (Some(from), false) => format!("status {from} → {to}"),
                (_, false) => format!("status {to}"),
                _ => "status changed".to_string(),
            }
        }
        "priority_changed" => {
            let to = payload_str(row, "to").unwrap_or_default();
            let from = payload_str(row, "from").filter(|value| !value.is_empty());
            match (from, to.is_empty()) {
                (Some(from), false) => format!("priority {from} → {to}"),
                (_, false) => format!("priority {to}"),
                _ => "priority changed".to_string(),
            }
        }
        "created" => match payload_str(row, "priority").filter(|value| !value.is_empty()) {
            Some(priority) => format!("created ({priority})"),
            None => "created".to_string(),
        },
        "label_added" => {
            let name = payload_str(row, "labelId")
                .and_then(|id| labels.get(&id).cloned())
                .unwrap_or_else(|| "a label".to_string());
            format!("label {name} added")
        }
        "assignee_changed" => "assignee changed".to_string(),
        "pr_opened" => "pull request opened".to_string(),
        "pr_merged" => "pull request merged".to_string(),
        // A future event kind this build predates still reads sensibly.
        other => other.replace('_', " "),
    }
}

/// A scalar payload field as text (the timeline's `payload_str` idiom —
/// numbers and bools stringify rather than vanishing).
fn payload_str(row: &EventRow, key: &str) -> Option<String> {
    match row.payload.as_ref()?.get(key)? {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// The pre-EXP-314 enum-anchor fallback (`in_progress` → `in progress`).
fn status_label(wire: &str) -> String {
    wire.replace('_', " ")
}

// ---------------------------------------------------------------------------
// The launch (foreground)
// ---------------------------------------------------------------------------

fn launch(
    fire: FireOrder,
    reservation: ReservationGuard,
    options: LaunchOptions,
    settings_path: PathBuf,
    device_id: String,
    cx: &mut App,
) {
    let FireOrder {
        action_id,
        team_id,
        note,
        ..
    } = fire;
    log::info!(
        "[automations] starting {action_id} ({})",
        note.started_reason()
    );
    // The poison-pill hook: a run that never reaches the agent (disabled
    // launcher, prepare failure, no window) has already advanced its
    // watermark — without a backoff its trigger would re-fire every beat.
    let failed_action = action_id.clone();
    let on_failed: action_run::ActionFailureHook = Box::new(move |cx: &mut App| {
        cx.background_executor()
            .spawn(async move {
                back_off(&settings_path, &device_id, &failed_action);
            })
            .detach();
    });
    action_run::start_action_run(
        StartActionArgs {
            action_id,
            team_id,
            // Local start: the action's own `repository_id` resolves through
            // `repositories.list` exactly like a dialog run.
            repo: ActionRepo::Resolve,
            options,
            // The run belongs to this machine's signed-in user, not a relay
            // requester — an automation is the device acting for its owner.
            origin: LaunchOrigin::Local,
            // Automations carry no run-time inputs (a trigger cannot prompt).
            inputs: Vec::new(),
            target: None,
            // Never steal focus: the whole point is an unattended run.
            activate_app: false,
            reservation: Some(reservation),
            trigger: Some(note),
            on_failed: Some(on_failed),
        },
        cx,
    );
}

/// Extend the action's cooldown by the prepare-failure backoff. Read-modify-
/// write on the CURRENT map so a concurrent pass's reseeds survive; a state
/// that vanished meanwhile (trigger deleted) is left alone.
fn back_off(settings_path: &Path, device_id: &str, action_id: &str) {
    let mut states = automations::read_states(settings_path, device_id);
    let Some(state) = states.get_mut(action_id) else {
        return;
    };
    let now_ms = Local::now().timestamp_millis();
    let base = state.cooldown_until.unwrap_or(now_ms).max(now_ms);
    state.cooldown_until = Some(base + automations::PREPARE_FAILURE_BACKOFF_MS);
    log::info!("[automations] {action_id} backed off after a failed start");
    write_states(settings_path, device_id, &states);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn action_row(id: &str, team_id: Option<&str>, trigger: Option<serde_json::Value>) -> ActionRow {
        ActionRow {
            id: id.to_string(),
            team_id: team_id.map(str::to_string),
            repository_id: None,
            name: Some(format!("Action {id}")),
            description: None,
            icon: None,
            inputs: None,
            trigger,
            sort_order: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn schedule_trigger(device_id: &str) -> serde_json::Value {
        json!({
            "kind": "schedule",
            "deviceId": device_id,
            "enabled": true,
            "interval": "daily",
            "minuteOfDay": 420,
        })
    }

    fn event_row(id: &str, kind: &str, payload: Option<serde_json::Value>) -> EventRow {
        EventRow {
            id: id.to_string(),
            issue_id: "issue-1".to_string(),
            team_id: Some("team-1".to_string()),
            created_at_ms: 1_000,
            kind: kind.to_string(),
            payload,
            board_id: Some("board-1".to_string()),
        }
    }

    fn displays() -> HashMap<String, IssueDisplay> {
        HashMap::from([(
            "issue-1".to_string(),
            IssueDisplay {
                identifier: "EXP-142".to_string(),
                title: "Ship it".to_string(),
            },
        )])
    }

    /// An automated run never launches into plan mode, whatever the device's
    /// saved default is — the shipped default IS plan mode for claude and pi,
    /// so an unattended run would park at the plan-approval card forever.
    #[test]
    fn automated_options_force_plan_mode_off() {
        let settings = coding::Settings::default();
        assert!(
            settings.plan_mode_for(settings.default_agent),
            "the shipped default parks claude/pi in plan mode"
        );
        let options = automated_options(&settings);
        assert!(!options.plan_mode);

        // Everything else still follows the device's own defaults — an
        // automation is this machine's run, exactly like a dialog start.
        let dialog = LaunchOptions::defaults(&settings);
        assert_eq!(options.agent, dialog.agent);
        assert_eq!(options.model, dialog.model);
        assert_eq!(options.effort, dialog.effort);
        assert_eq!(options.ultracode, dialog.ultracode);
        assert_eq!(options.skip_permissions, dialog.skip_permissions);
    }

    /// Only THIS device's parseable, team-bound triggers are evaluated —
    /// every other row belongs to another host (or to no host at all).
    #[test]
    fn triggered_actions_keep_only_this_devices_rows() {
        let rows = vec![
            action_row("mine", Some("team-1"), Some(schedule_trigger("dev-gui"))),
            action_row("theirs", Some("team-1"), Some(schedule_trigger("dev-cli"))),
            action_row("manual", Some("team-1"), None),
            // No deviceId: untargetable — no host could ever fire it.
            action_row(
                "untargeted",
                Some("team-1"),
                Some(json!({"kind": "schedule", "interval": "daily", "minuteOfDay": 1})),
            ),
            // A team-less row has nowhere to run.
            action_row("teamless", None, Some(schedule_trigger("dev-gui"))),
            // A FUTURE kind stays visible-but-inert (Unsupported), so it must
            // still reach the engine — which decides nothing for it.
            action_row(
                "future",
                Some("team-2"),
                Some(json!({"kind": "moon_phase", "deviceId": "dev-gui"})),
            ),
        ];
        let actions = triggered_actions(rows.iter(), "dev-gui");
        assert_eq!(
            actions
                .iter()
                .map(|action| action.action_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mine", "future"],
            "other devices', manual, untargetable and team-less rows are skipped"
        );
        assert_eq!(actions[1].trigger.kind, TriggerKind::Unsupported);
        assert_eq!(actions[0].team_id, "team-1");
        assert_eq!(actions[1].team_id, "team-2");

        // The fingerprint is the RAW trigger JSON's — identical input, identical
        // hash, so a GUI↔CLI hand-off never re-seeds.
        let again = triggered_actions(rows.iter(), "dev-gui");
        assert_eq!(actions[0].fingerprint, again[0].fingerprint);
        assert_eq!(
            actions[0].fingerprint,
            automations::trigger_fingerprint(&schedule_trigger("dev-gui")),
        );
    }

    /// EXP-562: the foreground pass walks the whole `issue_events`
    /// collection ONLY when its revision moved — every other beat reuses the
    /// rows, patching just the board joins that were still missing.
    #[test]
    fn event_cache_rebuilds_only_on_revision_change() {
        let mut cache = EventCache::default();
        // Nothing cached yet: the first pass must always build.
        assert!(cache.needs_rebuild(7));

        let mut unjoined = event_row("e1", "created", None);
        unjoined.board_id = None;
        cache.rows = Arc::new(vec![event_row("e0", "created", None), unjoined]);
        cache.revision = Some(7);
        assert!(!cache.needs_rebuild(7), "an unchanged revision reuses");
        assert!(cache.needs_rebuild(8), "any upsert/remove/refetch rebuilds");
        // A dropped cache (a pass that needed no events) rebuilds next time.
        assert!(EventCache::default().needs_rebuild(7));

        // The reuse path heals the row whose issue synced late…
        let before = cache.rows.clone();
        assert!(patch_missing_boards(&mut cache.rows, |issue_id| {
            (issue_id == "issue-1").then(|| "board-9".to_string())
        }));
        assert_eq!(cache.rows[1].board_id.as_deref(), Some("board-9"));
        assert_eq!(
            cache.rows[0].board_id.as_deref(),
            Some("board-1"),
            "an already-joined row is never re-read"
        );
        assert!(!Arc::ptr_eq(&before, &cache.rows), "the patch copies once");

        // …and once every join resolves, the pass allocates nothing.
        let steady = cache.rows.clone();
        assert!(!patch_missing_boards(&mut cache.rows, |_| Some(
            "board-9".to_string()
        )));
        assert!(Arc::ptr_eq(&steady, &cache.rows));
    }

    /// One line per event kind — the strings the agent reads to know WHY it
    /// was started.
    #[test]
    fn event_lines_render_every_kind() {
        let labels = HashMap::from([("label-1".to_string(), "bug".to_string())]);
        let issues = displays();
        let line = |row: &EventRow| event_line(row, &issues, &labels);

        assert_eq!(
            line(&event_row(
                "e1",
                "status_changed",
                Some(json!({"fromName": "Todo", "toName": "In Progress"})),
            )),
            "EXP-142 \"Ship it\" status Todo → In Progress"
        );
        // Pre-EXP-314 rows carry only the enum anchors — munged, not dropped.
        assert_eq!(
            line(&event_row(
                "e2",
                "status_changed",
                Some(json!({"from": "todo", "to": "in_progress"})),
            )),
            "EXP-142 \"Ship it\" status todo → in progress"
        );
        assert_eq!(
            line(&event_row(
                "e3",
                "priority_changed",
                Some(json!({"from": "low", "to": "urgent"})),
            )),
            "EXP-142 \"Ship it\" priority low → urgent"
        );
        assert_eq!(
            line(&event_row("e4", "created", Some(json!({"priority": "high"})))),
            "EXP-142 \"Ship it\" created (high)"
        );
        assert_eq!(
            line(&event_row(
                "e5",
                "label_added",
                Some(json!({"labelId": "label-1"})),
            )),
            "EXP-142 \"Ship it\" label bug added"
        );
        // An unsynced label degrades instead of naming a raw uuid.
        assert_eq!(
            line(&event_row(
                "e6",
                "label_added",
                Some(json!({"labelId": "label-gone"})),
            )),
            "EXP-142 \"Ship it\" label a label added"
        );
        assert_eq!(
            line(&event_row("e7", "assignee_changed", None)),
            "EXP-142 \"Ship it\" assignee changed"
        );
        assert_eq!(
            line(&event_row("e8", "pr_opened", None)),
            "EXP-142 \"Ship it\" pull request opened"
        );
        assert_eq!(
            line(&event_row("e9", "pr_merged", None)),
            "EXP-142 \"Ship it\" pull request merged"
        );

        // An unsynced issue still names its event (the id is the agent's handle).
        let mut orphan = event_row("e10", "created", None);
        orphan.issue_id = "issue-unknown".to_string();
        assert_eq!(line(&orphan), "issue issue-unknown created");
    }

    /// The prompt cap: the note keeps the first N lines and counts the rest,
    /// so a 24h catch-up burst can't blow the seed prompt open.
    #[test]
    fn event_note_caps_its_lines() {
        let snapshot = EvalSnapshot {
            settings_path: PathBuf::new(),
            device_id: "dev-gui".to_string(),
            actions: Vec::new(),
            events: Arc::default(),
            live_action_ids: HashSet::new(),
            issue_display: displays(),
            label_names: HashMap::new(),
            now_local: Local::now(),
            options: LaunchOptions::defaults(&coding::Settings::default()),
        };
        let action = TriggeredAction {
            action_id: "act".to_string(),
            team_id: "team-1".to_string(),
            trigger: automations::parse_trigger(&json!({
                "kind": "event",
                "deviceId": "dev-gui",
                "event": "created",
            }))
            .expect("parses"),
            fingerprint: "fp".to_string(),
        };
        let overflow = automations::TRIGGER_PROMPT_MAX_LINES + 7;
        let matches: Vec<EventRow> = (0..overflow)
            .map(|index| event_row(&format!("e{index}"), "created", None))
            .collect();
        match trigger_note(&action, &Firing::Event { matches }, &snapshot).kind {
            TriggerNoteKind::Event { lines, omitted } => {
                assert_eq!(lines.len(), automations::TRIGGER_PROMPT_MAX_LINES);
                assert_eq!(omitted, 7, "the overflow is counted, never dropped silently");
            }
            other => panic!("expected an event note, got {other:?}"),
        }

        // A schedule firing renders the SHARED phrase (identical copy on the
        // CLI host and in the UI's summary rows).
        let scheduled = TriggeredAction {
            action_id: "act".to_string(),
            team_id: "team-1".to_string(),
            trigger: automations::parse_trigger(&schedule_trigger("dev-gui")).expect("parses"),
            fingerprint: "fp".to_string(),
        };
        match trigger_note(
            &scheduled,
            &Firing::Schedule { occurrence_ms: 0 },
            &snapshot,
        )
        .kind
        {
            TriggerNoteKind::Schedule { phrase } => assert_eq!(phrase, "daily at 07:00"),
            other => panic!("expected a schedule note, got {other:?}"),
        }
    }
}
