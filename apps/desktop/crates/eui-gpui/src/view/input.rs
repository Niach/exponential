//! Host-owned text fields on gpui-component input states: the Input /
//! Textarea `.field`, the Composer, a NumberField's and a ChipInput's
//! `input`, a searchable Select's `search`.
//!
//! The gpui state OWNS the text. Every edit bumps the field's revision and
//! (re)schedules a 150 ms debounce; the debounce sends the core
//! `event(change, {value})` (bound values write through) and the host an
//! `InputEvent::Change` with that revision. Blur / Enter flush and send a
//! `Commit` / `submit`. An echo (a changed prop value) is written into the
//! field only when it is not focused and no newer edit is outstanding,
//! unless it came with a PROGRAMMATIC write (#VAPP-288: an action's `set`,
//! a package action's `sets`, which move the surface's write epoch): then
//! the field adopts it even while focused, drops the outstanding edit and
//! puts the cursor at the end (the form that clears itself after Save). A
//! growing field (Composer, autosize Textarea) is re-measured on every edit
//! from its LIVE text.

use std::collections::HashSet;
use std::time::Duration;

use eui_core::surface::PlacedNode;
use gpui::{div, prelude::*, px, AnyElement, SharedString, App, Context, Entity, FocusHandle, Focusable as _, MouseButton, Pixels, Point, Subscription, Task, Window};
use gpui_component::input::{Input, InputEvent, InputState, TextareaState};
use serde_json::{json, Value};

use super::state::is_text_field;
use super::SurfaceView;
use crate::measure::display_text;
use crate::paint::icons;

/// The debounce of `InputEvent::Change` (and a Select's `search`).
pub const INPUT_DEBOUNCE: Duration = Duration::from_millis(150);

pub(crate) enum FieldInput {
    Line(Entity<InputState>),
    Multi(Entity<TextareaState>),
}

/// An `Input{type: password}` (A2UI's obscured TextField): its text is
/// drawn as bullets and never exposed to accessibility (eDE's lock screen
/// showed the typed password).
pub(crate) fn is_secret(n: &PlacedNode) -> bool {
    FieldKind::of(n) == FieldKind::Input && n.props.get("type").and_then(Value::as_str) == Some("password")
}

/// Which host field a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldKind {
    Input,
    Textarea,
    Composer,
    Number,
    Chips,
    Search,
}

impl FieldKind {
    fn of(n: &PlacedNode) -> FieldKind {
        match (n.component.as_str(), n.owner_component.as_deref()) {
            ("Composer", _) => FieldKind::Composer,
            ("Textarea", _) => FieldKind::Textarea,
            (_, Some("NumberField")) => FieldKind::Number,
            (_, Some("ChipInput")) => FieldKind::Chips,
            (_, Some("Select")) => FieldKind::Search,
            // EXP-1259: a SearchList's (or a searchable submenu's) field.
            ("SearchList", _) => FieldKind::Search,
            _ => FieldKind::Input,
        }
    }

    /// The prop the core echoes the field's text in.
    fn echo_prop(self) -> &'static str {
        match self {
            FieldKind::Number | FieldKind::Chips => "text",
            _ => "value",
        }
    }

    /// The field grows with its text (re-measure on every edit).
    fn grows(self, n: &PlacedNode) -> bool {
        self == FieldKind::Composer || (self == FieldKind::Textarea && n.props.get("autosize").and_then(Value::as_bool) == Some(true))
    }
}

/// One host-owned text field.
pub(crate) struct Field {
    pub input: FieldInput,
    pub kind: FieldKind,
    /// Bumped per local edit.
    pub revision: u64,
    /// The revision last sent to the core/host.
    pub flushed: u64,
    pub pending: Option<Task<()>>,
    /// The last prop value seen (the echo source).
    pub external: String,
    /// #VAPP-288: the surface's write epoch when this field last looked.
    pub seen_epoch: u64,
    pub placeholder: String,
    /// The text last sent or echoed in (an edit not yet seen by the
    /// subscription still counts as outstanding).
    pub synced: String,
    pub grows: bool,
    /// The field has the focus (its `Focus` / `Blur` events).
    focused: bool,
    /// VAPP-208: where the press that focuses this field went down.
    focusing_press: Option<Point<Pixels>>,
    _subscription: Subscription,
}

impl Field {
    pub fn value(&self, cx: &App) -> String {
        match &self.input {
            FieldInput::Line(s) => s.read(cx).value().to_string(),
            FieldInput::Multi(s) => s.read(cx).value().to_string(),
        }
    }

    pub fn set_value(&self, value: String, window: &mut Window, cx: &mut App) {
        match &self.input {
            FieldInput::Line(s) => s.update(cx, |s, cx| s.set_value(value, window, cx)),
            FieldInput::Multi(s) => s.update(cx, |s, cx| s.set_value(value, window, cx)),
        }
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.input {
            FieldInput::Line(s) => s.read(cx).focus_handle(cx),
            FieldInput::Multi(s) => s.read(cx).focus_handle(cx),
        }
    }

    fn set_placeholder(&self, placeholder: String, window: &mut Window, cx: &mut App) {
        match &self.input {
            FieldInput::Line(s) => s.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx)),
            FieldInput::Multi(s) => s.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx)),
        }
    }

    /// #VAPP-288: adopt a programmatic write even while focused: the value,
    /// the outstanding edit dropped, the cursor at the end.
    fn adopt(&mut self, value: String, focused: bool, window: &mut Window, cx: &mut App) {
        self.pending = None;
        self.flushed = self.revision;
        self.synced = value.clone();
        if self.value(cx) != value {
            // A single-line state puts the caret at the end on `set_value`;
            // a multi-line one resets it to the start (moving it focuses, so
            // only a focused one moves).
            let end = gpui_component::input::Position::new(value.matches('\n').count() as u32, value.rsplit('\n').next().unwrap_or("").chars().count() as u32);
            self.set_value(value, window, cx);
            if let (FieldInput::Multi(s), true) = (&self.input, focused) {
                s.update(cx, |s, cx| s.set_cursor_position(end, window, cx));
            }
        }
    }

    fn select_all(&self, window: &mut Window, cx: &mut App) {
        if let FieldInput::Line(s) = &self.input {
            s.update(cx, |s, cx| s.select_all(window, cx));
        }
    }

    /// The text revision bookkeeping: an echo may apply now.
    pub fn idle(&self, cx: &App) -> bool {
        self.pending.is_none() && self.flushed == self.revision && self.value(cx) == self.synced
    }
}

/// The author's placeholder (none = none: the web shows no built-in one).
fn placeholder_of(n: &PlacedNode) -> String {
    n.props.get("placeholder").and_then(Value::as_str).unwrap_or("").to_string()
}

impl SurfaceView {
    /// Create states for new fields, apply echoes, drop stale fields.
    pub(crate) fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut keep = HashSet::new();
        let epoch = self.surface.write_epoch();
        let fields: Vec<PlacedNode> = self.cache.nodes.iter().filter(|n| !n.removed && is_text_field(n)).cloned().collect();
        let mut regrow = Vec::new();
        for n in fields {
            keep.insert(n.id.clone());
            let kind = FieldKind::of(&n);
            let external = display_text(n.props.get(kind.echo_prop()));
            let placeholder = placeholder_of(&n);
            let secret = is_secret(&n);
            if let Some(f) = self.fields.get_mut(&n.id) {
                if let FieldInput::Line(s) = &f.input {
                    if s.read(cx).presentation().is_masked() != secret {
                        s.update(cx, |s, cx| s.set_masked(secret, window, cx));
                    }
                }
                if f.placeholder != placeholder {
                    f.placeholder = placeholder.clone();
                    f.set_placeholder(placeholder, window, cx);
                }
                let moved = f.external != external;
                // #VAPP-288: a programmatic write (the epoch moved) wins over
                // the focus when the bound value moved since the field last
                // looked OR last wrote (a press flushes the field's own edit
                // first, so its `set` may land back on the value last seen).
                let programmatic = epoch != f.seen_epoch && (moved || f.synced != external);
                if moved {
                    f.external = external.clone();
                }
                let before = f.grows.then(|| f.value(cx));
                if programmatic {
                    let focused = f.focus_handle(cx).is_focused(window);
                    f.adopt(external, focused, window, cx);
                } else if moved {
                    let focused = f.focus_handle(cx).is_focused(window);
                    if !focused && f.idle(cx) && f.value(cx) != external {
                        f.synced = external.clone();
                        f.set_value(external, window, cx);
                    }
                }
                f.seen_epoch = epoch;
                // #EXP-1768: `set_value` emits no Change, and this pass
                // measured the field's live text BEFORE the write (a body
                // that loads after the first paint measured as the empty
                // field): a growing field measures again at the new text.
                if before.is_some_and(|before| before != f.value(cx)) {
                    regrow.push(n.id.clone());
                }
                continue;
            }
            let id = n.id.clone();
            let multi = matches!(kind, FieldKind::Textarea | FieldKind::Composer);
            let (input, sub) = if !multi {
                let (ph, v) = (placeholder.clone(), external.clone());
                let state = cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v).masked(secret));
                let fid = id.clone();
                let sub = cx.subscribe_in(&state, window, move |this, _, ev: &InputEvent, window, cx| this.on_field_event(&fid, ev, window, cx));
                (FieldInput::Line(state), sub)
            } else {
                let (ph, v) = (placeholder.clone(), external.clone());
                let rows = n.props.get("rows").and_then(Value::as_u64).unwrap_or(3).max(1) as usize;
                let composer = kind == FieldKind::Composer;
                let state = cx.new(|cx| {
                    let s = TextareaState::new(window, cx).placeholder(ph).default_value(v).submit_on_enter(composer);
                    if composer {
                        s.auto_grow(1, 8)
                    } else {
                        s.rows(rows)
                    }
                });
                let fid = id.clone();
                let sub = cx.subscribe_in(&state, window, move |this, _, ev: &InputEvent, window, cx| this.on_field_event(&fid, ev, window, cx));
                (FieldInput::Multi(state), sub)
            };
            let grows = kind.grows(&n);
            self.fields.insert(id, Field { input, kind, revision: 0, flushed: 0, pending: None, synced: external.clone(), external, seen_epoch: epoch, placeholder, grows, focused: false, focusing_press: None, _subscription: sub });
        }
        self.fields.retain(|id, _| keep.contains(id));
        for id in regrow {
            if let Some(i) = self.cache.index_of(&id) {
                self.surface.mark_dirty(i);
            }
            cx.notify();
        }
    }

    fn on_field_event(&mut self, id: &str, ev: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            InputEvent::Change => {
                let Some(f) = self.fields.get_mut(id) else { return };
                f.revision += 1;
                let grows = f.grows;
                let fid = id.to_string();
                f.pending = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(INPUT_DEBOUNCE).await;
                    let _ = this.update_in(cx, |this, window, cx| this.flush_field(&fid, None, window, cx));
                }));
                if grows {
                    // The live text decides the height: measure again now.
                    if let Some(i) = self.cache.index_of(id) {
                        self.surface.mark_dirty(i);
                    }
                    cx.notify();
                }
            }
            InputEvent::PressEnter { shift, .. } => {
                let kind = self.fields.get(id).map(|f| f.kind);
                match kind {
                    Some(FieldKind::Composer) if !shift => self.submit_composer(id, window, cx),
                    Some(FieldKind::Textarea | FieldKind::Composer) | None => {}
                    Some(FieldKind::Chips) => {
                        self.flush_field(id, Some("submit"), window, cx);
                        self.clear_field(id, window, cx);
                    }
                    // EXP-1259: Enter in a search list picks its highlighted row.
                    Some(FieldKind::Search) if self.cache.index_of(id).and_then(|i| self.cache.node(i)).is_some_and(|n| n.component == "SearchList") => self.search_key(id, "Enter", window, cx),
                    Some(_) => self.flush_field(id, Some("submit"), window, cx),
                }
            }
            InputEvent::Blur => {
                if let Some(f) = self.fields.get_mut(id) {
                    f.focused = false;
                    f.focusing_press = None;
                }
                self.flush_field(id, Some("commit"), window, cx);
                if let Some(i) = self.cache.index_of(id) {
                    self.fire(i, "blur", None, cx);
                }
            }
            InputEvent::Focus => {
                // VAPP-208: a NumberField's focus selects its value, so typing
                // replaces it (web's `select()` on focus).
                if let Some(f) = self.fields.get_mut(id) {
                    f.focused = true;
                    if f.kind == FieldKind::Number {
                        f.select_all(window, cx);
                    }
                }
                self.track_focus(window, cx);
                cx.notify();
            }
        }
    }

    /// A press went down in field `id` (it may be the one that focuses it).
    fn field_press_down(&mut self, id: &str, at: Point<Pixels>) {
        if let Some(f) = self.fields.get_mut(id) {
            f.focusing_press = (f.kind == FieldKind::Number && !f.focused).then_some(at);
        }
    }

    /// VAPP-208: the press's own caret and any jitter's range land after the
    /// focus: the release of the press that focused a NumberField selects
    /// again, unless it was a real drag.
    fn field_press_up(&mut self, id: &str, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.fields.get_mut(id) else { return };
        let Some(from) = f.focusing_press.take() else { return };
        let (dx, dy) = (f32::from(at.x - from.x), f32::from(at.y - from.y));
        if dx * dx + dy * dy <= 16.0 {
            f.select_all(window, cx);
        }
    }

    /// Empty a field without an echo (a ChipInput after a chip was added).
    /// #EXP-1765: a pill Select's search clear: the field empties, the core
    /// hears the empty `search`, and the focus stays in the field.
    pub(crate) fn clear_search(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_field(id, window, cx);
        if let Some(index) = self.cache.index_of(id) {
            let events = self.surface.event(index, "change", Some(json!({"value": ""})));
            self.dispatch(events, None, cx);
        }
        self.focus_field(id, window, cx);
        cx.notify();
    }

    fn clear_field(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(f) = self.fields.get_mut(id) {
            f.set_value(String::new(), window, cx);
            f.synced.clear();
            f.revision += 1;
            f.flushed = f.revision;
            f.external.clear();
        }
    }

    /// VAPP-208: send every field's outstanding edit now, without a commit
    /// (the blur still commits).
    pub(crate) fn flush_edits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let due: Vec<String> = self.fields.iter().filter(|(_, f)| f.flushed < f.revision).map(|(id, _)| id.clone()).collect();
        for id in due {
            self.flush_field(&id, None, window, cx);
        }
    }

    /// Send the outstanding edit (and a commit/submit event) of field `id`.
    pub fn flush_field(&mut self, id: &str, commit: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.cache.index_of(id) else { return };
        let Some(f) = self.fields.get_mut(id) else { return };
        f.pending = None;
        let value = f.value(cx);
        f.synced = value.clone();
        let needs_change = f.flushed < f.revision;
        f.flushed = f.revision;
        let rev = f.revision;
        let kind = f.kind;
        if needs_change {
            let events = self.surface.event(index, "change", Some(json!({"value": value})));
            self.dispatch(events, Some(rev), cx);
            // A comma ends a chip: the core added it, the field empties.
            if kind == FieldKind::Chips && value.ends_with(',') {
                self.clear_field(id, window, cx);
            }
        }
        if let Some(e) = commit {
            let events = self.surface.event(index, e, Some(json!({"value": value})));
            self.dispatch(events, Some(rev), cx);
        }
    }

    /// Append `text` to field `id` through the platform text-input entry
    /// point (`replace_text_in_range`, what a committed keystroke calls) —
    /// for automation and tests; it never focuses the field.
    pub fn type_into(&mut self, id: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        use gpui::EntityInputHandler as _;
        let Some(f) = self.fields.get(id) else { return false };
        let end = f.value(cx).encode_utf16().count();
        match &f.input {
            FieldInput::Line(s) => s.update(cx, |s, cx| s.replace_text_in_range(Some(end..end), text, window, cx)),
            FieldInput::Multi(s) => s.update(cx, |s, cx| s.replace_text_in_range(Some(end..end), text, window, cx)),
        }
        true
    }

    /// Type `text` over the current selection through the platform text-input
    /// entry point (no range = the selection, what a keystroke asks for).
    pub fn type_over_selection(&mut self, id: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        use gpui::EntityInputHandler as _;
        let Some(f) = self.fields.get(id) else { return false };
        match &f.input {
            FieldInput::Line(s) => s.update(cx, |s, cx| s.replace_text_in_range(None, text, window, cx)),
            FieldInput::Multi(s) => s.update(cx, |s, cx| s.replace_text_in_range(None, text, window, cx)),
        }
        true
    }

    /// What field `id` does on gaining focus, for automation and tests (a test
    /// window has no native view for a really focused input).
    pub fn field_gained_focus(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.on_field_event(id, &InputEvent::Focus, window, cx);
    }

    /// Really focus field `id` (its gpui focus handle) and run its focus,
    /// for automation and tests (#VAPP-288: the sync reads the real focus).
    pub fn focus_field(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(handle) = self.fields.get(id).map(|f| f.focus_handle(cx)) else { return false };
        window.focus(&handle, cx);
        self.on_field_event(id, &InputEvent::Focus, window, cx);
        true
    }

    /// Blur field `id` (the window's focus cleared) and run its blur, for
    /// automation and tests.
    pub fn blur_field(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        window.blur(cx);
        self.on_field_event(id, &InputEvent::Blur, window, cx);
    }

    /// #EDE-65 lane O: field `id`'s gpui focus handle (a host that moves
    /// focus to or from it, the IDE's composer).
    pub fn field_focus_handle(&self, id: &str, cx: &App) -> Option<gpui::FocusHandle> {
        self.fields.get(id).map(|f| f.focus_handle(cx))
    }

    /// The current text of field `id`.
    pub fn field_text(&self, id: &str, cx: &App) -> Option<String> {
        self.fields.get(id).map(|f| f.value(cx))
    }

    /// Field `id` draws its text as bullets (an `Input{type: password}`).
    pub fn field_masked(&self, id: &str, cx: &App) -> Option<bool> {
        match &self.fields.get(id)?.input {
            FieldInput::Line(s) => Some(s.read(cx).presentation().is_masked()),
            FieldInput::Multi(_) => Some(false),
        }
    }

    /// The composer's send: `submit {value}` then an emptied field (a busy
    /// composer sends `stop`).
    pub fn submit_composer(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.cache.index_of(id) else { return };
        let busy = self.cache.node(index).is_some_and(|n| matches!(n.props.get("busy"), Some(Value::Bool(true))));
        if busy {
            return self.fire(index, "stop", None, cx);
        }
        let Some(text) = self.fields.get(id).map(|f| f.value(cx).trim().to_string()) else { return };
        if text.is_empty() && !self.host.composer_submits_empty(id, cx) {
            return;
        }
        self.flush_field(id, None, window, cx);
        let rev = self.fields.get(id).map(|f| f.revision).unwrap_or(0);
        let events = self.surface.event(index, "submit", Some(json!({"value": text})));
        self.dispatch(events, Some(rev), cx);
        self.clear_field(id, window, cx);
        let rev = self.fields.get(id).map(|f| f.revision).unwrap_or(0);
        let events = self.surface.event(index, "change", Some(json!({"value": ""})));
        self.dispatch(events, Some(rev), cx);
        self.surface.mark_dirty(index);
    }

    /// The gpui-component input of a field leaf inside its content box (the
    /// frame div paints the recipe chrome; the input draws none). A Select
    /// search field shows its glyph at the start.
    /// #EXP-1629: a `row` Input's label (its recipe's type, never shrinking)
    /// and the value column (`flex_1`, `min_w_0`) holding the bare field.
    fn input_row(&self, lcx: &crate::paint::natives::LeafCx, row: gpui::Div, field: AnyElement) -> gpui::Div {
        let label = lcx.str("label");
        let row = if label.is_empty() {
            row
        } else {
            let lp = lcx.part_props("Input", "label", &[]);
            let ls = lcx.part("Input", "label", &[]);
            let mut font = lcx.font.clone();
            if let Some(w) = lp.get("fontWeight").and_then(Value::as_f64) {
                font.weight = gpui::FontWeight(w as f32);
            }
            let size = crate::paint::parts::px_prop(&lp, "fontSize").map(|f| f * lcx.scale).unwrap_or(lcx.text_style.font_size);
            let lh = crate::paint::parts::px_prop(&lp, "lineHeight").map(|f| f * lcx.scale).unwrap_or(lcx.text_style.line_height);
            row.child(div().flex_none().whitespace_nowrap().font(font).text_size(px(size)).line_height(px(lh)).text_color(ls.color.unwrap_or(lcx.ink)).when_some(ls.opacity, |d, o| d.opacity(o)).child(gpui::SharedString::from(label.to_string())))
        };
        row.child(div().flex_1().min_w_0().text_color(lcx.ink.opacity(0.7)).child(field))
    }

    pub(crate) fn field_element(&self, lcx: &crate::paint::natives::LeafCx, cx: &mut Context<Self>) -> Option<AnyElement> {
        let n = lcx.node;
        let f = self.fields.get(&n.id)?;
        let (x, y, w, h) = lcx.inner();
        let [_, _, _, _] = lcx.style.insets();
        // #EXP-1629: a `row`/`code` Textarea is the IDE's chromeless group
        // field, whose recipe padding INCLUDES gpui-component's own editor
        // inset (`Size::Medium`: `input_px` × `input_py`): the widget is
        // placed that inset outside the content box, so its text lands on it.
        let inset_variant = f.kind == FieldKind::Textarea && matches!(n.props.get("variant").and_then(Value::as_str), Some("row" | "code"));
        let (x, y, w, h) = if inset_variant {
            let (ex, ey) = (f32::from(gpui_component::Size::Medium.input_px()), f32::from(gpui_component::Size::Medium.input_py()));
            (x - ex, y - ey, w + 2.0 * ex, h + 2.0 * ey)
        } else {
            (x, y, w, h)
        };
        let disabled = matches!(n.props.get("disabled"), Some(Value::Bool(true))) || n.states.iter().any(|s| s == "disabled");
        let font = lcx.font.clone();
        let size = lcx.text_style.font_size;
        // #EXP-1787: the field's ink is the node's own `color` when set (the
        // core lands it on the `field` part as an authored key); it colours
        // only the TYPED text: gpui-component draws the placeholder and the
        // caret in its editor style's `muted_foreground` and `caret`.
        let ink = lcx.ink;
        let icon = (f.kind == FieldKind::Search).then(|| n.props.get("icon").and_then(Value::as_str).unwrap_or("search").to_string());
        // The glyph-to-text gap: the field's recipe `gap`, else 8.
        let icon_gap = if lcx.style.gap > 0.0 { lcx.style.gap } else { 8.0 };
        // #EXP-1765: a Select search's glyph takes its `searchIcon` part's size.
        let glyph_size = if n.owner_component.as_deref() == Some("Select") { crate::paint::parts::px_prop(&lcx.part_props("Select", "searchIcon", &[]), "width").map(|w| w * lcx.scale).unwrap_or(16.0) } else { 16.0 };
        let icon_w = if icon.is_some() { glyph_size + icon_gap } else { 0.0 };
        // #EXP-1765: a `pill` Select's search ends in a clear button while it
        // holds text (the IDE's `search_field` suffix: an xsmall ghost icon
        // button: its box 1.25× and its glyph 0.75× the search glyph, `icon_gap`
        // after the text).
        let pill_search = n.owner_component.as_deref() == Some("Select") && n.part.as_deref() == Some("search") && n.owner.as_ref().and_then(|o| self.cache.index_of(o)).and_then(|i| self.cache.node(i)).is_some_and(|o| o.props.get("triggerVariant").and_then(Value::as_str) == Some("pill"));
        let clear_box = if pill_search && !f.value(cx).is_empty() { glyph_size * 1.25 } else { 0.0 };
        let clear_w = if clear_box > 0.0 { clear_box + icon_gap } else { 0.0 };
        let field_w = (w - icon_w - clear_w).max(0.0);
        // A disabled field dims ONCE, through its frame's recipe opacity (the
        // reference's `color: inherit` value under the field's `disabled`
        // opacity): gpui-component's own disabled look would halve the text
        // again, so the input paints enabled and the field is made inert
        // here instead (no tab stop: `is_focusable`; no pointer: the
        // occluding cover below).
        // #EXP-1629: a `row` Input IS the Group row (`glass_input_row`): the
        // label leads, the value fills the rest end-aligned at 70% ink.
        let row_variant = f.kind == FieldKind::Input && n.props.get("variant").and_then(Value::as_str) == Some("row");
        let el = match &f.input {
            FieldInput::Line(s) if row_variant => Input::new(s).appearance(false).font(font).text_size(px(size)).text_color(ink.opacity(0.7)).h_auto().px_0().py_0().text_right().into_any_element(),
            // Round 2 §7: the text starts at border + paddingHorizontal (the
            // field's own content box): no input padding of its own.
            FieldInput::Line(s) => Input::new(s).appearance(false).font(font).text_size(px(size)).text_color(ink).px_0().py_0().w(px(field_w)).h(px(h)).into_any_element(),
            FieldInput::Multi(s) => gpui_component::input::Textarea::new(s).appearance(false).font(font).text_size(px(size)).text_color(ink).w(px(field_w)).h(px(h)).into_any_element(),
        };
        // A press in the field is the field's (caret, selection): it never
        // bubbles to a host's own press handling, like the pressable leaves claim theirs.
        let (down, up) = (n.id.clone(), n.id.clone());
        let mut row = lcx
            .row(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)))
            .items_center()
            .gap(px(if icon.is_some() { icon_gap } else if row_variant { lcx.style.gap } else { 0.0 }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                    this.field_press_down(&down, ev.position);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, ev: &gpui::MouseUpEvent, window, cx| this.field_press_up(&up, ev.position, window, cx)));
        if let Some(name) = icon {
            // The placeholder's ink (`mutedForeground`), as the IDE's
            // search rows draw their glyph; else the text's at 60%.
            let (glyph, fade) = lcx.theme_color("mutedForeground").map_or((ink, 0.6), |muted| (muted, 1.0));
            row = row.child(div().flex_none().opacity(fade).child(icons::concept(lcx.host, &name, glyph_size, glyph)));
        }
        let mut row = if row_variant { self.input_row(lcx, row, el) } else { row.child(el) };
        if clear_box > 0.0 {
            let id = n.id.clone();
            let muted = lcx.theme_color("mutedForeground").unwrap_or(ink);
            let label = SharedString::from(self.builtin_string("clearSearch"));
            row = row.child(
                div()
                    .id(SharedString::from(format!("{}.clear", n.id)))
                    .flex_none()
                    .size(px(clear_box))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .child(icons::concept(lcx.host, eui_core::layout_tree::builtin_icon("Select.clear").unwrap_or("ui-clear"), glyph_size * 0.75, muted))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| this.clear_search(&id, window, cx))),
            );
        }
        if disabled {
            return Some(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).child(row.left_0().top_0()).child(div().absolute().size_full().occlude()).into_any_element());
        }
        Some(row.into_any_element())
    }
}
