//! #EXP-2071 (`fixtures/chat-express.json` DRAW, #EXP-2067): the IDE's
//! transcript draws the Express blocks of an agent's text. Only agent rows
//! come here (a feed `narration` row, a synced agent message of the tail):
//! one [`ExpressRow`] per row key (`synced_row_key` / `feed_row_key`) fed
//! the row's text as it grows, final per the spec; `None` = no block, the
//! row draws exactly as today (no compiler runs for it).
//!
//! A row with blocks becomes its segments in order: each prose segment THE
//! MessageBubble (the caller's, markdown as today) and each block one item
//! in the agent's column ([`block_items`]): one surface card frame
//! ([`crate::surface_card::surface_frame`], no title) per surface the block
//! created, painted by THE plan card surface painter
//! ([`crate::plan_card::surface_options`]) as an eui-gpui [`SurfaceView`]
//! fed the block's A2UI messages (rebuilt on a `generation` bump);
//! `layout.blockGap` between them. A `failed` / `tooMany` / `tooLarge` block
//! draws its surfaces then THE inline marker `copy.blockFailed`; an `open`
//! block with no surface yet draws THE skeleton frame. ENTER
//! (`surfaceCard.enter`) only for a surface created while its row streamed
//! (gpui's limits as [`crate::surface_card`] documents: the frame's fade,
//! rise and glow, no children stagger); history, a reopen or a re-split
//! never re-enters. Every press = [`crate::surface_card::ActionHost`]
//! (`surface_action` into the session whose transcript draws the row).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use coding::express_blocks::{BlockState, BlockStatus, ExpressRow, RowSegment};
use domain::chat_express::{has_express_block, without_held_tail, CHAT_EXPRESS};
use eui_gpui::view::SurfaceView;
use gpui::{px, AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::Value;

/// One row's split, cached by its text and finality.
struct RowEntry {
    row: ExpressRow,
    text: String,
    is_final: bool,
    segments: Option<Rc<Vec<RowSegment>>>,
    /// The row was first split while it streamed: the surfaces its blocks
    /// create from then on arrive LIVE (ENTER).
    live: bool,
}

/// One block surface's view and what it was fed.
#[derive(Default)]
struct SurfaceSlot {
    generation: u32,
    /// `block.messages` consumed so far (every surface's).
    consumed: usize,
    view: Option<Entity<SurfaceView>>,
    /// Messages for a view still being built (applied once it exists).
    pending: Vec<Value>,
    building: bool,
}

/// What a block surface's view takes this frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Feed {
    /// (Re)build the view from these messages (a first sight, a restart).
    Build(Vec<Value>),
    /// Apply these to the existing view.
    Push(Vec<Value>),
    Keep,
}

/// The transcript's Express state (one per app: rows keep their surfaces
/// across reopens, so history never re-enters).
#[derive(Default)]
pub(crate) struct ExpressRows {
    rows: HashMap<String, RowEntry>,
    /// By `<block key>/<surface id>`.
    surfaces: HashMap<String, SurfaceSlot>,
    arrivals: HashMap<String, Instant>,
    /// Bumps whenever a surface's view changes (the list re-measures).
    revision: u64,
}

impl gpui::Global for ExpressRows {}

/// The surface a server→client message is about.
pub(crate) fn message_surface(message: &Value) -> Option<&str> {
    message.as_object()?.values().find_map(|body| body.get("surfaceId")?.as_str())
}

fn slot_key(block: &str, surface: &str) -> String {
    format!("{block}/{surface}")
}

impl ExpressRows {
    /// The row's segments for `text` so far (`None` = no block: draw as
    /// today). Re-split only when the text or its finality moved.
    pub(crate) fn split(&mut self, row_key: &str, text: &str, is_final: bool) -> Option<Rc<Vec<RowSegment>>> {
        // The gate first: a row without a block (and no held tag prefix)
        // never builds a row model.
        if !self.rows.contains_key(row_key) && !has_express_block(text) && without_held_tail(text, is_final).len() == text.len() {
            return None;
        }
        let fresh = !self.rows.contains_key(row_key);
        let entry = self.rows.entry(row_key.to_string()).or_insert_with(|| RowEntry {
            row: ExpressRow::new(row_key),
            text: String::new(),
            is_final: false,
            segments: None,
            live: !is_final,
        });
        if fresh || entry.text != text || entry.is_final != is_final {
            entry.segments = entry.row.update(text, is_final).map(Rc::new);
            entry.text = text.to_string();
            entry.is_final = is_final;
        }
        entry.segments.clone()
    }

    /// The row's surfaces arrive live (its first split streamed).
    pub(crate) fn live(&self, row_key: &str) -> bool {
        self.rows.get(row_key).is_some_and(|entry| entry.live)
    }

    /// What `surface` of `block` takes this frame; records a live arrival.
    pub(crate) fn feed(&mut self, block: &BlockState, surface: &str, live: bool, now: Instant) -> Feed {
        let key = slot_key(&block.key, surface);
        let mine = |messages: &[Value]| -> Vec<Value> {
            messages.iter().filter(|m| message_surface(m) == Some(surface)).cloned().collect()
        };
        let fresh = !self.surfaces.contains_key(&key);
        let slot = self.surfaces.entry(key.clone()).or_default();
        if fresh || slot.generation != block.generation || block.messages.len() < slot.consumed {
            *slot = SurfaceSlot { generation: block.generation, consumed: block.messages.len(), ..SurfaceSlot::default() };
            if fresh && live {
                self.arrivals.insert(key, now);
            }
            self.revision += 1;
            return Feed::Build(mine(&block.messages));
        }
        if block.messages.len() > slot.consumed {
            let new = mine(&block.messages[slot.consumed..]);
            slot.consumed = block.messages.len();
            if new.is_empty() {
                return Feed::Keep;
            }
            self.revision += 1;
            return Feed::Push(new);
        }
        Feed::Keep
    }
}

/// [`ExpressRows::split`] on the app's state.
pub(crate) fn split(row_key: &str, text: &str, is_final: bool, cx: &mut App) -> Option<Rc<Vec<RowSegment>>> {
    cx.default_global::<ExpressRows>().split(row_key, text, is_final)
}

/// A height fingerprint for a row of `text`: 0 without a block, else the
/// surfaces' revision (their views land and grow after the row's text).
pub(crate) fn row_extra(text: &str, cx: &App) -> u64 {
    if !has_express_block(text) {
        return 0;
    }
    cx.try_global::<ExpressRows>().map_or(1, |rows| rows.revision + 1)
}

/// The session a row's presses post into, and the synced row's id.
#[derive(Clone, Debug)]
pub(crate) struct ExpressTarget {
    pub session_id: String,
    pub message_id: Option<String>,
    pub row_key: String,
}

/// One block's items in the agent's column: a frame per surface (the
/// skeleton while an open block has none), THE marker under a failed one.
pub(crate) fn block_items(block: &BlockState, target: &ExpressTarget, window: &Window, cx: &mut App) -> Vec<AnyElement> {
    let live = cx.default_global::<ExpressRows>().live(&target.row_key);
    let reduced = theme::motion::reduce_motion();
    let now = Instant::now();
    let mut items = Vec::new();
    for surface in &block.surfaces {
        let key = slot_key(&block.key, surface);
        match cx.default_global::<ExpressRows>().feed(block, surface, live, now) {
            Feed::Build(messages) => build(&key, surface, block, target, messages, window, cx),
            Feed::Push(messages) => push(&key, messages, cx),
            Feed::Keep => {}
        }
        let state = cx.default_global::<ExpressRows>();
        let view = state.surfaces.get(&key).and_then(|slot| slot.view.clone());
        let arrival = state.arrivals.get(&key).copied();
        let look = arrival.and_then(|at| crate::surface_card::enter_frame(at.elapsed(), reduced));
        if look.is_some() {
            window.request_animation_frame();
        } else if arrival.is_some() {
            cx.default_global::<ExpressRows>().arrivals.remove(&key);
        }
        items.push(crate::surface_card::surface_frame(&format!("express-{key}"), None, view.map(|view| view.into_any_element()), look, cx));
    }
    if block.surfaces.is_empty() && block.status == BlockStatus::Open {
        // THE surface card skeleton (feedback-states.json): the frame's
        // skeleton-height box until the block's first surface compiles.
        items.push(crate::surface_card::surface_frame(&format!("express-{}", block.key), None, None, None, cx));
    }
    if block.status.marks() {
        items.push(crate::surface_card::transcript_marker(
            SharedString::from(format!("express-failed-{}", block.key)),
            &CHAT_EXPRESS.copy.block_failed,
            None,
            cx,
        ));
    }
    items
}

/// Builds the surface's view on the next frame from `messages` (plus any
/// pushed meanwhile).
fn build(key: &str, surface: &str, block: &BlockState, target: &ExpressTarget, messages: Vec<Value>, window: &Window, cx: &mut App) {
    if let Some(slot) = cx.default_global::<ExpressRows>().surfaces.get_mut(key) {
        slot.pending = messages;
        slot.building = true;
    }
    let (key, surface, generation) = (key.to_string(), surface.to_string(), block.generation);
    let source = crate::surface_card::ActionSource::Block { key: block.key.clone(), message_id: target.message_id.clone() };
    let session_id = target.session_id.clone();
    window.defer(cx, move |window, cx| {
        let host = Rc::new(crate::surface_card::ActionHost::new(&session_id, source, cx));
        let options = crate::plan_card::surface_options(&surface, host);
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        let state = cx.default_global::<ExpressRows>();
        let Some(slot) = state.surfaces.get_mut(&key).filter(|slot| slot.generation == generation && slot.building) else {
            return;
        };
        slot.building = false;
        slot.view = Some(view.clone());
        let messages = std::mem::take(&mut slot.pending);
        state.revision += 1;
        apply(&view, &key, &messages, cx);
        cx.refresh_windows();
    });
}

fn push(key: &str, messages: Vec<Value>, cx: &mut App) {
    let Some(slot) = cx.default_global::<ExpressRows>().surfaces.get_mut(key) else { return };
    match slot.view.clone() {
        Some(view) => apply(&view, key, &messages, cx),
        None => slot.pending.extend(messages),
    }
}

fn apply(view: &Entity<SurfaceView>, key: &str, messages: &[Value], cx: &mut App) {
    view.update(cx, |view, cx| {
        for message in messages {
            if let Err(error) = view.apply(message, cx) {
                log::warn!("[express] {key}: {error}");
            }
        }
    });
}

/// THE row's column: its segments in order, `prose(index, text, first,
/// last_prose)` drawing each prose bubble, the blocks' items between,
/// `layout.blockGap` apart.
pub(crate) fn row_column(
    id: &str,
    segments: &[RowSegment],
    target: &ExpressTarget,
    prose: &mut dyn FnMut(usize, &str, bool, bool, &mut App) -> AnyElement,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let last_prose = segments.iter().rposition(|segment| matches!(segment, RowSegment::Prose(_)));
    let mut children = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        match segment {
            RowSegment::Prose(text) => children.push(prose(index, text, index == 0, Some(index) == last_prose, cx)),
            RowSegment::Block(block) => children.extend(block_items(block, target, window, cx)),
        }
    }
    let _ = id;
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(CHAT_EXPRESS.layout.block_gap))
        .children(children)
        .into_any_element()
}

/// Find in chat's mark over a row split into prose bubbles: each bubble
/// takes the query, the CURRENT hit only on the bubble that holds it
/// (`current` counts across the row's prose in order).
pub(crate) fn segment_marks(segments: &[RowSegment], mark: Option<&Value>) -> Vec<Option<Value>> {
    let current = mark.and_then(|mark| mark.get("current")).and_then(Value::as_u64);
    let query = mark.and_then(|mark| mark.get("query")).and_then(Value::as_str).unwrap_or_default();
    let mut before = 0u64;
    segments
        .iter()
        .map(|segment| {
            let (RowSegment::Prose(text), Some(mark)) = (segment, mark) else { return None };
            let count = eui_gpui::paint::highlight::markdown_highlight_count(text, query) as u64;
            let local = current.filter(|current| *current >= before && *current < before + count).map(|current| current - before);
            before += count;
            Some(match local {
                Some(local) => serde_json::json!({"query": query, "current": local}),
                None => serde_json::json!({"query": mark["query"]}),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: &str = "Here is the build:\n<a2ui>\nroot = Stack(children=[title])\ntitle = Text(text=\"Build 1482 is green\")\n</a2ui>\nShip it?";

    fn blocks(segments: &[RowSegment]) -> Vec<&BlockState> {
        segments.iter().filter_map(|s| if let RowSegment::Block(b) = s { Some(b) } else { None }).collect()
    }

    /// Prose without a tag never builds a row model.
    #[test]
    fn a_row_without_a_block_draws_as_today() {
        let mut rows = ExpressRows::default();
        assert!(rows.split("m:m1", "Merged **#212**.", true).is_none());
        assert!(rows.split("m:m1", "Wrap it in `<a2ui>` tags.", true).is_none());
        assert!(rows.rows.is_empty());
    }

    /// A synced row: prose, ONE block keyed `<rowKey>#1`, prose; history
    /// (final at first sight) never enters.
    #[test]
    fn a_synced_row_splits_into_bubbles_and_a_block() {
        let mut rows = ExpressRows::default();
        let segments = rows.split("m:m1", ROW, true).unwrap();
        assert_eq!(segments.len(), 3);
        assert!(matches!(&segments[0], RowSegment::Prose(t) if t == "Here is the build:"));
        assert!(matches!(&segments[2], RowSegment::Prose(t) if t == "Ship it?"));
        let block = blocks(&segments)[0].clone();
        assert_eq!((block.key.as_str(), block.status), ("m:m1#1", BlockStatus::Ok));
        assert_eq!(block.surfaces, vec!["m:m1#1".to_string()]);
        assert!(!rows.live("m:m1"));
        let Feed::Build(messages) = rows.feed(&block, "m:m1#1", false, Instant::now()) else { panic!("a build") };
        assert!(messages.iter().all(|m| message_surface(m) == Some("m:m1#1")));
        assert!(!messages.is_empty());
        assert!(rows.arrivals.is_empty());
        // The same text again: the cached split, nothing new to feed.
        let again = rows.split("m:m1", ROW, true).unwrap();
        assert!(Rc::ptr_eq(&segments, &again));
        assert_eq!(rows.feed(&block, "m:m1#1", false, Instant::now()), Feed::Keep);
    }

    /// A feed row streams: the held tag prefix draws nothing, the block is
    /// `open` with what compiled, later fragments PUSH only the new
    /// messages, and its surface arrived live (ENTER).
    #[test]
    fn a_streaming_row_pushes_its_new_messages_and_enters() {
        let mut rows = ExpressRows::default();
        let key = "n:s1:msg_01";
        let held = rows.split(key, "Checking.\n<a2", false).unwrap();
        assert!(matches!(&held[..], [RowSegment::Prose(t)] if t == "Checking."));
        let open = rows.split(key, "Checking.\n<a2ui>\nroot = Stack(children=[a, b])\na = Text(text=\"Buil", false).unwrap();
        let block = blocks(&open)[0].clone();
        assert_eq!(block.status, BlockStatus::Open);
        assert!(rows.live(key));
        let surface = block.surfaces[0].clone();
        assert!(matches!(rows.feed(&block, &surface, true, Instant::now()), Feed::Build(_)));
        assert_eq!(rows.arrivals.len(), 1);
        let done = rows
            .split(key, "Checking.\n<a2ui>\nroot = Stack(children=[a, b])\na = Text(text=\"Build\")\nb = Text(text=\"Green\")\n</a2ui>\nShipping.", false)
            .unwrap();
        let block = blocks(&done)[0].clone();
        assert_eq!(block.status, BlockStatus::Ok);
        let Feed::Push(new) = rows.feed(&block, &surface, true, Instant::now()) else { panic!("a push") };
        assert!(!new.is_empty());
        assert_eq!(rows.feed(&block, &surface, true, Instant::now()), Feed::Keep);
    }

    /// An edit that is not an append restarts the block: a new generation
    /// rebuilds its view.
    #[test]
    fn a_restarted_block_rebuilds() {
        let mut rows = ExpressRows::default();
        let first = rows.split("m:m2", "<a2ui>\nroot = Text(text=\"One\")\n</a2ui>", true).unwrap();
        let block = blocks(&first)[0].clone();
        assert!(matches!(rows.feed(&block, &block.surfaces[0], false, Instant::now()), Feed::Build(_)));
        let edited = rows.split("m:m2", "<a2ui>\nroot = Text(text=\"Two\")\n</a2ui>", true).unwrap();
        let block = blocks(&edited)[0].clone();
        assert_eq!(block.generation, 1);
        assert!(matches!(rows.feed(&block, &block.surfaces[0], false, Instant::now()), Feed::Build(_)));
    }

    /// A failed block marks; a final row's unclosed block fails.
    #[test]
    fn a_failed_block_marks() {
        let mut rows = ExpressRows::default();
        let out = rows.split("m:m3", "A\n<a2ui>\nroot = Nope(x=1)\n</a2ui>", true).unwrap();
        let block = blocks(&out)[0];
        assert_eq!(block.status, BlockStatus::Failed);
        assert!(block.status.marks());
        assert!(block.surfaces.is_empty());
    }

    /// Find's current hit lands on the bubble that holds it.
    #[test]
    fn find_marks_count_across_the_bubbles() {
        let segments = vec![
            RowSegment::Prose("green build".into()),
            RowSegment::Prose("green light, green go".into()),
        ];
        let marks = segment_marks(&segments, Some(&serde_json::json!({"query": "green", "current": 2})));
        assert_eq!(marks[0], Some(serde_json::json!({"query": "green"})));
        assert_eq!(marks[1], Some(serde_json::json!({"query": "green", "current": 1})));
        assert_eq!(segment_marks(&segments, None), vec![None, None]);
    }
}
