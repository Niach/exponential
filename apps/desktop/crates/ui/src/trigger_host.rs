//! EXP-530: the desktop GUI's trigger host — the CLI daemon worker's twin,
//! per signed-in account and shaped exactly like [`crate::device_sync`].
//!
//! EXP-1294 (Threads v2): triggers live on AGENTS — an agent root (`sessions`
//! kind `agent`) hosted on THIS device carries its `triggers` jsonb, and a
//! firing posts ONE message into that agent's channel
//! (`api::session_device::post_message`); the pickup delivers it like any
//! other message. Nothing here launches a run.
//!
//! The engine ([`coding::triggers`]) is PURE and the SAME one the daemon's
//! `trigger_pass` drives: [`coding::triggers::bound_agent_triggers`] arms,
//! `evaluate` decides, [`coding::triggers::firing_text`] renders. This host
//! snapshots its inputs on the foreground (this device's agent rows, the busy
//! agents, the candidate `issue_events` rows with `board_id` joined from the
//! synced issues, the display lookups, the clock) and runs `read_states` →
//! `evaluate` → `write_states` on the background executor, because every one
//! of those touches the state file.
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
//! extends `cooldown_until` by [`coding::triggers::SEND_FAILURE_BACKOFF_MS`],
//! so a poison-pill trigger backs off instead of re-firing every beat.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use gpui::{App, AppContext as _, Global};

use coding::triggers::{
    self, ArmedTrigger, Decision, EvalInput, EventLookups, EventRow, TriggerState,
};
use domain::rows::{MessageRow, SessionRow};

use crate::queries;

/// The evaluation cadence — a schedule's finest granularity is one minute,
/// so a half-minute beat can never miss an occurrence, and the collection
/// watches cover event triggers in realtime anyway.
const BEAT: Duration = Duration::from_secs(30);
/// The nudge-poll granularity inside the beat loop (device_sync's TICK).
const TICK: Duration = Duration::from_secs(1);
/// How long a posted firing counts its agent busy before the `messages`
/// shape shows it (the daemon's `FIRING_SYNC_GRACE`).
const FIRING_SYNC_GRACE: Duration = Duration::from_secs(10 * 60);

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
        // Firings this host posted that the `messages` shape has not shown
        // yet: agent id → (message id, posted at) — see [`busy_agent_ids`].
        let mut posted: HashMap<String, (String, Instant)> = HashMap::new();
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
            let Some(snapshot) =
                cx.update(|cx| snapshot_for(&account_id, &mut event_cache, &mut posted, cx))
            else {
                // Account switched away — retire; connect_account restarts.
                return;
            };
            if snapshot.armed.is_empty() {
                posted.clear();
                continue;
            }
            let data_dir = snapshot.data_dir.clone();
            let device_id = snapshot.device_id.clone();

            let outcome = cx
                .background_executor()
                .spawn(async move { evaluate_pass(snapshot) })
                .await;
            if outcome.fires.is_empty() {
                continue;
            }
            let Some(trpc) = cx.update(|cx| queries::trpc_client(cx).map(Arc::new)) else {
                continue;
            };
            for fire in outcome.fires {
                let trpc = Arc::clone(&trpc);
                let data_dir = data_dir.clone();
                let device_id = device_id.clone();
                let agent_id = fire.agent_id.clone();
                let message_id = cx
                    .background_executor()
                    .spawn(async move { fire_trigger(&trpc, fire, &data_dir, &device_id) })
                    .await;
                if let Some(message_id) = message_id {
                    posted.insert(agent_id, (message_id, Instant::now()));
                }
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
    data_dir: PathBuf,
    /// The GUI's steer device id — triggers bound to the CLI daemon's
    /// sibling row (or another machine) belong to that host, never this one.
    device_id: String,
    /// This device's enabled agent triggers
    /// ([`coding::triggers::bound_agent_triggers`]).
    armed: Vec<ArmedTrigger>,
    /// Agents that must not get another firing yet ([`busy_agent_ids`]).
    busy: HashSet<String>,
    /// Shared with [`EventCache`] — the pass only reads it.
    events: Arc<Vec<EventRow>>,
    /// The names the firing text renders.
    lookups: EventLookups,
    now_local: DateTime<Local>,
}

fn snapshot_for(
    account_id: &str,
    cache: &mut EventCache,
    posted: &mut HashMap<String, (String, Instant)>,
    cx: &mut App,
) -> Option<EvalSnapshot> {
    let account = queries::active_account(cx)?;
    if account.id != account_id {
        return None;
    }
    let auth = crate::session::AuthContext::global(cx);
    let data_dir = auth.data_dir.clone();
    let device_id = steer::persistent_device_id(&data_dir);
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let sessions = collections.sessions.read(cx);
    let armed = triggers::bound_agent_triggers(sessions.iter(), &device_id);
    let busy = if armed.is_empty() {
        HashSet::new()
    } else {
        let agents: HashSet<&str> = armed.iter().map(|armed| armed.agent_id.as_str()).collect();
        let rows: Vec<SessionRow> = sessions
            .iter()
            .filter(|row| agents.contains(row.id.as_str()))
            .cloned()
            .collect();
        let messages: Vec<MessageRow> = collections
            .messages
            .read(cx)
            .iter()
            .filter(|row| agents.contains(row.session_id.as_str()))
            .cloned()
            .collect();
        busy_agent_ids(&rows, &messages, posted, Instant::now())
    };
    if armed.is_empty() || !triggers::needs_events(&armed) {
        // Nothing bound to this device, or nothing that READS events (a
        // schedule-only device never scans) — skip the potentially large
        // event collection entirely, and let the cache go with it.
        *cache = EventCache::default();
        return Some(EvalSnapshot {
            data_dir,
            device_id,
            armed,
            busy,
            events: Arc::default(),
            lookups: EventLookups::default(),
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
                // agent's own team (a missing value conservatively never
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

    // The display lookups: the distinct issues inside the catch-up window
    // (a small map over an already-narrowed list, not a second scan) plus
    // the two small team-wide tables whole — the daemon's `event_lookups`.
    let mut lookup_issues: HashMap<String, (String, String)> = HashMap::new();
    for row in events.iter() {
        if lookup_issues.contains_key(&row.issue_id) {
            continue;
        }
        if let Some(issue) = issues.get(&row.issue_id) {
            lookup_issues.insert(
                row.issue_id.clone(),
                (issue.identifier.clone(), issue.title.clone()),
            );
        }
    }
    let lookups = EventLookups {
        issues: lookup_issues,
        labels: collections
            .labels
            .read(cx)
            .iter()
            .map(|label| (label.id.clone(), label.name.clone()))
            .collect(),
        statuses: collections
            .issue_statuses
            .read(cx)
            .iter()
            .map(|status| (status.id.clone(), status.name.clone()))
            .collect(),
    };

    Some(EvalSnapshot {
        data_dir,
        device_id,
        armed,
        busy,
        events,
        lookups,
        now_local,
    })
}

/// The agents that must not get another firing yet (the daemon's rule): a
/// turn in flight (`busy`), an unconsumed person message in its channel (an
/// earlier firing included, once synced), or a firing this host posted that
/// the `messages` shape has not shown yet ([`FIRING_SYNC_GRACE`]). `posted`
/// forgets what the shape now carries (its row decides) or what outlived the
/// grace.
fn busy_agent_ids(
    rows: &[SessionRow],
    messages: &[MessageRow],
    posted: &mut HashMap<String, (String, Instant)>,
    now: Instant,
) -> HashSet<String> {
    let synced: HashSet<&str> = messages.iter().map(|message| message.id.as_str()).collect();
    posted.retain(|_, (message_id, at)| {
        !synced.contains(message_id.as_str())
            && now.saturating_duration_since(*at) < FIRING_SYNC_GRACE
    });
    let mut busy: HashSet<String> = posted.keys().cloned().collect();
    busy.extend(
        rows.iter()
            .filter(|row| row.is_agent() && row.busy)
            .map(|row| row.id.clone()),
    );
    busy.extend(
        messages
            .iter()
            .filter(|message| message.is_queued())
            .map(|message| message.session_id.clone()),
    );
    busy
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

// ---------------------------------------------------------------------------
// The evaluation pass (background executor)
// ---------------------------------------------------------------------------

/// One trigger the engine decided to fire, its state already persisted and
/// its message pre-rendered (the display lookups live in the snapshot).
struct FireOrder {
    trigger_id: String,
    /// The agent session the message goes to.
    agent_id: String,
    text: String,
}

struct PassOutcome {
    fires: Vec<FireOrder>,
}

fn evaluate_pass(snapshot: EvalSnapshot) -> PassOutcome {
    let store = triggers::TriggerStore::open(&snapshot.data_dir, &snapshot.device_id);
    let now_ms = snapshot.now_local.timestamp_millis();
    let mut states = triggers::read_states(&store);
    let decisions = triggers::evaluate(&EvalInput {
        armed: &snapshot.armed,
        states: &states,
        events: &snapshot.events[..],
        busy_agent_ids: &snapshot.busy,
        now_ms,
        now_local: snapshot.now_local,
    });
    // Re-seeds are pure bookkeeping (a new or edited trigger anchoring
    // itself) — persist the whole batch in ONE write.
    let mut reseeded = false;
    for decision in &decisions {
        if let Decision::Reseed {
            trigger_id,
            new_state,
        } = decision
        {
            states.insert(trigger_id.clone(), new_state.clone());
            reseeded = true;
        }
    }
    if reseeded {
        write_states(&store, &states);
    }
    let mut fires = Vec::new();
    let mut fired_agents: HashSet<String> = HashSet::new();
    for decision in decisions {
        let Decision::Fire {
            trigger_id,
            firing,
            new_state,
        } = decision
        else {
            continue;
        };
        let Some(armed) = snapshot
            .armed
            .iter()
            .find(|candidate| candidate.trigger_id == trigger_id)
        else {
            continue;
        };
        // Two triggers of one agent in one pass: the second waits for the
        // next beat (no state written — it re-decides then).
        if fired_agents.contains(&armed.agent_id) {
            continue;
        }
        // Watermark-at-post (the firing protocol): persist before posting,
        // so a crash mid-post drops the firing instead of re-firing the same
        // events forever. A FAILED write skips the post too.
        states.insert(trigger_id.clone(), new_state);
        if !write_states(&store, &states) {
            continue;
        }
        fired_agents.insert(armed.agent_id.clone());
        fires.push(FireOrder {
            agent_id: armed.agent_id.clone(),
            text: triggers::firing_text(armed, &firing, &snapshot.lookups),
            trigger_id,
        });
    }
    PassOutcome { fires }
}

fn write_states(store: &triggers::TriggerStore, states: &HashMap<String, TriggerState>) -> bool {
    match triggers::write_states(store, states) {
        Ok(()) => true,
        Err(err) => {
            log::warn!("[triggers] state write failed: {err}");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// The post (background executor)
// ---------------------------------------------------------------------------

/// Post one firing into its agent's channel; returns the posted message id.
/// A failed post backs the trigger off ([`back_off`]).
fn fire_trigger(
    trpc: &api::TrpcClient,
    fire: FireOrder,
    data_dir: &std::path::Path,
    device_id: &str,
) -> Option<String> {
    match api::session_device::post_message(trpc, &fire.agent_id, &fire.text) {
        Ok(message_id) => {
            log::info!(
                "[triggers] {} fired into agent {} (message {message_id})",
                fire.trigger_id,
                fire.agent_id
            );
            Some(message_id)
        }
        Err(err) => {
            log::warn!("[triggers] {} could not post: {err}", fire.trigger_id);
            back_off(data_dir, device_id, &fire.trigger_id);
            None
        }
    }
}

/// Extend the trigger's cooldown by the failure backoff. Read-modify-write
/// on the CURRENT map so a concurrent pass's reseeds survive; a state that
/// vanished meanwhile (trigger deleted) is left alone.
fn back_off(data_dir: &std::path::Path, device_id: &str, trigger_id: &str) {
    let store = triggers::TriggerStore::open(data_dir, device_id);
    let mut states = triggers::read_states(&store);
    let Some(state) = states.get_mut(trigger_id) else {
        return;
    };
    let now_ms = Local::now().timestamp_millis();
    let base = state.cooldown_until.unwrap_or(now_ms).max(now_ms);
    state.cooldown_until = Some(base + triggers::SEND_FAILURE_BACKOFF_MS);
    log::info!("[triggers] {trigger_id} backed off after a failed post");
    write_states(&store, &states);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    /// An agent is busy mid-turn, with an unconsumed message, or with a
    /// posted firing the shape has not shown yet (within the grace).
    #[test]
    fn busy_agents_cover_turns_queues_and_unsynced_posts() {
        let agent = |id: &str, busy: bool| -> SessionRow {
            serde_json::from_value(json!({"id": id, "kind": "agent", "busy": busy}))
                .expect("session row decodes")
        };
        let rows = vec![agent("a-busy", true), agent("a-queued", false), agent("a-posted", false)];
        let messages: Vec<MessageRow> = vec![serde_json::from_value(json!({
            "id": "m-1", "session_id": "a-queued", "root_id": "a-queued", "author_kind": "user",
        }))
        .expect("message row decodes")];
        let now = Instant::now();
        let mut posted = HashMap::from([
            ("a-posted".to_string(), ("m-new".to_string(), now)),
            ("a-old".to_string(), ("m-old".to_string(), now - FIRING_SYNC_GRACE * 2)),
        ]);
        let busy = busy_agent_ids(&rows, &messages, &mut posted, now);
        let mut busy: Vec<&str> = busy.iter().map(String::as_str).collect();
        busy.sort_unstable();
        assert_eq!(busy, vec!["a-busy", "a-posted", "a-queued"]);
        assert!(!posted.contains_key("a-old"), "an expired post is forgotten");
    }
}
