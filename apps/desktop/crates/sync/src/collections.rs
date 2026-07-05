//! The ONLY gpui-aware module in this crate (masterplan-v3 §3.1 / §5.8) — the
//! reactive seam between the headless sync engine and the view tree.
//!
//! Design (§5.8, mirrored from §3.5's threading model):
//!
//! * **One `gpui::Entity<Collection<T>>` per shape** (14 entities), all held
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
//! hard-401 gate: a dead token ROUTES TO LOGIN — the workspace renders
//! the login surface, never an empty board. Manager start/stop is wired to
//! the same transitions ([`Store::connect`] / [`Store::sign_out`] /
//! the drain's `Unauthorized` handling).

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, AppContext as _, AsyncApp, Entity, Global, Subscription};
use serde_json::{Map, Value};

use crate::client::{ShapeDelta, UnauthorizedFn};
use crate::manager::{AccountSyncConfig, SyncManager};
use crate::protocol::RowKey;
use crate::shapes::{shape_by_name, ShapeSpec};
use crate::store::{ShapeStore, StoreError};

use domain::rows::{
    Attachment, CodingSession, Comment, Issue, IssueEvent, IssueLabel, IssueSubscriber, Label,
    Notification, Project, User, Workspace, WorkspaceInvite, WorkspaceMember,
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
    /// Number of workspace windows currently open across the app.
    pub windows_open: usize,
    /// The §5 session state machine.
    pub session: SessionPhase,
}

// ---------------------------------------------------------------------------
// Per-shape reactive collections
// ---------------------------------------------------------------------------

/// Where a shape stands in its sync lifecycle — drives the debug board's
/// status line and the §4.1 `is_ready` skeleton-vs-empty distinction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeSyncPhase {
    /// No pipeline yet / no persisted cursor state.
    Waiting,
    /// Initial snapshot in progress (rows may be arriving).
    Snapshot,
    /// Caught up to head (`up-to-date` seen) — long-polling live.
    Live,
    /// A 409 / must-refetch was seen; the atomic re-snapshot is pending.
    /// Stale rows stay visible until it lands (§5.6c).
    Refetching,
}

impl ShapeSyncPhase {
    pub fn label(&self) -> &'static str {
        match self {
            ShapeSyncPhase::Waiting => "waiting",
            ShapeSyncPhase::Snapshot => "snapshot",
            ShapeSyncPhase::Live => "live",
            ShapeSyncPhase::Refetching => "refetching",
        }
    }
}

/// A typed row hydratable from the store's snake_case JSON objects. The 14
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

id_shape_row!(Workspace, "workspaces");
id_shape_row!(Project, "projects");
id_shape_row!(Issue, "issues");
id_shape_row!(Label, "labels");
id_shape_row!(User, "users");
id_shape_row!(WorkspaceMember, "workspace_members");
id_shape_row!(WorkspaceInvite, "workspace_invites");
id_shape_row!(Comment, "comments");
id_shape_row!(Attachment, "attachments");
id_shape_row!(Notification, "notifications");
id_shape_row!(IssueEvent, "issue_events");
id_shape_row!(IssueSubscriber, "issue_subscribers");
id_shape_row!(CodingSession, "coding_sessions");

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
pub struct Collection<T> {
    rows: HashMap<RowKey, T>,
    phase: ShapeSyncPhase,
    revision: u64,
}

impl<T> Collection<T> {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
            phase: ShapeSyncPhase::Waiting,
            revision: 0,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.rows.values()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Single-`id` lookup (composite-PK rows use [`Collection::get_key`]).
    pub fn get(&self, id: &str) -> Option<&T> {
        self.rows.get(&RowKey::Single(id.to_string()))
    }

    pub fn get_key(&self, key: &RowKey) -> Option<&T> {
        self.rows.get(key)
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
        matches!(self.phase, ShapeSyncPhase::Live | ShapeSyncPhase::Refetching)
    }

    fn set_phase(&mut self, phase: ShapeSyncPhase) {
        self.phase = phase;
    }

    fn replace_all(&mut self, rows: Vec<(RowKey, T)>) {
        self.rows = rows.into_iter().collect();
        self.revision += 1;
    }

    fn upsert(&mut self, key: RowKey, row: T) {
        self.rows.insert(key, row);
        self.revision += 1;
    }

    fn remove(&mut self, key: &RowKey) {
        if self.rows.remove(key).is_some() {
            self.revision += 1;
        }
    }

    fn clear(&mut self) {
        self.rows.clear();
        self.phase = ShapeSyncPhase::Waiting;
        self.revision += 1;
    }
}

/// Decode store-hydrated JSON objects into typed rows, dropping (and logging)
/// anything unhydratable — §5.5: a bad value is caught at hydrate, never at
/// apply, and never takes the batch down.
pub fn decode_rows<T: ShapeRow>(maps: Vec<Map<String, Value>>) -> Vec<(RowKey, T)> {
    maps.into_iter()
        .filter_map(|map| match serde_json::from_value::<T>(Value::Object(map)) {
            Ok(row) => Some((row.key(), row)),
            Err(err) => {
                log::warn!(
                    "[sync {}] dropping unhydratable row: {err}",
                    T::spec().name
                );
                None
            }
        })
        .collect()
}

/// One entry of the per-shape status line (the Phase-2 gate's "renders a
/// board" evidence surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeStatus {
    pub name: &'static str,
    pub phase: ShapeSyncPhase,
    pub rows: usize,
}

/// The 14 collection entities (§5.8). Cloning is cheap — `Entity` handles.
#[derive(Clone)]
pub struct Collections {
    pub workspaces: Entity<Collection<Workspace>>,
    pub projects: Entity<Collection<Project>>,
    pub issues: Entity<Collection<Issue>>,
    pub labels: Entity<Collection<Label>>,
    pub issue_labels: Entity<Collection<IssueLabel>>,
    pub users: Entity<Collection<User>>,
    pub workspace_members: Entity<Collection<WorkspaceMember>>,
    pub workspace_invites: Entity<Collection<WorkspaceInvite>>,
    pub comments: Entity<Collection<Comment>>,
    pub attachments: Entity<Collection<Attachment>>,
    pub notifications: Entity<Collection<Notification>>,
    pub issue_events: Entity<Collection<IssueEvent>>,
    pub issue_subscribers: Entity<Collection<IssueSubscriber>>,
    pub coding_sessions: Entity<Collection<CodingSession>>,
}

/// Run `$body` once per shape with `$entity` bound to that shape's collection
/// entity — the single dispatch point that keeps the 14-way fan-out in one
/// place.
macro_rules! for_each_collection {
    ($collections:expr, $entity:ident => $body:expr) => {{
        let $entity = &$collections.workspaces;
        $body;
        let $entity = &$collections.projects;
        $body;
        let $entity = &$collections.issues;
        $body;
        let $entity = &$collections.labels;
        $body;
        let $entity = &$collections.issue_labels;
        $body;
        let $entity = &$collections.users;
        $body;
        let $entity = &$collections.workspace_members;
        $body;
        let $entity = &$collections.workspace_invites;
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
    }};
}

impl Collections {
    fn new(cx: &mut App) -> Self {
        Self {
            workspaces: cx.new(|_| Collection::new()),
            projects: cx.new(|_| Collection::new()),
            issues: cx.new(|_| Collection::new()),
            labels: cx.new(|_| Collection::new()),
            issue_labels: cx.new(|_| Collection::new()),
            users: cx.new(|_| Collection::new()),
            workspace_members: cx.new(|_| Collection::new()),
            workspace_invites: cx.new(|_| Collection::new()),
            comments: cx.new(|_| Collection::new()),
            attachments: cx.new(|_| Collection::new()),
            notifications: cx.new(|_| Collection::new()),
            issue_events: cx.new(|_| Collection::new()),
            issue_subscribers: cx.new(|_| Collection::new()),
            coding_sessions: cx.new(|_| Collection::new()),
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
            "workspaces" => apply_to(&self.workspaces, keys, full_replace, sqlite, cx),
            "projects" => apply_to(&self.projects, keys, full_replace, sqlite, cx),
            "issues" => apply_to(&self.issues, keys, full_replace, sqlite, cx),
            "labels" => apply_to(&self.labels, keys, full_replace, sqlite, cx),
            "issue_labels" => apply_to(&self.issue_labels, keys, full_replace, sqlite, cx),
            "users" => apply_to(&self.users, keys, full_replace, sqlite, cx),
            "workspace_members" => {
                apply_to(&self.workspace_members, keys, full_replace, sqlite, cx)
            }
            "workspace_invites" => {
                apply_to(&self.workspace_invites, keys, full_replace, sqlite, cx)
            }
            "comments" => apply_to(&self.comments, keys, full_replace, sqlite, cx),
            "attachments" => apply_to(&self.attachments, keys, full_replace, sqlite, cx),
            "notifications" => apply_to(&self.notifications, keys, full_replace, sqlite, cx),
            "issue_events" => apply_to(&self.issue_events, keys, full_replace, sqlite, cx),
            "issue_subscribers" => {
                apply_to(&self.issue_subscribers, keys, full_replace, sqlite, cx)
            }
            "coding_sessions" => apply_to(&self.coding_sessions, keys, full_replace, sqlite, cx),
            other => log::warn!("[sync] delta for unknown shape {other}"),
        }
    }

    /// Full hydrate of all 14 collections from SQLite (§5.8 "hydrate typed
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
        let mut out = Vec::with_capacity(14);
        for_each_collection!(self, entity => out.push(status_of(entity, cx)));
        out
    }

    fn observe_all<V: 'static>(&self, cx: &mut gpui::Context<V>) -> Vec<Subscription> {
        let mut out = Vec::with_capacity(14);
        for_each_collection!(self, entity => {
            out.push(cx.observe(entity, |_, _, cx| cx.notify()))
        });
        out
    }
}

impl Collections {
    // -- workspace-scoped query helpers (§5.8: derived queries are plain Rust
    // over the in-memory collections; §4.1 moves the full set into
    // `ui/src/queries.rs` with the Phase-3 screens) ---------------------------

    /// All workspaces, name-sorted (the sidebar picker's read).
    pub fn workspaces_sorted(&self, cx: &App) -> Vec<Workspace> {
        let mut out: Vec<Workspace> = self.workspaces.read(cx).iter().cloned().collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    }

    /// A workspace's projects, sort-order-then-name sorted, archived hidden
    /// (web sidebar parity).
    pub fn projects_in_workspace(&self, workspace_id: &str, cx: &App) -> Vec<Project> {
        let mut out: Vec<Project> = self
            .projects
            .read(cx)
            .iter()
            .filter(|p| p.workspace_id == workspace_id && p.archived_at.is_none())
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            a.sort_order
                .unwrap_or(f64::MAX)
                .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        out
    }

    /// A project's issues, sort-order-then-identifier sorted, archived hidden
    /// (the board's base query; status grouping/filters sit on top).
    pub fn issues_in_project(&self, project_id: &str, cx: &App) -> Vec<Issue> {
        let mut out: Vec<Issue> = self
            .issues
            .read(cx)
            .iter()
            .filter(|i| i.project_id == project_id && i.archived_at.is_none())
            .cloned()
            .collect();
        sort_issues(&mut out);
        out
    }

    /// Every non-archived issue in a workspace (joins through the projects
    /// collection — referential integrity is a query-time concern, §5.4).
    pub fn issues_in_workspace(&self, workspace_id: &str, cx: &App) -> Vec<Issue> {
        let projects = self.projects.read(cx);
        let project_ids: std::collections::HashSet<&str> = projects
            .iter()
            .filter(|p| p.workspace_id == workspace_id)
            .map(|p| p.id.as_str())
            .collect();
        let mut out: Vec<Issue> = self
            .issues
            .read(cx)
            .iter()
            .filter(|i| project_ids.contains(i.project_id.as_str()) && i.archived_at.is_none())
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
}

impl Global for Store {}

impl Store {
    /// Build the store: the shared-state entity, the 14 collection entities,
    /// the [`SyncManager`], and the single foreground delta-drain task
    /// (§5.8). `on_unauthorized` is the §5.6b hook the app shell wires to
    /// `AuthStore::unauthorized_handler_fn()` — it deletes the dead token
    /// before the drain routes the UI to login.
    ///
    /// Pipelines start via [`Store::connect`] (login / warm-start resume),
    /// not here — `requireAuth` shapes must never be polled without a token
    /// (§5.9).
    pub fn open(cx: &mut App, on_unauthorized: Option<UnauthorizedFn>) -> Self {
        let state = cx.new(|_| SharedState {
            windows_open: 0,
            session: SessionPhase::SignedOut,
        });
        let collections = Collections::new(cx);
        let mut manager = SyncManager::new();
        if let Some(hook) = on_unauthorized {
            manager = manager.on_unauthorized(hook);
        }
        let store = Self {
            state,
            collections,
            manager: Arc::new(manager),
        };
        store.spawn_delta_drain(cx);
        store
    }

    /// Read the global store (panics if the shell has not installed it — the
    /// §3.6 bootstrap sets it before any window opens).
    pub fn global(cx: &App) -> &Self {
        cx.global::<Store>()
    }

    /// The shared cross-window state entity. Observe it for re-renders.
    pub fn state(&self) -> Entity<SharedState> {
        self.state.clone()
    }

    /// The current session phase (convenience read).
    pub fn session(&self, cx: &App) -> SessionPhase {
        self.state.read(cx).session.clone()
    }

    /// The 14 reactive collections.
    pub fn collections(&self) -> &Collections {
        &self.collections
    }

    /// Per-shape sync status snapshot — the debug board's status line.
    pub fn shape_statuses(&self, cx: &App) -> Vec<ShapeStatus> {
        self.collections.statuses(cx)
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

    /// `SigningIn/SignedOut → Synced`: start (or resume) the account's 14
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
        self.set_session(SessionPhase::Synced { account_id }, cx);
        Ok(())
    }

    /// `Synced/AuthExpired → SignedOut` (§5.10 sign-out): stop the pipeline
    /// (SQLite stays on disk for offline resume), clear the in-memory
    /// collections, route to login.
    pub fn sign_out(&self, account_id: &str, cx: &mut App) {
        self.manager.stop_account(account_id);
        self.collections.clear_all(cx);
        self.set_session(SessionPhase::SignedOut, cx);
    }

    fn set_session(&self, session: SessionPhase, cx: &mut App) {
        self.state.update(cx, |state, cx| {
            if state.session != session {
                state.session = session;
                cx.notify();
            }
        });
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
                    if state.session.account_id() == Some(account_id.as_str())
                        && !matches!(state.session, SessionPhase::AuthExpired { .. })
                    {
                        state.session = SessionPhase::AuthExpired { account_id };
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
                // Only the active account's deltas feed the collections
                // (background accounts still sync to their own SQLite).
                let active = self
                    .state
                    .read_with(cx, |state, _| state.session.account_id().map(String::from));
                if active.as_deref() != Some(account_id.as_str()) {
                    return;
                }
                let Some(sqlite) = self.manager.store(&account_id) else {
                    return;
                };
                self.collections
                    .apply(shape, &keys, full_replace, &sqlite, cx);
            }
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

    #[test]
    fn every_shape_has_a_typed_row_binding() {
        // The 14 ShapeRow impls cover the registry exactly (a 15th shape
        // without a typed row would silently never reach the UI).
        let bound = [
            Workspace::spec().name,
            Project::spec().name,
            Issue::spec().name,
            Label::spec().name,
            IssueLabel::spec().name,
            User::spec().name,
            WorkspaceMember::spec().name,
            WorkspaceInvite::spec().name,
            Comment::spec().name,
            Attachment::spec().name,
            Notification::spec().name,
            IssueEvent::spec().name,
            IssueSubscriber::spec().name,
            CodingSession::spec().name,
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
            "project_id": "p-1",
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
                "id": "i-1", "project_id": "p-1", "number": "1",
                "identifier": "EXP-1", "title": "ok", "status": "todo",
                "priority": "none"
            })),
            // Missing required identifier/title → dropped, not fatal (§5.5).
            obj(json!({ "id": "i-2", "project_id": "p-1", "number": "2" })),
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
            "id": "i-1", "project_id": "p-1", "number": 1,
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
