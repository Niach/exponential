//! The ONLY gpui-aware module in this crate (masterplan-v3 §3.1 / §5.8) — the
//! reactive seam between the headless sync engine and the view tree.
//!
//! Design (§5.8, mirrored from §3.5's threading model):
//!
//! * **One `gpui::Entity<Collection<T>>` per shape** (34 entities), all held
//!   by the global [`Store`]. Separate entities give fine-grained
//!   `cx.notify()` — an issue update wakes only the issue-list views, not the
//!   label chips.
//! * The shape threads never touch gpui: they emit [`ShapeDelta`]s over the
//!   manager's flume channel after each batch commits to SQLite. **One
//!   foreground `cx.spawn` task** (spawned by [`Store::open`]) drains that
//!   channel, does cheap point-reads from the read-only SQLite connection
//!   ([`ShapeStore::read_by_key`]), updates the matching collection Entity,
//!   and calls `cx.notify()`.
//! * Views `cx.observe(&collection_entity)` and re-render on notify. Derived
//!   queries are plain Rust closures over the in-memory collections — no SQL
//!   at render time.
//!
//! The [`Store`] also owns the §5 **session state machine**:
//! `SignedOut → SigningIn → Synced / AuthExpired`. `AuthExpired` is the
//! hard-401 gate: a dead token ROUTES TO LOGIN — the shell renders
//! the login surface, never an empty board. Manager start/stop is wired to
//! the same transitions ([`Store::connect`] / [`Store::sign_out`] /
//! the drain's `Unauthorized` handling).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use gpui::{App, AppContext as _, AsyncApp, Entity, Global, Subscription};
use serde_json::{Map, Value};

use crate::activity::{not_ready_names, CatchUp, CATCHING_UP_WINDOW};
// Re-exported so `sync::collections::ShapeSyncPhase` keeps resolving after
// EXP-533 moved the model into the gpui-free `activity` module.
pub use crate::activity::{ShapeStatus, ShapeSyncPhase};
use crate::client::{ShapeDelta, UnauthorizedFn, UpgradeRequiredFn};
use crate::health::{AccountHealth, SyncHealth, FAILURE_STREAK_GRACE};
use crate::manager::{AccountSyncConfig, SyncManager};
use crate::protocol::RowKey;
use crate::shapes::{shape_by_name, ShapeSpec};
use crate::store::{ShapeStore, StoreError};

use domain::rows::{
    AgentDefRow, Attachment, Board, Comment, ComposerDraftRow, DeviceRow, DeviceWorktreeRow, Issue, IssueDraftRow,
    IssueEvent, IssueLabel, IssueRelation, IssueStatusRow, IssueSubscriber, Label, MessageRow, ConversationRow,
    Notification, Pin, PullRequestCheckRow, PullRequestIssueRow, PullRequestRow, SessionIssueRow, SessionReadRow,
    EventRow, PromptRow, SkillRow, AppRow, AppVersionRow, AppRecordRow, SessionRow, Team, TeamInvite, TeamMember, User,
};

// ---------------------------------------------------------------------------
// Session state machine (§5)
// ---------------------------------------------------------------------------

/// The app-level auth/sync session phase. Views branch the whole window on
/// this: anything but `Synced` renders the login surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionPhase {
    /// No account holds a session token — show login.
    SignedOut,
    /// A sign-in request is in flight — login surface shows a busy state.
    SigningIn,
    /// One account is connected: its pipeline runs and the collections are
    /// live. (Multi-account UI is Phase-3 territory; the engine underneath is
    /// already multi-account.)
    Synced { account_id: String },
    /// The session token was rejected mid-run. The pipeline is
    /// down and the token deleted — ROUTE TO LOGIN, never an empty board.
    AuthExpired { account_id: String },
}

impl SessionPhase {
    /// The account this phase is about, if any.
    pub fn account_id(&self) -> Option<&str> {
        match self {
            SessionPhase::Synced { account_id } | SessionPhase::AuthExpired { account_id } => {
                Some(account_id)
            }
            _ => None,
        }
    }
}

/// Cross-window shared state, held in a single `Entity` so every window's
/// views can `cx.observe` it and re-render on change. Carries the session
/// phase (the login-vs-board switch) and the live window count (the §3.10
/// multi-window shared-state proof — every sidebar renders it, so opening
/// window 2 visibly updates window 1).
pub struct SharedState {
    /// Number of shell windows currently open across the app.
    pub windows_open: usize,
    /// The §5 session state machine.
    pub session: SessionPhase,
    /// EXP-501: per-account poll health (keyed like iOS `accountHealth` —
    /// background accounts keep truthful entries, but only the ACTIVE
    /// account's entry may drive the offline banner).
    pub sync_health: HashMap<String, AccountHealth>,
    /// The last derived active-account health that was `cx.notify`'d. Dedupes
    /// notifies: the shape threads heartbeat `Applied` roughly per minute
    /// each, and re-rendering every `SharedState` observer on each would be
    /// pure churn — observers wake only on an Ok ⇄ Offline transition.
    pub published_health: SyncHealth,
    /// EXP-533: the in-flight pipeline-restart stamp behind the rail's sync
    /// spinner. Not per-account: only the active account's pipeline is ever
    /// restarted from a user-visible path, and the spinner speaks for the
    /// window.
    pub catch_up: CatchUp,
    /// Dedupe for the spinner exactly like [`Self::published_health`] — the
    /// per-shape success reports would otherwise wake every observer on every
    /// batch.
    pub published_catching_up: bool,
}

/// The active account's sync health, snapshot for render paths.
#[derive(Clone, Debug, Default)]
pub struct ActiveSyncStatus {
    pub health: SyncHealth,
    pub last_success_at: Option<SystemTime>,
    pub last_error: Option<String>,
    /// EXP-533: a restart is still working its way back to head, or a core
    /// shape has not reached it — the rail shows its sync spinner.
    pub catching_up: bool,
}

/// EXP-533: how stale the last successful poll must be before a window
/// activation restarts the pipeline. Comfortably past the server's ~60s hold
/// window, so a healthy idle app (one heartbeat per hold) never kicks.
pub const ACTIVATION_STALE: Duration = Duration::from_secs(90);

/// EXP-533: minimum spacing between activation kicks — window focus fires on
/// every alt-tab, and a restart is not free.
pub const KICK_DEBOUNCE: Duration = Duration::from_secs(5);

/// EXP-501: derive the banner-driving health for the CURRENT session. Only a
/// `Synced` session can be `Offline`: `AuthExpired` routes to login (which
/// owns the 401 story — the banner must never double-report it),
/// `SignedOut`/`SigningIn` have no pipeline, and the 426 update gate replaces
/// the whole window before any banner could render.
pub fn derive_active_health(
    session: &SessionPhase,
    health: &HashMap<String, AccountHealth>,
    now: SystemTime,
) -> SyncHealth {
    match session {
        SessionPhase::Synced { account_id } => health
            .get(account_id)
            .map_or(SyncHealth::Ok, |h| h.health(now)),
        _ => SyncHealth::Ok,
    }
}

// ---------------------------------------------------------------------------
// Per-shape reactive collections
// ---------------------------------------------------------------------------

/// A typed row hydratable from the store's snake_case JSON objects. The 34
/// impls below bind each `domain::rows` struct to its [`ShapeSpec`].
pub trait ShapeRow: serde::de::DeserializeOwned + Send + 'static {
    fn spec() -> &'static ShapeSpec;
    /// The row's primary key, matching the Electric wire key (§5.2).
    fn key(&self) -> RowKey;
    /// EXP-1459: opens the row's sealed content in place ([`crate::sealed`]).
    fn open_sealed(&mut self) {}
    /// EXP-1459: sealed content this install could not open (yet): re-read
    /// and re-opened when keys arrive ([`Store::reopen_sealed`]).
    fn seal_closed(&self) -> bool {
        false
    }
    /// #LOGS-11: the group the collection indexes the row under
    /// ([`Collection::in_group`]); `None` = none.
    fn group(&self) -> Option<&str> {
        None
    }
}

macro_rules! id_shape_row {
    ($ty:ty, $name:literal) => {
        impl ShapeRow for $ty {
            fn spec() -> &'static ShapeSpec {
                shape_by_name($name).expect("shape registered")
            }
            fn key(&self) -> RowKey {
                RowKey::Single(self.id.clone())
            }
        }
    };
}

id_shape_row!(Team, "teams");
id_shape_row!(Board, "boards");
id_shape_row!(Issue, "issues");
id_shape_row!(Label, "labels");
id_shape_row!(User, "users");
id_shape_row!(TeamMember, "team_members");
id_shape_row!(TeamInvite, "team_invites");
id_shape_row!(Comment, "comments");
id_shape_row!(Attachment, "attachments");
id_shape_row!(Notification, "notifications");
id_shape_row!(IssueEvent, "issue_events");
id_shape_row!(IssueSubscriber, "issue_subscribers");
id_shape_row!(SessionIssueRow, "session_issues");
id_shape_row!(PullRequestRow, "pull_requests");
id_shape_row!(PullRequestIssueRow, "pull_request_issues");
id_shape_row!(PullRequestCheckRow, "pull_request_checks");
id_shape_row!(IssueStatusRow, "issue_statuses");
id_shape_row!(DeviceRow, "devices");
id_shape_row!(DeviceWorktreeRow, "device_worktrees");
id_shape_row!(IssueRelation, "issue_relations");
id_shape_row!(Pin, "pins");
id_shape_row!(IssueDraftRow, "issue_drafts");
id_shape_row!(ComposerDraftRow, "composer_drafts");
id_shape_row!(SessionReadRow, "session_reads");
id_shape_row!(AgentDefRow, "agents");
id_shape_row!(EventRow, "events");
id_shape_row!(PromptRow, "prompts");
id_shape_row!(AppRow, "apps");
id_shape_row!(AppVersionRow, "app_versions");
id_shape_row!(AppRecordRow, "app_records");
id_shape_row!(SkillRow, "skills");

/// EXP-1459: the three shapes that carry sealed content.
macro_rules! sealed_shape_row {
    ($ty:ty, $name:literal, $open:ident, $closed:expr) => {
        sealed_shape_row!($ty, $name, $open, $closed, |_| None);
    };
    ($ty:ty, $name:literal, $open:ident, $closed:expr, $group:expr) => {
        impl ShapeRow for $ty {
            fn spec() -> &'static ShapeSpec {
                shape_by_name($name).expect("shape registered")
            }
            fn key(&self) -> RowKey {
                RowKey::Single(self.id.clone())
            }
            fn open_sealed(&mut self) {
                let _ = crate::sealed::Opener::global().$open(self);
            }
            fn seal_closed(&self) -> bool {
                let closed: fn(&$ty) -> bool = $closed;
                closed(self)
            }
            fn group(&self) -> Option<&str> {
                let group: fn(&$ty) -> Option<&str> = $group;
                group(self)
            }
        }
    };
}

// A title opens through the key cache, which tells no closed state: every
// session re-opens (one cheap pass over the trees).
sealed_shape_row!(SessionRow, "sessions", open_session, |_| true);
// #LOGS-11: indexed by their session (a chat reads its own rows, never all).
sealed_shape_row!(MessageRow, "messages", open_message, |row| domain::sealed::is_closed(row.seal), domain::message_session::message_session);
sealed_shape_row!(ConversationRow, "conversations", open_conversation, |row| {
    domain::chats::ConversationDigest::decode(row.digest.as_ref()).is_some_and(|digest| {
        [&digest.first, &digest.preview, &digest.waiting]
            .into_iter()
            .flatten()
            .chain(digest.waits.iter())
            .any(|message| domain::sealed::is_closed(message.seal))
    })
});

impl ShapeRow for IssueLabel {
    fn spec() -> &'static ShapeSpec {
        shape_by_name("issue_labels").expect("shape registered")
    }
    fn key(&self) -> RowKey {
        RowKey::Pair(self.issue_id.clone(), self.label_id.clone())
    }
}

/// In-memory reactive projection of one shape (§5.8): a `HashMap<RowKey, T>`
/// of hydrated `domain` structs plus a monotonic revision counter for cheap
/// diffing and the shape's sync phase.
///
/// `seeded` (EXP-470) is the optimistic overlay: rows built locally from a
/// mutation's own response so the UI can act on them before the Electric
/// echo arrives (a new team's row only lands after the shape-identity
/// rotation resolves — seconds). It is deliberately a SEPARATE map, memory
/// only, never SQLite: a post-409 refetch (or the startup hydrate) calls
/// [`Collection::replace_all`], and a snapshot computed before the server's
/// membership view caught up would silently truncate an optimistic row out
/// of `rows` — bouncing the UI back. The overlay survives every
/// `replace_all` and an entry is removed only by an authoritative signal for
/// its exact key (a sync upsert/delete, or a snapshot that contains the
/// key). Invariant: a key is never in both maps — `rows` wins.
pub struct Collection<T> {
    rows: HashMap<RowKey, T>,
    seeded: HashMap<RowKey, T>,
    /// #LOGS-11: [`ShapeRow::group`] → the keys of its rows (both maps),
    /// kept per write.
    groups: HashMap<String, HashSet<RowKey>>,
    /// #LOGS-11: the [`Collection::revision`] each group last changed at
    /// (a write in it, a row moving in or out); `group_floor` = the last
    /// wholesale write, which moves every group.
    group_revisions: HashMap<String, u64>,
    group_floor: u64,
    /// #LOGS-11: the last [`CHANGE_LOG`] row writes, by the revision they
    /// made; nothing before `log_floor` (a wholesale write) is told.
    log: std::collections::VecDeque<(u64, RowKey)>,
    log_floor: u64,
    phase: ShapeSyncPhase,
    revision: u64,
    /// S-3: the shape reached `up-to-date` in THIS process. The persisted
    /// phase reads `Live` straight from the SQLite cache at boot, rows as old
    /// as the last quit; this says the server has answered since.
    caught_up: bool,
}

impl<T> Collection<T> {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
            seeded: HashMap::new(),
            groups: HashMap::new(),
            group_revisions: HashMap::new(),
            group_floor: 0,
            log: std::collections::VecDeque::new(),
            log_floor: 0,
            phase: ShapeSyncPhase::Waiting,
            revision: 0,
            caught_up: false,
        }
    }

    /// S-3: the `caught_up` flag — fresh rows, not the cache's.
    pub fn is_caught_up(&self) -> bool {
        self.caught_up
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.rows.values().chain(self.seeded.values())
    }

    pub fn len(&self) -> usize {
        self.rows.len() + self.seeded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.seeded.is_empty()
    }

    /// Single-`id` lookup (composite-PK rows use [`Collection::get_key`]).
    pub fn get(&self, id: &str) -> Option<&T> {
        self.get_key(&RowKey::Single(id.to_string()))
    }

    pub fn get_key(&self, key: &RowKey) -> Option<&T> {
        self.rows.get(key).or_else(|| self.seeded.get(key))
    }

    /// True while the row exists only as an optimistic seed — sync has not
    /// yet confirmed it. Drives "setting up…" surfaces.
    pub fn is_seeded(&self, id: &str) -> bool {
        self.seeded.contains_key(&RowKey::Single(id.to_string()))
    }


    /// Monotonic change counter — bumps on every applied delta.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn phase(&self) -> ShapeSyncPhase {
        self.phase
    }

    /// §4.1 `is_ready`: the shape has caught up to its first `up-to-date` —
    /// an empty collection before this is "still syncing", never "no data"
    /// (the empty-snapshot-as-empty-state trap).
    pub fn is_ready(&self) -> bool {
        matches!(
            self.phase,
            ShapeSyncPhase::Live | ShapeSyncPhase::Refetching
        )
    }

    fn set_phase(&mut self, phase: ShapeSyncPhase) {
        self.phase = phase;
    }

}

impl<T: ShapeRow> Collection<T> {
    /// #LOGS-11: the rows [`ShapeRow::group`] puts under `group` (a chat's
    /// messages), store order; no scan of the collection.
    pub fn in_group<'a>(&'a self, group: &str) -> impl Iterator<Item = &'a T> + 'a {
        self.groups.get(group).into_iter().flatten().filter_map(|key| self.get_key(key))
    }

    /// #LOGS-11: the keys written since `revision` (each once, oldest
    /// first); `None` = not told that far back (a wholesale write, or more
    /// than [`CHANGE_LOG`] writes since).
    pub fn changed_since(&self, revision: u64) -> Option<Vec<&RowKey>> {
        if revision < self.log_floor {
            return None;
        }
        let first_told = self.log.front().map_or(self.revision + 1, |(at, _)| *at);
        if first_told > revision + 1 && revision < self.revision {
            return None;
        }
        let mut seen: HashSet<&RowKey> = HashSet::new();
        Some(self.log.iter().filter(|(at, _)| *at > revision).map(|(_, key)| key).filter(|key| seen.insert(*key)).collect())
    }

    fn note_write(&mut self, key: &RowKey) {
        if self.log.len() == CHANGE_LOG {
            self.log.pop_front();
        }
        self.log.push_back((self.revision, key.clone()));
    }

    /// #LOGS-11: when anything in `group` last changed ([`Self::revision`]
    /// terms): a reader of one group skips the writes to every other.
    pub fn group_revision(&self, group: &str) -> u64 {
        self.group_revisions.get(group).copied().unwrap_or(0).max(self.group_floor)
    }

    fn index(&mut self, key: &RowKey, row: &T) {
        if let Some(group) = row.group() {
            self.groups.entry(group.to_string()).or_default().insert(key.clone());
            // The write's revision (the caller bumps it right after).
            self.group_revisions.insert(group.to_string(), self.revision + 1);
        }
    }

    fn unindex(&mut self, key: &RowKey, row: &T) {
        let Some(group) = row.group() else { return };
        self.group_revisions.insert(group.to_string(), self.revision + 1);
        if let Some(keys) = self.groups.get_mut(group) {
            keys.remove(key);
            if keys.is_empty() {
                self.groups.remove(group);
            }
        }
    }

    /// Optimistically insert a locally-built row (EXP-470). No-op when sync
    /// already delivered the key.
    pub fn seed(&mut self, key: RowKey, row: T) {
        if self.rows.contains_key(&key) {
            return;
        }
        self.index(&key, &row);
        if let Some(old) = self.seeded.insert(key.clone(), row) {
            self.unindex_unless_same(&key, &old);
        }
        self.revision += 1;
        self.note_write(&key);
    }

    /// A delete as a synced batch lands it (tests of the readers that
    /// follow [`Self::changed_since`]).
    #[doc(hidden)]
    pub fn remove_for_test(&mut self, key: &RowKey) {
        self.remove(key);
    }

    /// An old row's group entry, unless the row that replaced it under
    /// `key` sits in the same group.
    fn unindex_unless_same(&mut self, key: &RowKey, old: &T) {
        let current = self.get_key(key).and_then(|row| row.group()).map(str::to_string);
        if old.group() != current.as_deref() {
            self.unindex(key, old);
        }
    }

    fn replace_all(&mut self, rows: Vec<(RowKey, T)>) {
        drop(self.install(Hydrated::build(rows)));
    }

    /// #PERF-9: a wholesale write from a table [`Hydrated::build`] read and
    /// indexed off the main thread: here only the maps swap (the seeded
    /// overlay, a handful of rows, is re-indexed). Returns what it replaced,
    /// for the caller to drop off the main thread too.
    fn install(&mut self, hydrated: Hydrated<T>) -> Hydrated<T> {
        let old = Hydrated { rows: std::mem::replace(&mut self.rows, hydrated.rows), groups: std::mem::replace(&mut self.groups, hydrated.groups) };
        // Only a snapshot that CONTAINS a seeded key confirms it; a snapshot
        // without it may predate the mutation (stale where clause) and must
        // not evict the optimistic row.
        self.seeded.retain(|key, _| !self.rows.contains_key(key));
        for (key, row) in &self.seeded {
            if let Some(group) = row.group() {
                self.groups.entry(group.to_string()).or_default().insert(key.clone());
            }
        }
        self.group_revisions.clear();
        self.group_floor = self.revision + 1;
        self.log.clear();
        self.log_floor = self.revision + 1;
        self.revision += 1;
        old
    }

    fn upsert(&mut self, key: RowKey, row: T) {
        self.index(&key, &row);
        let seeded = self.seeded.remove(&key);
        let old = self.rows.insert(key.clone(), row);
        for old in seeded.iter().chain(old.iter()) {
            self.unindex_unless_same(&key, old);
        }
        self.revision += 1;
        self.note_write(&key);
    }

    fn remove(&mut self, key: &RowKey) {
        let seeded = self.seeded.remove(key);
        let row = self.rows.remove(key);
        for old in seeded.iter().chain(row.iter()) {
            self.unindex(key, old);
        }
        if row.is_some() || seeded.is_some() {
            self.revision += 1;
            self.note_write(key);
        }
    }

    fn clear(&mut self) {
        self.rows.clear();
        self.seeded.clear();
        self.groups.clear();
        self.group_revisions.clear();
        self.group_floor = self.revision + 1;
        self.log.clear();
        self.log_floor = self.revision + 1;
        self.phase = ShapeSyncPhase::Waiting;
        self.revision += 1;
        self.caught_up = false;
    }
}

/// #PERF-9: a whole table read, decoded and grouped OFF the main thread;
/// [`Collection::install`] swaps it in. (46.6k messages took 0.65–1.8 s
/// to read, decode and index on the main thread at boot.)
pub(crate) struct Hydrated<T> {
    rows: HashMap<RowKey, T>,
    groups: HashMap<String, HashSet<RowKey>>,
}

impl<T: ShapeRow> Hydrated<T> {
    pub(crate) fn build(rows: Vec<(RowKey, T)>) -> Self {
        let rows: HashMap<RowKey, T> = rows.into_iter().collect();
        let mut groups: HashMap<String, HashSet<RowKey>> = HashMap::new();
        for (key, row) in &rows {
            if let Some(group) = row.group() {
                groups.entry(group.to_string()).or_default().insert(key.clone());
            }
        }
        Self { rows, groups }
    }

    /// The table of `T` from `sqlite`, ready to install (background).
    fn read(sqlite: &ShapeStore) -> Option<Self> {
        let spec = T::spec();
        let _span = tracing::debug_span!("sync.hydrate", shape = spec.name).entered();
        match sqlite.read_all(spec) {
            Ok(maps) => Some(Self::build(decode_rows::<T>(maps))),
            Err(err) => {
                log::warn!("[sync {}] hydrate read failed: {err}", spec.name);
                None
            }
        }
    }
}

/// #PERF-9: the shapes whose startup hydrate runs off the main thread
/// ([`Collections::hydrate_all`]): 15–400 ms each on a real store (47k
/// messages, 34k PR checks, 26k issue events, 9k events, 3k issues;
/// sessions and conversations open sealed content per row). The rest (a
/// few ms together) stay on the foreground so the first frame paints the
/// last-known teams and boards; readers of these check `is_ready`.
const HYDRATE_OFF_MAIN: &[&str] = &["messages", "conversations", "sessions", "issues", "issue_events", "pull_request_checks", "events"];

/// #PERF-9: a heavy shape read off the main thread, ready to swap in
/// (main thread); answers what it replaced, to drop off the main thread
/// (a dropped installer frees its rows wherever it is dropped).
type Installer = Box<dyn FnOnce(&mut App) -> Box<dyn Send> + Send>;

fn read_off_main<T: ShapeRow>(entity: &Entity<Collection<T>>, sqlite: &Arc<ShapeStore>, cx: &mut App) -> gpui::Task<Option<Installer>> {
    let (entity, sqlite) = (entity.clone(), Arc::clone(sqlite));
    cx.background_executor().spawn(async move {
        let name = T::spec().name;
        let rows = Hydrated::<T>::read(&sqlite)?;
        let phase = phase_from_store(&sqlite, name);
        let install: Installer = Box::new(move |cx: &mut App| {
            let replaced = entity.update(cx, |collection, cx| {
                let replaced = collection.install(rows);
                collection.set_phase(phase);
                cx.notify();
                replaced
            });
            Box::new(replaced) as Box<dyn Send>
        });
        Some(install)
    })
}

fn entity_spec<T: ShapeRow>(_: &Entity<Collection<T>>) -> &'static ShapeSpec {
    T::spec()
}

/// #LOGS-11: how many row writes a collection tells
/// ([`Collection::changed_since`]).
const CHANGE_LOG: usize = 1024;

/// Decode store-hydrated JSON objects into typed rows, dropping (and logging)
/// anything unhydratable — §5.5: a bad value is caught at hydrate, never at
/// apply, and never takes the batch down.
pub fn decode_rows<T: ShapeRow>(maps: Vec<Map<String, Value>>) -> Vec<(RowKey, T)> {
    maps.into_iter()
        .filter_map(
            |map| match serde_json::from_value::<T>(Value::Object(map)) {
                Ok(mut row) => {
                    row.open_sealed();
                    Some((row.key(), row))
                }
                Err(err) => {
                    log::warn!("[sync {}] dropping unhydratable row: {err}", T::spec().name);
                    None
                }
            },
        )
        .collect()
}

/// The 34 collection entities (§5.8). Cloning is cheap — `Entity` handles.
#[derive(Clone)]
pub struct Collections {
    pub teams: Entity<Collection<Team>>,
    pub boards: Entity<Collection<Board>>,
    pub issues: Entity<Collection<Issue>>,
    pub labels: Entity<Collection<Label>>,
    pub issue_labels: Entity<Collection<IssueLabel>>,
    pub users: Entity<Collection<User>>,
    pub team_members: Entity<Collection<TeamMember>>,
    pub team_invites: Entity<Collection<TeamInvite>>,
    pub comments: Entity<Collection<Comment>>,
    pub attachments: Entity<Collection<Attachment>>,
    pub notifications: Entity<Collection<Notification>>,
    pub issue_events: Entity<Collection<IssueEvent>>,
    pub issue_subscribers: Entity<Collection<IssueSubscriber>>,
    /// EXP-1294 THE session tree (roots `thread` | `agent` + children).
    pub sessions: Entity<Collection<SessionRow>>,
    /// EXP-1294 root session ↔ issue links.
    pub session_issues: Entity<Collection<SessionIssueRow>>,
    /// EXP-1294 every session's conversation (`consumed_at` NULL = queued).
    pub messages: Entity<Collection<MessageRow>>,
    /// NEW-8 the conversations messages live in (the 31st shape): a
    /// message's session is its conversation's
    /// (`domain::message_session`).
    pub conversations: Entity<Collection<ConversationRow>>,
    /// EXP-1294 every pull request (PR state left `issues`).
    pub pull_requests: Entity<Collection<PullRequestRow>>,
    /// EXP-1294 pull request ↔ issue links.
    pub pull_request_issues: Entity<Collection<PullRequestIssueRow>>,
    /// #EXP-1610 CI checks at each PR's head (the 33rd shape): the Merge
    /// pill and the chat header's Checks submenu (#EXP-1611).
    pub pull_request_checks: Entity<Collection<PullRequestCheckRow>>,
    /// EXP-314 per-team custom issue statuses (the 16th shape).
    pub issue_statuses: Entity<Collection<IssueStatusRow>>,
    /// EXP-481 per-user device registry (the 17th shape).
    pub devices: Entity<Collection<DeviceRow>>,
    /// EXP-481 per-device worktree inventory (the 18th shape).
    pub device_worktrees: Entity<Collection<DeviceWorktreeRow>>,
    /// EXP-736 issue relations (the 20th shape) — blocks / parent /
    /// duplicate / related, scoped by the SOURCE issue's board.
    pub issue_relations: Entity<Collection<IssueRelation>>,
    /// EXP-778 personal pins (the 21st shape) — per-user, never team/trash
    /// scoped; the rail filters to the active team and to resolvable targets.
    pub pins: Entity<Collection<Pin>>,
    /// EXP-878 unfiled create-issue dialogs (the 22nd shape) — per-user and
    /// static like `pins`; the Drafts page filters to the active team and to
    /// rows whose board still resolves.
    pub issue_drafts: Entity<Collection<IssueDraftRow>>,
    /// #FEED-219 every composer's synced draft (the 35th shape) — per-user
    /// and static like `issue_drafts`; keyed by `<kind>:<id>`.
    pub composer_drafts: Entity<Collection<ComposerDraftRow>>,
    /// EXP-1323 the viewer's session read marks (the 26th shape) — per-user
    /// and static like `pins`; unread titles + the Chats nav dot.
    pub session_reads: Entity<Collection<SessionReadRow>>,
    /// NEW-2 the team's agent DEFINITIONS (the 27th shape).
    pub agents: Entity<Collection<AgentDefRow>>,
    /// NEW-2 the team's event stream (the 28th shape).
    pub events: Entity<Collection<EventRow>>,
    /// NEW-2 saved prompts, body-less (the 29th shape).
    pub prompts: Entity<Collection<PromptRow>>,
    /// VAPP-110 the team's apps, the head package riding along (the 30th
    /// shape).
    pub apps: Entity<Collection<AppRow>>,
    /// VAPP-110 the apps' versions, package-less (the 31st shape).
    pub app_versions: Entity<Collection<AppVersionRow>>,
    /// VAPP-122 the apps' records, their own data (the 32nd shape).
    pub app_records: Entity<Collection<AppRecordRow>>,
    /// #EXP-1656 the team's skills, body-less (the 34th shape).
    pub skills: Entity<Collection<SkillRow>>,
    /// #PERF-9: the heavy shapes ([`HYDRATE_OFF_MAIN`]) whose startup
    /// hydrate is still being read off the main thread; a delta of one waits
    /// for its install ([`Self::hydrated`]). Disconnects = installed.
    hydrating: Arc<Mutex<HashMap<&'static str, flume::Receiver<()>>>>,
    /// #PERF-9: bumped by every hydrate and clear: a hydrate read under an
    /// older one (the account switched or signed out meanwhile) is dropped.
    generation: Arc<std::sync::atomic::AtomicU64>,
}

/// Run `$body` once per shape with `$entity` bound to that shape's collection
/// entity — the single dispatch point that keeps the 34-way fan-out in one
/// place.
macro_rules! for_each_collection {
    ($collections:expr, $entity:ident => $body:expr) => {{
        let $entity = &$collections.teams;
        $body;
        let $entity = &$collections.boards;
        $body;
        let $entity = &$collections.issues;
        $body;
        let $entity = &$collections.labels;
        $body;
        let $entity = &$collections.issue_labels;
        $body;
        let $entity = &$collections.users;
        $body;
        let $entity = &$collections.team_members;
        $body;
        let $entity = &$collections.team_invites;
        $body;
        let $entity = &$collections.comments;
        $body;
        let $entity = &$collections.attachments;
        $body;
        let $entity = &$collections.notifications;
        $body;
        let $entity = &$collections.issue_events;
        $body;
        let $entity = &$collections.issue_subscribers;
        $body;
        let $entity = &$collections.sessions;
        $body;
        let $entity = &$collections.session_issues;
        $body;
        let $entity = &$collections.messages;
        $body;
        let $entity = &$collections.conversations;
        $body;
        let $entity = &$collections.pull_requests;
        $body;
        let $entity = &$collections.pull_request_issues;
        $body;
        let $entity = &$collections.pull_request_checks;
        $body;
        let $entity = &$collections.issue_statuses;
        $body;
        let $entity = &$collections.devices;
        $body;
        let $entity = &$collections.device_worktrees;
        $body;
        let $entity = &$collections.issue_relations;
        $body;
        let $entity = &$collections.pins;
        $body;
        let $entity = &$collections.issue_drafts;
        $body;
        let $entity = &$collections.composer_drafts;
        $body;
        let $entity = &$collections.session_reads;
        $body;
        let $entity = &$collections.agents;
        $body;
        let $entity = &$collections.events;
        $body;
        let $entity = &$collections.prompts;
        $body;
        let $entity = &$collections.apps;
        $body;
        let $entity = &$collections.app_versions;
        $body;
        let $entity = &$collections.app_records;
        $body;
        let $entity = &$collections.skills;
        $body;
    }};
}

impl Collections {
    fn new(cx: &mut App) -> Self {
        Self {
            teams: cx.new(|_| Collection::new()),
            boards: cx.new(|_| Collection::new()),
            issues: cx.new(|_| Collection::new()),
            labels: cx.new(|_| Collection::new()),
            issue_labels: cx.new(|_| Collection::new()),
            users: cx.new(|_| Collection::new()),
            team_members: cx.new(|_| Collection::new()),
            team_invites: cx.new(|_| Collection::new()),
            comments: cx.new(|_| Collection::new()),
            attachments: cx.new(|_| Collection::new()),
            notifications: cx.new(|_| Collection::new()),
            issue_events: cx.new(|_| Collection::new()),
            issue_subscribers: cx.new(|_| Collection::new()),
            sessions: cx.new(|_| Collection::new()),
            session_issues: cx.new(|_| Collection::new()),
            messages: cx.new(|_| Collection::new()),
            conversations: cx.new(|_| Collection::new()),
            pull_requests: cx.new(|_| Collection::new()),
            pull_request_issues: cx.new(|_| Collection::new()),
            pull_request_checks: cx.new(|_| Collection::new()),
            issue_statuses: cx.new(|_| Collection::new()),
            devices: cx.new(|_| Collection::new()),
            device_worktrees: cx.new(|_| Collection::new()),
            issue_relations: cx.new(|_| Collection::new()),
            pins: cx.new(|_| Collection::new()),
            issue_drafts: cx.new(|_| Collection::new()),
            composer_drafts: cx.new(|_| Collection::new()),
            session_reads: cx.new(|_| Collection::new()),
            agents: cx.new(|_| Collection::new()),
            events: cx.new(|_| Collection::new()),
            prompts: cx.new(|_| Collection::new()),
            apps: cx.new(|_| Collection::new()),
            app_versions: cx.new(|_| Collection::new()),
            app_records: cx.new(|_| Collection::new()),
            skills: cx.new(|_| Collection::new()),
            hydrating: Arc::default(),
            generation: Arc::default(),
        }
    }

    /// #PERF-9: resolves once `shape`'s startup hydrate is installed (at
    /// once when none is pending). The delta drain awaits it before reading
    /// a batch, so no batch commits between a hydrate's snapshot read and
    /// its install (the race the synchronous hydrate used to rule out).
    pub(crate) async fn hydrated(&self, shape: &str) {
        let pending = self.hydrating.lock().expect("hydrating poisoned").get(shape).cloned();
        if let Some(pending) = pending {
            let _ = pending.recv_async().await;
        }
    }

    /// #PERF-9: every pending startup hydrate is installed (benches, tests,
    /// and readers that must see the whole store).
    pub async fn all_hydrated(&self) {
        let pending: Vec<flume::Receiver<()>> = self.hydrating.lock().expect("hydrating poisoned").values().cloned().collect();
        for pending in pending {
            let _ = pending.recv_async().await;
        }
    }

    /// #PERF-9 (benches): re-read `shape` and install it as the boot's
    /// off-main hydrate does, the read done before; the caller times the
    /// install and what it wakes. Answers the read's milliseconds.
    #[doc(hidden)]
    pub fn reinstall_for_bench(&self, shape: &str, sqlite: &ShapeStore, cx: &mut App) -> f64 {
        fn one<T: ShapeRow>(entity: &Entity<Collection<T>>, sqlite: &ShapeStore, cx: &mut App) -> f64 {
            let started = Instant::now();
            let rows = Hydrated::<T>::read(sqlite).expect("the table");
            let read = started.elapsed().as_secs_f64() * 1e3;
            let replaced = entity.update(cx, |collection, cx| {
                let replaced = collection.install(rows);
                cx.notify();
                replaced
            });
            std::mem::forget(replaced);
            read
        }
        match shape {
            "messages" => one(&self.messages, sqlite, cx),
            "sessions" => one(&self.sessions, sqlite, cx),
            "conversations" => one(&self.conversations, sqlite, cx),
            "issues" => one(&self.issues, sqlite, cx),
            "issue_events" => one(&self.issue_events, sqlite, cx),
            "pull_request_checks" => one(&self.pull_request_checks, sqlite, cx),
            "events" => one(&self.events, sqlite, cx),
            other => panic!("not a heavy shape: {other}"),
        }
    }

    /// #PERF-9: no startup hydrate is pending for `shape` (its collection
    /// holds the store's rows, not an empty placeholder).
    pub fn is_hydrated(&self, shape: &str) -> bool {
        self.hydrating.lock().expect("hydrating poisoned").get(shape).is_none_or(|pending| pending.is_disconnected())
    }

    /// Apply one committed batch's keys to the matching typed collection
    /// (foreground; point-reads from the read-only WAL connection, §5.8).
    /// #LOGS-3: read and decode one batch of `shape` (call it OFF the main
    /// thread); `None` = nothing to commit.
    pub(crate) fn prepare(
        shape: &str,
        keys: &[RowKey],
        full_replace: bool,
        up_to_date: bool,
        sqlite: &ShapeStore,
    ) -> Option<Prepared> {
        match shape {
            "teams" => prepare_for(|c| &c.teams, keys, full_replace, up_to_date, sqlite),
            "boards" => prepare_for(|c| &c.boards, keys, full_replace, up_to_date, sqlite),
            "issues" => prepare_for(|c| &c.issues, keys, full_replace, up_to_date, sqlite),
            "labels" => prepare_for(|c| &c.labels, keys, full_replace, up_to_date, sqlite),
            "issue_labels" => prepare_for(|c| &c.issue_labels, keys, full_replace, up_to_date, sqlite),
            "users" => prepare_for(|c| &c.users, keys, full_replace, up_to_date, sqlite),
            "team_members" => prepare_for(|c| &c.team_members, keys, full_replace, up_to_date, sqlite),
            "team_invites" => prepare_for(|c| &c.team_invites, keys, full_replace, up_to_date, sqlite),
            "comments" => prepare_for(|c| &c.comments, keys, full_replace, up_to_date, sqlite),
            "attachments" => prepare_for(|c| &c.attachments, keys, full_replace, up_to_date, sqlite),
            "notifications" => prepare_for(|c| &c.notifications, keys, full_replace, up_to_date, sqlite),
            "issue_events" => prepare_for(|c| &c.issue_events, keys, full_replace, up_to_date, sqlite),
            "issue_subscribers" => prepare_for(|c| &c.issue_subscribers, keys, full_replace, up_to_date, sqlite),
            "sessions" => prepare_for(|c| &c.sessions, keys, full_replace, up_to_date, sqlite),
            "session_issues" => prepare_for(|c| &c.session_issues, keys, full_replace, up_to_date, sqlite),
            "messages" => prepare_for(|c| &c.messages, keys, full_replace, up_to_date, sqlite),
            "conversations" => prepare_for(|c| &c.conversations, keys, full_replace, up_to_date, sqlite),
            "pull_requests" => prepare_for(|c| &c.pull_requests, keys, full_replace, up_to_date, sqlite),
            "pull_request_issues" => prepare_for(|c| &c.pull_request_issues, keys, full_replace, up_to_date, sqlite),
            "pull_request_checks" => prepare_for(|c| &c.pull_request_checks, keys, full_replace, up_to_date, sqlite),
            "issue_statuses" => prepare_for(|c| &c.issue_statuses, keys, full_replace, up_to_date, sqlite),
            "devices" => prepare_for(|c| &c.devices, keys, full_replace, up_to_date, sqlite),
            "device_worktrees" => prepare_for(|c| &c.device_worktrees, keys, full_replace, up_to_date, sqlite),
            "issue_relations" => prepare_for(|c| &c.issue_relations, keys, full_replace, up_to_date, sqlite),
            "pins" => prepare_for(|c| &c.pins, keys, full_replace, up_to_date, sqlite),
            "issue_drafts" => prepare_for(|c| &c.issue_drafts, keys, full_replace, up_to_date, sqlite),
            "composer_drafts" => prepare_for(|c| &c.composer_drafts, keys, full_replace, up_to_date, sqlite),
            "session_reads" => prepare_for(|c| &c.session_reads, keys, full_replace, up_to_date, sqlite),
            "agents" => prepare_for(|c| &c.agents, keys, full_replace, up_to_date, sqlite),
            "events" => prepare_for(|c| &c.events, keys, full_replace, up_to_date, sqlite),
            "prompts" => prepare_for(|c| &c.prompts, keys, full_replace, up_to_date, sqlite),
            "apps" => prepare_for(|c| &c.apps, keys, full_replace, up_to_date, sqlite),
            "app_versions" => prepare_for(|c| &c.app_versions, keys, full_replace, up_to_date, sqlite),
            "app_records" => prepare_for(|c| &c.app_records, keys, full_replace, up_to_date, sqlite),
            "skills" => prepare_for(|c| &c.skills, keys, full_replace, up_to_date, sqlite),
            other => {
                log::warn!("[sync] delta for unknown shape {other}");
                None
            }
        }
    }

    /// Commit a [`Self::prepare`]d batch of `shape` (main thread).
    pub(crate) fn commit(&self, shape: &str, prepared: Prepared, sqlite: &Arc<ShapeStore>, cx: &mut AsyncApp) {
        prepared(self, cx);
        if shape == "sessions" {
            // EXP-1459: a digest opened before its session's row arrived
            // learns its root now. #PERF-1: its reads run off the main thread
            // (they share the reader lock with the delta reads).
            if crate::sealed::Opener::global().take_unresolved() {
                let (conversations, sqlite) = (self.conversations.clone(), Arc::clone(sqlite));
                cx.spawn(async move |cx| reopen_closed(&conversations, &sqlite, cx).await).detach();
            }
        }
    }

    /// Read, decode and commit one batch; the read runs on the background
    /// executor while the main thread waits on nothing.
    pub(crate) async fn apply(&self, shape: &str, keys: Vec<RowKey>, full_replace: bool, up_to_date: bool, sqlite: &Arc<ShapeStore>, cx: &mut AsyncApp) {
        self.hydrated(shape).await;
        let read = {
            let (shape, sqlite) = (shape.to_string(), Arc::clone(sqlite));
            cx.background_executor().spawn(async move { Self::prepare(&shape, &keys, full_replace, up_to_date, &sqlite) })
        };
        if let Some(prepared) = read.await {
            self.commit(shape, prepared, sqlite, cx);
        }
    }

    /// Full hydrate of all 34 collections from SQLite (§5.8 "hydrate typed
    /// in-memory collections from SQLite at startup"). The light shapes run
    /// synchronously on the foreground, so the first frame paints the
    /// last-known board: every batch committed to SQLite has a matching
    /// [`ShapeDelta`] queued behind this call, so a snapshot read here can
    /// never lose a concurrent write (the delta re-reads the row right
    /// after). #PERF-9: the heavy ones ([`HYDRATE_OFF_MAIN`]: 46.6k messages
    /// held the main thread 0.65–1.8 s) are read, decoded and indexed on the
    /// background executor and swapped in; their deltas wait for the swap
    /// ([`Self::hydrated`]), which keeps the same guarantee. Until then the
    /// collection stays empty and `Waiting` (not ready).
    fn hydrate_all(&self, sqlite: &Arc<ShapeStore>, cx: &mut App) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        for_each_collection!(self, entity => {
            let name = entity_spec(entity).name;
            if HYDRATE_OFF_MAIN.contains(&name) {
                self.hydrate_off_main(name, read_off_main(entity, sqlite, cx), sqlite, generation, cx);
            } else {
                hydrate_collection(entity, sqlite, cx);
            }
        });
    }

    /// One heavy shape: its read runs on the background executor (all of
    /// them in parallel), its install is ONE short main-thread update of its
    /// own. (Installing them all in one update woke every derived view over
    /// the whole store at once: 115–145 ms on a real store, against ≤ 41 ms
    /// for the largest single shape.)
    fn hydrate_off_main(&self, name: &'static str, read: gpui::Task<Option<Installer>>, sqlite: &Arc<ShapeStore>, generation: u64, cx: &mut App) {
        let (done, pending) = flume::bounded::<()>(1);
        self.hydrating.lock().expect("hydrating poisoned").insert(name, pending);
        let (collections, sqlite, current) = (self.clone(), Arc::clone(sqlite), Arc::clone(&self.generation));
        cx.spawn(async move |cx| {
            let Some(install) = read.await else {
                drop(done);
                return;
            };
            if current.load(Ordering::SeqCst) != generation {
                // Signed out or switched accounts while it read.
                cx.background_executor().spawn(async move { drop(install) }).detach();
                drop(done);
                return;
            }
            let replaced = cx.update(|cx| {
                let _span = tracing::debug_span!("sync.install", shape = name).entered();
                install(cx)
            });
            cx.background_executor().spawn(async move { drop(replaced) }).detach();
            drop(done);
            // EXP-1459: keys that arrived while it read re-open what stayed
            // closed (the reopen pass then saw an empty collection).
            match name {
                "sessions" => reopen_closed(&collections.sessions, &sqlite, cx).await,
                "messages" => reopen_closed(&collections.messages, &sqlite, cx).await,
                "conversations" => reopen_closed(&collections.conversations, &sqlite, cx).await,
                _ => {}
            }
        })
        .detach();
    }

    fn clear_all(&self, cx: &mut App) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.hydrating.lock().expect("hydrating poisoned").clear();
        for_each_collection!(self, entity => entity.update(cx, |collection, cx| {
            collection.clear();
            cx.notify();
        }));
    }

    fn statuses(&self, cx: &App) -> Vec<ShapeStatus> {
        let mut out = Vec::with_capacity(crate::shapes::SHAPES.len());
        for_each_collection!(self, entity => out.push(status_of(entity, cx)));
        out
    }

    fn observe_all<V: 'static>(&self, cx: &mut gpui::Context<V>) -> Vec<Subscription> {
        self.observe_all_with(cx, |_, cx| cx.notify())
    }

    fn observe_all_except<V: 'static>(&self, skip: &'static [&'static str], cx: &mut gpui::Context<V>) -> Vec<Subscription> {
        fn observe<T: ShapeRow, V: 'static>(entity: &Entity<Collection<T>>, skip: &[&str], cx: &mut gpui::Context<V>) -> Option<Subscription> {
            (!skip.contains(&T::spec().name)).then(|| cx.observe(entity, |_, _, cx| cx.notify()))
        }
        let mut out = Vec::with_capacity(crate::shapes::SHAPES.len());
        for_each_collection!(self, entity => out.extend(observe(entity, skip, cx)));
        out
    }

    /// Run `on_change` on `V` whenever ANY of the 34 collections notifies
    /// (EXP-1326: the IDE's cached dock area re-renders on every synced
    /// change this way, so a screen reading a collection without observing
    /// it never goes stale behind the cache).
    pub fn observe_all_with<V: 'static>(
        &self,
        cx: &mut gpui::Context<V>,
        on_change: impl Fn(&mut V, &mut gpui::Context<V>) + Clone + 'static,
    ) -> Vec<Subscription> {
        let mut out = Vec::with_capacity(crate::shapes::SHAPES.len());
        for_each_collection!(self, entity => {
            let on_change = on_change.clone();
            out.push(cx.observe(entity, move |this, _, cx| on_change(this, cx)))
        });
        out
    }

    /// #LOGS-11: every collection's revision but those of the shapes named
    /// in `skip`, for a view that derives from many of them: an equal
    /// answer = nothing it reads moved.
    pub fn revisions_except(&self, skip: &[&str], cx: &App) -> Vec<u64> {
        fn revision<T: ShapeRow>(entity: &Entity<Collection<T>>, skip: &[&str], cx: &App) -> Option<u64> {
            (!skip.contains(&T::spec().name)).then(|| entity.read(cx).revision())
        }
        let mut out = Vec::with_capacity(crate::shapes::SHAPES.len());
        for_each_collection!(self, entity => out.extend(revision(entity, skip, cx)));
        out
    }
}

impl Collections {
    /// Optimistically seed a team row from a mutation response (EXP-470) —
    /// the create/join flows switch to the team immediately instead of
    /// waiting out the Electric shape-identity rotation.
    pub fn seed_team(&self, team: Team, cx: &mut App) {
        self.teams.update(cx, |collection, cx| {
            collection.seed(team.key(), team);
            cx.notify();
        });
    }

    /// Optimistically seed a board row from a mutation response (EXP-470) —
    /// covers the onboarding wizard's board step in a just-created team,
    /// whose boards shape has not rotated yet.
    pub fn seed_board(&self, board: Board, cx: &mut App) {
        self.boards.update(cx, |collection, cx| {
            collection.seed(board.key(), board);
            cx.notify();
        });
    }

    // -- team-scoped query helpers (§5.8: derived queries are plain Rust
    // over the in-memory collections; §4.1 moves the full set into
    // `ui/src/queries.rs` with the Phase-3 screens) ---------------------------

    /// All teams, name-sorted (the sidebar picker's read).
    pub fn teams_sorted(&self, cx: &App) -> Vec<Team> {
        let mut out: Vec<Team> = self.teams.read(cx).iter().cloned().collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    }

    /// A team's boards in the canonical cross-client order (EXP-525):
    /// `sort_order` asc with NULLS LAST, tie-broken by `created_at` asc —
    /// byte-identical to the web sidebar's comparator (`use-team-data.ts`),
    /// so the rail and the web sidebar always list boards identically. (The
    /// old name tiebreak diverged from the web for null sort orders.)
    pub fn boards_in_team(&self, team_id: &str, cx: &App) -> Vec<Board> {
        let mut out: Vec<Board> = self
            .boards
            .read(cx)
            .iter()
            .filter(|p| p.team_id == team_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            a.sort_order
                .unwrap_or(f64::MAX)
                .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        out
    }

    /// A board's issues, sort-order-then-identifier sorted (the board's base
    /// query; status grouping/filters sit on top).
    pub fn issues_in_board(&self, board_id: &str, cx: &App) -> Vec<Issue> {
        let mut out: Vec<Issue> = self
            .issues
            .read(cx)
            .iter()
            .filter(|i| i.board_id == board_id)
            .cloned()
            .collect();
        sort_issues(&mut out);
        out
    }

    /// Every issue in a team (joins through the boards collection —
    /// referential integrity is a query-time concern, §5.4).
    pub fn issues_in_team(&self, team_id: &str, cx: &App) -> Vec<Issue> {
        let boards = self.boards.read(cx);
        let board_ids: std::collections::HashSet<&str> = boards
            .iter()
            .filter(|p| p.team_id == team_id)
            .map(|p| p.id.as_str())
            .collect();
        let mut out: Vec<Issue> = self
            .issues
            .read(cx)
            .iter()
            .filter(|i| board_ids.contains(i.board_id.as_str()))
            .cloned()
            .collect();
        sort_issues(&mut out);
        out
    }

    /// EXP-868: [`Self::issues_in_team`] without the clone and the sort — a
    /// BORROWED, unordered view for the hot paths (render-time pickers,
    /// per-keystroke snapshots) that filter or project before they own
    /// anything. Cloning every issue (descriptions included) there made the
    /// whole window stutter on each redraw.
    pub fn issue_refs_in_team<'a>(&'a self, team_id: &str, cx: &'a App) -> Vec<&'a Issue> {
        let board_ids: std::collections::HashSet<&str> = self
            .boards
            .read(cx)
            .iter()
            .filter(|p| p.team_id == team_id)
            .map(|p| p.id.as_str())
            .collect();
        self.issues
            .read(cx)
            .iter()
            .filter(|i| board_ids.contains(i.board_id.as_str()))
            .collect()
    }

    /// EXP-736: every relation row touching `issue_id`, from EITHER side —
    /// the shape syncs rows scoped by the SOURCE issue's board, and a row
    /// whose `related_issue_id` is this issue is the inverse side of the same
    /// relation. Callers group by the per-side label and hide rows whose
    /// other issue has not synced. Insertion order is not meaningful (the
    /// collection is a map), so rows come back sorted by id for a stable
    /// render.
    pub fn relations_for_issue(&self, issue_id: &str, cx: &App) -> Vec<IssueRelation> {
        let mut out: Vec<IssueRelation> = self
            .issue_relations
            .read(cx)
            .iter()
            .filter(|row| row.issue_id == issue_id || row.related_issue_id == issue_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }
}

fn sort_issues(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(f64::MAX)
            .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
            .then_with(|| cmp_identifiers(&a.identifier, &b.identifier))
    });
}

/// Natural comparison of issue identifiers so `EXP-2` sorts before `EXP-10`
/// (§8.7): compare the non-numeric prefix lexicographically, then the trailing
/// number numerically. Malformed identifiers (no `{prefix}-{number}` shape)
/// fall back to a plain string compare, preserving a total order.
pub fn cmp_identifiers(a: &str, b: &str) -> std::cmp::Ordering {
    match (split_identifier(a), split_identifier(b)) {
        (Some((pa, na)), Some((pb, nb))) => pa.cmp(pb).then_with(|| na.cmp(&nb)),
        _ => a.cmp(b),
    }
}

/// Split `EXP-10` into (`"EXP"`, `10`); `None` when the trailing segment after
/// the last `-` is not a number.
fn split_identifier(ident: &str) -> Option<(&str, u64)> {
    let (prefix, number) = ident.rsplit_once('-')?;
    Some((prefix, number.parse().ok()?))
}

fn status_of<T: ShapeRow>(entity: &Entity<Collection<T>>, cx: &App) -> ShapeStatus {
    let collection = entity.read(cx);
    ShapeStatus {
        name: T::spec().name,
        phase: collection.phase(),
        rows: collection.len(),
    }
}

/// The shape's sync phase, derived from the persisted cursor state (§5.4's
/// `electric_offsets` row — the single source of truth the poll loop itself
/// drives from).
fn phase_from_store(sqlite: &ShapeStore, shape: &str) -> ShapeSyncPhase {
    match sqlite.shape_state(shape) {
        Ok(Some(state)) if state.needs_refetch => ShapeSyncPhase::Refetching,
        Ok(Some(state)) if state.is_live => ShapeSyncPhase::Live,
        Ok(Some(_)) => ShapeSyncPhase::Snapshot,
        Ok(None) => ShapeSyncPhase::Snapshot,
        Err(err) => {
            log::warn!("[sync {shape}] reading cursor state: {err}");
            ShapeSyncPhase::Waiting
        }
    }
}

fn hydrate_collection<T: ShapeRow>(
    entity: &Entity<Collection<T>>,
    sqlite: &Arc<ShapeStore>,
    cx: &mut App,
) {
    let spec = T::spec();
    let maps = match sqlite.read_all(spec) {
        Ok(maps) => maps,
        Err(err) => {
            log::warn!("[sync {}] hydrate read failed: {err}", spec.name);
            return;
        }
    };
    let rows = decode_rows::<T>(maps);
    let phase = phase_from_store(sqlite, spec.name);
    entity.update(cx, |collection, cx| {
        collection.replace_all(rows);
        collection.set_phase(phase);
        cx.notify();
    });
}

/// #LOGS-3: one shape batch read back from SQLite and decoded, OFF the main
/// thread ([`read_batch`]); [`commit_batch`] writes it into the collection
/// on the main thread. A point read waits on the disk, and on a busy machine
/// (agents building, swap) that wait reached seconds on the UI thread.
enum Batch<T> {
    /// §5.6c atomic refetch: the whole table (#PERF-9: indexed off main).
    Replace(Hydrated<T>),
    /// Per key: the row (an upsert) or `None` (gone: a delete).
    Rows(Vec<(RowKey, Option<T>)>),
}

/// A batch ready for the main thread.
pub(crate) type Prepared = Box<dyn FnOnce(&Collections, &mut AsyncApp) + Send>;

fn read_batch<T: ShapeRow>(keys: &[RowKey], full_replace: bool, sqlite: &ShapeStore) -> Option<(Batch<T>, ShapeSyncPhase)> {
    let spec = T::spec();
    let _span = tracing::debug_span!("sync.read", shape = spec.name, rows = keys.len(), full_replace).entered();
    let phase = phase_from_store(sqlite, spec.name);
    if full_replace {
        // §5.6c atomic refetch: the batch replaced the WHOLE table in one
        // commit — re-hydrate wholesale (point reads are not enough).
        return Hydrated::read(sqlite).map(|rows| (Batch::Replace(rows), phase));
    }
    let mut rows = Vec::with_capacity(keys.len());
    for key in keys {
        match sqlite.read_by_key(spec, key) {
            // Present in SQLite → it was an upsert.
            Ok(Some(map)) => match serde_json::from_value::<T>(Value::Object(map)) {
                Ok(mut row) => {
                    row.open_sealed();
                    rows.push((key.clone(), Some(row)));
                }
                Err(err) => log::warn!("[sync {}] dropping unhydratable row: {err}", spec.name),
            },
            // Gone from SQLite → it was a delete.
            Ok(None) => rows.push((key.clone(), None)),
            Err(err) => log::warn!("[sync {}] point read failed: {err}", spec.name),
        }
    }
    Some((Batch::Rows(rows), phase))
}

fn commit_batch<T: ShapeRow>(entity: &Entity<Collection<T>>, batch: Batch<T>, phase: ShapeSyncPhase, up_to_date: bool, cx: &mut AsyncApp) {
    // #LOGS-2: the foreground half of a shape batch (the writes + notify).
    let _span = tracing::debug_span!("sync.apply", shape = T::spec().name).entered();
    let replaced = entity.update(cx, |collection, cx| {
        let mut replaced = None;
        match batch {
            Batch::Replace(rows) => replaced = Some(collection.install(rows)),
            Batch::Rows(rows) => {
                for (key, row) in rows {
                    match row {
                        Some(row) => collection.upsert(key, row),
                        None => collection.remove(&key),
                    }
                }
            }
        }
        collection.set_phase(phase);
        collection.caught_up |= up_to_date;
        cx.notify();
        replaced
    });
    // #PERF-9: a whole table's worth of rows is freed off the main thread.
    if let Some(replaced) = replaced {
        cx.background_executor().spawn(async move { drop(replaced) }).detach();
    }
}

fn prepare_for<T: ShapeRow + Send + 'static>(
    pick: fn(&Collections) -> &Entity<Collection<T>>,
    keys: &[RowKey],
    full_replace: bool,
    up_to_date: bool,
    sqlite: &ShapeStore,
) -> Option<Prepared> {
    let (batch, phase) = read_batch::<T>(keys, full_replace, sqlite)?;
    Some(Box::new(move |collections, cx| commit_batch(pick(collections), batch, phase, up_to_date, cx)))
}

/// EXP-1459: re-reads and re-opens the rows whose sealed content stayed
/// closed (keys arrived, or a digest's root became known). #PERF-1: the
/// point reads and the decode run on the background executor: the reader
/// connection sits behind one lock, and a delta's read (a refetch's
/// `read_all` of `messages`) held it for seconds while the main thread
/// waited here inside `sync.apply(sessions)`.
async fn reopen_closed<T: ShapeRow + Send + 'static>(entity: &Entity<Collection<T>>, sqlite: &Arc<ShapeStore>, cx: &mut AsyncApp) {
    let keys: Vec<RowKey> = entity.read_with(cx, |collection, _| {
        collection.rows.iter().filter(|(_, row)| row.seal_closed()).map(|(key, _)| key.clone()).collect()
    });
    if keys.is_empty() {
        return;
    }
    let sqlite = Arc::clone(sqlite);
    let rows = cx.background_executor().spawn(async move { read_reopened::<T>(&keys, &sqlite) }).await;
    if rows.is_empty() {
        return;
    }
    entity.update(cx, |collection, cx| {
        if reopen_rows(collection, rows) {
            cx.notify();
        }
    });
}

/// The rows of `keys` read back and opened (background).
fn read_reopened<T: ShapeRow>(keys: &[RowKey], sqlite: &ShapeStore) -> Vec<(RowKey, T)> {
    let spec = T::spec();
    let _span = tracing::debug_span!("sync.reopen", shape = spec.name, rows = keys.len()).entered();
    keys.iter()
        .filter_map(|key| {
            let map = sqlite.read_by_key(spec, key).ok()??;
            let mut row = serde_json::from_value::<T>(Value::Object(map)).ok()?;
            row.open_sealed();
            Some((key.clone(), row))
        })
        .collect()
}

/// Write re-opened rows over the ones still closed; a row a delta replaced
/// or removed while they were read keeps the delta's version. True = wrote.
fn reopen_rows<T: ShapeRow>(collection: &mut Collection<T>, rows: Vec<(RowKey, T)>) -> bool {
    let mut wrote = false;
    for (key, row) in rows {
        if collection.rows.get(&key).is_some_and(|current| current.seal_closed()) {
            collection.upsert(key, row);
            wrote = true;
        }
    }
    wrote
}

// ---------------------------------------------------------------------------
// The Store global
// ---------------------------------------------------------------------------

/// The sync store — a gpui [`Global`] (§3.5: `Store` and `Theme` are globals;
/// views `cx.observe` the specific collections they read). Cheap to clone (a
/// handle of Entities + Arcs) — clone out of `Store::global(cx)` before
/// calling `&mut App` methods.
#[derive(Clone)]
pub struct Store {
    state: Entity<SharedState>,
    collections: Collections,
    manager: Arc<SyncManager>,
    /// EXP-501: CAS guard so at most one grace-expiry recheck timer is in
    /// flight (see [`Store::record_sync`]).
    health_recheck_pending: Arc<AtomicBool>,
    /// EXP-533: the same CAS guard for the catching-up window expiry.
    catch_up_recheck_pending: Arc<AtomicBool>,
    /// EXP-533: when [`Store::kick_if_stale`] last ATTEMPTED a restart — window
    /// activation can fire in bursts (focus follows every alt-tab), and the
    /// manager stamps nothing when it had no live pipeline to rebuild.
    last_kick_at: Arc<Mutex<Option<Instant>>>,
}

impl Global for Store {}

impl Store {
    /// Build the store: the shared-state entity, the 34 collection entities,
    /// the [`SyncManager`], and the single foreground delta-drain task
    /// (§5.8). `on_unauthorized` is the §5.6b hook the app shell wires to
    /// `AuthStore::unauthorized_handler_fn()` — it deletes the dead token
    /// before the drain routes the UI to login.
    ///
    /// Pipelines start via [`Store::connect`] (login / warm-start resume),
    /// not here — `requireAuth` shapes must never be polled without a token
    /// (§5.9).
    pub fn open(
        cx: &mut App,
        on_unauthorized: Option<UnauthorizedFn>,
        on_upgrade_required: Option<UpgradeRequiredFn>,
    ) -> Self {
        let state = cx.new(|_| SharedState {
            windows_open: 0,
            session: SessionPhase::SignedOut,
            sync_health: HashMap::new(),
            published_health: SyncHealth::Ok,
            catch_up: CatchUp::default(),
            published_catching_up: false,
        });
        let collections = Collections::new(cx);
        let mut manager = SyncManager::new();
        if let Some(hook) = on_unauthorized {
            manager = manager.on_unauthorized(hook);
        }
        if let Some(hook) = on_upgrade_required {
            manager = manager.on_upgrade_required(hook);
        }
        let store = Self {
            state,
            collections,
            manager: Arc::new(manager),
            health_recheck_pending: Arc::new(AtomicBool::new(false)),
            catch_up_recheck_pending: Arc::new(AtomicBool::new(false)),
            last_kick_at: Arc::new(Mutex::new(None)),
        };
        store.spawn_delta_drain(cx);
        // EXP-533: a closed lid parks every shape thread in a read on an h2
        // connection that dies with the machine; the watchdog notices the
        // wake and restarts the pipeline instead of waiting out the timeout.
        crate::manager::spawn_wake_watchdog(&store.manager);
        store
    }

    /// Read the global store (panics if the shell has not installed it — the
    /// §3.6 bootstrap sets it before any window opens).
    pub fn global(cx: &App) -> &Self {
        cx.global::<Store>()
    }

    /// Like [`Self::global`] but `None` before the store is installed —
    /// headless view tests construct views without a sync store.
    pub fn try_global(cx: &App) -> Option<&Self> {
        cx.try_global::<Store>()
    }

    /// The shared cross-window state entity. Observe it for re-renders.
    pub fn state(&self) -> Entity<SharedState> {
        self.state.clone()
    }

    /// The current session phase (convenience read).
    pub fn session(&self, cx: &App) -> SessionPhase {
        self.state.read(cx).session.clone()
    }

    /// The 34 reactive collections.
    pub fn collections(&self) -> &Collections {
        &self.collections
    }

    /// The sync manager: its per-account stores are readable off the UI
    /// thread (VAPP-43: eNet's stored replay reads a `sessions` row there).
    pub fn manager(&self) -> Arc<SyncManager> {
        Arc::clone(&self.manager)
    }

    /// Per-shape sync status snapshot — the debug board's status line.
    pub fn shape_statuses(&self, cx: &App) -> Vec<ShapeStatus> {
        self.collections.statuses(cx)
    }

    /// EXP-633: the shapes that have NOT seen their first `up-to-date` yet,
    /// by name — the capture pipeline's readiness handshake says what it is
    /// still waiting for instead of sleeping a fixed number of seconds. The
    /// predicate is [`Collection::is_ready`]'s (`Live | Refetching`), and an
    /// empty shape still flips to Live on a bare `up-to-date`, so an account
    /// with no data reaches ready like any other.
    pub fn shapes_not_ready(&self, cx: &App) -> Vec<&'static str> {
        not_ready_names(&self.collections.statuses(cx))
    }

    /// EXP-633: every subscribed shape has seen its first `up-to-date`.
    pub fn all_shapes_ready(&self, cx: &App) -> bool {
        self.shapes_not_ready(cx).is_empty()
    }

    /// Observe every collection entity with a plain `cx.notify()` — for
    /// coarse-grained views (the debug board). Real screens observe only the
    /// collections they read (§5.8 fine-grained rule).
    pub fn observe_collections<V: 'static>(&self, cx: &mut gpui::Context<V>) -> Vec<Subscription> {
        self.collections.observe_all(cx)
    }

    /// [`Self::observe_collections`] without the shapes named in `skip`
    /// (#LOGS-11: a view that never reads the transcripts or the event
    /// stream re-rendered on every streamed message).
    pub fn observe_collections_except<V: 'static>(&self, skip: &'static [&'static str], cx: &mut gpui::Context<V>) -> Vec<Subscription> {
        self.collections.observe_all_except(skip, cx)
    }

    // -- session transitions (§5 state machine) ------------------------------

    /// `* → SigningIn` — the login surface shows a busy state.
    pub fn begin_sign_in(&self, cx: &mut App) {
        self.set_session(SessionPhase::SigningIn, cx);
    }

    /// `SigningIn → SignedOut` — a failed sign-in attempt.
    pub fn abort_sign_in(&self, cx: &mut App) {
        self.set_session(SessionPhase::SignedOut, cx);
    }

    /// `SigningIn/SignedOut → Synced`: start (or resume) the account's 16
    /// shape threads against its per-account SQLite, hydrate the collections
    /// from disk (a warm start paints the last-known board immediately while
    /// the pipeline resumes from the persisted cursor — §5.11 gate 3), and
    /// flip the session phase.
    pub fn connect(&self, config: AccountSyncConfig, cx: &mut App) -> Result<(), StoreError> {
        let account_id = config.account_id.clone();
        self.manager.start_account(config)?;
        if let Some(sqlite) = self.manager.store(&account_id) {
            self.collections.hydrate_all(&sqlite, cx);
        }
        // EXP-501: a (re-)connecting account starts with fresh health — a
        // warm re-login must get a fresh failure-streak grace, not inherit a
        // pre-logout streak that would flash the offline banner instantly.
        self.state.update(cx, |state, _| {
            state.sync_health.remove(&account_id);
        });
        self.set_session(SessionPhase::Synced { account_id }, cx);
        Ok(())
    }

    /// #EXP-1727: a Local desktop as the active account: its store is
    /// authoritative, so there is no pipeline: the collections hydrate from
    /// `db_path` and follow `changes` (`(table, row ids)`, the local store's
    /// deltas) until another account connects.
    pub fn connect_local(&self, account_id: &str, db_path: &std::path::Path, changes: flume::Receiver<(String, Vec<String>)>, cx: &mut App) -> Result<(), StoreError> {
        let sqlite = Arc::new(ShapeStore::open(db_path)?);
        let shapes: Vec<&str> = crate::shapes::SHAPES.iter().map(|spec| spec.name).collect();
        sqlite.mark_live(&shapes)?;
        self.manager.adopt_store(account_id, Arc::clone(&sqlite), db_path.to_path_buf());
        self.collections.clear_all(cx);
        self.collections.hydrate_all(&sqlite, cx);
        self.set_session(SessionPhase::Synced { account_id: account_id.to_string() }, cx);
        let store = self.clone();
        let account_id = account_id.to_string();
        cx.spawn(async move |cx| {
            while let Ok((table, ids)) = changes.recv_async().await {
                let active = store.state.read_with(cx, |state, _| state.session.account_id() == Some(account_id.as_str()));
                if !active {
                    return;
                }
                let keys: Vec<RowKey> = ids.into_iter().map(RowKey::Single).collect();
                store.collections.apply(&table, keys, false, true, &sqlite, cx).await;
            }
        })
        .detach();
        Ok(())
    }

    /// EXP-1459: tree keys arrived (the key cache's change counter moved):
    /// re-open every row whose sealed content stayed closed.
    pub fn reopen_sealed(&self, cx: &mut App) {
        let Some(account_id) = self.session(cx).account_id().map(str::to_string) else {
            return;
        };
        let Some(sqlite) = self.manager.store(&account_id) else {
            return;
        };
        // #PERF-1: read off the main thread (see `reopen_closed`).
        let collections = self.collections.clone();
        cx.spawn(async move |cx| {
            reopen_closed(&collections.sessions, &sqlite, cx).await;
            reopen_closed(&collections.messages, &sqlite, cx).await;
            reopen_closed(&collections.conversations, &sqlite, cx).await;
        })
        .detach();
    }

    /// Restart the active account's shape pipeline (EXP-470): after a team
    /// create/join the rotated shapes re-poll immediately instead of waiting
    /// out their parked live long-polls. No-op when not `Synced`.
    pub fn resync_active(&self, cx: &App) {
        let session = self.session(cx);
        if let Some(account_id) = session.account_id() {
            self.manager.restart_account(account_id);
        }
    }

    /// `Synced/AuthExpired → SignedOut` (§5.10 sign-out): stop the pipeline
    /// (SQLite stays on disk for offline resume), clear the in-memory
    /// collections, route to login.
    pub fn sign_out(&self, account_id: &str, cx: &mut App) {
        self.manager.stop_account(account_id);
        self.collections.clear_all(cx);
        // EXP-501: drop all health — a stale failure streak must not survive
        // into the next sign-in (`set_session` republishes below).
        self.state.update(cx, |state, _| {
            state.sync_health.clear();
        });
        self.set_session(SessionPhase::SignedOut, cx);
    }

    fn set_session(&self, session: SessionPhase, cx: &mut App) {
        self.state.update(cx, |state, cx| {
            let mut changed = false;
            if state.session != session {
                state.session = session;
                changed = true;
            }
            // EXP-501: the banner-driving health is a function of the session
            // (account switch / sign-out flips it without any new delta).
            let derived =
                derive_active_health(&state.session, &state.sync_health, SystemTime::now());
            if state.published_health != derived {
                state.published_health = derived;
                changed = true;
            }
            if changed {
                cx.notify();
            }
        });
    }

    /// EXP-501: the active account's sync health, derived fresh — render
    /// paths call this per frame (pure read, mirrors iOS `health()`).
    pub fn sync_status(&self, cx: &App) -> ActiveSyncStatus {
        let statuses = self.collections.statuses(cx);
        let state = self.state.read(cx);
        let SessionPhase::Synced { account_id } = &state.session else {
            return ActiveSyncStatus::default();
        };
        let now = SystemTime::now();
        let health = state.sync_health.get(account_id);
        ActiveSyncStatus {
            health: health.map_or(SyncHealth::Ok, |h| h.health(now)),
            last_success_at: health.and_then(|h| h.last_success_at),
            last_error: health.and_then(|h| h.last_error.clone()),
            catching_up: state.catch_up.is_catching_up(&statuses, now),
        }
    }

    /// EXP-533: the window came to the front. A machine that slept through
    /// the watchdog's window (or simply sat idle behind a dead connection)
    /// gets one restart here so the user's first look at the app is at fresh
    /// data. No-op unless `Synced`, the last successful poll is genuinely old
    /// ([`ACTIVATION_STALE`]), and the pipeline was not restarted inside
    /// [`KICK_DEBOUNCE`] — by us (activation fires on every alt-tab) or by
    /// anyone else, notably the wake watchdog, which fires on the SAME wake
    /// that brings the window to the front and leaves `last_success_at` stale
    /// until its first fresh poll lands.
    pub fn kick_if_stale(&self, cx: &App) {
        let state = self.state.read(cx);
        let SessionPhase::Synced { account_id } = &state.session else {
            return;
        };
        let stale = state
            .sync_health
            .get(account_id)
            .and_then(|health| health.last_success_at)
            // No success recorded at all = a pipeline that has not reported
            // yet (cold start); restarting THAT would only set it back.
            .is_some_and(|at| {
                SystemTime::now()
                    .duration_since(at)
                    .unwrap_or(Duration::ZERO)
                    >= ACTIVATION_STALE
            });
        if !stale {
            return;
        }
        let account_id = account_id.clone();
        if self.manager.restarted_within(KICK_DEBOUNCE) {
            return;
        }
        {
            let mut last = self.last_kick_at.lock().expect("last_kick_at poisoned");
            if last.is_some_and(|at| at.elapsed() < KICK_DEBOUNCE) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.manager.restart_account(&account_id);
    }

    // -- the single foreground drain (§5.8 / §3.5) ---------------------------

    fn spawn_delta_drain(&self, cx: &mut App) {
        let deltas = self.manager.deltas();
        let store = self.clone();
        cx.spawn(async move |cx| {
            while let Ok(delta) = deltas.recv_async().await {
                store.apply_delta(delta, cx).await;
            }
        })
        .detach();
    }

    async fn apply_delta(&self, delta: ShapeDelta, cx: &mut AsyncApp) {
        // Never held across an await: the main thread's span stack (#LOGS-2)
        // would name it while other work runs.
        let delta_span = tracing::debug_span!("sync.delta").entered();
        match delta {
            ShapeDelta::Unauthorized { account_id } => {
                // §5.6b: the pipeline tore itself down and the token is
                // already deleted (the on_unauthorized hook ran on the shape
                // thread). Sweep the dead pipeline entry and ROUTE TO LOGIN —
                // never an empty board (the Phase-2 gate bullet).
                //
                // Only the ACTIVE account's 401 flips the session. A pipeline
                // can only be running for an account that already went through
                // `connect` (which sets `Synced` in the same foreground turn),
                // so a 401 arriving during `SigningIn` is always a stale /
                // background account — stomping the in-flight sign-in with
                // AuthExpired{wrong account} would mislabel it.
                self.manager.stop_account(&account_id);
                self.state.update(cx, |state, cx| {
                    // EXP-501: drop the dead account's health so an offline
                    // banner can never race / shadow the login routing.
                    state.sync_health.remove(&account_id);
                    if state.session.account_id() == Some(account_id.as_str())
                        && !matches!(state.session, SessionPhase::AuthExpired { .. })
                    {
                        state.session = SessionPhase::AuthExpired { account_id };
                        cx.notify();
                    }
                    let derived = derive_active_health(
                        &state.session,
                        &state.sync_health,
                        SystemTime::now(),
                    );
                    if state.published_health != derived {
                        state.published_health = derived;
                        cx.notify();
                    }
                });
            }
            ShapeDelta::Applied {
                account_id,
                shape,
                keys,
                full_replace,
                up_to_date,
            } => {
                // EXP-501: every Applied delta — row batch or idle
                // `up-to-date` heartbeat — is a 2xx poll. Recorded BEFORE the
                // active-account guard so background accounts' health stays
                // truthful too.
                self.record_sync(&account_id, Ok(()), cx);
                // Only the active account's deltas feed the collections
                // (background accounts still sync to their own SQLite).
                let active = self
                    .state
                    .read_with(cx, |state, _| state.session.account_id().map(String::from));
                if active.as_deref() != Some(account_id.as_str()) {
                    return;
                }
                // EXP-533: this core shape is back at head — shrink the
                // post-restart stamp (and repaint only on a real shrink).
                self.record_catch_up(shape, cx);
                let Some(sqlite) = self.manager.store(&account_id) else {
                    return;
                };
                // #LOGS-3: read + decode off the main thread, then commit
                // unless the account switched (or signed out) meanwhile.
                drop(delta_span);
                // #PERF-9: a heavy shape's startup hydrate lands first.
                self.collections.hydrated(shape).await;
                let read = {
                    let sqlite = Arc::clone(&sqlite);
                    cx.background_executor().spawn(async move { Collections::prepare(shape, &keys, full_replace, up_to_date, &sqlite) })
                };
                let Some(prepared) = read.await else { return };
                let still_active = self.state.read_with(cx, |state, _| state.session.account_id() == Some(account_id.as_str()));
                if still_active {
                    self.collections.commit(shape, prepared, &sqlite, cx);
                }
            }
            ShapeDelta::PollFailed {
                account_id,
                shape: _,
                error,
            } => {
                self.record_sync(&account_id, Err(error), cx);
            }
            ShapeDelta::PipelineRestarted { account_id } => {
                // EXP-533: only the ACTIVE account's restart drives the rail
                // spinner (a background account's resync is invisible).
                let active = self
                    .state
                    .read_with(cx, |state, _| state.session.account_id().map(String::from));
                if active.as_deref() != Some(account_id.as_str()) {
                    return;
                }
                self.begin_catch_up(cx);
            }
        }
    }

    /// EXP-533: stamp a fresh restart and arm the window-expiry recheck, so
    /// the spinner stops on its own when nothing ever reports back (an
    /// offline machine — the banner owns that story).
    fn begin_catch_up(&self, cx: &mut AsyncApp) {
        self.state.update(cx, |state, cx| {
            state.catch_up.begin(SystemTime::now());
            if !state.published_catching_up {
                state.published_catching_up = true;
            }
            cx.notify();
        });
        if !self.catch_up_recheck_pending.swap(true, Ordering::SeqCst) {
            let store = self.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(CATCHING_UP_WINDOW + Duration::from_millis(500))
                    .await;
                store.catch_up_recheck_pending.store(false, Ordering::SeqCst);
                store.publish_catching_up(cx);
            })
            .detach();
        }
    }

    /// One core shape reported back after a restart.
    fn record_catch_up(&self, shape: &str, cx: &mut AsyncApp) {
        let shrank = self
            .state
            .update(cx, |state, _| state.catch_up.record_success(shape));
        if shrank {
            self.publish_catching_up(cx);
        }
    }

    /// Republish the derived spinner flag, notifying only on a flip.
    fn publish_catching_up(&self, cx: &mut AsyncApp) {
        let statuses = cx.update(|cx| self.collections.statuses(cx));
        self.state.update(cx, |state, cx| {
            let derived = state
                .catch_up
                .is_catching_up(&statuses, SystemTime::now());
            if state.published_catching_up != derived {
                state.published_catching_up = derived;
                cx.notify();
            }
        });
    }

    /// EXP-501: fold one poll outcome into the account's health, publishing
    /// the derived active-account health only when it flips (Ok ⇄ Offline) —
    /// the per-minute heartbeats of every shape thread must not wake every
    /// `SharedState` observer.
    fn record_sync(&self, account_id: &str, outcome: Result<(), String>, cx: &mut AsyncApp) {
        let now = SystemTime::now();
        let failed = outcome.is_err();
        self.state.update(cx, |state, cx| {
            let health = state.sync_health.entry(account_id.to_string()).or_default();
            match outcome {
                Ok(()) => health.record_success(now),
                Err(error) => health.record_failure(now, error),
            }
            let derived = derive_active_health(&state.session, &state.sync_health, now);
            // While Offline persists, failure deltas (≤30s apart under the
            // backoff cap) still notify so the banner's "last synced" ago-
            // label keeps ticking; successes dedupe as before.
            if state.published_health != derived || (failed && derived == SyncHealth::Offline) {
                state.published_health = derived;
                cx.notify();
            }
        });
        // A failure inside the grace window still derives Ok — schedule ONE
        // recheck just past the grace so the banner appears at ~12s instead
        // of whenever the next (backoff-capped, up to 30s) failure delta
        // happens to land. Offline → Ok needs no timer: the first successful
        // poll emits `Applied` immediately.
        if failed && !self.health_recheck_pending.swap(true, Ordering::SeqCst) {
            let store = self.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(FAILURE_STREAK_GRACE + Duration::from_millis(500))
                    .await;
                store.health_recheck_pending.store(false, Ordering::SeqCst);
                store.state.update(cx, |state, cx| {
                    let derived = derive_active_health(
                        &state.session,
                        &state.sync_health,
                        SystemTime::now(),
                    );
                    if state.published_health != derived {
                        state.published_health = derived;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => panic!("expected object"),
        }
    }

    // EXP-501: an account whose failure streak long outlived the grace —
    // alarming on its own, so any Ok below proves the SESSION gate.
    fn alarming_health(account_id: &str, now: SystemTime) -> HashMap<String, AccountHealth> {
        let mut health = AccountHealth::default();
        health.record_failure(now - Duration::from_secs(60), "http 500".into());
        health.record_failure(now - Duration::from_secs(1), "http 500".into());
        assert_eq!(health.health(now), SyncHealth::Offline);
        HashMap::from([(account_id.to_string(), health)])
    }

    #[test]
    fn only_a_synced_session_can_be_offline() {
        let now = SystemTime::now();
        let map = alarming_health("acct-1", now);
        // AuthExpired routes to login (owns the 401 story), SignedOut /
        // SigningIn have no pipeline — none of them may alarm.
        for session in [
            SessionPhase::SignedOut,
            SessionPhase::SigningIn,
            SessionPhase::AuthExpired {
                account_id: "acct-1".into(),
            },
        ] {
            assert_eq!(derive_active_health(&session, &map, now), SyncHealth::Ok);
        }
        let synced = SessionPhase::Synced {
            account_id: "acct-1".into(),
        };
        assert_eq!(derive_active_health(&synced, &map, now), SyncHealth::Offline);
    }

    #[test]
    fn background_account_outage_never_alarms_the_active_one() {
        let now = SystemTime::now();
        let map = alarming_health("acct-background", now);
        let synced = SessionPhase::Synced {
            account_id: "acct-active".into(),
        };
        // The active account has no entry at all → healthy.
        assert_eq!(derive_active_health(&synced, &map, now), SyncHealth::Ok);
    }

    #[test]
    fn every_shape_has_a_typed_row_binding() {
        // The 34 ShapeRow impls cover the registry exactly (a 35th shape
        // without a typed row would silently never reach the UI).
        let bound = [
            Team::spec().name,
            Board::spec().name,
            Issue::spec().name,
            Label::spec().name,
            IssueLabel::spec().name,
            User::spec().name,
            TeamMember::spec().name,
            TeamInvite::spec().name,
            Comment::spec().name,
            Attachment::spec().name,
            Notification::spec().name,
            IssueEvent::spec().name,
            IssueSubscriber::spec().name,
            SessionRow::spec().name,
            SessionIssueRow::spec().name,
            MessageRow::spec().name,
            ConversationRow::spec().name,
            PullRequestRow::spec().name,
            PullRequestIssueRow::spec().name,
            PullRequestCheckRow::spec().name,
            IssueStatusRow::spec().name,
            DeviceRow::spec().name,
            DeviceWorktreeRow::spec().name,
            IssueRelation::spec().name,
            Pin::spec().name,
            IssueDraftRow::spec().name,
            ComposerDraftRow::spec().name,
            SessionReadRow::spec().name,
            AgentDefRow::spec().name,
            EventRow::spec().name,
            PromptRow::spec().name,
            AppRow::spec().name,
            AppVersionRow::spec().name,
            AppRecordRow::spec().name,
            SkillRow::spec().name,
        ];
        let registry: Vec<&str> = crate::shapes::SHAPES.iter().map(|s| s.name).collect();
        assert_eq!(bound.len(), registry.len());
        for name in registry {
            assert!(bound.contains(&name), "no typed row for shape {name}");
        }
    }

    #[test]
    fn row_keys_match_the_wire_key_forms() {
        let issue: Issue = serde_json::from_value(json!({
            "id": "01J9K0A0X3CB4E5F6G7H8J9K0L",
            "board_id": "p-1",
            "number": 1,
            "identifier": "EXP-1",
            "title": "t",
            "status": "backlog",
            "priority": "none"
        }))
        .unwrap();
        assert_eq!(
            issue.key(),
            RowKey::Single("01J9K0A0X3CB4E5F6G7H8J9K0L".into())
        );

        let link: IssueLabel = serde_json::from_value(json!({
            "issue_id": "i-1",
            "label_id": "l-1"
        }))
        .unwrap();
        assert_eq!(link.key(), RowKey::Pair("i-1".into(), "l-1".into()));
    }

    #[test]
    fn decode_rows_drops_bad_rows_and_keeps_good_ones() {
        let maps = vec![
            obj(json!({
                "id": "i-1", "board_id": "p-1", "number": "1",
                "identifier": "EXP-1", "title": "ok", "status": "backlog",
                "priority": "none"
            })),
            // Missing required identifier/title → dropped, not fatal (§5.5).
            obj(json!({ "id": "i-2", "board_id": "p-1", "number": "2" })),
        ];
        let rows = decode_rows::<Issue>(maps);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.identifier, "EXP-1");
    }

    #[test]
    fn collection_tracks_revision_and_readiness() {
        let mut collection: Collection<Issue> = Collection::new();
        assert!(!collection.is_ready());
        assert_eq!(collection.revision(), 0);

        let issue: Issue = serde_json::from_value(json!({
            "id": "i-1", "board_id": "p-1", "number": 1,
            "identifier": "EXP-1", "title": "t", "status": "backlog",
            "priority": "none"
        }))
        .unwrap();
        collection.upsert(issue.key(), issue.clone());
        assert_eq!(collection.len(), 1);
        assert_eq!(collection.revision(), 1);
        assert!(collection.get("i-1").is_some());

        collection.set_phase(ShapeSyncPhase::Live);
        assert!(collection.is_ready());

        collection.remove(&issue.key());
        assert!(collection.is_empty());
        assert_eq!(collection.revision(), 2);
        // Removing an absent key does not bump the revision.
        collection.remove(&RowKey::Single("gone".into()));
        assert_eq!(collection.revision(), 2);
    }

    fn team(id: &str, name: &str) -> Team {
        serde_json::from_value(json!({ "id": id, "name": name })).unwrap()
    }

    #[test]
    fn seeded_rows_read_like_synced_rows_until_confirmed() {
        let mut collection: Collection<Team> = Collection::new();
        let seeded = team("t-new", "New team");
        collection.seed(seeded.key(), seeded.clone());

        assert_eq!(collection.len(), 1);
        assert!(!collection.is_empty());
        assert_eq!(collection.get("t-new").map(|t| t.name.as_str()), Some("New team"));
        assert_eq!(collection.iter().count(), 1);
        assert!(collection.is_seeded("t-new"));
        assert_eq!(collection.revision(), 1);

        // The Electric echo confirms: the server row wins, the overlay entry
        // is gone.
        let echo = team("t-new", "New team (server)");
        collection.upsert(echo.key(), echo);
        assert!(!collection.is_seeded("t-new"));
        assert_eq!(collection.len(), 1);
        assert_eq!(
            collection.get("t-new").map(|t| t.name.as_str()),
            Some("New team (server)")
        );
    }

    #[test]
    fn seed_is_a_noop_when_sync_already_delivered_the_key() {
        let mut collection: Collection<Team> = Collection::new();
        let synced = team("t-1", "Synced");
        collection.upsert(synced.key(), synced.clone());

        collection.seed(synced.key(), team("t-1", "Stale local"));
        assert!(!collection.is_seeded("t-1"));
        assert_eq!(collection.get("t-1").map(|t| t.name.as_str()), Some("Synced"));
        assert_eq!(collection.len(), 1);
    }

    #[test]
    fn replace_all_without_the_key_keeps_the_seeded_row() {
        // The truncation guard (EXP-470): a post-409 refetch snapshot computed
        // under a stale membership view must not evict the optimistic row —
        // otherwise the UI would bounce back to the old team.
        let mut collection: Collection<Team> = Collection::new();
        let seeded = team("t-new", "New team");
        collection.seed(seeded.key(), seeded.clone());

        let old = team("t-old", "Old team");
        collection.replace_all(vec![(old.key(), old)]);
        assert!(collection.is_seeded("t-new"));
        assert!(collection.get("t-new").is_some());
        assert!(collection.get("t-old").is_some());
        assert_eq!(collection.len(), 2);

        // A snapshot that CONTAINS the key confirms it.
        let old = team("t-old", "Old team");
        let confirmed = team("t-new", "New team");
        collection.replace_all(vec![(old.key(), old), (confirmed.key(), confirmed)]);
        assert!(!collection.is_seeded("t-new"));
        assert_eq!(collection.len(), 2);
    }

    #[test]
    fn remove_and_clear_drop_seeded_rows() {
        let mut collection: Collection<Team> = Collection::new();
        let seeded = team("t-new", "New team");
        collection.seed(seeded.key(), seeded.clone());

        // An authoritative delete for the exact key drops the overlay entry.
        let revision = collection.revision();
        collection.remove(&seeded.key());
        assert!(!collection.is_seeded("t-new"));
        assert!(collection.is_empty());
        assert_eq!(collection.revision(), revision + 1);

        // Sign-out clear wipes the overlay with everything else.
        let seeded = team("t-new", "New team");
        collection.seed(seeded.key(), seeded);
        collection.clear();
        assert!(!collection.is_seeded("t-new"));
        assert!(collection.is_empty());
    }

    /// #LOGS-11: a chat's messages read through the group index, kept
    /// through every write (a moved row leaves its old group).
    #[test]
    fn messages_group_by_session_through_every_write() {
        let message = |id: &str, session: &str| -> MessageRow {
            serde_json::from_value(serde_json::json!({"id": id, "session_id": session})).unwrap()
        };
        let ids = |collection: &Collection<MessageRow>, session: &str| {
            let mut ids: Vec<String> = collection.in_group(session).map(|m| m.id.clone()).collect();
            ids.sort();
            ids
        };
        let mut collection: Collection<MessageRow> = Collection::new();
        collection.replace_all(vec![(message("m1", "s1").key(), message("m1", "s1")), (message("m2", "s2").key(), message("m2", "s2"))]);
        collection.seed(message("m3", "s1").key(), message("m3", "s1"));
        assert_eq!(ids(&collection, "s1"), ["m1", "m3"]);
        // The sync echo of a seeded row, and a row that moves session.
        collection.upsert(message("m3", "s1").key(), message("m3", "s1"));
        collection.upsert(message("m1", "s2").key(), message("m1", "s2"));
        assert_eq!(ids(&collection, "s1"), ["m3"]);
        assert_eq!(ids(&collection, "s2"), ["m1", "m2"]);
        collection.remove(&message("m2", "s2").key());
        assert_eq!(ids(&collection, "s2"), ["m1"]);
        // A session-less row belongs to none.
        collection.upsert(message("m4", "").key(), message("m4", ""));
        assert!(ids(&collection, "").is_empty());
        // A write moves its own group's revision only.
        let (s1, s2) = (collection.group_revision("s1"), collection.group_revision("s2"));
        collection.upsert(message("m5", "s2").key(), message("m5", "s2"));
        assert_eq!(collection.group_revision("s1"), s1);
        assert!(collection.group_revision("s2") > s2);
        collection.clear();
        assert!(ids(&collection, "s1").is_empty());
        assert!(collection.group_revision("s1") > s1);
    }

    /// #PERF-1: re-opened rows (read off the main thread) land only over
    /// rows still closed; a delta's write meanwhile wins.
    #[test]
    fn reopened_rows_never_overwrite_a_newer_write() {
        let message = |id: &str, seal: Option<domain::sealed::SealState>, body: &str| -> MessageRow {
            let mut row: MessageRow = serde_json::from_value(serde_json::json!({"id": id, "session_id": "s1", "body": body})).unwrap();
            row.seal = seal;
            row
        };
        let closed = Some(domain::sealed::SealState::Locked);
        let mut collection: Collection<MessageRow> = Collection::new();
        collection.replace_all(vec![
            (message("m1", closed, "").key(), message("m1", closed, "")),
            (message("m2", closed, "").key(), message("m2", closed, "")),
        ]);
        // m2 was re-written open by a delta while the reopen read ran; m3 is gone.
        collection.upsert(message("m2", None, "delta").key(), message("m2", None, "delta"));
        let read = vec![
            (message("m1", None, "").key(), message("m1", None, "opened")),
            (message("m2", None, "").key(), message("m2", None, "stale")),
            (message("m3", None, "").key(), message("m3", None, "gone")),
        ];
        assert!(reopen_rows(&mut collection, read));
        assert_eq!(collection.get("m1").unwrap().body.as_deref(), Some("opened"));
        assert_eq!(collection.get("m2").unwrap().body.as_deref(), Some("delta"));
        assert!(collection.get("m3").is_none());
        assert!(!reopen_rows(&mut collection, vec![(message("m1", None, "").key(), message("m1", None, "again"))]), "nothing closed is left");
    }

    /// #LOGS-11: a reader asks which rows moved since its revision; a
    /// wholesale write tells nothing older.
    #[test]
    fn the_change_log_tells_the_keys_written_since_a_revision() {
        let message = |id: &str| -> MessageRow { serde_json::from_value(serde_json::json!({"id": id, "session_id": "s1"})).unwrap() };
        let mut collection: Collection<MessageRow> = Collection::new();
        collection.replace_all(vec![(message("m1").key(), message("m1"))]);
        let at = collection.revision();
        assert_eq!(collection.changed_since(at), Some(vec![]));
        collection.upsert(message("m2").key(), message("m2"));
        collection.upsert(message("m2").key(), message("m2"));
        collection.remove(&message("m1").key());
        let keys: Vec<RowKey> = collection.changed_since(at).unwrap().into_iter().cloned().collect();
        assert_eq!(keys, vec![message("m2").key(), message("m1").key()]);
        assert_eq!(collection.changed_since(at - 1), None, "before the wholesale write");
        for n in 0..CHANGE_LOG {
            collection.upsert(message(&format!("x{n}")).key(), message(&format!("x{n}")));
        }
        assert_eq!(collection.changed_since(at), None, "past the log");
        let recent = collection.revision() - 1;
        assert_eq!(collection.changed_since(recent).map(|keys| keys.len()), Some(1));
    }

    #[test]
    fn session_phase_reports_its_account() {
        assert_eq!(SessionPhase::SignedOut.account_id(), None);
        assert_eq!(SessionPhase::SigningIn.account_id(), None);
        assert_eq!(
            SessionPhase::Synced {
                account_id: "a".into()
            }
            .account_id(),
            Some("a")
        );
        assert_eq!(
            SessionPhase::AuthExpired {
                account_id: "a".into()
            }
            .account_id(),
            Some("a")
        );
    }

    #[test]
    fn identifiers_sort_numerically_within_a_prefix() {
        // §8.7: EXP-2 must precede EXP-10 (lexicographic order would flip them).
        let mut idents = ["EXP-10", "EXP-1", "EXP-2", "EXP-20", "EXP-3"];
        idents.sort_by(|a, b| cmp_identifiers(a, b));
        assert_eq!(idents, ["EXP-1", "EXP-2", "EXP-3", "EXP-10", "EXP-20"]);

        // Prefix wins over the number, then the number breaks the tie.
        assert_eq!(cmp_identifiers("AAA-9", "EXP-1"), std::cmp::Ordering::Less);
        assert_eq!(cmp_identifiers("EXP-2", "EXP-10"), std::cmp::Ordering::Less);
        // Malformed identifiers fall back to a plain string compare.
        assert_eq!(cmp_identifiers("EXP-x", "EXP-y"), std::cmp::Ordering::Less);
    }
}

/// #PERF-9: per-shape hydrate cost on a real store, split into the
/// background half (SQLite read + decode + index build) and the main-thread
/// commit.
///
/// ```sh
/// PERF9_DB=/tmp/copy/sync-v2.sqlite cargo test -p sync --release --lib hydrate_bench -- --ignored --nocapture
/// ```
#[cfg(test)]
mod hydrate_bench {
    use super::*;

    fn one<T: ShapeRow>(sqlite: &ShapeStore, total: &mut (f64, f64)) {
        let started = Instant::now();
        let maps = sqlite.read_all(T::spec()).expect("rows");
        let rows = decode_rows::<T>(maps);
        let n = rows.len();
        let built = Hydrated::build(rows);
        let background = started.elapsed().as_secs_f64() * 1e3;
        let started = Instant::now();
        let mut collection: Collection<T> = Collection::new();
        collection.install(built);
        let main = started.elapsed().as_secs_f64() * 1e3;
        if HYDRATE_OFF_MAIN.contains(&T::spec().name) {
            total.0 += background;
            total.1 += main;
        } else {
            total.1 += background + main;
        }
        if n > 500 {
            println!("[hydrate] {:<22} {n:6} rows · background {background:7.1} ms · main {main:6.2} ms", T::spec().name);
        }
    }

    #[test]
    #[ignore = "a release bench on a real store"]
    fn hydrate_bench() {
        let Some(db) = std::env::var_os("PERF9_DB") else { return };
        let sqlite = ShapeStore::open(std::path::Path::new(&db)).expect("store");
        let mut total = (0., 0.);
        one::<Team>(&sqlite, &mut total);
        one::<Board>(&sqlite, &mut total);
        one::<Issue>(&sqlite, &mut total);
        one::<Label>(&sqlite, &mut total);
        one::<IssueLabel>(&sqlite, &mut total);
        one::<User>(&sqlite, &mut total);
        one::<TeamMember>(&sqlite, &mut total);
        one::<Comment>(&sqlite, &mut total);
        one::<Attachment>(&sqlite, &mut total);
        one::<Notification>(&sqlite, &mut total);
        one::<IssueEvent>(&sqlite, &mut total);
        one::<SessionRow>(&sqlite, &mut total);
        one::<SessionIssueRow>(&sqlite, &mut total);
        one::<MessageRow>(&sqlite, &mut total);
        one::<ConversationRow>(&sqlite, &mut total);
        one::<PullRequestRow>(&sqlite, &mut total);
        one::<PullRequestIssueRow>(&sqlite, &mut total);
        one::<PullRequestCheckRow>(&sqlite, &mut total);
        one::<EventRow>(&sqlite, &mut total);
        println!("[hydrate] total off main {:.1} ms · on main {:.2} ms (before: {:.1} ms on main)", total.0, total.1, total.0 + total.1);
    }
}
