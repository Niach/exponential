//! The ⌘K quick-open — web parity target
//! `apps/web/src/components/issue-search-sheet.tsx` (masterplan-v3 §4.2:
//! "`IssueSearchSheet` is a `Dialog` (⌘K quick-open by title/identifier)"),
//! **extended (EXP-15) to also search the open repo's local files** — a file
//! finder + a `git grep` full-text pass alongside the issue results.
//!
//! Desktop renders the web component's desktop branch only (§4.9 — no
//! mobile bottom sheet): a centered `Dialog` at ~15% from the top with a
//! borderless search input and the result rows. When a repo/worktree is open
//! the results split into three labelled sections — **Issues**, **Files**
//! (fuzzy filename match), and **In files** (`git grep` matches with a
//! file:line preview); picking a row navigates to the issue detail or opens
//! the file in the read-only viewer, then closes the dialog. With no repo
//! open (or none cloned yet) only the flat Issues list shows, exactly as
//! before.
//!
//! Built on gpui-component's `List` (`ListState` + [`ListDelegate`]): the
//! searchable query input, ↑/↓ selection (across sections), Enter-confirm,
//! Esc-cancel and virtualization all come from the component, so keyboard nav
//! is first-class, not bolted on.
//!
//! [`init`] registers the App-global [`OpenSearch`] handler (the sidebar's
//! Search row dispatches it) and the global ⌘K / Ctrl-K binding.
//!
//! Performance (EXP-15): the UI thread never walks the tree or greps. Filename
//! ranking runs on the background executor over a `git ls-files` snapshot
//! loaded once per dialog; content search spawns `git grep` (argv git only —
//! DNR L5; `gh`/ripgrep are never assumed present) on the background executor
//! behind the same 250 ms debounce the server issue pass uses. Every async
//! result is applied only when its query is still current (stale-drop), and the
//! List drops the previous `perform_search` task on each keystroke (that drop
//! is the debounce cancel). Results are capped (Issues 30, Files 50, In files
//! 100) and the section header says so when truncated.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, IntoElement, KeyBinding, ParentElement,
    SharedString, Styled, Task, Window, WindowId,
};
use gpui_component::{
    h_flex,
    list::{List, ListDelegate, ListItem, ListState},
    v_flex, ActiveTheme as _, Icon, IconName, IndexPath, Sizable as _, WindowExt as _,
};
use sync::{SessionPhase, Store};

use coding::run_launch::run_root;
use domain::options::get_issue_status_config;
use domain::IssueStatus;

use crate::actions::OpenSearch;
use crate::coding_flow::CodingHub;
use crate::icons::option_icon;
use crate::issue_list::parse_hex_color;
use crate::navigation::{
    active_board_id, active_team_id, nav_for_window, navigate, Navigation, Screen,
};
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// Web `.slice(0, 30)` — cap the issue result list.
const MAX_RESULTS: usize = 30;

/// EXP-15: cap the fuzzy filename hits shown under "Files".
const FILE_RESULT_CAP: usize = 50;

/// EXP-15: cap the `git grep` hits shown under "In files".
const CONTENT_RESULT_CAP: usize = 100;

/// EXP-15: never fuzzy-match against more than this many paths (a pathological
/// mono-repo could otherwise list hundreds of thousands of files). Filename
/// ranking is O(files) per keystroke on a background thread, so a hard cap
/// keeps the worst case bounded.
const MAX_FILE_LIST: usize = 50_000;

/// EXP-15: content search only runs for queries of at least this length — a
/// one-character `git grep` matches nearly everything and is never useful.
const CONTENT_MIN_QUERY_LEN: usize = 2;

/// EXP-15: clamp an over-long matched line so one giant minified line can't
/// blow up the row.
const PREVIEW_MAX_CHARS: usize = 200;

/// EXP-3: how long a keystroke must rest before the server full-text pass and
/// the `git grep` content pass fire. The List replaces its search task per
/// keystroke (dropping — i.e. cancelling — the previous one), so the timer
/// doubles as the debouncer.
const SERVER_SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

/// EXP-3: server `issues.search` page size (server default 20, max 50).
const SERVER_SEARCH_LIMIT: u32 = 20;

/// Web `sm:max-w-lg` (32rem).
const DIALOG_WIDTH: f32 = 512.;

/// Every result row is this tall (a two-line row). The List measures ONE item
/// height for the whole virtualized list, so all three sections must share it.
const ROW_HEIGHT: f32 = 44.;

/// Section header height (kept uniform — the List measures section 0's header
/// once and assumes the rest match).
const HEADER_HEIGHT: f32 = 26.;

// ---------------------------------------------------------------------------
// Section indices (the delegate's fixed 3-section layout when a repo is open)
// ---------------------------------------------------------------------------

const SECTION_ISSUES: usize = 0;
const SECTION_FILES: usize = 1;
const SECTION_CONTENT: usize = 2;

/// Register the App-global open handler + the quick-open keybinding. Called
/// once from `ui::init`.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &OpenSearch, cx| {
        crate::navigation::on_active_window(cx, |window, cx| open_search(window, cx));
    });
    // ⌘K (command-palette) and ⌘F (§8.13 quick-find parity) both open the same
    // quick-search. It searches issues plus — when a repo is open — the local
    // files, which is exactly the "find" affordance ⌘F implies, so ⌘F reuses
    // this modal rather than standing up a second one.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-k", OpenSearch, None),
        KeyBinding::new("cmd-f", OpenSearch, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-k", OpenSearch, None),
        KeyBinding::new("ctrl-f", OpenSearch, None),
    ]);
}

/// Open the search dialog on `window` (no-op unless the session is `Synced`
/// with a resolvable team — ⌘K on the login surface must do nothing).
pub fn open_search(window: &mut Window, cx: &mut App) {
    if !matches!(Store::global(cx).session(cx), SessionPhase::Synced { .. }) {
        return;
    }
    // Never stack search over an already-open dialog (⌘K spam / ⌘K while a
    // modal is up).
    if window.has_active_dialog(cx) {
        return;
    }
    let nav = nav_for_window(window, cx);
    let Some(team_id) = active_team_id(&nav, cx) else {
        return;
    };
    let repo_resolver = repo_resolver_for_window(window, cx);
    let window_id = window.window_handle().window_id();

    let list = cx.new(|cx| {
        ListState::new(
            SearchDelegate::new(team_id, window_id, nav.clone(), repo_resolver),
            window,
            cx,
        )
        .searchable(true)
    });

    // Web: top-[15%], max-h-[60vh]. The List auto-grows with results up to
    // max_h (its `Infer` sizing), so the empty prompt stays a small dialog.
    let viewport = window.viewport_size();
    let margin_top = viewport.height * 0.15;
    let max_h = viewport.height * 0.6;

    window.open_dialog(cx, {
        let list = list.clone();
        move |dialog, _window, _cx| {
            dialog
                .close_button(false)
                .w(px(DIALOG_WIDTH))
                .margin_top(margin_top)
                .p_0()
                .child(
                    List::new(&list)
                        .search_placeholder("Search issues and files…")
                        .max_h(max_h),
                )
        }
    });
    // Focus the query input (searchable list → input handle) so typing starts
    // immediately, like the web autoFocus.
    list.update(cx, |list, cx| list.focus(window, cx));
}

/// One resolved issue hit — board name/color denormalized at search time
/// (web `boardMap.get(issue.boardId)`).
struct SearchHit {
    issue_id: String,
    identifier: String,
    title: String,
    status: IssueStatus,
    board_name: Option<String>,
    board_color: Option<String>,
}

/// One `git grep` content hit.
struct ContentHit {
    /// Trunk-relative path (also the `Screen::FileViewer` path).
    path: String,
    line: u32,
    /// The matched line, trimmed + clamped.
    preview: String,
}

/// The local-repo resolution + file-list snapshot for the active board. Kept
/// per dialog session (the delegate is fresh on every open), resolved lazily on
/// the first keystroke so an issues-only session never touches git.
enum RepoState {
    /// Resolution not attempted yet (retried on each keystroke until it settles).
    Unresolved,
    /// The file-list load is in flight (guards against a second spawn).
    Loading,
    /// Resolved + cloned: the trunk root and its `git ls-files` snapshot.
    Ready {
        root: PathBuf,
        files: Arc<Vec<String>>,
    },
    /// No repo backs the active board, or it isn't cloned yet — the local
    /// sections stay hidden and only issues show.
    Unavailable,
}

pub struct SearchDelegate {
    team_id: String,
    window_id: WindowId,
    nav: Entity<Navigation>,
    repo_resolver: Entity<RepoResolver>,
    query: String,

    // Section 0 — issues (Electric substring + debounced server full-text).
    issue_hits: Vec<SearchHit>,

    // Local file search (only when `repo` is `Ready`).
    repo: RepoState,
    file_hits: Vec<String>,
    file_truncated: bool,
    content_hits: Vec<ContentHit>,
    content_truncated: bool,

    selected: Option<IndexPath>,
}

impl SearchDelegate {
    fn new(
        team_id: String,
        window_id: WindowId,
        nav: Entity<Navigation>,
        repo_resolver: Entity<RepoResolver>,
    ) -> Self {
        Self {
            team_id,
            window_id,
            nav,
            repo_resolver,
            query: String::new(),
            issue_hits: Vec::new(),
            repo: RepoState::Unresolved,
            file_hits: Vec::new(),
            file_truncated: false,
            content_hits: Vec::new(),
            content_truncated: false,
            selected: None,
        }
    }

    /// Whether the three labelled sections (Issues / Files / In files) are in
    /// play — only once a cloned repo is resolved for the active board.
    fn local_sections_visible(&self) -> bool {
        matches!(self.repo, RepoState::Ready { .. })
    }

    /// Filter the synced issues by title/identifier (web matches title; §4.2
    /// adds identifier for the desktop quick-open). Hits snapshot at search
    /// time — an Electric echo mid-dialog refreshes on the next keystroke.
    fn search_issues(&mut self, cx: &App) {
        self.issue_hits.clear();
        let query = self.query.to_lowercase();
        if query.is_empty() {
            return;
        }
        let collections = Store::global(cx).collections();
        let boards = collections.boards.read(cx);
        self.issue_hits = collections
            .issues_in_team(&self.team_id, cx)
            .into_iter()
            .filter(|issue| {
                issue.title.to_lowercase().contains(&query)
                    || issue.identifier.to_lowercase().contains(&query)
            })
            .take(MAX_RESULTS)
            .map(|issue| {
                let board = boards.get(&issue.board_id);
                SearchHit {
                    issue_id: issue.id,
                    identifier: issue.identifier,
                    title: issue.title,
                    status: issue.status,
                    board_name: board.map(|p| p.name.clone()),
                    board_color: board.and_then(|p| p.color.clone()),
                }
            })
            .collect();
    }

    /// EXP-3: append server full-text hits (description/comment matches) the
    /// local substring filter missed, deduped by id. The synced row wins for
    /// rendering; a hit not (yet) synced locally renders from the returned
    /// fields — its board denormalization then resolves best-effort.
    fn merge_server_hits(&mut self, server_hits: Vec<api::issues::IssueSearchHit>, cx: &App) {
        let seen: std::collections::HashSet<String> = self
            .issue_hits
            .iter()
            .map(|hit| hit.issue_id.clone())
            .collect();
        let collections = Store::global(cx).collections();
        let issues = collections.issues.read(cx);
        let boards = collections.boards.read(cx);
        for hit in server_hits {
            if self.issue_hits.len() >= MAX_RESULTS {
                break;
            }
            if seen.contains(&hit.id) {
                continue;
            }
            let (identifier, title, status, board_id) = match issues.get(&hit.id) {
                Some(issue) => (
                    issue.identifier.clone(),
                    issue.title.clone(),
                    issue.status,
                    issue.board_id.clone(),
                ),
                None => (hit.identifier, hit.title, hit.status, hit.board_id),
            };
            let board = boards.get(&board_id);
            self.issue_hits.push(SearchHit {
                issue_id: hit.id,
                identifier,
                title,
                status,
                board_name: board.map(|p| p.name.clone()),
                board_color: board.and_then(|p| p.color.clone()),
            });
        }
    }

    /// EXP-15: resolve the active board's trunk clone root (off the shared
    /// per-window resolver, exactly like the file tree) and load its
    /// `git ls-files` snapshot on a background thread. Idempotent + lazy:
    /// called at the top of every `perform_search`, it settles once per dialog
    /// (retrying while the resolver is still fetching), so an issues-only
    /// session never spawns git.
    fn ensure_repo(&mut self, cx: &mut gpui::Context<ListState<Self>>) {
        if !matches!(self.repo, RepoState::Unresolved) {
            return; // already Loading / Ready / Unavailable
        }
        self.repo_resolver
            .update(cx, |resolver, cx| resolver.ensure_loaded(cx));
        let Some(board_id) = active_board_id(&self.nav, cx) else {
            return; // no active board yet — retry next keystroke
        };
        let full_name = match self.repo_resolver.read(cx).lookup_board(&board_id) {
            RepoLookup::Loading => return, // resolver still fetching — retry next keystroke
            RepoLookup::Found(repo) => repo.full_name,
            RepoLookup::NotFound | RepoLookup::Error(_) => {
                self.repo = RepoState::Unavailable;
                return;
            }
        };
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        self.repo = RepoState::Loading;
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let root = run_root(&settings.repos_root_path(), &full_name);
                    if !root.join(".git").is_dir() {
                        return None; // not cloned yet — the git bar owns cloning
                    }
                    let files = list_repo_files(&root);
                    Some((root, files))
                })
                .await;
            let _ = this.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                match loaded {
                    Some((root, files)) => {
                        delegate.repo = RepoState::Ready {
                            root,
                            files: Arc::new(files),
                        };
                        // The user may have typed (and paused) before the load
                        // landed — run both local passes for the current query.
                        delegate.refresh_file_hits(cx);
                        delegate.spawn_content_search(cx);
                    }
                    None => delegate.repo = RepoState::Unavailable,
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Rank the cached file list against the current query on the background
    /// executor and swap in the hits (stale-dropped by query). Filename ranking
    /// is CPU-only + fast, so it runs un-debounced — the "Files" section feels
    /// instant.
    fn refresh_file_hits(&mut self, cx: &mut gpui::Context<ListState<Self>>) {
        let RepoState::Ready { files, .. } = &self.repo else {
            self.file_hits.clear();
            self.file_truncated = false;
            return;
        };
        if self.query.is_empty() {
            self.file_hits.clear();
            self.file_truncated = false;
            return;
        }
        let files = files.clone();
        let query = self.query.clone();
        cx.spawn(async move |this, cx| {
            let rank_query = query.clone();
            let ranked = cx
                .background_executor()
                .spawn(async move { rank_files(&files, &rank_query, FILE_RESULT_CAP) })
                .await;
            let _ = this.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                if delegate.query != query {
                    return; // stale — the query moved on
                }
                delegate.file_hits = ranked.hits;
                delegate.file_truncated = ranked.truncated;
                cx.notify();
            });
        })
        .detach();
    }

    /// Run one debounced `git grep` for the current query on the background
    /// executor and swap in the content hits (stale-dropped by query). Used
    /// from the repo-load callback for the "typed before the repo resolved"
    /// case; steady-state keystrokes drive content search through the
    /// cancellable `perform_search` task instead.
    fn spawn_content_search(&mut self, cx: &mut gpui::Context<ListState<Self>>) {
        let RepoState::Ready { root, .. } = &self.repo else {
            return;
        };
        if self.query.chars().count() < CONTENT_MIN_QUERY_LEN {
            self.content_hits.clear();
            self.content_truncated = false;
            return;
        }
        let root = root.clone();
        let query = self.query.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SERVER_SEARCH_DEBOUNCE).await;
            let (grep_root, grep_query) = (root.clone(), query.clone());
            let (hits, truncated) = cx
                .background_executor()
                .spawn(async move { git_grep(&grep_root, &grep_query, CONTENT_RESULT_CAP) })
                .await;
            let _ = this.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                if delegate.query != query {
                    return;
                }
                delegate.content_hits = hits;
                delegate.content_truncated = truncated;
                cx.notify();
            });
        })
        .detach();
    }

    /// Open a trunk-relative file result in the read-only viewer + close the
    /// dialog. Publishes the resolved trunk root first so the viewer can turn
    /// the relative path absolute even if the Files rail was never opened this
    /// session (which is what normally publishes it).
    fn open_file(&self, path: String, window: &mut Window, cx: &mut App) {
        if let RepoState::Ready { root, .. } = &self.repo {
            crate::file_tree::publish_trunk_root(self.window_id, root.clone(), cx);
        }
        navigate(window, cx, Screen::FileViewer { path });
        window.close_dialog(cx);
    }

    // -- row builders (each returns None for an out-of-range index) -----------

    fn render_issue_row(&self, ix: IndexPath, cx: &App) -> Option<ListItem> {
        let hit = self.issue_hits.get(ix.row)?;
        let status_config = get_issue_status_config(hit.status);
        let board_dot = hit
            .board_color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let subtitle: SharedString = match &hit.board_name {
            Some(name) => format!("{name} · {}", hit.identifier).into(),
            None => hit.identifier.clone().into(),
        };

        Some(
            two_line_row(("issue-hit", ix.row), cx)
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .overflow_hidden()
                        .child(option_icon(status_config, cx).small())
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(line_primary(hit.title.clone()))
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .items_center()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            div()
                                                .size_1p5()
                                                .rounded_full()
                                                .flex_shrink_0()
                                                .bg(board_dot),
                                        )
                                        .child(line_secondary(subtitle)),
                                ),
                        ),
                ),
        )
    }

    fn render_file_row(&self, ix: IndexPath, cx: &App) -> Option<ListItem> {
        let path = self.file_hits.get(ix.row)?;
        let (dir, name) = split_path(path);
        Some(
            two_line_row(("file-hit", ix.row), cx).child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        Icon::new(IconName::File)
                            .small()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(line_primary(SharedString::from(name.to_string())))
                            .child(line_secondary(SharedString::from(dir.to_string()))),
                    ),
            ),
        )
    }

    fn render_content_row(&self, ix: IndexPath, cx: &App) -> Option<ListItem> {
        let hit = self.content_hits.get(ix.row)?;
        let location: SharedString = format!("{}:{}", hit.path, hit.line).into();
        Some(
            two_line_row(("content-hit", ix.row), cx).child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        Icon::new(IconName::File)
                            .small()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(line_primary(SharedString::from(hit.preview.clone())))
                            .child(line_secondary(location)),
                    ),
            ),
        )
    }
}

impl ListDelegate for SearchDelegate {
    type Item = ListItem;

    fn perform_search(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Task<()> {
        self.query = query.trim().to_string();
        // Kick the local-repo resolution/load (idempotent), then the instant
        // local passes (issue substring is sync; filename ranking is a fast
        // background task).
        self.ensure_repo(cx);
        self.search_issues(cx);
        self.refresh_file_hits(cx);

        if self.query.is_empty() {
            self.content_hits.clear();
            self.content_truncated = false;
            return Task::ready(());
        }

        // Slow, debounced passes: the server full-text issue search and the
        // `git grep` content search. Returning the task (not detaching) hands
        // its lifetime to the List, which drops it on the next keystroke — that
        // drop IS the debounce cancel.
        let trpc = crate::queries::trpc_client(cx);
        let team_id = self.team_id.clone();
        let query = self.query.clone();
        let grep_root = match &self.repo {
            RepoState::Ready { root, .. } => Some(root.clone()),
            _ => None,
        };
        cx.spawn_in(window, async move |this, window| {
            window
                .background_executor()
                .timer(SERVER_SEARCH_DEBOUNCE)
                .await;

            // Server issue full-text pass (EXP-3).
            if let Some(trpc) = trpc {
                let (search_team, search_query) = (team_id.clone(), query.clone());
                let result = window
                    .background_executor()
                    .spawn(async move {
                        api::issues::search(
                            &trpc,
                            &search_team,
                            &search_query,
                            SERVER_SEARCH_LIMIT,
                        )
                    })
                    .await;
                match result {
                    Ok(hits) => {
                        let _ = this.update_in(window, |list, _, cx| {
                            let delegate = list.delegate_mut();
                            if delegate.query == query {
                                delegate.merge_server_hits(hits, cx);
                                cx.notify();
                            }
                        });
                    }
                    Err(error) => {
                        // Local-only degradation — the substring hits already render.
                        log::warn!("[ui] search: issues.search failed: {error}");
                    }
                }
            }

            // Content search pass (EXP-15) — argv `git grep` on a background
            // thread, only for a repo that's resolved + a long-enough query.
            if query.chars().count() >= CONTENT_MIN_QUERY_LEN {
                if let Some(root) = grep_root {
                    let (grep_root, grep_query) = (root, query.clone());
                    let (hits, truncated) = window
                        .background_executor()
                        .spawn(async move {
                            git_grep(&grep_root, &grep_query, CONTENT_RESULT_CAP)
                        })
                        .await;
                    let _ = this.update_in(window, |list, _, cx| {
                        let delegate = list.delegate_mut();
                        if delegate.query == query {
                            delegate.content_hits = hits;
                            delegate.content_truncated = truncated;
                            cx.notify();
                        }
                    });
                }
            }
        })
    }

    fn sections_count(&self, _cx: &App) -> usize {
        if self.local_sections_visible() {
            3
        } else {
            1
        }
    }

    fn items_count(&self, section: usize, _cx: &App) -> usize {
        match section {
            SECTION_ISSUES => self.issue_hits.len(),
            SECTION_FILES => self.file_hits.len(),
            SECTION_CONTENT => self.content_hits.len(),
            _ => 0,
        }
    }

    fn render_section_header(
        &mut self,
        section: usize,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        // No headers in issues-only mode (web parity — a flat list).
        if !self.local_sections_visible() {
            return None;
        }
        let (label, truncated, cap) = match section {
            SECTION_ISSUES => ("Issues", false, MAX_RESULTS),
            SECTION_FILES => ("Files", self.file_truncated, FILE_RESULT_CAP),
            SECTION_CONTENT => ("In files", self.content_truncated, CONTENT_RESULT_CAP),
            _ => return None,
        };
        let mut header = h_flex()
            .h(px(HEADER_HEIGHT))
            .px_3()
            .items_center()
            .justify_between()
            .bg(cx.theme().muted.opacity(0.4))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(label)),
            );
        if truncated {
            header = header.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(SharedString::from(format!("first {cap}"))),
            );
        }
        Some(header.into_any_element())
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        _cx: &mut gpui::Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let row = match ix.section {
            SECTION_ISSUES => self.render_issue_row(ix, _cx),
            SECTION_FILES => self.render_file_row(ix, _cx),
            SECTION_CONTENT => self.render_content_row(ix, _cx),
            _ => None,
        };
        // The List measures a single row height from item (0,0) even when that
        // section is momentarily empty (e.g. a query that only hits files).
        // Return a blank but full-height row for an out-of-range index so the
        // virtualized list measures ROW_HEIGHT instead of collapsing to zero.
        Some(row.unwrap_or_else(|| {
            ListItem::new(SharedString::from(format!("blank-{}-{}", ix.section, ix.row)))
                .h(px(ROW_HEIGHT))
        }))
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut gpui::Context<ListState<Self>>,
    ) {
        self.selected = ix;
    }

    /// Click or Enter: issues navigate to their detail; file/content hits open
    /// in the read-only viewer. Either way the dialog closes.
    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) {
        let Some(ix) = self.selected else {
            return;
        };
        match ix.section {
            SECTION_ISSUES => {
                let Some(hit) = self.issue_hits.get(ix.row) else {
                    return;
                };
                let issue_id = hit.issue_id.clone();
                navigate(window, cx, Screen::IssueDetail { issue_id });
                window.close_dialog(cx);
            }
            SECTION_FILES => {
                let Some(path) = self.file_hits.get(ix.row).cloned() else {
                    return;
                };
                self.open_file(path, window, cx);
            }
            SECTION_CONTENT => {
                // Line targeting is not supported by the read-only viewer
                // (`set_path` takes no line), so open the file at its top.
                let Some(path) = self.content_hits.get(ix.row).map(|hit| hit.path.clone()) else {
                    return;
                };
                self.open_file(path, window, cx);
            }
            _ => {}
        }
    }

    /// Esc closes (the List consumes Escape ahead of the dialog's own
    /// binding, so the delegate owns the close).
    fn cancel(&mut self, window: &mut Window, cx: &mut gpui::Context<ListState<Self>>) {
        window.close_dialog(cx);
    }

    /// Web: the pre-query hint.
    fn render_initial(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Option<gpui::AnyElement> {
        let hint = if self.local_sections_visible() {
            "Type to search issues and files"
        } else {
            "Type to search issues"
        };
        Some(
            v_flex()
                .items_center()
                .justify_center()
                .p_8()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(
                    Icon::new(IconName::Search)
                        .size_6()
                        .text_color(cx.theme().muted_foreground.opacity(0.5)),
                )
                .child(div().text_sm().child(hint))
                .into_any_element(),
        )
    }

    /// Web: `No results for "{query}"`.
    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> impl IntoElement {
        v_flex()
            .items_center()
            .justify_center()
            .p_8()
            .text_color(cx.theme().muted_foreground)
            .child(
                div()
                    .text_sm()
                    .child(SharedString::from(format!("No results for \"{}\"", self.query))),
            )
    }
}

// ---------------------------------------------------------------------------
// Row/text helpers (uniform two-line rows across all three sections)
// ---------------------------------------------------------------------------

fn two_line_row(id: impl Into<gpui::ElementId>, cx: &App) -> ListItem {
    ListItem::new(id)
        .h(px(ROW_HEIGHT))
        .px_3()
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.3))
}

fn line_primary(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_sm()
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .child(text.into())
}

fn line_secondary(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .child(text.into())
}

/// Split a trunk-relative path into `(directory, basename)` (directory empty
/// for a root-level file).
fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    }
}

// ---------------------------------------------------------------------------
// Git file listing + content grep (argv git only — DNR L5; background-safe)
// ---------------------------------------------------------------------------

/// The repo's searchable files: tracked (`--cached`) + untracked-but-not-ignored
/// (`--others --exclude-standard`), so `.gitignore` is respected. `-z` for NUL
/// separation (paths with odd bytes survive). Empty on any failure.
fn list_repo_files(root: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
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
    let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect();
    files.truncate(MAX_FILE_LIST);
    files
}

/// Run `git grep` for `query` under `root` on the calling (background) thread,
/// streaming stdout and stopping (killing the child) once `cap` hits land.
///
/// Flags: `-n` line numbers, `-I` skip binary, `-i` case-insensitive, `-F`
/// fixed-string (the query is a literal, never a regex), `-z` NUL field
/// separators (`path\0line\0content\n`), `--untracked` also searches untracked
/// non-ignored files (still honoring `.gitignore`), `--no-color`.
fn git_grep(root: &Path, query: &str, cap: usize) -> (Vec<ContentHit>, bool) {
    let child = Command::new("git")
        .args([
            "grep",
            "--no-color",
            "-n",
            "-I",
            "-i",
            "-F",
            "-z",
            "--untracked",
            "-e",
            query,
        ])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .spawn();
    let Ok(mut child) = child else {
        return (Vec::new(), false);
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return (Vec::new(), false);
    };

    let mut hits = Vec::new();
    let mut truncated = false;
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        let Some(hit) = parse_grep_line(&line) else {
            continue;
        };
        if hits.len() >= cap {
            truncated = true;
            break;
        }
        hits.push(hit);
    }
    // Stop git early once capped (or on a read error); reap it either way.
    let _ = child.kill();
    let _ = child.wait();
    (hits, truncated)
}

/// Parse one NUL-separated `git grep -z -n` record: `path\0line\0content`.
fn parse_grep_line(line: &str) -> Option<ContentHit> {
    let mut parts = line.splitn(3, '\0');
    let path = parts.next()?;
    let line_no = parts.next()?.parse::<u32>().ok()?;
    let preview = parts.next().unwrap_or("");
    if path.is_empty() {
        return None;
    }
    Some(ContentHit {
        path: path.to_string(),
        line: line_no,
        preview: clamp_preview(preview),
    })
}

/// Trim a matched line and clamp it so one giant line can't blow up the row.
fn clamp_preview(preview: &str) -> String {
    let trimmed = preview.trim();
    if trimmed.chars().count() > PREVIEW_MAX_CHARS {
        let clamped: String = trimmed.chars().take(PREVIEW_MAX_CHARS).collect();
        format!("{clamped}…")
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Fuzzy filename matching (dependency-free — no fuzzy crate is in the tree)
// ---------------------------------------------------------------------------

/// The outcome of a filename fuzzy-rank pass.
struct RankResult {
    hits: Vec<String>,
    truncated: bool,
}

/// Rank `files` by a fuzzy subsequence match against `query`, best first,
/// capped at `cap` (with a `truncated` flag when more matched). Case-insensitive.
fn rank_files(files: &[String], query: &str, cap: usize) -> RankResult {
    let query = query.to_ascii_lowercase();
    let mut scored: Vec<(i32, &String)> = files
        .iter()
        .filter_map(|path| fuzzy_score(&query, path).map(|score| (score, path)))
        .collect();
    // Best score first; ties → shorter path, then lexicographic (stable, so a
    // given query always orders the same way).
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.len().cmp(&b.1.len()))
            .then_with(|| a.1.cmp(b.1))
    });
    let truncated = scored.len() > cap;
    let hits = scored
        .into_iter()
        .take(cap)
        .map(|(_, path)| path.clone())
        .collect();
    RankResult { hits, truncated }
}

/// Case-insensitive subsequence match with a small VS-Code-style score:
/// boundary starts (`/ _ - .` / camelCase), consecutive runs, and the basename
/// region are rewarded. Returns `None` when `query` is not a subsequence of
/// `text`. `query` must already be ASCII-lowercased; higher is better.
fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let chars: Vec<char> = text.chars().collect();
    // Char index at which the basename begins (0 for a root-level file).
    let basename_start = text
        .rfind('/')
        .map(|byte| text[..byte].chars().count() + 1)
        .unwrap_or(0);

    let mut qi = 0usize;
    let mut score = 0i32;
    let mut prev_matched = false;
    for (i, &ch) in chars.iter().enumerate() {
        if qi >= q.len() {
            break;
        }
        if ch.to_ascii_lowercase() == q[qi] {
            let at_boundary = i == 0 || {
                let prev = chars[i - 1];
                prev == '/'
                    || prev == '_'
                    || prev == '-'
                    || prev == '.'
                    || prev == ' '
                    || (prev.is_ascii_lowercase() && ch.is_ascii_uppercase())
            };
            let mut bonus = 1;
            if at_boundary {
                bonus += 10;
            }
            if prev_matched {
                bonus += 8;
            }
            if i >= basename_start {
                bonus += 5;
            }
            score += bonus;
            qi += 1;
            prev_matched = true;
        } else {
            prev_matched = false;
        }
    }
    (qi == q.len()).then_some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_requires_full_subsequence() {
        assert!(fuzzy_score("search", "crates/ui/src/search_sheet.rs").is_some());
        // A char not present (in order) → no match.
        assert!(fuzzy_score("zzz", "search_sheet.rs").is_none());
        // Out of order → no match.
        assert!(fuzzy_score("tehsear", "search_sheet.rs").is_none());
    }

    #[test]
    fn fuzzy_is_case_insensitive() {
        // Query is pre-lowercased by the caller; text may be mixed case.
        assert!(fuzzy_score("readme", "docs/README.md").is_some());
    }

    #[test]
    fn fuzzy_scores_basename_above_deep_path() {
        // "search" in the basename should beat "search" spread across dirs.
        let basename = fuzzy_score("search", "a/b/search_sheet.rs").unwrap();
        let scattered = fuzzy_score("search", "search/e/a/r/c/h/x.rs").unwrap();
        assert!(
            basename > scattered,
            "basename {basename} should beat scattered {scattered}"
        );
    }

    #[test]
    fn fuzzy_rewards_consecutive_and_boundary() {
        // Contiguous run beats the same chars split up.
        let contiguous = fuzzy_score("abc", "abc.rs").unwrap();
        let split = fuzzy_score("abc", "axbxc.rs").unwrap();
        assert!(contiguous > split);
    }

    #[test]
    fn rank_orders_best_first_and_caps() {
        let files = vec![
            "src/search_sheet.rs".to_string(), // "search" contiguous in the basename
            "src/search/board.rs".to_string(), // "search" in the directory, not basename
            "docs/research/notes.md".to_string(), // "search" as a subsequence of "research"
            "src/unrelated.rs".to_string(),    // not a subsequence of "search"
        ];
        let ranked = rank_files(&files, "search", 2);
        assert_eq!(ranked.hits.len(), 2);
        assert!(ranked.truncated); // 3 of the 4 match, cap is 2
        // Basename match outranks a directory-only match.
        assert_eq!(ranked.hits[0], "src/search_sheet.rs");
    }

    #[test]
    fn rank_no_matches_is_empty_not_truncated() {
        let files = vec!["a.rs".to_string(), "b.rs".to_string()];
        let ranked = rank_files(&files, "zzz", 10);
        assert!(ranked.hits.is_empty());
        assert!(!ranked.truncated);
    }

    #[test]
    fn parse_grep_line_splits_nul_fields() {
        let hit = parse_grep_line("src/lib.rs\u{0}42\u{0}    let x = run_root();").unwrap();
        assert_eq!(hit.path, "src/lib.rs");
        assert_eq!(hit.line, 42);
        assert_eq!(hit.preview, "let x = run_root();");
    }

    #[test]
    fn parse_grep_line_rejects_malformed() {
        assert!(parse_grep_line("no-nuls-here").is_none());
        assert!(parse_grep_line("path\u{0}notanumber\u{0}body").is_none());
    }

    #[test]
    fn clamp_preview_trims_and_bounds() {
        assert_eq!(clamp_preview("   hello  "), "hello");
        let long: String = "x".repeat(PREVIEW_MAX_CHARS + 50);
        let clamped = clamp_preview(&long);
        assert_eq!(clamped.chars().count(), PREVIEW_MAX_CHARS + 1); // +1 for the ellipsis
        assert!(clamped.ends_with('…'));
    }

    #[test]
    fn split_path_separates_dir_and_basename() {
        assert_eq!(split_path("a/b/c.rs"), ("a/b", "c.rs"));
        assert_eq!(split_path("root.rs"), ("", "root.rs"));
    }
}
