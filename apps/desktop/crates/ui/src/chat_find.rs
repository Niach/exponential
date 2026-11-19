//! #EDE-129 (packages/domain-contract/fixtures/find-in-chat.json, the spec
//! ×4; `domain::find_in_chat` the rules): find in chat on the IDE's
//! transcript. A child module of `steer_viewer` (it reads the view's rows):
//! the bar under the chat header (the search field, "n of m", previous,
//! next, close), the stops over the LOADED rows (a bubble's markdown, a
//! plain tool line's title + its output's stop, a folded run's members)
//! plus the chat store's older hits, the current stop kept while answers
//! arrive, and the landing (open the run, page an older one in, centre).

use std::time::Duration;

use domain::find_in_chat::{self as find, FindOlderHit, FindRow, FindSearchPhase, FindStep, FindStop};
use gpui::{div, px, AnyElement, App, AppContext as _, Entity, Focusable as _, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};
use steer::feed::{FeedItem, FeedItemId, FeedKind, FeedRowSpec};

use super::SteerSessionView;
use crate::icons::registry;

gpui::actions!(chat_find, [FindInChat]);

/// Ctrl/Cmd+F inside a session view = find in chat; outside it stays THE
/// global search (`search_sheet`), Ctrl/Cmd+K stays global everywhere.
pub(crate) fn find_key_bindings() -> Vec<gpui::KeyBinding> {
    #[cfg(target_os = "macos")]
    let chord = "cmd-f";
    #[cfg(not(target_os = "macos"))]
    let chord = "ctrl-f";
    vec![gpui::KeyBinding::new(chord, FindInChat, Some(KEY_CONTEXT))]
}

/// The session view's key context (its root carries it).
pub(crate) const KEY_CONTEXT: &str = "SteerSession";

/// A `…` menu's Search in chat: the chat whose view opens its bar next
/// (`None` = whichever session view renders next in the window).
#[derive(Default)]
struct FindRequest(Option<Option<String>>);

impl gpui::Global for FindRequest {}

/// The `…` entry's press: `session_id` = the chat's root (None = the
/// screen's own view, an agent's conversation).
pub(crate) fn request_find(session_id: Option<String>, window: &mut Window, cx: &mut App) {
    cx.set_global(FindRequest(Some(session_id)));
    window.refresh();
}

/// The `…` entry's trailing shortcut hint.
pub(crate) fn find_shortcut_hint(cx: &App) -> AnyElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(find::shortcut(cfg!(target_os = "macos")).to_string())
        .into_any_element()
}

/// The bar's state while it is open.
pub(crate) struct ChatFind {
    input: Entity<InputState>,
    query: String,
    stops: Vec<FindStop>,
    current: Option<FindStop>,
    older: Vec<FindOlderHit>,
    store: FindSearchPhase,
    /// The last query landed on (a new one starts at the newest stop).
    started: String,
    /// The query the stops were counted for (the counter shows nothing
    /// for a newer one yet, never the old query's "No results").
    counted: String,
    /// A step asked for a landing the next frame runs.
    land: bool,
    debounce: Option<gpui::Task<()>>,
    _subscription: gpui::Subscription,
}

/// A loaded row's find facts: its stop row, its list row (`None` = a
/// placeholder's bubble, no list to scroll), its run.
struct Located {
    row: FindRow,
    ix: Option<usize>,
    run: Option<FeedItemId>,
}

/// #EXP-1975: an own bubble's key: its stored row's (`m:`), else its
/// outbox entry's (an unsent one).
pub(crate) fn own_bubble_key(bubble: &crate::session_outbox::OwnBubble) -> String {
    match (&bubble.message_id, &bubble.client_id) {
        (Some(id), _) => find::message_key(id),
        (None, Some(client_id)) => format!("o:{client_id}"),
        (None, None) => find::message_key(&bubble.key),
    }
}

fn item_key(session_id: &str, item: &FeedItem) -> String {
    match item.seq {
        Some(seq) => find::event_key(session_id, seq as i64),
        None => format!("l:{}", item.id),
    }
}

/// A plain tool line's title (what the row paints), None = drawn another way.
fn tool_title(item: &FeedItem) -> Option<String> {
    let FeedKind::Tool { name, detail, settled, failed, tool_kind, .. } = &item.kind else { return None };
    if steer::script_card::is_script_tool(name) || steer::exp_tool_display(name, *settled).is_some() {
        return None;
    }
    let kind = tool_kind.map(|kind| kind.as_str()).unwrap_or("other");
    Some(steer::trace::trace_tool(kind, name, detail.as_deref(), *settled, *failed).title)
}

fn tool_output_matches(item: &FeedItem, query: &str) -> usize {
    match &item.kind {
        FeedKind::Tool { output: Some(output), .. } => eui_core::text_highlight::highlight_count(output, query),
        _ => 0,
    }
}

impl SteerSessionView {
    /// #EXP-1975: the genUI `highlight` a row the IDE draws itself (an own
    /// bubble, a host narration) paints for find in chat (`None` = closed).
    pub(crate) fn find_mark(&self, key: &str) -> Option<serde_json::Value> {
        let open = self.find.as_ref().filter(|open| !open.query.is_empty())?;
        let current = open.current.as_ref().filter(|stop| stop.loaded && !stop.output && stop.key == key).map(|stop| stop.local);
        Some(match current {
            Some(local) => serde_json::json!({"query": open.query, "current": local}),
            None => serde_json::json!({"query": open.query}),
        })
    }

    /// #EXP-1975: a feed item's find key while the bar has a query.
    pub(crate) fn find_item_key(&self, item: &FeedItem) -> Option<String> {
        self.find.as_ref().filter(|open| !open.query.is_empty())?;
        Some(item_key(&self.session_id, item))
    }

    /// Opens the bar (or re-focuses it with its query selected).
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(open) = &self.find {
            let input = open.input.clone();
            input.update(cx, |state, cx| state.select_all(window, cx));
            input.read(cx).focus_handle(cx).focus(window, cx);
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(find::copy().placeholder.clone()));
        let subscription = cx.subscribe_in(&input, window, |this: &mut Self, input, event: &InputEvent, _window, cx| {
            if matches!(event, InputEvent::Change) {
                let raw = input.read(cx).value().to_string();
                this.find_debounce(raw, cx);
            }
        });
        input.read(cx).focus_handle(cx).focus(window, cx);
        self.find = Some(ChatFind {
            input,
            query: String::new(),
            stops: Vec::new(),
            current: None,
            older: Vec::new(),
            store: FindSearchPhase::None,
            started: String::new(),
            counted: String::new(),
            land: false,
            debounce: None,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Escape, ×: every mark goes, the scroll and the opened runs stay.
    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.find.take().is_some() {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    /// Takes a `…` press meant for this view.
    pub(crate) fn take_find_request(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // DEV ONLY (`EXP_DEV_FIND=<query>`, the pictures): open the bar on
        // the query once the rows are in.
        if self.find.is_none() && (!self.rows.is_empty() || !self.synced_tail.is_empty() || !self.own_tail.is_empty() || self.feed.is_empty()) {
            if let Some(query) = crate::screens::take_dev_find() {
                self.open_find(window, cx);
                if let Some(open) = &self.find {
                    let input = open.input.clone();
                    input.update(cx, |state, cx| state.set_value(query.clone(), window, cx));
                }
                self.set_find_query(find::query(&query), cx);
            }
        }
        let wanted = cx.try_global::<FindRequest>().and_then(|request| request.0.clone());
        let Some(target) = wanted else { return };
        if target.as_deref().is_some_and(|id| id != self.session_id) {
            return;
        }
        cx.set_global(FindRequest(None));
        self.open_find(window, cx);
    }

    fn find_debounce(&mut self, raw: String, cx: &mut gpui::Context<Self>) {
        let query = find::query(&raw);
        let Some(open) = self.find.as_mut() else { return };
        open.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(find::debounce_ms())).await;
            let _ = this.update(cx, |this, cx| this.set_find_query(query, cx));
        }));
    }

    fn set_find_query(&mut self, query: String, cx: &mut gpui::Context<Self>) {
        let root = crate::session_data::session(&self.session_id, cx).map(|row| row.root_id_or_self().to_string());
        let session_id = self.session_id.clone();
        let Some(open) = self.find.as_mut() else { return };
        if open.query == query {
            return;
        }
        open.query = query.clone();
        open.current = None;
        open.older.clear();
        open.store = FindSearchPhase::None;
        if !query.is_empty() {
            // The chat store's hits of THIS session below the loaded rows.
            if let (Some(store), Some(root)) = (crate::chat_store::store(cx), root) {
                open.store = FindSearchPhase::Searching;
                let task = cx.background_executor().spawn(async move { store.search_chat(&root, &query, find::hits_max()).ok() });
                cx.spawn(async move |this, cx| {
                    let result = task.await;
                    let _ = this.update(cx, |this, cx| {
                        let Some(open) = this.find.as_mut() else { return };
                        open.store = FindSearchPhase::Done;
                        open.older = result
                            .map(|result| result.hits)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|hit| hit.session_id == session_id)
                            .filter_map(|hit| {
                                let seq = hit.seq?;
                                Some(FindOlderHit { key: find::event_key(&session_id, seq as i64), output: hit.kind == chat_store::DocKind::ToolOutput })
                            })
                            .collect();
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }

    /// The loaded rows' find facts for `query`, top to bottom.
    fn find_located(&self, query: &str, cx: &App) -> Vec<Located> {
        let items = self.feed.items();
        let agent = self.agent();
        let mut out = Vec::new();
        let tool = |item: &FeedItem, ix: usize, run: Option<FeedItemId>| {
            let title = tool_title(item)?;
            let output = tool_output_matches(item, query);
            Some(Located {
                row: FindRow {
                    key: item_key(&self.session_id, item),
                    count: eui_core::text_highlight::highlight_count(&title, query),
                    group: run.map(|run| run.to_string()),
                    output_hit: output > 0,
                },
                ix: Some(ix),
                run,
            })
        };
        // #EXP-1975 (find-in-chat.json NO TRANSCRIPT): with neither feed
        // nor synced tail the view shows its placeholder under the sent
        // inputs and the own tail: those bubbles are its loaded rows.
        if self.feed.is_empty() && self.synced_tail.is_empty() {
            if !self.row_ended() {
                for (id, text, _) in self.no_feed_input_rows(cx) {
                    let count = eui_gpui::paint::highlight::markdown_highlight_count(&text, query);
                    out.push(Located { row: FindRow { key: find::message_key(&id), count, group: None, output_hit: false }, ix: None, run: None });
                }
            }
            for bubble in &self.own_tail {
                let count = eui_gpui::paint::highlight::markdown_highlight_count(&bubble.text, query);
                out.push(Located { row: FindRow { key: own_bubble_key(bubble), count, group: None, output_hit: false }, ix: None, run: None });
            }
            return out;
        }
        for (ix, spec) in self.rows.iter().enumerate() {
            match spec {
                FeedRowSpec::Single { item, .. } => {
                    let Some(item) = items.get(*item) else { continue };
                    let text = match &item.kind {
                        FeedKind::Narration { text, .. } => Some(text),
                        FeedKind::UserMessage { text, .. }
                            if domain::chats::command_divider(text).is_none() && steer::commands::parse_command(text, agent).is_none() =>
                        {
                            Some(text)
                        }
                        _ => None,
                    };
                    if let Some(text) = text {
                        let count = eui_gpui::paint::highlight::markdown_highlight_count(text, query);
                        out.push(Located { row: FindRow { key: item_key(&self.session_id, item), count, group: None, output_hit: false }, ix: Some(ix), run: None });
                    } else if item.is_tool() && !matches!(&item.kind, FeedKind::Tool { diff: Some(diff), .. } if !diff.is_empty()) {
                        out.extend(tool(item, ix, None));
                    }
                }
                FeedRowSpec::ToolRun { id, items: members } => {
                    for item in members.iter().filter_map(|index| items.get(*index)) {
                        out.extend(tool(item, ix, Some(*id)));
                    }
                }
                _ => {}
            }
        }
        // #FEED-260: the synced tail's bubbles (a not-live feed), after it.
        for (offset, row) in self.synced_tail.iter().enumerate() {
            let count = eui_gpui::paint::highlight::markdown_highlight_count(&row.text, query);
            out.push(Located { row: FindRow { key: find::message_key(&row.id), count, group: None, output_hit: false }, ix: Some(self.rows.len() + offset), run: None });
        }
        // #EXP-1975: then the viewer's own bubbles (the unconsumed rows, the
        // outbox), after the orphan and trailing cards.
        let before_own = self.rows.len() + self.synced_tail.len() + self.orphan_workflows.len() + self.conv_tail().len();
        for (offset, bubble) in self.own_tail.iter().enumerate() {
            let count = eui_gpui::paint::highlight::markdown_highlight_count(&bubble.text, query);
            out.push(Located { row: FindRow { key: own_bubble_key(bubble), count, group: None, output_hit: false }, ix: Some(before_own + offset), run: None });
        }
        out
    }

    /// Recomputes the stops (the current one kept), lands a pending step,
    /// and returns the paint for the bubbles. Called each render.
    pub(crate) fn find_frame(&mut self, cx: &mut gpui::Context<Self>) -> Option<crate::chat_tab_transcript::FindPaint> {
        let query = self.find.as_ref().map(|open| open.query.clone()).filter(|query| !query.is_empty())?;
        let located = self.find_located(&query, cx);
        let rows: Vec<FindRow> = located.iter().map(|located| located.row.clone()).collect();
        let lowest = self.feed.items().iter().find_map(|item| item.seq);
        let open = self.find.as_mut()?;
        let below = |hit: &&FindOlderHit| {
            let seq: Option<u64> = hit.key.rsplit(':').next().and_then(|seq| seq.parse().ok());
            matches!((seq, lowest), (Some(seq), Some(lowest)) if seq < lowest) || lowest.is_none()
        };
        let older: Vec<FindOlderHit> = open.older.iter().filter(below).cloned().collect();
        let stops = find::stops(&rows, &older);
        let mut index = open
            .current
            .as_ref()
            .and_then(|was| stops.iter().position(|stop| stop.key == was.key && stop.local == was.local && stop.output == was.output));
        if open.started != query && !stops.is_empty() {
            open.started = query.clone();
            index = find::step(None, stops.len(), FindStep::Start);
            open.land = true;
        }
        let index = index.or_else(|| stops.len().checked_sub(1));
        let current = index.and_then(|index| stops.get(index).cloned());
        // The bar renders above the transcript: a counter that moved in this
        // frame shows on the next one (#EXP-1975).
        if open.stops != stops || open.current != current || open.counted != query {
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        open.counted = query.clone();
        open.current = current;
        open.stops = stops;
        if std::mem::take(&mut open.land) {
            if let Some(stop) = open.current.clone() {
                let landing = find::land(&stop);
                if landing.page_in {
                    // Older rows page in; the next frames land once it is loaded.
                    open.land = true;
                    if self.can_grow_window() {
                        self.extend_window = true;
                    } else {
                        self.request_older_page();
                    }
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        let _ = this.update(cx, |_, cx| cx.notify());
                    })
                    .detach();
                } else if let Some(at) = located.iter().find(|located| located.row.key == stop.key) {
                    if let Some(run) = at.run {
                        self.expanded_groups.insert(run);
                    }
                    if let Some(ix) = at.ix {
                        self.chat.pin(true);
                        self.chat.reveal(ix, "center");
                    }
                }
            }
        }
        let open = self.find.as_ref()?;
        let current = open.current.as_ref().filter(|stop| stop.loaded && !stop.output).and_then(|stop| {
            let at = located.iter().find(|located| located.row.key == stop.key)?;
            let key = self.row_keys.get(at.ix?)?;
            Some((crate::chat_tab_transcript::row_key(key.id), stop.local))
        });
        Some(crate::chat_tab_transcript::FindPaint { query, current })
    }

    fn find_step(&mut self, step: FindStep, cx: &mut gpui::Context<Self>) {
        let Some(open) = self.find.as_mut() else { return };
        let index = open
            .current
            .as_ref()
            .and_then(|was| open.stops.iter().position(|stop| stop == was));
        if let Some(next) = find::step(index, open.stops.len(), step) {
            open.current = open.stops.get(next).cloned();
            open.land = true;
        }
        cx.notify();
    }

    /// A tool line's title marks (the current one when it is this row's).
    pub(crate) fn find_title_marks(&self, item: &FeedItem, title: &str) -> Vec<(std::ops::Range<usize>, gpui::Hsla)> {
        let Some(open) = self.find.as_ref().filter(|open| !open.query.is_empty()) else { return Vec::new() };
        let key = item_key(&self.session_id, item);
        let current = open.current.as_ref().filter(|stop| stop.key == key && !stop.output).map(|stop| stop.local);
        let theme = eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID);
        let colors = eui_gpui::paint::highlight::MarkColors::of(theme.as_deref(), eui_core::theme::Mode::Dark);
        eui_core::text_highlight::highlight_ranges(title, &open.query)
            .into_iter()
            .enumerate()
            .map(|(ix, range)| (range, colors.fill(current == Some(ix))))
            .collect()
    }

    /// A tool line's "n matches in output" hint (none without a match).
    pub(crate) fn find_output_hint(&self, item: &FeedItem) -> Option<String> {
        let open = self.find.as_ref().filter(|open| !open.query.is_empty())?;
        tool_title(item)?;
        let n = tool_output_matches(item, &open.query);
        (n > 0).then(|| find::output_hint(n))
    }

    /// THE bar (wide: under the chat header, across the chat column) and
    /// its status line.
    pub(crate) fn render_find_bar(&self, window: &mut Window, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let open = self.find.as_ref()?;
        let ids = find::test_ids();
        let copy = find::copy();
        let muted = cx.theme().muted_foreground;
        let done = open.store != FindSearchPhase::Searching;
        let current = open.current.as_ref().and_then(|was| open.stops.iter().position(|stop| stop == was));
        let counter = if open.query.is_empty() || open.counted != open.query { String::new() } else { find::counter(current, open.stops.len(), done, false) };
        let status = (!open.query.is_empty()).then(|| find::status(open.store, FindSearchPhase::None, "")).flatten();
        let button = |id: &str, icon, label: &str, cx: &App| {
            crate::controls::ghost_icon_button(gpui::SharedString::from(id.to_string()), Icon::new(icon), cx).accessibility_label(label.to_string())
        };
        let bar = h_flex()
            .id(gpui::SharedString::from(ids.bar.clone()))
            .w_full()
            .gap_2()
            .px_6()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted.opacity(0.3))
            // The field owns Enter (previous), Shift+Enter (next), Escape.
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                let key = match keystroke.key.as_str() {
                    "enter" => "Enter",
                    "escape" => "Escape",
                    other => other,
                };
                match find::key(key, keystroke.modifiers.secondary(cx), keystroke.modifiers.shift, true, true) {
                    Some(find::FindKeyAction::Previous) => this.find_step(FindStep::Previous, cx),
                    Some(find::FindKeyAction::Next) => this.find_step(FindStep::Next, cx),
                    Some(find::FindKeyAction::Close) => this.close_find(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                div()
                    .id(gpui::SharedString::from(ids.input.clone()))
                    .w(px(384.))
                    .flex_shrink_0()
                    .child(crate::controls::search_field(&open.input, crate::controls::SearchFieldSize::Md, window, cx)),
            )
            .child(div().id(gpui::SharedString::from(ids.counter.clone())).text_sm().text_color(muted).child(counter))
            .child(button(&ids.previous, registry::UI_CHEVRON_UP, &copy.previous, cx).on_click(cx.listener(|this, _, _, cx| this.find_step(FindStep::Previous, cx))))
            .child(button(&ids.next, registry::UI_CHEVRON_DOWN, &copy.next, cx).on_click(cx.listener(|this, _, _, cx| this.find_step(FindStep::Next, cx))))
            .child(button(&ids.close, registry::UI_CLOSE, &copy.close, cx).on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))));
        Some(
            v_flex()
                .w_full()
                .flex_shrink_0()
                .child(bar)
                .children(status.map(|status| {
                    div().id(gpui::SharedString::from(ids.status.clone())).px_6().pt_2().text_xs().text_color(muted).child(status)
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::own_bubble_key;
    use crate::session_outbox::OwnBubble;
    use domain::message_delivery::DeliveryState;

    fn bubble(message_id: Option<&str>, client_id: Option<&str>) -> OwnBubble {
        OwnBubble {
            key: message_id.or(client_id).unwrap_or_default().to_string(),
            text: "line 2".to_string(),
            created_at: String::new(),
            state: DeliveryState::Read,
            stored: message_id.is_some(),
            message_id: message_id.map(str::to_string),
            client_id: client_id.map(str::to_string),
        }
    }

    /// #EXP-1975 (find-in-chat.json): an own bubble's stored row is `m:`,
    /// an outbox entry with no row yet `o:<clientId>`.
    #[test]
    fn own_bubbles_take_their_rows_key_else_the_outbox_entrys() {
        assert_eq!(own_bubble_key(&bubble(Some("8a74cf03-f526-4ecd-a16b-90e36110bbeb"), Some("c-1"))), "m:8a74cf03-f526-4ecd-a16b-90e36110bbeb");
        assert_eq!(own_bubble_key(&bubble(None, Some("c-1"))), "o:c-1");
    }
}
