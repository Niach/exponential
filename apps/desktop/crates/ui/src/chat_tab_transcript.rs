//! #EDE-65 lane O: the Chat tab's transcript IS the shared chat document
//! (`apps/ede/ui/chat.json`, `uicore_model::chat`), mounted in place like
//! the sidebar region (`shell_regions.rs`). A person's prompt and the
//! agent's narration are the document's own genUI `MessageBubble`s (side,
//! author, the sender's avatar, the press that opens a session); every other
//! row (tool runs, the edits card, asks, subagents, workflow and
//! conversation cards, Working…) stays the IDE's to paint: a `host` row, an
//! Embed this module claims by its painted id (`chat_row_host.<key>`) and
//! fills with the session view's own row ([`SteerSessionView::chat_host_row`]).
//! The composer, THE Dock, the strips and the header are the IDE's, around
//! it. The document's rows: the paging sentinel first (painting it pulls the
//! older transcript in), the list's rows, the tail spacer last (the gap
//! between the newest row and THE Dock's band, whose top edge ends the
//! document, #EXP-1782).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use domain::transcript_grouping::{feed_input, FeedInput, TranscriptRowLayout, TranscriptSender, TranscriptSide};
use eui_core::host::{MediaOptions, MediaRequest, TextLink, UrlPolicy};
use eui_core::theme::Mode;
use eui_gpui::host::{HostPlugin, MarkdownAttachment, MarkdownOptions};
use eui_gpui::view::SurfaceView;
use gpui::{
    div, px, relative, size, AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, Entity, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, SharedString, Style, Styled as _, WeakEntity, Window,
};
use serde_json::{json, Value};
use uicore_model::chat::{ChatGroup, ChatInput, ChatRowInput};

use crate::markdown::{AppMarkdownHost, DocumentProse};
use crate::steer_viewer::SteerSessionView;

/// The document's transcript list.
pub(crate) const SCROLL: &str = "chat_scroll";
/// The document's host row (an Embed): `chat_row_host.<key>` once painted.
pub(crate) const HOST_NODE: &str = "chat_row_host";
/// What the IDE draws itself: the eDE home, the header, the composer.
pub(crate) const PRUNED: &[&str] = &["chat_home", "chat_header", "chat_footer"];
/// The paging sentinel's row id (the old list's top padding, `pt_2`).
pub(crate) const PAGE_ROW: &str = "page";
const PAGE_HEIGHT: f32 = 8.;
/// The tail spacer's row id (the gap above THE Dock's band).
pub(crate) const TAIL_ROW: &str = "tail";
/// A host row's height before it was ever measured.
const ESTIMATE: f32 = 40.;
/// A sender's avatar, painted by the host: `ide:sender:<row key>`.
pub(crate) const SENDER_ICON: &str = "ide:sender:";
/// A link to a session (a URL the host opens in place).
pub(crate) const SESSION_SCHEME: &str = "exp-session";
/// The document's call (a relay's name line: `ede_chat open {sessionId}`).
const CHAT_CALL: &str = "ede_chat";

/// The document's row key of a list row (its [`steer::FeedItemId`]).
pub(crate) fn row_key(id: steer::FeedItemId) -> String {
    format!("r{id}")
}

/// A host row's painted id.
pub(crate) fn host_id(key: &str) -> String {
    format!("{HOST_NODE}.{key}")
}

// ---------------------------------------------------------------------------
// The mapping (pure)
// ---------------------------------------------------------------------------

/// What one transcript list row says, as far as the document cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowContent<'a> {
    /// A person's prompt (not a divider, not a catalog command): its raw text.
    Prompt(&'a str),
    /// The agent's narration.
    Narration(&'a str),
    /// Anything the IDE paints itself.
    Other,
}

/// One list row as the mapping reads it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RowFacts<'a> {
    pub key: &'a str,
    pub content: RowContent<'a>,
    /// Its transcript-grouping layout (#EXP-1736).
    pub layout: Option<&'a TranscriptRowLayout>,
    /// The ladder gap above it, px.
    pub gap: f32,
    /// Conversation cards ride it (anchored to its calls, or the head cards
    /// over the first row): it stays the IDE's row.
    pub cards: bool,
    /// #EXP-1756: a person's prompt carries its `HH:mm` (epoch ms); the
    /// agent's narration none.
    pub at: Option<i64>,
}

/// A prompt the document's bubble shows as the IDE did: no picture
/// markers, no file chips, short enough that the IDE never folded it.
pub(crate) fn prompt_is_plain(text: &str) -> bool {
    let parsed = steer::parse_steer_message(text);
    parsed.files.is_empty() && parsed.attachment_ids.is_empty() && !crate::steer_viewer::clampable(text)
}

/// #EXP-1772: a relay the document's bubble shows whole (no picture
/// markers, no file chips); #FEED-238: one that folds is the IDE's bubble.
fn plain_relay(text: &str) -> bool {
    let parsed = steer::parse_steer_message(text);
    parsed.files.is_empty() && parsed.attachment_ids.is_empty() && !crate::steer_viewer::clampable(text)
}

/// The document's gap spacers (3, 8, 12, 16 px): the nearest one.
fn spacer(gap: f32) -> u32 {
    if gap <= 0.5 {
        return 0;
    }
    [3u32, 8, 12, 16].into_iter().min_by(|a, b| (*a as f32 - gap).abs().total_cmp(&(*b as f32 - gap).abs())).unwrap_or(16)
}

/// One list row as the document's row: a bubble for anyone else's plain
/// prompt or the agent's narration (no cards beside it, nothing to fold),
/// else a host row. #FEED-239: the viewer's OWN bubble is the IDE's row (it
/// wears the viewer's avatar on the right, [`crate::session_chat::own_row`],
/// which the document's `out` bubble has no column for); #FEED-238: so is
/// every bubble that folds (THE fold's Show more).
pub(crate) fn chat_row(facts: &RowFacts) -> ChatRowInput {
    let host = || ChatRowInput::Host { id: facts.key.to_string() };
    if facts.cards {
        return host();
    }
    let sender = facts.layout.and_then(|layout| layout.sender.as_ref());
    let other = facts.layout.is_some_and(|layout| layout.side == TranscriptSide::Other);
    let relay = other && sender.is_some_and(|sender| sender.link.is_some());
    let own = match facts.content {
        // #EXP-1772: a system input is THE marker, the IDE's own row.
        RowContent::Prompt(text) if matches!(feed_input(text), Some(FeedInput::Marker { .. })) => return host(),
        RowContent::Prompt(text) if prompt_is_plain(text) || (relay && plain_relay(text)) => {
            facts.layout.is_none_or(|layout| layout.side == TranscriptSide::Own)
        }
        RowContent::Narration(text) if !crate::steer_viewer::clampable(text) => false,
        _ => return host(),
    };
    if own {
        return host();
    }
    let body = match facts.content {
        // A session's input (`Message from …`) reads as its body alone.
        RowContent::Prompt(text) if !own => match feed_input(text) {
            Some(FeedInput::Relay { body, .. }) => body,
            _ => text.to_string(),
        },
        RowContent::Prompt(text) | RowContent::Narration(text) => text.to_string(),
        RowContent::Other => unreachable!("a host row above"),
    };
    ChatRowInput::Message {
        id: facts.key.to_string(),
        author: if own { "user" } else { "agent" }.to_string(),
        body,
        // #EXP-1772: the document draws a relay's name (+ its chevron) above
        // the bubble itself.
        name: facts.layout.filter(|layout| layout.name).and(sender).map(|sender| sender.name.clone()),
        time: match facts.content {
            RowContent::Prompt(_) => facts.at.map(crate::session_chat::clock_ms).unwrap_or_default(),
            _ => String::new(),
        },
        queued: false,
        // #FEED-237: the live transcript's echo of the viewer's own turn
        // reads two checks.
        status: (own && matches!(facts.content, RowContent::Prompt(_))).then(|| "read".to_string()),
        group: Some(ChatGroup {
            side: Some(if own { "out" } else { "in" }.to_string()),
            avatar: (other && facts.layout.is_some_and(|layout| layout.avatar) && sender.is_some()).then(|| format!("{SENDER_ICON}{}", facts.key)),
            indent: other,
            link: sender.filter(|_| other).and_then(|sender| sender.link.clone()),
            gap: spacer(facts.gap),
        }),
    }
}

// ---------------------------------------------------------------------------
// The host's state
// ---------------------------------------------------------------------------

/// What a host row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostRow {
    /// List row `ix` of the session view, its [`crate::transcript_rows::RowKey`] fingerprint.
    List { ix: usize, fingerprint: u64 },
    /// #LOGS-10: a head card (the view's conversation card `index`, its
    /// call above the loaded rows), a row of its own above the first.
    Card { index: usize, fingerprint: u64 },
    Page,
    Tail,
}

impl HostRow {
    /// A measured row's content fingerprint (the spacers have none).
    fn fingerprint(&self) -> Option<u64> {
        match *self {
            HostRow::List { fingerprint, .. } | HostRow::Card { fingerprint, .. } => Some(fingerprint),
            HostRow::Page | HostRow::Tail => None,
        }
    }
}

#[derive(Default)]
struct HostState {
    steer: Option<WeakEntity<SteerSessionView>>,
    view: Option<WeakEntity<SurfaceView>>,
    /// Painted id → the row.
    rows: HashMap<String, HostRow>,
    /// Painted id → its last measured height.
    heights: HashMap<String, f32>,
    /// Painted id → the fingerprint and width it was measured at.
    measured_at: HashMap<String, (u64, f32)>,
    /// Avatar icon name → the sender (and a member's picture).
    senders: HashMap<String, (TranscriptSender, Option<Arc<gpui::Image>>)>,
    /// The tail spacer's height.
    tail: f32,
    /// The rows' width as they last painted.
    width: Option<f32>,
    /// #VAPP-258: the live reply's prepared text (the last narration while
    /// the run is live): its Markdown streams (genUI holds the unfinished
    /// tail back), as `render_narration` did.
    streaming: Option<String>,
    /// Moves every time a row measured at a height other than it was laid
    /// out at (the rows below it moved): a deep link's scroll settles only
    /// once a layout moved nothing.
    relayouts: u64,
    /// Painted ids [`TranscriptHost::host_leaf`] filled (tests).
    #[cfg(test)]
    painted: Vec<String>,
    /// Rows painted in a frame not their measured height: `(id, frame,
    /// measured)` (tests).
    #[cfg(test)]
    misfits: Vec<(String, f32, f32)>,
}

impl HostState {
    fn height(&self, id: &str) -> f32 {
        match self.rows.get(id) {
            Some(HostRow::Page) => PAGE_HEIGHT,
            Some(HostRow::Tail) => self.tail,
            Some(HostRow::List { .. } | HostRow::Card { .. }) => self.heights.get(id).copied().unwrap_or(ESTIMATE),
            None => 0.,
        }
    }

    /// A row measured at `width`: whether the document must lay out again
    /// (the height it was answered differs).
    fn measured(&mut self, id: &str, fingerprint: u64, width: f32, height: f32) -> bool {
        let answered = self.height(id);
        self.width = Some(width);
        self.heights.insert(id.to_string(), height);
        self.measured_at.insert(id.to_string(), (fingerprint, width));
        let moved = (answered - height).abs() > 0.5;
        if moved {
            self.relayouts += 1;
        }
        moved
    }

    fn fresh(&self, id: &str, fingerprint: u64, width: f32) -> bool {
        self.measured_at.get(id).is_some_and(|(at, w)| *at == fingerprint && (w - width).abs() < 0.5)
    }
}

// ---------------------------------------------------------------------------
// The transcript
// ---------------------------------------------------------------------------

/// What a push of the rows needs besides them.
pub(crate) struct ChatPush {
    pub session_id: String,
    pub prose: DocumentProse,
    /// List row `ix`'s painted host id → the row (the bubbles' rows absent).
    pub host_rows: HashMap<String, HostRow>,
    /// Avatar icon name → its sender (+ picture).
    pub senders: HashMap<String, (TranscriptSender, Option<Arc<gpui::Image>>)>,
    /// The live reply's row key (its bubble's Markdown streams).
    pub streaming: Option<String>,
}

/// The Chat tab's transcript: the mounted document, the host rows, the
/// FOLLOW (the old gpui list's `FollowMode::Tail`), paging and jumps.
pub(crate) struct ChatTranscript {
    view: Option<Entity<SurfaceView>>,
    md: Option<Rc<AppMarkdownHost>>,
    state: Rc<RefCell<HostState>>,
    shown: Option<Value>,
    /// The rows changed since the last push.
    pub(crate) dirty: bool,
    /// The store revision the bubbles' prose was prepared at.
    revision: u64,
    /// The document's row count (sentinel + list + tail).
    count: usize,
    /// The live reply's row key at the last push.
    streaming_key: Option<String>,
    /// The document's list follows its newest row (as of this frame's
    /// render; the document holds it, [`SurfaceView::keep_list_place`]).
    follow: bool,
    to_end: bool,
    scroll_to: Option<(u32, &'static str)>,
    /// waiting-cards.json DEEP LINKS: a deep link is pending, the list holds
    /// its place (no FOLLOW to the newest row).
    pinned: bool,
    /// The last reveal: the relayout count it was applied at and the
    /// layouts since.
    revealed: Option<(u64, u32)>,
    /// The reader scrolled the list themselves (wheel, trackpad, a drag's
    /// auto-scroll) since the last take.
    reader_scrolled: Rc<std::cell::Cell<bool>>,
}

impl Default for ChatTranscript {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatTranscript {
    /// A session tab opens on its newest rows and follows them.
    pub(crate) fn new() -> Self {
        Self {
            view: None,
            md: None,
            state: Rc::new(RefCell::new(HostState::default())),
            shown: None,
            dirty: true,
            revision: u64::MAX,
            count: 0,
            streaming_key: None,
            follow: true,
            to_end: true,
            scroll_to: None,
            pinned: false,
            revealed: None,
            reader_scrolled: Rc::default(),
        }
    }

    /// The document's view, once mounted.
    pub(crate) fn view(&self) -> Option<&Entity<SurfaceView>> {
        self.view.as_ref()
    }

    /// The store revision the bubbles were prepared at.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Mounts the document for `steer` (once).
    pub(crate) fn mount(&mut self, steer: WeakEntity<SteerSessionView>, team_id: Option<String>, window: &mut Window, cx: &mut App) {
        if self.view.is_some() {
            return;
        }
        let md = Rc::new(AppMarkdownHost::for_document(team_id, cx));
        let host = Rc::new(TranscriptHost { state: self.state.clone(), md: md.clone() });
        let doc = document();
        let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
        let view = uicore_gpui::inline::mount(&doc, &uicore_data::Feed::new(), theme, host, Mode::Dark, window, cx);
        // The FOLLOW and the reader's place (paging, rows measured above
        // them) are the document's, settled in the frame it lays out; the
        // paging sentinel is never the reader's row.
        // A short transcript ends right above THE Dock (the old gpui list
        // sat at the pane's bottom), a long one scrolls.
        view.update(cx, |view, cx| {
            view.keep_list_place(SCROLL, true, 1, cx);
            view.anchor_list_end(SCROLL, true, cx);
        });
        {
            let mut state = self.state.borrow_mut();
            state.steer = Some(steer);
            state.view = Some(view.downgrade());
        }
        self.view = Some(view);
        self.md = Some(md);
        self.dirty = true;
    }

    /// The rows the document draws: `rows` = the list's (the sentinel and
    /// the tail spacer are added here). A new or changed host row measures
    /// where the document is about to paint it (`host_leaf_prepaint`).
    pub(crate) fn push(&mut self, rows: Vec<ChatRowInput>, push: ChatPush, cx: &mut App) {
        let (Some(view), Some(md)) = (self.view.clone(), self.md.clone()) else { return };
        self.dirty = false;
        self.revision = crate::markdown::store_revision(cx);
        let mut rows = rows;
        // The bubbles' prose, as the steer feed's MarkdownView prepared it.
        let bodies: Vec<String> = rows
            .iter()
            .filter_map(|row| match row {
                ChatRowInput::Message { body, .. } => Some(body.clone()),
                _ => None,
            })
            .collect();
        #[cfg(test)]
        counters::bump(|c| c.2 += bodies.len());
        let mut prepared = {
            let _span = tracing::trace_span!("chat.prepare", rows = bodies.len()).entered();
            md.prepare_document(&bodies, push.prose, cx).into_iter()
        };
        let mut streaming = None;
        for row in &mut rows {
            if let ChatRowInput::Message { id, body, .. } = row {
                *body = prepared.next().unwrap_or_default();
                if push.streaming.as_deref() == Some(id.as_str()) {
                    streaming = Some(body.clone());
                }
            }
        }
        self.streaming_key = push.streaming;
        // The reply that stopped streaming (or started) lays out again under
        // its new options (its text may be the same).
        let restream = {
            let mut state = self.state.borrow_mut();
            (state.streaming != streaming).then(|| [std::mem::replace(&mut state.streaming, streaming.clone()), streaming])
        };
        let mut all = Vec::with_capacity(rows.len() + 2);
        all.push(ChatRowInput::Host { id: PAGE_ROW.into() });
        all.extend(rows);
        all.push(ChatRowInput::Host { id: TAIL_ROW.into() });
        self.count = all.len();
        // Paging: older rows come in ABOVE the reader's row; the document
        // keeps that row where it is on screen (its kept place).
        // The host rows: a changed one lays out again.
        let mut changed = Vec::new();
        {
            let mut state = self.state.borrow_mut();
            for (id, row) in &push.host_rows {
                let Some(fingerprint) = row.fingerprint() else { continue };
                if state.rows.get(id).and_then(HostRow::fingerprint).is_some_and(|old| old != fingerprint) {
                    changed.push(id.clone());
                }
            }
            let mut rows = push.host_rows;
            rows.insert(host_id(PAGE_ROW), HostRow::Page);
            rows.insert(host_id(TAIL_ROW), HostRow::Tail);
            state.heights.retain(|id, _| rows.contains_key(id));
            state.measured_at.retain(|id, _| rows.contains_key(id));
            state.rows = rows;
            state.senders = push.senders;
        }
        let input = ChatInput { session_id: Some(push.session_id), bubbles: true, slash_commands: true, rows: all, ..ChatInput::default() };
        let value = {
            let _span = tracing::trace_span!("chat.view").entered();
            serde_json::to_value(uicore_model::chat::chat_view(&input)).unwrap_or_default()
        };
        if self.shown.as_ref() != Some(&value) {
            self.shown = Some(value.clone());
            let _span = tracing::trace_span!("chat.set_data").entered();
            view.update(cx, |view, cx| {
                let _ = view.set_data("/chatting", Some(value), cx);
            });
        }
        if !changed.is_empty() {
            view.update(cx, |view, cx| {
                for id in &changed {
                    view.host_leaf_resized(id, cx);
                }
            });
        }
        if let Some(texts) = restream {
            view.update(cx, |view, cx| {
                for text in texts.iter().flatten() {
                    view.markdown_options_changed(text, cx);
                }
            });
        }
    }

    /// The live reply's row key as last pushed.
    pub(crate) fn streaming_key(&self) -> Option<&str> {
        self.streaming_key.as_deref()
    }

    /// The gap between the newest row and THE Dock's band.
    pub(crate) fn set_tail(&mut self, height: f32, cx: &mut App) {
        let moved = {
            let mut state = self.state.borrow_mut();
            let moved = (state.tail - height).abs() > 0.5;
            state.tail = height;
            moved
        };
        if moved {
            if let Some(view) = &self.view {
                let id = host_id(TAIL_ROW);
                view.update(cx, |view, cx| view.host_leaf_resized(&id, cx));
            }
        }
    }

    /// Every host row measures again (something under every row moved).
    pub(crate) fn remeasure_all(&mut self, cx: &mut App) {
        let ids: Vec<String> = {
            let mut state = self.state.borrow_mut();
            state.measured_at.clear();
            state.rows.keys().cloned().collect()
        };
        if let Some(view) = &self.view {
            view.update(cx, |view, cx| {
                for id in &ids {
                    view.host_leaf_resized(id, cx);
                }
            });
        }
    }

    // -- FOLLOW ------------------------------------------------------------

    /// The list follows its newest rows.
    pub(crate) fn following(&self) -> bool {
        self.follow
    }

    /// Back to the newest row, following it.
    pub(crate) fn scroll_to_end(&mut self) {
        self.follow = true;
        self.to_end = true;
        self.scroll_to = None;
    }

    /// List row `ix` scrolled to `align` (the reader stops following).
    pub(crate) fn reveal(&mut self, ix: usize, align: &'static str) {
        self.follow = false;
        self.to_end = false;
        self.scroll_to = Some((ix as u32 + 1, align));
    }

    /// waiting-cards.json DEEP LINKS: a deep link holds (`true`) or releases
    /// the list. Holding stops the FOLLOW; releasing forgets the reveal.
    pub(crate) fn pin(&mut self, on: bool) {
        if on && !self.pinned {
            self.follow = false;
            self.to_end = false;
            self.reader_scrolled.set(false);
        }
        if !on {
            self.revealed = None;
        }
        self.pinned = on;
    }

    /// The document laid out since the rows were last pushed (a found row
    /// can be scrolled to).
    pub(crate) fn laid_out(&self, cx: &App) -> bool {
        !self.dirty && self.extent(cx).is_some()
    }

    /// A frame begins: the last frame laid the list out (call before the
    /// deep link's step reads [`Self::reveal_settled`]). A row measured
    /// anew since the reveal restarts its count.
    pub(crate) fn note_layout(&mut self) {
        if self.scroll_to.is_some() {
            return;
        }
        if let Some((at, layouts)) = self.revealed {
            let relayouts = self.state.borrow().relayouts;
            self.revealed = Some(if at == relayouts { (at, layouts + 1) } else { (relayouts, 0) });
        }
    }

    /// The last reveal was applied and a layout since measured no row anew
    /// (the row stands where the scroll put it).
    pub(crate) fn reveal_settled(&self) -> bool {
        let relayouts = self.state.borrow().relayouts;
        self.scroll_to.is_none() && self.revealed.is_some_and(|(at, layouts)| at == relayouts && layouts >= 1)
    }

    /// The reader scrolled the list since the last take.
    pub(crate) fn take_reader_scrolled(&self) -> bool {
        self.reader_scrolled.replace(false)
    }

    /// `(offset, viewport, content)` of the list, as last laid out.
    fn extent(&self, cx: &App) -> Option<(f32, f32, f32)> {
        self.view.as_ref()?.read(cx).scroll_extent(SCROLL)
    }

    /// How far the list sits above its bottom; `None` before a layout.
    pub(crate) fn distance_from_bottom(&self, cx: &App) -> Option<f32> {
        let (offset, viewport, content) = self.extent(cx)?;
        Some(((content - viewport).max(0.) - offset).max(0.))
    }

    /// The reader is within `transcript.followSlack` of the bottom (or the
    /// list follows already): the chats' FOLLOW rule.
    pub(crate) fn near_tail(&self, cx: &App) -> bool {
        self.follow || self.distance_from_bottom(cx).is_none_or(|d| d <= domain::chats::CHATS.transcript.follow_slack)
    }

    /// Rows arrived: a reader inside the slack is still on the tail (never
    /// while a deep link holds the list).
    pub(crate) fn arrival(&mut self, cx: &App) {
        if !self.pinned && !self.follow && self.near_tail(cx) {
            self.scroll_to_end();
        }
    }

    /// The list left its top.
    pub(crate) fn scrolled(&self, cx: &App) -> bool {
        self.extent(cx).is_some_and(|(offset, _, _)| offset > 0.)
    }

    /// Before the document lays out this frame: the scroll this frame asks
    /// for (a row revealed, back to the newest), then what the document's
    /// list does (it follows the newest row until the reader scrolls up,
    /// and again once they reach it; it settles in the frame it lays out).
    /// Returns the extent it read.
    pub(crate) fn sync_scroll(&mut self, cx: &mut App) -> Option<(f32, f32, f32)> {
        let view = self.view.clone()?;
        if let Some((index, align)) = self.scroll_to.take() {
            view.update(cx, |view, cx| {
                view.keep_list_place(SCROLL, false, 1, cx);
                view.scroll_to_index(SCROLL, index, Some(align), cx);
            });
            self.revealed = Some((self.state.borrow().relayouts, 0));
        }
        if std::mem::take(&mut self.to_end) {
            view.update(cx, |view, cx| view.keep_list_place(SCROLL, true, 1, cx));
        }
        // DEEP LINKS: a pending link holds the reader's place, never the tail.
        if self.pinned && view.read(cx).list_follows(SCROLL).unwrap_or(true) {
            view.update(cx, |view, cx| view.keep_list_place(SCROLL, false, 1, cx));
        }
        self.follow = !self.pinned && view.read(cx).list_follows(SCROLL).unwrap_or(true);
        self.extent(cx)
    }

    /// The document filling the feed's box, and a probe that asks the
    /// session view for one more frame when the document's settling changed
    /// what this frame drew from (the FOLLOW, the list leaving its top, the
    /// first layout).
    pub(crate) fn element(&self, before: Option<(f32, f32, f32)>, steer: gpui::EntityId) -> Option<AnyElement> {
        let view = self.view.clone()?;
        let follow = self.follow;
        let probe_view = view.clone();
        let probe = gpui::canvas(
            move |_, _, cx| {
                let view = probe_view.read(cx);
                let now = view.scroll_extent(SCROLL);
                let follows = view.list_follows(SCROLL).unwrap_or(true);
                let top = |extent: Option<(f32, f32, f32)>| extent.is_some_and(|(offset, _, _)| offset > 0.);
                if follows != follow || (before.is_none() && now.is_some()) || top(before) != top(now) {
                    cx.defer(move |cx| cx.notify(steer));
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_0();
        // DEEP LINKS: the reader's own scroll over the list (capture phase:
        // the document's scroller consumes the wheel) drops a pending link.
        let scrolled = self.reader_scrolled.clone();
        let reader = gpui::canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                window.on_mouse_event(move |ev: &gpui::ScrollWheelEvent, phase, _, _| {
                    if phase == gpui::DispatchPhase::Capture && bounds.contains(&ev.position) {
                        scrolled.set(true);
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        Some(div().relative().flex_1().min_h_0().w_full().child(uicore_gpui::inline::region(&view)).child(probe).child(reader).into_any_element())
    }

    /// Test aid: the host rows the document had the host paint.
    #[cfg(test)]
    pub(crate) fn painted(&self) -> Vec<String> {
        self.state.borrow().painted.clone()
    }

    /// Test aid: the host rows painted in a frame other than their measured
    /// height (`id`, frame, measured).
    #[cfg(test)]
    pub(crate) fn misfits(&self) -> Vec<(String, f32, f32)> {
        self.state.borrow().misfits.clone()
    }

    /// Test aid: a host row's measured height.
    #[cfg(test)]
    pub(crate) fn host_height(&self, id: &str) -> Option<f32> {
        self.state.borrow().heights.get(id).copied()
    }
}

/// Bench and guard counters (tests): host rows painted, host rows
/// measured, bubble bodies prepared (Markdown parsed for the document).
#[cfg(test)]
pub(crate) mod counters {
    use std::cell::Cell;

    thread_local! {
        static COUNTS: Cell<(usize, usize, usize)> = const { Cell::new((0, 0, 0)) };
    }

    pub(crate) fn bump(f: impl FnOnce(&mut (usize, usize, usize))) {
        COUNTS.with(|counts| {
            let mut now = counts.get();
            f(&mut now);
            counts.set(now);
        });
    }

    pub(crate) fn reset() {
        COUNTS.with(|counts| counts.set((0, 0, 0)));
    }

    /// `(painted, measured, bodies)` since the last take, and zeroed.
    pub(crate) fn take() -> (usize, usize, usize) {
        COUNTS.with(|counts| counts.replace((0, 0, 0)))
    }
}

/// The chat document as the Chat tab draws it: the rows alone (the IDE's
/// header, composer and Dock around them), no frame of its own, the rows in
/// the IDE's reading column (the list itself spans the pane: its scrollbar
/// sits at the pane's edge), the gaps the rows' own.
pub(crate) fn document() -> uicore_gpui::inline::Document {
    let mut doc = uicore_gpui::inline::document(uicore_docs::doc("chat").expect("the factory chat"));
    uicore_gpui::inline::without(&mut doc, PRUNED);
    for key in ["borderWidth", "borderColor", "borderRadius"] {
        uicore_gpui::inline::set_style(&mut doc, "chat", key, Value::Null);
    }
    uicore_gpui::inline::set_prop(&mut doc, SCROLL, "style", json!({"flexGrow": 1, "minHeight": 0, "width": "100%", "overflowY": "auto"}));
    for (key, value) in [
        ("width", json!("100%")),
        ("maxWidth", json!(crate::work_header::CHAT_COLUMN_W)),
        ("alignSelf", json!("center")),
        ("paddingLeft", json!(crate::work_header::WORK_GUTTER)),
        ("paddingRight", json!(crate::work_header::WORK_GUTTER)),
    ] {
        uicore_gpui::inline::set_style(&mut doc, "chat_row", key, value);
    }
    doc
}

// ---------------------------------------------------------------------------
// The host
// ---------------------------------------------------------------------------

/// The document's host: the IDE's rows and avatars, the steer feed's prose.
struct TranscriptHost {
    state: Rc<RefCell<HostState>>,
    md: Rc<AppMarkdownHost>,
}

impl HostPlugin for TranscriptHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.md.icon(name)
    }

    fn icon_element(&self, name: &str, _size: f32, _color: Hsla, cx: &App) -> Option<AnyElement> {
        name.strip_prefix(SENDER_ICON)?;
        let state = self.state.borrow();
        let (sender, image) = state.senders.get(name)?;
        Some(crate::session_chat::sender_avatar_of(sender, image.clone(), cx))
    }

    fn host_leaf_height(&self, component_id: &str, width: f32, _window: &mut Window, _cx: &mut App) -> Option<f32> {
        component_id.strip_prefix(HOST_NODE)?.strip_prefix('.')?;
        let state = self.state.borrow();
        let _ = width;
        Some(state.height(component_id))
    }

    /// The row measured where the document is about to paint it (prepaint,
    /// outside its layout) when it changed or was never measured at this
    /// width: the document lays out around its real height in this frame.
    fn host_leaf_prepaint(&self, component_id: &str, width: f32, window: &mut Window, cx: &mut App) -> bool {
        let (row, fingerprint, steer) = {
            let state = self.state.borrow();
            let Some(row) = state.rows.get(component_id).copied() else { return false };
            let Some(fingerprint) = row.fingerprint() else { return false };
            if state.fresh(component_id, fingerprint, width) {
                return false;
            }
            (row, fingerprint, state.steer.clone().and_then(|steer| steer.upgrade()))
        };
        let Some(steer) = steer else { return false };
        let element = steer.update(cx, |this, cx| match row {
            HostRow::Card { index, .. } => this.chat_card_row(index, window, cx),
            HostRow::List { ix, .. } => this.chat_host_row(ix, window, cx),
            HostRow::Page | HostRow::Tail => div().into_any_element(),
        });
        #[cfg(test)]
        counters::bump(|c| c.1 += 1);
        let height = measure(element, width, window, cx);
        self.state.borrow_mut().measured(component_id, fingerprint, width, height)
    }

    fn host_leaf(&self, component_id: &str, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        component_id.strip_prefix(HOST_NODE)?.strip_prefix('.')?;
        let (row, steer) = {
            let mut state = self.state.borrow_mut();
            #[cfg(test)]
            state.painted.push(component_id.to_string());
            #[cfg(not(test))]
            let _ = &mut state;
            (state.rows.get(component_id).copied(), state.steer.clone().and_then(|steer| steer.upgrade()))
        };
        let Some(steer) = steer else { return Some(div().into_any_element()) };
        match row {
            Some(HostRow::Page) => {
                steer.update(cx, |this, cx| this.chat_page_painted(cx));
                Some(div().into_any_element())
            }
            Some(row @ (HostRow::List { .. } | HostRow::Card { .. })) => {
                #[cfg(test)]
                counters::bump(|c| c.0 += 1);
                let fingerprint = row.fingerprint().unwrap_or_default();
                let child = steer.update(cx, |this, cx| match row {
                    HostRow::Card { index, .. } => this.chat_card_row(index, window, cx),
                    HostRow::List { ix, .. } => this.chat_host_row(ix, window, cx),
                    HostRow::Page | HostRow::Tail => div().into_any_element(),
                });
                let (state, id) = (self.state.clone(), component_id.to_string());
                let on_height: OnHeight = Box::new(move |width, framed, height, cx| {
                    let (resize, view) = {
                        let mut state = state.borrow_mut();
                        #[cfg(test)]
                        if (framed - height).abs() > 0.5 {
                            state.misfits.push((id.clone(), framed, height));
                        }
                        #[cfg(not(test))]
                        let _ = framed;
                        (state.measured(&id, fingerprint, width, height), state.view.clone())
                    };
                    if resize {
                        cx.defer(move |cx| {
                            if let Some(view) = view.and_then(|view| view.upgrade()) {
                                view.update(cx, |view, cx| view.host_leaf_resized(&id, cx));
                            }
                        });
                    }
                });
                Some(MeasuredRow { child, on_height: Some(on_height) }.into_any_element())
            }
            _ => Some(div().into_any_element()),
        }
    }

    /// #EXP-1772: a relay's name line (`ede_chat open {sessionId}`) opens
    /// that session on the back stack.
    fn on_function_call(&self, event: &eui_gpui::host::FunctionCallEvent, cx: &mut App) {
        if event.name != CHAT_CALL || event.args.get("action").and_then(Value::as_str) != Some("open") {
            return;
        }
        let Some(session) = event.args.get("sessionId").and_then(Value::as_str).filter(|id| !id.is_empty()) else { return };
        let session = session.to_string();
        crate::navigation::on_active_window(cx, move |window, cx| crate::navigation::open_session_link(&session, window, cx));
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        if let Some(session) = url.strip_prefix(SESSION_SCHEME).and_then(|rest| rest.strip_prefix(':')) {
            let session = session.to_string();
            crate::navigation::on_active_window(cx, move |window, cx| crate::navigation::open_session_link(&session, window, cx));
            return;
        }
        self.md.open_url(url, cx)
    }

    fn url_policy(&self) -> Option<UrlPolicy> {
        let mut policy = self.md.url_policy().unwrap_or_default();
        policy.schemes.get_or_insert_with(Vec::new).push(SESSION_SCHEME.to_string());
        Some(policy)
    }

    fn media_options(&self) -> MediaOptions {
        self.md.media_options()
    }

    fn media_request(&self, src: &str) -> Option<MediaRequest> {
        self.md.media_request(src)
    }

    fn text_links(&self, text: &str, cx: &App) -> Option<Vec<TextLink>> {
        self.md.text_links(text, cx)
    }

    fn image_size(&self, src: &str, cx: &App) -> Option<(f32, f32)> {
        self.md.image_size(src, cx)
    }

    fn image_pressable(&self, cx: &App) -> bool {
        self.md.image_pressable(cx)
    }

    fn on_image_press(&self, src: &str, window: &mut Window, cx: &mut App) {
        self.md.on_image_press(src, window, cx)
    }

    fn attachment(&self, href: &str, cx: &App) -> Option<MarkdownAttachment> {
        self.md.attachment(href, cx)
    }

    fn markdown_hooks_version(&self, text: &str, cx: &App) -> Option<u64> {
        self.md.markdown_hooks_version(text, cx)
    }

    fn markdown_window_selection(&self, cx: &App) -> bool {
        self.md.markdown_window_selection(cx)
    }

    fn chip_icon(&self, name: &str, size: f32, color: Hsla) -> Option<AnyElement> {
        self.md.chip_icon(name, size, color)
    }

    fn markdown_options(&self, cx: &App) -> MarkdownOptions {
        self.md.markdown_options(cx)
    }

    /// #VAPP-258: the live reply streams (its unfinished tail held back);
    /// every other bubble reads whole.
    fn markdown_options_for(&self, text: &str, cx: &App) -> MarkdownOptions {
        let streaming = self.state.borrow().streaming.as_deref() == Some(text);
        MarkdownOptions { streaming, ..self.md.markdown_options(cx) }
    }

    fn pick_files(&self, request: &eui_gpui::host::FilePickRequest, cx: &mut App) -> bool {
        self.md.pick_files(request, cx)
    }
}

// ---------------------------------------------------------------------------
// A host row measured where it paints
// ---------------------------------------------------------------------------

/// `(width, the frame's height, the measured height)`.
type OnHeight = Box<dyn FnOnce(f32, f32, f32, &mut App)>;

/// The session view's row filling the frame the document gave it: laid out
/// at the frame's width (gpui `list`'s measure), its height reported (a new
/// one lays the document out again), clipped to the frame until it does.
struct MeasuredRow {
    child: AnyElement,
    on_height: Option<OnHeight>,
}

impl IntoElement for MeasuredRow {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MeasuredRow {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        let measured = self.child.layout_as_root(size(AvailableSpace::Definite(bounds.size.width), AvailableSpace::MinContent), window, cx);
        if let Some(on_height) = self.on_height.take() {
            on_height(f32::from(bounds.size.width), f32::from(bounds.size.height), f32::from(measured.height), cx);
        }
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.prepaint_at(bounds.origin, window, cx);
        });
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| self.child.paint(window, cx));
    }
}

/// A host row laid out at `width` now (the measure [`MeasuredRow`] takes).
pub(crate) fn measure(mut element: AnyElement, width: f32, window: &mut Window, cx: &mut App) -> f32 {
    f32::from(element.layout_as_root(size(AvailableSpace::Definite(px(width)), AvailableSpace::MinContent), window, cx).height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::transcript_grouping::SenderKind;

    fn layout(side: TranscriptSide, sender: Option<TranscriptSender>, avatar: bool, name: bool) -> TranscriptRowLayout {
        TranscriptRowLayout { id: "x".into(), side, sender, avatar, name, gap: 0. }
    }

    fn facts<'a>(content: RowContent<'a>, layout: Option<&'a TranscriptRowLayout>, gap: f32, cards: bool) -> RowFacts<'a> {
        RowFacts { key: "r7", content, layout, gap, cards, at: None }
    }

    /// #FEED-239: the viewer's own bubble wears THEIR avatar on the right:
    /// the IDE paints it (the document's `out` bubble has no avatar column).
    #[test]
    fn the_viewers_own_prompt_is_the_ides_row() {
        let own = layout(TranscriptSide::Own, None, true, false);
        assert!(matches!(chat_row(&facts(RowContent::Prompt("Ship it"), Some(&own), 16., false)), ChatRowInput::Host { id } if id == "r7"));
        assert!(matches!(chat_row(&facts(RowContent::Prompt("Ship it"), None, 16., false)), ChatRowInput::Host { .. }));
    }

    /// #FEED-238 THE FOLD: a bubble that folds is the IDE's row (its Show
    /// more), the agent's narration and a relay included.
    #[test]
    fn a_bubble_that_folds_is_the_ides_row() {
        let agent = TranscriptSender { kind: SenderKind::Agent, id: "ag1".into(), name: "Bro".into(), link: None };
        let first = layout(TranscriptSide::Other, Some(agent), true, true);
        let long = "word ".repeat(200);
        assert!(matches!(chat_row(&facts(RowContent::Narration(&long), Some(&first), 12., false)), ChatRowInput::Host { .. }));
        let session = TranscriptSender { kind: SenderKind::Session, id: "s2".into(), name: "Fix login".into(), link: Some("s2".into()) };
        let relay = layout(TranscriptSide::Other, Some(session), true, true);
        let text = format!("Message from Fix login (session s2): {long}");
        assert!(matches!(chat_row(&facts(RowContent::Prompt(&text), Some(&relay), 8., false)), ChatRowInput::Host { .. }));
    }

    #[test]
    fn narration_sits_under_its_senders_avatar_with_its_name() {
        let agent = TranscriptSender { kind: SenderKind::Agent, id: "ag1".into(), name: "Bro".into(), link: None };
        let first = layout(TranscriptSide::Other, Some(agent.clone()), true, true);
        let ChatRowInput::Message { author, body, name, group, .. } = chat_row(&facts(RowContent::Narration("Done."), Some(&first), 12., false)) else { panic!("a bubble") };
        assert_eq!((author.as_str(), body.as_str(), name.as_deref()), ("agent", "Done.", Some("Bro")));
        let group = group.expect("grouped");
        assert_eq!((group.side.as_deref(), group.avatar.as_deref(), group.indent, group.gap), (Some("in"), Some("ide:sender:r7"), true, 12));
        // A later row of the group: the avatar's spacer, no name.
        let later = layout(TranscriptSide::Other, Some(agent), false, false);
        let ChatRowInput::Message { name, group, .. } = chat_row(&facts(RowContent::Narration("More."), Some(&later), 3., false)) else { panic!("a bubble") };
        let group = group.expect("grouped");
        assert_eq!((name, group.avatar, group.indent, group.gap), (None, None, true, 3));
    }

    #[test]
    fn a_session_input_is_that_sessions_linked_bubble() {
        let session = TranscriptSender { kind: SenderKind::Session, id: "s2".into(), name: "Fix login".into(), link: Some("s2".into()) };
        let other = layout(TranscriptSide::Other, Some(session), true, true);
        let text = "Message from Fix login (session s2): All green.";
        let ChatRowInput::Message { author, body, name, group, .. } = chat_row(&facts(RowContent::Prompt(text), Some(&other), 8., false)) else { panic!("a bubble") };
        assert_eq!((author.as_str(), body.as_str()), ("agent", "All green."));
        assert_eq!(name.as_deref(), Some("Fix login"), "the document adds the chevron above the bubble");
        assert_eq!(group.and_then(|group| group.link).as_deref(), Some("s2"));
        // A tagged system input is the IDE's marker row.
        let marker = "[schedule] Schedule `*/30 * * * *` fired.";
        assert!(matches!(chat_row(&facts(RowContent::Prompt(marker), None, 8., false)), ChatRowInput::Host { .. }));
    }

    #[test]
    fn everything_else_is_a_host_row() {
        let host = |row: ChatRowInput| matches!(row, ChatRowInput::Host { id } if id == "r7");
        assert!(host(chat_row(&facts(RowContent::Other, None, 12., false))));
        // Cards ride the row: the IDE paints it with them.
        assert!(host(chat_row(&facts(RowContent::Narration("Posted a plan."), None, 12., true))));
        // A folded prompt, a picture, a file: the IDE's bubble.
        let long = "word ".repeat(400);
        assert!(host(chat_row(&facts(RowContent::Prompt(&long), None, 12., false))));
        let picture = steer::build_steer_image_message("Look", &["a1".to_string()]);
        assert!(host(chat_row(&facts(RowContent::Prompt(&picture), None, 12., false))));
    }

    /// #EXP-1756: a person's bubble carries its `HH:mm`, the agent's
    /// narration none.
    #[test]
    fn a_prompt_carries_its_time() {
        let at = 1_762_700_000_000;
        let member = TranscriptSender { kind: SenderKind::Member, id: "u2".into(), name: "Alex".into(), link: None };
        let other = layout(TranscriptSide::Other, Some(member), true, true);
        let prompt = RowFacts { at: Some(at), ..facts(RowContent::Prompt("Ship it"), Some(&other), 0., false) };
        let ChatRowInput::Message { time, .. } = chat_row(&prompt) else { panic!("a bubble") };
        assert_eq!(time, crate::session_chat::clock_ms(at));
        assert!(!time.is_empty());
        let narration = RowFacts { at: Some(at), ..facts(RowContent::Narration("Done."), None, 0., false) };
        let ChatRowInput::Message { time, .. } = chat_row(&narration) else { panic!("a bubble") };
        assert!(time.is_empty());
    }

    #[test]
    fn a_gap_lands_on_the_documents_spacers() {
        assert_eq!([0., 3., 8., 12., 16., 10.5, 20.].map(spacer), [0, 3, 8, 12, 16, 12, 16]);
    }

    #[test]
    fn the_document_prunes_only_nodes_it_has() {
        let doc = uicore_docs::doc("chat").expect("the chat");
        for id in PRUNED.iter().chain(&[SCROLL, HOST_NODE, "chat_row", "chat"]) {
            assert!(doc.source.contains(&format!("\"id\": \"{id}\"")), "{id}");
        }
        let _ = document();
    }
}
