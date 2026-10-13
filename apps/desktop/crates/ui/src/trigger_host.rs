//! EXP-530: the desktop GUI's trigger host — the CLI daemon worker's twin,
//! per signed-in account and shaped exactly like [`crate::device_sync`].
//!
//! EXP-1294 (Threads v2): triggers live on AGENTS — an agent root (`sessions`
//! kind `agent`) hosted on THIS device carries its `triggers` jsonb, and a
//! firing posts ONE message into that agent's channel
//! (`api::session_device::post_message`); the pickup delivers it like any
//! other message. Nothing here launches a run.
//!
//! The engine ([`coding::triggers`]) is PURE: this host snapshots its inputs
//! on the foreground (the triggers of this device's agents, the candidate
//! `issue_events` rows with `board_id` joined from the synced issues, the
//! clock) and runs `read_states` → `evaluate` → `write_states` on the
//! background executor, because every one of those touches the state file.
//!
//! Cadence mirrors the device-sync beat: a 1s tick gated to a 30s
//! evaluation, plus `cx.observe` watches on the `issue_events` and `sessions`
//! collections that flip `eval_soon` so a freshly synced event (or an edited
//! trigger) is acted on within a tick instead of waiting out the beat.
//!
//! EXP-562: the foreground half is the part that must stay cheap, so it is
//! gated twice — a device with no ENABLED event trigger never looks at the
//! `issue_events` collection at all, and one that does reuses its last
//! [`EventCache`] until the collection's revision moves.
//!
//! Firing protocol: persist the new state FIRST → post. A failed post
//! extends `cooldown_until` by [`coding::triggers::PREPARE_FAILURE_BACKOFF_MS`],
//! so a poison-pill trigger backs off instead of re-firing every beat.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local};
use gpui::{App, AppContext as _, Global};

use coding::triggers::{self, ArmedTrigger, Decision, EvalInput, EventRow, Firing, TriggerState};
use domain::rows::SessionRow;

use crate::queries;

/// The evaluation cadence — a schedule's finest granularity is one minute,
/// so a half-minute beat can never miss an occurrence, and the collection
/// watches cover event triggers in realtime anyway.
const BEAT: Duration = Duration::from_secs(30);
/// The nudge-poll granularity inside the beat loop (device_sync's TICK).
const TICK: Duration = Duration::from_secs(1);

#[derive(Default)]
struct TriggerHostState {
    /// Stop flag per account (sign-out flips it; the loop retires itself).
    by_account: HashMap<String, Arc<AtomicBool>>,
    /// The issue_events + sessions shape watches per account.
    watch_by_account: HashMap<String, Vec<gpui::Subscription>>,
    /// A synced-row change asked for an off-cadence evaluation.
    eval_soon: Arc<AtomicBool>,
}

struct TriggerHostGlobal(gpui::Entity<TriggerHostState>);
impl Global for TriggerHostGlobal {}

fn state(cx: &mut App) -> gpui::Entity<TriggerHostState> {
    if let Some(global) = cx.try_global::<TriggerHostGlobal>() {
        return global.0.clone();
    }
    let entity = cx.new(|_| TriggerHostState::default());
    cx.set_global(TriggerHostGlobal(entity.clone()));
    entity
}

/// Start the host for `account` (from `session::connect_account`, beside
/// [`crate::device_sync::start_device_sync`]). Restarting for the same
/// account replaces the old loop.
pub fn start_trigger_host(account: &api::Account, cx: &mut App) {
    // EXP-1102: the state's one move out of settings.json — a no-op once done.
    {
        let auth = crate::session::AuthContext::global(cx);
        let device_id = steer::persistent_device_id(&auth.data_dir);
        let settings_path = coding::Settings::default_path(&auth.data_dir);
        if triggers::TriggerStore::open(&auth.data_dir, &device_id)
            .migrate_legacy(&settings_path)
        {
            log::info!("[triggers] moved the trigger state out of settings.json");
        }
    }
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
            if snapshot.armed.is_empty() {
                continue;
            }
            let settings_path = snapshot.settings_path.clone();
            let device_id = snapshot.device_id.clone();

            let outcome = cx
                .background_executor()
                .spawn(async move { evaluate_pass(snapshot) })
                .await;
            if outcome.fires.is_empty() {
                continue;
            }

            // Persist BEFORE posting (the firing protocol): a crash between
            // here and the post drops the firing instead of double-firing it.
            // A FAILED write skips the post too: without a durable watermark
            // the same occurrence would re-fire every beat.
            let mut states = outcome.states;
            let advanced: Vec<(String, TriggerState)> = outcome
                .fires
                .iter()
                .map(|fire| (fire.trigger_id.clone(), fire.new_state.clone()))
                .collect();
            let persisted = {
                let settings_path = settings_path.clone();
                let device_id = device_id.clone();
                cx.background_executor()
                    .spawn(async move {
                        for (trigger_id, new_state) in advanced {
                            states.insert(trigger_id, new_state);
                        }
                        write_states(&settings_path, &device_id, &states)
                    })
                    .await
            };
            if !persisted {
                continue;
            }
            let Some(trpc) = cx.update(|cx| queries::trpc_client(cx).map(Arc::new)) else {
                continue;
            };
            for fire in outcome.fires {
                let trpc = Arc::clone(&trpc);
                let settings_path = settings_path.clone();
                let device_id = device_id.clone();
                cx.background_executor()
                    .spawn(async move { fire_trigger(&trpc, fire, &settings_path, &device_id) })
                    .detach();
            }
        }
    })
    .detach();
}

/// Stop `account_id`'s host (from the sign-out paths, beside
/// [`crate::device_sync::stop_device_sync`]).
pub fn stop_trigger_host(account_id: &str, cx: &mut App) {
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
/// the whole point of an event trigger, and an agent whose `triggers`
/// changed must re-seed within a tick instead of a beat. Unlike the device-sync
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
        watch_flag(&collections.sessions, &stop, &eval_soon, cx),
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
    /// This device's enabled, readable agent triggers, each carrying its
    /// team (the event fence). `action_id` names the AGENT session.
    armed: Vec<ArmedTrigger>,
    /// trigger id → the trigger element as synced (what the firing message
    /// renders).
    raw: HashMap<String, serde_json::Value>,
    /// Shared with [`EventCache`] — the pass only reads it.
    events: Arc<Vec<EventRow>>,
    issue_display: HashMap<String, IssueDisplay>,
    label_names: HashMap<String, String>,
    now_local: DateTime<Local>,
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
    let (triggers_bound, raw) = armed_triggers(collections.sessions.read(cx).iter(), &device_id);
    if triggers_bound.is_empty() || !triggers::needs_events(&triggers_bound) {
        // Nothing bound to this device, or nothing that READS events (a
        // schedule-only device never scans) — skip the potentially large
        // event collection entirely, and let the cache go with it.
        *cache = EventCache::default();
        return Some(EvalSnapshot {
            settings_path,
            device_id,
            armed: triggers_bound,
            raw,
            events: Arc::default(),
            issue_display: HashMap::new(),
            label_names: HashMap::new(),
            now_local: Local::now(),
        });
    }

    let now_local = Local::now();
    let cutoff = now_local.timestamp_millis() - domain::contract::TRIGGER_EVENT_CATCHUP_MS;
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
        armed: triggers_bound,
        raw,
        events,
        issue_display,
        label_names,
        now_local,
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

/// EXP-1294 — the triggers this device evaluates: every ENABLED, readable
/// trigger on an agent root hosted HERE (its `device_id`) that has a team to
/// fence events by. Returns the engine input (`action_id` = the agent's
/// session id) plus each trigger's raw element. The fingerprint is the
/// WHEN-part's, so toggling a trigger never re-seeds its state.
fn armed_triggers<'a>(
    rows: impl Iterator<Item = &'a SessionRow>,
    device_id: &str,
) -> (Vec<ArmedTrigger>, HashMap<String, serde_json::Value>) {
    let mut bound = Vec::new();
    let mut raw = HashMap::new();
    for agent in rows.filter(|row| row.is_agent() && row.is_root()) {
        if agent.device_id.as_deref() != Some(device_id) {
            continue;
        }
        let Some(team_id) = agent.team_id.clone() else {
            continue;
        };
        for element in agent.trigger_list() {
            let Some(trigger_id) = element
                .get("id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
            else {
                continue;
            };
            // Only an explicit `false` pauses.
            if element.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
                continue;
            }
            let Some(when) = triggers::parse_trigger(&element)
                .filter(|when| when.kind != triggers::TriggerKind::Unsupported)
            else {
                continue;
            };
            bound.push(ArmedTrigger {
                fingerprint: triggers::trigger_fingerprint(&triggers::when_part(&element)),
                trigger_id: trigger_id.clone(),
                action_id: agent.id.clone(),
                team_id: team_id.clone(),
                trigger: when,
            });
            raw.insert(trigger_id, element);
        }
    }
    (bound, raw)
}

// ---------------------------------------------------------------------------
// The evaluation pass (background executor)
// ---------------------------------------------------------------------------

/// One trigger the engine decided to fire, with its message pre-rendered
/// (the display lookups live in the snapshot, not the post).
struct FireOrder {
    trigger_id: String,
    /// The agent session the message goes to.
    agent_id: String,
    text: String,
    new_state: TriggerState,
}

struct PassOutcome {
    /// The device's whole state map INCLUDING the applied reseeds — the fire
    /// writes replace it wholesale, so it must stay complete.
    states: HashMap<String, TriggerState>,
    fires: Vec<FireOrder>,
}

fn evaluate_pass(snapshot: EvalSnapshot) -> PassOutcome {
    let now_ms = snapshot.now_local.timestamp_millis();
    let mut states = triggers::read_states(&store(&snapshot.settings_path, &snapshot.device_id));
    // A firing only posts a message (the channel queues it), so nothing is
    // ever deferred for a live run.
    let live = HashSet::new();
    let decisions = triggers::evaluate(&EvalInput {
        armed: &snapshot.armed,
        states: &states,
        events: &snapshot.events[..],
        live_action_ids: &live,
        now_ms,
        now_local: snapshot.now_local.clone(),
    });
    let mut reseeded = false;
    let mut fires = Vec::new();
    for decision in decisions {
        match decision {
            // Cooling down — nothing to persist.
            Decision::Defer { .. } => {}
            Decision::Reseed {
                trigger_id,
                new_state,
            } => {
                states.insert(trigger_id, new_state);
                reseeded = true;
            }
            Decision::Fire {
                trigger_id,
                firing,
                new_state,
            } => {
                let Some(armed) = snapshot
                    .armed
                    .iter()
                    .find(|candidate| candidate.trigger_id == trigger_id)
                else {
                    continue;
                };
                let Some(raw) = snapshot.raw.get(&trigger_id) else {
                    continue;
                };
                fires.push(FireOrder {
                    agent_id: armed.action_id.clone(),
                    text: firing_message(raw, &firing, &snapshot),
                    trigger_id,
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

/// EXP-1102: the trigger state's own store, `{data_dir}/triggers/
/// <device_id>.json` — `settings_path` only names the data dir now.
fn store(settings_path: &Path, device_id: &str) -> triggers::TriggerStore {
    let data_dir = settings_path.parent().unwrap_or_else(|| Path::new("."));
    triggers::TriggerStore::open(data_dir, device_id)
}

fn write_states(
    settings_path: &Path,
    device_id: &str,
    states: &HashMap<String, TriggerState>,
) -> bool {
    match triggers::write_states(&store(settings_path, device_id), states) {
        Ok(()) => true,
        Err(err) => {
            log::warn!("[triggers] state write failed: {err}");
            false
        }
    }
}

/// EXP-1294: the message a firing posts into its agent's channel — the
/// trigger's own text (`coding::pickup::firing_text`) and, for an event
/// firing, one line per matched event (capped; the overflow is counted).
fn firing_message(raw: &serde_json::Value, firing: &Firing, snapshot: &EvalSnapshot) -> String {
    let mut text = coding::pickup::firing_text(raw);
    if let Firing::Event { matches } = firing {
        let mut lines: Vec<String> = matches
            .iter()
            .take(triggers::TRIGGER_PROMPT_MAX_LINES)
            .map(|row| format!("- {}", event_line(row, &snapshot.issue_display, &snapshot.label_names)))
            .collect();
        let omitted = matches.len().saturating_sub(triggers::TRIGGER_PROMPT_MAX_LINES);
        if omitted > 0 {
            lines.push(format!("- …and {omitted} more."));
        }
        if !text.is_empty() && !lines.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&lines.join("\n"));
    }
    text
}

/// One event's prompt line: `EXP-142 "Ship it" status Backlog → In Progress`.
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
// The post (background executor)
// ---------------------------------------------------------------------------

/// Post one firing into its agent's channel; a failed post backs the trigger
/// off ([`back_off`]).
fn fire_trigger(trpc: &api::TrpcClient, fire: FireOrder, settings_path: &Path, device_id: &str) {
    log::info!("[triggers] {} → agent {}", fire.trigger_id, fire.agent_id);
    if let Err(err) = api::session_device::post_message(trpc, &fire.agent_id, &fire.text) {
        log::warn!("[triggers] {} could not post: {err}", fire.trigger_id);
        back_off(settings_path, device_id, &fire.trigger_id);
    }
}

/// Extend the trigger's cooldown by the failure backoff. Read-modify-
/// write on the CURRENT map so a concurrent pass's reseeds survive; a state
/// that vanished meanwhile (trigger deleted) is left alone.
fn back_off(settings_path: &Path, device_id: &str, trigger_id: &str) {
    let mut states = triggers::read_states(&store(settings_path, device_id));
    let Some(state) = states.get_mut(trigger_id) else {
        return;
    };
    let now_ms = Local::now().timestamp_millis();
    let base = state.cooldown_until.unwrap_or(now_ms).max(now_ms);
    state.cooldown_until = Some(base + triggers::PREPARE_FAILURE_BACKOFF_MS);
    log::info!("[triggers] {trigger_id} backed off after a failed post");
    write_states(settings_path, device_id, &states);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// One synced agent root carrying `triggers`, hosted on `device_id`.
    fn agent_row(id: &str, team_id: Option<&str>, device_id: &str, triggers: serde_json::Value) -> SessionRow {
        let mut row = json!({
            "id": id, "kind": "agent", "root_id": id, "device_id": device_id,
            "name": id, "triggers": triggers,
        });
        if let Some(team_id) = team_id {
            row["team_id"] = json!(team_id);
        }
        serde_json::from_value(row).expect("session row decodes")
    }

    fn schedule_trigger() -> serde_json::Value {
        json!({"kind": "schedule", "interval": "daily", "minuteOfDay": 420})
    }

    /// One trigger element beside [`schedule_trigger`].
    fn trigger(id: &str, enabled: bool) -> serde_json::Value {
        let mut element = schedule_trigger();
        element["id"] = json!(id);
        element["enabled"] = json!(enabled);
        element
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

    /// EXP-1294: only the ENABLED, readable triggers of agents hosted on
    /// THIS device are evaluated — and the firing targets the agent.
    #[test]
    fn armed_triggers_keep_only_this_devices_agents() {
        let rows = vec![
            agent_row(
                "agent-1",
                Some("team-1"),
                "dev-gui",
                json!([
                    trigger("mine", true),
                    // Switched off — inert before the engine ever sees it.
                    trigger("off", false),
                    // A FUTURE kind is unreadable here: skipped.
                    {"id": "future", "kind": "moon_phase"},
                ]),
            ),
            // Another machine's agent.
            agent_row("agent-2", Some("team-1"), "dev-cli", json!([trigger("theirs", true)])),
            // A team-less agent has no event fence.
            agent_row("agent-3", None, "dev-gui", json!([trigger("teamless", true)])),
            agent_row("agent-4", Some("team-2"), "dev-gui", json!([trigger("second", true)])),
        ];
        let (bound, raw) = armed_triggers(rows.iter(), "dev-gui");
        assert_eq!(
            bound
                .iter()
                .map(|armed| armed.trigger_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mine", "second"],
        );
        assert_eq!(bound[0].action_id, "agent-1", "the firing targets the agent");
        assert_eq!(bound[0].team_id, "team-1");
        assert_eq!(bound[1].action_id, "agent-4");
        assert_eq!(raw.len(), 2);
        // The fingerprint is the WHEN-part's — toggling never re-seeds.
        assert_eq!(
            bound[0].fingerprint,
            triggers::trigger_fingerprint(&schedule_trigger()),
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
                Some(json!({"fromName": "Backlog", "toName": "In Progress"})),
            )),
            "EXP-142 \"Ship it\" status Backlog → In Progress"
        );
        // Pre-EXP-314 rows carry only the enum anchors — munged, not dropped.
        assert_eq!(
            line(&event_row(
                "e2",
                "status_changed",
                Some(json!({"from": "backlog", "to": "in_progress"})),
            )),
            "EXP-142 \"Ship it\" status backlog → in progress"
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

    /// The message cap: an event firing lists the first N lines and counts
    /// the rest, so a 24h catch-up burst can't blow the message open.
    #[test]
    fn event_message_caps_its_lines() {
        let snapshot = EvalSnapshot {
            settings_path: PathBuf::new(),
            device_id: "dev-gui".to_string(),
            armed: Vec::new(),
            raw: HashMap::new(),
            events: Arc::default(),
            issue_display: displays(),
            label_names: HashMap::new(),
            now_local: Local::now(),
        };
        let overflow = triggers::TRIGGER_PROMPT_MAX_LINES + 7;
        let matches: Vec<EventRow> = (0..overflow)
            .map(|index| event_row(&format!("e{index}"), "created", None))
            .collect();
        let text = firing_message(&json!({"kind": "event", "event": "created"}), &Firing::Event { matches }, &snapshot);
        let lines: Vec<&str> = text.lines().filter(|line| line.starts_with("- ")).collect();
        assert_eq!(lines.len(), triggers::TRIGGER_PROMPT_MAX_LINES + 1);
        assert_eq!(lines.last().copied(), Some("- …and 7 more."));
        assert_eq!(lines[0], "- EXP-142 \"Ship it\" created");
    }
}
