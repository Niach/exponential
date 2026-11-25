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
//!
//! #EDE-99 (search-list.json `chats`, the spec ×4; `domain::search_list` the
//! rules): with the chat store on this desktop and a query typed, the list
//! becomes Sections, `Issues` then `Chats`: the store's `search_all` on a
//! background task (debounced, a newer query drops the older answer), the
//! active team's roots only, the previous rows + the band spinner while a
//! newer query runs. A chat pick opens its session with find in chat on the
//! query, starting at the hit ([`crate::steer_viewer::request_find_at`]).
//!
//! #NEW-133 (search-list.json SUGGESTION; `domain::search_list::suggestion`):
//! a sentence the phrase grammar recognises puts ONE action row
//! `Search: <query>` on top (above the rows and any section header, never
//! counted, instead of the empty state); picking it rewrites the field.
//!
//! #NEW-136 (search-list.json TRANSLATE; issue-search-translation.json): a
//! sentence the grammar does NOT recognise (4+ words, no filter token) puts
//! the same row there named `Translate to a search`, only while this device
//! has a signed-in claude login. Picking it runs ONE print-mode call of that
//! CLI (`coding::search_translate`, never while typing); the sanitized answer
//! rewrites the field, anything else is the failed-action toast.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chat_store::sqlite::SqliteStore;
use domain::search_list::{
    self as global, GlobalChat, GlobalChatHit, GlobalChatRoot, GlobalChatRow, GlobalChatRowsInput, GlobalChatsState,
    GlobalSectionKey, GlobalSections,
};
use gpui::{
    div, prelude::FluentBuilder as _, px, size, App, AppContext as _, InteractiveElement as _,
    StatefulInteractiveElement as _,
    IntoElement, KeyBinding, ParentElement, Pixels, Render, Styled, Task, Window,
};
use gpui_component::{
    input::InputEvent,
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::{SessionPhase, Store};

use crate::native_dialog::{self, DialogContent, DialogSpec};

use crate::actions::OpenSearch;
use crate::icons::registry;
use crate::chats_view::row_team;
use crate::user_avatar::{with_team_mark, RowTeam};
use crate::navigation::{screen_team_id, nav_for_window, navigate, Screen};
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
const SEARCH_EMPTY_HINT: &str = "Search issues across all your boards.";
const SEARCH_EMPTY_DETAIL: &str =
    "Matches identifiers, titles and full text. Filter with by:, is:, status:, created: and updated:.";
const SEARCH_NO_RESULTS: &str = "No issues match";
/// #NEW-136: the model fallback's row and its failure toast (search-list.json
/// `suggestion.translate`; the same gate greps these).
const SEARCH_TRANSLATE_LABEL: &str = "Translate to a search";
const SEARCH_TRANSLATE_FAILED: &str = "Couldn't translate this search";

/// #EDE-99: a section header's height (the band + its gap).
const SECTION_HEADER_HEIGHT: f32 = 32.;

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
    // ⌘K (command-palette) is the issue search everywhere; ⌘F is find in
    // chat while a chat screen is open, else the same search (§8.13).
    #[cfg(target_os = "macos")]
    cx.bind_keys([KeyBinding::new("cmd-k", OpenSearch, None)]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("ctrl-k", OpenSearch, None)]);
    // #EDE-129: a session view handles Ctrl/Cmd+F itself (focus inside it).
    // Unhandled (#EXP-2021: nothing focused, e.g. right after launch) it
    // lands here: a chat screen opens its bar, any other the search.
    cx.bind_keys(crate::steer_viewer::find_key_bindings());
    cx.on_action(|_: &crate::steer_viewer::FindInChat, cx| {
        crate::navigation::on_active_window(cx, |window, cx| {
            let nav = crate::navigation::nav_for_window(window, cx);
            if nav.read(cx).screen().is_some_and(crate::steer_viewer::screen_has_find) {
                crate::steer_viewer::request_find(None, window, cx);
            } else {
                open_search(window, cx);
            }
        });
    });
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
    let Some(team_id) = screen_team_id(&nav, cx) else {
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
    // #EDE-99: the chat store of this desktop (none = today's list).
    let store = crate::chat_store::store(cx);
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| SearchSheetView::new(team_id, store, max_height, window, cx));
        DialogContent::new(view).padless()
    });
}

/// #EDE-99: one Chats row and the store's hit it opens.
struct ChatHitRow {
    row: GlobalChatRow,
    hit: GlobalChatHit,
}

/// One row of the list, in display order (issues, then chats).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    /// #NEW-133: the `Search: <query>` row, or #NEW-136's `Translate to a
    /// search` in its place (always the top one, never both).
    Suggestion,
    Issue(usize),
    Chat(usize),
}

/// The top action row of the typed text.
enum Lead {
    /// #NEW-133: a recognised sentence.
    Search(global::SearchSuggestion),
    /// #NEW-136: an unrecognised one the model may translate; `busy` while
    /// the call runs.
    Translate { busy: bool },
}

/// The top row for the typed text: the phrase grammar over the synced users
/// (the rows that feed the creator names) at the clock (#NEW-133), else the
/// model fallback's offer while this device has a login for it (#NEW-136).
fn lead_for(query: &str, cx: &mut App) -> Option<Lead> {
    let (suggestion, wanted) = with_members(cx, |members| {
        let now = domain::issue_search::now_ms();
        let suggestion = global::suggestion(query, members, now);
        let wanted = suggestion.is_none() && domain::issue_search_translation::wants(query, members, now);
        (suggestion, wanted)
    });
    match suggestion {
        Some(suggestion) => Some(Lead::Search(suggestion)),
        None => (wanted && translator_for(cx).is_some()).then_some(Lead::Translate { busy: false }),
    }
}

/// The grammar's members: the synced users.
fn with_members<T>(cx: &App, run: impl FnOnce(&[domain::issue_search_phrase::PhraseMember<'_>]) -> T) -> T {
    let users = Store::global(cx).collections().users.read(cx);
    let members: Vec<domain::issue_search_phrase::PhraseMember<'_>> = users
        .iter()
        .map(|user| domain::issue_search_phrase::PhraseMember {
            name: user.name.as_deref(),
            email: user.email.as_deref(),
        })
        .collect();
    run(&members)
}

/// #NEW-136: the signed-in claude login a translate call runs on, from the
/// app's cached doctor report (`None` = no row).
fn translator_for(cx: &mut App) -> Option<coding::search_translate::Translator> {
    let hub = crate::coding_flow::CodingHub::global(cx);
    let hub = hub.read(cx);
    let report = hub.doctor.report.as_ref()?;
    coding::search_translate::Translator::resolve(report, &hub.settings, &crate::coding_flow::coding_data_dir(cx))
}

/// The Chats rows of a store answer (`globalChatRows`): the active team's
/// roots (the Chats list's titles, agent conversations = the agent icon).
fn chat_rows(team_id: &str, query: &str, answer: Vec<chat_store::ChatRow>, cx: &mut App) -> Vec<ChatHitRow> {
    // #EXP-2027: every member team's chats.
    let _ = team_id;
    let model = crate::session_data::chats_model_all(cx);
    let mut roots: HashMap<String, GlobalChatRoot> = HashMap::new();
    for row in model.list.rows.iter().chain(model.archived.rows.iter()) {
        roots.insert(row.session_id.clone(), GlobalChatRoot { title: row.title.clone(), agent: false });
    }
    for row in &model.app_rows {
        if let Some(id) = &row.session_id {
            roots.insert(id.clone(), GlobalChatRoot { title: row.name.clone(), agent: false });
        }
    }
    for row in &model.agent_rows {
        if let Some(id) = &row.session_id {
            roots.insert(id.clone(), GlobalChatRoot { title: row.name.clone(), agent: true });
        }
    }
    let chats: Vec<GlobalChat> = answer
        .iter()
        .filter_map(|chat| serde_json::to_value(chat).ok().and_then(|value| serde_json::from_value(value).ok()))
        .collect();
    let authors: HashMap<String, String> = {
        let messages = Store::global(cx).collections().messages.read(cx);
        chats
            .iter()
            .filter_map(|chat| chat.hit.message_id.as_deref())
            .filter_map(|id| Some((id.to_string(), messages.get(id)?.author_kind.clone()?)))
            .collect()
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let offset = chrono::Local::now().offset().local_minus_utc() / 60;
    global::global_chat_rows(GlobalChatRowsInput {
        query,
        now: &now,
        utc_offset_minutes: offset,
        chats: &chats,
        roots: &roots,
        message_authors: &authors,
        limit: global::chats().chats_max,
    })
    .into_iter()
    .filter_map(|row| {
        let hit = chats.iter().find(|chat| chat.root_id == row.root_id)?.hit.clone();
        Some(ChatHitRow { row, hit })
    })
    .collect()
}

/// Window-content wrapper: the web `IssueSearchSheet` shell — THE search
/// list's input row (with an in-row ✕, the mouse dismissal a chromeless
/// window needs, EXP-415) over its rows. Owns the keys.
struct SearchSheetView {
    team_id: String,
    /// #EXP-2027: every member team (search spans them all).
    teams: Vec<String>,
    list: SearchListState,
    hits: Vec<IssueHit>,
    /// The query `hits` answer.
    searched: String,
    /// #NEW-133 + #NEW-136: the top action row of `searched` (`None` = none).
    lead: Option<Lead>,
    /// #NEW-136: the running translate call; dropping it cancels the call
    /// (a new text, the sheet closing).
    translating: Option<(coding::search_translate::CancelOnDrop, Task<()>)>,
    server: Option<Task<()>>,
    /// #EDE-99: this desktop's chat store (`None` = no Chats section).
    store: Option<Arc<SqliteStore>>,
    /// The Chats rows of the last answer (kept while a newer query runs).
    chats: Vec<ChatHitRow>,
    /// The store is answering a newer query.
    chats_loading: bool,
    chats_task: Option<Task<()>>,
    /// EXP-716: the height cap the window grows toward — web `max-h-[60vh]`,
    /// computed from the OPENER's viewport at open time.
    max_height: Pixels,
    /// The last window height `fit_to_content` asked for, so an in-flight
    /// resize (the viewport follows a frame later) is not re-requested.
    last_requested_height: Option<Pixels>,
    _subscription: gpui::Subscription,
    /// #NEW-136: the doctor's report (the claude login) landing or changing
    /// re-reads the top row.
    _doctor: gpui::Subscription,
}

impl SearchSheetView {
    fn new(
        team_id: String,
        store: Option<Arc<SqliteStore>>,
        max_height: Pixels,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let placeholder: &'static str =
            if store.is_some() { global::copy().placeholder.as_str() } else { SEARCH_PLACEHOLDER };
        let list = SearchListState::new(placeholder, window, cx);
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
        let hub = crate::coding_flow::CodingHub::global(cx);
        let doctor = cx.observe(&hub, |this: &mut Self, _, cx| {
            if this.translating.is_some() {
                return;
            }
            let had = this.lead.is_some();
            let searched = this.searched.clone();
            this.lead = lead_for(&searched, cx);
            if had != this.lead.is_some() {
                cx.notify();
            }
        });
        // DEV-ONLY (the EXP_DEV_* family): `EXP_DEV_SEARCH_PICK=<ms>` picks
        // the top row that long after the palette opened, as Enter would.
        if let Some(delay) = std::env::var("EXP_DEV_SEARCH_PICK").ok().and_then(|ms| ms.trim().parse::<u64>().ok()) {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(delay)).await;
                let _ = this.update_in(cx, |this, window, cx| this.open_selected(window, cx));
            })
            .detach();
        }
        let mut this = Self {
            team_id,
            teams: Store::global(cx).collections().teams_sorted(cx).into_iter().map(|team| team.id).collect(),
            list,
            hits: Vec::new(),
            searched: String::new(),
            lead: None,
            translating: None,
            server: None,
            store,
            chats: Vec::new(),
            chats_loading: false,
            chats_task: None,
            max_height,
            last_requested_height: None,
            _subscription: subscription,
            _doctor: doctor,
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
        // #NEW-136: a changed text ends a running translate call.
        self.translating = None;
        self.lead = lead_for(&query, cx);
        if query.is_empty() {
            self.hits.clear();
            self.server = None;
            self.chats.clear();
            self.chats_loading = false;
            self.chats_task = None;
            cx.notify();
            return;
        }
        self.hits = search_list::local_issue_hits_teams(&self.teams, &query, cx);
        self.search_chats(query.clone(), cx);
        self.server = Some(search_list::server_issue_search_teams(
            self.teams.clone(),
            query,
            window,
            cx,
            |this: &mut Self, query, hits, cx| {
                if this.searched != query {
                    return;
                }
                let key = this.selected_key();
                let local = std::mem::take(&mut this.hits);
                this.hits = search_list::splice_server_hits_teams(&this.teams, local, &hits, cx);
                this.restore_key(key);
                cx.notify();
            },
        ));
        cx.notify();
    }

    /// #EDE-99: the store's `search_all` for `query`, `debounceMs` after the
    /// last keystroke, off the main thread; the previous rows stay (with the
    /// band spinner) until the answer replaces them, a newer query drops it.
    fn search_chats(&mut self, query: String, cx: &mut gpui::Context<Self>) {
        let Some(store) = self.store.clone() else { return };
        self.chats_loading = true;
        let asked = domain::find_in_chat::query(&query);
        self.chats_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(global::chats().debounce_ms)).await;
            let answer = cx
                .background_executor()
                .spawn(async move { store.search_all(&asked, domain::find_in_chat::hits_max()).ok() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.searched != query {
                    return;
                }
                let key = this.selected_key();
                let answer = answer.map(|answer| answer.chats).unwrap_or_default();
                this.chats = chat_rows(&this.team_id, &query, answer, cx);
                this.chats_loading = false;
                this.restore_key(key);
                cx.notify();
            });
        }));
    }

    /// The sections (`globalSections`).
    fn sections(&self) -> GlobalSections {
        global::global_sections(
            &self.searched,
            self.hits.len(),
            self.store.as_ref().map(|_| GlobalChatsState { rows: self.chats.len(), loading: self.chats_loading }),
        )
    }

    /// The rows in display order (headers are not rows).
    fn entries(&self) -> Vec<Entry> {
        self.lead
            .iter()
            .map(|_| Entry::Suggestion)
            .chain((0..self.hits.len()).map(Entry::Issue))
            .chain((0..self.chats.len()).map(Entry::Chat))
            .collect()
    }

    /// How many rows lead the results (the suggestion row: 1 or 0).
    fn lead(&self) -> usize {
        usize::from(self.lead.is_some())
    }

    fn entry_key(&self, entry: Entry) -> Option<String> {
        match entry {
            Entry::Suggestion => Some("suggestion".to_string()),
            Entry::Issue(ix) => self.hits.get(ix).map(|hit| format!("issue:{}", hit.issue_id)),
            Entry::Chat(ix) => self.chats.get(ix).map(|chat| chat.row.key.clone()),
        }
    }

    fn selected_key(&self) -> Option<String> {
        let entries = self.entries();
        let position = self.list.cursor(entries.len())?;
        self.entry_key(entries[position])
    }

    /// The selected row keeps its key while answers arrive, else the top row.
    fn restore_key(&mut self, key: Option<String>) {
        let entries = self.entries();
        let position = key
            .and_then(|key| entries.iter().position(|entry| self.entry_key(*entry).as_deref() == Some(key.as_str())))
            .unwrap_or(0);
        self.list.hover(position);
    }

    /// The list child a row position sits at (the section headers count).
    fn child_index(&self, position: usize) -> usize {
        let lead = self.lead();
        if !self.sections().titled || position < lead {
            return position;
        }
        let issues_header = usize::from(!self.hits.is_empty());
        if position - lead < self.hits.len() {
            position + issues_header
        } else {
            position + issues_header + 1
        }
    }

    /// EXP-716: size the window to the query row + the rows (at most eight
    /// visible, each one search-list row tall) + the window's own chrome,
    /// capped at [`Self::max_height`]. Runs at render time; the resize itself
    /// is deferred and top-anchored, so the palette extends downward like a
    /// dropdown and shrinks back when a query narrows.
    fn fit_to_content(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let sections = self.sections();
        let lead = self.lead() as f32 * search_list::ROW_HEIGHT;
        let content = if sections.titled && !sections.sections.is_empty() {
            let rows: f32 = sections
                .sections
                .iter()
                .map(|section| match section.key {
                    GlobalSectionKey::Issues => section.count as f32 * search_list::ROW_HEIGHT,
                    GlobalSectionKey::Chats => section.count as f32 * global::chats().row.chat_height,
                })
                .sum();
            px(lead + rows + sections.sections.len() as f32 * SECTION_HEADER_HEIGHT + ROWS_INSET)
        } else if self.hits.is_empty() && self.lead.is_none() {
            px(EMPTY_HEIGHT)
        } else {
            // The suggestion row is a row of the list (never the empty state).
            let visible = (self.hits.len() + self.lead()).min(search_list::MAX_VISIBLE_ROWS);
            px(visible as f32 * search_list::ROW_HEIGHT + ROWS_INSET)
        };
        let chrome = native_dialog::chromeless_vertical_chrome(Some(window));
        let target = (px(INPUT_ROW_HEIGHT) + content + chrome).min(self.max_height);
        let Some(current) = native_dialog::dialog_size(window, cx) else {
            return;
        };
        if (target - current.height).abs() <= px(1.) || self.last_requested_height == Some(target) {
            return;
        }
        self.last_requested_height = Some(target);
        native_dialog::resize_dialog_keeping_top(window, cx, size(current.width, target));
    }

    /// Click, Enter or Tab: navigate to the issue detail and close.
    /// #EDE-99: a chat row opens the hit's session (root or child) with
    /// find in chat on the query, starting at the hit (`globalChatPick`).
    fn open(&mut self, position: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match self.entries().get(position).copied() {
            // #NEW-133: the suggestion rewrites the field; the focus stays
            // in it, the search re-runs, nothing navigates.
            Some(Entry::Suggestion) => match &self.lead {
                Some(Lead::Search(suggestion)) => {
                    let query = suggestion.query.clone();
                    self.rewrite(query, window, cx);
                }
                // #NEW-136: the one place a model is called; a second pick
                // while it runs does nothing.
                Some(Lead::Translate { busy: false }) => self.translate(window, cx),
                Some(Lead::Translate { busy: true }) | None => {}
            },
            Some(Entry::Issue(ix)) => {
                let Some(hit) = self.hits.get(ix) else { return };
                let issue_id = hit.issue_id.clone();
                native_dialog::close_then(window, cx, move |window, cx| {
                    navigate(window, cx, Screen::IssueDetail { issue_id });
                });
            }
            Some(Entry::Chat(ix)) => {
                let Some(chat) = self.chats.get(ix) else { return };
                let pick = global::global_chat_pick(&self.searched, &chat.hit);
                native_dialog::close_then(window, cx, move |window, cx| {
                    let session_id = pick.session_id.clone();
                    crate::steer_viewer::request_find_at(pick.session_id, pick.find.query, pick.find.target, window, cx);
                    crate::navigation::open_session_link(&session_id, window, cx);
                });
            }
            None => {}
        }
    }

    /// Replace the field's text with `query`: the focus stays in the field
    /// and the search re-runs.
    fn rewrite(&mut self, query: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.list.input.update(cx, |state, cx| {
            state.set_value(query, window, cx);
            state.focus(window, cx);
        });
        self.search(window, cx);
        self.list.hover(0);
    }

    /// #NEW-136: translate the typed sentence with ONE call on the user's own
    /// claude login, off the UI thread. The answer goes through the sanitizer
    /// (never trusted); no query = the failed-action toast, the text stays.
    fn translate(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(translator) = translator_for(cx) else { return };
        let text = self.searched.clone();
        let prompt = with_members(cx, |members| {
            domain::issue_search_translation::prompt(&text, members, domain::issue_search::now_ms())
        });
        let cancel = coding::search_translate::Cancel::new();
        let stop = cancel.clone();
        self.lead = Some(Lead::Translate { busy: true });
        let task = cx.spawn_in(window, async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { coding::search_translate::run(&translator, &prompt, &stop) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.searched != text || !matches!(this.lead, Some(Lead::Translate { busy: true })) {
                    return;
                }
                // The call is over: let its handle go without cancelling this task.
                if let Some((_, task)) = this.translating.take() {
                    task.detach();
                }
                let query = answer.and_then(|answer| {
                    with_members(cx, |members| {
                        domain::issue_search_translation::sanitize(&answer, &text, members, domain::issue_search::now_ms())
                    })
                });
                match query {
                    Some(query) => this.rewrite(query, window, cx),
                    None => {
                        this.lead = Some(Lead::Translate { busy: false });
                        crate::toast::show(crate::toast::Toast::error(SEARCH_TRANSLATE_FAILED), window, cx);
                        cx.notify();
                    }
                }
            });
        });
        self.translating = Some((coding::search_translate::CancelOnDrop(cancel), task));
        cx.notify();
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(position) = self.list.cursor(self.entries().len()) {
            self.open(position, window, cx);
        }
    }

    /// ↑/↓ across both sections (the headers are skipped).
    fn step(&mut self, delta: isize) {
        let len = self.entries().len();
        self.list.step(delta, len);
        if let Some(position) = self.list.cursor(len) {
            self.list.scroll.scroll_to_item(self.child_index(position));
        }
    }

    fn on_move_up(
        &mut self,
        _: &gpui_component::input::MoveUp,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.step(-1);
        cx.stop_propagation();
        cx.notify();
    }

    fn on_move_down(
        &mut self,
        _: &gpui_component::input::MoveDown,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.step(1);
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
        let sections = self.sections();
        if sections.titled {
            // #EDE-99: `No results` once both are done with nothing; blank
            // while the store still answers (never a skeleton).
            return if sections.empty {
                empty
                    .child(crate::controls::empty_state_list(
                        Icon::new(registry::UI_EMPTY_SEARCH),
                        global::copy().no_results.clone(),
                        cx,
                    ))
                    .into_any_element()
            } else {
                empty.into_any_element()
            };
        }
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

/// #EDE-99: a section's header = THE Section band (genUI `Section`, web
/// `GlassSectionHeader`): `copy.issues` / `copy.chats`, the reload spinner
/// while the store answers a newer query, then the muted count.
fn section_header(id: &str, label: &str, count: usize, loading: bool, cx: &App) -> gpui::AnyElement {
    let count = div().text_xs().text_color(cx.theme().muted_foreground).child(count.to_string()).into_any_element();
    eui_gpui::controls::glass_section_band_loading(None, label.to_string(), loading, Some(count), cx)
        .id(gpui::SharedString::from(id.to_string()))
        .flex_shrink_0()
        .into_any_element()
}

/// A chat hit's team.
fn root_team(root_id: &str, cx: &App) -> Option<String> {
    crate::session_data::session(root_id, cx)?.team_id
}

/// #EDE-99: a Chats row's body = the Picker row's (web `PickerItemBody`,
/// [`crate::picker::picker_item_body`]): the concept icon (its team's mark on it), the root's title
/// over the hit's snippet (its matches painted by the genUI highlight,
/// text-highlight.json: every match, no current), the muted date trailing.
fn chat_row_body(row: &GlobalChatRow, team: Option<&RowTeam>, cx: &App) -> gpui::AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let icon = registry::concept_by_name(&row.icon).unwrap_or(registry::SEARCH_CHAT);
    let ui_theme = eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID);
    let colors = eui_gpui::paint::highlight::MarkColors::of(ui_theme.as_deref(), eui_core::theme::Mode::Dark);
    let spec = eui_core::text_highlight::HighlightSpec { query: row.highlight.clone(), current: None, before: None };
    let marks = eui_gpui::paint::highlight::text_marks(&row.subtitle, &spec, colors);
    let text = gpui::SharedString::from(row.subtitle.clone());
    let styled = gpui::StyledText::new(text.clone());
    let layout = styled.layout().clone();
    let snippet = eui_gpui::paint::highlight::marked(styled, layout, text.len(), eui_gpui::paint::highlight::plain_fills(&marks));
    h_flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(with_team_mark(&row.key, Icon::new(icon).size(px(14.)).flex_shrink_0().text_color(muted), 14., team, theme.popover, cx))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().w_full().whitespace_nowrap().overflow_hidden().text_ellipsis().text_color(theme.foreground).child(row.title.clone()))
                .child(
                    div()
                        .id(gpui::SharedString::from(format!("{}-snippet", row.key)))
                        .w_full()
                        .text_xs()
                        .text_color(muted)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(snippet),
                ),
        )
        .child(div().flex_shrink_0().text_xs().text_color(muted).child(row.trailing.clone()))
        .into_any_element()
}

impl Render for SearchSheetView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SearchSheetView").entered();
        self.fit_to_content(window, cx);
        let entries = self.entries();
        let selected = self.list.cursor(entries.len());
        let sections = self.sections();
        let entity = cx.entity().downgrade();
        let shell = |id: gpui::ElementId, position: usize, cx: &App| {
            let hover = entity.clone();
            let pick = entity.clone();
            search_list::row(id, selected == Some(position), cx)
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
        };
        let lead = self.lead();
        // #NEW-133: the suggestion = an ACTION row (icon by concept + name);
        // #NEW-136: the same row named `Translate to a search`, THE inline
        // spinner trailing while its call runs.
        let suggestion_row = self.lead.as_ref().map(|lead| {
            let spec = global::suggestion_spec();
            let icon = registry::concept_by_name(&spec.icon).unwrap_or(registry::NAV_SEARCH);
            let (test_id, label, busy) = match lead {
                Lead::Search(suggestion) => (spec.test_id.clone(), suggestion.label.clone(), false),
                Lead::Translate { busy } => (spec.translate.test_id.clone(), SEARCH_TRANSLATE_LABEL.to_string(), *busy),
            };
            shell(gpui::SharedString::from(test_id).into(), 0, cx)
                .child(search_list::action_row_body(Icon::new(icon), label, cx))
                .when(busy, |row| row.child(crate::feedback::inline_spinner(cx)))
                .into_any_element()
        });
        let marks = crate::chats_view::team_marks(cx);
        let issue_rows: Vec<gpui::AnyElement> = self
            .hits
            .iter()
            .enumerate()
            .map(|(ix, hit)| {
                let position = lead + ix;
                shell(("issue-hit", ix).into(), position, cx)
                    .child(search_list::issue_row_body(
                        Some(with_team_mark(
                            &hit.issue_id,
                            crate::icons::resolved_status_icon(&hit.status, cx).xsmall(),
                            12.,
                            row_team(&marks, crate::queries::issue_team_id(cx, &hit.issue_id).as_deref()),
                            cx.theme().popover,
                            cx,
                        )),
                        hit.identifier.clone(),
                        hit.title.clone(),
                        None,
                        cx,
                    ))
                    .into_any_element()
            })
            .collect();
        let body = if sections.titled && !sections.sections.is_empty() {
            let spec = global::chats();
            let mut children: Vec<gpui::AnyElement> = suggestion_row.into_iter().collect();
            let mut issue_rows = Some(issue_rows);
            for section in &sections.sections {
                match section.key {
                    GlobalSectionKey::Issues => {
                        children.push(section_header(&spec.test_ids.issues, &spec.copy.issues, section.count, false, cx));
                        children.extend(issue_rows.take().unwrap_or_default());
                    }
                    GlobalSectionKey::Chats => {
                        children.push(section_header(&spec.test_ids.chats, &spec.copy.chats, section.count, section.loading, cx));
                        for (ix, chat) in self.chats.iter().enumerate() {
                            let position = lead + self.hits.len() + ix;
                            children.push(
                                shell(("search-list-chat-row", ix).into(), position, cx)
                                    .h(px(spec.row.chat_height))
                                    .child(chat_row_body(&chat.row, row_team(&marks, root_team(&chat.row.root_id, cx).as_deref()), cx))
                                    .into_any_element(),
                            );
                        }
                    }
                }
            }
            div()
                .flex_1()
                .min_h_0()
                .p_1()
                .child(
                    v_flex()
                        .id("search-sheet-rows")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.list.scroll)
                        .children(children),
                )
                .into_any_element()
        } else if issue_rows.is_empty() && suggestion_row.is_none() {
            self.render_empty(cx)
        } else {
            let issue_rows: Vec<gpui::AnyElement> = suggestion_row.into_iter().chain(issue_rows).collect();
            let count = issue_rows.len();
            div()
                .flex_1()
                .min_h_0()
                .p_1()
                .child(search_list::rows_viewport(
                    "search-sheet-rows",
                    issue_rows,
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
                // EXP-1557: no ✕ after the count (the web palette has none):
                // the scrim press and Escape dismiss it. #EDE-99: the count
                // = issue rows + chat rows.
                search_list::input_row(&self.list.input, sections.count.filter(|_| !self.searched.is_empty()), cx),
            )
            .child(search_list::hairline(cx))
            .child(body)
    }
}
