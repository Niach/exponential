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
//! SLOP-28 (EXP-1259 D4): the body is THE search list
//! ([`crate::picker::search_list`]) — its input row (glyph, field, result
//! count), its one-line issue rows (status glyph, mono identifier, title; the
//! two-line board row is gone ×4), at most eight rows visible, then it
//! scrolls under the bottom fade. The shared list keyboard contract (EXP-892,
//! uniform ×4): the top row is selected as soon as results land, ↑/↓ move
//! it, hovering a row MOVES it, Enter or Tab opens the selected issue, Escape
//! closes.
//!
//! [`init`] registers the App-global [`OpenSearch`] handler (the sidebar's
//! Search row dispatches it) and the global ⌘K / ⌘F bindings.
//!
//! Searching itself is the ONE engine (`domain::issue_search`): an instant
//! local rank over the synced rows, then the server's full-text pass
//! (`api::issues::search`, debounced 250 ms) spliced in behind them
//! ([`search_list::local_issue_hits`], [`search_list::splice_server_hits`]).
//! Every async result is applied only when its query is still current
//! (stale-drop), and each keystroke drops the previous pass (the debounce).

use gpui::{
    div, px, size, App, AppContext as _, InteractiveElement as _, StatefulInteractiveElement as _,
    IntoElement, KeyBinding, ParentElement, Pixels, Render, Styled, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::InputEvent,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::{SessionPhase, Store};

use crate::native_dialog::{self, DialogContent, DialogSpec};

use crate::actions::OpenSearch;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, navigate, Screen};
use crate::picker::search_list::{self, IssueHit, SearchListState};

/// Web `sm:max-w-lg` (32rem).
const DIALOG_WIDTH: f32 = 512.;

/// The query row and the hairline under it (the search list's).
const INPUT_ROW_HEIGHT: f32 = search_list::INPUT_HEIGHT + 1.;

/// The rows' inset inside the sheet (the search list's `p_1`, both edges).
const ROWS_INSET: f32 = 8.;

/// EXP-716: the empty state (pre-query hint / "No results") is a FIXED-height
/// block so the palette's opening size is deterministic and the two empty
/// variants never resize the window against each other.
const EMPTY_HEIGHT: f32 = 132.;

/// EXP-922: the ONE copy set every search surface renders — web
/// `lib/issue-search.ts`, iOS `SearchView`, Android `SearchScreen`. Same words
/// everywhere; the web `issue-search-surfaces.test.ts` greps THIS file for them.
const SEARCH_PLACEHOLDER: &str = "Search issues";
const SEARCH_EMPTY_HINT: &str = "Search issues across this team's boards.";
const SEARCH_EMPTY_DETAIL: &str = "Matches identifiers, titles, and full text.";
const SEARCH_NO_RESULTS: &str = "No issues match";

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
        let view = cx.new(|cx| SearchSheetView::new(team_id, max_height, window, cx));
        DialogContent::new(view).padless()
    });
}

/// Window-content wrapper: the web `IssueSearchSheet` shell — THE search
/// list's input row (with an in-row ✕, the mouse dismissal a chromeless
/// window needs, EXP-415) over its rows. Owns the keys.
struct SearchSheetView {
    team_id: String,
    list: SearchListState,
    hits: Vec<IssueHit>,
    /// The query `hits` answer.
    searched: String,
    server: Option<Task<()>>,
    /// EXP-716: the height cap the window grows toward — web `max-h-[60vh]`,
    /// computed from the OPENER's viewport at open time.
    max_height: Pixels,
    /// The last window height `fit_to_content` asked for, so an in-flight
    /// resize (the viewport follows a frame later) is not re-requested.
    last_requested_height: Option<Pixels>,
    _subscription: gpui::Subscription,
}

impl SearchSheetView {
    fn new(team_id: String, max_height: Pixels, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let list = SearchListState::new(SEARCH_PLACEHOLDER, window, cx);
        // Focus the query input so typing starts immediately (web autoFocus).
        list.input.update(cx, |state, cx| state.focus(window, cx));
        // DEV-ONLY (§11.4 headless verification, the EXP_DEV_* family):
        // `EXP_DEV_SEARCH_QUERY=<text>` opens the palette with the query
        // already typed. Unset/blank in normal runs. Never document for users.
        if let Some(query) = std::env::var("EXP_DEV_SEARCH_QUERY")
            .ok()
            .map(|query| query.trim().to_string())
            .filter(|query| !query.is_empty())
        {
            list.input.update(cx, |state, cx| state.set_value(query, window, cx));
        }
        let subscription = cx.subscribe_in(
            &list.input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.search(window, cx);
                }
            },
        );
        let mut this = Self {
            team_id,
            list,
            hits: Vec::new(),
            searched: String::new(),
            server: None,
            max_height,
            last_requested_height: None,
            _subscription: subscription,
        };
        this.search(window, cx);
        this
    }

    /// Re-rank for the field's query: nothing on an empty one (the hint
    /// shows), the local rows at once, the server's spliced in once the
    /// keystrokes rest.
    fn search(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let query = self.list.query(cx);
        self.list.sync_query(&query);
        self.searched = query.clone();
        if query.is_empty() {
            self.hits.clear();
            self.server = None;
            cx.notify();
            return;
        }
        self.hits = search_list::local_issue_hits(&self.team_id, &query, cx);
        self.server = Some(search_list::server_issue_search(
            self.team_id.clone(),
            query,
            window,
            cx,
            |this: &mut Self, query, hits, cx| {
                if this.searched != query {
                    return;
                }
                let local = std::mem::take(&mut this.hits);
                this.hits = search_list::splice_server_hits(&this.team_id, local, &hits, cx);
                cx.notify();
            },
        ));
        cx.notify();
    }

    /// EXP-716: size the window to the query row + the rows (at most eight
    /// visible, each one search-list row tall) + the window's own chrome,
    /// capped at [`Self::max_height`]. Runs at render time; the resize itself
    /// is deferred and top-anchored, so the palette extends downward like a
    /// dropdown and shrinks back when a query narrows.
    fn fit_to_content(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let content = if self.hits.is_empty() {
            px(EMPTY_HEIGHT)
        } else {
            let visible = self.hits.len().min(search_list::MAX_VISIBLE_ROWS);
            px(visible as f32 * search_list::ROW_HEIGHT + ROWS_INSET)
        };
        let chrome = native_dialog::chromeless_vertical_chrome(Some(window));
        let target = (px(INPUT_ROW_HEIGHT) + content + chrome).min(self.max_height);
        let current = window.viewport_size();
        if (target - current.height).abs() <= px(1.) || self.last_requested_height == Some(target) {
            return;
        }
        self.last_requested_height = Some(target);
        native_dialog::resize_dialog_keeping_top(window, cx, size(current.width, target));
    }

    /// Click, Enter or Tab: navigate to the issue detail and close.
    fn open(&mut self, position: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(hit) = self.hits.get(position) else {
            return;
        };
        let issue_id = hit.issue_id.clone();
        native_dialog::close_then(window, cx, move |window, cx| {
            navigate(window, cx, Screen::IssueDetail { issue_id });
        });
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(position) = self.list.cursor(self.hits.len()) {
            self.open(position, window, cx);
        }
    }

    fn on_move_up(
        &mut self,
        _: &gpui_component::input::MoveUp,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.list.step(-1, self.hits.len());
        cx.stop_propagation();
        cx.notify();
    }

    fn on_move_down(
        &mut self,
        _: &gpui_component::input::MoveDown,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.list.step(1, self.hits.len());
        cx.stop_propagation();
        cx.notify();
    }

    fn on_enter(
        &mut self,
        _: &gpui_component::input::Enter,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_selected(window, cx);
        cx.stop_propagation();
    }

    fn on_tab(
        &mut self,
        _: &gpui_component::input::IndentInline,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_selected(window, cx);
        cx.stop_propagation();
    }

    fn on_escape(
        &mut self,
        _: &gpui_component::input::Escape,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        native_dialog::close_dialog_window(window, cx);
        cx.stop_propagation();
    }

    /// The pre-query hint (empty query) or the no-results line — the ×4 copy
    /// set (EXP-922), a FIXED height (EXP-716, see `EMPTY_HEIGHT`). EXP-1274:
    /// both lines are the shared LIST empty state; the hint's detail
    /// sentence rides under it.
    fn render_empty(&self, cx: &App) -> gpui::AnyElement {
        let empty = v_flex()
            .h(px(EMPTY_HEIGHT))
            .items_center()
            .justify_center()
            .px_12();
        if self.searched.is_empty() {
            empty
                .child(
                    crate::controls::empty_state_list(
                        Icon::new(registry::NAV_SEARCH),
                        SEARCH_EMPTY_HINT,
                        cx,
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                            .child(SEARCH_EMPTY_DETAIL),
                    ),
                )
                .into_any_element()
        } else {
            empty
                .child(crate::controls::empty_state_list(
                    Icon::new(registry::UI_EMPTY_SEARCH),
                    SEARCH_NO_RESULTS,
                    cx,
                ))
                .into_any_element()
        }
    }
}

impl Render for SearchSheetView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.fit_to_content(window, cx);
        let muted = cx.theme().muted_foreground;
        let selected = self.list.cursor(self.hits.len());
        let entity = cx.entity().downgrade();
        let rows: Vec<gpui::AnyElement> = self
            .hits
            .iter()
            .enumerate()
            .map(|(position, hit)| {
                let hover = entity.clone();
                let pick = entity.clone();
                search_list::row(("issue-hit", position), selected == Some(position), cx)
                    // EXP-892: hovering MOVES the selection — one highlight,
                    // and Enter always opens what the pointer is on.
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            let _ = hover.update(cx, |this, cx| {
                                if this.list.hover(position) {
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                        let _ = pick.update(cx, |this, cx| this.open(position, window, cx));
                    })
                    .child(search_list::issue_row_body(
                        Some(
                            crate::icons::resolved_status_icon(&hit.status, cx)
                                .xsmall()
                                .into_any_element(),
                        ),
                        hit.identifier.clone(),
                        hit.title.clone(),
                        None,
                        cx,
                    ))
                    .into_any_element()
            })
            .collect();
        let count = rows.len();
        let body = if rows.is_empty() {
            self.render_empty(cx)
        } else {
            div()
                .flex_1()
                .min_h_0()
                .p_1()
                .child(search_list::rows_viewport(
                    "search-sheet-rows",
                    rows,
                    count,
                    &self.list.scroll,
                    cx,
                ))
                .into_any_element()
        };
        v_flex()
            .size_full()
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_tab))
            .capture_action(cx.listener(Self::on_escape))
            .child(
                search_list::input_row(&self.list.input, (!self.searched.is_empty()).then_some(count), cx)
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
            .child(search_list::hairline(cx))
            .child(body)
    }
}
