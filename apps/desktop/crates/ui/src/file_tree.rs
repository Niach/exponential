//! File tree (masterplan v4 §4.5) — the left-dock "Files" rail showing the
//! trunk working directory as a gpui-component `tree`: lazy directories,
//! `.git` hidden, gitignored entries dimmed, git status dots (M/A/?) on
//! changed files, context-menu "Reveal in file manager" / "Open terminal
//! here". Clicking a file opens [`crate::navigation::Screen::FileViewer`].
//!
//! Scope: the tree follows the window's active board (its trunk clone,
//! `<repos_root>/<owner>/<name>` — v4 §4.2 "trunk is *the* IDE surface"). The
//! repo→trunk-root resolution needs a tRPC-only `repositories.list` lookup
//! (never synced), so it runs off the foreground like the run bar / `+` shell
//! tab; the resolved root is published into a per-window registry the
//! [`crate::file_viewer`] reads (both live in the same window).
//!
//! EXP-635: the trunk is the DEFAULT root, not the only one — a switcher on
//! top of the tree lists the clone's linked worktrees (the coding sessions'
//! `exp/<IDENTIFIER>` checkouts) and re-roots the whole surface at the picked
//! one, so a branch's files are browsable without leaving the IDE. The
//! registry always holds whatever root the tree is showing, so the viewer
//! resolves its relative paths against the same tree. Git reads are
//! `std::process::Command("git")` argv only (DNR L5) — status + ignored are a
//! one-shot snapshot per load (the git bar / Source Control screen own the
//! live trunk state, v4 §4.3/§4.4); [`FileTreeView::refresh`] re-reads them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Once;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, Action, App, AppContext as _, ClickEvent, Entity, FontWeight, Global, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    list::ListItem,
    menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    tree::{tree, TreeEntry, TreeEvent, TreeItem, TreeState},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use serde::Deserialize;
use sync::Store;

use coding::scm::{self, FileStatus};

use crate::coding_flow::CodingHub;
use crate::navigation::{self, Navigation};
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::icons::registry;

/// Files >2 MB (and binary files) never load in the read-only viewer; the tree
/// carries the same 2 MB constant so the viewer placeholder threshold is one
/// source of truth (v4 §4.5).
pub(crate) const MAX_VIEWER_BYTES: u64 = 2 * 1024 * 1024;

/// Suffix marking a directory's lazy placeholder child — a dir starts with one
/// synthetic child so the gpui-component tree treats it as a folder (its
/// `is_folder()` == "has children") *before* its real children are read; the
/// `Expanded` event then swaps in the real listing. Never a real path.
const LOADING_SUFFIX: &str = "\u{1}__loading";

// ---------------------------------------------------------------------------
// Per-window file-root registry (shared with the file viewer)
// ---------------------------------------------------------------------------

/// Window → the file tree's ACTIVE root: the board's trunk clone, or (EXP-635)
/// the linked worktree the switcher picked. The file tree writes it on every
/// load; [`crate::file_viewer`] reads it to turn a root-relative
/// `Screen::FileViewer { path }` into an absolute path (a file is only
/// reachable from the tree, so the root is always resolved first).
#[derive(Default)]
struct FileRootRegistry {
    by_window: HashMap<WindowId, PathBuf>,
}

impl Global for FileRootRegistry {}

/// The root the file tree resolved for `window_id`, if any.
pub(crate) fn window_file_root(window_id: WindowId, cx: &App) -> Option<PathBuf> {
    cx.try_global::<FileRootRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
}

/// Publish a resolved root for `window_id` from OUTSIDE the file tree.
///
/// The file viewer turns a root-relative `Screen::FileViewer { path }` into an
/// absolute path via [`window_file_root`], which the file tree normally
/// populates on load. The ⌘K search (`crate::search_sheet`) can open a file
/// result before the Files rail was ever shown (so the tree never rendered and
/// never published), so it resolves the trunk root itself and publishes it here
/// before navigating. Idempotent — a later file-tree load overwrites it with
/// the same value.
pub(crate) fn publish_file_root(window_id: WindowId, root: PathBuf, cx: &mut App) {
    set_window_file_root(window_id, root, cx);
}

fn set_window_file_root(window_id: WindowId, root: PathBuf, cx: &mut App) {
    cx.default_global::<FileRootRegistry>()
        .by_window
        .insert(window_id, root);
}

/// Re-root the window's file tree at the TRUNK clone (EXP-635). The ⌘K search
/// greps the trunk, so opening one of its hits must put the tree back on the
/// trunk — otherwise the published root (the trunk, from
/// [`publish_file_root`]) and the visible tree (a worktree) would disagree.
/// No-op when the tree is already on the trunk.
pub(crate) fn select_trunk_root(window: &mut Window, cx: &mut App) {
    let tree = crate::sidebar::window_file_tree(window, cx);
    tree.update(cx, |tree, cx| tree.select_worktree(None, cx));
}

// ---------------------------------------------------------------------------
// Context-menu actions (§4.5) — global handlers registered once per process
// ---------------------------------------------------------------------------

/// Context-menu "Reveal in file manager": `path` is absolute (the tree builds
/// it from the trunk root + the entry's relative path).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct RevealInFileManager {
    pub path: String,
}

/// Context-menu "Open terminal here" (and the viewer's oversized/binary
/// placeholder button): open a `+` shell tab in the bottom dock at `path` (an
/// absolute directory — for a file it is the containing directory).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct OpenTerminalHere {
    pub path: String,
}

/// [`OpenTerminalHere`]'s agent sibling (EXP-369): open a promptless agent
/// session at `path` instead of a plain shell — the settings pane's
/// per-worktree terminal dropdown dispatches it. The repo ids ride the action
/// because the agent's JIT installation token mints off `repository_id`;
/// `agent` is a [`coding::CodingAgent`] id (`claude`/`codex`/`pi`).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct OpenAgentShellHere {
    pub agent: String,
    pub repository_id: String,
    pub full_name: String,
    pub path: String,
}

static REGISTER_ACTIONS: Once = Once::new();

/// Register the context-menu action handlers exactly once. Menus render in
/// the `Root` overlay, so their dispatched actions reach only App-global
/// handlers (§3.6 / the navigation.rs global-listener rule) — never an
/// element-tree `.on_action`. Idempotent across windows via [`Once`], so any
/// surface that dispatches one of these (the tree, the viewer, the settings
/// Local-repositories pane) may call it.
pub(crate) fn ensure_actions_registered(cx: &mut App) {
    REGISTER_ACTIONS.call_once(|| {
        cx.on_action(|action: &RevealInFileManager, cx| {
            // gpui's reveal selects the file where the OS supports it (macOS
            // Finder `-R`, Windows `/select`, Linux via the XDG portal with an
            // open-the-parent fallback) — best-effort, detached internally.
            cx.reveal_path(Path::new(&action.path));
        });
        cx.on_action(|action: &OpenTerminalHere, cx| {
            let dir = PathBuf::from(&action.path);
            navigation::on_active_window(cx, move |window, cx| {
                let Some(manager) = crate::coding_flow::window_terminal_manager(window, cx) else {
                    return;
                };
                let shell_override = crate::coding_flow::terminal_shell_override(cx);
                manager.update(cx, |manager, cx| {
                    if let Err(err) = manager.open_shell(Some(dir), shell_override, cx) {
                        log::error!("[ui] file tree: open terminal here failed: {err:#}");
                    }
                });
            });
        });
        cx.on_action(|action: &OpenAgentShellHere, cx| {
            let Some(agent) = coding::CodingAgent::parse(&action.agent) else {
                log::warn!("[ui] open agent shell: unknown agent {:?}", action.agent);
                return;
            };
            let cwd = PathBuf::from(&action.path);
            let repository_id = action.repository_id.clone();
            let full_name = action.full_name.clone();
            navigation::on_active_window(cx, move |window, cx| {
                let Some(panel) = crate::coding_flow::window_terminal_dock(window, cx) else {
                    return;
                };
                panel.update(cx, |panel, cx| {
                    panel.launch_agent_shell(
                        agent,
                        repository_id,
                        full_name,
                        Some(cwd),
                        window,
                        cx,
                    );
                });
            });
        });
    });
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// Per-path render metadata, shared into the tree's render/context closures.
#[derive(Clone, Copy)]
struct NodeMeta {
    is_dir: bool,
    /// `git status` dot char (`M`/`A`/`D`/`R`/`?`) — files only.
    status: Option<char>,
    /// gitignored (dimmed, v4 §4.5).
    ignored: bool,
}

/// A directory listing entry (Send-safe — built on the background executor,
/// turned into `TreeItem`s on the foreground since `TreeItem` holds `Rc`).
struct DirEntry {
    /// Trunk-relative path (the `TreeItem` id + the `Screen::FileViewer` path).
    rel: String,
    name: String,
    is_dir: bool,
}

/// Background load result: the resolved roots, the root-level listing, and the
/// git status / ignored snapshots (all `Send`).
struct TreeLoad {
    /// The board's trunk clone — the switcher's anchor, and the root every
    /// worktree of `worktrees` is registered against.
    clone_root: PathBuf,
    /// The root the tree actually shows: `clone_root`, or the picked worktree
    /// (EXP-635). The resolution happens on the background thread because it
    /// needs the fresh `git worktree list`.
    root: PathBuf,
    /// The clone's registered worktrees, MAIN WORKING TREE FIRST (git's own
    /// ordering) — empty when the clone is missing or git failed.
    worktrees: Vec<coding::git_worktree::WorktreeEntry>,
    entries: Vec<DirEntry>,
    status: HashMap<String, char>,
    ignored: Vec<String>,
}

enum Load {
    Idle,
    Loading,
    Ready,
}

pub struct FileTreeView {
    nav: Entity<Navigation>,
    /// The shared per-window repo resolver (§4.2) — the trunk clone root comes
    /// from here instead of a per-tree `repositories.list` call.
    repo_resolver: Entity<RepoResolver>,
    window_id: WindowId,
    /// The board scope the tree is showing (sticky — a non-board screen,
    /// e.g. the file viewer itself, keeps the last board).
    board_id: Option<String>,
    load: Load,
    /// Stale-fetch guard (scope changes bump it).
    generation: u64,
    /// The root the tree is showing (absolute) — the trunk clone, or the
    /// worktree [`Self::selected_worktree`] picked.
    active_root: Option<PathBuf>,
    /// The board's trunk clone root (absolute) — the switcher's anchor, kept
    /// even while a worktree is the active root.
    clone_root: Option<PathBuf>,
    /// The clone's registered worktrees, MAIN WORKING TREE FIRST (EXP-635 —
    /// the switcher's menu, re-read on every load and every `refresh`).
    worktrees: Vec<coding::git_worktree::WorktreeEntry>,
    /// The worktree the switcher picked; `None` = the trunk. Held by PATH
    /// (git's own absolute form) so a re-listing can tell whether it is still
    /// registered — a removed worktree falls the tree back to the trunk.
    selected_worktree: Option<PathBuf>,
    tree_state: Entity<TreeState>,
    /// The persistent tree model (source of truth for rebuilds — expand state
    /// lives in each `TreeItem`'s shared `Rc`, so mutating children in place
    /// and re-`set_items` preserves it).
    roots: Vec<TreeItem>,
    /// Directories whose children have been read (lazy-load guard).
    loaded_dirs: HashSet<String>,
    /// Path → dot char (from `git status`).
    status: HashMap<String, char>,
    /// gitignored trunk-relative paths (from `git ls-files -oi`).
    ignored: Vec<String>,
    /// Shared render metadata (`Rc` captured by the tree closures per render).
    meta: Rc<RefCell<HashMap<SharedString, NodeMeta>>>,
    _subscriptions: Vec<Subscription>,
}

impl FileTreeView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        ensure_actions_registered(cx);
        let nav = navigation::nav_for_window(window, cx);
        let repo_resolver = repo_resolver_for_window(window, cx);
        let tree_state = cx.new(|cx| TreeState::new(cx));
        let collections = Store::global(cx).collections().clone();
        let mut subscriptions = vec![
            // Scope follows navigation (board / issue-detail → board).
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // The issue→board join reads the synced collections; re-render
            // when they land.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // Re-render when the shared repo resolution lands / changes.
            cx.observe(&repo_resolver, |_, _, cx| cx.notify()),
            // Lazy directory loading rides the tree's expand events.
            cx.subscribe(&tree_state, |this, _state, event: &TreeEvent, cx| {
                if let TreeEvent::Expanded(id) = event {
                    this.on_expand(id.to_string(), cx);
                }
            }),
        ];
        subscriptions.shrink_to_fit();

        Self {
            nav,
            repo_resolver,
            window_id: window.window_handle().window_id(),
            board_id: None,
            load: Load::Idle,
            generation: 0,
            active_root: None,
            clone_root: None,
            worktrees: Vec::new(),
            selected_worktree: None,
            tree_state,
            roots: Vec::new(),
            loaded_dirs: HashSet::new(),
            status: HashMap::new(),
            ignored: Vec::new(),
            meta: Rc::new(RefCell::new(HashMap::new())),
            _subscriptions: subscriptions,
        }
    }

    /// Re-read the git status / ignored snapshots for the active root without
    /// rebuilding the tree (the rail toggle calls this so dots refresh when the
    /// Files pane is shown). Structure + expand state are untouched. EXP-635:
    /// the worktree list is re-read with them, so a session started since the
    /// last load shows up in the switcher — and a worktree removed underneath
    /// the selection falls the tree back to the trunk (a full reload).
    pub fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(root) = self.active_root.clone() else {
            return;
        };
        let clone_root = self.clone_root.clone();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async move {
                    (
                        read_status(&root),
                        list_ignored(&root),
                        clone_root.map(|clone| list_worktrees(&clone)).unwrap_or_default(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let (status, ignored, worktrees) = snapshot;
                this.worktrees = worktrees;
                if let Some(selected) = this.selected_worktree.clone() {
                    if !is_linked_worktree(&this.worktrees, &selected) {
                        // Gone (`git worktree remove`, EXP-481 device
                        // command, …) — the trunk is the only safe root.
                        this.select_worktree(None, cx);
                        return;
                    }
                }
                this.status = status;
                this.ignored = ignored;
                this.recompute_meta();
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-root the tree at one of the clone's worktrees (`None` = the trunk).
    /// Drops the loaded structure and kicks a fresh load — the old tree's
    /// paths belong to the old root.
    pub(crate) fn select_worktree(
        &mut self,
        worktree: Option<PathBuf>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.selected_worktree == worktree {
            return;
        }
        self.selected_worktree = worktree;
        self.load = Load::Idle;
        self.reset_tree(cx);
        cx.notify();
    }

    /// The window's active board (screen scope with the last-board
    /// fallback) — populated on every screen so the Files tool window never
    /// empties on navigation.
    fn scope_board_id(&self, cx: &App) -> Option<String> {
        navigation::active_board_id(&self.nav, cx)
    }

    /// Render-time load gate: a new board scope resets and kicks one
    /// background resolve (repo → trunk root) + snapshot (status/ignored) +
    /// root listing.
    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        // Drive the shared window resolver (idempotent — one fetch per
        // team, shared by all five trunk/IDE surfaces).
        self.repo_resolver
            .update(cx, |resolver, cx| resolver.ensure_loaded(cx));

        if let Some(scope) = self.scope_board_id(cx) {
            if self.board_id.as_deref() != Some(scope.as_str()) {
                self.board_id = Some(scope);
                self.load = Load::Idle;
                // A new board means a new clone: the switcher's options and
                // its pick belong to the OLD one (unlike a plain re-root,
                // which keeps them painted while the next root loads).
                self.clone_root = None;
                self.worktrees.clear();
                self.selected_worktree = None;
                self.reset_tree(cx);
            }
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(board_id) = self.board_id.clone() else {
            return;
        };
        // The trunk clone root comes from the shared resolver.
        let full_name = match self.repo_resolver.read(cx).lookup_board(&board_id) {
            RepoLookup::Loading => return, // the resolver observer re-renders us
            RepoLookup::Found(repo) => repo.full_name,
            RepoLookup::NotFound | RepoLookup::Error(_) => {
                // No repo linked (or resolution failed) — nothing to show.
                self.load = Load::Ready;
                cx.notify();
                return;
            }
        };
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        let selected = self.selected_worktree.clone();

        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let clone_root =
                        coding::clone_path(&settings.repos_root_path(), &full_name);
                    if !clone_root.join(".git").is_dir() {
                        // Not cloned yet — the git bar's clone job owns that;
                        // the tree shows an empty root until it lands.
                        return TreeLoad {
                            root: clone_root.clone(),
                            clone_root,
                            worktrees: Vec::new(),
                            entries: Vec::new(),
                            status: HashMap::new(),
                            ignored: Vec::new(),
                        };
                    }
                    // EXP-635: the switcher's menu, and with it the check
                    // that a remembered worktree is still registered — a
                    // stale pick silently falls back to the trunk.
                    let worktrees = list_worktrees(&clone_root);
                    let root = selected
                        .filter(|path| is_linked_worktree(&worktrees, path))
                        .unwrap_or_else(|| clone_root.clone());
                    let entries = list_dir(&root, "");
                    TreeLoad {
                        status: read_status(&root),
                        ignored: list_ignored(&root),
                        entries,
                        clone_root,
                        worktrees,
                        root,
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer scope
                }
                this.apply_load(loaded, cx);
            });
        })
        .detach();
    }

    /// Install a completed background load: publish the active root, keep the
    /// snapshots, and build the root-level `TreeItem`s.
    fn apply_load(&mut self, load: TreeLoad, cx: &mut gpui::Context<Self>) {
        set_window_file_root(self.window_id, load.root.clone(), cx);
        // A worktree that vanished between the pick and the load resolved
        // back to the trunk on the background thread — mirror that here so
        // the switcher's label agrees with what is on screen.
        if load.root == load.clone_root {
            self.selected_worktree = None;
        }
        self.clone_root = Some(load.clone_root);
        self.worktrees = load.worktrees;
        self.active_root = Some(load.root);
        self.status = load.status;
        self.ignored = load.ignored;
        self.roots = self.build_items(&load.entries);
        self.loaded_dirs.clear();
        self.loaded_dirs.insert(String::new());
        let roots = self.roots.clone();
        self.tree_state
            .update(cx, |state, cx| state.set_items(roots, cx));
        self.load = Load::Ready;
        cx.notify();
    }

    /// A directory was expanded: read its children once and splice them into
    /// the model (the placeholder child is replaced).
    fn on_expand(&mut self, id: String, cx: &mut gpui::Context<Self>) {
        if id.ends_with(LOADING_SUFFIX) || self.loaded_dirs.contains(&id) {
            return;
        }
        let Some(root) = self.active_root.clone() else {
            return;
        };
        let entries = list_dir(&root, &id);
        let children = self.build_items(&entries);
        if let Some(slot) = find_children_mut(&mut self.roots, &id) {
            *slot = children;
        }
        self.loaded_dirs.insert(id);
        let roots = self.roots.clone();
        self.tree_state
            .update(cx, |state, cx| state.set_items(roots, cx));
        cx.notify();
    }

    /// Turn a directory listing into `TreeItem`s (dirs carry a lazy placeholder
    /// child so the tree renders them as folders before expansion) and record
    /// each node's render metadata.
    fn build_items(&self, entries: &[DirEntry]) -> Vec<TreeItem> {
        let mut meta = self.meta.borrow_mut();
        entries
            .iter()
            .map(|entry| {
                let id: SharedString = entry.rel.clone().into();
                let ignored = self.is_ignored(&entry.rel);
                let status = if entry.is_dir {
                    None
                } else {
                    self.status.get(&entry.rel).copied()
                };
                meta.insert(
                    id.clone(),
                    NodeMeta {
                        is_dir: entry.is_dir,
                        status,
                        ignored,
                    },
                );
                let item = TreeItem::new(id, entry.name.clone());
                if entry.is_dir {
                    item.child(
                        TreeItem::new(
                            SharedString::from(format!("{}{LOADING_SUFFIX}", entry.rel)),
                            "…",
                        )
                        .disabled(true),
                    )
                } else {
                    item
                }
            })
            .collect()
    }

    /// Recompute status/ignored flags on the existing metadata after a
    /// [`refresh`](Self::refresh) (structure unchanged).
    fn recompute_meta(&mut self) {
        let paths: Vec<SharedString> = self.meta.borrow().keys().cloned().collect();
        let mut meta = self.meta.borrow_mut();
        for path in paths {
            if let Some(node) = meta.get_mut(&path) {
                node.ignored = self.is_ignored(path.as_ref());
                node.status = if node.is_dir {
                    None
                } else {
                    self.status.get(path.as_ref()).copied()
                };
            }
        }
    }

    /// Whether a trunk-relative path is gitignored (its own entry, or under an
    /// ignored directory — `git ls-files --directory` collapses ignored dirs to
    /// one `dir/` entry).
    fn is_ignored(&self, rel: &str) -> bool {
        self.ignored.iter().any(|entry| {
            let entry = entry.trim_end_matches('/');
            rel == entry || rel.starts_with(&format!("{entry}/"))
        })
    }

    fn reset_tree(&mut self, cx: &mut gpui::Context<Self>) {
        self.active_root = None;
        // `clone_root` / `worktrees` deliberately survive: the switcher keeps
        // its options (and its label) painted while the next root loads.
        self.roots.clear();
        self.loaded_dirs.clear();
        self.status.clear();
        self.ignored.clear();
        self.meta.borrow_mut().clear();
        self.tree_state
            .update(cx, |state, cx| state.set_items(Vec::new(), cx));
    }

    // -- rendering ----------------------------------------------------------

    /// EXP-635: the worktree switcher above the tree — the trunk plus every
    /// linked worktree (the coding sessions' `exp/<IDENTIFIER>` checkouts),
    /// labelled by branch. It doubles as the "which tree am I browsing"
    /// indicator, so it renders whenever git listed anything at all; a
    /// missing/uncloned repo (empty list) keeps the tree headerless.
    fn render_root_switcher(&self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        let trunk = self.worktrees.first()?;
        let trunk_label = root_label(trunk);
        let linked: Vec<(SharedString, PathBuf)> = self
            .worktrees
            .iter()
            .skip(1)
            .map(|entry| (root_label(entry), entry.path.clone()))
            .collect();
        let selected = self.selected_worktree.clone();
        let label = match &selected {
            Some(path) => linked
                .iter()
                .find(|(_, candidate)| candidate == path)
                .map(|(label, _)| label.clone())
                .unwrap_or_else(|| trunk_label.clone()),
            None => trunk_label.clone(),
        };
        let view = cx.entity().downgrade();
        // The pick is one shared closure: it clears the file selection (the
        // open file's path belongs to the OLD root) before re-rooting.
        let pick = move |view: &gpui::WeakEntity<Self>,
                         target: Option<PathBuf>,
                         window: &mut Window,
                         cx: &mut App| {
            let Some(view) = view.upgrade() else {
                return;
            };
            crate::sidebar::select_file(window, cx, None);
            view.update(cx, |tree, cx| tree.select_worktree(target, cx));
        };
        Some(
            h_flex()
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .px_2()
                .pb_1()
                .child(
                    Button::new("file-tree-root")
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(Icon::new(registry::UI_BRANCH))
                        .label(label)
                        .dropdown_caret(true)
                        .tooltip("Browse another worktree")
                        .dropdown_menu(move |mut menu, _window, _cx| {
                            let view = view.clone();
                            menu = menu.item(PopupMenuItem::label("Trunk")).item(
                                PopupMenuItem::new(trunk_label.clone())
                                    .checked(selected.is_none())
                                    .on_click({
                                        let view = view.clone();
                                        move |_, window, cx| pick(&view, None, window, cx)
                                    }),
                            );
                            if !linked.is_empty() {
                                menu = menu
                                    .item(PopupMenuItem::separator())
                                    .item(PopupMenuItem::label("Worktrees"));
                            }
                            for (label, path) in &linked {
                                let view = view.clone();
                                let path = path.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(label.clone())
                                        .checked(selected.as_ref() == Some(&path))
                                        .on_click(move |_, window, cx| {
                                            pick(&view, Some(path.clone()), window, cx)
                                        }),
                                );
                            }
                            menu
                        }),
                )
                .into_any_element(),
        )
    }

    fn render_placeholder(&self, message: &str, cx: &App) -> gpui::AnyElement {
        v_flex()
            .size_full()
            .p_3()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(message.to_string())),
            )
            .into_any_element()
    }
}

impl Render for FileTreeView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let scope_before = self.board_id.clone();
        self.ensure_loaded(cx);
        // EXP-288: a board-scope change invalidates the file selection (the
        // trunk-relative path belongs to the OLD board's clone).
        if self.board_id != scope_before {
            crate::sidebar::select_file(window, cx, None);
        }

        let still_resolving = matches!(self.load, Load::Idle | Load::Loading);
        let body: gpui::AnyElement = if self.board_id.is_none() {
            self.render_placeholder("Open a board to browse its files.", cx)
        } else if self.active_root.is_none() {
            if still_resolving {
                self.render_placeholder("Loading files…", cx)
            } else {
                self.render_placeholder("No repository linked to this board.", cx)
            }
        } else if self.roots.is_empty() {
            self.render_placeholder("This repository is not cloned yet.", cx)
        } else {
            let meta = self.meta.clone();
            let trunk = self.active_root.clone().unwrap_or_default();
            let ctx_meta = meta.clone();
            let ctx_trunk = trunk.clone();
            // The active file (the center viewer's file, driven by the
            // rail's selection since EXP-288 — files are not tabs) is
            // highlighted so the rail and viewer stay in sync.
            let active_file: Option<SharedString> =
                crate::sidebar::rail_shared_for_window(window, cx)
                    .read(cx)
                    .selected_file()
                    .map(|path| SharedString::from(path.to_string()));
            tree(&self.tree_state, move |ix, entry, _selected, _window, cx| {
                render_tree_item(&meta, &trunk, active_file.as_deref(), ix, entry, cx)
            })
            .context_menu(move |_ix, entry, menu, _window, _cx| {
                build_context_menu(&ctx_meta, &ctx_trunk, entry, menu)
            })
            .size_full()
            .into_any_element()
        };

        // The switcher is a fixed-height sibling, so the tree gets an
        // explicitly sized flex-column wrapper to resolve its `size_full`
        // against (the sidebar's dock-child rule).
        v_flex()
            .size_full()
            .children(self.render_root_switcher(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(body),
            )
    }
}

// ---------------------------------------------------------------------------
// Row + context-menu builders (free fns — captured by the tree closures)
// ---------------------------------------------------------------------------

/// The clone's registered worktrees, MAIN WORKING TREE FIRST (git's ordering).
/// A missing git / failed listing yields an empty list, which reads as "trunk
/// only" everywhere — the switcher never blocks browsing. BLOCKING (spawns
/// git); background executor only.
fn list_worktrees(clone: &Path) -> Vec<coding::git_worktree::WorktreeEntry> {
    coding::git_worktree::list_worktrees(clone).unwrap_or_default()
}

/// Is `path` one of the clone's LINKED worktrees? The main working tree — git
/// always lists it FIRST, and the switcher calls it the trunk — deliberately
/// never matches: the trunk is the `None` selection, so matching it here would
/// let the tree hold a "worktree" pick that the label calls the trunk.
fn is_linked_worktree(worktrees: &[coding::git_worktree::WorktreeEntry], path: &Path) -> bool {
    worktrees
        .iter()
        .skip(1)
        .any(|worktree| worktree.path == path)
}

/// The display name of one root: its branch, else its directory name (a
/// detached worktree — `git worktree list` reports no branch for one).
fn root_label(entry: &coding::git_worktree::WorktreeEntry) -> SharedString {
    match &entry.branch {
        Some(branch) => SharedString::from(branch.clone()),
        None => SharedString::from(
            entry
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.path.to_string_lossy().into_owned()),
        ),
    }
}

/// One tree row, JetBrains-style: a single compact line — expand
/// chevron (dirs only; files get an equal-width spacer so names align), then
/// the folder/file icon, then the name — with gitignore dimming and a
/// trailing git-status dot. `ListItem` lays its children out in a plain block
/// `div`, so the whole row body must be ONE `h_flex` child (separate children
/// stack vertically). File rows navigate to the read-only viewer on click
/// (directory rows toggle expand, handled by the tree).
fn render_tree_item(
    meta: &Rc<RefCell<HashMap<SharedString, NodeMeta>>>,
    trunk: &Path,
    active_file: Option<&str>,
    ix: usize,
    entry: &TreeEntry,
    cx: &App,
) -> ListItem {
    let theme = cx.theme();
    let id = entry.item().id.clone();

    // The lazy placeholder row: a muted "…" while the real listing loads.
    if id.ends_with(LOADING_SUFFIX) {
        return ListItem::new(ix)
            .pl(indent(entry.depth()))
            .py_0()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("…"),
            );
    }

    let node = meta.borrow().get(&id).copied().unwrap_or(NodeMeta {
        is_dir: entry.is_folder(),
        status: None,
        ignored: false,
    });

    let icon = if node.is_dir {
        if entry.is_expanded() {
            registry::UI_FOLDER_OPEN
        } else {
            registry::UI_FOLDER
        }
    } else {
        registry::UI_FILE
    };

    let label_color = if node.ignored {
        theme.muted_foreground.opacity(0.6)
    } else {
        theme.foreground
    };

    // Dirs get the expand chevron; files an equal-width spacer (alignment).
    let chevron: gpui::AnyElement = if node.is_dir {
        Icon::new(if entry.is_expanded() {
            registry::UI_CHEVRON_DOWN
        } else {
            registry::UI_CHEVRON_RIGHT
        })
        .xsmall()
        .text_color(theme.muted_foreground)
        .into_any_element()
    } else {
        div().w(px(14.)).flex_shrink_0().into_any_element()
    };

    // The open file (viewer's active tab) is highlighted; folders never are.
    let is_active_file = !node.is_dir && active_file == Some(id.as_ref());

    let mut row = ListItem::new(ix)
        .pl(indent(entry.depth()))
        .py_0()
        .when(is_active_file, |row| row.bg(theme.list_active))
        .child(
            h_flex()
                .h(px(24.))
                .items_center()
                .gap_1()
                .overflow_hidden()
                .child(chevron)
                .child(
                    Icon::new(icon)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(theme.muted_foreground),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_sm()
                        .text_color(label_color)
                        .child(entry.item().label.clone()),
                ),
        );

    // Trailing status dot (files only).
    if let Some(code) = node.status {
        let color = status_color(code, cx);
        row = row.suffix(move |_, _| {
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(SharedString::from(code.to_string()))
                .into_any_element()
        });
    }

    // File click → the read-only viewer (trunk-relative path) via the
    // rail's selection channel — no tab (EXP-288); deselecting the active
    // tab shows the viewer (the Files tool's center default). Directories
    // fall through to the tree's own expand toggle.
    if !node.is_dir {
        let path = id.to_string();
        row = row.on_click(move |_: &ClickEvent, window, cx| {
            crate::sidebar::select_file(window, cx, Some(path.clone()));
            navigation::set_screen(window, cx, None);
        });
    }
    let _ = trunk; // trunk is used by the context menu, not the row body
    row
}

/// The right-click menu (§4.5): Reveal in file manager + Open terminal here.
/// Both carry the entry's ABSOLUTE path (dir for a directory, containing dir
/// for a file — "open terminal here" always lands in a directory).
fn build_context_menu(
    meta: &Rc<RefCell<HashMap<SharedString, NodeMeta>>>,
    trunk: &Path,
    entry: &TreeEntry,
    menu: PopupMenu,
) -> PopupMenu {
    let id = entry.item().id.clone();
    if id.ends_with(LOADING_SUFFIX) {
        return menu;
    }
    let is_dir = meta
        .borrow()
        .get(&id)
        .map(|node| node.is_dir)
        .unwrap_or_else(|| entry.is_folder());

    let abs = trunk.join(id.as_ref());
    let abs_str = abs.to_string_lossy().into_owned();
    let terminal_dir = if is_dir {
        abs_str.clone()
    } else {
        abs.parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| trunk.to_string_lossy().into_owned())
    };

    menu.menu_with_icon(
        "Reveal in file manager",
        registry::UI_EXTERNAL_LINK,
        Box::new(RevealInFileManager { path: abs_str }),
    )
    .menu_with_icon(
        "Open terminal here",
        registry::NAV_TERMINAL,
        Box::new(OpenTerminalHere { path: terminal_dir }),
    )
}

/// Depth → left padding. The tree renders a flat list; the indent is the only
/// hierarchy cue besides the folder icons.
fn indent(depth: usize) -> gpui::Pixels {
    px(8.0 + 14.0 * depth as f32)
}

/// `git status` code char → dot color (added/untracked green, modified amber,
/// deleted red, renamed blue).
fn status_color(code: char, cx: &App) -> gpui::Hsla {
    let theme = cx.theme();
    match code {
        'A' | '?' => theme.green,
        'D' => theme.red,
        'R' => theme.blue,
        _ => theme.yellow,
    }
}

fn status_char(status: FileStatus) -> char {
    match status {
        FileStatus::Modified => 'M',
        FileStatus::Added => 'A',
        FileStatus::Deleted => 'D',
        FileStatus::Renamed => 'R',
        FileStatus::Untracked => '?',
    }
}

// ---------------------------------------------------------------------------
// Filesystem + git reads (pure, background-safe)
// ---------------------------------------------------------------------------

/// List one directory level (`root/rel`), skipping `.git`. Directories sort
/// before files, then case-insensitive by name. Errors (unreadable dir) yield
/// an empty listing — the node simply shows no children.
fn list_dir(root: &Path, rel: &str) -> Vec<DirEntry> {
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut entries: Vec<DirEntry> = Vec::new();
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let is_dir = item.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        entries.push(DirEntry {
            rel: child_rel,
            name,
            is_dir,
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// Path → status dot char from `git status --porcelain=v2` (via the shared
/// [`scm`] parser). Both staged + unstaged entries collapse to one dot; a real
/// change char (M/A/D/R) wins over untracked.
fn read_status(root: &Path) -> HashMap<String, char> {
    let mut map: HashMap<String, char> = HashMap::new();
    let Ok(summary) = scm::status(root) else {
        return map;
    };
    for change in summary.changes {
        let code = status_char(change.status);
        map.entry(change.path)
            .and_modify(|existing| {
                if *existing == '?' {
                    *existing = code;
                }
            })
            .or_insert(code);
    }
    map
}

/// Trunk-relative gitignored paths: `git ls-files -o -i --exclude-standard
/// --directory` (ignored dirs collapse to one `dir/` entry). argv git only
/// (DNR L5). Empty on any failure — nothing gets dimmed.
fn list_ignored(root: &Path) -> Vec<String> {
    let output = terminal::process::background_command("git")
        .args([
            "ls-files",
            "-o",
            "-i",
            "--exclude-standard",
            "--directory",
        ])
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Find the mutable children slot of the model node with the given id (used to
/// splice a directory's lazily-loaded children in place).
fn find_children_mut<'a>(items: &'a mut Vec<TreeItem>, id: &str) -> Option<&'a mut Vec<TreeItem>> {
    for item in items.iter_mut() {
        if item.id.as_ref() == id {
            return Some(&mut item.children);
        }
        if let Some(found) = find_children_mut(&mut item.children, id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use coding::git_worktree::WorktreeEntry;

    fn entry(path: &str, branch: Option<&str>) -> WorktreeEntry {
        WorktreeEntry {
            path: PathBuf::from(path),
            branch: branch.map(str::to_string),
        }
    }

    #[test]
    fn root_label_prefers_the_branch_then_the_directory() {
        assert_eq!(
            root_label(&entry("/repos/acme/app", Some("master"))),
            "master"
        );
        // Detached (or git-less): the directory name identifies it.
        assert_eq!(
            root_label(&entry("/repos/acme/.worktrees/EXP-42", None)),
            "EXP-42"
        );
    }

    #[test]
    fn only_linked_worktrees_count_as_a_selection() {
        // `git worktree list` order: the main working tree FIRST.
        let worktrees = vec![
            entry("/repos/acme/app", Some("master")),
            entry("/repos/acme/app.worktrees/EXP-42", Some("exp/EXP-42")),
        ];
        assert!(is_linked_worktree(
            &worktrees,
            Path::new("/repos/acme/app.worktrees/EXP-42")
        ));
        // The trunk is the `None` selection — never a "worktree" pick.
        assert!(!is_linked_worktree(&worktrees, Path::new("/repos/acme/app")));
        // A removed worktree stops matching, which is what falls the tree
        // back to the trunk.
        assert!(!is_linked_worktree(
            &worktrees,
            Path::new("/repos/acme/app.worktrees/EXP-7")
        ));
        assert!(!is_linked_worktree(
            &[],
            Path::new("/repos/acme/app.worktrees/EXP-42")
        ));
    }
}
