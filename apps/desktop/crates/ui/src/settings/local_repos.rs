//! Settings → Worktrees (masterplan v4 §4.7; nav label renamed in EXP-490).
//!
//! A **desktop-local** section (like the §7.7 Coding pane — per install, never
//! synced, not owner-gated): every trunk clone under the coding `repos_root`
//! with its on-disk size, its worktrees, and two maintenance actions.
//!
//! - **Two-phase scan** (EXP-490): the walk is split so the rows never wait on
//!   the disk-usage numbers. Phase 1 enumerates the clones (`read_dir` two
//!   levels) and lists each one's worktrees (`git worktree list`) — tens of
//!   milliseconds — and lands as [`Scan::Ready`] straight away. Phase 2 is a
//!   follow-up background walk that measures each clone (`du`-style, clone +
//!   `.worktrees`, seconds per tree) and merges the sizes in as they arrive;
//!   a row shows a small skeleton until its number lands. Both phases run on
//!   the background executor (never the gpui foreground) under ONE
//!   `generation` guard, so a stale phase can never clobber a newer scan.
//!   Measured sizes are cached by clone path and carried across re-scans (a
//!   [`SIZE_TTL`] keeps them from re-walking on every auto-invalidation); the
//!   Refresh button and the destructive actions drop the cache instead.
//! - **Auto-rescan** (EXP-490): the pane invalidates itself when the synced
//!   `device_worktrees` collection changes (remote removes, the 120s
//!   auto-prune, launcher creates — this machine reports its own rows) and
//!   when the process-global `LocalSessions` registry changes (a session
//!   start/end creates or frees a worktree locally). Phase 1 is cheap enough
//!   that liberal invalidation is free. Refresh stays as the manual escape
//!   hatch.
//! - **Worktrees** (EXP-369, regrouped in EXP-694): the scan carries each
//!   clone's linked worktrees, and the pane renders ALL of them as ONE flat
//!   hairline LADDER under the band (EXP-1076) — no per-clone
//!   nesting and no expander (a clone with no worktrees simply contributes no
//!   row; its maintenance row still appears in the "Local repositories" group
//!   below). Per worktree: a confirmed force-remove and a terminal button
//!   whose dropdown mirrors the terminal dock's "+" (installed agents + "New
//!   shell"), launched at THAT worktree's path. The force-remove is
//!   **blocked while a coding session holds that worktree's branch** — the
//!   same guard as "Remove local copy", one level down (a `--force` remove
//!   would otherwise yank a running agent's cwd).
//! - **Prune merged worktrees** (EXP-465): [`coding::prune::prune_landed`]
//!   under the [`crate::worktree_prune`] policy — worktrees whose work has
//!   LANDED on the default branch (merged PR, or git-confirmed for finished/
//!   deleted issues and stale batch branches; squash merges detected) go,
//!   along with their branches and any stale landed prefix branches. Tracked
//!   modifications always skip (reported); untracked-only debris does not.
//!   All git ops are `std::process::Command("git")` with explicit argv
//!   (masterplan L5) — no `gh`, no git library, no shell. EXP-694: the
//!   section header carries the broom, which sweeps EVERY
//!   clone (each still under its own policy and its own inline report); a
//!   clone row keeps the same broom for itself alone.
//! - **Remove local copy**: delete the clone dir + its `.worktrees` sibling
//!   behind a confirm dialog. **Blocked while a coding session is running** on
//!   one of the clone's worktrees (the Remove button disables with the reason).
//!
//! No auto-GC (§4.7): every deletion is an explicit, confirmed user action.
//!
//! EXP-1020: this is THE worktrees surface. The device settings dialog's
//! section (and the remote `worktree_remove` / `worktree_prune` queue behind
//! it) is gone on all four clients — a machine's worktrees are LOCAL, so
//! they are cleaned where they live. The section also says when the repos
//! root's filesystem is running low, since worktrees are the one thing here
//! that grows without being asked.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use sync::Store;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};

use coding::git_worktree::{sanitize_branch_for_path, worktrees_dir};
use coding::CodingAgent;

use crate::coding_flow::{CodingHub, LocalSessions};
use crate::file_tree::{self, OpenAgentShellHere, OpenTerminalHere};
use crate::native_dialog::{self, AlertSpec};
use crate::repo_resolver::{repo_resolver_for_window, RepoResolver};

use super::doc::{and, empty_state, faded, hex, path, IdeToasts, Ink, PillPaint, SM, VALUE_ALPHA, XS};

// ---------------------------------------------------------------------------
// Background scan model
// ---------------------------------------------------------------------------

/// One trunk clone found under `repos_root` (`<owner>/<name>`), with its cached
/// disk usage (clone + `.worktrees`) and its linked worktrees.
#[derive(Clone)]
struct RepoEntry {
    full_name: String,
    clone_path: PathBuf,
    /// `None` while phase 2 is still measuring this clone (the row shows a
    /// skeleton); a carried-forward cache entry fills it in immediately on a
    /// re-scan.
    size_bytes: Option<u64>,
    worktrees: Vec<WorktreeRow>,
}

/// One LINKED worktree of a clone (the main working tree is never listed).
#[derive(Clone)]
struct WorktreeRow {
    path: PathBuf,
    /// The checked-out branch — `None` for a detached worktree, and for every
    /// entry of the git-less directory fallback.
    branch: Option<String>,
}

impl WorktreeRow {
    /// The row's headline: the branch it holds, else its directory name.
    fn label(&self) -> SharedString {
        match &self.branch {
            Some(branch) => SharedString::from(branch.clone()),
            None => SharedString::from(
                self.path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| self.path.to_string_lossy().into_owned()),
            ),
        }
    }
}

enum Scan {
    /// Nothing to show yet (first paint, or the root just changed) — the next
    /// render kicks phase 1 and the list paints as skeletons meanwhile.
    Idle,
    /// The last phase-1 result. A re-scan KEEPS it on screen (the rows are
    /// what the user is reading; a 30ms phase 1 must not blink them away) —
    /// `stale`/`scanning` carry the "a walk is pending/running" state instead.
    Ready(Vec<RepoEntry>),
}

/// Per-clone transient action state (busy spinner + last result line). Keyed by
/// `full_name` and NOT cleared on re-scan, so a "Removed 2, skipped 1" summary
/// survives the refresh a prune triggers.
#[derive(Default)]
struct ActionState {
    busy: bool,
    /// The muted result of the last prune/remove (a failure is a toast,
    /// FEED-79).
    message: Option<SharedString>,
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

/// How long a measured clone size stays good enough to skip a re-walk. The
/// walk is seconds of `lstat` per clone while the auto-invalidations can
/// arrive on every synced `device_worktrees` echo, so phase 2 re-measures only
/// what is unknown or stale. The paths that DO change a size on purpose
/// (refresh, prune, remove) drop the cache entry instead of waiting this out.
const SIZE_TTL: Duration = Duration::from_secs(60);

/// EXP-1020: how long a free-space reading stays good enough. One `statvfs`
/// is cheap, but the pane re-renders on every synced echo and a syscall per
/// frame is still a syscall per frame.
const DISK_TTL: Duration = Duration::from_secs(30);

/// EXP-1020: when the Worktrees section warns. Worktrees are the one thing
/// on this page that GROWS without being asked, so the page that can delete
/// them is where a full disk has to be said out loud — either bound trips
/// it, so a small SSD warns on the fraction and a large one on the absolute
/// headroom.
const DISK_LOW_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const DISK_LOW_FRACTION: f64 = 0.10;

/// The filesystem holding `path`: `(free, capacity)` in bytes. `None` when
/// the path does not exist yet, the syscall refuses, or the platform has no
/// probe here — an unknown disk never warns.
///
/// `statvfs` is unix-only (libc does not declare it on Windows, and
/// build-desktop.yml does build an msvc target), so the windows arm returns
/// `None` until someone wires `GetDiskFreeSpaceExW`. A missing warning is
/// the safe direction: the section still works, it just says nothing about
/// headroom.
#[cfg(unix)]
fn disk_free(path: &Path) -> Option<(u64, u64)> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: `c_path` is a valid NUL-terminated string for the call, and
    // `stat` is written only on success (the zeroed value is never read).
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        let unit = stat.f_frsize as u64;
        Some((stat.f_bavail as u64 * unit, stat.f_blocks as u64 * unit))
    }
}

#[cfg(not(unix))]
fn disk_free(_path: &Path) -> Option<(u64, u64)> {
    None
}

/// EXP-1558 (DEV-ONLY, `EXP_DEV_DISK_FREE=<bytes>`): the headroom the
/// `settings-worktrees` capture reads instead of the capture Mac's own disk
/// (its free space is no fact of the product), against a 500 GB disk.
fn dev_disk_free() -> Option<(u64, u64)> {
    let free = std::env::var("EXP_DEV_DISK_FREE").ok()?.trim().parse().ok()?;
    Some((free, 500_000_000_000))
}

/// Whether `(free, capacity)` is low enough to say so.
fn disk_is_low(free: u64, capacity: u64) -> bool {
    if capacity == 0 {
        return false;
    }
    free < DISK_LOW_BYTES || (free as f64 / capacity as f64) < DISK_LOW_FRACTION
}

pub struct LocalReposPane {
    scan: Scan,
    /// The `repos_root` the current `scan` belongs to; a settings change
    /// (Coding pane) re-scans.
    scanned_root: Option<PathBuf>,
    /// Measured disk usage by clone path, `(bytes, when measured)` — survives
    /// re-scans so an auto-invalidation refills the rows instead of flashing
    /// skeletons, and feeds the [`SIZE_TTL`] freshness check.
    sizes: HashMap<PathBuf, (u64, Instant)>,
    /// EXP-1020: the repos root's filesystem headroom, `((free, capacity),
    /// when read)` — see [`DISK_TTL`].
    disk: Option<((u64, u64), Instant)>,
    /// The scan no longer describes the disk (an action, a Refresh click, or
    /// one of the EXP-490 auto-invalidations) — the next render re-walks.
    stale: bool,
    /// Phase 1 is in flight (the Refresh button's spinner).
    scanning: bool,
    /// Monotonic guard: a stale in-flight scan (either phase) must not clobber
    /// a newer one.
    generation: u64,
    actions: HashMap<String, ActionState>,
    /// The window's shared repo resolver, bound on first render (the pane is
    /// constructed without a window): it maps a scanned `owner/name` back to
    /// the team's `repositories` row id, which the agent launches need.
    resolver: Option<Entity<RepoResolver>>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's state (headless has no clones or sessions).
    #[cfg(test)]
    shot: Option<LocalShot>,
    _subscriptions: Vec<Subscription>,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct LocalShot {
    /// The clones: full name, size (`None` = still measuring), worktrees
    /// (directory name, branch); `None` = the first scan is still out.
    pub repos: Option<Vec<(String, Option<u64>, Vec<(String, Option<String>)>)>>,
    /// The disk's free and capacity bytes.
    pub disk: Option<(u64, u64)>,
    /// Clones a live session holds.
    pub in_use: Vec<String>,
    /// A clone mid-prune and the report under a clone: (full name, text).
    pub busy: Option<String>,
    pub message: Option<(String, String)>,
}

impl LocalReposPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // EXP-369: the worktree rows dispatch the file-tree terminal actions;
        // registration is a process-wide `Once`, so claiming it here keeps the
        // pane working even in a window whose file tree was never built.
        file_tree::ensure_actions_registered(cx);
        // The repos root lives in the coding hub (Coding pane edits it); the
        // running-session gate reads the synced sessions collection.
        let hub = CodingHub::global(cx);
        let collections = Store::global(cx).collections().clone();
        let local_sessions = LocalSessions::global(cx);
        let subscriptions = vec![
            // Repos-root edits only re-render; `ensure_scanned` re-scans off
            // the changed `scanned_root` by itself.
            cx.observe(&hub, |_, _, cx| cx.notify()),
            // The busy gates read the synced rows (incl. OTHER devices'
            // sessions, which never move a local worktree) — notify only.
            cx.observe(&collections.sessions, |_, _, cx| cx.notify()),
            // EXP-490: worktrees this pane doesn't own appear and vanish —
            // launcher creates, the 120s auto-prune, remote device commands.
            // Every one of them lands in the synced inventory this machine
            // reports, so its deltas are the pane's invalidation feed.
            cx.observe(&collections.device_worktrees, |this, _, cx| {
                this.stale = true;
                cx.notify();
            }),
            // Local session start/end creates or frees a worktree NOW; the
            // synced echo above is up to a heartbeat behind.
            cx.observe(&local_sessions, |this, _, cx| {
                this.stale = true;
                cx.notify();
            }),
        ];
        // Every action leaves the binding for the pane (the confirms and the
        // terminal dispatch need its window).
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/pane", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(std::rc::Rc::new(IdeToasts));
            for name in ["refresh", "pruneAll", "prune", "removeRepo", "removeWorktree", "terminal"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &key, &value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let chrome = Chrome {
            foreground: hex(theme.foreground),
            muted: hex(theme.muted_foreground),
            danger: hex(theme.danger),
            warning: hex(theme.warning),
            ghost_hover: hex(theme.secondary_hover),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
            skeleton_value: hex(theme.skeleton),
            radius_md: eui_gpui::chrome::Chrome::global(cx).radius_md,
        };
        let plugin: std::rc::Rc<dyn HostPlugin> = std::rc::Rc::new(PanePlugin { inner: super::doc::ide_plugin() });
        let view = super::doc::mount_with("settings-local-repos", document(&Ink::of(cx), &chrome), &binding, plugin, true, window, cx);
        Self {
            binding,
            view,
            scan: Scan::Idle,
            scanned_root: None,
            sizes: HashMap::new(),
            disk: None,
            stale: false,
            scanning: false,
            generation: 0,
            actions: HashMap::new(),
            resolver: None,
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    /// The pane in a pinned state, its clones under `/bench/repos`.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: LocalShot, cx: &mut gpui::Context<Self>) {
        let root = PathBuf::from("/bench/repos");
        // A scan the first frame started (the real root) never lands.
        self.generation += 1;
        self.scanning = false;
        self.scanned_root = Some(root.clone());
        self.scan = match &shot.repos {
            None => Scan::Idle,
            Some(repos) => Scan::Ready(
                repos
                    .iter()
                    .map(|(name, size, worktrees)| RepoEntry {
                        full_name: name.clone(),
                        clone_path: root.join(name),
                        size_bytes: *size,
                        worktrees: worktrees
                            .iter()
                            .map(|(dir, branch)| WorktreeRow { path: root.join(format!("{name}.worktrees")).join(dir), branch: branch.clone() })
                            .collect(),
                    })
                    .collect(),
            ),
        };
        self.disk = shot.disk.map(|disk| (disk, Instant::now()));
        if let Some(name) = &shot.busy {
            self.action_mut(name).busy = true;
        }
        if let Some((name, text)) = &shot.message {
            self.action_mut(name).message = Some(SharedString::from(text.clone()));
        }
        self.shot = Some(shot);
        cx.notify();
    }

    /// The repos root (the bench's, under test).
    fn root(&self, cx: &mut App) -> PathBuf {
        #[cfg(test)]
        if self.shot.is_some() {
            return PathBuf::from("/bench/repos");
        }
        CodingHub::global(cx).read(cx).settings.repos_root_path()
    }

    /// Whether a live session holds `clone` (the bench's list, under test).
    fn clone_held(&self, clone: &Path, live: &HashSet<String>) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.in_use.iter().any(|name| clone.ends_with(name));
        }
        clone_in_use(clone, live)
    }

    /// Bind (once) the window's shared repo resolver and keep it warm — the
    /// worktree rows need `repositories.list` to turn a local clone's
    /// `owner/name` into the `repository_id` an agent launch mints against.
    fn ensure_resolver(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.resolver.is_none() {
            let resolver = repo_resolver_for_window(window, cx);
            self._subscriptions
                .push(cx.observe(&resolver, |_, _, cx| cx.notify()));
            self.resolver = Some(resolver);
        }
        if let Some(resolver) = self.resolver.clone() {
            resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
        }
    }

    /// `owner/name` → the team's `repositories` row id, for the clones the
    /// resolver knows. Empty while `repositories.list` is still loading.
    fn repository_ids(&self, cx: &App) -> HashMap<String, String> {
        self.resolver
            .as_ref()
            .and_then(|resolver| resolver.read(cx).all_repos())
            .map(|repos| {
                repos
                    .iter()
                    .map(|repo| (repo.full_name.clone(), repo.repository_id.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Kick phase 1 of the background walk when the root changed or the scan
    /// was invalidated (refresh / post-action / auto). Runs at render time so a
    /// hidden pane never scans.
    ///
    /// Phase 1 only enumerates clones and lists their worktrees (~tens of ms),
    /// so the rows paint immediately; [`Self::measure_sizes`] fills the disk
    /// usage in afterwards under the same `generation`.
    fn ensure_scanned(&mut self, root: PathBuf, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
        let same_root = self.scanned_root.as_ref() == Some(&root);
        let have_result = matches!(self.scan, Scan::Ready(_));
        if same_root && !self.stale && (self.scanning || have_result) {
            return;
        }
        if !same_root {
            // Another root's rows are not this root's — drop them.
            self.scan = Scan::Idle;
        }
        self.stale = false;
        self.scanning = true;
        self.scanned_root = Some(root.clone());
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let entries = cx
                .background_executor()
                .spawn(async move { scan_repos(&root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer scan (which owns `scanning`)
                }
                this.scanning = false;
                // Clones that went away stop holding a cached size.
                let present: HashSet<PathBuf> =
                    entries.iter().map(|entry| entry.clone_path.clone()).collect();
                this.sizes.retain(|path, _| present.contains(path));

                let now = Instant::now();
                let mut unmeasured = Vec::new();
                let entries: Vec<RepoEntry> = entries
                    .into_iter()
                    .map(|mut entry| {
                        match this.sizes.get(&entry.clone_path) {
                            Some((bytes, measured_at)) => {
                                // Carry the last number forward as the
                                // placeholder — an auto-invalidation must not
                                // flash a skeleton over a size that is still
                                // essentially right.
                                entry.size_bytes = Some(*bytes);
                                if now.duration_since(*measured_at) >= SIZE_TTL {
                                    unmeasured.push(entry.clone_path.clone());
                                }
                            }
                            None => unmeasured.push(entry.clone_path.clone()),
                        }
                        entry
                    })
                    .collect();
                this.scan = Scan::Ready(entries);
                cx.notify();
                this.measure_sizes(unmeasured, generation, cx);
            });
        })
        .detach();
    }

    /// Phase 2: walk each clone's disk usage (clone + `.worktrees`) on the
    /// background executor and merge the numbers into [`Scan::Ready`] one
    /// clone at a time, so the rows fill in progressively instead of waiting
    /// on the slowest tree. Guarded by the SAME `generation` as phase 1: a
    /// superseded pass drops its result and stops walking.
    fn measure_sizes(
        &mut self,
        clones: Vec<PathBuf>,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if clones.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            for clone in clones {
                let measured = {
                    let clone = clone.clone();
                    cx.background_executor()
                        .spawn(async move { repo_disk_size(&clone) })
                        .await
                };
                let current = this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return false; // superseded by a newer scan
                        }
                        this.sizes.insert(clone.clone(), (measured, Instant::now()));
                        if let Scan::Ready(entries) = &mut this.scan {
                            if let Some(entry) =
                                entries.iter_mut().find(|entry| entry.clone_path == clone)
                            {
                                entry.size_bytes = Some(measured);
                            }
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !current {
                    return;
                }
            }
        })
        .detach();
    }

    /// The manual escape hatch: a full re-scan INCLUDING the size walk (the
    /// auto-invalidations lean on the size cache, this one distrusts it).
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.sizes.clear();
        self.stale = true;
        cx.notify();
    }

    fn action_mut(&mut self, full_name: &str) -> &mut ActionState {
        self.actions.entry(full_name.to_string()).or_default()
    }

    /// Prune the clone's landed worktrees + stale branches (EXP-465): derive
    /// the issue-state policy on the foreground (repo-scoped when the
    /// resolver knows this clone, git-truth-only otherwise), run
    /// [`coding::prune::prune_landed`] off it, report inline. Tracked changes
    /// always skip; untracked-only debris goes with the worktree (§4.7's
    /// "explicit user action" — this button IS the confirmation).
    fn run_prune(
        &mut self,
        full_name: String,
        clone: PathBuf,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.action_mut(&full_name).busy {
            return;
        }
        let resolved = self
            .resolver
            .as_ref()
            .and_then(|resolver| resolver.read(cx).all_repos())
            .and_then(|repos| repos.iter().find(|repo| repo.full_name == full_name))
            .map(|repo| (repo.repository_id.clone(), repo.default_branch.clone()));
        let policy = match resolved {
            Some((repository_id, default_branch)) => {
                crate::worktree_prune::prune_policy_for_repo(
                    &repository_id,
                    default_branch,
                    window,
                    cx,
                )
            }
            None => crate::worktree_prune::prune_policy_unscoped(window, cx),
        };
        self.action_mut(&full_name).busy = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let measured = clone.clone();
            let report = cx
                .background_executor()
                .spawn(async move { coding::prune::prune_landed(&clone, &policy) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let entry = this.action_mut(&full_name);
                entry.busy = false;
                entry.message = Some(format_prune_result(&report));
                // Counts and size moved — re-scan, and re-measure THIS clone
                // instead of carrying the pre-prune number forward.
                this.sizes.remove(&measured);
                this.stale = true;
                cx.notify();
            });
        })
        .detach();
    }

    /// Delete the clone + its `.worktrees` sibling off the foreground, then
    /// re-scan (the row disappears on success; a failure surfaces inline).
    fn run_remove(&mut self, full_name: String, clone: PathBuf, cx: &mut gpui::Context<Self>) {
        if self.action_mut(&full_name).busy {
            return;
        }
        self.action_mut(&full_name).busy = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { remove_local_copy(&clone) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.actions.remove(&full_name);
                        this.stale = true;
                    }
                    // FEED-79: a failed remove is the error toast.
                    Err(message) => {
                        this.action_mut(&full_name).busy = false;
                        crate::action_error::failed_action(
                            "remove",
                            "the local copy",
                            Some(message.to_string()),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Force-remove ONE worktree off the foreground, then re-scan. `--force`
    /// is deliberate here (the confirm warned about uncommitted work) —
    /// unlike the prune path, which never forces.
    fn run_remove_worktree(
        &mut self,
        full_name: String,
        clone: PathBuf,
        worktree: PathBuf,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.action_mut(&full_name).busy {
            return;
        }
        self.action_mut(&full_name).busy = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let measured = clone.clone();
            let result = cx
                .background_executor()
                .spawn(async move { remove_worktree(&clone, &worktree) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let entry = this.action_mut(&full_name);
                entry.busy = false;
                entry.message = match result {
                    Ok(()) => Some("Removed 1 worktree.".into()),
                    // FEED-79: a failed remove is the error toast.
                    Err(detail) => {
                        crate::action_error::failed_action(
                            "remove",
                            "the worktree",
                            Some(detail.to_string()),
                            cx,
                        );
                        None
                    }
                };
                // A worktree's bytes just went away — don't carry the stale
                // number forward across the re-scan.
                this.sizes.remove(&measured);
                this.stale = true;
                cx.notify();
            });
        })
        .detach();
    }

    /// The confirm dialog for a worktree trash click — `--force` discards
    /// uncommitted work, so the warning has to say so. Only reached when the
    /// trash button is enabled (no live session on the worktree's branch).
    fn confirm_remove_worktree(
        &self,
        full_name: String,
        clone: PathBuf,
        worktree: WorktreeRow,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane = cx.entity();
        let path = worktree.path.clone();
        let spec = AlertSpec::new(
            "Remove worktree",
            format!(
                "This deletes the {} worktree at {} from disk, including any \
                 uncommitted or untracked changes in it. The branch itself is \
                 kept.",
                worktree.label(),
                path.display()
            ),
            "Remove worktree",
        )
        .ok_variant(gpui_component::button::ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let full_name = full_name.clone();
            let clone = clone.clone();
            let path = path.clone();
            pane.update(cx, |this, cx| {
                this.run_remove_worktree(full_name, clone, path, cx);
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// EXP-694: the Worktrees section's prune affordance is MACHINE-wide —
    /// the flat list is not grouped by clone any more, so neither is the
    /// sweep. Each clone still runs its
    /// own [`Self::run_prune`] (its own policy, its own inline report).
    fn run_prune_all(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let clones: Vec<(String, PathBuf)> = match &self.scan {
            Scan::Ready(entries) => entries
                .iter()
                .filter(|entry| !entry.worktrees.is_empty())
                .map(|entry| (entry.full_name.clone(), entry.clone_path.clone()))
                .collect(),
            Scan::Idle => Vec::new(),
        };
        for (full_name, clone) in clones {
            self.run_prune(full_name, clone, window, cx);
        }
    }

    /// EXP-1020: the repos root's headroom, re-read at most every
    /// [`DISK_TTL`]. Takes `&mut self` because the reading is cached on the
    /// pane, and it is read from the render.
    fn disk_headroom(&mut self, root: &Path) -> Option<(u64, u64)> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.disk;
        }
        if let Some((reading, at)) = self.disk {
            if at.elapsed() < DISK_TTL {
                return Some(reading);
            }
        }
        let reading = dev_disk_free().or_else(|| disk_free(root))?;
        self.disk = Some((reading, Instant::now()));
        Some(reading)
    }

    /// Whether any clone is mid-prune/-remove (the header broom's spinner).
    fn any_busy(&self) -> bool {
        self.actions.values().any(|action| action.busy)
    }

    /// The confirm dialog for "Remove local copy" (web `boards.delete`
    /// pattern). Only reached when the Remove button is enabled (no running
    /// session); the pane entity handle carries the action into `on_ok`.
    fn confirm_remove(
        &self,
        full_name: String,
        clone: PathBuf,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane = cx.entity();
        let spec = AlertSpec::new(
            "Remove local copy",
            format!(
                "This deletes the local clone of {full_name} and all its \
                 worktrees from disk. Your work on GitHub is untouched; \
                 the clone re-creates on the next \u{201c}Start coding\u{201d}."
            ),
            "Remove local copy",
        )
        .ok_variant(gpui_component::button::ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let full_name = full_name.clone();
            let clone = clone.clone();
            pane.update(cx, |this, cx| {
                this.run_remove(full_name, clone, cx);
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// One CLONE row of the "Local repositories" group (EXP-694): name, disk
    /// usage + worktree count, its own broom and the confirmed
    /// "Remove local copy". The worktrees themselves are no longer nested
    /// here — they are the flat group above, across every clone.
    /// The worktree row's terminal menu: every installed agent (inert
    /// without a matching team repository: no installation token to mint),
    /// then "New shell" (EXP-697: no dividers in menus).
    fn terminal_items(installed: &[CodingAgent], has_repository: bool) -> Vec<Value> {
        let mut items: Vec<Value> = installed
            .iter()
            .map(|agent| json!({"label": agent.label(), "value": format!("agent:{}", agent.id()), "icon": agent_icon_name(*agent), "disabled": !has_repository}))
            .collect();
        items.push(json!({"label": "New shell", "value": "shell", "icon": "nav-terminal"}));
        items
    }

    /// The state as the document binds it.
    fn project(&mut self, root: &Path, cx: &mut gpui::Context<Self>) -> Value {
        let installed: Vec<CodingAgent> = CodingHub::global(cx)
            .read(cx)
            .doctor
            .report
            .as_ref()
            .map(|report| report.installed_agents())
            .unwrap_or_default();
        let sweeping = self.any_busy();
        let disk = self.disk_headroom(root).filter(|(free, capacity)| disk_is_low(*free, *capacity));
        let mut out = json!({
            "root": root.to_string_lossy(),
            "diskLow": disk.is_some(),
            "disk": disk.map(|(free, _)| format!("Only {} free on this disk. Pruning merged worktrees frees what the agents left behind.", format_size(free))).unwrap_or_default(),
            "scanning": self.scanning,
            "idle": !self.scanning,
            "sweeping": sweeping,
            "loading": matches!(self.scan, Scan::Idle),
        });
        let Scan::Ready(repos) = &self.scan else {
            out["noRepos"] = json!(false);
            out["hasRepos"] = json!(false);
            out["pruneAllOff"] = json!(true);
            out["pruneAllOn"] = json!(false);
            return out;
        };
        let live = live_session_branches(cx);
        let repository_ids = self.repository_ids(cx);
        let mut worktrees = Vec::new();
        let mut clones = Vec::new();
        for repo in repos {
            let action = self.actions.get(&repo.full_name);
            let busy = action.is_some_and(|action| action.busy);
            let worktrees_used: Vec<bool> = repo
                .worktrees
                .iter()
                .map(|worktree| worktree_in_use(&repo.clone_path, worktree, &live) || worktree.branch.as_deref().is_some_and(|branch| local_session_on_branch(branch, cx)))
                .collect();
            // A held worktree holds its clone too, even when the
            // branch→dir mapping didn't resolve.
            let in_use = self.clone_held(&repo.clone_path, &live) || worktrees_used.iter().any(|used| *used);
            let has_repository = repository_ids.contains_key(&repo.full_name);
            for (ix, worktree) in repo.worktrees.iter().enumerate() {
                let used = worktrees_used.get(ix).copied().unwrap_or(false);
                worktrees.push(json!({
                    "key": format!("{}\u{0}{}", repo.full_name, worktree.path.to_string_lossy()),
                    "repo": repo.full_name,
                    "label": worktree.label().to_string(),
                    "path": worktree.path.to_string_lossy(),
                    "items": Self::terminal_items(&installed, has_repository),
                    "removeOff": busy || used,
                    "removeOn": !(busy || used),
                    "held": used,
                    "free": !used,
                    // A `--force` remove would yank a running agent's cwd.
                    "removeTip": if used { "A session is active on this worktree. Stop it first." } else { "Remove this worktree from disk (confirmed)" },
                }));
            }
            let count = repo.worktrees.len();
            clones.push(json!({
                "key": repo.full_name,
                "name": repo.full_name,
                "sized": repo.size_bytes.is_some(),
                "measuring": repo.size_bytes.is_none(),
                "size": repo.size_bytes.map(format_size).unwrap_or_default(),
                "count": if count == 1 { "1 worktree".to_string() } else { format!("{count} worktrees") },
                "pruning": busy,
                "pruneOff": !busy && count == 0,
                "pruneOn": !busy && count > 0,
                "held": in_use,
                "free": !in_use,
                "removeOff": busy || in_use,
                "removeTip": if in_use { "A session is active on this repository. Stop it first." } else { "Delete the local clone and its worktrees from disk (confirmed)." },
                "hasMessage": action.is_some_and(|action| action.message.is_some()),
                "message": action.and_then(|action| action.message.clone()).map(|m| m.to_string()).unwrap_or_default(),
            }));
        }
        let has_worktrees = !worktrees.is_empty();
        out["noRepos"] = json!(repos.is_empty());
        out["hasRepos"] = json!(!repos.is_empty());
        out["noWorktrees"] = json!(!has_worktrees);
        out["hasWorktrees"] = json!(has_worktrees);
        out["pruneAllOff"] = json!(!sweeping && !has_worktrees);
        out["pruneAllOn"] = json!(!sweeping && has_worktrees);
        out["worktrees"] = json!(worktrees);
        out["repos"] = json!(clones);
        out
    }

    /// A document action: the band's Refresh and Prune-all, a clone's Prune
    /// and Remove local copy, a worktree's Remove and terminal pick.
    fn action(&mut self, name: &str, key: &str, value: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Scan::Ready(repos) = &self.scan else {
            if name == "refresh" {
                self.refresh(cx);
            }
            return;
        };
        let (repo_name, worktree_path) = key.split_once('\u{0}').unwrap_or((key, ""));
        let Some(repo) = repos.iter().find(|repo| repo.full_name == repo_name).cloned().or_else(|| (name == "refresh" || name == "pruneAll").then(|| repos[0].clone())) else {
            if name == "refresh" {
                self.refresh(cx);
            }
            return;
        };
        match name {
            "refresh" => self.refresh(cx),
            "pruneAll" => self.run_prune_all(window, cx),
            "prune" => self.run_prune(repo.full_name.clone(), repo.clone_path.clone(), window, cx),
            "removeRepo" => self.confirm_remove(repo.full_name.clone(), repo.clone_path.clone(), window, cx),
            "removeWorktree" => {
                if let Some(worktree) = repo.worktrees.iter().find(|w| w.path.to_string_lossy() == worktree_path).cloned() {
                    self.confirm_remove_worktree(repo.full_name.clone(), repo.clone_path.clone(), worktree, window, cx);
                }
            }
            "terminal" => {
                let path = worktree_path.to_string();
                if value == "shell" {
                    window.dispatch_action(Box::new(OpenTerminalHere { path }), cx);
                } else if let Some(agent) = value.strip_prefix("agent:") {
                    if let Some(repository_id) = self.repository_ids(cx).get(&repo.full_name).cloned() {
                        window.dispatch_action(Box::new(OpenAgentShellHere { agent: agent.to_string(), repository_id, full_name: repo.full_name.clone(), path }), cx);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The registry name of an agent's mark.
fn agent_icon_name(agent: CodingAgent) -> &'static str {
    match agent {
        CodingAgent::Claude => "claude",
        CodingAgent::Codex => "codex",
    }
}

impl Render for LocalReposPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "LocalReposPane").entered();
        let root = self.root(cx);
        self.ensure_scanned(root.clone(), cx);
        self.ensure_resolver(window, cx);
        let projection = self.project(&root, cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The pane's plugin: the IDE's, the agents' brand marks in their colour
/// (`coding_selects::mark_icon`).
struct PanePlugin {
    inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        (name == "claude").then(|| { use gpui_component::Sizable as _; crate::coding_selects::mark_icon(crate::icons::ExpIcon::Claude).with_size(gpui::px(size)).into_any_element() }).or_else(|| self.inner.icon_element(name, size, color, cx))
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }
}

/// The colours the document needs beyond [`Ink`].
struct Chrome {
    /// The ghost glyphs' plain ink and their disabled ink.
    foreground: String,
    muted: String,
    danger: String,
    warning: String,
    ghost_hover: String,
    skeleton: String,
    skeleton_value: String,
    radius_md: f32,
}

/// A `web_icon_xs` ghost `Button`: the 24 px circle, its `size_3` glyph in
/// `tint`, the ghost hover; `loading` swaps the glyph for the spinner, a
/// disabled one takes no press and draws a plain glyph muted at half (a
/// tinted glyph keeps its own colour, as gpui-component's does).
#[allow(clippy::too_many_arguments)]
fn ghost_xs(chrome: &Chrome, id: &str, icon: &str, tint: &str, tip: Value, event: Value, visible: Value, dim: bool) -> Value {
    let untinted = tint == chrome.foreground;
    let mut button = json!({"id": id, "component": "Box", "visible": visible, "accessibility": {"label": tip.clone()},
        "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": 24, "height": 24, "borderRadius": 9999},
        "children": [{"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": 10.5, "height": 10.5, "color": if dim && untinted { &chrome.muted } else { tint }, "opacity": if dim && untinted { 0.5 } else { 1.0 }}}]});
    if !dim {
        button["props"] = json!({"pressable": true});
        button["on"] = json!({"press": {"event": event}});
        button["style"]["cursor"] = json!("pointer");
        button["style"][":hover"] = json!({"backgroundColor": chrome.ghost_hover});
    }
    json!({"id": format!("{id}_tip"), "component": "Tooltip", "visible": button["visible"].clone(), "props": {"content": tip}, "children": [button]})
}

/// `controls::ghost_icon_button` (the band's): the 32 px square on the
/// medium radius, its `size_3p5` glyph at 70% (dimmed by half when
/// disabled), the ghost hover.
#[allow(clippy::too_many_arguments)]
fn band_ghost(ink: &Ink, chrome: &Chrome, id: &str, icon: &str, tip: &str, event: Value, visible: Value, dim: bool) -> Value {
    let glyph = ink.r(0.875);
    let mut button = json!({"id": id, "component": "Box", "accessibility": {"label": tip},
        "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": 32, "height": 32, "borderRadius": ink.radius_md},
        "children": [{"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": glyph, "height": glyph, "color": if dim { &ink.muted } else { &ink.foreground }, "opacity": if dim { 0.5 } else { VALUE_ALPHA }}}]});
    if !dim {
        button["props"] = json!({"pressable": true});
        button["on"] = json!({"press": {"event": event}});
        button["style"]["cursor"] = json!("pointer");
        button["style"][":hover"] = json!({"backgroundColor": chrome.ghost_hover});
    }
    json!({"id": format!("{id}_tip"), "component": "Tooltip", "visible": visible, "props": {"content": tip}, "children": [button]})
}

/// The band button's spinner (its `loading` look).
fn band_spinner(ink: &Ink, id: &str, visible: Value, disabled: bool) -> Value {
    let glyph = ink.r(0.875);
    let (color, alpha) = if disabled { (&ink.muted, 0.5) } else { (&ink.foreground, VALUE_ALPHA) };
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": 32, "height": 32},
        "children": [{"id": format!("{id}_spin"), "component": "Spinner", "style": {"width": glyph, "height": glyph, "color": color, "opacity": alpha}}]})
}

/// The 24 px ghost slot's spinner (a busy button).
fn ghost_spinner(ink: &Ink, id: &str, visible: Value) -> Value {
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": 24, "height": 24},
        "children": [{"id": format!("{id}_spin"), "component": "Spinner", "style": {"width": 10.5, "height": 10.5, "color": ink.muted, "opacity": 0.5}}]})
}

/// One worktree row (EXP-369/694, the ladder's rhythm: `px_3` / `py_2`,
/// `gap_2`): the branch glyph, `repo branch` over the path (mono, `text_xs`),
/// the terminal menu and the confirmed force-remove.
fn worktree_row(ink: &Ink, chrome: &Chrome) -> Value {
    let mono = |id: &str, text: Value, color: &str| {
        let mut t = ink.text(id, text, XS, color);
        t["style"]["fontFamily"] = json!("$type.family.mono");
        t["props"]["lines"] = json!(1);
        t
    };
    let mut repo = mono("wt_repo", path("repo"), &ink.muted);
    repo["style"]["flexShrink"] = json!(0);
    let mut label = mono("wt_label", path("label"), &ink.foreground);
    label["style"]["minWidth"] = json!(0);
    let head = json!({"id": "wt_head", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "gap": ink.r(0.25), "minWidth": 0, "overflow": "hidden"}, "children": [repo, label]});
    let mut path_text = faded(mono("wt_path", path("path"), &ink.muted), 0.7);
    path_text["style"]["minWidth"] = json!(0);
    let column = json!({"id": "wt_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "minWidth": 0}, "children": [head, path_text]});
    let glyph = json!({"id": "wt_glyph", "component": "Box", "style": {"display": "flex", "flexShrink": 0}, "children": [{"id": "wt_glyph_icon", "component": "Icon", "props": {"name": "ui-branch"}, "style": {"width": ink.r(XS), "height": ink.r(XS), "color": ink.muted}}]});
    let terminal = ghost_xs(chrome, "wt_terminal", "nav-terminal", &ink.foreground, json!("Open a terminal in this worktree"), json!({"name": "menu"}), json!(true), false);
    let terminal = json!({"id": "wt_menu", "component": "Menu", "props": {"items": path("items"), "align": "end"}, "on": {"select": {"event": {"name": "terminal", "context": {"key": path("key")}}}}, "children": [terminal]});
    let event = json!({"name": "removeWorktree", "context": {"key": path("key")}});
    let remove = ghost_xs(chrome, "wt_remove", "ui-delete", &chrome.danger, path("removeTip"), event.clone(), path("removeOn"), false);
    let remove_off = ghost_xs(chrome, "wt_remove_off", "ui-delete", &chrome.danger, path("removeTip"), event.clone(), and(path("removeOff"), path("free")), true);
    let remove_held = ghost_xs(chrome, "wt_remove_held", "ui-delete", &ink.muted, path("removeTip"), event, path("held"), true);
    let actions = json!({"id": "wt_actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "gap": ink.r(0.25), "flexShrink": 0}, "children": [terminal, remove, remove_off, remove_held]});
    json!({"id": "wt_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [glyph, column, actions]})
}

/// One clone row (EXP-1076: the hairline ladder): the folder glyph, the
/// name over its size · worktree count, Prune and Remove local copy; a
/// prune/remove report under it.
fn repo_row(ink: &Ink, chrome: &Chrome) -> Value {
    let mut name = ink.text("repo_name", path("name"), SM, &ink.foreground);
    name["props"]["lines"] = json!(1);
    name["style"]["fontWeight"] = json!(500);
    let bar = json!({"id": "repo_size_bar", "component": "Skeleton", "visible": path("measuring"), "props": {"width": "42", "height": "10.5"}, "style": {"width": 42, "height": 10.5, "borderRadius": chrome.radius_md, "backgroundColor": chrome.skeleton_value}});
    let mut size = ink.text("repo_size", path("size"), XS, &ink.muted);
    size["visible"] = path("sized");
    let storage = json!({"id": "repo_storage", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.25)},
        "children": [{"id": "repo_storage_icon", "component": "Icon", "props": {"name": "settings-storage"}, "style": {"width": ink.r(XS), "height": ink.r(XS), "color": ink.muted}}, size, bar]});
    let meta = json!({"id": "repo_meta", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5)},
        "children": [storage, ink.text("repo_dot", json!("·"), XS, &ink.muted), ink.text("repo_count", path("count"), XS, &ink.muted)]});
    let column = json!({"id": "repo_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.125), "flexGrow": 1, "flexShrink": 1, "minWidth": 0}, "children": [name, meta]});
    let folder = json!({"id": "repo_folder", "component": "Icon", "props": {"name": "ui-folder"}, "style": {"width": ink.r(SM), "height": ink.r(SM), "color": ink.muted, "flexShrink": 0}});
    let tip = json!("Remove worktrees whose work has landed on the default branch (merged PR, squash merges included, or a finished issue) and delete stale session branches. Uncommitted changes to tracked files are never discarded; live sessions are never touched.");
    let prune_event = json!({"name": "prune", "context": {"key": path("key")}});
    let prune = ghost_xs(chrome, "repo_prune", "ui-clean", &ink.foreground, tip.clone(), prune_event.clone(), path("pruneOn"), false);
    let prune_off = ghost_xs(chrome, "repo_prune_off", "ui-clean", &ink.foreground, tip, prune_event, path("pruneOff"), true);
    let pruning = ghost_spinner(ink, "repo_pruning", path("pruning"));
    let remove_event = json!({"name": "removeRepo", "context": {"key": path("key")}});
    let pill = |id: &str, tint: &str, visible: Value| {
        let [on, off] = ink.pill_button_with(id, PillPaint::Glass, Some(json!("ui-delete")), Some(tint), json!("Remove local copy"), path("removeOff"), None, remove_event.clone());
        json!({"id": format!("{id}_tip"), "component": "Tooltip", "visible": visible, "props": {"content": path("removeTip")}, "children": [{"id": format!("{id}_slot"), "component": "Box", "style": {"display": "flex", "flexShrink": 0}, "children": [on, off]}]})
    };
    let actions = json!({"id": "repo_actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.25), "flexShrink": 0},
        "children": [prune, prune_off, pruning, pill("repo_remove", &chrome.danger, path("free")), pill("repo_remove_held", &ink.muted, path("held"))]});
    let row = json!({"id": "repo_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [folder, column, actions]});
    let mut report = ink.text("repo_message", path("message"), XS, &ink.muted);
    report["visible"] = path("hasMessage");
    report["style"]["paddingHorizontal"] = json!(ink.r(0.75));
    report["style"]["paddingBottom"] = json!(ink.r(0.5));
    json!({"id": "repo_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row, report]})
}

/// A flat list of `row` over `/pane/<list>`.
fn ladder(list: &str, row: Value, visible: Value) -> Value {
    json!({"id": format!("{list}_rows"), "component": "List", "visible": visible, "props": {"gap": "none"},
        "template": {"component": format!("{list}_item"), "path": format!("/pane/{list}"), "key": "key"},
        "children": [{"id": format!("{list}_item"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]})
}

/// The pane's tree (`section`: `gap_2`): the Worktrees band (Refresh and
/// the broom), the root, the disk warning, then the skeleton / empty state
/// / the worktree ladder and the Local repositories band over the clones.
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    // EXP-862: Refresh and the broom ride the band's trailing slot as
    // ghost glyphs; `loading` swaps a glyph for the spinner.
    let refresh = band_ghost(ink, chrome, "refresh", "ui-refresh", "Refresh", json!({"name": "refresh"}), path("/pane/idle"), false);
    let refreshing = band_spinner(ink, "refreshing", path("/pane/scanning"), false);
    let prune_all = band_ghost(ink, chrome, "prune_all", "ui-clean", "Prune merged worktrees", json!({"name": "pruneAll"}), path("/pane/pruneAllOn"), false);
    let prune_all_off = band_ghost(ink, chrome, "prune_all_off", "ui-clean", "Prune merged worktrees", json!({"name": "pruneAll"}), path("/pane/pruneAllOff"), true);
    let sweeping = band_spinner(ink, "sweeping", path("/pane/sweeping"), true);
    let trailing = json!({"id": "band_actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.25)}, "children": [refresh, refreshing, prune_all, prune_all_off, sweeping]});
    let mut root_line = faded(ink.text("root_path", path("/pane/root"), XS, &ink.muted), 0.7);
    root_line["props"]["lines"] = json!(1);
    root_line["style"]["fontFamily"] = json!("$type.family.mono");
    let mut disk = ink.text("disk", path("/pane/disk"), XS, &chrome.warning);
    disk["visible"] = path("/pane/diskLow");
    let skeleton = ink.skeleton_rows("skeleton", &chrome.skeleton, path("/pane/loading"));
    let no_repos = empty_state(ink, "no_repos", "settings-local-repos", "No local repositories yet", path("/pane/noRepos"));
    let no_worktrees = empty_state(ink, "no_worktrees", "ui-branch", "No worktrees", and(path("/pane/hasRepos"), path("/pane/noWorktrees")));
    let mut clones_band = ink.band("clones_band", "Local repositories", None);
    clones_band["visible"] = path("/pane/hasRepos");
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            ink.band("band", "Worktrees", Some(trailing)), root_line, disk, skeleton, no_repos, no_worktrees,
            ladder("worktrees", worktree_row(ink, chrome), and(path("/pane/hasRepos"), path("/pane/hasWorktrees"))),
            clones_band,
            ladder("repos", repo_row(ink, chrome), path("/pane/hasRepos")),
        ]})
}

// ---------------------------------------------------------------------------
// Foreground helpers (synced-collection reads)
// ---------------------------------------------------------------------------

/// The branches held by a live session ANYWHERE: every synced session that
/// is queued or running and names its `branch` (EXP-1294: the branch lives on
/// the session row). Includes other devices' sessions; an idle or ended one
/// holds nothing.
fn live_session_branches(cx: &App) -> HashSet<String> {
    Store::global(cx)
        .collections()
        .sessions
        .read(cx)
        .iter()
        .filter(|session| crate::session_data::session_is_working(session))
        .filter_map(|session| session.branch.clone())
        .filter(|branch| !branch.is_empty())
        .collect()
}

/// Whether THIS process is running a session on `branch` — the same
/// [`LocalSessions::session_on_branch`] source of truth, which (unlike the
/// synced rows) covers a launch whose row has not echoed back yet.
fn local_session_on_branch(branch: &str, cx: &App) -> bool {
    LocalSessions::global_ref(cx)
        .is_some_and(|sessions| sessions.read(cx).session_on_branch(branch).is_some())
}

/// Whether a live coding session is bound to one of this clone's worktrees:
/// a live branch whose worktree dir exists under `<clone>.worktrees`. Errs
/// toward "in use" — never delete a clone out from under a live session
/// (§4.7).
fn clone_in_use(clone: &Path, live: &HashSet<String>) -> bool {
    let worktrees = worktrees_dir(clone);
    live.iter()
        .any(|branch| worktrees.join(sanitize_branch_for_path(branch)).exists())
}

/// Whether a live session holds THIS worktree — the per-row mirror of
/// [`clone_in_use`], gating the `--force` remove. A row that knows its branch
/// matches by branch name; a branch-less row (detached, or the git-less
/// directory fallback) matches by the directory a live branch would occupy.
fn worktree_in_use(clone: &Path, worktree: &WorktreeRow, live: &HashSet<String>) -> bool {
    match &worktree.branch {
        Some(branch) => live.contains(branch),
        None => live.iter().any(|branch| {
            worktrees_dir(clone).join(sanitize_branch_for_path(branch)) == worktree.path
        }),
    }
}

// ---------------------------------------------------------------------------
// Background helpers (filesystem + argv git — no gpui, unit-testable)
// ---------------------------------------------------------------------------

/// Phase 1: walk `<repos_root>/<owner>/<name>` two levels deep for trunk clones
/// (a dir with a `.git`, not the `.worktrees` sibling) and list each one's
/// worktrees. Deliberately does NOT size anything — that is
/// [`repo_disk_size`], run per clone by phase 2, so the row list lands in
/// milliseconds instead of waiting on a multi-second `du` walk (EXP-490).
/// Blocking; the caller runs it on the background executor.
fn scan_repos(root: &Path) -> Vec<RepoEntry> {
    let mut out = Vec::new();
    let Ok(owners) = std::fs::read_dir(root) else {
        return out;
    };
    for owner in owners.flatten() {
        if !owner.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let owner_name = owner.file_name().to_string_lossy().into_owned();
        let Ok(names) = std::fs::read_dir(owner.path()) else {
            continue;
        };
        for name in names.flatten() {
            if !name.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }
            let dir_name = name.file_name().to_string_lossy().into_owned();
            if dir_name.ends_with(".worktrees") {
                continue; // the sibling worktrees dir, not a clone
            }
            let path = name.path();
            if !path.join(".git").exists() {
                continue;
            }
            out.push(RepoEntry {
                worktrees: list_repo_worktrees(&path),
                size_bytes: None, // phase 2
                full_name: format!("{owner_name}/{dir_name}"),
                clone_path: path,
            });
        }
    }
    out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    out
}

/// Phase 2's per-clone measurement: the clone tree plus its `.worktrees`
/// sibling. Seconds of `lstat` on a large repo — background executor only.
fn repo_disk_size(clone: &Path) -> u64 {
    dir_size(clone) + dir_size(&worktrees_dir(clone))
}

/// Recursive on-disk size of `path` (regular files only). Iterative (an
/// explicit stack, not recursion) so a deep tree can't overflow the stack;
/// symlinks are not followed (`file_type` returns the link type), so no cycles.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
    }
    total
}

/// A clone's linked worktrees via `git worktree list --porcelain`. Falls back
/// to the `.worktrees` subdirectories (branch-less) when git is unavailable,
/// so the row still lists — and can still remove — what is on disk.
fn list_repo_worktrees(clone: &Path) -> Vec<WorktreeRow> {
    if let Some(entries) = worktree_list(clone) {
        return entries
            .into_iter()
            .map(|(path, branch)| WorktreeRow { path, branch })
            .collect();
    }
    let worktrees = worktrees_dir(clone);
    std::fs::read_dir(&worktrees)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.join(".git").exists())
                .map(|path| WorktreeRow { path, branch: None })
                .collect()
        })
        .unwrap_or_default()
}

/// The clone's LINKED worktrees as `(path, branch)` — the main working tree
/// (always the first `git worktree list` entry) is dropped. `None` when git is
/// missing or the command fails. `branch` is `None` for a detached worktree.
fn worktree_list(clone: &Path) -> Option<Vec<(PathBuf, Option<String>)>> {
    let output = base_git(clone)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<(PathBuf, Option<String>)> = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(previous) = path.take() {
                entries.push((previous, branch.take()));
            }
            path = Some(PathBuf::from(rest));
            branch = None;
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = Some(rest.trim_start_matches("refs/heads/").to_string());
        }
    }
    if let Some(previous) = path.take() {
        entries.push((previous, branch.take()));
    }
    if !entries.is_empty() {
        entries.remove(0); // the main clone working tree
    }
    Some(entries)
}

/// Remove ONE worktree of `clone` (EXP-369, the sub-row's trash action).
/// `--force` because the confirm dialog already warned that uncommitted work
/// goes with it — the prune path stays non-forcing on purpose. The branch is
/// deliberately left behind (an unpushed commit survives on it). Blocking;
/// runs on the background executor.
fn remove_worktree(clone: &Path, worktree: &Path) -> Result<(), String> {
    git_ok(
        clone,
        &["worktree", "remove", "--force", &worktree.to_string_lossy()],
    )
}

/// Delete the clone and its `.worktrees` sibling from disk. Best-effort on the
/// worktrees dir (it may not exist); the clone removal is the one that must
/// succeed. No git here — the whole tree is going away.
fn remove_local_copy(clone: &Path) -> Result<(), String> {
    let worktrees = worktrees_dir(clone);
    if worktrees.exists() {
        std::fs::remove_dir_all(&worktrees)
            .map_err(|err| format!("Couldn't remove worktrees: {err}"))?;
    }
    std::fs::remove_dir_all(clone)
        .map_err(|err| format!("Couldn't remove {}: {err}", clone.display()))?;
    Ok(())
}

/// A local, network-free `git -C <cwd>` command (no token, no credential
/// prompt) — the maintenance ops here never touch a remote.
fn base_git(cwd: &Path) -> Command {
    let mut command = terminal::process::background_command("git");
    command.arg("-C").arg(cwd);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

/// Run a local git command for its success/failure; on failure return the
/// trimmed stderr (else the exit code) as the reported reason.
fn git_ok(cwd: &Path, args: &[&str]) -> Result<(), String> {
    let output = base_git(cwd)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        detail = format!("exit code {}", output.status.code().unwrap_or(-1));
    }
    Err(detail)
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// Human-readable byte size (`1.5 GB`, `812.0 KB`, `0 B`).
fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// The inline result line for a prune (`Removed 2 worktrees. Deleted 14
/// branches. Skipped exp/EXP-3 (uncommitted changes).`).
fn format_prune_result(report: &coding::PruneReport) -> SharedString {
    if report.blocked_by_launch {
        // EXP-478: the pass never ran — a launch held the clone's gate.
        return "A session is being launched. Try again in a moment.".into();
    }
    if report.is_empty() {
        return "No merged worktrees to prune.".into();
    }
    let mut parts = Vec::new();
    let removed = report.removed_worktrees.len();
    if removed > 0 {
        let noun = if removed == 1 { "worktree" } else { "worktrees" };
        parts.push(format!("Removed {removed} {noun}."));
    }
    let deleted = report.deleted_branches.len();
    if deleted > 0 {
        let noun = if deleted == 1 { "branch" } else { "branches" };
        parts.push(format!("Deleted {deleted} {noun}."));
    }
    if !report.skipped.is_empty() {
        let detail = report
            .skipped
            .iter()
            .map(|(branch, reason)| {
                format!("{branch} ({})", reason.describe(report.default_branch.as_deref()))
            })
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Skipped {detail}."));
    }
    parts.join(" ").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// refuses (#EXP-1767's class).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        };
        let chrome = Chrome {
            foreground: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            danger: "#f87171ff".into(),
            warning: "#fbbf24ff".into(),
            ghost_hover: "#ffffff0f".into(),
            skeleton: "#a1a1aa2e".into(),
            skeleton_value: "#27272aff".into(),
            radius_md: 8.,
        };
        let tree = document(&ink, &chrome);
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-local-repos", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn format_size_scales_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1_572_864), "1.5 MB");
        assert_eq!(format_size(1_610_612_736), "1.5 GB");
    }

    /// EXP-1020: either bound trips the warning — a small disk runs out of
    /// FRACTION first, a large one out of absolute headroom.
    #[test]
    fn the_disk_warning_trips_on_the_fraction_or_the_headroom() {
        const GB: u64 = 1024 * 1024 * 1024;
        // Plenty on both counts.
        assert!(!disk_is_low(200 * GB, 1000 * GB));
        // A big disk with little left: the absolute bound.
        assert!(disk_is_low(5 * GB, 4000 * GB));
        // A small disk with a healthy-looking absolute number: the fraction.
        assert!(disk_is_low(20 * GB, 1000 * GB));
        // An unknown filesystem never warns.
        assert!(!disk_is_low(0, 0));
    }

    #[test]
    fn prune_result_renders_removed_deleted_and_skipped() {
        use coding::{PruneReport, SkipReason};
        assert_eq!(
            format_prune_result(&PruneReport::default()),
            SharedString::from("No merged worktrees to prune.")
        );
        assert_eq!(
            format_prune_result(&PruneReport {
                removed_worktrees: vec!["exp/EXP-1".into(), "exp/EXP-2".into()],
                deleted_branches: vec![
                    "exp/EXP-1".into(),
                    "exp/EXP-2".into(),
                    "exp/OLD-9".into()
                ],
                skipped: vec![],
                default_branch: Some("master".into()),
                ..Default::default()
            }),
            SharedString::from("Removed 2 worktrees. Deleted 3 branches.")
        );
        assert_eq!(
            format_prune_result(&PruneReport {
                removed_worktrees: vec!["exp/EXP-1".into()],
                deleted_branches: vec!["exp/EXP-1".into()],
                skipped: vec![
                    ("exp/EXP-3".to_string(), SkipReason::TrackedChanges),
                    ("exp/EXP-4".to_string(), SkipReason::NotLanded),
                ],
                default_branch: Some("master".into()),
                ..Default::default()
            }),
            SharedString::from(
                "Removed 1 worktree. Deleted 1 branch. Skipped exp/EXP-3 \
                 (uncommitted changes), exp/EXP-4 (not merged into master)."
            )
        );
    }

    /// EXP-478: a blocked pass reports as such — never as "nothing to prune".
    #[test]
    fn prune_result_reports_a_launch_blocked_pass() {
        use coding::PruneReport;
        assert_eq!(
            format_prune_result(&PruneReport {
                blocked_by_launch: true,
                ..Default::default()
            }),
            SharedString::from("A session is being launched. Try again in a moment.")
        );
    }

    // ---- real-git integration (hermetic: local file:// remote, no network) ----

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-local-repos-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A clone with two branch worktrees (`exp/EXP-1`, `exp/EXP-2`) laid out at
    /// the §7.1 paths, plus a scannable `<owner>/<name>` root.
    fn seed_clone(dir: &Path) -> (PathBuf, PathBuf) {
        let origin = dir.join("origin-src");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        std::fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);

        let root = dir.join("repos");
        let clone = root.join("acme").join("web");
        std::fs::create_dir_all(clone.parent().unwrap()).unwrap();
        git(dir, &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()]);

        let worktrees = worktrees_dir(&clone);
        std::fs::create_dir_all(&worktrees).unwrap();
        for branch in ["exp/EXP-1", "exp/EXP-2"] {
            let path = worktrees.join(sanitize_branch_for_path(branch));
            git(&clone, &["worktree", "add", "-b", branch, path.to_str().unwrap(), "HEAD"]);
        }
        (root, clone)
    }

    #[test]
    fn scan_finds_the_clone_and_lists_its_worktrees() {
        let dir = temp_dir("scan");
        let (root, _clone) = seed_clone(&dir.0);
        let entries = scan_repos(&root);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].full_name, "acme/web");
        // EXP-490: phase 1 never sizes anything — the rows must not wait on
        // the `du` walk. Phase 2 measures the same tree separately.
        assert!(entries[0].size_bytes.is_none());
        assert!(repo_disk_size(&entries[0].clone_path) > 0);
        // EXP-369: the scan carries the worktrees themselves (branch + path),
        // not just how many there are — the row expands into them.
        let branches: Vec<Option<String>> = entries[0]
            .worktrees
            .iter()
            .map(|worktree| worktree.branch.clone())
            .collect();
        assert_eq!(
            branches,
            vec![
                Some("exp/EXP-1".to_string()),
                Some("exp/EXP-2".to_string())
            ]
        );
        assert!(entries[0].worktrees.iter().all(|w| w.path.exists()));
        assert_eq!(entries[0].worktrees[0].label(), "exp/EXP-1");
    }

    /// Phase 2 measures the clone AND its `.worktrees` sibling — the pane's
    /// number is what "Remove local copy" would reclaim, not just the trunk.
    #[test]
    fn repo_disk_size_counts_the_worktrees_sibling() {
        let dir = temp_dir("size");
        let (_root, clone) = seed_clone(&dir.0);
        assert!(repo_disk_size(&clone) > dir_size(&clone));
    }

    /// A branch-less (detached / git-missing fallback) row falls back to its
    /// directory name so the sub-row is never blank.
    #[test]
    fn worktree_label_falls_back_to_the_directory_name() {
        let row = WorktreeRow {
            path: PathBuf::from("/repos/acme/web.worktrees/exp-EXP-9"),
            branch: None,
        };
        assert_eq!(row.label(), "exp-EXP-9");
    }

    /// The sub-row's trash action force-removes — including a worktree the
    /// non-forcing prune would (rightly) refuse — and keeps the branch.
    #[test]
    fn remove_worktree_forces_past_uncommitted_changes() {
        let dir = temp_dir("remove-worktree");
        let (_root, clone) = seed_clone(&dir.0);
        let dirty = worktrees_dir(&clone).join(sanitize_branch_for_path("exp/EXP-2"));
        std::fs::write(dirty.join("README.md"), "changed\n").unwrap();

        remove_worktree(&clone, &dirty).unwrap();

        assert!(!dirty.exists());
        assert_eq!(list_repo_worktrees(&clone).len(), 1);
        // The branch survives its worktree.
        assert!(git_ok(&clone, &["rev-parse", "--verify", "--quiet", "refs/heads/exp/EXP-2"]).is_ok());
    }

    /// The live-session gate: a held branch blocks BOTH its own worktree's
    /// force-remove and the clone's "Remove local copy"; an unheld sibling
    /// stays removable.
    #[test]
    fn live_branches_gate_the_matching_worktree_and_its_clone() {
        let dir = temp_dir("in-use");
        let (_root, clone) = seed_clone(&dir.0);
        let worktrees = list_repo_worktrees(&clone);
        let live: HashSet<String> = ["exp/EXP-1".to_string()].into_iter().collect();

        assert!(clone_in_use(&clone, &live));
        assert!(worktree_in_use(&clone, &worktrees[0], &live));
        assert!(!worktree_in_use(&clone, &worktrees[1], &live));

        // No live branch → nothing gated.
        assert!(!clone_in_use(&clone, &HashSet::new()));
        assert!(worktrees
            .iter()
            .all(|worktree| !worktree_in_use(&clone, worktree, &HashSet::new())));
    }

    /// A branch-less row (detached worktree / git-less fallback) is matched by
    /// the directory a live branch would occupy.
    #[test]
    fn branchless_worktree_matches_a_live_branch_by_directory() {
        let clone = PathBuf::from("/repos/acme/web");
        let row = WorktreeRow {
            path: worktrees_dir(&clone).join(sanitize_branch_for_path("exp/EXP-9")),
            branch: None,
        };
        let live: HashSet<String> = ["exp/EXP-9".to_string()].into_iter().collect();
        assert!(worktree_in_use(&clone, &row, &live));
        let other: HashSet<String> = ["exp/EXP-8".to_string()].into_iter().collect();
        assert!(!worktree_in_use(&clone, &row, &other));
    }

    #[test]
    fn remove_local_copy_deletes_clone_and_worktrees() {
        let dir = temp_dir("remove");
        let (root, clone) = seed_clone(&dir.0);
        remove_local_copy(&clone).unwrap();
        assert!(!clone.exists());
        assert!(!worktrees_dir(&clone).exists());
        // The root remains (only the clone tree went away).
        assert!(root.exists());
    }
}
