//! #EXP-2204 (`packages/exponential-ui/docs/ci-components-contract.md`
//! §1, §4-§6): the LogLines and JobGraph leaves and the CodeBlock anchor
//! additions. The core builds LogLines and JobGraph as ONE leaf each, its
//! state resolved (`open`, `query`, `follow`, `timestamps`, `selection`;
//! JobGraph `compact` and `value`) and owns every write; these painters keep
//! only what the platform owns (the scroll position, the hover, the current
//! match, the keyboard's node) and send `toggle`, `reveal`, `search`,
//! `follow`, `timestamps`, `select` (LogLines) and `select {id}` (JobGraph).
//!
//! LogLines paints the toolbar (a gpui-component input for the search, the
//! `n of m` count, prev / next, the Follow and Timestamps toggles) over the
//! body: the rows of `ci_log::log_rows_open`, FIXED heights (a step row =
//! the `section` recipe's height, a line = the `line` recipe's lineHeight),
//! windowed by `list::virtual_window` (only the rows in view ±
//! `windowOverscan` are built), each line's ANSI tokens from the core
//! tokenizer cached per line, the search marks as run backgrounds. Follow
//! sticks the body to its bottom; a person's scroll up turns it off.
//!
//! JobGraph paints `job_graph::job_graph_layout`: graph mode (stage titles,
//! absolute nodes, the connectors of the hovered / keyboard-focused node's
//! upstream set only, as cubic paths) or the compact status list. The
//! running status glyph spins one turn per `spinMs` on the shared stepped
//! clock (never a per-frame loop; still under reduced motion).

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::Duration;

use eui_core::ci_log::{self, LogMatch, LogModel, LogRow};
use eui_core::code::{tokenize_code, CodeToken};
use eui_core::job_graph::{self, JobGraphLayout};
use eui_core::surface::PlacedNode;
use eui_core::theme::{Mode, ResolvedTheme};
use gpui::{canvas, div, point, prelude::*, px, Animation, Focusable as _, AnyElement, Context, Entity, Font, Hsla, PathBuilder, SharedString, StyledText, Subscription, TextRun, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::{json, Map, Value};

use super::SurfaceView;
use crate::host::HostPlugin;
use crate::paint::natives::LeafCx;
use crate::paint::parts::{control, part_props, px_prop, spacing};
use crate::paint::{icons, PaintStyle};

/// What the pointer is over inside a LogLines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogHover {
    Section(usize),
    Gutter(usize),
    Button(&'static str),
}

/// Where the body scrolls next (applied when its row is laid out).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollTarget {
    /// The line's top in the top third (a selection).
    Third(usize),
    /// The line into view, only when it is out of view (a match).
    IfHidden(usize),
}

/// What a LogLines painted last (tests, automation).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogLinesState {
    /// The rows built this frame (the window), in order.
    pub built: Vec<LogRow>,
    /// Every row of the body.
    pub rows: usize,
    /// The body's scroll offset `(x, y)` and its largest `(x, y)`.
    pub scroll: (f32, f32),
    pub max_scroll: (f32, f32),
    /// The current match (0-based) and how many there are.
    pub current: usize,
    pub matches: usize,
    /// The line rows painted with match marks: `(line, marks, current)`.
    pub marked: Vec<(usize, usize, bool)>,
}

/// One LogLines node's platform state.
#[derive(Default)]
pub(crate) struct LogLeaf {
    key: u64,
    model: Rc<LogModel>,
    /// The widest content line in characters (escape-free).
    columns: usize,
    query: String,
    matches: Vec<LogMatch>,
    by_line: HashMap<usize, Vec<usize>>,
    tokens: HashMap<usize, Rc<Vec<CodeToken>>>,
    current: usize,
    scroll: (f32, f32),
    max: (f32, f32),
    line_height: f32,
    hovered: Option<LogHover>,
    pending: Option<ScrollTarget>,
    /// The selection last handled (from outside or ours).
    handled_selection: Option<String>,
    /// The anchor our own gutter press asked for.
    own_selection: Option<String>,
    /// Follow turned off by us, the prop not back yet.
    follow_off_sent: bool,
    /// The query last pushed into (or typed in) the field.
    query_seen: String,
    /// Typing asked for the first match to show once its matches land.
    reveal_first: bool,
    search: Option<(Entity<InputState>, Subscription)>,
    last: LogLinesState,
}

impl LogLeaf {
    /// Re-parse the model when `lines` / `sections` changed.
    fn sync_model(&mut self, props: &Map<String, Value>) {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        props.get("lines").map(Value::to_string).hash(&mut h);
        props.get("sections").map(Value::to_string).hash(&mut h);
        let key = h.finish() | 1;
        if key == self.key {
            return;
        }
        self.key = key;
        let model = ci_log::log_model(props.get("lines"), props.get("sections"));
        self.columns = model.lines.iter().map(|l| ci_log::strip_ansi(&l.text).chars().count()).max().unwrap_or(0);
        self.model = Rc::new(model);
        self.tokens.clear();
        self.query = String::from("\u{0}");
        let q = props.get("query").and_then(Value::as_str).unwrap_or("").to_string();
        self.sync_query(&q);
    }

    /// Re-search when the query changed (the current match resets).
    fn sync_query(&mut self, query: &str) {
        if query == self.query {
            return;
        }
        self.query = query.to_string();
        self.matches = ci_log::log_search(&self.model, query);
        self.by_line.clear();
        for (i, m) in self.matches.iter().enumerate() {
            self.by_line.entry(m.line).or_default().push(i);
        }
        self.current = 0;
    }

    /// The ansi tokens of content line `n`, tokenized once.
    fn tokens(&mut self, n: usize) -> Rc<Vec<CodeToken>> {
        if let Some(t) = self.tokens.get(&n) {
            return t.clone();
        }
        let t = Rc::new(self.model.line(n).map(|l| tokenize_code(&l.text, "ansi").into_iter().next().unwrap_or_default()).unwrap_or_default());
        self.tokens.insert(n, t.clone());
        t
    }
}

/// One JobGraph node's platform state.
pub(crate) struct GraphLeaf {
    key: u64,
    layout: Rc<JobGraphLayout>,
    hovered: Option<String>,
    /// The keyboard's node while the graph has the focus.
    keyboard: Option<String>,
    scroll_x: f32,
    max_x: f32,
    /// What the last frame lit: the upstream set and the edges drawn.
    lit: Vec<String>,
    edges: usize,
}

impl Default for GraphLeaf {
    fn default() -> Self {
        GraphLeaf { key: 0, layout: Rc::new(job_graph::job_graph_layout(None, false, &job_graph::JOB_GRAPH)), hovered: None, keyboard: None, scroll_x: 0.0, max_x: 0.0, lit: Vec::new(), edges: 0 }
    }
}

impl GraphLeaf {
    fn sync(&mut self, props: &Map<String, Value>) {
        let compact = props.get("compact").and_then(Value::as_bool) == Some(true);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        props.get("jobs").map(Value::to_string).hash(&mut h);
        compact.hash(&mut h);
        let key = h.finish() | 1;
        if key != self.key {
            self.key = key;
            self.layout = Rc::new(job_graph::job_graph_layout(props.get("jobs"), compact, &job_graph::JOB_GRAPH));
        }
    }
}

/// What a JobGraph painted last (tests, automation).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JobGraphState {
    pub mode: &'static str,
    /// The node ids in paint (column) order.
    pub nodes: Vec<String>,
    /// The highlighted upstream set (empty = nothing hovered or focused).
    pub lit: Vec<String>,
    /// The connectors drawn.
    pub edges: usize,
    pub hovered: Option<String>,
    pub keyboard: Option<String>,
}

#[derive(Default)]
pub(crate) struct CiLeaves {
    logs: RefCell<HashMap<String, Rc<RefCell<LogLeaf>>>>,
    graphs: RefCell<HashMap<String, Rc<RefCell<GraphLeaf>>>>,
    /// CodeBlock selections last seen (a new one scrolls into view).
    code_selection: RefCell<HashMap<String, String>>,
}

impl CiLeaves {
    fn log(&self, id: &str) -> Rc<RefCell<LogLeaf>> {
        self.logs.borrow_mut().entry(id.to_string()).or_default().clone()
    }

    fn graph(&self, id: &str) -> Rc<RefCell<GraphLeaf>> {
        self.graphs.borrow_mut().entry(id.to_string()).or_default().clone()
    }
}

/// A resolved part look: the painted visual + the flat style.
struct Look {
    s: PaintStyle,
    p: Map<String, Value>,
}

impl Look {
    fn px(&self, key: &str) -> Option<f32> {
        px_prop(&self.p, key)
    }
    fn pad_h(&self) -> f32 {
        self.px("paddingHorizontal").or_else(|| self.px("paddingLeft")).or_else(|| self.px("padding")).unwrap_or(0.0)
    }
    fn pad_v(&self) -> f32 {
        self.px("paddingVertical").or_else(|| self.px("paddingTop")).or_else(|| self.px("padding")).unwrap_or(0.0)
    }
    fn gap(&self, fallback: f32) -> f32 {
        self.px("gap").filter(|g| *g > 0.0).unwrap_or(fallback)
    }
    fn border(&self) -> f32 {
        self.s.border.iter().copied().fold(0.0, f32::max)
    }
}

/// The box of a look on any element: fill, corners, hairline, shadow, opacity.
fn boxed<E: Styled>(mut e: E, s: &PaintStyle) -> E {
    if let Some(bg) = s.bg {
        e = e.bg(bg);
    }
    if s.radii[0] > 0.0 {
        e = e.rounded(px(s.radii[0]));
    }
    if s.has_border() {
        let [t, r, b, l] = s.border;
        e = e.border_t(px(t)).border_r(px(r)).border_b(px(b)).border_l(px(l)).border_color(s.border_color.unwrap_or_default());
    }
    if !s.shadows.is_empty() {
        e = e.shadow(s.shadows.clone());
    }
    if let Some(o) = s.opacity {
        e = e.opacity(o);
    }
    e
}

/// `{name}` placeholders of a string-table entry filled.
fn fill(template: String, pairs: &[(&str, String)]) -> String {
    pairs.iter().fold(template, |t, (k, v)| t.replace(&format!("{{{k}}}"), v))
}

/// THE status mark (§1): the glyph of `style.json ciStatus` in `color`;
/// `running` turns once per `spinMs` on the shared stepped clock (a
/// timer-driven repaint of the glyph, never a frame loop), still under
/// reduced motion.
pub(crate) fn status_glyph(host: Rc<dyn HostPlugin>, id: SharedString, status: &str, size: f32, color: Hsla, reduced: bool) -> AnyElement {
    let entry = ci_log::ci_status_entry(Some(&Value::String(status.to_string())));
    if entry.spin && !reduced {
        let icon = entry.icon;
        return crate::controls::stepped_glyph(id, gpui::size(px(size), px(size)), Animation::new(Duration::from_millis(ci_log::CI_STATUS_SPIN_MS as u64)).repeat(), crate::controls::SMOOTH_STEP, move |delta, _, _| {
            icons::concept_rotated(host.as_ref(), icon, size, color, delta * TAU)
        })
        .into_any_element();
    }
    div().flex_none().size(px(size)).child(icons::concept(host.as_ref(), entry.icon, size, color)).into_any_element()
}

/// The painter's view of a theme.
struct Theme<'a> {
    theme: Option<&'a ResolvedTheme>,
    mode: Mode,
}

impl Theme<'_> {
    fn look(&self, component: &str, part: &str, props: &Map<String, Value>, states: &[&str]) -> Look {
        let states: Vec<String> = states.iter().map(|s| s.to_string()).collect();
        let p = part_props(self.theme, self.mode, component, part, props, &states);
        let s = PaintStyle::from_visual(&eui_core::style::visual(&p, eui_core::style::BoxKind::Leaf));
        Look { s, p }
    }
}

fn status_props(status: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("status".into(), Value::String(status.to_string()));
    m
}

/// Line `text`'s runs: each token in its `token {kind}` colour (a prefixed
/// line paints every token with its `kind`), split at the search marks,
/// which take the `match` fill (`selected` = the current one).
fn code_runs(tokens: &[CodeToken], marks: &[(usize, usize, bool)], font: &Font, color: &dyn Fn(&str) -> Hsla, fills: (Hsla, Hsla)) -> (String, Vec<TextRun>) {
    let text: String = tokens.iter().map(|t| t.text.as_str()).collect();
    // Code-point offsets → byte offsets.
    let mut bytes: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    bytes.push(text.len());
    let at = |cp: usize| bytes.get(cp).copied().unwrap_or(text.len());
    let ranges: Vec<(usize, usize, bool)> = marks.iter().map(|(s, e, c)| (at(*s), at(*e), *c)).filter(|(s, e, _)| e > s).collect();
    let mut cuts: Vec<usize> = vec![0, text.len()];
    let mut pos = 0;
    for t in tokens {
        pos += t.text.len();
        cuts.push(pos);
    }
    for (s, e, _) in &ranges {
        cuts.push(*s);
        cuts.push(*e);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs = Vec::new();
    let mut token_end = 0;
    let mut tokens_iter = tokens.iter();
    let mut kind = "plain";
    for w in cuts.windows(2) {
        let (s, e) = (w[0], w[1]);
        if e <= s {
            continue;
        }
        while s >= token_end {
            match tokens_iter.next() {
                Some(t) => {
                    kind = if t.kind.is_empty() { "plain" } else { t.kind.as_str() };
                    token_end += t.text.len();
                }
                None => break,
            }
        }
        let bg = ranges.iter().find(|(ms, me, _)| s >= *ms && e <= *me).map(|(_, _, current)| if *current { fills.1 } else { fills.0 });
        runs.push(TextRun { len: e - s, font: font.clone(), color: color(kind), background_color: bg, underline: None, strikethrough: None });
    }
    (text, runs)
}

impl SurfaceView {
    // ------------------------------------------------------------------
    // Inspection (tests, automation)
    // ------------------------------------------------------------------

    /// What LogLines `id` painted last.
    pub fn log_lines_state(&self, id: &str) -> Option<LogLinesState> {
        self.ci_leaves.logs.borrow().get(id).map(|l| l.borrow().last.clone())
    }

    /// What JobGraph `id` painted last.
    pub fn job_graph_state(&self, id: &str) -> Option<JobGraphState> {
        self.ci_leaves.graphs.borrow().get(id).map(|g| {
            let g = g.borrow();
            JobGraphState { mode: g.layout.mode, nodes: g.layout.nodes.iter().map(|n| n.id.clone()).collect(), lit: g.lit.clone(), edges: g.edges, hovered: g.hovered.clone(), keyboard: g.keyboard.clone() }
        })
    }

    /// The search field of LogLines `id` (tests: type into it).
    pub fn log_search_field(&self, id: &str) -> Option<Entity<InputState>> {
        self.ci_leaves.logs.borrow().get(id).and_then(|l| l.borrow().search.as_ref().map(|(s, _)| s.clone()))
    }

    /// Scroll LogLines `id`'s body by `(dx, dy)` px as a person would
    /// (automation; a scroll up turns Follow off).
    pub fn log_scroll_by(&mut self, id: &str, dx: f32, dy: f32, cx: &mut Context<Self>) {
        self.log_scroll(id, dx, dy, cx);
    }

    fn probe(&self, key: String) -> Option<AnyElement> {
        let map = self.debug_bounds.clone()?;
        Some(canvas(move |b, _, _| {
            map.borrow_mut().insert(key.clone(), b);
        }, |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element())
    }

    fn ci_fire(&mut self, id: &str, event: &str, payload: Value, cx: &mut Context<Self>) {
        if let Some(i) = self.cache.index_of(id) {
            self.fire(i, event, Some(payload), cx);
        }
    }

    // ------------------------------------------------------------------
    // After each layout: search fields, selections, CodeBlock reveal
    // ------------------------------------------------------------------

    /// Create the LogLines search fields, echo an outside query into them,
    /// reveal a new selection (its step opens, it lands in the top third,
    /// Follow turns off), show the first match of a typed query, and
    /// scroll a self-scrolling CodeBlock's new selection into view.
    pub(crate) fn sync_ci(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let logs: Vec<PlacedNode> = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "LogLines").cloned().collect();
        let live: std::collections::HashSet<String> = logs.iter().map(|n| n.id.clone()).collect();
        self.ci_leaves.logs.borrow_mut().retain(|k, _| live.contains(k));
        let graphs: std::collections::HashSet<String> = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "JobGraph").map(|n| n.id.clone()).collect();
        self.ci_leaves.graphs.borrow_mut().retain(|k, _| graphs.contains(k));
        let placeholder = self.builtin_string("searchLog");
        for n in logs {
            let leaf = self.ci_leaves.log(&n.id);
            let query = n.props.get("query").and_then(Value::as_str).unwrap_or("").to_string();
            let follow = n.props.get("follow").and_then(Value::as_bool).unwrap_or(true);
            let mut l = leaf.borrow_mut();
            l.sync_model(&n.props);
            l.sync_query(&query);
            if !follow {
                l.follow_off_sent = false;
            }
            // The field.
            if l.search.is_none() {
                let (ph, v) = (placeholder.clone(), query.clone());
                let state = cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v));
                let id = n.id.clone();
                let sub = cx.subscribe_in(&state, window, move |this, _, ev: &InputEvent, window, cx| this.log_search_event(&id, ev, window, cx));
                l.search = Some((state, sub));
                l.query_seen = query.clone();
            } else if l.query_seen != query {
                l.query_seen = query.clone();
                let state = l.search.as_ref().map(|(s, _)| s.clone()).expect("a field");
                let focused = state.read(cx).focus_handle(cx).is_focused(window);
                if !focused && state.read(cx).value() != query.as_str() {
                    let q = query.clone();
                    state.update(cx, |s, cx| s.set_value(q, window, cx));
                }
            }
            // A new selection (initial or from outside).
            let selection = n.props.get("selection").and_then(Value::as_str).map(str::to_string);
            if selection != l.handled_selection {
                l.handled_selection = selection.clone();
                let ours = selection.is_some() && selection == l.own_selection;
                if let (false, Some(r)) = (ours, selection.as_deref().and_then(ci_log::parse_line_anchor)) {
                    l.pending = Some(ScrollTarget::Third(r.start as usize));
                    let id = n.id.clone();
                    let off = follow && !l.follow_off_sent;
                    if off {
                        l.follow_off_sent = true;
                    }
                    cx.defer_in(window, move |this, _, cx| {
                        this.ci_fire(&id, "reveal", json!({"line": r.start}), cx);
                        if off {
                            this.ci_fire(&id, "follow", json!({"follow": false}), cx);
                        }
                    });
                }
            }
            // Typing: the first match shows (once the query landed).
            if l.reveal_first && l.query == l.query_seen {
                l.reveal_first = false;
                if let Some(m) = l.matches.first().copied() {
                    l.pending = Some(ScrollTarget::IfHidden(m.line));
                    let id = n.id.clone();
                    cx.defer_in(window, move |this, _, cx| this.ci_fire(&id, "reveal", json!({"line": m.line}), cx));
                }
            }
        }
        // §6: a self-scrolling CodeBlock scrolls its first selected line into view.
        let blocks: Vec<(String, Option<String>)> = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "CodeBlock" && n.part.is_none()).map(|n| (n.id.clone(), n.props.get("selection").and_then(Value::as_str).map(str::to_string))).collect();
        let mut seen = self.ci_leaves.code_selection.borrow_mut();
        seen.retain(|k, _| blocks.iter().any(|(id, _)| id == k));
        for (id, selection) in blocks {
            let Some(sel) = selection else {
                seen.remove(&id);
                continue;
            };
            if seen.get(&id) == Some(&sel) {
                continue;
            }
            seen.insert(id.clone(), sel.clone());
            let Some(r) = ci_log::parse_line_anchor(&sel) else { continue };
            let row = self.cache.nodes.iter().find(|m| !m.removed && m.owner.as_deref() == Some(id.as_str()) && m.part.as_deref() == Some("line") && m.props.get("line").and_then(Value::as_u64) == Some(r.start)).map(|m| m.id.clone());
            if let Some(row) = row {
                // After the pass (the scroll containers are known then).
                cx.defer_in(window, move |this, _, cx| {
                    let body_scrolls = this.cache.nodes.iter().any(|m| !m.removed && m.owner.as_deref() == Some(id.as_str()) && m.part.as_deref() == Some("body") && this.scrolls.get(&m.index).is_some_and(|s| s.scroll_y));
                    if !body_scrolls {
                        return;
                    }
                    let events = this.surface.scroll_into_view(&row);
                    this.dispatch(events, None, cx);
                    cx.notify();
                });
            }
        }
    }

    // ------------------------------------------------------------------
    // LogLines
    // ------------------------------------------------------------------

    fn log_search_event(&mut self, id: &str, ev: &InputEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(leaf) = self.ci_leaves.logs.borrow().get(id).cloned() else { return };
        match ev {
            InputEvent::Change => {
                let value = {
                    let l = leaf.borrow();
                    let Some((state, _)) = &l.search else { return };
                    state.read(cx).value().to_string()
                };
                {
                    let mut l = leaf.borrow_mut();
                    if value == l.query_seen {
                        return;
                    }
                    l.query_seen = value.clone();
                    l.reveal_first = !value.is_empty();
                    l.current = 0;
                }
                self.ci_fire(id, "search", json!({"query": value}), cx);
            }
            InputEvent::PressEnter { shift, .. } => self.log_step(id, if *shift { -1 } else { 1 }, cx),
            _ => cx.notify(),
        }
    }

    /// Enter = next, Shift+Enter = previous (both wrap): the match's step
    /// opens and the match scrolls into view when it is out of it.
    pub(crate) fn log_step(&mut self, id: &str, by: i64, cx: &mut Context<Self>) {
        let Some(leaf) = self.ci_leaves.logs.borrow().get(id).cloned() else { return };
        let line = {
            let mut l = leaf.borrow_mut();
            let total = l.matches.len() as i64;
            if total == 0 {
                return;
            }
            l.current = (l.current as i64 + by).rem_euclid(total) as usize;
            let line = l.matches[l.current].line;
            l.pending = Some(ScrollTarget::IfHidden(line));
            line
        };
        self.ci_fire(id, "reveal", json!({"line": line}), cx);
        cx.notify();
    }

    fn log_toggle(&mut self, id: &str, section: usize, cx: &mut Context<Self>) {
        self.ci_fire(id, "toggle", json!({"section": section}), cx);
    }

    /// A press on a line number: the anchor (Shift extends); Follow turns
    /// off (it would scroll the pressed line away).
    fn log_select(&mut self, id: &str, n: usize, shift: bool, cx: &mut Context<Self>) {
        let Some(leaf) = self.ci_leaves.logs.borrow().get(id).cloned() else { return };
        let node = self.cache.index_of(id).and_then(|i| self.cache.node(i)).cloned();
        let current = node.as_ref().and_then(|n| n.props.get("selection")).cloned();
        let follow = node.as_ref().and_then(|n| n.props.get("follow")).and_then(Value::as_bool).unwrap_or(true);
        let anchor = ci_log::extend_line_anchor(current.as_ref(), n as u64, shift);
        let off = {
            let mut l = leaf.borrow_mut();
            l.own_selection = Some(anchor);
            let off = follow && !l.follow_off_sent;
            if off {
                l.follow_off_sent = true;
            }
            off
        };
        self.ci_fire(id, "select", json!({"line": n, "shift": shift}), cx);
        if off {
            self.ci_fire(id, "follow", json!({"follow": false}), cx);
        }
    }

    fn log_flag(&mut self, id: &str, flag: &'static str, cx: &mut Context<Self>) {
        let node = self.cache.index_of(id).and_then(|i| self.cache.node(i)).cloned();
        let on = node.as_ref().and_then(|n| n.props.get(flag)).and_then(Value::as_bool).unwrap_or(flag == "follow");
        if flag == "follow" && !on {
            // Back on: the body jumps to its bottom.
            if let Some(l) = self.ci_leaves.logs.borrow().get(id) {
                let mut l = l.borrow_mut();
                l.pending = None;
                l.follow_off_sent = false;
            }
        }
        let mut payload = Map::new();
        payload.insert(flag.to_string(), Value::Bool(!on));
        self.ci_fire(id, flag, Value::Object(payload), cx);
    }

    fn log_hover(&mut self, id: &str, what: LogHover, on: bool, cx: &mut Context<Self>) {
        let Some(leaf) = self.ci_leaves.logs.borrow().get(id).cloned() else { return };
        let mut l = leaf.borrow_mut();
        let next = if on { Some(what) } else if l.hovered == Some(what) { None } else { l.hovered };
        if next != l.hovered {
            l.hovered = next;
            cx.notify();
        }
    }

    /// A person's scroll of the body (wheel, trackpad, automation): a
    /// scroll UP while Follow is on turns it off.
    fn log_scroll(&mut self, id: &str, dx: f32, dy: f32, cx: &mut Context<Self>) -> bool {
        let Some(leaf) = self.ci_leaves.logs.borrow().get(id).cloned() else { return false };
        let follow = self.cache.index_of(id).and_then(|i| self.cache.node(i)).and_then(|n| n.props.get("follow")).and_then(Value::as_bool).unwrap_or(true);
        let (moved, up, off) = {
            let mut l = leaf.borrow_mut();
            let (x0, y0) = l.scroll;
            let x = (x0 - dx).clamp(0.0, l.max.0);
            let y = (y0 - dy).clamp(0.0, l.max.1);
            l.scroll = (x, y);
            let moved = (x - x0).abs() > 0.01 || (y - y0).abs() > 0.01;
            let up = y < y0 - 0.01;
            if moved {
                l.pending = None;
            }
            let off = up && follow && !l.follow_off_sent;
            if off {
                l.follow_off_sent = true;
            }
            (moved, up, off)
        };
        if off {
            self.ci_fire(id, "follow", json!({"follow": false}), cx);
        }
        if moved {
            cx.notify();
        }
        moved || up
    }

    /// The LogLines leaf (toolbar over the windowed body).
    pub(super) fn log_lines_element(&self, lcx: &LeafCx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let n = lcx.node;
        let id = n.id.clone();
        let leaf_rc = self.ci_leaves.log(&id);
        let mut leaf = leaf_rc.borrow_mut();
        leaf.sync_model(&n.props);
        let query = n.props.get("query").and_then(Value::as_str).unwrap_or("").to_string();
        leaf.sync_query(&query);
        let follow = n.props.get("follow").and_then(Value::as_bool).unwrap_or(true);
        let timestamps = n.props.get("timestamps").and_then(Value::as_bool) == Some(true);
        let open: Vec<usize> = n.props.get("open").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as usize).collect()).unwrap_or_default();
        let selection = n.props.get("selection").and_then(Value::as_str).and_then(ci_log::parse_line_anchor);
        let model = leaf.model.clone();
        let rows = ci_log::log_rows_open(&model, &open);
        let t = Theme { theme: lcx.theme, mode: lcx.mode };
        let empty = Map::new();
        let root = t.look("LogLines", "root", &empty, &[]);
        let toolbar = t.look("LogLines", "toolbar", &empty, &[]);
        let body = t.look("LogLines", "body", &empty, &[]);
        let section_look = t.look("LogLines", "section", &empty, &[]);
        let line_look = t.look("LogLines", "line", &empty, &[]);
        let sm = spacing(lcx.theme, "sm");
        let (w, h) = (lcx.w, lcx.h);
        let bar_h = toolbar.px("height").unwrap_or_else(|| control(lcx.theme, "input", 36.0));
        let gap = root.gap(sm);
        let body_h = (h - bar_h - gap).max(0.0);
        let bw = body.border();
        let pad_v = body.pad_v();
        let view_w = (w - 2.0 * bw).max(0.0);
        // The body's vertical padding scrolls with its rows (as the web's).
        let view_h = (body_h - 2.0 * bw).max(0.0);
        let lh = line_look.px("lineHeight").or_else(|| body.px("lineHeight")).unwrap_or(22.75);
        let sec_h = section_look.px("height").unwrap_or_else(|| control(lcx.theme, "row", 40.0));
        leaf.line_height = lh;
        // The mono face of the body (CodeBlock's type).
        let code_size = body.px("fontSize").unwrap_or(14.0);
        let mono = self.fonts.font(body.p.get("fontFamily").and_then(Value::as_str).or(Some(self.fonts.mono.as_ref())), 400, false);
        let ch = {
            let ts = window.text_system();
            let fid = ts.resolve_font(&mono);
            ts.advance(fid, px(code_size), '0').map(|s| f32::from(s.width)).unwrap_or(code_size * 0.6)
        };
        let gutter_base = t.look("LogLines", "gutter", &empty, &[]);
        let time_look = t.look("LogLines", "time", &empty, &[]);
        let digits = model.lines.len().max(1).to_string().len();
        let gutter_w = (digits as f32 + 1.0) * ch + 2.0 * gutter_base.pad_h();
        let lead = (line_look.pad_h() - gutter_base.pad_h()).max(0.0);
        let time_w = if timestamps { 8.0 * ch + 2.0 * time_look.pad_h() } else { 0.0 };
        let indent = if model.sections.is_empty() { 0.0 } else { ci_log::LOG_INDENT };
        let content_w = indent + lead + gutter_w + time_w + leaf.columns as f32 * ch + line_look.pad_h();
        // The rows' fixed extents.
        let extents: Vec<f64> = rows.iter().map(|r| if matches!(r, LogRow::Section(_)) { sec_h as f64 } else { lh as f64 }).collect();
        let mut offsets = Vec::with_capacity(rows.len() + 1);
        let mut acc = pad_v as f64;
        for e in &extents {
            offsets.push(acc);
            acc += e;
        }
        let total = acc as f32 + pad_v;
        leaf.max = ((content_w - view_w).max(0.0), (total - view_h).max(0.0));
        // Where the body is: a pending reveal, else Follow's bottom.
        if let Some(target) = leaf.pending {
            let line = match target {
                ScrollTarget::Third(l) | ScrollTarget::IfHidden(l) => l,
            };
            if let Some(r) = rows.iter().position(|r| *r == LogRow::Line(line)) {
                let top = offsets[r] as f32;
                let y = leaf.scroll.1;
                leaf.scroll.1 = match target {
                    ScrollTarget::Third(_) => top - view_h / 3.0,
                    ScrollTarget::IfHidden(_) if top < y || top + lh > y + view_h => top - (view_h - lh) / 2.0,
                    ScrollTarget::IfHidden(_) => y,
                };
                leaf.pending = None;
            }
        } else if follow && !leaf.follow_off_sent {
            leaf.scroll.1 = leaf.max.1;
        }
        leaf.scroll = (leaf.scroll.0.clamp(0.0, leaf.max.0), leaf.scroll.1.clamp(0.0, leaf.max.1));
        let (sx, sy) = leaf.scroll;
        let window_range = eui_core::list::virtual_window(&extents, 0.0, (sy - pad_v) as f64, view_h as f64, eui_core::list::DEFAULT_OVERSCAN);
        let host = self.host.clone();
        let ink = lcx.ink;
        let reduced = lcx.reduced_motion;
        // The marks' fills.
        let match_fill = t.look("LogLines", "match", &empty, &[]).s.bg.unwrap_or(ink.opacity(0.2));
        let current_fill = t.look("LogLines", "match", &empty, &["selected"]).s.bg.unwrap_or(ink.opacity(0.4));
        let token_colors: RefCell<HashMap<String, Hsla>> = RefCell::new(HashMap::new());
        let token_color = |kind: &str| -> Hsla {
            if let Some(c) = token_colors.borrow().get(kind) {
                return *c;
            }
            let mut p = Map::new();
            p.insert("kind".into(), Value::String(kind.to_string()));
            let c = t.look("LogLines", "token", &p, &[]).s.color.unwrap_or(ink);
            token_colors.borrow_mut().insert(kind.to_string(), c);
            c
        };
        let sans = |look: &Look, weight: u16| self.fonts.font(look.p.get("fontFamily").and_then(Value::as_str), look.p.get("fontWeight").and_then(Value::as_f64).map(|w| w as u16).unwrap_or(weight), false);
        let hovered = leaf.hovered;
        let mut built = Vec::new();
        let mut marked = Vec::new();
        let mut row_els: Vec<AnyElement> = Vec::new();
        for r in window_range.start..window_range.end {
            let row = rows[r];
            built.push(row);
            let y = offsets[r] as f32 - sy;
            match row {
                LogRow::Section(i) => {
                    let Some(s) = model.sections.get(i) else { continue };
                    let is_open = open.contains(&i);
                    let mut props = status_props(s.status);
                    props.insert("open".into(), Value::Bool(is_open));
                    let hover = hovered == Some(LogHover::Section(i));
                    let look = t.look("LogLines", "section", &props, if hover { &["hover"] } else { &[] });
                    let chevron = t.look("LogLines", "chevron", &props, &[]);
                    let icon = t.look("LogLines", "sectionIcon", &props, &[]);
                    let title = t.look("LogLines", "sectionTitle", &props, &[]);
                    let meta = t.look("LogLines", "sectionMeta", &props, &[]);
                    let icon_size = icon.px("width").unwrap_or_else(|| control(lcx.theme, "iconSm", 16.0));
                    let turn = if is_open { if chevron.s.transform.rotate.abs() > 0.1 { chevron.s.transform.rotate } else { 90.0 } } else { 0.0 };
                    let label = format!("{}, {}", s.title, self.builtin_string(ci_log::ci_status_entry(Some(&Value::String(s.status.to_string()))).label));
                    let key = format!("{id}.section.{i}");
                    let (tid, hid) = (id.clone(), id.clone());
                    let mut el = div()
                        .id(SharedString::from(key.clone()))
                        .absolute()
                        .left_0()
                        .top(px(y))
                        .w(px(view_w))
                        .h(px(sec_h))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(look.gap(sm)))
                        .px(px(look.pad_h()))
                        .cursor_pointer()
                        .role(gpui::Role::Button)
                        .aria_label(SharedString::from(label))
                        .aria_expanded(is_open)
                        .when_some(look.s.bg, |d, bg| d.bg(bg))
                        .on_click(cx.listener(move |this, _, _, cx| this.log_toggle(&tid, i, cx)))
                        .on_hover(cx.listener(move |this, on: &bool, _, cx| this.log_hover(&hid, LogHover::Section(i), *on, cx)))
                        .child(div().flex_none().child(icons::concept_rotated(host.as_ref(), "ui-chevron-right", chevron.px("width").unwrap_or(icon_size), chevron.s.color.unwrap_or(ink), turn.to_radians())))
                        .child(status_glyph(host.clone(), SharedString::from(format!("{key}.status")), s.status, icon_size, icon.s.color.unwrap_or(ink), reduced));
                    let tsz = title.px("fontSize").unwrap_or(14.0);
                    el = el.child(
                        div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().font(sans(&title, 500)).text_size(px(tsz)).line_height(px(title.px("lineHeight").unwrap_or(20.0))).text_color(title.s.color.unwrap_or(ink)).child(SharedString::from(s.title.clone())),
                    );
                    if let Some(d) = &s.duration {
                        el = el.child(div().flex_none().whitespace_nowrap().font(crate::measure::with_tabular_nums(sans(&meta, 400))).text_size(px(meta.px("fontSize").unwrap_or(12.0))).line_height(px(meta.px("lineHeight").unwrap_or(16.0))).text_color(meta.s.color.unwrap_or(ink)).child(SharedString::from(d.clone())));
                    }
                    if let Some(p) = self.probe(key) {
                        el = el.child(p);
                    }
                    row_els.push(el.into_any_element());
                }
                LogRow::Line(ln) => {
                    let Some(line) = model.line(ln) else { continue };
                    let selected = selection.is_some_and(|s| s.contains(ln as u64));
                    let look = t.look("LogLines", "line", &empty, if selected { &["selected"] } else { &[] });
                    let ghover = hovered == Some(LogHover::Gutter(ln));
                    let gutter = t.look("LogLines", "gutter", &empty, if ghover { &["hover"] } else { &[] });
                    let tokens = leaf.tokens(ln);
                    let marks: Vec<(usize, usize, bool)> = leaf.by_line.get(&ln).map(|ms| ms.iter().map(|&k| (leaf.matches[k].start, leaf.matches[k].end, k == leaf.current)).collect()).unwrap_or_default();
                    if !marks.is_empty() {
                        marked.push((ln, marks.len(), marks.iter().any(|m| m.2)));
                    }
                    let kind = line.kind;
                    let color_of = |k: &str| token_color(kind.unwrap_or(k));
                    let (text, runs) = code_runs(&tokens, &marks, &mono, &color_of, (match_fill, current_fill));
                    let in_section = line.section >= 0;
                    let key = format!("{id}.gutter.{ln}");
                    let (sid, hid) = (id.clone(), id.clone());
                    let mut gut = div()
                        .id(SharedString::from(key.clone()))
                        .flex_none()
                        .w(px(gutter_w))
                        .h(px(lh))
                        .px(px(gutter.pad_h()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_end()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .text_color(gutter.s.color.unwrap_or(ink))
                        .when_some(gutter.s.opacity, |d, o| d.opacity(o))
                        .role(gpui::Role::Link)
                        .aria_label(SharedString::from(fill(self.builtin_string("lineAnchor"), &[("line", ln.to_string())])))
                        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, _, cx| this.log_select(&sid, ln, ev.modifiers().shift, cx)))
                        .on_hover(cx.listener(move |this, on: &bool, _, cx| this.log_hover(&hid, LogHover::Gutter(ln), *on, cx)))
                        .child(SharedString::from(ln.to_string()));
                    if let Some(p) = self.probe(key) {
                        gut = gut.child(p);
                    }
                    let mut el = div()
                        .absolute()
                        .left(px(-sx))
                        .top(px(y))
                        .w(px(content_w.max(view_w + sx)))
                        .h(px(lh))
                        .flex()
                        .flex_row()
                        .items_center()
                        .font(mono.clone())
                        .text_size(px(code_size))
                        .line_height(px(lh))
                        .when_some(look.s.bg, |d, bg| d.bg(bg))
                        .child(div().flex_none().w(px(lead + if in_section { indent } else { 0.0 })))
                        .child(gut);
                    if timestamps {
                        el = el.child(div().flex_none().w(px(time_w)).px(px(time_look.pad_h())).whitespace_nowrap().text_color(time_look.s.color.unwrap_or(ink)).child(SharedString::from(line.time.clone().unwrap_or_default())));
                    }
                    if !text.is_empty() {
                        el = el.child(div().flex_none().whitespace_nowrap().child(StyledText::new(SharedString::from(text)).with_runs(runs)));
                    }
                    if let Some(p) = self.probe(format!("{id}.line.{ln}")) {
                        el = el.child(p);
                    }
                    row_els.push(el.into_any_element());
                }
            }
        }
        // The toolbar.
        let total = leaf.matches.len();
        let shown = if total == 0 { 0 } else { leaf.current.min(total - 1) + 1 };
        let bar = self.log_toolbar(lcx, &t, &*leaf, &id, w, bar_h, follow, timestamps, &query, (shown, total), window, cx);
        leaf.last = LogLinesState { built, rows: rows.len(), scroll: leaf.scroll, max_scroll: leaf.max, current: leaf.current, matches: total, marked };
        drop(leaf);
        let wid = id.clone();
        let body_el = boxed(
            div()
                .id(SharedString::from(format!("{id}.body")))
                .absolute()
                .left_0()
                .top(px(bar_h + gap))
                .w(px(w))
                .h(px(body_h))
                .overflow_hidden()
                .role(gpui::Role::List)
                .on_scroll(cx.listener(move |this, ev: &gpui::ScrollEvent, _, cx| {
                    let d = ev.delta.pixel_delta(px(lh));
                    let (mut dx, dy) = (f32::from(d.x), f32::from(d.y));
                    if ev.modifiers.shift && dx == 0.0 {
                        dx = dy;
                        if this.log_scroll(&wid, dx, 0.0, cx) {
                            cx.stop_propagation();
                        }
                        return;
                    }
                    if this.log_scroll(&wid, dx, dy, cx) {
                        cx.stop_propagation();
                    }
                })),
            &body.s,
        )
        .child(div().absolute().left(px(0.0)).top(px(0.0)).w(px(view_w)).h(px(view_h)).overflow_hidden().children(row_els));
        div().size_full().relative().child(bar).child(body_el).into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn log_toolbar(&self, lcx: &LeafCx, t: &Theme, leaf: &LogLeaf, id: &str, w: f32, bar_h: f32, follow: bool, timestamps: bool, query: &str, (shown, total): (usize, usize), window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let empty = Map::new();
        let ink = lcx.ink;
        let sm = spacing(lcx.theme, "sm");
        let toolbar = t.look("LogLines", "toolbar", &empty, &[]);
        let sans = |look: &Look, weight: u16| self.fonts.font(look.p.get("fontFamily").and_then(Value::as_str), look.p.get("fontWeight").and_then(Value::as_f64).map(|w| w as u16).unwrap_or(weight), false);
        let focused = leaf.search.as_ref().is_some_and(|(s, _)| s.read(cx).focus_handle(cx).is_focused(window));
        let search = t.look("LogLines", "search", &empty, if focused { &["focus"] } else { &[] });
        let search_h = search.px("height").unwrap_or(bar_h);
        let fsize = search.px("fontSize").unwrap_or(14.0);
        let mut bar = div().absolute().top_0().left_0().w(px(w)).h(px(bar_h)).flex().flex_row().items_center().gap(px(toolbar.gap(sm)));
        let mut field = boxed(div().id(SharedString::from(format!("{id}.search"))).flex_shrink(1.0).min_w(px(48.0)).w(px(ci_log::LOG_SEARCH_WIDTH)).h(px(search_h)).px(px(search.pad_h())).flex().items_center().overflow_hidden(), &search.s);
        match &leaf.search {
            Some((state, _)) => {
                field = field.child(Input::new(state).appearance(false).font(sans(&search, 400)).text_size(px(fsize)).text_color(search.s.color.unwrap_or(ink)).px_0().py_0().w_full().h(px(search_h - 2.0 * search.border())));
            }
            None => {
                let muted = lcx.theme_color("mutedForeground").unwrap_or(ink);
                field = field.child(div().text_size(px(fsize)).text_color(muted).child(SharedString::from(self.builtin_string("searchLog"))));
            }
        }
        bar = bar.child(field);
        if !query.is_empty() {
            let count = t.look("LogLines", "count", &empty, &[]);
            let text = fill(self.builtin_string("matchCount"), &[("current", shown.to_string()), ("total", total.to_string())]);
            bar = bar.child(div().flex_none().whitespace_nowrap().font(crate::measure::with_tabular_nums(sans(&count, 400))).text_size(px(count.px("fontSize").unwrap_or(14.0))).line_height(px(count.px("lineHeight").unwrap_or(20.0))).text_color(count.s.color.unwrap_or(ink)).child(SharedString::from(text)));
        }
        for (part, icon, label, by) in [("prev", "ui-chevron-up", "previousMatch", -1i64), ("next", "ui-chevron-down", "nextMatch", 1i64)] {
            let enabled = total > 0;
            let hover = enabled && leaf.hovered == Some(LogHover::Button(part));
            let states: &[&str] = if !enabled { &["disabled"] } else if hover { &["hover"] } else { &[] };
            let look = t.look("LogLines", part, &empty, states);
            let side = look.px("width").unwrap_or_else(|| control(lcx.theme, "buttonSm", 32.0));
            let (sid, hid) = (id.to_string(), id.to_string());
            let mut b = boxed(div().id(SharedString::from(format!("{id}.{part}"))).flex_none().w(px(side)).h(px(look.px("height").unwrap_or(side))).flex().items_center().justify_center(), &look.s)
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(self.builtin_string(label)))
                .child(icons::concept(lcx.host, icon, control(lcx.theme, "iconSm", 16.0), look.s.color.unwrap_or(ink)))
                .on_hover(cx.listener(move |this, on: &bool, _, cx| this.log_hover(&hid, LogHover::Button(part), *on, cx)));
            if enabled {
                b = b.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.log_step(&sid, by, cx)));
            }
            if let Some(p) = self.probe(format!("{id}.{part}")) {
                b = b.child(p);
            }
            bar = bar.child(b);
        }
        bar = bar.child(div().flex_1());
        for (part, on) in [("follow", follow), ("timestamps", timestamps)] {
            let hover = leaf.hovered == Some(LogHover::Button(part));
            let mut states: Vec<&str> = Vec::new();
            if on {
                states.push("selected");
            }
            if hover {
                states.push("hover");
            }
            let look = t.look("LogLines", part, &empty, &states);
            let (sid, hid) = (id.to_string(), id.to_string());
            let mut b = boxed(div().id(SharedString::from(format!("{id}.{part}"))).flex_none().h(px(look.px("height").unwrap_or_else(|| control(lcx.theme, "buttonSm", 32.0)))).px(px(look.pad_h())).flex().items_center().cursor_pointer(), &look.s)
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(self.builtin_string(part)))
                .aria_toggled(if on { gpui::Toggled::True } else { gpui::Toggled::False })
                .whitespace_nowrap()
                .font(sans(&look, 500))
                .text_size(px(look.px("fontSize").unwrap_or(14.0)))
                .text_color(look.s.color.unwrap_or(ink))
                .child(SharedString::from(self.builtin_string(part)))
                .on_click(cx.listener(move |this, _, _, cx| this.log_flag(&sid, part, cx)))
                .on_hover(cx.listener(move |this, on: &bool, _, cx| this.log_hover(&hid, LogHover::Button(part), *on, cx)));
            if let Some(p) = self.probe(format!("{id}.{part}")) {
                b = b.child(p);
            }
            bar = bar.child(b);
        }
        bar.into_any_element()
    }

    // ------------------------------------------------------------------
    // JobGraph
    // ------------------------------------------------------------------

    fn graph_hover(&mut self, id: &str, node: &str, on: bool, cx: &mut Context<Self>) {
        let Some(g) = self.ci_leaves.graphs.borrow().get(id).cloned() else { return };
        let mut g = g.borrow_mut();
        let next = if on { Some(node.to_string()) } else if g.hovered.as_deref() == Some(node) { None } else { g.hovered.clone() };
        if next != g.hovered {
            g.hovered = next;
            cx.notify();
        }
    }

    fn graph_select(&mut self, id: &str, node: &str, cx: &mut Context<Self>) {
        self.ci_fire(id, "select", json!({"id": node}), cx);
    }

    /// The keyboard arrived on JobGraph `index` (Tab): its first node (the
    /// last one going backwards).
    pub(crate) fn job_graph_entered(&mut self, index: u32, backwards: bool) {
        let Some(n) = self.cache.node(index).filter(|n| n.component == "JobGraph").cloned() else { return };
        let g = self.ci_leaves.graph(&n.id);
        let mut g = g.borrow_mut();
        g.sync(&n.props);
        let nodes = &g.layout.nodes;
        let pick = if backwards { nodes.last() } else { nodes.first() }.map(|n| n.id.clone());
        g.keyboard = pick;
    }

    /// A key on a focused JobGraph: Tab / arrows move between the nodes in
    /// column order (Tab past the last one leaves the graph), Enter /
    /// Space select. `true` = handled.
    pub(crate) fn job_graph_key(&mut self, index: u32, key: &str, shift: bool, cx: &mut Context<Self>) -> bool {
        let Some(n) = self.cache.node(index).filter(|n| n.component == "JobGraph").cloned() else { return false };
        let g = self.ci_leaves.graph(&n.id);
        let ids: Vec<String> = {
            let mut g = g.borrow_mut();
            g.sync(&n.props);
            g.layout.nodes.iter().map(|n| n.id.clone()).collect()
        };
        if ids.is_empty() {
            return false;
        }
        let at = g.borrow().keyboard.as_ref().and_then(|k| ids.iter().position(|i| i == k));
        let step = match key {
            "tab" if shift => -1,
            "tab" => 1,
            "right" | "down" => 1,
            "left" | "up" => -1,
            "home" | "end" => 0,
            "enter" | "space" => {
                let target = at.map(|i| ids[i].clone()).or_else(|| n.props.get("value").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| ids[0].clone());
                self.graph_select(&n.id, &target, cx);
                return true;
            }
            _ => return false,
        };
        let next = match (key, at) {
            ("home", _) => Some(0),
            ("end", _) => Some(ids.len() - 1),
            (_, None) => Some(if step > 0 { 0 } else { ids.len() - 1 }),
            (_, Some(i)) => {
                let j = i as i64 + step;
                if j < 0 || j >= ids.len() as i64 {
                    // Tab leaves the graph at its ends; arrows stop.
                    if key == "tab" {
                        g.borrow_mut().keyboard = None;
                        cx.notify();
                        return false;
                    }
                    Some(i)
                } else {
                    Some(j as usize)
                }
            }
        };
        g.borrow_mut().keyboard = next.map(|i| ids[i].clone());
        cx.notify();
        true
    }

    /// The JobGraph leaf: the graph (stages, nodes, the lit connectors),
    /// or the compact status list.
    pub(super) fn job_graph_element(&self, lcx: &LeafCx, cx: &mut Context<Self>) -> AnyElement {
        let n = lcx.node;
        let id = n.id.clone();
        let g_rc = self.ci_leaves.graph(&id);
        let mut g = g_rc.borrow_mut();
        g.sync(&n.props);
        let layout = g.layout.clone();
        let value = n.props.get("value").and_then(Value::as_str).unwrap_or("").to_string();
        let focused = self.focused == Some(n.index);
        let keyboard = if focused { g.keyboard.clone() } else { None };
        let focus_visible = focused && self.interaction.get(&n.id).is_some_and(|s| s.focus_visible);
        let active = g.hovered.clone().or_else(|| keyboard.clone());
        let lit: Vec<String> = if layout.mode == "graph" { active.as_deref().map(|a| job_graph::job_graph_upstream(&layout, a)).unwrap_or_default() } else { Vec::new() };
        let t = Theme { theme: lcx.theme, mode: lcx.mode };
        let ink = lcx.ink;
        let sm = spacing(lcx.theme, "sm");
        let host = self.host.clone();
        let reduced = lcx.reduced_motion;
        let icon_default = control(lcx.theme, "iconSm", 16.0);
        let sans = |look: &Look, weight: u16| self.fonts.font(look.p.get("fontFamily").and_then(Value::as_str), look.p.get("fontWeight").and_then(Value::as_f64).map(|w| w as u16).unwrap_or(weight), false);
        let label_of = |node: &job_graph::JobGraphNode| {
            let mut s = format!("{}, {}", node.name, self.builtin_string(ci_log::ci_status_entry(Some(&Value::String(node.status.to_string()))).label));
            if let Some((p, total)) = node.count {
                s.push_str(&format!(", {p}/{total}"));
            }
            s
        };
        // A node's content: glyph · name · spacer · meta (· chevron in the list).
        let content = |node: &job_graph::JobGraphNode, look: &Look, chevron: bool| -> gpui::Div {
            let props = status_props(node.status);
            let icon = t.look("JobGraph", "icon", &props, &[]);
            let name = t.look("JobGraph", "name", &props, &[]);
            let meta = t.look("JobGraph", "meta", &props, &[]);
            let size = icon.px("width").unwrap_or(icon_default);
            let mut d = div()
                .size_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(look.gap(sm)))
                .px(px(look.pad_h()))
                .child(status_glyph(host.clone(), SharedString::from(format!("{id}.status.{}", node.id)), node.status, size, icon.s.color.unwrap_or(ink), reduced))
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().font(sans(&name, 500)).text_size(px(name.px("fontSize").unwrap_or(14.0))).line_height(px(name.px("lineHeight").unwrap_or(20.0))).text_color(name.s.color.unwrap_or(ink)).child(SharedString::from(node.name.clone())));
            if let Some(m) = node.meta() {
                d = d.child(div().flex_none().whitespace_nowrap().font(crate::measure::with_tabular_nums(sans(&meta, 400))).text_size(px(meta.px("fontSize").unwrap_or(12.0))).line_height(px(meta.px("lineHeight").unwrap_or(16.0))).text_color(meta.s.color.unwrap_or(ink)).child(SharedString::from(m)));
            }
            if chevron {
                let c = t.look("JobGraph", "chevron", &props, &[]);
                d = d.child(div().flex_none().child(icons::concept_mirrored(lcx.host, "ui-chevron-right", c.px("width").unwrap_or(size), c.s.color.unwrap_or(ink), lcx.rtl)));
            }
            d
        };
        let mut edges_drawn = 0;
        let el = if layout.mode == "list" {
            let mut list = div().size_full().relative().flex().flex_col();
            for node in &layout.nodes {
                let props = status_props(node.status);
                let mut states: Vec<&str> = Vec::new();
                if g.hovered.as_deref() == Some(node.id.as_str()) {
                    states.push("hover");
                }
                if value == node.id {
                    states.push("selected");
                }
                let look = t.look("JobGraph", "row", &props, &states);
                let key = format!("{id}.row.{}", node.id);
                let (sid, hid, nid, nid2) = (id.clone(), id.clone(), node.id.clone(), node.id.clone());
                let mut row = boxed(div().id(SharedString::from(key.clone())).flex_none().w_full().h(px(node.height as f32)).relative().cursor_pointer(), &PaintStyle { radii: [0.0; 4], ..look.s.clone() })
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(label_of(node)))
                    .aria_selected(value == node.id)
                    .on_click(cx.listener(move |this, _, _, cx| this.graph_select(&sid, &nid, cx)))
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| this.graph_hover(&hid, &nid2, *on, cx)))
                    .child(content(node, &look, true));
                if let Some(p) = self.probe(key) {
                    row = row.child(p);
                }
                list = list.child(row);
            }
            list.into_any_element()
        } else {
            let stage = t.look("JobGraph", "stage", &Map::new(), &[]);
            let edge = t.look("JobGraph", "edge", &Map::new(), &[]);
            let geo = job_graph::JOB_GRAPH;
            let mut inner = div().absolute().top_0().left(px(-g.scroll_x)).w(px(layout.width as f32)).h(px(layout.height as f32));
            for (title, x) in &layout.stages {
                inner = inner.child(
                    div().absolute().left(px(*x as f32)).top_0().w(px(geo.node_width as f32)).h(px(geo.stage_height as f32)).flex().items_center().overflow_hidden().whitespace_nowrap().text_ellipsis().font(sans(&stage, 500)).text_size(px(stage.px("fontSize").unwrap_or(12.0))).line_height(px(stage.px("lineHeight").unwrap_or(16.0))).text_color(stage.s.color.unwrap_or(ink)).child(SharedString::from(title.clone())),
                );
            }
            // §5.4: the connectors of the active node's upstream set only.
            let drawn: Vec<([f32; 2], [f32; 2], [f32; 2], [f32; 2])> = layout.edges.iter().filter(|e| lit.contains(&e.from) && lit.contains(&e.to)).map(|e| {
                let p = |a: [f64; 2]| [a[0] as f32, a[1] as f32];
                (p(e.start), p(e.c1), p(e.c2), p(e.end))
            }).collect();
            edges_drawn = drawn.len();
            if !drawn.is_empty() {
                let color = edge.s.color.unwrap_or(ink);
                let width = geo.edge_width as f32;
                inner = inner.child(
                    canvas(|_, _, _| {}, move |bounds, _, window, _| {
                        let o = bounds.origin;
                        let at = |p: [f32; 2]| point(o.x + px(p[0]), o.y + px(p[1]));
                        for (s, c1, c2, e) in &drawn {
                            let mut b = PathBuilder::stroke(px(width));
                            b.move_to(at(*s));
                            b.cubic_bezier_to(at(*e), at(*c1), at(*c2));
                            if let Ok(path) = b.build() {
                                window.paint_path(path, color);
                            }
                        }
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(layout.width as f32))
                    .h(px(layout.height as f32)),
                );
            }
            for node in &layout.nodes {
                let props = status_props(node.status);
                let mut states: Vec<&str> = Vec::new();
                if g.hovered.as_deref() == Some(node.id.as_str()) {
                    states.push("hover");
                }
                if focus_visible && keyboard.as_deref() == Some(node.id.as_str()) {
                    states.push("focus");
                }
                if lit.contains(&node.id) {
                    states.push("highlight");
                }
                if value == node.id {
                    states.push("selected");
                }
                let look = t.look("JobGraph", "node", &props, &states);
                let key = format!("{id}.node.{}", node.id);
                let (sid, hid, nid, nid2) = (id.clone(), id.clone(), node.id.clone(), node.id.clone());
                let mut el = boxed(div().id(SharedString::from(key.clone())).absolute().left(px(node.x as f32)).top(px(node.y as f32)).w(px(node.width as f32)).h(px(node.height as f32)).cursor_pointer(), &look.s)
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(label_of(node)))
                    .aria_selected(value == node.id)
                    .on_click(cx.listener(move |this, _, _, cx| this.graph_select(&sid, &nid, cx)))
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| this.graph_hover(&hid, &nid2, *on, cx)))
                    .child(content(node, &look, false));
                if let Some(p) = self.probe(key) {
                    el = el.child(p);
                }
                inner = inner.child(el);
            }
            g.max_x = (layout.width as f32 - lcx.w).max(0.0);
            g.scroll_x = g.scroll_x.clamp(0.0, g.max_x);
            let wid = id.clone();
            div()
                .id(SharedString::from(format!("{id}.graph")))
                .size_full()
                .relative()
                .overflow_hidden()
                .on_scroll(cx.listener(move |this, ev: &gpui::ScrollEvent, _, cx| {
                    let d = ev.delta.pixel_delta(px(20.0));
                    let dx = if f32::from(d.x) != 0.0 { f32::from(d.x) } else if ev.modifiers.shift { f32::from(d.y) } else { 0.0 };
                    if this.graph_scroll(&wid, dx, cx) {
                        cx.stop_propagation();
                    }
                }))
                .child(inner)
                .into_any_element()
        };
        g.lit = lit;
        g.edges = edges_drawn;
        el
    }

    /// A graph wider than its box scrolls horizontally.
    fn graph_scroll(&mut self, id: &str, dx: f32, cx: &mut Context<Self>) -> bool {
        let Some(g) = self.ci_leaves.graphs.borrow().get(id).cloned() else { return false };
        let mut g = g.borrow_mut();
        let x = (g.scroll_x - dx).clamp(0.0, g.max_x);
        if (x - g.scroll_x).abs() < 0.01 {
            return false;
        }
        g.scroll_x = x;
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(kind: &str, text: &str) -> CodeToken {
        CodeToken { kind: kind.into(), text: text.into() }
    }

    #[test]
    fn runs_split_tokens_at_the_marks() {
        let font = Font::default();
        let red = gpui::red();
        let blue = gpui::blue();
        let color = |k: &str| if k == "error" { red } else { blue };
        let (m, c) = (gpui::yellow(), gpui::green());
        let (text, runs) = code_runs(&[tok("error", "(fail)"), tok("plain", " store > get")], &[(7, 12, true)], &font, &color, (m, c));
        assert_eq!(text, "(fail) store > get");
        let lens: Vec<usize> = runs.iter().map(|r| r.len).collect();
        assert_eq!(lens, vec![6, 1, 5, 6]);
        assert_eq!(runs[0].color, red);
        assert_eq!(runs[2].background_color, Some(c));
        assert_eq!(runs[1].background_color, None);
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), text.len());
    }

    #[test]
    fn a_mark_across_tokens_and_code_points() {
        let font = Font::default();
        let color = |_: &str| gpui::white();
        let (text, runs) = code_runs(&[tok("success", "✓ ab"), tok("plain", "cd")], &[(3, 5, false)], &font, &color, (gpui::yellow(), gpui::green()));
        assert_eq!(text, "✓ abcd");
        let marked: usize = runs.iter().filter(|r| r.background_color.is_some()).map(|r| r.len).sum();
        assert_eq!(marked, 2, "b + c, across the token edge");
    }

    #[test]
    fn placeholders_fill() {
        assert_eq!(fill("{current} of {total}".into(), &[("current", "2".into()), ("total", "5".into())]), "2 of 5");
    }
}
