//! Settings → Local repositories (masterplan v4 §4.7).
//!
//! A **desktop-local** section (like the §7.7 Coding pane — per install, never
//! synced, not owner-gated): every trunk clone under the coding `repos_root`
//! with its on-disk size, worktree count, and two maintenance actions.
//!
//! - **Disk usage** is a `du`-style recursive scan run on the background
//!   executor (never the gpui foreground) and cached in [`Scan::Ready`] until a
//!   refresh — a clone tree can be large, so the pane paints immediately and
//!   fills the sizes in when the walk finishes.
//! - **Prune merged worktrees**: for each of a clone's linked worktrees whose
//!   issue has `prState = 'merged'`, `git worktree remove` + `git branch -D`.
//!   A worktree with uncommitted changes is **skipped** (never force-removed)
//!   and reported. All git ops are `std::process::Command("git")` with explicit
//!   argv (masterplan L5) — no `gh`, no git library, no shell.
//! - **Remove local copy**: delete the clone dir + its `.worktrees` sibling
//!   behind a confirm dialog. **Blocked while a coding session is running** on
//!   one of the clone's worktrees (the Remove button disables with the reason).
//!
//! No auto-GC (§4.7): every deletion is an explicit, confirmed user action.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use gpui::{
    div, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use std::collections::HashMap;
use sync::Store;

use coding::branch_name;
use coding::git_worktree::{sanitize_branch_for_path, worktrees_dir};

use crate::coding_flow::CodingHub;

use super::{card, card_header, error_notice};

// ---------------------------------------------------------------------------
// Background scan model
// ---------------------------------------------------------------------------

/// One trunk clone found under `repos_root` (`<owner>/<name>`), with its cached
/// disk usage (clone + `.worktrees`) and linked-worktree count.
#[derive(Clone)]
struct RepoEntry {
    full_name: String,
    clone_path: PathBuf,
    size_bytes: u64,
    worktree_count: usize,
}

enum Scan {
    /// Not scanning and no result — the next render kicks the walk.
    Idle,
    Scanning,
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

pub struct LocalReposPane {
    scan: Scan,
    /// The `repos_root` the current `scan` belongs to; a settings change
    /// (Coding pane) re-scans.
    scanned_root: Option<PathBuf>,
    /// Monotonic guard: a stale in-flight scan must not clobber a newer one.
    generation: u64,
    actions: HashMap<String, ActionState>,
    _subscriptions: Vec<Subscription>,
}

impl LocalReposPane {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // The repos root lives in the coding hub (Coding pane edits it); the
        // running-session gate reads the synced coding_sessions collection.
        let hub = CodingHub::global(cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&hub, |_, _, cx| cx.notify()),
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
        ];
        Self {
            scan: Scan::Idle,
            scanned_root: None,
            generation: 0,
            actions: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Kick the background walk when the root changed or the scan was
    /// invalidated (refresh / post-action). Runs at render time so a hidden
    /// pane never scans.
    fn ensure_scanned(&mut self, root: PathBuf, cx: &mut gpui::Context<Self>) {
        let same_root = self.scanned_root.as_ref() == Some(&root);
        if same_root && !matches!(self.scan, Scan::Idle) {
            return;
        }
        self.scan = Scan::Scanning;
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
                    return; // superseded by a newer scan
                }
                this.scan = Scan::Ready(entries);
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.scan = Scan::Idle;
        cx.notify();
    }

    fn action_mut(&mut self, full_name: &str) -> &mut ActionState {
        self.actions.entry(full_name.to_string()).or_default()
    }

    /// Prune the clone's merged worktrees: gather the merged-branch set from the
    /// synced issues (foreground), then remove + delete-branch off the
    /// foreground; a dirty worktree is skipped and reported (§4.7).
    fn run_prune(&mut self, full_name: String, clone: PathBuf, cx: &mut gpui::Context<Self>) {
        if self.action_mut(&full_name).busy {
            return;
        }
        let prefix = CodingHub::global(cx)
            .read(cx)
            .settings
            .branch_prefix
            .clone();
        let merged = collect_merged_branches(&prefix, cx);
        self.action_mut(&full_name).busy = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { prune_merged(&clone, &merged) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let entry = this.action_mut(&full_name);
                entry.busy = false;
                entry.message = Some((false, format_prune_result(&result)));
                // Counts and size moved — re-scan.
                this.scan = Scan::Idle;
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
                        this.scan = Scan::Idle;
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
        // AlertDialog — a plain Dialog never renders the button_props footer
        // (EXP-181, same fix as the board-delete confirm).
        window.open_alert_dialog(cx, move |alert, _, _| {
            let full_name = full_name.clone();
            let clone = clone.clone();
            let pane = pane.clone();
            alert
                .overlay_closable(true)
                .close_button(true)
                .title("Remove local copy")
                .description(SharedString::from(format!(
                    "This deletes the local clone of {full_name} and all its \
                     worktrees from disk. Your work on GitHub is untouched; \
                     the clone re-creates on the next \u{201c}Start coding\u{201d}."
                )))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Remove local copy")
                        .ok_variant(ButtonVariant::Danger)
                        .show_cancel(true)
                        .on_ok({
                            let full_name = full_name.clone();
                            let clone = clone.clone();
                            let pane = pane.clone();
                            move |_, _, cx| {
                                let full_name = full_name.clone();
                                let clone = clone.clone();
                                pane.update(cx, |this, cx| {
                                    this.run_remove(full_name, clone, cx);
                                });
                                true
                            }
                        }),
                )
        });
    }

    /// One clone row: name, disk usage + worktree count, and the two actions.
    fn render_repo_row(
        &self,
        ix: usize,
        repo: &RepoEntry,
        in_use: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let action = self.actions.get(&repo.full_name);
        let busy = action.map(|a| a.busy).unwrap_or(false);
        let worktrees = if repo.worktree_count == 1 {
            "1 worktree".to_string()
        } else {
            format!("{} worktrees", repo.worktree_count)
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
                    .child(Icon::new(IconName::HardDrive).xsmall())
                    .child(SharedString::from(format_size(repo.size_bytes))),
            )
            .child(div().child("·"))
            .child(SharedString::from(worktrees));

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
                .xsmall()
                .label("Prune merged worktrees")
                .tooltip(
                    "Remove worktrees whose PR is merged (git worktree remove + delete \
                     branch). A worktree with uncommitted changes is skipped.",
                )
                .loading(busy)
                .disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_prune(full_name.clone(), clone.clone(), cx);
                }))
        };

        let remove = {
            let full_name = repo.full_name.clone();
            let clone = repo.clone_path.clone();
            let mut button = Button::new(("repo-remove", ix))
                .ghost()
                .xsmall()
                .icon(Icon::new(IconName::Delete).text_color(if in_use {
                    cx.theme().muted_foreground
                } else {
                    cx.theme().danger
                }))
                .label("Remove local copy")
                .disabled(busy || in_use);
            if in_use {
                button = button
                    .tooltip("A coding session is running on this repository — stop it first.");
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
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        Icon::new(IconName::Folder)
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
        row
    }
}

impl Render for LocalReposPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let root = CodingHub::global(cx).read(cx).settings.repos_root_path();
        self.ensure_scanned(root.clone(), cx);
        let prefix = CodingHub::global(cx)
            .read(cx)
            .settings
            .branch_prefix
            .clone();

        let count = match &self.scan {
            Scan::Ready(repos) => repos.len(),
            _ => 0,
        };

        let mut body = card(cx).child(card_header(
            format!("Local repositories · {count}"),
            "Trunk clones under your repos root. Disk usage is scanned in the background; \
             actions are per machine and never synced.",
            cx,
        ));

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
            Scan::Idle | Scan::Scanning => {
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
                        .border_color(cx.theme().border)
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No repositories cloned locally yet."),
                );
            }
            Scan::Ready(repos) => {
                // Snapshot (clone + in-use gate) so the manager/collection
                // borrows end before the row listeners borrow `cx`.
                let rows: Vec<(RepoEntry, bool)> = repos
                    .iter()
                    .map(|repo| (repo.clone(), clone_in_use(&repo.clone_path, &prefix, cx)))
                    .collect();
                let mut list = v_flex().gap_2();
                for (ix, (repo, in_use)) in rows.iter().enumerate() {
                    list = list.child(self.render_repo_row(ix, repo, *in_use, cx));
                }
                body = body.child(list);
            }
        }

        body = body.child(
            h_flex().gap_2().child(
                Button::new("local-repos-refresh")
                    .ghost()
                    .xsmall()
                    .label("Refresh")
                    .loading(matches!(self.scan, Scan::Scanning))
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            ),
        );

        v_flex().child(body)
    }
}

// ---------------------------------------------------------------------------
// Foreground helpers (synced-collection reads)
// ---------------------------------------------------------------------------

/// The set of coding branches whose issue's PR is merged — matched against a
/// clone's worktree branches by [`prune_merged`]. Both the DB `branch` field
/// and the locally-composed `<prefix><IDENTIFIER>` are included so a changed
/// branch prefix still resolves.
fn collect_merged_branches(prefix: &str, cx: &App) -> HashSet<String> {
    let mut set = HashSet::new();
    for issue in Store::global(cx).collections().issues.read(cx).iter() {
        if issue.pr_state.as_deref() != Some(domain::contract::PR_STATE_MERGED) {
            continue;
        }
        set.insert(branch_name(prefix, &issue.identifier));
        if let Some(branch) = &issue.branch {
            set.insert(branch.clone());
        }
    }
    set
}

/// Whether a running coding session is bound to one of this clone's worktrees:
/// a synced `running` session whose issue's worktree dir exists under
/// `<clone>.worktrees`. Errs toward "in use" — never delete a clone out from
/// under a live session (§4.7).
fn clone_in_use(clone: &Path, prefix: &str, cx: &App) -> bool {
    let collections = Store::global(cx).collections();
    let issues = collections.issues.read(cx);
    let worktrees = worktrees_dir(clone);
    collections
        .coding_sessions
        .read(cx)
        .iter()
        .filter(|session| {
            session.status.as_deref() == Some(domain::contract::CODING_SESSION_STATUS_RUNNING)
        })
        .filter_map(|session| session.issue_id.as_deref())
        .filter_map(|issue_id| issues.get(issue_id))
        .any(|issue| {
            let branch = branch_name(prefix, &issue.identifier);
            worktrees.join(sanitize_branch_for_path(&branch)).exists()
        })
}

// ---------------------------------------------------------------------------
// Background helpers (filesystem + argv git — no gpui, unit-testable)
// ---------------------------------------------------------------------------

/// Walk `<repos_root>/<owner>/<name>` two levels deep for trunk clones (a dir
/// with a `.git`, not the `.worktrees` sibling), sized and worktree-counted.
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
            let worktrees = worktrees_dir(&path);
            let size = dir_size(&path) + dir_size(&worktrees);
            out.push(RepoEntry {
                worktree_count: count_worktrees(&path),
                size_bytes: size,
                full_name: format!("{owner_name}/{dir_name}"),
                clone_path: path,
            });
        }
    }
    out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    out
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

/// Linked-worktree count via `git worktree list --porcelain` (total entries
/// minus the main clone). Falls back to counting `.worktrees` subdirectories
/// when git is unavailable.
fn count_worktrees(clone: &Path) -> usize {
    if let Some(entries) = worktree_list(clone) {
        return entries.len();
    }
    let worktrees = worktrees_dir(clone);
    std::fs::read_dir(&worktrees)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().join(".git").exists())
                .count()
        })
        .unwrap_or(0)
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

/// Outcome of a prune: worktrees removed + skipped `(branch, reason)` pairs.
#[derive(Debug, Default, PartialEq, Eq)]
struct PruneResult {
    removed: usize,
    skipped: Vec<(String, String)>,
}

/// Remove every merged-branch worktree of `clone`: `git worktree remove` +
/// `git branch -D`. A worktree with uncommitted changes (or a failing remove)
/// is skipped and reported — never force-removed (§4.7). Blocking; runs on the
/// background executor.
fn prune_merged(clone: &Path, merged_branches: &HashSet<String>) -> PruneResult {
    let mut result = PruneResult::default();
    let Some(worktrees) = worktree_list(clone) else {
        return result;
    };
    for (path, branch) in worktrees {
        let Some(branch) = branch else {
            continue; // detached — no issue mapping
        };
        if !merged_branches.contains(&branch) {
            continue;
        }
        if is_dirty(&path) {
            result
                .skipped
                .push((branch, "uncommitted changes".to_string()));
            continue;
        }
        if let Err(detail) = git_ok(clone, &["worktree", "remove", &path.to_string_lossy()]) {
            result.skipped.push((branch, detail));
            continue;
        }
        // Branch delete is best-effort — the worktree (the disk win) is gone.
        let _ = git_ok(clone, &["branch", "-D", &branch]);
        result.removed += 1;
    }
    result
}

/// Whether a worktree has staged/unstaged/untracked changes
/// (`git status --porcelain` non-empty). A failure is treated as dirty (fail
/// safe — never prune a worktree we couldn't inspect).
fn is_dirty(worktree: &Path) -> bool {
    match base_git(worktree).args(["status", "--porcelain"]).output() {
        Ok(output) if output.status.success() => !output.stdout.iter().all(u8::is_ascii_whitespace),
        _ => true,
    }
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
    let mut command = Command::new("git");
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

/// The inline result line for a prune (`Removed 2. Skipped exp/EXP-3
/// (uncommitted changes).`).
fn format_prune_result(result: &PruneResult) -> SharedString {
    if result.removed == 0 && result.skipped.is_empty() {
        return "No merged worktrees to prune.".into();
    }
    let mut parts = Vec::new();
    if result.removed > 0 {
        let noun = if result.removed == 1 { "worktree" } else { "worktrees" };
        parts.push(format!("Removed {} {noun}.", result.removed));
    }
    if !result.skipped.is_empty() {
        let detail = result
            .skipped
            .iter()
            .map(|(branch, reason)| format!("{branch} ({reason})"))
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
    fn prune_result_renders_removed_and_skipped() {
        assert_eq!(
            format_prune_result(&PruneResult::default()),
            SharedString::from("No merged worktrees to prune.")
        );
        assert_eq!(
            format_prune_result(&PruneResult { removed: 2, skipped: vec![] }),
            SharedString::from("Removed 2 worktrees.")
        );
        assert_eq!(
            format_prune_result(&PruneResult {
                removed: 1,
                skipped: vec![("exp/EXP-3".to_string(), "uncommitted changes".to_string())],
            }),
            SharedString::from("Removed 1 worktree. Skipped exp/EXP-3 (uncommitted changes).")
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
    fn scan_finds_the_clone_and_counts_its_worktrees() {
        let dir = temp_dir("scan");
        let (root, _clone) = seed_clone(&dir.0);
        let entries = scan_repos(&root);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].full_name, "acme/web");
        assert_eq!(entries[0].worktree_count, 2);
        assert!(entries[0].size_bytes > 0);
    }

    #[test]
    fn prune_removes_merged_clean_worktrees_and_skips_dirty() {
        let dir = temp_dir("prune");
        let (_root, clone) = seed_clone(&dir.0);

        // EXP-2 is dirty (uncommitted change); EXP-1 is clean.
        let dirty = worktrees_dir(&clone).join(sanitize_branch_for_path("exp/EXP-2"));
        std::fs::write(dirty.join("README.md"), "changed\n").unwrap();

        let merged: HashSet<String> =
            ["exp/EXP-1".to_string(), "exp/EXP-2".to_string()].into_iter().collect();
        let result = prune_merged(&clone, &merged);

        assert_eq!(result.removed, 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].0, "exp/EXP-2");

        // The clean worktree + its branch are gone; the dirty one survives.
        assert!(!worktrees_dir(&clone)
            .join(sanitize_branch_for_path("exp/EXP-1"))
            .exists());
        assert!(dirty.exists());
        assert!(git_ok(&clone, &["rev-parse", "--verify", "--quiet", "refs/heads/exp/EXP-1"]).is_err());
    }

    #[test]
    fn prune_ignores_unmerged_worktrees() {
        let dir = temp_dir("unmerged");
        let (_root, clone) = seed_clone(&dir.0);
        // Empty merged set → nothing pruned.
        let result = prune_merged(&clone, &HashSet::new());
        assert_eq!(result, PruneResult::default());
        assert_eq!(count_worktrees(&clone), 2);
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
