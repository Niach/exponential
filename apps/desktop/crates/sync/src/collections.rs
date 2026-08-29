//! The ONLY gpui-aware module in this crate (masterplan-v3 §3.1 / §5.8) — the
//! reactive seam between the headless sync engine and the view tree.
//!
//! Design (§5.8, mirrored from §3.5's threading model):
//!
//! * **One `gpui::Entity<Collection<T>>` per shape** (19 entities), all held
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

use std::collections::HashMap;
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
    ActionRow, Attachment, AutomationRow, Board, CodingSession, Comment, DeviceRow,
    DeviceWorktreeRow, Issue, IssueEvent, IssueLabel, IssueStatusRow, IssueSubscriber, Label,
    Notification, Team, TeamInvite, TeamMember, User,
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

/// A typed row hydratable from the store's snake_case JSON objects. The 19
/// impls below bind each `domain::rows` struct to its [`ShapeSpec`].
pub trait ShapeRow: serde::de::DeserializeOwned + Send + 'static {
    fn spec() -> &'static ShapeSpec;
    /// The row's primary key, matching the Electric wire key (§5.2).
    fn key(&self) -> RowKey;
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
id_shape_row!(CodingSession, "coding_sessions");
id_shape_row!(ActionRow, "actions");
id_shape_row!(IssueStatusRow, "issue_statuses");
id_shape_row!(DeviceRow, "devices");
id_shape_row!(DeviceWorktreeRow, "device_worktrees");
id_shape_row!(AutomationRow, "automations");

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
    phase: ShapeSyncPhase,
    revision: u64,
}

impl<T> Collection<T> {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
            seeded: HashMap::new(),
            phase: ShapeSyncPhase::Waiting,
            revision: 0,
        }
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

    /// Optimistically insert a locally-built row (EXP-470). No-op when sync
    /// already delivered the key.
    pub fn seed(&mut self, key: RowKey, row: T) {
        if self.rows.contains_key(&key) {
            return;
        }
        self.seeded.insert(key, row);
        self.revision += 1;
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

    fn replace_all(&mut self, rows: Vec<(RowKey, T)>) {
        self.rows = rows.into_iter().collect();
        // Only a snapshot that CONTAINS a seeded key confirms it; a snapshot
        // without it may predate the mutation (stale where clause) and must
        // not evict the optimistic row.
        self.seeded.retain(|key, _| !self.rows.contains_key(key));
        self.revision += 1;
    }

    fn upsert(&mut self, key: RowKey, row: T) {
        self.seeded.remove(&key);
        self.rows.insert(key, row);
        self.revision += 1;
    }

    fn remove(&mut self, key: &RowKey) {
        let seeded = self.seeded.remove(key).is_some();
        if self.rows.remove(key).is_some() || seeded {
            self.revision += 1;
        }
    }

    fn clear(&mut self) {
        self.rows.clear();
        self.seeded.clear();
        self.phase = ShapeSyncPhase::Waiting;
        self.revision += 1;
    }
}

/// Decode store-hydrated JSON objects into typed rows, dropping (and logging)
/// anything unhydratable — §5.5: a bad value is caught at hydrate, never at
/// apply, and never takes the batch down.
pub fn decode_rows<T: ShapeRow>(maps: Vec<Map<String, Value>>) -> Vec<(RowKey, T)> {
    maps.into_iter()
        .filter_map(
            |map| match serde_json::from_value::<T>(Value::Object(map)) {
                Ok(row) => Some((row.key(), row)),
                Err(err) => {
                    log::warn!("[sync {}] dropping unhydratable row: {err}", T::spec().name);
                    None
                }
            },
        )
        .collect()
}

/// The 19 collection entities (§5.8). Cloning is cheap — `Entity` handles.
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
    pub coding_sessions: Entity<Collection<CodingSession>>,
    pub actions: Entity<Collection<ActionRow>>,
    /// EXP-314 per-team custom issue statuses (the 16th shape).
    pub issue_statuses: Entity<Collection<IssueStatusRow>>,
    /// EXP-481 per-user device registry (the 17th shape).
    pub devices: Entity<Collection<DeviceRow>>,
    /// EXP-481 per-device worktree inventory (the 18th shape).
    pub device_worktrees: Entity<Collection<DeviceWorktreeRow>>,
    /// EXP-583 per-team automations (the 19th shape) — one action + one
    /// device + one trigger, split out of the old `actions.trigger`.
    pub automations: Entity<Collection<AutomationRow>>,
}

/// Run `$body` once per shape with `$entity` bound to that shape's collection
/// entity — the single dispatch point that keeps the 19-way fan-out in one
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
        let $entity = &$collections.coding_sessions;
        $body;
        let $entity = &$collections.actions;
        $body;
        let $entity = &$collections.issue_statuses;
        $body;
        let $entity = &$collections.devices;
        $body;
        let $entity = &$collections.device_worktrees;
        $body;
        let $entity = &$collections.automations;
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
            coding_sessions: cx.new(|_| Collection::new()),
            actions: cx.new(|_| Collection::new()),
            issue_statuses: cx.new(|_| Collection::new()),
            devices: cx.new(|_| Collection::new()),
            device_worktrees: cx.new(|_| Collection::new()),
            automations: cx.new(|_| Collection::new()),
        }
    }

    /// Apply one committed batch's keys to the matching typed collection
    /// (foreground; point-reads from the read-only WAL connection, §5.8).
    fn apply(
        &self,
        shape: &str,
        keys: &[RowKey],
        full_replace: bool,
        sqlite: &Arc<ShapeStore>,
        cx: &mut AsyncApp,
    ) {
        match shape {
            "teams" => apply_to(&self.teams, keys, full_replace, sqlite, cx),
            "boards" => apply_to(&self.boards, keys, full_replace, sqlite, cx),
            "issues" => apply_to(&self.issues, keys, full_replace, sqlite, cx),
            "labels" => apply_to(&self.labels, keys, full_replace, sqlite, cx),
            "issue_labels" => apply_to(&self.issue_labels, keys, full_replace, sqlite, cx),
            "users" => apply_to(&self.users, keys, full_replace, sqlite, cx),
            "team_members" => apply_to(&self.team_members, keys, full_replace, sqlite, cx),
            "team_invites" => apply_to(&self.team_invites, keys, full_replace, sqlite, cx),
            "comments" => apply_to(&self.comments, keys, full_replace, sqlite, cx),
            "attachments" => apply_to(&self.attachments, keys, full_replace, sqlite, cx),
            "notifications" => apply_to(&self.notifications, keys, full_replace, sqlite, cx),
            "issue_events" => apply_to(&self.issue_events, keys, full_replace, sqlite, cx),
            "issue_subscribers" => {
                apply_to(&self.issue_subscribers, keys, full_replace, sqlite, cx)
            }
            "coding_sessions" => apply_to(&self.coding_sessions, keys, full_replace, sqlite, cx),
            "actions" => apply_to(&self.actions, keys, full_replace, sqlite, cx),
            "issue_statuses" => {
                apply_to(&self.issue_statuses, keys, full_replace, sqlite, cx)
            }
            "devices" => apply_to(&self.devices, keys, full_replace, sqlite, cx),
            "device_worktrees" => {
                apply_to(&self.device_worktrees, keys, full_replace, sqlite, cx)
            }
            "automations" => apply_to(&self.automations, keys, full_replace, sqlite, cx),
            other => log::warn!("[sync] delta for unknown shape {other}"),
        }
    }

    /// Full hydrate of all 19 collections from SQLite (§5.8 "hydrate typed
    /// in-memory collections from SQLite at startup"). Runs synchronously on
    /// the foreground — deliberately: every batch committed to SQLite has a
    /// matching [`ShapeDelta`] queued behind this call, so a snapshot read
    /// here can never lose a concurrent write (the delta re-reads the row
    /// right after). A background hydrate would open exactly that race. The
    /// per-account working set is small; this is a few ms.
    fn hydrate_all(&self, sqlite: &Arc<ShapeStore>, cx: &mut App) {
        for_each_collection!(self, entity => hydrate_collection(entity, sqlite, cx));
    }

    fn clear_all(&self, cx: &mut App) {
        for_each_collection!(self, entity => entity.update(cx, |collection, cx| {
            collection.clear();
            cx.notify();
        }));
    }

    fn statuses(&self, cx: &App) -> Vec<ShapeStatus> {
        let mut out = Vec::with_capacity(19);
        for_each_collection!(self, entity => out.push(status_of(entity, cx)));
        out
    }

    fn observe_all<V: 'static>(&self, cx: &mut gpui::Context<V>) -> Vec<Subscription> {
        let mut out = Vec::with_capacity(19);
        for_each_collection!(self, entity => {
            out.push(cx.observe(entity, |_, _, cx| cx.notify()))
        });
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

fn apply_to<T: ShapeRow>(
    entity: &Entity<Collection<T>>,
    keys: &[RowKey],
    full_replace: bool,
    sqlite: &Arc<ShapeStore>,
    cx: &mut AsyncApp,
) {
    let spec = T::spec();
    let phase = phase_from_store(sqlite, spec.name);
    if full_replace {
        // §5.6c atomic refetch: the batch replaced the WHOLE table in one
        // commit — re-hydrate wholesale (point reads are not enough).
        let rows = match sqlite.read_all(spec) {
            Ok(maps) => decode_rows::<T>(maps),
            Err(err) => {
                log::warn!("[sync {}] refetch re-hydrate failed: {err}", spec.name);
                return;
            }
        };
        entity.update(cx, |collection, cx| {
            collection.replace_all(rows);
            collection.set_phase(phase);
            cx.notify();
        });
        return;
    }

    entity.update(cx, |collection, cx| {
        for key in keys {
            match sqlite.read_by_key(spec, key) {
                // Present in SQLite → it was an upsert.
                Ok(Some(map)) => match serde_json::from_value::<T>(Value::Object(map)) {
                    Ok(row) => collection.upsert(key.clone(), row),
                    Err(err) => {
                        log::warn!("[sync {}] dropping unhydratable row: {err}", spec.name)
                    }
                },
                // Gone from SQLite → it was a delete.
                Ok(None) => collection.remove(key),
                Err(err) => log::warn!("[sync {}] point read failed: {err}", spec.name),
            }
        }
        collection.set_phase(phase);
        cx.notify();
    });
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
    /// Build the store: the shared-state entity, the 16 collection entities,
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

    /// The 16 reactive collections.
    pub fn collections(&self) -> &Collections {
        &self.collections
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
                store.apply_delta(delta, cx);
            }
        })
        .detach();
    }

    fn apply_delta(&self, delta: ShapeDelta, cx: &mut AsyncApp) {
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
                up_to_date: _,
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
                self.collections
                    .apply(shape, &keys, full_replace, &sqlite, cx);
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
        // The 19 ShapeRow impls cover the registry exactly (a 20th shape
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
            CodingSession::spec().name,
            ActionRow::spec().name,
            IssueStatusRow::spec().name,
            DeviceRow::spec().name,
            DeviceWorktreeRow::spec().name,
            AutomationRow::spec().name,
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
            "status": "todo",
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
                "identifier": "EXP-1", "title": "ok", "status": "todo",
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
            "identifier": "EXP-1", "title": "t", "status": "todo",
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
