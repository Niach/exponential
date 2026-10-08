//! Host-owned text fields on gpui-component input states: the Input /
//! Textarea `.field`, the Composer, a NumberField's and a ChipInput's
//! `input`, a searchable Select's `search`.
//!
//! The gpui state OWNS the text. Every edit bumps the field's revision and
//! (re)schedules a 150 ms debounce; the debounce sends the core
//! `event(change, {value})` (bound values write through) and the host an
//! `InputEvent::Change` with that revision. Blur / Enter flush and send a
//! `Commit` / `submit`. An echo (a changed prop value) is written into the
//! field only when it is not focused and no newer edit is outstanding. A
//! growing field (Composer, autosize Textarea) is re-measured on every edit
//! from its LIVE text.

use std::collections::HashSet;
use std::time::Duration;

use exponential_ui::surface::PlacedNode;
use gpui::{div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, Focusable as _, Subscription, Task, Window};
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
    pub placeholder: String,
    /// The text last sent or echoed in (an edit not yet seen by the
    /// subscription still counts as outstanding).
    pub synced: String,
    pub grows: bool,
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
        let fields: Vec<PlacedNode> = self.cache.nodes.iter().filter(|n| !n.removed && is_text_field(n)).cloned().collect();
        for n in fields {
            keep.insert(n.id.clone());
            let kind = FieldKind::of(&n);
            let external = display_text(n.props.get(kind.echo_prop()));
            let placeholder = placeholder_of(&n);
            if let Some(f) = self.fields.get_mut(&n.id) {
                if f.placeholder != placeholder {
                    f.placeholder = placeholder.clone();
                    f.set_placeholder(placeholder, window, cx);
                }
                if f.external != external {
                    f.external = external.clone();
                    let focused = f.focus_handle(cx).is_focused(window);
                    if !focused && f.idle(cx) && f.value(cx) != external {
                        f.synced = external.clone();
                        f.set_value(external, window, cx);
                    }
                }
                continue;
            }
            let id = n.id.clone();
            let multi = matches!(kind, FieldKind::Textarea | FieldKind::Composer);
            let (input, sub) = if !multi {
                let (ph, v) = (placeholder.clone(), external.clone());
                let state = cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v));
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
            self.fields.insert(id, Field { input, kind, revision: 0, flushed: 0, pending: None, synced: external.clone(), external, placeholder, grows, _subscription: sub });
        }
        self.fields.retain(|id, _| keep.contains(id));
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
                    Some(_) => self.flush_field(id, Some("submit"), window, cx),
                }
            }
            InputEvent::Blur => {
                self.flush_field(id, Some("commit"), window, cx);
                if let Some(i) = self.cache.index_of(id) {
                    self.fire(i, "blur", None, cx);
                }
            }
            InputEvent::Focus => {
                self.track_focus(window, cx);
                cx.notify();
            }
        }
    }

    /// Empty a field without an echo (a ChipInput after a chip was added).
    fn clear_field(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(f) = self.fields.get_mut(id) {
            f.set_value(String::new(), window, cx);
            f.synced.clear();
            f.revision += 1;
            f.flushed = f.revision;
            f.external.clear();
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

    /// The current text of field `id`.
    pub fn field_text(&self, id: &str, cx: &App) -> Option<String> {
        self.fields.get(id).map(|f| f.value(cx))
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
        if text.is_empty() {
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
    pub(crate) fn field_element(&self, lcx: &crate::paint::natives::LeafCx) -> Option<AnyElement> {
        let n = lcx.node;
        let f = self.fields.get(&n.id)?;
        let (x, y, w, h) = lcx.inner();
        let [_, _, _, _] = lcx.style.insets();
        let disabled = matches!(n.props.get("disabled"), Some(Value::Bool(true))) || n.states.iter().any(|s| s == "disabled");
        let font = lcx.font.clone();
        let size = lcx.text_style.font_size;
        let ink = lcx.ink;
        let icon = (f.kind == FieldKind::Search).then(|| n.props.get("icon").and_then(Value::as_str).unwrap_or("search").to_string());
        let icon_w = if icon.is_some() { 16.0 + 8.0 } else { 0.0 };
        let field_w = (w - icon_w).max(0.0);
        let el = match &f.input {
            // Round 2 §7: the text starts at border + paddingHorizontal (the
            // field's own content box): no input padding of its own.
            FieldInput::Line(s) => Input::new(s).appearance(false).disabled(disabled).font(font).text_size(px(size)).text_color(ink).px_0().py_0().w(px(field_w)).h(px(h)).into_any_element(),
            FieldInput::Multi(s) => gpui_component::input::Textarea::new(s).appearance(false).disabled(disabled).font(font).text_size(px(size)).text_color(ink).w(px(field_w)).h(px(h)).into_any_element(),
        };
        let mut row = lcx.row(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))).items_center().gap(px(if icon.is_some() { 8.0 } else { 0.0 }));
        if let Some(name) = icon {
            row = row.child(div().flex_none().opacity(0.6).child(icons::concept(lcx.host, &name, 16.0, ink)));
        }
        Some(row.child(el).into_any_element())
    }
}
