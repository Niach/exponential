//! SLOP-28 (EXP-1259 D4/D5) — THE search list on the IDE, the desktop twin
//! of `packages/domain-contract/fixtures/search-list.json`: ONE component for
//! every place that searches a list and picks from it.
//!
//! Anatomy: an INPUT row ([`INPUT_HEIGHT`], the `nav-search` glyph, the
//! field, trailing the muted result COUNT once a query is typed) over a
//! hairline, then the ROWS — the Picker rows, [`ROW_HEIGHT`] each, no new
//! row type — in a viewport that shows at most [`MAX_VISIBLE_ROWS`] and
//! scrolls, with a [`FADE_HEIGHT`] bottom fade while more rows sit below.
//! The top row is preselected, ↑/↓ move, Enter or Tab pick, a hover selects.
//!
//! The pieces, smallest first: [`input_row`], [`row`], [`rows_viewport`],
//! assembled by [`SearchList`]; [`SearchListState`] is the keyboard model
//! (field + cursor + scroll) a host keeps across frames. Who mounts it:
//!
//! - the searchable [`super::Picker`] surface (every issue / label / board
//!   pick) builds its column from [`input_row`] + [`rows_viewport`];
//! - the `#` typeahead ([`IssueTypeahead`]) in every IDE editor that has
//!   one, hung off the caret ([`typeahead_popover`]);
//! - the composer "+" menu's Implement issue › and Run action › submenus
//!   (`chat_screen::plus_menu`);
//! - the ⌘K issue search (`search_sheet`).
//!
//! Results for issues are the ONE engine (`domain::issue_search`): the local
//! rank at the DEFAULT limit with the server's full-text hits spliced in
//! behind ([`local_issue_hits`], [`splice_server_hits`]). The editors' old
//! 6/8 typeahead caps are gone.

use std::collections::HashSet;
use std::time::Duration;

use gpui::{
    anchored, deferred, div, linear_color_stop, linear_gradient, point, prelude::FluentBuilder as _,
    px, AnyElement, App, AppContext as _, Context, Div, ElementId, Entity, EventEmitter,
    Focusable as _, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    ScrollHandle, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window,
};
use gpui_component::{
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;
use theme::tokens as t;

use crate::icons::registry;

/// The input row's height (`input.height`).
pub(crate) const INPUT_HEIGHT: f32 = 40.;
/// One row on a pointer (`row.height`; the composer menu's
/// `submenuRowHeight`).
pub(crate) const ROW_HEIGHT: f32 = 32.;
/// The most rows visible before the list scrolls (`maxVisibleRows`; the
/// composer menu's `submenuMaxRows`).
pub(crate) const MAX_VISIBLE_ROWS: usize = 8;
/// The bottom fade while more rows sit below (`fadeHeight`).
pub(crate) const FADE_HEIGHT: f32 = 24.;
/// The `#` typeahead popover's width on a pointer (`popoverWidth`).
pub(crate) const POPOVER_WIDTH: f32 = 560.;
/// The rows viewport's ceiling: exactly [`MAX_VISIBLE_ROWS`] rows.
pub(crate) const ROWS_MAX_HEIGHT: f32 = ROW_HEIGHT * MAX_VISIBLE_ROWS as f32;

/// The ×4 copy (web `ISSUE_SEARCH_PLACEHOLDER` / `ISSUE_SEARCH_NO_RESULTS`).
pub(crate) const ISSUE_SEARCH_PLACEHOLDER: &str = "Search issues";
pub(crate) const ISSUE_SEARCH_NO_RESULTS: &str = "No issues match";

/// How long a keystroke rests before the server's full-text pass fires.
const SERVER_SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

// ---------------------------------------------------------------------------
// The `#` trigger (pure)
// ---------------------------------------------------------------------------

/// A `#token` behind the caret that opens the typeahead: the `#`'s byte
/// offset and the token typed after it (what the field opens prefilled
/// with). The fixture's `typeaheadPrefill`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeaheadPrefill {
    pub(crate) start: usize,
    pub(crate) query: String,
}

impl TypeaheadPrefill {
    /// The token's byte length in the editor, `#` included — what a pick
    /// replaces.
    pub(crate) fn token_len(&self) -> usize {
        1 + self.query.len()
    }
}

/// The `#` rule (search-list.json): a `#` at the start of the text, after
/// whitespace or after `(` opens the list, and the token between it and the
/// caret (no whitespace in it) prefills the field. A `#` inside a word
/// (`C#sharp`) is not a trigger, and whitespace after the `#` ends the token.
pub(crate) fn typeahead_prefill(text: &str, caret: usize) -> Option<TypeaheadPrefill> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let before = &text[..caret];
    let token_start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(ix, c)| ix + c.len_utf8())
        .unwrap_or(0);
    let token = &before[token_start..];
    let start = if token.starts_with('#') {
        token_start
    } else if token.starts_with("(#") {
        token_start + 1
    } else {
        return None;
    };
    let query = &before[start + 1..];
    // `##` is a heading, not a reference.
    if query.contains('#') {
        return None;
    }
    Some(TypeaheadPrefill {
        start,
        query: query.to_string(),
    })
}

/// Whether the bottom fade paints: more rows than fit, and the viewport not
/// yet scrolled to its end. gpui's scroll offset is NEGATIVE as the content
/// moves up; an unmeasured viewport (`max_offset` still 0, the first frame)
/// counts as "at the top".
pub(crate) fn has_more_below(row_count: usize, offset_y: f32, max_offset_y: f32) -> bool {
    if row_count <= MAX_VISIBLE_ROWS {
        return false;
    }
    !(max_offset_y > 0. && -offset_y >= max_offset_y - 1.)
}

// ---------------------------------------------------------------------------
// The pieces
// ---------------------------------------------------------------------------

/// The INPUT row: the search glyph, the borderless field, and — once a query
/// is typed — the muted result count.
pub(crate) fn input_row(input: &Entity<InputState>, count: Option<usize>, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let typed = !input.read(cx).value().trim().is_empty();
    h_flex()
        .flex_shrink_0()
        .w_full()
        .h(px(INPUT_HEIGHT))
        .px_3()
        .gap_2()
        .items_center()
        .child(
            Icon::new(registry::NAV_SEARCH)
                .size(px(16.))
                .flex_shrink_0()
                .text_color(muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .child(Input::new(input).appearance(false).small().p_0()),
        )
        .when_some(count.filter(|_| typed), |row, count| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(muted)
                    .child(SharedString::from(count.to_string())),
            )
        })
}

/// The hairline between the input row and the rows.
pub(crate) fn hairline(cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .h(px(1.))
        .w_full()
        .bg(cx.theme().border.opacity(0.5))
}

/// One row SHELL: [`ROW_HEIGHT`] tall, the Picker row's padding, corner and
/// text rung, the list's active fill on the SELECTED row only (a hover moves
/// the selection; the host chains `.on_hover` + the pick). The body is the
/// host's child ([`issue_row_body`], [`super::picker_item_body`]).
pub(crate) fn row(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .w_full()
        .min_w_0()
        .h(px(ROW_HEIGHT))
        .px_2()
        .gap_2()
        .rounded(cx.theme().radius)
        .text_sm()
        .cursor_pointer()
        .when(selected, |row| row.bg(cx.theme().list_active))
}

/// An ISSUE row's body: the status glyph, the mono identifier, the title,
/// and an optional trailing part (a check, a muted note).
pub(crate) fn issue_row_body(
    status: Option<AnyElement>,
    identifier: impl Into<SharedString>,
    title: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_2p5()
        .children(status)
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(identifier.into()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme.foreground)
                .child(title.into()),
        )
        .children(trailing)
        .into_any_element()
}

/// An ACTION row's body: the action's icon and its NAME (no description —
/// composer-menu.json).
pub(crate) fn action_row_body(icon: Icon, name: impl Into<SharedString>, cx: &App) -> AnyElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(
            icon.size(px(14.))
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(cx.theme().foreground)
                .child(name.into()),
        )
        .into_any_element()
}

/// The trailing check of a picked row (the "+" menu's issue submenu).
pub(crate) fn row_check(cx: &App) -> AnyElement {
    Icon::new(registry::UI_CHECK)
        .size(px(14.))
        .flex_shrink_0()
        .text_color(cx.theme().foreground)
        .into_any_element()
}

/// The muted line an empty list reads as.
pub(crate) fn empty_row(message: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .w_full()
        .px_3()
        .py_3()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(message.into())
}

/// The rows viewport: at most [`MAX_VISIBLE_ROWS`] rows tall, scrolling, the
/// [`FADE_HEIGHT`] fade over its bottom while more rows sit below. `count`
/// is the number of ROWS among `children` (a trailing note is not one).
pub(crate) fn rows_viewport(
    id: impl Into<ElementId>,
    children: Vec<AnyElement>,
    count: usize,
    scroll: &ScrollHandle,
    cx: &App,
) -> Div {
    let more = has_more_below(
        count,
        f32::from(scroll.offset().y),
        f32::from(scroll.max_offset().y),
    );
    div()
        .relative()
        .w_full()
        .min_h_0()
        .flex_shrink(1.)
        .child(
            v_flex()
                .id(id)
                .w_full()
                .max_h(px(ROWS_MAX_HEIGHT))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .children(children),
        )
        .when(more, |viewport| viewport.child(bottom_fade(cx.theme().popover)))
}

/// The bottom fade: transparent to the surface fill, paint only.
fn bottom_fade(fill: Hsla) -> Div {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(FADE_HEIGHT))
        .bg(linear_gradient(
            180.,
            linear_color_stop(fill.opacity(0.), 0.),
            linear_color_stop(fill, 1.),
        ))
}

/// The floating surface a search list sits on outside a menu (the `#`
/// typeahead): the opaque popover fill under the card hairline.
pub(crate) fn surface(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .occlude()
        .overflow_hidden()
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .rounded(px(t::radius::MD))
        .shadow_md()
}

/// THE search list, assembled: input row, hairline, rows (or the empty
/// line), then an optional footer. The host owns where it sits (a popover,
/// a submenu, a dialog) and the keys ([`SearchListState`]).
///
/// A styleguide specimen is `SearchList::new("id", &input, &scroll)
/// .rows(rows, count).render(cx)` over static rows built with [`row`] +
/// [`issue_row_body`] / [`action_row_body`].
pub(crate) struct SearchList {
    id: ElementId,
    input: Entity<InputState>,
    scroll: ScrollHandle,
    count: Option<usize>,
    rows: Vec<AnyElement>,
    row_count: usize,
    empty: Option<AnyElement>,
    footer: Option<AnyElement>,
}

impl SearchList {
    pub(crate) fn new(id: impl Into<ElementId>, input: &Entity<InputState>, scroll: &ScrollHandle) -> Self {
        Self {
            id: id.into(),
            input: input.clone(),
            scroll: scroll.clone(),
            count: None,
            rows: Vec::new(),
            row_count: 0,
            empty: None,
            footer: None,
        }
    }

    /// The rows, and how many of them are rows (the count the input row
    /// shows once a query is typed; a trailing note is not counted).
    pub(crate) fn rows(mut self, rows: Vec<AnyElement>, row_count: usize) -> Self {
        self.rows = rows;
        self.row_count = row_count;
        self.count = Some(row_count);
        self
    }

    /// Override the count the input row shows (`None` = no count).
    pub(crate) fn count(mut self, count: Option<usize>) -> Self {
        self.count = count;
        self
    }

    /// What an empty list reads as.
    pub(crate) fn empty(mut self, empty: impl IntoElement) -> Self {
        self.empty = Some(empty.into_any_element());
        self
    }

    /// Something under the rows (the composer's Implement button).
    pub(crate) fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    pub(crate) fn render(self, cx: &App) -> Div {
        let SearchList {
            id,
            input,
            scroll,
            count,
            rows,
            row_count,
            empty,
            footer,
        } = self;
        let body: AnyElement = if rows.is_empty() {
            empty.unwrap_or_else(|| gpui::Empty.into_any_element())
        } else {
            div()
                .w_full()
                .p_1()
                .min_h_0()
                .flex_shrink(1.)
                .child(rows_viewport(id, rows, row_count, &scroll, cx))
                .into_any_element()
        };
        v_flex()
            .w_full()
            .min_h_0()
            .child(input_row(&input, count, cx))
            .child(hairline(cx))
            .child(body)
            .children(footer)
    }
}

// ---------------------------------------------------------------------------
// The keyboard model
// ---------------------------------------------------------------------------

/// What a host keeps across frames for one open search list: the field, the
/// keyboard cursor (a POSITION in the visible rows: the top row on every
/// open and every query change) and the viewport's scroll.
pub(crate) struct SearchListState {
    pub(crate) input: Entity<InputState>,
    pub(crate) scroll: ScrollHandle,
    cursor: usize,
    query: String,
    focus_claimed: bool,
}

impl SearchListState {
    pub(crate) fn new(placeholder: &'static str, window: &mut Window, cx: &mut App) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        Self {
            input,
            scroll: ScrollHandle::new(),
            cursor: 0,
            query: String::new(),
            focus_claimed: false,
        }
    }

    /// The trimmed query in the field.
    pub(crate) fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().trim().to_string()
    }

    /// The cursor clamped to `len` rows (`None` = nothing to pick).
    pub(crate) fn cursor(&self, len: usize) -> Option<usize> {
        (len > 0).then(|| self.cursor.min(len - 1))
    }

    /// A changed query puts the top row back under the cursor.
    pub(crate) fn sync_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.cursor = 0;
            self.scroll.set_offset(point(px(0.), px(0.)));
        }
    }

    /// ↑/↓: one step, wrapping at both ends, keeping the row in view.
    pub(crate) fn step(&mut self, delta: isize, len: usize) {
        let enabled = vec![true; len];
        if let Some(next) =
            crate::pickers::step_picker_selection(self.cursor.min(len.saturating_sub(1)), delta, &enabled)
        {
            self.cursor = next;
            self.scroll.scroll_to_item(next);
        }
    }

    /// A hover moves the cursor; `true` when it moved (a repaint is owed).
    pub(crate) fn hover(&mut self, position: usize) -> bool {
        std::mem::replace(&mut self.cursor, position) != position
    }

    /// `true` exactly once: the frame the field takes focus.
    pub(crate) fn claim_focus(&mut self) -> bool {
        !std::mem::replace(&mut self.focus_claimed, true)
    }

    /// Focus the field on the first frame it paints.
    pub(crate) fn focus_once(&mut self, window: &mut Window, cx: &mut App) {
        if self.claim_focus() {
            self.input.read(cx).focus_handle(cx).focus(window, cx);
        }
    }
}

// ---------------------------------------------------------------------------
// Issue results: the ONE engine + the server splice
// ---------------------------------------------------------------------------

/// One issue result, resolved for its row.
#[derive(Clone)]
pub(crate) struct IssueHit {
    pub(crate) issue_id: String,
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) status: domain::statuses::ResolvedStatus,
}

/// The local pass: the team's synced issues ranked by `domain::issue_search`
/// at the DEFAULT limit (an empty query lists the newest open work).
pub(crate) fn local_issue_hits(team_id: &str, query: &str, cx: &App) -> Vec<IssueHit> {
    let collections = Store::global(cx).collections();
    let statuses = crate::queries::team_statuses(cx, team_id);
    let issues = collections.issue_refs_in_team(team_id, cx);
    let rows: Vec<domain::issue_search::SearchRow<'_>> =
        issues.iter().map(|issue| (*issue).into()).collect();
    domain::issue_search::rank(&rows, query, domain::issue_search::DEFAULT_LIMIT, &HashSet::new())
        .into_iter()
        .map(|ix| {
            let issue = issues[ix];
            IssueHit {
                issue_id: issue.id.clone(),
                identifier: issue.identifier.clone(),
                title: issue.title.clone(),
                status: domain::statuses::resolve_status_sorted(issue, &statuses),
            }
        })
        .collect()
}

/// The server's full-text hits spliced in behind the local ones (deduped by
/// id, capped across both halves at the DEFAULT limit). The synced row wins
/// for rendering; a hit not synced yet renders from its own fields.
pub(crate) fn splice_server_hits(
    team_id: &str,
    local: Vec<IssueHit>,
    hits: &[api::issues::IssueSearchHit],
    cx: &App,
) -> Vec<IssueHit> {
    let collections = Store::global(cx).collections();
    let issues = collections.issues.read(cx);
    let statuses = crate::queries::team_statuses(cx, team_id);
    domain::issue_search::merge_server_hits(
        local,
        hits,
        domain::issue_search::DEFAULT_LIMIT,
        &HashSet::new(),
        |hit: &IssueHit| hit.issue_id.as_str(),
        |hit: &api::issues::IssueSearchHit| hit.id.as_str(),
        |hit: &api::issues::IssueSearchHit| {
            Some(match issues.get(&hit.id) {
                Some(issue) => IssueHit {
                    issue_id: issue.id.clone(),
                    identifier: issue.identifier.clone(),
                    title: issue.title.clone(),
                    status: domain::statuses::resolve_status_sorted(issue, &statuses),
                },
                None => IssueHit {
                    issue_id: hit.id.clone(),
                    identifier: hit.identifier.clone(),
                    title: hit.title.clone(),
                    status: crate::queries::resolve_status_ref(
                        hit.status_id.as_deref(),
                        hit.status,
                        &statuses,
                    ),
                },
            })
        },
    )
}

/// The debounced server pass for `query`, handed to `apply` on the entity
/// that asked (only while `query` is still its current one — the caller
/// checks). Dropping the task cancels it; replacing it per keystroke IS the
/// debounce.
pub(crate) fn server_issue_search<V: 'static>(
    team_id: String,
    query: String,
    window: &mut Window,
    cx: &mut Context<V>,
    apply: impl FnOnce(&mut V, &str, Vec<api::issues::IssueSearchHit>, &mut Context<V>) + 'static,
) -> Task<()> {
    let trpc = crate::queries::trpc_client(cx);
    cx.spawn_in(window, async move |this, window| {
        window.background_executor().timer(SERVER_SEARCH_DEBOUNCE).await;
        let Some(trpc) = trpc else {
            return;
        };
        let (search_team, search_query) = (team_id.clone(), query.clone());
        let result = window
            .background_executor()
            .spawn(async move {
                api::issues::search(
                    &trpc,
                    &search_team,
                    &search_query,
                    domain::issue_search::DEFAULT_LIMIT as u32,
                )
            })
            .await;
        match result {
            Ok(hits) => {
                let _ = this.update(window, |view, cx| apply(view, &query, hits, cx));
            }
            // Local-only degradation: the local rows already render.
            Err(error) => log::warn!("[ui] search list: issues.search failed: {error}"),
        }
    })
}

// ---------------------------------------------------------------------------
// The `#` typeahead
// ---------------------------------------------------------------------------

/// What the `#` typeahead tells its editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IssueTypeaheadEvent {
    /// Replace the `#token` with `#IDENT ` and put the caret after it.
    Pick(String),
    /// Escape (`delete_token = false`: the text stays as typed) or Backspace
    /// in the EMPTY field (`delete_token = true`: the `#token` goes too);
    /// focus returns to the editor either way.
    Close { delete_token: bool },
    /// Focus went somewhere else (a click outside): close and leave it there.
    Dismiss,
}

/// The caret line's left edge, top and bottom in window coordinates — where
/// the typeahead hangs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CaretAnchor {
    pub(crate) left: Pixels,
    pub(crate) top: Pixels,
    pub(crate) bottom: Pixels,
}

/// THE `#` typeahead: the search list in a popover at the caret, its OWN
/// field focused and prefilled with the token, spaces allowed, issues from
/// the ONE engine plus the server splice. The editor keeps the entity while
/// it is open, renders it with [`typeahead_popover`] and applies the
/// [`IssueTypeaheadEvent`]s it emits.
pub(crate) struct IssueTypeahead {
    team_id: String,
    /// The token in the editor's text this list was opened on.
    pub(crate) prefill: TypeaheadPrefill,
    list: SearchListState,
    hits: Vec<IssueHit>,
    /// The query `hits` answer.
    searched: String,
    server: Option<Task<()>>,
    _subscription: Subscription,
}

impl EventEmitter<IssueTypeaheadEvent> for IssueTypeahead {}

impl IssueTypeahead {
    pub(crate) fn new(
        team_id: String,
        prefill: TypeaheadPrefill,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = SearchListState::new(ISSUE_SEARCH_PLACEHOLDER, window, cx);
        let query = prefill.query.clone();
        list.input
            .update(cx, |input, cx| input.set_value(query, window, cx));
        let subscription = cx.subscribe_in(
            &list.input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => this.search(window, cx),
                InputEvent::Blur => cx.emit(IssueTypeaheadEvent::Dismiss),
                _ => {}
            },
        );
        let mut this = Self {
            team_id,
            prefill,
            list,
            hits: Vec::new(),
            searched: String::new(),
            server: None,
            _subscription: subscription,
        };
        this.search(window, cx);
        this
    }

    /// Re-rank for the field's current query: the local rows at once, the
    /// server's spliced in once the keystrokes rest.
    fn search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.list.query(cx);
        self.list.sync_query(&query);
        self.hits = local_issue_hits(&self.team_id, &query, cx);
        self.searched = query.clone();
        self.server = (!query.is_empty()).then(|| {
            server_issue_search(
                self.team_id.clone(),
                query,
                window,
                cx,
                |this: &mut Self, query, hits, cx| {
                    if this.searched != query {
                        return;
                    }
                    let local = std::mem::take(&mut this.hits);
                    this.hits = splice_server_hits(&this.team_id, local, &hits, cx);
                    cx.notify();
                },
            )
        });
        cx.notify();
    }

    fn pick(&mut self, position: usize, cx: &mut Context<Self>) {
        if let Some(hit) = self.hits.get(position) {
            cx.emit(IssueTypeaheadEvent::Pick(hit.identifier.clone()));
        }
    }

    fn pick_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(position) = self.list.cursor(self.hits.len()) {
            self.pick(position, cx);
        }
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.list.step(delta, self.hits.len());
        cx.notify();
    }
}

/// Mount an open [`IssueTypeahead`] at `anchor`: below the caret line, or
/// above it when the room below runs out (the EXP-970 placement rule), the
/// search list on the floating surface, [`POPOVER_WIDTH`] wide. The field
/// takes focus on the first frame.
pub(crate) fn typeahead_popover(
    typeahead: &Entity<IssueTypeahead>,
    anchor: CaretAnchor,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    use gpui_component::input::{Backspace, Enter, Escape, IndentInline, MoveDown, MoveUp};
    typeahead.update(cx, |this, cx| this.list.focus_once(window, cx));
    let (input, scroll, hits, selected) = {
        let this = typeahead.read(cx);
        (
            this.list.input.clone(),
            this.list.scroll.clone(),
            this.hits.clone(),
            this.list.cursor(this.hits.len()),
        )
    };
    let query_typed = !input.read(cx).value().trim().is_empty();
    let rows: Vec<AnyElement> = hits
        .iter()
        .enumerate()
        .map(|(position, hit)| {
            let hover_target = typeahead.clone();
            let pick_target = typeahead.clone();
            row(("issue-typeahead-row", position), selected == Some(position), cx)
                .on_hover(move |hovered, _, cx| {
                    if *hovered {
                        hover_target.update(cx, |this, cx| {
                            if this.list.hover(position) {
                                cx.notify();
                            }
                        });
                    }
                })
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    pick_target.update(cx, |this, cx| this.pick(position, cx));
                })
                .child(issue_row_body(
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
    let mut list = SearchList::new("issue-typeahead-rows", &input, &scroll).rows(rows, count);
    if query_typed {
        list = list.empty(empty_row(ISSUE_SEARCH_NO_RESULTS, cx));
    }

    let plan = crate::controls::plan_typeahead(
        f32::from(anchor.top),
        f32::from(anchor.bottom),
        f32::from(window.viewport_size().height),
    );
    let on = |f: fn(&mut IssueTypeahead, &mut Context<IssueTypeahead>)| {
        let target = typeahead.clone();
        move |cx: &mut App| target.update(cx, |this, cx| f(this, cx))
    };
    let up = on(|this, cx| this.step(-1, cx));
    let down = on(|this, cx| this.step(1, cx));
    let enter = on(|this, cx| this.pick_selected(cx));
    let tab = on(|this, cx| this.pick_selected(cx));
    let escape = on(|_, cx| cx.emit(IssueTypeaheadEvent::Close { delete_token: false }));
    let erase = on(|_, cx| cx.emit(IssueTypeaheadEvent::Close { delete_token: true }));
    let dismiss = on(|_, cx| cx.emit(IssueTypeaheadEvent::Dismiss));
    let erase_input = input.clone();
    let panel = surface(cx)
        .id("issue-typeahead")
        .w(px(POPOVER_WIDTH))
        .max_h(px(plan.max_h.max(INPUT_HEIGHT + ROW_HEIGHT * 3.)))
        .capture_action(move |_: &MoveUp, _, cx| {
            up(cx);
            cx.stop_propagation();
        })
        .capture_action(move |_: &MoveDown, _, cx| {
            down(cx);
            cx.stop_propagation();
        })
        .capture_action(move |action: &Enter, _, cx| {
            // Shift/cmd+Enter belong to whoever is underneath.
            if action.shift || action.secondary {
                return;
            }
            enter(cx);
            cx.stop_propagation();
        })
        .capture_action(move |_: &IndentInline, _, cx| {
            tab(cx);
            cx.stop_propagation();
        })
        .capture_action(move |_: &Escape, _, cx| {
            escape(cx);
            cx.stop_propagation();
        })
        .capture_action(move |_: &Backspace, _, cx| {
            // Deleting past the empty field deletes the `#` (and closes).
            if erase_input.read(cx).value().is_empty() {
                erase(cx);
                cx.stop_propagation();
            }
        })
        .on_mouse_down_out(move |_, _, cx| dismiss(cx))
        .child(list.render(cx));

    let (corner, position) = match plan.placement {
        crate::controls::TypeaheadPlacement::Below => (
            gpui::Anchor::TopLeft,
            point(anchor.left, anchor.bottom + px(TYPEAHEAD_GAP)),
        ),
        crate::controls::TypeaheadPlacement::Above => (
            gpui::Anchor::BottomLeft,
            point(anchor.left, anchor.top - px(TYPEAHEAD_GAP)),
        ),
    };
    deferred(
        anchored()
            .anchor(corner)
            .position(position)
            .snap_to_window_with_margin(px(8.))
            .child(panel),
    )
    .with_priority(200)
    .into_any_element()
}

/// The gap between the caret line and the popover (the EXP-970 menu's).
const TYPEAHEAD_GAP: f32 = 4.;

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../../../../packages/domain-contract/fixtures/search-list.json");
    const COMPOSER_MENU: &str =
        include_str!("../../../../../../packages/domain-contract/fixtures/composer-menu.json");

    /// search-list.json's numbers ARE this module's constants, and the
    /// composer menu's submenus take the same row height and row cap.
    #[test]
    fn the_constants_mirror_the_search_list_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture parses");
        assert_eq!(fixture["input"]["height"], INPUT_HEIGHT as u64);
        assert_eq!(fixture["input"]["icon"], "nav-search");
        assert_eq!(fixture["row"]["height"], ROW_HEIGHT as u64);
        assert_eq!(fixture["maxVisibleRows"], MAX_VISIBLE_ROWS as u64);
        assert_eq!(fixture["fadeHeight"], FADE_HEIGHT as u64);
        assert_eq!(fixture["popoverWidth"], POPOVER_WIDTH as u64);
        use gpui_component::IconNamed as _;
        assert_eq!(
            crate::icons::registry::concept_by_name("nav-search").map(|icon| icon.path()),
            Some(registry::NAV_SEARCH.path()),
        );
        let menu: serde_json::Value = serde_json::from_str(COMPOSER_MENU).expect("fixture parses");
        assert_eq!(menu["submenuRowHeight"], ROW_HEIGHT as u64);
        assert_eq!(menu["submenuMaxRows"], MAX_VISIBLE_ROWS as u64);
    }

    /// Every `typeaheadPrefill` case, replayed by name.
    #[test]
    fn typeahead_prefill_replays_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture parses");
        let cases = fixture["typeaheadPrefill"].as_array().expect("cases");
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().expect("name");
            let text = case["text"].as_str().expect("text");
            let caret = case["caret"].as_u64().expect("caret") as usize;
            let expected = match &case["expected"] {
                serde_json::Value::Null => None,
                expected => Some(TypeaheadPrefill {
                    start: expected["start"].as_u64().expect("start") as usize,
                    query: expected["query"].as_str().expect("query").to_string(),
                }),
            };
            assert_eq!(typeahead_prefill(text, caret), expected, "case: {name}");
        }
    }

    #[test]
    fn the_prefill_is_safe_on_odd_input() {
        // A caret inside a multibyte char, or past the end, is no token.
        assert_eq!(typeahead_prefill("#é", 2), None);
        assert_eq!(typeahead_prefill("#", 5), None);
        // `##` is a heading, never a reference.
        assert_eq!(typeahead_prefill("##", 2), None);
        // The token after a newline starts the line.
        assert_eq!(
            typeahead_prefill("a\n#x", 4),
            Some(TypeaheadPrefill {
                start: 2,
                query: "x".to_string()
            })
        );
        // What a pick replaces: the `#` plus the token.
        assert_eq!(typeahead_prefill("see #ab", 7).expect("token").token_len(), 3);
    }

    #[test]
    fn the_fade_shows_only_while_rows_sit_below() {
        // Eight rows fit: never a fade.
        assert!(!has_more_below(MAX_VISIBLE_ROWS, 0., 0.));
        // Nine, unmeasured (the first frame): the fade.
        assert!(has_more_below(MAX_VISIBLE_ROWS + 1, 0., 0.));
        // Measured, at the top and midway: the fade.
        assert!(has_more_below(20, 0., 300.));
        assert!(has_more_below(20, -150., 300.));
        // Scrolled to the end: gone.
        assert!(!has_more_below(20, -300., 300.));
    }

    #[test]
    fn the_cursor_resets_on_a_new_query_and_wraps() {
        let mut cursor = 3usize;
        // The keyboard rule `SearchListState::step` rides (wrapping both
        // ends), replayed on a bare cursor.
        let enabled = vec![true; 4];
        cursor = crate::pickers::step_picker_selection(cursor, 1, &enabled).expect("a row");
        assert_eq!(cursor, 0, "Down on the last row wraps to the top");
        cursor = crate::pickers::step_picker_selection(cursor, -1, &enabled).expect("a row");
        assert_eq!(cursor, 3, "Up on the top row wraps to the last");
        assert_eq!(ROWS_MAX_HEIGHT, 256.);
    }
}
