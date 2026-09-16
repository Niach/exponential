//! The ⌘K / ⌘F quick-open — web parity target
//! `apps/web/src/components/issue-search-sheet.tsx` (masterplan-v3 §4.2:
//! "`IssueSearchSheet` is a `Dialog` (⌘K quick-open by title/identifier)").
//!
//! EXP-892: it searches ISSUES, nothing else. One flat result list with no
//! section headers, exactly like the web dialog — the EXP-15 repo file finder
//! and its `git grep` content pass are gone (the Files tool owns the repo).
//!
//! Desktop renders the web component's desktop branch only (§4.9 — no
//! mobile bottom sheet): a `Dialog` anchored 15% from the opener's top with a
//! borderless search input and the result rows. EXP-716: the window is SIZED
//! TO ITS CONTENT — it opens at the empty-state height and grows downward
//! (top edge pinned, [`native_dialog::resize_dialog_keeping_top`]) as rows
//! land, up to the web's `max-h-[60vh]` cap, instead of standing as a fixed
//! 60vh tower around a single hit. Picking a row navigates to the issue
//! detail and closes the dialog.
//!
//! Built on gpui-component's `List` (`ListState` + [`ListDelegate`]): the
//! virtualization, Enter-confirm and Esc-cancel come from the component. The
//! shared list keyboard contract (EXP-892, uniform ×4) rides on top — the top
//! row is selected as soon as results land, ↑/↓ move the selection, hovering
//! a row MOVES it (one highlight, never a second hover tint) and Enter opens
//! the selected issue.
//!
//! [`init`] registers the App-global [`OpenSearch`] handler (the sidebar's
//! Search row dispatches it) and the global ⌘K / ⌘F bindings.
//!
//! Searching itself is the ONE engine (`domain::issue_search`): an instant
//! local rank over the synced rows, then the server's full-text pass
//! (`api::issues::search`, debounced 250 ms) spliced in behind them. Every
//! async result is applied only when its query is still current (stale-drop),
//! and the List drops the previous `perform_search` task on each keystroke
//! (that drop is the debounce cancel).

use std::collections::HashSet;
use std::time::Duration;

use gpui::{
    div, prelude::FluentBuilder as _, px, size, App, AppContext as _, Entity,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement, Pixels, Render, SharedString,
    Styled, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    list::{List, ListDelegate, ListItem, ListState},
    v_flex, ActiveTheme as _, Icon, IndexPath, Sizable as _,
};
use sync::{SessionPhase, Store};

use crate::native_dialog::{self, DialogContent, DialogSpec};

use crate::actions::OpenSearch;
use crate::controls::glass_input;
use crate::icons::registry;
use crate::issue_list::parse_hex_color;
use crate::navigation::{active_team_id, nav_for_window, navigate, Screen};

/// Web `.slice(0, 30)` — cap the result list.
const MAX_RESULTS: usize = 30;

/// EXP-3: how long a keystroke must rest before the server full-text pass
/// fires. The List replaces its search task per keystroke (dropping — i.e.
/// cancelling — the previous one), so the timer doubles as the debouncer.
const SERVER_SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

/// EXP-3: server `issues.search` page size (server default 20, max 50).
const SERVER_SEARCH_LIMIT: u32 = 20;

/// Web `sm:max-w-lg` (32rem).
const DIALOG_WIDTH: f32 = 512.;

/// Every result row is this tall (a two-line row, web `px-4 py-3` scale).
/// The List measures ONE item height for the whole virtualized list.
const ROW_HEIGHT: f32 = 54.;

/// The query row (web cmdk `h-14` wrapper class), border included.
const INPUT_ROW_HEIGHT: f32 = 52.;

/// EXP-716: the empty state (pre-query hint / "No results") is a FIXED-height
/// block so the palette's opening size is deterministic and the two empty
/// variants never resize the window against each other.
const EMPTY_HEIGHT: f32 = 132.;

/// Web `sm:top-[15%]`: the palette's top edge as a fraction of the opener's
/// height. With the 60vh cap the window bottoms out at 75% of the opener.
const TOP_ANCHOR: f32 = 0.15;

/// Web `sm:max-h-[60vh]`, with EXP-415's absolute cap for very tall screens.
fn max_palette_height(window: &Window) -> Pixels {
    (window.viewport_size().height * 0.6).min(px(640.))
}

/// Register the App-global open handler + the quick-open keybindings. Called
/// once from `ui::init`.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &OpenSearch, cx| {
        crate::navigation::on_active_window(cx, |window, cx| open_search(window, cx));
    });
    // ⌘K (command-palette) and ⌘F (§8.13 quick-find parity) both open the
    // same issue search — there is no second find surface to route ⌘F to.
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
    // modal is up); EXP-287 raises that dialog instead of dropping the ⌘K.
    if native_dialog::raise_existing_dialog(window, cx) {
        return;
    }
    let nav = nav_for_window(window, cx);
    let Some(team_id) = active_team_id(&nav, cx) else {
        return;
    };

    // Web caps at max-h-[60vh] and SHRINKS to content. EXP-716: so does the
    // native window now — it opens at the empty-state height and
    // `SearchSheetView::fit_to_content` grows it toward this cap as results
    // land (EXP-415's absolute cap keeps very tall screens from a tower).
    let max_height = max_palette_height(window);
    let height =
        px(INPUT_ROW_HEIGHT + EMPTY_HEIGHT) + native_dialog::chromeless_vertical_chrome(None);
    // EXP-285: chromeless — macOS traffic lights would overlap the search
    // input of a palette.
    let spec = DialogSpec::new("Search", size(px(DIALOG_WIDTH), height))
        .chromeless()
        .anchor_top(TOP_ANCHOR);
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        // EXP-525: the List is NOT `searchable` — the web has no "Search"
        // title row, a taller borderless input and no clear-✕, none of which
        // the component's baked-in query strip can render. The view owns the
        // input and drives the list through `set_query`.
        let list = cx.new(|cx| ListState::new(SearchDelegate::new(team_id), window, cx));
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search issues…"));
        // Focus the query input so typing starts immediately (web autoFocus).
        input.update(cx, |state, cx| state.focus(window, cx));
        // DEV-ONLY (§11.4 headless verification, the EXP_DEV_* family):
        // `EXP_DEV_SEARCH_QUERY=<text>` opens the palette with the query
        // already typed — both the input's value AND the list's query, since
        // `set_value` fires no `InputEvent::Change`. Unset/blank in normal
        // runs. Never document for users.
        if let Some(query) = std::env::var("EXP_DEV_SEARCH_QUERY")
            .ok()
            .map(|query| query.trim().to_string())
            .filter(|query| !query.is_empty())
        {
            input.update(cx, |state, cx| state.set_value(query.clone(), window, cx));
            list.update(cx, |list, cx| list.set_query(&query, window, cx));
        }
        let view = cx.new(|cx| SearchSheetView::new(list, input, max_height, window, cx));
        DialogContent::new(view).padless()
    });
}

/// Window-content wrapper: the web `IssueSearchSheet` shell — an h-14-class
/// borderless input row (search glyph prefix, in-row ✕ as the mouse
/// dismissal a chromeless window needs, EXP-415) over the result list. Owns
/// keyboard nav: ↑/↓ walk the rows, Enter confirms.
struct SearchSheetView {
    list: Entity<ListState<SearchDelegate>>,
    input: Entity<InputState>,
    /// EXP-716: the height cap the window grows toward — web `max-h-[60vh]`,
    /// computed from the OPENER's viewport at open time.
    max_height: Pixels,
    /// The last window height `fit_to_content` asked for, so an in-flight
    /// resize (the viewport follows a frame later) is not re-requested.
    last_requested_height: Option<Pixels>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl SearchSheetView {
    fn new(
        list: Entity<ListState<SearchDelegate>>,
        input: Entity<InputState>,
        max_height: Pixels,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // EXP-716: results land on the ListState (its own notify); the view
        // must re-render on each to refit the window around them.
        let list_observer = cx.observe(&list, |_, _, cx| cx.notify());
        let subscription = cx.subscribe_in(
            &input,
            _window,
            |this: &mut Self, state, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = state.read(cx).value().to_string();
                    this.list
                        .update(cx, |list, cx| list.set_query(&value, window, cx));
                }
                InputEvent::PressEnter { .. } => this.confirm(window, cx),
                _ => {}
            },
        );
        Self {
            list,
            input,
            max_height,
            last_requested_height: None,
            _subscriptions: vec![list_observer, subscription],
        }
    }

    /// EXP-716: size the window to the query row + the list's content (its
    /// rows are fixed-height, so the height is exact) + the window's own
    /// chrome, capped at [`Self::max_height`]. Runs at render time; the
    /// resize itself is deferred and top-anchored, so the palette extends
    /// downward like a dropdown and shrinks back when a query narrows.
    fn fit_to_content(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let content = self.list.read(cx).delegate().content_height();
        let chrome = native_dialog::chromeless_vertical_chrome(Some(window));
        let target = (px(INPUT_ROW_HEIGHT) + content + chrome).min(self.max_height);
        let current = window.viewport_size();
        if (target - current.height).abs() <= px(1.) || self.last_requested_height == Some(target) {
            return;
        }
        self.last_requested_height = Some(target);
        native_dialog::resize_dialog_keeping_top(window, cx, size(current.width, target));
    }

    /// EXP-892 (the uniform list contract): the TOP row is selected as soon
    /// as results arrive and stays selected while typing — a query that
    /// narrows past the old selection re-selects row 0, an empty list
    /// deselects. Idempotent, so running it every render settles in one
    /// frame.
    fn ensure_selection(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let count = self.list.read(cx).delegate().hit_count();
        let selected = self.list.read(cx).selected_index();
        let target = match selected {
            _ if count == 0 => None,
            Some(ix) if ix.section == 0 && ix.row < count => Some(ix),
            _ => Some(IndexPath::default()),
        };
        if target == selected {
            return;
        }
        self.list
            .update(cx, |list, cx| list.set_selected_index(target, window, cx));
    }

    /// ↑/↓ from the input: move the list selection, clamped to the ends.
    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let count = self.list.read(cx).delegate().hit_count();
        if count == 0 {
            return;
        }
        let current = self
            .list
            .read(cx)
            .selected_index()
            .map(|selected| selected.row)
            .unwrap_or(0);
        let next = current.min(count - 1).saturating_add_signed(delta).min(count - 1);
        self.list.update(cx, |list, cx| {
            list.set_selected_index(
                Some(IndexPath {
                    section: 0,
                    row: next,
                    column: 0,
                }),
                window,
                cx,
            );
            list.scroll_to_selected_item(window, cx);
        });
        cx.notify();
    }

    /// ↑/↓ arrive as the input's own `MoveUp`/`MoveDown` actions — a
    /// single-line input no-ops them, so the wrapper CAPTURES them for list
    /// navigation (the `MentionInput` pattern).
    fn on_move_up(
        &mut self,
        _: &gpui_component::input::MoveUp,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.move_selection(-1, window, cx);
    }

    fn on_move_down(
        &mut self,
        _: &gpui_component::input::MoveDown,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.move_selection(1, window, cx);
    }

    /// Enter: confirm the selection (first row when nothing is selected yet —
    /// web's cmdk does the same).
    fn confirm(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.list.read(cx).selected_index().is_none() {
            if self.list.read(cx).delegate().hit_count() == 0 {
                return;
            }
            self.list.update(cx, |list, cx| {
                list.set_selected_index(Some(IndexPath::default()), window, cx)
            });
        }
        self.list.update(cx, |list, cx| {
            list.delegate_mut().confirm(false, window, cx);
        });
    }
}

impl Render for SearchSheetView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.fit_to_content(window, cx);
        self.ensure_selection(window, cx);
        let max_h = window.viewport_size().height;
        let muted = cx.theme().muted_foreground;
        v_flex()
            .size_full()
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .child(
                // Web: the cmdk input wrapper — h-14, borderless input with a
                // leading search glyph, hairline below.
                h_flex()
                    .flex_shrink_0()
                    .h(px(INPUT_ROW_HEIGHT))
                    .px_4()
                    .gap_2()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.5))
                    .child(
                        Icon::new(registry::NAV_SEARCH)
                            .size_4()
                            .flex_shrink_0()
                            .text_color(muted),
                    )
                    .child(
                        div().flex_1().min_w_0().text_size(px(14.)).child(
                            glass_input(&self.input, window, cx).appearance(false).p_0(),
                        ),
                    )
                    .child(
                        Button::new("search-sheet-close")
                            .ghost()
                            .cursor_pointer()
                            .xsmall()
                            .icon(Icon::new(registry::UI_CLOSE).small().text_color(muted))
                            .on_click(|_, window, cx| {
                                native_dialog::close_dialog_window(window, cx);
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(List::new(&self.list).max_h(max_h)),
            )
    }
}

/// One resolved issue hit — board name/color denormalized at search time
/// (web `boardMap.get(issue.boardId)`).
struct SearchHit {
    issue_id: String,
    identifier: String,
    title: String,
    /// EXP-314: the RESOLVED status. The sheet is single-TEAM, so resolution
    /// against the team's rows is exact — a server hit that is not synced
    /// locally resolves from its `status_id`/anchor pair.
    status: domain::statuses::ResolvedStatus,
    board_name: Option<String>,
    board_color: Option<String>,
    /// EXP-525: the board's real glyph (web `BoardGlyph`) — the sub-line used
    /// to show a bare color dot.
    board_glyph: Option<crate::icons::ExpIcon>,
}

pub struct SearchDelegate {
    team_id: String,
    query: String,
    issue_hits: Vec<SearchHit>,
    selected: Option<IndexPath>,
}

impl SearchDelegate {
    fn new(team_id: String) -> Self {
        Self {
            team_id,
            query: String::new(),
            issue_hits: Vec::new(),
            selected: None,
        }
    }

    fn hit_count(&self) -> usize {
        self.issue_hits.len()
    }

    /// EXP-716: the exact height the List renders for the current results —
    /// every row is [`ROW_HEIGHT`]; no results at all render the fixed
    /// [`EMPTY_HEIGHT`] block.
    fn content_height(&self) -> Pixels {
        if self.issue_hits.is_empty() {
            return px(EMPTY_HEIGHT);
        }
        px(self.issue_hits.len() as f32 * ROW_HEIGHT)
    }

    /// EXP-892: rank the synced team issues with the ONE engine
    /// (`domain::issue_search` — identifier over title over description, ties
    /// by recency). Hits snapshot at search time — an Electric echo mid-dialog
    /// refreshes on the next keystroke.
    fn search_issues(&mut self, cx: &App) {
        self.issue_hits.clear();
        if self.query.is_empty() {
            return;
        }
        let collections = Store::global(cx).collections();
        let boards = collections.boards.read(cx);
        let statuses = crate::queries::team_statuses(cx, &self.team_id);
        let issues = collections.issue_refs_in_team(&self.team_id, cx);
        let rows: Vec<domain::issue_search::SearchRow<'_>> =
            issues.iter().map(|issue| (*issue).into()).collect();
        let order =
            domain::issue_search::rank(&rows, &self.query, MAX_RESULTS, &HashSet::new());
        self.issue_hits = order
            .into_iter()
            .map(|ix| {
                let issue = issues[ix];
                let board = boards.get(&issue.board_id);
                SearchHit {
                    issue_id: issue.id.clone(),
                    identifier: issue.identifier.clone(),
                    title: issue.title.clone(),
                    status: domain::statuses::resolve_status_sorted(issue, &statuses),
                    board_name: board.map(|p| p.name.clone()),
                    board_color: board.and_then(|p| p.color.clone()),
                    board_glyph: board.map(crate::icons::board_glyph),
                }
            })
            .collect();
    }

    /// EXP-3/EXP-892: splice the server's full-text hits (description/comment
    /// matches) in behind the local ranking with the shared merge, deduped by
    /// id and capped across both halves. The synced row wins for rendering; a
    /// hit not (yet) synced locally renders from the returned fields — its
    /// board denormalization then resolves best-effort.
    fn merge_server_hits(&mut self, server_hits: Vec<api::issues::IssueSearchHit>, cx: &App) {
        let collections = Store::global(cx).collections();
        let issues = collections.issues.read(cx);
        let boards = collections.boards.read(cx);
        let statuses = crate::queries::team_statuses(cx, &self.team_id);
        let local = std::mem::take(&mut self.issue_hits);
        self.issue_hits = domain::issue_search::merge_server_hits(
            local,
            &server_hits,
            MAX_RESULTS,
            &HashSet::new(),
            |hit: &SearchHit| hit.issue_id.as_str(),
            |hit: &api::issues::IssueSearchHit| hit.id.as_str(),
            |hit: &api::issues::IssueSearchHit| {
                // Prefer the LOCAL synced row (fresher than the search
                // snapshot); fall back to the hit's own `status_id`/anchor
                // pair, resolved against the same team vocabulary.
                let (identifier, title, status, board_id) = match issues.get(&hit.id) {
                    Some(issue) => (
                        issue.identifier.clone(),
                        issue.title.clone(),
                        domain::statuses::resolve_status_sorted(issue, &statuses),
                        issue.board_id.clone(),
                    ),
                    None => (
                        hit.identifier.clone(),
                        hit.title.clone(),
                        crate::queries::resolve_status_ref(
                            hit.status_id.as_deref(),
                            hit.status,
                            &statuses,
                        ),
                        hit.board_id.clone(),
                    ),
                };
                let board = boards.get(&board_id);
                Some(SearchHit {
                    issue_id: hit.id.clone(),
                    identifier,
                    title,
                    status,
                    board_name: board.map(|p| p.name.clone()),
                    board_color: board.and_then(|p| p.color.clone()),
                    board_glyph: board.map(crate::icons::board_glyph),
                })
            },
        );
    }

    fn render_issue_row(
        &self,
        ix: IndexPath,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let hit = self.issue_hits.get(ix.row)?;
        let board_tint = hit
            .board_color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let subtitle: SharedString = match &hit.board_name {
            Some(name) => format!("{name} · {}", hit.identifier).into(),
            None => hit.identifier.clone().into(),
        };
        let muted = cx.theme().muted_foreground;
        let status_icon = crate::icons::resolved_status_icon(&hit.status, cx).small();

        Some(
            two_line_row(("issue-hit", ix.row), cx)
                // EXP-892: hovering MOVES the selection — one highlight, and
                // Enter always picks what the pointer is on.
                .on_mouse_enter(cx.listener(move |list, _, window, cx| {
                    if list.selected_index() != Some(ix) {
                        list.set_selected_index(Some(ix), window, cx);
                    }
                }))
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .overflow_hidden()
                        .child(status_icon)
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
                                        .text_color(muted)
                                        // Web `BoardGlyph size-3`: the board's
                                        // real icon, tinted its color.
                                        .when_some(hit.board_glyph.clone(), |row, glyph| {
                                            row.child(
                                                Icon::from(glyph)
                                                    .size_3()
                                                    .flex_shrink_0()
                                                    .text_color(board_tint),
                                            )
                                        })
                                        .child(line_secondary(subtitle)),
                                ),
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
        // The instant local pass.
        self.search_issues(cx);

        if self.query.is_empty() {
            return Task::ready(());
        }

        // The slow, debounced pass: the server's full-text issue search.
        // Returning the task (not detaching) hands its lifetime to the List,
        // which drops it on the next keystroke — that drop IS the debounce
        // cancel.
        let trpc = crate::queries::trpc_client(cx);
        let team_id = self.team_id.clone();
        let query = self.query.clone();
        cx.spawn_in(window, async move |this, window| {
            window
                .background_executor()
                .timer(SERVER_SEARCH_DEBOUNCE)
                .await;

            let Some(trpc) = trpc else {
                return;
            };
            let (search_team, search_query) = (team_id.clone(), query.clone());
            let result = window
                .background_executor()
                .spawn(async move {
                    api::issues::search(&trpc, &search_team, &search_query, SERVER_SEARCH_LIMIT)
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
                    // Local-only degradation — the local hits already render.
                    log::warn!("[ui] search: issues.search failed: {error}");
                }
            }
        })
    }

    fn sections_count(&self, _cx: &App) -> usize {
        1
    }

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.issue_hits.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        // The List measures a single row height from item (0,0) even when the
        // list is momentarily empty. Return a blank but full-height row for an
        // out-of-range index so the virtualized list measures ROW_HEIGHT
        // instead of collapsing to zero.
        Some(self.render_issue_row(ix, cx).unwrap_or_else(|| {
            ListItem::new(SharedString::from(format!("blank-{}", ix.row))).h(px(ROW_HEIGHT))
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

    /// Click or Enter: navigate to the issue detail and close the dialog.
    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) {
        let Some(ix) = self.selected else {
            return;
        };
        let Some(hit) = self.issue_hits.get(ix.row) else {
            return;
        };
        let issue_id = hit.issue_id.clone();
        native_dialog::close_then(window, cx, move |window, cx| {
            navigate(window, cx, Screen::IssueDetail { issue_id });
        });
    }

    /// Esc closes (the List consumes Escape ahead of the dialog shell's own
    /// binding, so the delegate owns the close).
    fn cancel(&mut self, window: &mut Window, cx: &mut gpui::Context<ListState<Self>>) {
        native_dialog::close_dialog_window(window, cx);
    }

    /// Web: the pre-query hint (empty query) or `No issues match "{query}"`.
    /// Both live here — the view owns the query input (EXP-525), so the
    /// component's searchable-only `render_initial` never runs.
    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui::Context<ListState<Self>>,
    ) -> impl IntoElement {
        // EXP-716: fixed height — see `EMPTY_HEIGHT`.
        let empty = v_flex()
            .h(px(EMPTY_HEIGHT))
            .items_center()
            .justify_center()
            .px_12()
            .gap_3()
            .text_color(cx.theme().muted_foreground);
        if self.query.is_empty() {
            empty
                .child(
                    Icon::new(registry::NAV_SEARCH)
                        .size(px(32.))
                        .text_color(cx.theme().muted_foreground.opacity(0.5)),
                )
                .child(div().text_sm().child("Type to search issues"))
        } else {
            empty.child(div().text_sm().child(SharedString::from(format!(
                "No issues match \"{}\"",
                self.query
            ))))
        }
    }
}

// ---------------------------------------------------------------------------
// Row/text helpers
// ---------------------------------------------------------------------------

fn two_line_row(id: impl Into<gpui::ElementId>, cx: &App) -> ListItem {
    ListItem::new(id)
        .h(px(ROW_HEIGHT))
        .px_4()
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
