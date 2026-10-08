//! Host-owned text fields (Input / Textarea `.field`, Composer) on
//! gpui-component input states, and the Select / DatePicker popups.
//!
//! The gpui state OWNS the text. Every edit bumps the field's revision and
//! (re)schedules a 150 ms debounce; the debounce sends the core
//! `event(change, {value})` (bound values write through) and the host an
//! `InputEvent::Change` with that revision. Blur / Enter flush and send a
//! `Commit`. An echo (a changed prop value) is written into the field only
//! when it is not focused and no newer edit is outstanding.

use std::collections::HashSet;
use std::time::Duration;

use exponential_ui::surface::PlacedNode;
use gpui::{div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, Focusable as _, SharedString, Subscription, Task, Window};
use gpui_component::input::{Input, InputEvent, InputState, TextareaState};
use serde_json::{json, Value};

use super::state::is_text_field;
use super::SurfaceView;
use crate::measure::display_text;
use crate::paint::date;
use crate::paint::icons::{self, Glyph};
use crate::paint::natives::styled_box;
use crate::paint::parts::{part_props, part_visual, px_prop, spacing, theme_color};
use crate::paint::PaintStyle;

/// The debounce of `InputEvent::Change`.
pub const INPUT_DEBOUNCE: Duration = Duration::from_millis(150);

pub(crate) enum FieldInput {
    Line(Entity<InputState>),
    Multi(Entity<TextareaState>),
}

/// One host-owned text field.
pub(crate) struct Field {
    pub input: FieldInput,
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
    pub composer: bool,
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

/// The open Select / DatePicker popup.
pub(crate) struct Popup {
    pub field: String,
    pub kind: PopupKind,
}

pub(crate) enum PopupKind {
    Select { search: Option<(Entity<InputState>, Subscription)> },
    Date { year: i32, month: u32 },
}

fn placeholder_of(n: &PlacedNode) -> String {
    let p = n.props.get("placeholder").and_then(Value::as_str).unwrap_or("");
    if p.is_empty() && n.component == "Composer" {
        "Message".to_string()
    } else {
        p.to_string()
    }
}

impl SurfaceView {
    /// Create states for new fields, apply echoes, drop stale fields.
    pub(crate) fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut keep = HashSet::new();
        let fields: Vec<PlacedNode> = self.cache.nodes.iter().filter(|n| is_text_field(n)).cloned().collect();
        for n in fields {
            keep.insert(n.id.clone());
            let external = display_text(n.props.get("value"));
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
            let composer = n.component == "Composer";
            let id = n.id.clone();
            let (input, sub) = if n.component == "Input" {
                let (ph, v) = (placeholder.clone(), external.clone());
                let state = cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v));
                let fid = id.clone();
                let sub = cx.subscribe_in(&state, window, move |this, _, ev: &InputEvent, window, cx| this.on_field_event(&fid, ev, window, cx));
                (FieldInput::Line(state), sub)
            } else {
                let (ph, v) = (placeholder.clone(), external.clone());
                let rows = n.props.get("rows").and_then(Value::as_u64).unwrap_or(3).max(1) as usize;
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
            self.fields.insert(id, Field { input, revision: 0, flushed: 0, pending: None, synced: external.clone(), external, placeholder, composer, _subscription: sub });
        }
        self.fields.retain(|id, _| keep.contains(id));
    }

    fn on_field_event(&mut self, id: &str, ev: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            InputEvent::Change => {
                let Some(f) = self.fields.get_mut(id) else { return };
                f.revision += 1;
                let fid = id.to_string();
                f.pending = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(INPUT_DEBOUNCE).await;
                    let _ = this.update_in(cx, |this, window, cx| this.flush_field(&fid, None, window, cx));
                }));
            }
            InputEvent::PressEnter { shift, .. } => {
                let composer = self.fields.get(id).is_some_and(|f| f.composer);
                let multi = self.fields.get(id).is_some_and(|f| matches!(f.input, FieldInput::Multi(_)));
                if composer && !shift {
                    self.submit_composer(id, window, cx);
                } else if !multi {
                    self.flush_field(id, Some("submit"), window, cx);
                }
            }
            InputEvent::Blur => self.flush_field(id, Some("commit"), window, cx),
            InputEvent::Focus => cx.notify(),
        }
    }

    /// Send the outstanding edit (and a commit event) of field `id`.
    pub fn flush_field(&mut self, id: &str, commit: Option<&str>, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.cache.index_of(id) else { return };
        let Some(f) = self.fields.get_mut(id) else { return };
        f.pending = None;
        let value = f.value(cx);
        f.synced = value.clone();
        let needs_change = f.flushed < f.revision;
        f.flushed = f.revision;
        let rev = f.revision;
        if needs_change {
            let events = self.surface.event(index, "change", Some(json!({"value": value})));
            self.dispatch(events, Some(rev), cx);
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
        if let Some(f) = self.fields.get_mut(id) {
            f.set_value(String::new(), window, cx);
            f.synced.clear();
            f.revision += 1;
            f.flushed = f.revision;
            f.external.clear();
            let rev = f.revision;
            let events = self.surface.event(index, "change", Some(json!({"value": ""})));
            self.dispatch(events, Some(rev), cx);
        }
    }

    /// The gpui-component input of a field leaf, styled from the `.field`
    /// visual (the frame div paints the recipe chrome; the input draws none).
    pub(crate) fn field_element(&self, lcx: &crate::paint::natives::LeafCx) -> Option<AnyElement> {
        let (n, style, w, h, font, size, ink) = (lcx.node, lcx.style, lcx.w, lcx.h, lcx.font.clone(), lcx.text_style.font_size, lcx.ink);
        let f = self.fields.get(&n.id)?;
        let disabled = matches!(n.props.get("disabled"), Some(Value::Bool(true)));
        let border = style.border_width;
        let el = match &f.input {
            FieldInput::Line(s) => Input::new(s)
                .appearance(false)
                .disabled(disabled)
                .font(font)
                .text_size(px(size))
                .text_color(ink)
                .px(px(style.pad_h))
                .w(px((w - 2.0 * border).max(0.0)))
                .h(px((h - 2.0 * border).max(0.0)))
                .into_any_element(),
            FieldInput::Multi(s) => gpui_component::input::Textarea::new(s)
                .appearance(false)
                .disabled(disabled)
                .font(font)
                .text_size(px(size))
                .text_color(ink)
                .w(px((w - 2.0 * border).max(0.0)))
                .h(px((h - 2.0 * border).max(0.0)))
                .into_any_element(),
        };
        Some(div().absolute().left(px(border)).top(px(border)).child(el).into_any_element())
    }

    // -- Select / DatePicker popups ---------------------------------------------

    pub(crate) fn toggle_popup(&mut self, index: u32, date_picker: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(n) = self.cache.node(index).cloned() else { return };
        if self.popup.as_ref().is_some_and(|p| p.field == n.id) {
            self.popup = None;
            cx.notify();
            return;
        }
        if self.just_dismissed.take().as_deref() == Some(n.id.as_str()) {
            cx.notify();
            return;
        }
        let kind = if date_picker {
            let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
            let external = n.props.get("value").cloned().unwrap_or(Value::Null);
            let value = display_text(Some(&self.mirrored(&owner, &external)));
            let (year, month, _) = date::parse_iso(&value).unwrap_or_else(date::today);
            PopupKind::Date { year, month }
        } else {
            let searchable = matches!(n.props.get("searchable"), Some(Value::Bool(true)));
            let search = searchable.then(|| {
                let state = cx.new(|cx| InputState::new(window, cx).placeholder("Search…"));
                let sub = cx.subscribe_in(&state, window, |_, _, _: &InputEvent, _, cx| cx.notify());
                (state, sub)
            });
            PopupKind::Select { search }
        };
        self.popup = Some(Popup { field: n.id.clone(), kind });
        cx.notify();
    }

    pub(crate) fn select_option(&mut self, index: u32, value: Value, cx: &mut Context<Self>) {
        let Some(n) = self.cache.node(index).cloned() else { return };
        let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
        let multiple = matches!(n.props.get("multiple"), Some(Value::Bool(true)));
        let external = n.props.get("value").cloned().unwrap_or(Value::Null);
        let next = if multiple {
            let mut set: Vec<Value> = match self.mirrored(&owner, &external) {
                Value::Array(a) => a,
                Value::Null => Vec::new(),
                v => vec![v],
            };
            match set.iter().position(|v| *v == value) {
                Some(p) => {
                    set.remove(p);
                }
                None => set.push(value),
            }
            Value::Array(set)
        } else {
            self.popup = None;
            value
        };
        self.set_mirror(&owner, external, next.clone());
        self.fire(index, "change", Some(json!({"value": next})), cx);
    }

    fn close_popup_outside(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.popup.take() {
            self.just_dismissed = Some(p.field);
            cx.notify();
        }
    }

    /// The open popup, positioned under its field (surface coordinates).
    pub(crate) fn paint_popup(&self, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.popup.as_ref()?;
        let index = self.cache.index_of(&p.field)?;
        let n = self.cache.node(index)?;
        let f = *self.frames.get(index as usize)?;
        let theme = self.surface.theme().cloned();
        let theme = theme.as_deref();
        let mode = self.surface.mode();
        let component = n.owner_component.clone().unwrap_or_else(|| n.component.clone());
        let owner_index = self.cache.owner_of(index);
        let owner_props = self.cache.node(owner_index).map(|o| o.props.clone()).unwrap_or_default();
        let ink = self.inks.get(index as usize).copied().unwrap_or(gpui::white());
        let open = vec!["open".to_string()];
        let content_part = if matches!(p.kind, PopupKind::Date { .. }) { "calendar" } else { "content" };
        let cstyle = PaintStyle::from_visual(&part_visual(theme, mode, &component, content_part, &owner_props, &open));
        let cprops = part_props(theme, mode, &component, content_part, &owner_props, &open);
        let pad = px_prop(&cprops, "padding").unwrap_or(4.0);
        let fg = cstyle.color.unwrap_or(ink);
        let fallback_bg = theme_color(theme, mode, "popover").unwrap_or(gpui::black());
        let cstyle = PaintStyle { bg: cstyle.bg.or(Some(fallback_bg)), ..cstyle };
        let accent = theme_color(theme, mode, "accent").unwrap_or(fg.opacity(0.1));
        let gap = 4.0;
        let x = f.x;
        let y = f.y + f.h + gap;
        let field_index = index;
        let body: AnyElement = match &p.kind {
            PopupKind::Select { search } => {
                let item_props = part_props(theme, mode, &component, "item", &owner_props, &[]);
                let item_h = px_prop(&item_props, "height").unwrap_or(32.0);
                let item_pad = px_prop(&item_props, "paddingHorizontal").unwrap_or(8.0);
                let item_style = PaintStyle::from_visual(&part_visual(theme, mode, &component, "item", &owner_props, &[]));
                let selected_style = PaintStyle::from_visual(&part_visual(theme, mode, &component, "item", &owner_props, &["selected".to_string()]));
                let query = search.as_ref().map(|(s, _)| s.read(cx).value().to_lowercase()).unwrap_or_default();
                let external = n.props.get("value").cloned().unwrap_or(Value::Null);
                let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
                let chosen: Vec<String> = match self.mirrored(&owner, &external) {
                    Value::Array(a) => a.iter().map(|v| display_text(Some(v))).collect(),
                    Value::Null => Vec::new(),
                    v => vec![display_text(Some(&v))],
                };
                let options = n.props.get("options").and_then(Value::as_array).cloned().unwrap_or_default();
                let mut col = div().flex().flex_col();
                if let Some((s, _)) = search {
                    col = col.child(div().pb(px(4.0)).mb(px(4.0)).border_b_1().border_color(fg.opacity(0.15)).child(Input::new(s).appearance(false).text_size(px(14.0))));
                }
                for (i, o) in options.iter().enumerate() {
                    let label = display_text(o.get("label"));
                    if !query.is_empty() && !label.to_lowercase().contains(&query) {
                        continue;
                    }
                    let value = o.get("value").cloned().unwrap_or(Value::Null);
                    let selected = chosen.contains(&display_text(Some(&value)));
                    let disabled = matches!(o.get("disabled"), Some(Value::Bool(true)));
                    let st = if selected { &selected_style } else { &item_style };
                    let color = st.color.unwrap_or(fg);
                    let mut row = div()
                        .id(SharedString::from(format!("{}.option.{i}", n.id)))
                        .h(px(item_h))
                        .px(px(item_pad))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(st.radius))
                        .text_color(color)
                        .when_some(st.bg, |d, bg| d.bg(bg))
                        .role(gpui::Role::ListBoxOption)
                        .aria_label(SharedString::from(label.clone()))
                        .aria_selected(selected)
                        .when(disabled, |d| d.opacity(0.5));
                    if !disabled {
                        row = row.cursor_pointer().hover(move |s| s.bg(accent)).on_click(cx.listener(move |this, _, _, cx| this.select_option(field_index, value.clone(), cx)));
                    }
                    if let Some(icon) = o.get("icon").and_then(Value::as_str) {
                        row = row.child(icons::concept(self.host.as_ref(), icon, 16.0, color));
                    }
                    row = row.child(div().flex_1().whitespace_nowrap().child(SharedString::from(label)));
                    if selected {
                        row = row.child(icons::glyph(Glyph::Check, 16.0, color));
                    }
                    col = col.child(row);
                }
                col.into_any_element()
            }
            PopupKind::Date { year, month } => {
                let (year, month) = (*year, *month);
                let external = n.props.get("value").cloned().unwrap_or(Value::Null);
                let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
                let selected = date::parse_iso(&display_text(Some(&self.mirrored(&owner, &external))));
                let min = n.props.get("min").and_then(Value::as_str).and_then(date::parse_iso).map(|(y, m, d)| date::days_from_civil(y, m, d));
                let max = n.props.get("max").and_then(Value::as_str).and_then(date::parse_iso).map(|(y, m, d)| date::days_from_civil(y, m, d));
                let day_props = part_props(theme, mode, &component, "day", &owner_props, &[]);
                let cell = px_prop(&day_props, "width").unwrap_or(32.0);
                let day_style = PaintStyle::from_visual(&part_visual(theme, mode, &component, "day", &owner_props, &[]));
                let sel_style = PaintStyle::from_visual(&part_visual(theme, mode, &component, "day", &owner_props, &["selected".to_string()]));
                let muted = theme_color(theme, mode, "mutedForeground").unwrap_or(fg.opacity(0.6));
                let nav = |id: &str, glyph: Glyph, delta: i32| {
                    div()
                        .id(SharedString::from(format!("{}.{id}", n.id)))
                        .size(px(28.0))
                        .rounded(px(6.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |s| s.bg(accent))
                        .role(gpui::Role::Button)
                        .aria_label(if delta < 0 { "Previous month" } else { "Next month" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(Popup { kind: PopupKind::Date { year, month }, .. }) = this.popup.as_mut() {
                                let (y, m) = date::add_months(*year, *month, delta);
                                *year = y;
                                *month = m;
                                cx.notify();
                            }
                        }))
                        .child(icons::glyph(glyph, 16.0, fg))
                };
                let head = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(nav("prev", Glyph::ChevronLeft, -1))
                    .child(div().font_weight(gpui::FontWeight(500.0)).child(SharedString::from(format!("{} {}", date::MONTH_NAMES[(month - 1) as usize], year))))
                    .child(nav("next", Glyph::ChevronRight, 1));
                let mut grid = div().flex().flex_row().flex_wrap().w(px(7.0 * cell + 6.0 * 2.0)).gap(px(2.0));
                for w in date::WEEKDAYS {
                    grid = grid.child(div().w(px(cell)).h(px(24.0)).flex().items_center().justify_center().text_size(px(12.0)).text_color(muted).child(w));
                }
                for (i, (y, m, d, inside)) in date::month_grid(year, month).into_iter().enumerate() {
                    let z = date::days_from_civil(y, m, d);
                    let out_of_range = min.is_some_and(|lo| z < lo) || max.is_some_and(|hi| z > hi);
                    let is_sel = selected == Some((y, m, d));
                    let st = if is_sel { &sel_style } else { &day_style };
                    let color = st.color.unwrap_or(fg);
                    let mut cellel = div()
                        .id(SharedString::from(format!("{}.day.{i}", n.id)))
                        .size(px(cell))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(st.radius.min(cell / 2.0)))
                        .text_size(px(14.0))
                        .text_color(color)
                        .when_some(st.bg, |d, bg| d.bg(bg))
                        .when(!inside, |d| d.opacity(0.4))
                        .role(gpui::Role::Button)
                        .aria_label(SharedString::from(date::iso(y, m, d)))
                        .aria_selected(is_sel)
                        .child(SharedString::from(d.to_string()));
                    if out_of_range {
                        cellel = cellel.opacity(0.3);
                    } else {
                        cellel = cellel.cursor_pointer().when(!is_sel, |c| c.hover(move |s| s.bg(accent))).on_click(cx.listener(move |this, _, _, cx| this.pick_date(field_index, (y, m, d), cx)));
                    }
                    grid = grid.child(cellel);
                }
                div().flex().flex_col().gap(px(spacing(theme, "sm"))).child(head).child(grid).into_any_element()
            }
        };
        let min_w = if matches!(p.kind, PopupKind::Date { .. }) { 0.0 } else { f.w.max(160.0) };
        let panel = styled_box(div().id(SharedString::from(format!("{}.popup", n.id))).absolute().left(px(x)).top(px(y)).min_w(px(min_w)).p(px(pad)).text_color(fg).text_size(px(14.0)).line_height(px(20.0)), &cstyle, 1000.0, 1000.0)
            .rounded(px(cstyle.radius))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_popup_outside(cx)))
            .role(if matches!(p.kind, PopupKind::Date { .. }) { gpui::Role::Dialog } else { gpui::Role::ListBox })
            .child(body);
        Some(div().absolute().left_0().top_0().child(panel).into_any_element())
    }
}
