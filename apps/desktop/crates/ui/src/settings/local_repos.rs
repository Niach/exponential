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
//! - **Worktrees** (EXP-369): the scan carries each clone's linked worktrees,
//!   and the row expands into them. Per worktree: a confirmed force-remove
//!   and a terminal button whose dropdown mirrors the terminal dock's "+"
//!   (installed agents + "New shell"), launched at THAT worktree's path. The
//!   force-remove is **blocked while a coding session holds that worktree's
//!   branch** — the same guard as "Remove local copy", one level down (a
//!   `--force` remove would otherwise yank a running agent's cwd).
//! - **Prune merged worktrees** (EXP-465): [`coding::prune::prune_landed`]
//!   under the [`crate::worktree_prune`] policy — worktrees whose work has
//!   LANDED on the default branch (merged PR, or git-confirmed for finished/
//!   deleted issues and stale batch branches; squash merges detected) go,
//!   along with their branches and any stale landed prefix branches. Tracked
//!   modifications always skip (reported); untracked-only debris does not.
//!   All git ops are `std::process::Command("git")` with explicit argv
//!   (masterplan L5) — no `gh`, no git library, no shell.
//! - **Remove local copy**: delete the clone dir + its `.worktrees` sibling
//!   behind a confirm dialog. **Blocked while a coding session is running** on
//!   one of the clone's worktrees (the Remove button disables with the reason).
//!
//! No auto-GC (§4.7): every deletion is an explicit, confirmed user action.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::FluentBuilder as _, App, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use std::collections::HashMap;
use sync::Store;

use coding::branch_name;
use coding::git_worktree::{sanitize_branch_for_path, worktrees_dir};
use coding::CodingAgent;

use crate::coding_flow::{CodingHub, LocalSessions};
use crate::controls::WebControl as _;
use crate::file_tree::{self, OpenAgentShellHere, OpenTerminalHere};
use crate::native_dialog::{self, AlertSpec};
use crate::repo_resolver::{repo_resolver_for_window, RepoResolver};

use super::{card_title, section};
use crate::icons::registry;

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
    /// `(is_error, text)` — the inline result of the last prune/remove.
    message: Option<(bool, SharedString)>,
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

pub struct LocalReposPane {
    scan: Scan,
    /// The `repos_root` the current `scan` belongs to; a settings change
    /// (Coding pane) re-scans.
    scanned_root: Option<PathBuf>,
    /// Measured disk usage by clone path, `(bytes, when measured)` — survives
    /// re-scans so an auto-invalidation refills the rows instead of flashing
    /// skeletons, and feeds the [`SIZE_TTL`] freshness check.
    sizes: HashMap<PathBuf, (u64, Instant)>,
    /// The scan no longer describes the disk (an action, a Refresh click, or
    /// one of the EXP-490 auto-invalidations) — the next render re-walks.
    stale: bool,
    /// Phase 1 is in flight (the Refresh button's spinner).
    scanning: bool,
    /// Monotonic guard: a stale in-flight scan (either phase) must not clobber
    /// a newer one.
    generation: u64,
    actions: HashMap<String, ActionState>,
    /// Clones (by `full_name`) whose worktree list is unfolded (EXP-369).
    expanded: HashSet<String>,
    /// The window's shared repo resolver, bound on first render (the pane is
    /// constructed without a window): it maps a scanned `owner/name` back to
    /// the team's `repositories` row id, which the agent launches need.
    resolver: Option<Entity<RepoResolver>>,
    _subscriptions: Vec<Subscription>,
}

impl LocalReposPane {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // EXP-369: the worktree rows dispatch the file-tree terminal actions;
        // registration is a process-wide `Once`, so claiming it here keeps the
        // pane working even in a window whose file tree was never built.
        file_tree::ensure_actions_registered(cx);
        // The repos root lives in the coding hub (Coding pane edits it); the
        // running-session gate reads the synced coding_sessions collection.
        let hub = CodingHub::global(cx);
        let collections = Store::global(cx).collections().clone();
        let local_sessions = LocalSessions::global(cx);
        let subscriptions = vec![
            // Repos-root edits only re-render; `ensure_scanned` re-scans off
            // the changed `scanned_root` by itself.
            cx.observe(&hub, |_, _, cx| cx.notify()),
            // The busy gates read the synced rows (incl. OTHER devices'
            // sessions, which never move a local worktree) — notify only.
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
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
        Self {
            scan: Scan::Idle,
            scanned_root: None,
            sizes: HashMap::new(),
            stale: false,
            scanning: false,
            generation: 0,
            actions: HashMap::new(),
            expanded: HashSet::new(),
            resolver: None,
            _subscriptions: subscriptions,
        }
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
                entry.message = Some((false, format_prune_result(&report)));
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
                    Err(message) => {
                        let entry = this.action_mut(&full_name);
                        entry.busy = false;
                        entry.message = Some((true, message.into()));
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
                    Ok(()) => Some((false, "Removed 1 worktree.".into())),
                    Err(detail) => Some((true, detail.into())),
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
        .ok_variant(ButtonVariant::Danger)
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

    fn toggle_expanded(&mut self, full_name: &str, cx: &mut gpui::Context<Self>) {
        if !self.expanded.remove(full_name) {
            self.expanded.insert(full_name.to_string());
        }
        cx.notify();
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
        .ok_variant(ButtonVariant::Danger)
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

    /// One clone row: name, disk usage + the worktree expander, and the two
    /// actions — followed by the worktree sub-rows while expanded.
    /// `worktrees_used` is the per-worktree live-session gate, parallel to
    /// `repo.worktrees`.
    #[allow(clippy::too_many_arguments)]
    fn render_repo_row(
        &self,
        ix: usize,
        repo: &RepoEntry,
        in_use: bool,
        worktrees_used: &[bool],
        repository_id: Option<&String>,
        installed: &[CodingAgent],
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let action = self.actions.get(&repo.full_name);
        let busy = action.map(|a| a.busy).unwrap_or(false);
        let count = repo.worktrees.len();
        let expanded = self.expanded.contains(&repo.full_name);
        let worktrees_label = if count == 1 {
            "1 worktree".to_string()
        } else {
            format!("{count} worktrees")
        };

        let meta = h_flex()
            .gap_2()
            .items_center()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(registry::SETTINGS_STORAGE).xsmall())
                    // Phase 2 still walking this clone (first scan only — a
                    // re-scan carries the previous number forward).
                    .child(match repo.size_bytes {
                        Some(bytes) => div()
                            .child(SharedString::from(format_size(bytes)))
                            .into_any_element(),
                        None => Skeleton::new().h_3().w_12().into_any_element(),
                    }),
            )
            .child(div().child("·"))
            .map(|meta| {
                // Nothing to unfold at zero — the count stays plain text.
                if count == 0 {
                    return meta.child(SharedString::from(worktrees_label));
                }
                let full_name = repo.full_name.clone();
                meta.child(
                    Button::new(("repo-worktrees", ix))
                        .ghost()
                        .web_xs()
                        .icon(if expanded {
                            registry::UI_CHEVRON_DOWN
                        } else {
                            registry::UI_CHEVRON_RIGHT
                        })
                        .label(SharedString::from(worktrees_label))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_expanded(&full_name, cx);
                        })),
                )
            });

        let name_col = v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(repo.full_name.clone())),
            )
            .child(meta);

        let prune = {
            let full_name = repo.full_name.clone();
            let clone = repo.clone_path.clone();
            Button::new(("repo-prune", ix))
                .outline()
                .web_xs()
                .label("Prune merged worktrees")
                .tooltip(
                    "Remove worktrees whose work has landed on the default branch \
                     (merged PR, squash merges included, or a finished issue) and \
                     delete stale session branches. Uncommitted changes to tracked \
                     files are never discarded; live sessions are never touched.",
                )
                .loading(busy)
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.run_prune(full_name.clone(), clone.clone(), window, cx);
                }))
        };

        let remove = {
            let full_name = repo.full_name.clone();
            let clone = repo.clone_path.clone();
            let mut button = Button::new(("repo-remove", ix))
                .ghost()
                .web_xs()
                .icon(Icon::new(registry::UI_DELETE).text_color(if in_use {
                    cx.theme().muted_foreground
                } else {
                    cx.theme().danger
                }))
                .label("Remove local copy")
                .disabled(busy || in_use);
            if in_use {
                button = button
                    .tooltip("A coding session is running on this repository. Stop it first.");
            } else {
                button = button
                    .tooltip("Delete the local clone and its worktrees from disk (confirmed).")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_remove(full_name.clone(), clone.clone(), window, cx);
                    }));
            }
            button
        };

        let mut row = v_flex()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(super::row_stroke(cx))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        Icon::new(registry::UI_FOLDER)
                            .small()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(name_col)
                    .child(h_flex().gap_1().flex_shrink_0().child(prune).child(remove)),
            );

        if let Some((is_error, text)) = action.and_then(|a| a.message.clone()) {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(if is_error {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(text),
            );
        }

        if expanded {
            let mut list = v_flex().gap_0p5().pl_7();
            for (wt_ix, worktree) in repo.worktrees.iter().enumerate() {
                list = list.child(self.render_worktree_row(
                    ix,
                    wt_ix,
                    repo,
                    worktree,
                    repository_id,
                    installed,
                    busy,
                    worktrees_used.get(wt_ix).copied().unwrap_or(false),
                    cx,
                ));
            }
            row = row.child(list);
        }
        row
    }

    /// One worktree sub-row (EXP-369): branch (or directory) over its path,
    /// with the terminal dropdown and the confirmed force-remove on the right.
    #[allow(clippy::too_many_arguments)]
    fn render_worktree_row(
        &self,
        repo_ix: usize,
        wt_ix: usize,
        repo: &RepoEntry,
        worktree: &WorktreeRow,
        repository_id: Option<&String>,
        installed: &[CodingAgent],
        busy: bool,
        in_use: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let path = worktree.path.to_string_lossy().into_owned();
        // Element ids are (repo, worktree)-unique — gpui has no 3-tuple id.
        let terminal_id = SharedString::from(format!("worktree-terminal-{repo_ix}"));
        let remove_id = SharedString::from(format!("worktree-remove-{repo_ix}"));

        let terminal = {
            // Everything the menu dispatches is resolved here: the closure is
            // an `Fn` that rebuilds its action boxes on every open.
            let agents: Vec<CodingAgent> = installed.to_vec();
            let repository_id = repository_id.cloned();
            let full_name = repo.full_name.clone();
            let path = path.clone();
            Button::new((terminal_id, wt_ix))
                .ghost()
                .web_icon_xs()
                .icon(registry::NAV_TERMINAL)
                .tooltip("Open a terminal in this worktree")
                .dropdown_menu(move |mut menu, _window, _cx| {
                    for agent in &agents {
                        // No matching team repository ⇒ no repository_id ⇒ no
                        // installation token to mint: the agent items stay
                        // inert (a plain shell always works).
                        let action = OpenAgentShellHere {
                            agent: agent.id().to_string(),
                            repository_id: repository_id.clone().unwrap_or_default(),
                            full_name: full_name.clone(),
                            path: path.clone(),
                        };
                        menu = menu.menu_with_icon_and_disabled(
                            agent.label(),
                            Icon::from(crate::coding_selects::agent_icon(*agent)),
                            Box::new(action),
                            repository_id.is_none(),
                        );
                    }
                    menu.separator().menu_with_icon(
                        "New shell",
                        Icon::new(registry::NAV_TERMINAL),
                        Box::new(OpenTerminalHere { path: path.clone() }),
                    )
                })
        };

        let remove = {
            let full_name = repo.full_name.clone();
            let clone = repo.clone_path.clone();
            let worktree = worktree.clone();
            let mut button = Button::new((remove_id, wt_ix))
                .ghost()
                .web_icon_xs()
                .icon(Icon::new(registry::UI_DELETE).text_color(if in_use {
                    cx.theme().muted_foreground
                } else {
                    cx.theme().danger
                }))
                .disabled(busy || in_use);
            // A `--force` remove would yank a running agent's cwd — same gate
            // as "Remove local copy", one level down.
            if in_use {
                button = button
                    .tooltip("A coding session is running on this worktree. Stop it first.");
            } else {
                button = button
                    .tooltip("Remove this worktree from disk (confirmed)")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_remove_worktree(
                            full_name.clone(),
                            clone.clone(),
                            worktree.clone(),
                            window,
                            cx,
                        );
                    }));
            }
            button
        };

        h_flex()
            .gap_2()
            .items_center()
            .py_1()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(worktree.label()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(path)),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .child(terminal)
                    .child(remove),
            )
    }
}

impl Render for LocalReposPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let root = CodingHub::global(cx).read(cx).settings.repos_root_path();
        self.ensure_scanned(root.clone(), cx);
        self.ensure_resolver(window, cx);
        let hub = CodingHub::global(cx);
        let prefix = hub.read(cx).settings.branch_prefix.clone();
        let installed: Vec<CodingAgent> = hub
            .read(cx)
            .doctor
            .report
            .as_ref()
            .map(|report| report.installed_agents())
            .unwrap_or_default();

        // The count would be a REPO count — misleading under this title.
        let mut body = section(cx).child(card_title("Worktrees"));

        body = body.child(
            div()
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .text_color(cx.theme().muted_foreground.opacity(0.7))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(root.to_string_lossy().into_owned())),
        );

        match &self.scan {
            // Only before the FIRST result (or right after a root change) —
            // a re-scan keeps the rows it already has.
            Scan::Idle => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_10().w_full())
                        .child(Skeleton::new().h_10().w_full()),
                );
            }
            Scan::Ready(repos) if repos.is_empty() => {
                body = body.child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(super::row_stroke(cx))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No repositories cloned locally yet."),
                );
            }
            Scan::Ready(repos) => {
                // Snapshot (clone + in-use gates, repo AND per worktree) so
                // the manager/collection borrows end before the row listeners
                // borrow `cx`.
                let live = live_session_branches(&prefix, cx);
                let rows: Vec<(RepoEntry, bool, Vec<bool>)> = repos
                    .iter()
                    .map(|repo| {
                        let worktrees_used: Vec<bool> = repo
                            .worktrees
                            .iter()
                            .map(|worktree| {
                                worktree_in_use(&repo.clone_path, worktree, &live)
                                    || worktree.branch.as_deref().is_some_and(|branch| {
                                        local_session_on_branch(branch, cx)
                                    })
                            })
                            .collect();
                        // A held worktree holds its clone too, even when the
                        // branch→dir mapping didn't resolve (batch/action runs).
                        let in_use = clone_in_use(&repo.clone_path, &live)
                            || worktrees_used.iter().any(|used| *used);
                        (repo.clone(), in_use, worktrees_used)
                    })
                    .collect();
                let repository_ids = self.repository_ids(cx);
                let mut list = v_flex().gap_2();
                for (ix, (repo, in_use, worktrees_used)) in rows.iter().enumerate() {
                    list = list.child(self.render_repo_row(
                        ix,
                        repo,
                        *in_use,
                        worktrees_used,
                        repository_ids.get(&repo.full_name),
                        &installed,
                        cx,
                    ));
                }
                body = body.child(list);
            }
        }

        body = body.child(
            h_flex().gap_2().child(
                Button::new("local-repos-refresh")
                    .ghost()
                    .web_xs()
                    .label("Refresh")
                    .loading(self.scanning)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            ),
        );

        v_flex().child(body)
    }
}

// ---------------------------------------------------------------------------
// Foreground helpers (synced-collection reads)
// ---------------------------------------------------------------------------

/// The branches held by a live coding session ANYWHERE: a synced
/// `running`/`in_review` session mapped through its issue to
/// `<prefix><IDENTIFIER>`. Includes other devices' sessions; batch
/// and action runs have no issue row to map through and are covered by
/// [`local_session_on_branch`] instead.
fn live_session_branches(prefix: &str, cx: &App) -> HashSet<String> {
    let collections = Store::global(cx).collections();
    let issues = collections.issues.read(cx);
    collections
        .coding_sessions
        .read(cx)
        .iter()
        .filter(|session| {
            matches!(
                session.status.as_deref(),
                Some(domain::contract::CODING_SESSION_STATUS_RUNNING)
                    | Some(domain::contract::CODING_SESSION_STATUS_IN_REVIEW)
            )
        })
        .filter_map(|session| session.issue_id.as_deref())
        .filter_map(|issue_id| issues.get(issue_id))
        .map(|issue| branch_name(prefix, &issue.identifier))
        .collect()
}

/// Whether THIS process is running a session on `branch` — the same
/// [`LocalSessions::session_on_branch`] source of truth the fix-conflicts
/// launch uses, which (unlike the synced rows) also covers batch and action
/// runs.
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
        return "A coding session is being launched — try again in a moment.".into();
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

    #[test]
    fn format_size_scales_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1_572_864), "1.5 MB");
        assert_eq!(format_size(1_610_612_736), "1.5 GB");
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
            SharedString::from("A coding session is being launched — try again in a moment.")
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
