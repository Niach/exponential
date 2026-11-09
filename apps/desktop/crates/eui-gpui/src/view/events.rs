//! Interaction: presses (pointer and keyboard), the OutEvent dispatch to the
//! host, keyboard focus in paint order with the layer focus trap and the
//! `catalog/a11y.json` keys (roving arrows, Home/End, menus, listboxes,
//! calendars, sliders, number fields — mirrored in RTL), Escape and
//! dismissal (respecting `dismissible`), hover-opened triggers with the
//! platform delay, toast timers, scrolling, slider and scrollbar drags, the
//! context menu, file picks and drops, the clipboard.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eui_core::layout_tree::{LayerClass, ToastSpec};
use eui_core::surface::{Layer, OutEvent};
use eui_core::hold::HoldInput;
use gpui::{ClipboardItem, Context, KeyDownEvent, KeyUpEvent, MouseMoveEvent, MouseUpEvent, PathPromptOptions, Pixels, Point, ScrollEvent, Window};
use serde_json::{json, Value};

use super::state::{focus_order, is_focusable, is_text_field, next_focus, NodeFlags};
use super::{Drag, SurfaceView, ToastTimer};
use crate::host::{ActionEvent, FilePickRequest, FunctionCallEvent, InputEvent, InputKind, UploadEvent};

/// The hover delay of tooltips and hover cards.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(300);
/// How long the pointer may travel from a hover trigger to its card.
pub const HOVER_GRACE: Duration = Duration::from_millis(120);
/// How long a CodeBlock shows `copied`.
pub const COPIED_RESET: Duration = Duration::from_millis(2000);

/// How far a Drawer is dragged toward its edge before it dismisses.
pub const SHEET_DISMISS_PX: f32 = 64.0;
/// EXP-1555: how long overlay scrollbars stay after the last scroll step.

/// A sheet drag's offset: the pointer's travel toward the sheet's edge only.
pub fn sheet_offset(side: &str, x: f32, y: f32) -> (f32, f32) {
    match side {
        "bottom" => (0.0, y.max(0.0)),
        "top" => (0.0, y.min(0.0)),
        "left" => (x.min(0.0), 0.0),
        "right" => (x.max(0.0), 0.0),
        _ => (0.0, 0.0),
    }
}

/// A slider value at pointer x: clamped to [min, max], snapped to `step`
/// (the minimum sits at the right edge in RTL).
pub fn slider_value_at(x: f32, left: f32, width: f32, min: f64, max: f64, step: f64, rtl: bool) -> f64 {
    let mut frac = if width > 0.0 { ((x - left) / width).clamp(0.0, 1.0) as f64 } else { 0.0 };
    if rtl {
        frac = 1.0 - frac;
    }
    snap(min + frac * (max - min), min, max, step)
}

/// `v` snapped to `min + k·step` and clamped.
pub fn snap(v: f64, min: f64, max: f64, step: f64) -> f64 {
    let v = if step > 0.0 { min + ((v - min) / step).round() * step } else { v };
    let v = v.clamp(min.min(max), max.max(min));
    (v * 1e9).round() / 1e9
}

/// What a key means along a horizontal axis: `+1` forward, `-1` back
/// (Left/Right swap in RTL); Up/Down when `vertical` too.
pub fn arrow_step(key: &str, rtl: bool, vertical: bool) -> Option<i64> {
    let (fwd, back) = if rtl { ("left", "right") } else { ("right", "left") };
    match key {
        k if k == fwd => Some(1),
        k if k == back => Some(-1),
        "down" if vertical => Some(1),
        "up" if vertical => Some(-1),
        _ => None,
    }
}

/// VAPP-162: a `hold: true` Button (the node itself, not a part).
pub(crate) fn is_hold_button(n: &eui_core::surface::PlacedNode) -> bool {
    n.component == "Button" && n.part.is_none() && n.props.get("hold").and_then(Value::as_bool) == Some(true)
}

/// A gpui key name → the hold machine's key (`" "`, `Enter`, `Escape`).
fn hold_key_name(key: &str) -> Option<&'static str> {
    match key {
        "space" | " " => Some(" "),
        "enter" => Some("Enter"),
        "escape" => Some("Escape"),
        _ => None,
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A MIME type from a file extension (what a browser would report).
pub fn mime_of(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("txt" | "md") => "text/plain",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("zip") => "application/zip",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("mp3") => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

impl SurfaceView {
    /// Forward the core's OutEvents to the host. `input` = the revision of a
    /// host-owned text edit (else a per-component counter).
    pub(crate) fn dispatch(&mut self, events: Vec<OutEvent>, input: Option<u64>, cx: &mut Context<Self>) {
        let host = self.host.clone();
        let surface_id = self.surface.id.clone();
        let mut events = events.into_iter().peekable();
        while let Some(e) = events.next() {
            match e {
                // Round 9 §6.8: the function first, its event only when it did not fail.
                OutEvent::FunctionCall { component_id, name, args } if matches!(events.peek(), Some(OutEvent::Action { .. })) => {
                    let Some(OutEvent::Action { name: action, component_id: source, event, context, payload, user_message }) = events.next() else { continue };
                    let then_host = host.clone();
                    let then = ActionEvent { surface_id: surface_id.clone(), event, name: action, component_id: source, context, payload, user_message };
                    host.on_function_call_then(&FunctionCallEvent { surface_id: surface_id.clone(), component_id, name, args }, Box::new(move |cx| then_host.on_action(&then, cx)), cx);
                }
                OutEvent::Action { name, component_id, event, context, payload, user_message } => host.on_action(&ActionEvent { surface_id: surface_id.clone(), event, name, component_id, context, payload, user_message }, cx),
                OutEvent::OpenUrl { url } => host.open_url(&url, cx),
                OutEvent::FunctionCall { component_id, name, args } => host.on_function_call(&FunctionCallEvent { surface_id: surface_id.clone(), component_id, name, args }, cx),
                OutEvent::Input { component_id, name, path, value, commit } => {
                    let revision = match input {
                        Some(r) => r,
                        None => {
                            let r = self.revisions.entry(component_id.clone()).or_insert(0);
                            *r += 1;
                            *r
                        }
                    };
                    host.on_input(&InputEvent { surface_id: surface_id.clone(), component_id, name, path, value, revision, kind: if commit { InputKind::Commit } else { InputKind::Change } }, cx)
                }
                OutEvent::Focus { id, .. } => {
                    self.keyboard = true;
                    self.pending_focus = Some(id);
                }
                OutEvent::Announce { text, live } => {
                    host.announce(&text, &live, cx);
                    if std::env::var_os("EXP_UI_TRACE").is_some() {
                        eprintln!("[exponential-ui gpui] announce ({live}): {text}");
                    }
                    self.announcement = Some((text.into(), live));
                }
                OutEvent::Copy { text } => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                OutEvent::ScrollSurface { x, y } => host.scroll_surface(x, y, cx),
                OutEvent::PickFiles { component_id, accept, multiple } => self.pick_files(component_id, accept, multiple, cx),
                // A hover overlay's close delay: ask the core again when it
                // ran out (it stays open if the pointer came back).
                OutEvent::HoverTimer { owner, delay_ms } => {
                    let key = owner.clone();
                    let task = cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(Duration::from_millis(u64::from(delay_ms))).await;
                        let _ = this.update(cx, |this, cx| {
                            this.hover_close_timers.remove(&owner);
                            let events = this.surface.hover_timeout(&owner);
                            if !events.is_empty() {
                                this.dispatch(events, None, cx);
                            }
                            cx.notify();
                        });
                    });
                    self.hover_close_timers.insert(key, task);
                }
                _ => {}
            }
        }
        cx.notify();
    }

    /// A host command (`focus`, `announce`, `scrollIntoView`,
    /// `scrollToIndex`; `a11y.json`), dispatched like an interaction's events.
    pub fn command(&mut self, command: &eui_core::surface::SurfaceCommand, cx: &mut Context<Self>) {
        let events = self.surface.command(command);
        self.dispatch(events, None, cx);
    }

    /// Round 2 §5: bring item `index` (DATA order) of the List or Table `id`
    /// into view (`align` = start | center | end | nearest, default nearest).
    pub fn scroll_to_index(&mut self, id: &str, index: u32, align: Option<&str>, cx: &mut Context<Self>) {
        self.command(&eui_core::surface::SurfaceCommand::ScrollToIndex { id: id.to_string(), index, align: align.map(str::to_string) }, cx);
    }

    /// Fire `event` on node `index` through the core and dispatch the result.
    pub fn fire(&mut self, index: u32, event: &str, payload: Option<Value>, cx: &mut Context<Self>) {
        // EXP-1324: a CodeBlock `copy` part carries the text to copy (ANSI
        // escapes stripped); the owner's `code` is the fallback.
        let copy_text = self.cache.node(index).filter(|n| n.part.as_deref() == Some("copy")).and_then(|n| n.props.get("text")).and_then(Value::as_str).map(str::to_string);
        let mut events = self.surface.event(index, event, payload);
        if let Some(t) = copy_text {
            for e in &mut events {
                if let OutEvent::Copy { text } = e {
                    *text = t.clone();
                }
            }
        }
        let copied = events.iter().any(|e| matches!(e, OutEvent::Copy { .. }));
        self.dispatch(events, None, cx);
        if copied {
            self.schedule_copy_reset(index, cx);
        }
    }

    /// A CodeBlock copy button shows `copied`, then resets.
    fn schedule_copy_reset(&mut self, index: u32, cx: &mut Context<Self>) {
        let Some(id) = self.cache.node(index).map(|n| n.id.clone()) else { return };
        let key = id.clone();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_RESET).await;
            let _ = this.update(cx, |this, cx| {
                this.copy_resets.remove(&id);
                if let Some(i) = this.cache.index_of(&id) {
                    this.fire(i, "reset", None, cx);
                }
            });
        });
        self.copy_resets.insert(key, task);
    }

    pub(crate) fn disabled(&self, index: u32) -> bool {
        let Some(n) = self.cache.node(index) else { return true };
        let off = |p: &serde_json::Map<String, Value>| matches!(p.get("disabled"), Some(Value::Bool(true)));
        off(&n.props) || NodeFlags::of(n).disabled || n.owner.as_deref().and_then(|o| self.cache.index_of(o)).and_then(|o| self.cache.node(o)).is_some_and(|o| off(&o.props) && n.part.as_deref() != Some("label"))
    }

    /// A full press of node `index` (pointer up inside, Enter / Space).
    pub fn press(&mut self, index: u32, window: &mut Window, cx: &mut Context<Self>) {
        // VAPP-208: a press reads what the person typed, even inside the debounce.
        self.flush_edits(window, cx);
        if self.disabled(index) {
            return;
        }
        let Some(n) = self.cache.node(index).cloned() else { return };
        if let Some(target) = n.trigger_for.clone() {
            if self.just_dismissed.take().as_deref() == Some(target.as_str()) {
                cx.notify();
                return;
            }
            self.layer_return.insert(target, n.id.clone());
        }
        if is_text_field(&n) || matches!((n.component.as_str(), n.part.as_deref()), ("Slider", Some("track")) | ("Segmented", _)) {
            return;
        }
        // EXP-1605: a pressable Motion's click (pointer or Enter / Space)
        // reaches its state machine first.
        if n.component == "Motion" && n.part.is_none() {
            self.motion_interact(&n.id, &eui_motion::state_machine::Interaction::Click { layer: None }, cx);
        }
        self.fire(index, "press", None, cx);
    }

    /// Pointer down on a pressable: the `pressed` state (pointer focus is
    /// not `:focus-visible`).
    pub(crate) fn press_down(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = false;
        // VAPP-208: a field's edit lands before this press is judged: gpui
        // blurs the field after this down, where the web blurs before its click.
        self.flush_edits(window, cx);
        if let Some(prev) = self.pressed.take() {
            self.set_interaction(&prev, |s| s.pressed = false);
        }
        if self.cache.index_of(id).is_some_and(|i| self.disabled(i)) {
            return;
        }
        self.pressed = Some(id.to_string());
        if self.is_motion(id) {
            self.motion_pointer(id, true, false, cx);
        }
        if self.set_interaction(id, |s| {
            s.pressed = true;
            s.focus_visible = false;
        }) {
            cx.notify();
        }
    }

    /// Pointer up inside: press it when it was the one pressed down.
    pub(crate) fn press_up(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let was = self.pressed.take();
        self.set_interaction(id, |s| s.pressed = false);
        if self.is_motion(id) {
            self.motion_pointer(id, false, false, cx);
        }
        cx.notify();
        if was.as_deref() == Some(id) {
            if let Some(index) = self.cache.index_of(id) {
                self.press(index, window, cx);
            }
        }
    }

    /// #EDE-65 lane O: the host selects Markdown text at the window level
    /// ([`crate::host::HostPlugin::markdown_window_selection`]).
    pub(crate) fn host_window_selection(&self, cx: &gpui::App) -> bool {
        self.host.markdown_window_selection(cx)
    }

    pub(crate) fn press_cancel(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.pressed.as_deref() == Some(id) {
            self.pressed = None;
        }
        if self.set_interaction(id, |s| s.pressed = false) {
            cx.notify();
        }
    }

    pub(crate) fn hover(&mut self, id: &str, hovered: bool, cx: &mut Context<Self>) {
        // #EXP-1629: a Select option's `hover` is the cursor: the pointer
        // moves it (one highlight), leaving keeps it.
        if self.is_select_item(id) {
            if hovered {
                self.move_select_cursor(id, cx);
            }
            self.host.on_hover(id, hovered, cx);
            return;
        }
        if self.set_interaction(id, |s| s.hover = hovered) {
            self.host.on_hover(id, hovered, cx);
            cx.notify();
        }
    }

    /// #EXP-1629: an option row of a Select's popup.
    pub(crate) fn is_select_item(&self, id: &str) -> bool {
        self.cache.index_of(id).and_then(|i| self.cache.node(i)).is_some_and(|n| n.owner_component.as_deref() == Some("Select") && n.part.as_deref() == Some("item"))
    }

    /// #EXP-1629: the Select's cursor moves to option `id` (its `hover`).
    pub(crate) fn move_select_cursor(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(owner) = self.cache.index_of(id).and_then(|i| self.cache.node(i)).and_then(|n| n.owner.clone()) else { return };
        let mut changed = false;
        if let Some(old) = self.select_cursor.insert(owner, id.to_string()).filter(|o| o != id) {
            changed |= self.set_interaction(&old, |s| s.hover = false);
        }
        changed |= self.set_interaction(id, |s| s.hover = true);
        if changed {
            cx.notify();
        }
    }

    /// #EXP-1629: the pointer over an option takes the keyboard focus along
    /// when it sits on another option (never out of the search field).
    pub(crate) fn select_hover_focus(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let on_option = self.focused.and_then(|i| self.cache.node(i)).is_some_and(|n| n.id != id && n.owner_component.as_deref() == Some("Select") && n.part.as_deref() == Some("item"));
        if on_option && self.is_select_item(id) {
            self.focus_id(id, window, cx);
        }
    }

    /// Does overlay `target` open on hover (Tooltip, a hover Popover)?
    pub(crate) fn opens_on_hover(&self, target: &str) -> bool {
        self.cache.index_of(target).and_then(|i| self.cache.node(i)).is_some_and(|t| t.component == "Tooltip" || (t.component == "Popover" && t.props.get("openOn").and_then(Value::as_str) == Some("hover")))
    }

    /// Hover on a trigger that opens on hover: its `hover` state (which the
    /// core opens the overlay on) follows after [`TOOLTIP_DELAY`]; leaving
    /// clears it after [`HOVER_GRACE`] unless the pointer reached the card.
    pub(crate) fn hover_trigger(&mut self, id: &str, target: &str, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.layers.iter().any(|l| l.owner == target);
        let delay = if hovered { if open { Duration::ZERO } else { TOOLTIP_DELAY } } else { HOVER_GRACE };
        let (tid, tgt) = (id.to_string(), target.to_string());
        let task = cx.spawn_in(window, async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let _ = this.update(cx, |this, cx| {
                let keep = !hovered && this.interaction.get(&format!("{tgt}.layer")).is_some_and(|s| s.hover);
                if !keep && this.set_interaction(&tid, |s| s.hover = hovered) {
                    cx.notify();
                }
            });
        });
        self.hover_timer = Some((id.to_string(), task));
    }

    /// The pointer over a hover-opened card keeps its trigger hovered.
    pub(crate) fn hover_card(&mut self, owner: &str, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.set_interaction(&format!("{owner}.layer"), |s| s.hover = hovered);
        self.states_dirty.remove(&format!("{owner}.layer"));
        if let Some(trigger) = self.cache.trigger_of(owner).and_then(|i| self.cache.node(i)).map(|n| n.id.clone()) {
            if hovered {
                self.hover_timer = None;
                if self.set_interaction(&trigger, |s| s.hover = true) {
                    cx.notify();
                }
            } else {
                self.hover_trigger(&trigger, owner, false, window, cx);
            }
        }
    }

    /// Close the layer whose root is `root` (scrim press, Escape): the core
    /// refuses a non-dismissible one.
    pub(crate) fn dismiss_layer_at(&mut self, root: u32, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(layer) = self.layers.iter().find(|l| l.root == root).cloned() else { return };
        if !layer.dismissible {
            return;
        }
        self.just_dismissed = Some(layer.owner.clone());
        self.fire(root, "dismiss", None, cx);
    }

    /// A press outside a non-modal layer: dismiss it unless the press landed
    /// in another open layer (a submenu, a nested popup).
    pub(crate) fn outside_press(&mut self, root: u32, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let (px_, py) = (f32::from(position.x - self.origin.x), f32::from(position.y - self.origin.y));
        for l in &self.layers {
            // VAPP-281: a hosted popup is in another window.
            if l.root == root || l.hosted {
                continue;
            }
            let (shift_x, shift) = self.layer_offset(l);
            if let Some(f) = self.frames.get(l.root as usize) {
                if px_ >= f.x + shift_x && px_ <= f.x + shift_x + f.w && py >= f.y + shift && py <= f.y + shift + f.h {
                    return;
                }
            }
        }
        self.dismiss_layer_at(root, window, cx);
    }

    /// Escape: close the TOP dismissible layer (an AlertDialog presses its
    /// cancel), else hide a tooltip, else dismiss a focused toast. Returns
    /// whether something handled it.
    pub fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(top) = self.layers.iter().rev().find(|l| l.class == LayerClass::Overlay && l.kind != "Tooltip").cloned() {
            if top.dismissible {
                self.just_dismissed = None;
                self.fire(top.root, "dismiss", None, cx);
            } else if let Some(cancel) = self.layer_nodes(&top).into_iter().find(|i| self.cache.node(*i).is_some_and(|n| n.id.ends_with(".cancel") && n.pressable)) {
                self.press(cancel, window, cx);
            }
            return true;
        }
        if let Some(tip) = self.layers.iter().rev().find(|l| l.kind == "Tooltip").cloned() {
            let events = self.surface.set_open(&tip.owner, false);
            self.dispatch(events, None, cx);
            return true;
        }
        if let Some(n) = self.focused.and_then(|i| self.cache.node(i)).cloned() {
            if n.owner_component.as_deref() == Some("Toast") {
                if let Some(owner) = n.owner.clone() {
                    let events = self.surface.dismiss_toast(&owner);
                    self.dispatch(events, None, cx);
                    return true;
                }
            }
        }
        false
    }

    /// Every node of a layer, in paint order.
    fn layer_nodes(&self, layer: &Layer) -> Vec<u32> {
        layer.frames.iter().map(|f| f.index).collect()
    }

    /// Focus the node with this id (a field's own handle for text fields).
    pub(crate) fn focus_id(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.focus_handles.get(id).cloned() {
            h.focus(window, cx);
        } else if let Some(f) = self.fields.get(id) {
            f.focus_handle(cx).focus(window, cx);
        } else {
            return;
        }
        let prev = self.focused;
        self.focused = self.cache.index_of(id);
        if prev != self.focused {
            if let Some(p) = prev {
                self.leave_focus(p, cx);
            }
        }
        self.track_focus(window, cx);
        cx.notify();
    }

    /// Focus left node `index`: its `focus`/`focus-visible` states clear and
    /// a chart's keyboard tooltip closes (the web clears it on blur).
    fn leave_focus(&mut self, index: u32, cx: &mut Context<Self>) {
        let Some((id, chart)) = self.cache.node(index).map(|n| (n.id.clone(), n.component == "Chart")) else { return };
        // VAPP-162: losing focus cancels a hold.
        if self.cache.node(index).is_some_and(is_hold_button) && self.surface.is_held(&id) {
            self.hold_input(index, HoldInput::Blur, cx);
        }
        self.set_interaction(&id, |s| {
            s.focus = false;
            s.focus_visible = false;
        });
        if chart && self.chart_hover.remove(&index).is_some() {
            cx.notify();
        }
    }

    /// Which focusable node holds focus now; its `focus` (and, from the
    /// keyboard, `focus-visible`) state follows.
    pub(crate) fn track_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut now = None;
        for (id, h) in &self.focus_handles {
            if h.is_focused(window) {
                now = self.cache.index_of(id);
                break;
            }
        }
        if now.is_none() {
            for (id, f) in &self.fields {
                if f.focus_handle(cx).is_focused(window) {
                    now = self.cache.index_of(id);
                    break;
                }
            }
        }
        let keyboard = self.keyboard;
        let now_id = now.and_then(|i| self.cache.node(i)).map(|n| n.id.clone());
        if now != self.focused {
            if let Some(prev) = self.focused {
                self.leave_focus(prev, cx);
            }
            self.focused = now;
        }
        if let Some(id) = now_id.clone() {
            self.set_interaction(&id, |s| {
                s.focus = true;
                s.focus_visible = keyboard;
            });
            if self.is_select_item(&id) && self.select_cursor.values().all(|c| *c != id) {
                self.move_select_cursor(&id, cx);
                if keyboard {
                    let reveal = id.clone();
                    cx.defer_in(window, move |this, _, cx| {
                        let events = this.surface.scroll_into_view(&reveal);
                        this.dispatch(events, None, cx);
                        cx.notify();
                    });
                }
            }
        }
        // Keyboard focus inside a hover-opened trigger (a Tooltip anchor, a
        // hover Popover trigger) is that trigger's `focus-visible`: it opens.
        let trigger = if keyboard { now.and_then(|i| self.hover_trigger_above(i)) } else { None };
        if trigger != self.focus_trigger {
            if let Some(old) = self.focus_trigger.take() {
                if self.cache.index_of(&old).is_some() {
                    self.set_interaction(&old, |s| s.focus_visible = false);
                }
            }
            if let Some(t) = trigger.clone() {
                if Some(&t) != now_id.as_ref() {
                    self.set_interaction(&t, |s| s.focus_visible = true);
                }
            }
            self.focus_trigger = trigger;
        }
    }

    /// The nearest node at or above `index` that opens an overlay on hover.
    fn hover_trigger_above(&self, index: u32) -> Option<String> {
        let mut cur = Some(index);
        while let Some(i) = cur {
            let n = self.cache.node(i)?;
            if n.trigger_for.as_deref().is_some_and(|t| self.opens_on_hover(t)) {
                return Some(n.id.clone());
            }
            cur = n.parent;
        }
        None
    }

    /// The Tab order in force: the top modal / menu layer's (focus trap),
    /// else the main tree's (paint order, roving stops).
    pub fn focus_order(&self) -> Vec<u32> {
        let mut order = match self.layers.iter().enumerate().rev().find(|(_, l)| l.class == LayerClass::Overlay && l.kind != "Tooltip") {
            Some((k, l)) => focus_order(&self.cache.nodes, self.layer_orders.get(k).map(Vec::as_slice).unwrap_or(&[]), l.layer),
            None => focus_order(&self.cache.nodes, &self.order, 0),
        };
        // VAPP-126: a `display: none` subtree (a SplitView's phone back bar on
        // a wide surface) is laid out at zero size and clipped: no tab stop,
        // as on the web.
        order.retain(|&i| !self.in_display_none(i));
        order
    }

    /// Whether the node or an ancestor has `display: none` in force.
    fn in_display_none(&self, index: u32) -> bool {
        let mut cur = Some(index);
        while let Some(i) = cur {
            if self.styles.get(i as usize).is_some_and(|s| s.display_none) {
                return true;
            }
            cur = self.cache.node(i).and_then(|n| n.parent);
        }
        false
    }

    /// Move focus to the next (previous) focusable node (from the keyboard).
    pub fn focus_next(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) -> Option<u32> {
        let order = self.focus_order();
        let next = next_focus(&order, self.focused, backwards)?;
        self.keyboard = true;
        let id = self.cache.node(next)?.id.clone();
        self.focus_id(&id, window, cx);
        self.focused = Some(next);
        cx.notify();
        Some(next)
    }

    /// Focus a node from the keyboard (arrows inside a widget).
    fn focus_key(&mut self, index: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = true;
        if let Some(id) = self.cache.node(index).map(|n| n.id.clone()) {
            self.focus_id(&id, window, cx);
        }
    }

    /// The siblings a roving widget moves between: same owner + part, in
    /// paint order, enabled.
    fn roving_set(&self, n: &eui_core::surface::PlacedNode) -> Vec<u32> {
        let order: Vec<u32> = if n.layer == 0 { self.order.clone() } else { self.layer_orders.iter().find(|o| o.contains(&n.index)).cloned().unwrap_or_default() };
        order
            .into_iter()
            .filter(|i| {
                self.cache.node(*i).is_some_and(|m| m.owner == n.owner && m.part == n.part && m.component == n.component && !m.hidden && is_focusable(m) && !self.disabled(*i))
            })
            .collect()
    }

    /// EXP-1280: whether a ChoiceTiles part's owner picks several values.
    fn choice_tiles_multiple(&self, n: &eui_core::surface::PlacedNode) -> bool {
        n.owner
            .as_deref()
            .and_then(|owner| self.index_of(owner))
            .and_then(|i| self.cache.node(i))
            .is_some_and(|owner| owner.props.get("type").and_then(Value::as_str) == Some("multiple"))
    }

    /// Move within a roving set by `step` (wrapping), Home/End = ends.
    fn roving_target(set: &[u32], current: u32, key: &str, step: Option<i64>) -> Option<u32> {
        if set.is_empty() {
            return None;
        }
        match key {
            "home" => return set.first().copied(),
            "end" => return set.last().copied(),
            _ => {}
        }
        let step = step?;
        let pos = set.iter().position(|i| *i == current).unwrap_or(0) as i64;
        let len = set.len() as i64;
        Some(set[((pos + step).rem_euclid(len)) as usize])
    }

    pub(crate) fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if (m.secondary(cx) || m.control) && ev.keystroke.key == "c" && self.copy_selection(cx) {
            cx.stop_propagation();
            return;
        }
        // VAPP-279: Ctrl/Cmd+Shift+ArrowLeft/ArrowRight on a strip tab
        // moves it one place (the core fires `reorder`).
        if (m.secondary(cx) || m.control) && m.shift && matches!(ev.keystroke.key.as_str(), "left" | "right") {
            if let Some(i) = self.focused.filter(|&i| self.is_strip_tab(i)) {
                let key = if ev.keystroke.key == "left" { "ArrowLeft" } else { "ArrowRight" };
                self.fire(i, "key", Some(json!({"key": key})), cx);
                cx.stop_propagation();
                return;
            }
        }
        // VAPP-162: key repeat never restarts a hold.
        if ev.is_held && matches!(ev.keystroke.key.as_str(), "space" | "enter") && self.focused_hold().is_some() {
            cx.stop_propagation();
            return;
        }
        if self.handle_key(&ev.keystroke.key, ev.keystroke.modifiers.shift, ev.keystroke.key_char.as_deref(), window, cx) {
            cx.stop_propagation();
        }
    }

    /// VAPP-162: a key released on the focused `hold` Button ends a hold
    /// the same key started.
    pub(crate) fn on_key_up(&mut self, ev: &KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        // #EXP-1629: a recording KeyRecorder takes every key up.
        if self.record_key_up(ev, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.key_up(&ev.keystroke.key, cx) {
            cx.stop_propagation();
        }
    }

    /// A key up on the focused node (`true` = a hold Button took it).
    pub fn key_up(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.focused_hold() else { return false };
        let Some(key) = hold_key_name(key) else { return false };
        self.hold_input(index, HoldInput::KeyUp { key: key.to_string() }, cx)
    }

    /// The focused node when it is an enabled `hold: true` Button.
    pub(crate) fn focused_hold(&self) -> Option<u32> {
        self.focused.filter(|i| self.cache.node(*i).is_some_and(is_hold_button))
    }

    /// VAPP-162 (round-5 contract §9): one input to the hold of Button
    /// `index` through the core (`Surface::hold`: holdStart / holdEnd /
    /// holdCancel, the `pressed` state, the `holdingLabel`), dispatched.
    /// `true` = it fired an event.
    pub fn hold_input(&mut self, index: u32, input: HoldInput, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.cache.node(index).map(|n| n.id.clone()) else { return false };
        let events = self.surface.hold(index, &input);
        let fired = events.iter().any(|e| matches!(e, OutEvent::Action { .. }));
        let held = self.surface.is_held(&id);
        match input {
            HoldInput::PointerDown { .. } if held => self.holding = Some(id),
            _ if !held => {
                if self.holding.as_deref() == Some(id.as_str()) {
                    self.holding = None;
                }
            }
            _ => {}
        }
        self.dispatch(events, None, cx);
        fired
    }

    /// A pointer input at window `position` to the held button (relative
    /// to its painted box).
    pub(crate) fn hold_pointer(&mut self, at: Point<Pixels>, up: bool, cx: &mut Context<Self>) {
        let Some(id) = self.holding.clone() else { return };
        let Some(index) = self.cache.index_of(&id) else {
            self.holding = None;
            return;
        };
        let (x, y) = self.hold_point(&id, at);
        let input = if up { HoldInput::PointerUp { x, y } } else { HoldInput::PointerMove { x, y } };
        self.hold_input(index, input, cx);
    }

    /// Window `at` relative to the painted box of hold Button `id`.
    pub(crate) fn hold_point(&self, id: &str, at: Point<Pixels>) -> (f32, f32) {
        match self.hold_bounds.borrow().get(id) {
            Some(b) => (f32::from(at.x - b.origin.x), f32::from(at.y - b.origin.y)),
            None => (0.0, 0.0),
        }
    }

    /// VAPP-162 (round-5 contract §7): the HOST decoded `value` in the
    /// QrScanner `component_id`'s feed ([`crate::host::HostPlugin::qr_scanner_feed`]):
    /// fires `scanned {value}` (announced politely) once per distinct value
    /// until it changes or the scanner remounts; a disabled scanner, an
    /// empty value or a repeat fires nothing. `true` = it fired.
    pub fn qr_scanned(&mut self, component_id: &str, value: &str, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.cache.index_of(component_id) else { return false };
        let Some(n) = self.cache.node(index) else { return false };
        if n.component != "QrScanner" || value.is_empty() || self.disabled(index) {
            return false;
        }
        // A scanner that left the tree reports afresh when it comes back.
        let gone: Vec<String> = self.qr_last.keys().filter(|k| self.cache.index_of(k).is_none()).cloned().collect();
        for k in gone {
            self.qr_last.remove(&k);
        }
        if self.qr_last.get(component_id).map(String::as_str) == Some(value) {
            return false;
        }
        self.qr_last.insert(component_id.to_string(), value.to_string());
        let mut events = self.surface.event(index, "scanned", Some(json!({"value": value})));
        events.push(OutEvent::Announce { text: value.to_string(), live: "polite".into() });
        self.dispatch(events, None, cx);
        true
    }

    /// One key on the focused node (the a11y spec); `true` = handled.
    pub fn handle_key(&mut self, key: &str, shift: bool, typed: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // VAPP-162: Space / Enter down holds a focused `hold` Button (never
        // a press); Escape cancels a hold in flight.
        if let Some(index) = self.focused_hold() {
            let held = self.cache.node(index).is_some_and(|n| self.surface.is_held(&n.id));
            match hold_key_name(key) {
                Some(k) if k != "Escape" || held => {
                    self.hold_input(index, HoldInput::KeyDown { key: k.to_string(), repeat: false }, cx);
                    return true;
                }
                _ => {}
            }
        }
        let focused = self.focused.and_then(|i| self.cache.node(i)).cloned();
        // EXP-1586: + = − 0 1 zoom a focused ZoomableImage (its view takes
        // them first when the window delivers the key).
        if let Some(n) = focused.as_ref().filter(|n| n.component == "ZoomableImage") {
            let id = n.id.clone();
            if self.zoom_key(&id, typed.filter(|t| !t.is_empty()).unwrap_or(key), window, cx) {
                return true;
            }
        }
        let in_field = focused.as_ref().is_some_and(is_text_field);
        let rtl = self.rtl;
        // EXP-1259: Tab in a search list's field picks its highlighted row.
        if key == "tab" && !shift {
            if let Some(n) = focused.as_ref().filter(|n| n.component == "SearchList") {
                let id = n.id.clone();
                self.search_key(&id, "Tab", window, cx);
                return true;
            }
        }
        match key {
            "tab" => {
                self.focus_next(shift, window, cx);
                return true;
            }
            "escape" => {
                if let Some(n) = focused.as_ref().filter(|n| n.component == "Composer" && n.props.get("busy").and_then(Value::as_bool) == Some(true)) {
                    self.fire(n.index, "stop", None, cx);
                    return true;
                }
                // A focused chart's keyboard tooltip closes first.
                if let Some(n) = focused.as_ref().filter(|n| n.component == "Chart" && self.chart_hover.contains_key(&n.index)) {
                    self.chart_hovered(n.index, None, cx);
                    return true;
                }
                return self.escape(window, cx);
            }
            "f10" if shift => {
                if let Some(n) = focused {
                    let f = self.frames.get(n.index as usize).copied().unwrap_or_default();
                    let at = Point { x: self.origin.x + gpui::px(f.x + f.w / 2.0), y: self.origin.y + gpui::px(f.y + f.h / 2.0) };
                    self.context_menu(n.index, at, cx);
                    return true;
                }
                return false;
            }
            _ => {}
        }
        let Some(n) = focused else { return false };
        let owner = n.owner_component.clone().unwrap_or_default();
        let part = n.part.clone();
        // Text fields keep their keys, except the spinbutton / chip ones.
        if in_field {
            return self.field_key(&n, key, shift, window, cx);
        }
        let step_h = arrow_step(key, rtl, false);
        let step_hv = arrow_step(key, rtl, true);
        match (n.component.as_str(), owner.as_str(), part.as_deref()) {
            ("Segmented", _, _) => return self.segmented_key(&n, key, step_hv, window, cx),
            ("Chart", _, None) if !super::state::is_sparkline(&n) => {
                let count = crate::paint::chart::point_count(&n.props);
                return match crate::paint::chart::step_point(self.chart_hover(n.index), count, step_h, key) {
                    Some(next) => {
                        self.chart_hovered(n.index, next, cx);
                        true
                    }
                    None => false,
                };
            }
            ("Slider", _, Some("track")) => {
                let (min, max, step) = (num(n.props.get("min")).unwrap_or(0.0), num(n.props.get("max")).unwrap_or(100.0), num(n.props.get("step")).unwrap_or(1.0));
                let current = self.slider_value(n.index);
                let up = match key {
                    "up" => Some(1.0),
                    "down" => Some(-1.0),
                    "pageup" => Some(10.0),
                    "pagedown" => Some(-10.0),
                    _ => step_h.map(|s| s as f64),
                };
                let value = match key {
                    "home" => min,
                    "end" => max,
                    _ => match up {
                        Some(d) => snap(current + d * step, min, max, step),
                        None => return false,
                    },
                };
                self.fire(n.index, "change", Some(json!({"value": value})), cx);
                return true;
            }
            ("Box", "Carousel", Some("indicator")) => {
                let owner_index = self.cache.owner_of(n.index);
                let count = num(n.props.get("count")).unwrap_or(0.0) as i64;
                let page = num(n.props.get("page")).unwrap_or(0.0) as i64;
                if count <= 0 {
                    return false;
                }
                let next = match key {
                    "home" => 0,
                    "end" => count - 1,
                    _ => match step_h {
                        Some(s) => (page + s).rem_euclid(count),
                        None => return false,
                    },
                };
                self.fire(owner_index, "change", Some(json!({"page": next})), cx);
                return true;
            }
            ("Text", "Tabs", Some("tab")) => {
                let set = self.roving_set(&n);
                if let Some(t) = Self::roving_target(&set, n.index, key, step_h) {
                    self.focus_key(t, window, cx);
                    self.press(t, window, cx);
                    return true;
                }
            }
            // EXP-1280 (a11y.json ChoiceTiles): arrows move the roving
            // tile, a single pick follows the focus; Space/Enter toggle.
            ("Box", "ChoiceTiles", Some("tile")) => {
                let set = self.roving_set(&n);
                if let Some(t) = Self::roving_target(&set, n.index, key, step_hv) {
                    self.focus_key(t, window, cx);
                    if !self.choice_tiles_multiple(&n) {
                        self.press(t, window, cx);
                    }
                    return true;
                }
            }
            ("Radio", "Radio", Some("dot")) => {
                let set = self.roving_set(&n);
                if let Some(t) = Self::roving_target(&set, n.index, key, step_hv) {
                    self.focus_key(t, window, cx);
                    self.press(t, window, cx);
                    return true;
                }
            }
            ("Text", "Accordion", Some("trigger")) => {
                let set = self.roving_set(&n);
                let vertical = match key {
                    "down" => Some(1),
                    "up" => Some(-1),
                    _ => None,
                };
                if let Some(t) = Self::roving_target(&set, n.index, key, vertical) {
                    self.focus_key(t, window, cx);
                    return true;
                }
            }
            ("Box", "Menu", Some("item")) => {
                let kind = n.props.get("kind").and_then(Value::as_str).unwrap_or("item");
                let (open_key, close_key) = if rtl { ("left", "right") } else { ("right", "left") };
                if key == open_key && kind == "submenu" {
                    self.press(n.index, window, cx);
                    return true;
                }
                if key == close_key && n.layer > 1 {
                    // Back to the parent menu: close this submenu.
                    if let Some(l) = self.layers.iter().find(|l| l.layer == n.layer).cloned() {
                        self.fire(l.root, "dismiss", None, cx);
                        return true;
                    }
                }
                let set = self.roving_set(&n);
                let vertical = match key {
                    "down" => Some(1),
                    "up" => Some(-1),
                    _ => None,
                };
                if let Some(t) = Self::roving_target(&set, n.index, key, vertical) {
                    self.focus_key(t, window, cx);
                    return true;
                }
                if let Some(t) = typed.filter(|t| t.chars().count() == 1 && !t.trim().is_empty()) {
                    return self.typeahead(&set, n.index, t, window, cx);
                }
            }
            ("Text", "Select" | "TimePicker", Some("item")) => {
                let set = self.roving_set(&n);
                let vertical = match key {
                    "down" => Some(1),
                    "up" => Some(-1),
                    _ => None,
                };
                if let Some(t) = Self::roving_target(&set, n.index, key, vertical) {
                    self.focus_key(t, window, cx);
                    return true;
                }
                if let Some(t) = typed.filter(|t| t.chars().count() == 1 && !t.trim().is_empty()) {
                    return self.typeahead(&set, n.index, t, window, cx);
                }
            }
            ("Text", "DatePicker" | "DateRangePicker", Some("day")) => return self.calendar_key(&n, key, shift, step_h, window, cx),
            // Round 2 §1: arrows / Home / End / Enter resize (the core's
            // `keyboardResize`, rtl-aware).
            ("Box", "Resizable", Some("handle")) => {
                let dom = match key {
                    "left" => "ArrowLeft",
                    "right" => "ArrowRight",
                    "up" => "ArrowUp",
                    "down" => "ArrowDown",
                    "home" => "Home",
                    "end" => "End",
                    "enter" => "Enter",
                    _ => return false,
                };
                self.fire(n.index, "key", Some(json!({"key": dom})), cx);
                return true;
            }
            // #VAPP-285: a Tree row (`treeitem`): the core's tree keys move
            // the focus (a `Focus` event) or fold it.
            _ if matches!(key, "up" | "down" | "home" | "end" | "left" | "right") && n.accessibility.as_ref().and_then(|a| a.get("role")).and_then(Value::as_str) == Some("treeitem") => {
                let dom = match key {
                    "up" => "ArrowUp",
                    "down" => "ArrowDown",
                    "home" => "Home",
                    "end" => "End",
                    "left" => "ArrowLeft",
                    _ => "ArrowRight",
                };
                self.fire(n.index, "key", Some(json!({"key": dom})), cx);
                return true;
            }
            // #VAPP-284: a node that folds (a foldable Row's head or fold
            // button: `expanded` set): the core's fold keys decide.
            _ if matches!(key, "left" | "right") && n.accessibility.as_ref().and_then(|a| a.get("expanded")).is_some_and(Value::is_boolean) => {
                let dom = if key == "left" { "ArrowLeft" } else { "ArrowRight" };
                self.fire(n.index, "key", Some(json!({"key": dom})), cx);
                return true;
            }
            _ => {}
        }
        // Openers: ArrowDown (Up) on a picker / menu trigger opens it.
        if matches!(key, "down" | "up") && (n.trigger_for.is_some() || matches!(part.as_deref(), Some("trigger"))) && !matches!(owner.as_str(), "Accordion" | "Tooltip") {
            let open = NodeFlags::of(&n).open;
            if !open {
                self.press(n.index, window, cx);
            }
            return true;
        }
        // A focused scroll container scrolls.
        if let Some(s) = self.scrolls.get(&n.index).copied() {
            let f = self.frames.get(n.index as usize).copied().unwrap_or_default();
            let max_y = (s.content_height - f.h).max(0.0);
            let y = match key {
                "down" => s.offset_y + 40.0,
                "up" => s.offset_y - 40.0,
                "pagedown" | "space" => s.offset_y + f.h * 0.9,
                "pageup" => s.offset_y - f.h * 0.9,
                "home" => 0.0,
                "end" => max_y,
                _ => -1.0,
            };
            if y >= -0.5 {
                if self.surface.scroll_to(&n.id, s.offset_x, y.clamp(0.0, max_y)) {
                    self.reader_scrolled(n.index, s.offset_y, y.clamp(0.0, max_y));
                    self.scrolled(cx);
                }
                return true;
            }
        }
        if matches!(key, "enter" | "space") {
            if key == "enter" && owner == "Table" && part.as_deref() == Some("row") {
                self.fire(n.index, "press", None, cx);
                return true;
            }
            self.press(n.index, window, cx);
            return true;
        }
        false
    }

    /// Keys inside a host text field: NumberField steps, ChipInput chips.
    fn field_key(&mut self, n: &eui_core::surface::PlacedNode, key: &str, shift: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let owner = n.owner_component.as_deref().unwrap_or("");
        // EXP-1259: a search list's field keeps the focus; the arrows move
        // the highlighted row (the core's), ← at the caret's start leaves a
        // searchable submenu.
        if n.component == "SearchList" {
            let caret_at_start = self.fields.get(&n.id).is_some_and(|f| match &f.input {
                super::input::FieldInput::Line(s) => s.read(cx).cursor() == 0,
                super::input::FieldInput::Multi(_) => false,
            });
            let dom = match key {
                "down" => "ArrowDown",
                "up" => "ArrowUp",
                "left" if owner == "Menu" && caret_at_start && !self.rtl => "ArrowLeft",
                "right" if owner == "Menu" && caret_at_start && self.rtl => "ArrowLeft",
                _ => return false,
            };
            let id = n.id.clone();
            self.search_key(&id, dom, window, cx);
            return true;
        }
        match (owner, key) {
            ("NumberField", "up" | "down" | "pageup" | "pagedown") => {
                let Some(owner_index) = n.owner.as_deref().and_then(|o| self.cache.index_of(o)) else { return false };
                let times = match key {
                    "pageup" | "pagedown" => 10,
                    _ if shift => 10,
                    _ => 1,
                };
                let part = if matches!(key, "up" | "pageup") { "increment" } else { "decrement" };
                let Some(btn) = self.cache.index_of(&format!("{}.{part}", self.cache.nodes[owner_index as usize].id)) else { return false };
                self.flush_field(&n.id, None, window, cx);
                for _ in 0..times {
                    self.fire(btn, "press", None, cx);
                }
                true
            }
            ("ChipInput", "backspace") => {
                let empty = self.fields.get(&n.id).is_none_or(|f| f.value(cx).is_empty());
                if !empty {
                    return false;
                }
                let owner_id = n.owner.clone().unwrap_or_default();
                let last = self.cache.nodes.iter().filter(|m| !m.removed && m.owner.as_deref() == Some(owner_id.as_str()) && m.part.as_deref() == Some("remove")).map(|m| m.index).next_back();
                if let Some(r) = last {
                    self.fire(r, "press", None, cx);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// EXP-1259: a key for a search list's field (`ArrowDown`, `ArrowUp`,
    /// `Enter`, `Tab`, `ArrowLeft` = leave a searchable submenu): the
    /// outstanding text is flushed first (the filter is current), then the
    /// core moves or picks the highlighted row. Public for automation and
    /// tests (it never focuses the field).
    pub fn search_key(&mut self, id: &str, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_field(id, None, window, cx);
        if let Some(i) = self.cache.index_of(id) {
            self.fire(i, "key", Some(json!({"key": key})), cx);
        }
    }

    /// Type-ahead in a listbox / menu: the next entry starting with `typed`.
    fn typeahead(&mut self, set: &[u32], current: u32, typed: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let lower = typed.to_lowercase();
        let pos = set.iter().position(|i| *i == current).unwrap_or(0);
        let text_of = |i: u32| -> String {
            let Some(n) = self.cache.node(i) else { return String::new() };
            let text = |n: &eui_core::surface::PlacedNode| Some(crate::measure::display_text(n.props.get("text"))).filter(|t| !t.is_empty());
            text(n).or_else(|| n.children.iter().filter_map(|c| self.cache.node(*c)).find_map(text)).unwrap_or_default()
        };
        for k in 1..=set.len() {
            let i = set[(pos + k) % set.len()];
            if text_of(i).to_lowercase().starts_with(&lower) {
                self.focus_key(i, window, cx);
                return true;
            }
        }
        false
    }

    /// Calendar grid keys: ±1 day / ±1 week, PageUp/PageDown month,
    /// Home/End the row (the locale's week).
    fn calendar_key(&mut self, n: &eui_core::surface::PlacedNode, key: &str, shift: bool, step_h: Option<i64>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let date = n.props.get("date").and_then(Value::as_str).and_then(crate::paint::date::parse_iso);
        let Some((y, m, d)) = date else { return false };
        let z = crate::paint::date::days_from_civil(y, m, d);
        let owner = n.owner.clone().unwrap_or_default();
        let target = match key {
            "up" => Some(z - 7),
            "down" => Some(z + 7),
            "home" | "end" => {
                // The row's first / last cell.
                let row = n.parent.and_then(|p| self.cache.node(p)).map(|r| r.children.clone()).unwrap_or_default();
                let pick = if key == "home" { row.first() } else { row.last() };
                if let Some(i) = pick.copied() {
                    self.focus_key(i, window, cx);
                    return true;
                }
                None
            }
            "pageup" | "pagedown" => {
                let months = if shift { 12 } else { 1 } * if key == "pageup" { -1 } else { 1 };
                let (ny, nm) = crate::paint::date::add_months(y, m, months);
                let nd = d.min(crate::paint::date::days_in_month(ny, nm));
                Some(crate::paint::date::days_from_civil(ny, nm, nd))
            }
            _ => step_h.map(|s| z + s),
        };
        let Some(t) = target else { return false };
        let (ty, tm, td) = crate::paint::date::civil_from_days(t);
        let iso = crate::paint::date::iso(ty, tm, td);
        if let Some(i) = self.day_node(&owner, &iso) {
            self.focus_key(i, window, cx);
            return true;
        }
        // Another month: page the calendar, focus after the relayout.
        let forward = t > z;
        if let Some(btn) = self.cache.index_of(&format!("{owner}.{}", if forward { "next" } else { "previous" })) {
            let months = ((ty - y) * 12 + tm as i32 - m as i32).abs().max(1);
            for _ in 0..months {
                self.fire(btn, "press", None, cx);
            }
            self.pending_day = Some((owner, iso));
            return true;
        }
        false
    }

    /// The day cell of calendar `owner` showing `iso`.
    pub(crate) fn day_node(&self, owner: &str, iso: &str) -> Option<u32> {
        self.cache.nodes.iter().find(|x| !x.removed && x.owner.as_deref() == Some(owner) && x.part.as_deref() == Some("day") && x.props.get("date").and_then(Value::as_str) == Some(iso) && x.props.get("outside").and_then(Value::as_bool) != Some(true)).map(|x| x.index)
    }

    /// Segmented keys: arrows move the roving item, Space/Enter toggle it.
    fn segmented_key(&mut self, n: &eui_core::surface::PlacedNode, key: &str, step: Option<i64>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let items = n.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        if items.is_empty() {
            return false;
        }
        let len = items.len() as i64;
        let current = self.group_index(n) as i64;
        let next = match key {
            "home" => 0,
            "end" => len - 1,
            "enter" | "space" => {
                if let Some(v) = items.get(current as usize).and_then(|i| i.get("value")).cloned() {
                    self.segmented_select(n.index, v, window, cx);
                }
                return true;
            }
            _ => match step {
                Some(s) => (current + s).rem_euclid(len),
                None => return false,
            },
        };
        self.group_focus.insert(n.id.clone(), next as usize);
        cx.notify();
        true
    }

    /// A Segmented's roving item: the one the arrows moved to, else the
    /// first selected, else the first.
    pub(crate) fn group_index(&self, n: &eui_core::surface::PlacedNode) -> usize {
        if let Some(i) = self.group_focus.get(&n.id) {
            return *i;
        }
        let items = n.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let chosen: Vec<String> = match n.props.get("value") {
            Some(Value::Array(a)) => a.iter().map(|v| crate::measure::display_text(Some(v))).collect(),
            Some(Value::String(s)) => s.split(',').map(str::to_string).collect(),
            Some(v) => vec![crate::measure::display_text(Some(v))],
            None => Vec::new(),
        };
        items.iter().position(|i| chosen.contains(&crate::measure::display_text(i.get("value")))).unwrap_or(0)
    }

    /// Open-layer bookkeeping after a pass: enter animations, focus into a
    /// newly opened layer (the selected option / day, else its first
    /// focusable; never a toast or tooltip), focus back to the trigger when
    /// one closes.
    pub(crate) fn layers_changed(&mut self, layers: &[Layer], now: Instant, window: &mut Window, cx: &mut Context<Self>) {
        let reduced = self.surface.settings().reduced_motion;
        // EXP-1243: enters take `motion.standard` (exits the faster `fast`).
        let enter_ms = self
            .surface
            .theme()
            .and_then(|t| t.tokens.motion.get("standard").copied())
            .unwrap_or(super::motion::ENTER_FALLBACK_MS) as f32;
        for l in layers {
            if !self.layers.iter().any(|o| o.owner == l.owner && o.layer == l.layer) && !reduced {
                self.motion.layer_opened(&l.owner, enter_ms, now);
            }
        }
        let interactive = |l: &&Layer| l.class == LayerClass::Overlay && l.kind != "Tooltip";
        let now_open: Vec<String> = layers.iter().filter(interactive).map(|l| format!("{}#{}", l.owner, l.layer)).collect();
        if now_open == self.open_layers {
            if let Some((owner, iso)) = self.pending_day.take() {
                if let Some(i) = self.day_node(&owner, &iso) {
                    self.pending_focus = self.cache.node(i).map(|n| n.id.clone());
                }
            }
            return;
        }
        let opened: Vec<&Layer> = layers.iter().filter(interactive).filter(|l| !self.open_layers.contains(&format!("{}#{}", l.owner, l.layer))).collect();
        let closed: Vec<String> = self.open_layers.iter().filter(|o| !now_open.contains(o)).map(|o| o.split('#').next().unwrap_or_default().to_string()).collect();
        let mut target = None;
        if let Some(top) = opened.last() {
            if let Some(trigger) = self.cache.trigger_of(&top.owner).and_then(|i| self.cache.node(i)).map(|n| n.id.clone()) {
                self.layer_return.entry(top.owner.clone()).or_insert(trigger);
            }
            let k = layers.iter().position(|l| l.owner == top.owner && l.layer == top.layer).unwrap_or(0);
            let order: Vec<u32> = top.frames.iter().map(|f| f.index).collect();
            let candidates = focus_order(&self.cache.nodes, &order, top.layer);
            let selected = candidates.iter().copied().find(|i| self.cache.node(*i).is_some_and(|n| NodeFlags::of(n).selected && !is_text_field(n)));
            let first = candidates.iter().copied().find(|i| self.cache.node(*i).is_some_and(|n| !is_text_field(n)));
            // EXP-1259: a searchable submenu opens with its field focused.
            let search = candidates.iter().copied().find(|i| self.cache.node(*i).is_some_and(|n| n.component == "SearchList" && n.part.as_deref() == Some("search")));
            target = search.or(selected).or(first).and_then(|i| self.cache.node(i)).map(|n| n.id.clone());
            // #EXP-1629: a Select opens as the IDE's picker: a fresh pick,
            // the cursor on the FIRST enabled option, the search field (if
            // any) focused, else that option.
            if top.kind == "Select" {
                let item = |i: &u32| self.cache.node(*i).is_some_and(|n| n.part.as_deref() == Some("item") && !NodeFlags::of(n).disabled);
                let first_item = candidates.iter().copied().find(item).and_then(|i| self.cache.node(i)).map(|n| n.id.clone());
                let field = candidates.iter().copied().find(|i| self.cache.node(*i).is_some_and(|n| n.part.as_deref() == Some("search"))).and_then(|i| self.cache.node(i)).map(|n| n.id.clone());
                if let Some(first) = &first_item {
                    self.move_select_cursor(first, cx);
                }
                target = field.or(first_item);
            }
            let _ = k;
        } else if let Some(o) = closed.last() {
            let inside = self.focused.and_then(|i| self.cache.node(i)).is_none_or(|n| n.layer > 0) || self.focused.is_none();
            if inside {
                target = self.layer_return.get(o).cloned();
            }
        }
        for o in &closed {
            if let Some(c) = self.select_cursor.remove(o) {
                self.set_interaction(&c, |s| s.hover = false);
            }
        }
        self.open_layers = now_open;
        if let Some(id) = target {
            self.pending_focus = Some(id.clone());
            cx.defer_in(window, move |this, window, cx| this.focus_id(&id, window, cx));
        }
    }

    // -- Toasts -------------------------------------------------------------

    /// Start a timer for every new timed toast; forget the gone ones.
    pub(crate) fn sync_toasts(&mut self, toasts: &[ToastSpec], window: &mut Window, cx: &mut Context<Self>) {
        self.toast_timers.retain(|id, _| toasts.iter().any(|t| t.id == *id));
        for t in toasts {
            if t.duration_ms <= 0.0 || self.toast_timers.contains_key(&t.id) {
                continue;
            }
            self.toast_timers.insert(t.id.clone(), ToastTimer { remaining_ms: t.duration_ms, started: None, task: None });
            if !self.toast_hover.contains(&t.id) {
                self.toast_resume(&t.id.clone(), window, cx);
            }
        }
    }

    fn toast_resume(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(timer) = self.toast_timers.get_mut(id) else { return };
        let remaining = Duration::from_secs_f64((timer.remaining_ms / 1000.0).max(0.0));
        let tid = id.to_string();
        timer.started = Some(Instant::now());
        timer.task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(remaining).await;
            let _ = this.update(cx, |this, cx| this.toast_expired(&tid, cx));
        }));
    }

    fn toast_expired(&mut self, id: &str, cx: &mut Context<Self>) {
        self.toast_timers.remove(id);
        let events = self.surface.dismiss_toast(id);
        self.dispatch(events, None, cx);
    }

    /// Hover (or focus) pauses a toast's timer.
    pub(crate) fn toast_hovered(&mut self, id: &str, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        if hovered {
            self.toast_hover.insert(id.to_string());
            if let Some(t) = self.toast_timers.get_mut(id) {
                if let Some(start) = t.started.take() {
                    t.remaining_ms = (t.remaining_ms - start.elapsed().as_secs_f64() * 1000.0).max(0.0);
                }
                t.task = None;
            }
        } else {
            self.toast_hover.remove(id);
            if self.toast_timers.get(id).is_some_and(|t| t.task.is_none()) {
                self.toast_resume(id, window, cx);
            }
        }
    }

    // -- Pointer: context menu, files, sliders, scrolling -----------------

    /// A right click (or Shift+F10) inside a context Menu: open it at the
    /// pointer (surface coordinates).
    pub(crate) fn context_menu(&mut self, index: u32, position: Point<Pixels>, cx: &mut Context<Self>) {
        let x = f32::from(position.x - self.origin.x);
        let y = f32::from(position.y - self.origin.y);
        self.fire(index, "contextmenu", Some(json!({"x": x, "y": y})), cx);
    }

    fn pick_files(&mut self, component_id: String, accept: Option<String>, multiple: bool, cx: &mut Context<Self>) {
        let req = FilePickRequest { surface_id: self.surface.id.clone(), component_id: component_id.clone(), accept, multiple };
        if self.host.pick_files(&req, cx) {
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple, prompt: None });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |this, cx| this.files_picked(&component_id, paths, cx));
            }
        })
        .detach();
    }

    /// Files picked for (or dropped on) FileUpload `component_id`: the host
    /// gets the paths, the surface `upload {files}` (name, size, type).
    pub fn files_picked(&mut self, component_id: &str, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(zone) = self.cache.index_of(&format!("{component_id}.dropzone")).or_else(|| self.cache.index_of(component_id)) else { return };
        let name = self.cache.index_of(component_id).and_then(|i| self.cache.node(i)).and_then(|n| n.props.get("name")).and_then(Value::as_str).unwrap_or("").to_string();
        let files: Vec<Value> = paths
            .iter()
            .map(|p| {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                json!({"name": p.file_name().and_then(|n| n.to_str()).unwrap_or(""), "size": size, "type": mime_of(p)})
            })
            .collect();
        self.host.on_upload(&UploadEvent { surface_id: self.surface.id.clone(), component_id: component_id.to_string(), name, paths }, cx);
        self.fire(zone, "upload", Some(json!({"files": files})), cx);
    }

    pub(crate) fn files_dropped(&mut self, dropzone_id: &str, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let owner = self.cache.index_of(dropzone_id).and_then(|i| self.cache.node(i)).and_then(|n| n.owner.clone()).unwrap_or_else(|| dropzone_id.to_string());
        self.files_picked(&owner, paths, cx);
    }

    pub(crate) fn drag_start(&mut self, track_id: &str, x: f32, cx: &mut Context<Self>) {
        let Some(index) = self.cache.index_of(track_id) else { return };
        if self.disabled(index) {
            return;
        }
        let Some(bounds) = self.slider_bounds.borrow().get(track_id).copied() else { return };
        let n = &self.cache.nodes[index as usize];
        let (min, max, step) = (num(n.props.get("min")).unwrap_or(0.0), num(n.props.get("max")).unwrap_or(100.0), num(n.props.get("step")).unwrap_or(1.0));
        let value = slider_value_at(x, f32::from(bounds.origin.x), f32::from(bounds.size.width), min, max, step, self.rtl);
        self.drag = Some(Drag { track: track_id.to_string(), value });
        cx.notify();
    }

    /// A press on a Drawer's handle or sheet: a drag toward the sheet's edge
    /// (past [`SHEET_DISMISS_PX`]) dismisses it when it allows.
    pub(crate) fn sheet_drag_start(&mut self, root: u32, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(layer) = self.layers.iter().find(|l| l.root == root) else { return };
        let owner_drag = self.cache.index_of(&layer.owner).and_then(|o| self.cache.node(o)).and_then(|o| o.props.get("dragToDismiss")).and_then(Value::as_bool) != Some(false);
        if !layer.dismissible || !owner_drag {
            return;
        }
        self.sheet_drag = Some(super::SheetDrag { root, side: layer.position.clone(), start: (f32::from(at.x), f32::from(at.y)), offset: (0.0, 0.0) });
        cx.notify();
    }

    /// A press on a Resizable handle's hit area: the drag starts (the core
    /// keeps the sizes it starts from).
    pub(crate) fn resize_start(&mut self, handle_id: &str, vertical: bool, pointer: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.cache.index_of(handle_id) else { return };
        self.focus_id(handle_id, window, cx);
        self.resize_drag = Some(super::ResizeDrag { handle: handle_id.to_string(), vertical, start: pointer });
        self.fire(index, "drag", Some(json!({"phase": "start", "delta": 0})), cx);
        cx.notify();
    }

    /// VAPP-279: is this a `strip` Tabs' tab (it reorders by drag and by
    /// Ctrl/Cmd+Shift+Arrow)?
    pub(crate) fn is_strip_tab(&self, index: u32) -> bool {
        let Some(n) = self.cache.node(index) else { return false };
        if n.owner_component.as_deref() != Some("Tabs") || n.part.as_deref() != Some("tab") {
            return false;
        }
        let owner = self.cache.owner_of(index);
        self.cache.node(owner).and_then(|o| o.props.get("variant")).and_then(Value::as_str) == Some("strip")
    }

    /// VAPP-279: a press on a strip tab: it becomes a reorder drag once
    /// the pointer travels `tabDragThreshold` (then the press is spent).
    pub(crate) fn tab_drag_down(&mut self, tab_id: &str, x: f32) {
        self.tab_drag = Some(super::TabDrag { tab: tab_id.to_string(), start: x, started: false });
    }

    pub(crate) fn on_pointer_move(&mut self, ev: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if ev.pressed_button != Some(gpui::MouseButton::Left) {
            return;
        }
        if self.md_dragging {
            if let Some(node) = self.md_selection.map(|s| s.node) {
                self.md_drag(node, ev.position, cx);
            }
            return;
        }
        if let Some(d) = self.resize_drag.clone() {
            let p = if d.vertical { f32::from(ev.position.y) } else { f32::from(ev.position.x) };
            if let Some(i) = self.cache.index_of(&d.handle) {
                self.fire(i, "drag", Some(json!({"phase": "move", "delta": p - d.start})), cx);
            }
            return;
        }
        if let Some(d) = self.tab_drag.clone() {
            let delta = f32::from(ev.position.x) - d.start;
            let Some(i) = self.cache.index_of(&d.tab) else { return };
            if !d.started {
                if delta.abs() < eui_core::layout::TAB_DRAG_THRESHOLD {
                    return;
                }
                // The press is spent: the release drops, never presses.
                self.press_cancel(&d.tab, cx);
                self.pressed = None;
                self.tab_drag = Some(super::TabDrag { started: true, ..d.clone() });
                self.fire(i, "drag", Some(json!({"phase": "start", "delta": 0})), cx);
            }
            self.fire(i, "drag", Some(json!({"phase": "move", "delta": delta})), cx);
            return;
        }
        if let Some(d) = self.sheet_drag.as_mut() {
            let (x, y) = (f32::from(ev.position.x) - d.start.0, f32::from(ev.position.y) - d.start.1);
            // Only toward the edge the sheet hangs from.
            d.offset = sheet_offset(&d.side, x, y);
            cx.notify();
            return;
        }
        let Some(drag) = self.drag.clone() else { return };
        let Some(index) = self.cache.index_of(&drag.track) else { return };
        let Some(bounds) = self.slider_bounds.borrow().get(&drag.track).copied() else { return };
        let n = &self.cache.nodes[index as usize];
        let (min, max, step) = (num(n.props.get("min")).unwrap_or(0.0), num(n.props.get("max")).unwrap_or(100.0), num(n.props.get("step")).unwrap_or(1.0));
        let value = slider_value_at(f32::from(ev.position.x), f32::from(bounds.origin.x), f32::from(bounds.size.width), min, max, step, self.rtl);
        if (value - drag.value).abs() > f64::EPSILON {
            self.drag = Some(Drag { value, ..drag });
            cx.notify();
        }
    }

    pub(crate) fn on_pointer_up(&mut self, ev: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // A dismissal remembered for the trigger under this click is spent.
        self.just_dismissed = None;
        if let Some(d) = self.resize_drag.take() {
            let p = if d.vertical { f32::from(ev.position.y) } else { f32::from(ev.position.x) };
            if let Some(i) = self.cache.index_of(&d.handle) {
                self.fire(i, "drag", Some(json!({"phase": "end", "delta": p - d.start})), cx);
            }
            cx.notify();
            return;
        }
        if let Some(d) = self.tab_drag.take() {
            if d.started {
                if let Some(i) = self.cache.index_of(&d.tab) {
                    self.fire(i, "drag", Some(json!({"phase": "end", "delta": f32::from(ev.position.x) - d.start})), cx);
                }
                cx.notify();
                return;
            }
        }
        self.md_dragging = false;
        if let Some(d) = self.sheet_drag.take() {
            if d.offset.0.abs().max(d.offset.1.abs()) >= SHEET_DISMISS_PX {
                self.fire(d.root, "dismiss", None, cx);
            }
            cx.notify();
        }
        let Some(drag) = self.drag.take() else { return };
        let Some(index) = self.cache.index_of(&drag.track) else { return };
        self.fire(index, "change", Some(json!({"value": drag.value})), cx);
    }

    /// The value a slider track shows (an in-flight drag, else the prop).
    pub(crate) fn slider_value(&self, index: u32) -> f64 {
        let Some(n) = self.cache.node(index) else { return 0.0 };
        if let Some(d) = self.drag.as_ref().filter(|d| d.track == n.id) {
            return d.value;
        }
        num(n.props.get("value")).or_else(|| num(n.props.get("min"))).unwrap_or(0.0)
    }

    /// The wheel / trackpad over a scroll container: the core's offset
    /// moves; at an edge the event chains to the host.
    pub(crate) fn wheel(&mut self, index: u32, id: &str, ev: &ScrollEvent, cx: &mut Context<Self>) {
        let Some(s) = self.scrolls.get(&index).copied() else { return };
        let f = self.frames.get(index as usize).copied().unwrap_or_default();
        let d = ev.delta.pixel_delta(gpui::px(20.0));
        let (dx, dy) = (f32::from(d.x), f32::from(d.y));
        let max_x = (s.content_width - f.w).max(0.0);
        let max_y = (s.content_height - f.h).max(0.0);
        let x = if s.scroll_x { (s.offset_x - dx).clamp(0.0, max_x) } else { s.offset_x };
        let y = if s.scroll_y { (s.offset_y - dy).clamp(0.0, max_y) } else { s.offset_y };
        if (x - s.offset_x).abs() < 0.01 && (y - s.offset_y).abs() < 0.01 {
            return;
        }
        cx.stop_propagation();
        if self.surface.scroll_to(id, x, y) {
            self.scrolls.insert(index, eui_core::surface::ScrollOutput { offset_x: x, offset_y: y, ..s });
            self.reader_scrolled(index, s.offset_y, y);
            self.scrolled(cx);
        }
    }

    /// A container scrolled: repaint, and take what the core raised for it
    /// (#EXP-1739: an overlay anchored inside closes, its `change` fires).
    fn scrolled(&mut self, cx: &mut Context<Self>) {
        let late = self.surface.take_events();
        if !late.is_empty() {
            self.dispatch(late, None, cx);
        }
        cx.notify();
    }

    /// #EXP-1629: a thumb drag's offset (`view/scrollbar.rs`) goes to the
    /// core; returns whether one moved.
    pub(crate) fn take_scrollbar_drags(&mut self) -> bool {
        let pending: Vec<(String, (u32, f32, f32))> = self.scrollbars.borrow().iter().filter_map(|(id, h)| Some((id.clone(), h.take_pending()?))).collect();
        let mut moved = false;
        for (id, (index, x, y)) in pending {
            let Some(s) = self.scrolls.get(&index).copied() else { continue };
            let f = self.frames.get(index as usize).copied().unwrap_or_default();
            let x = if s.scroll_x { x.clamp(0.0, (s.content_width - f.w).max(0.0)) } else { s.offset_x };
            let y = if s.scroll_y { y.clamp(0.0, (s.content_height - f.h).max(0.0)) } else { s.offset_y };
            if self.surface.scroll_to(&id, x, y) {
                self.scrolls.insert(index, eui_core::surface::ScrollOutput { offset_x: x, offset_y: y, ..s });
                self.reader_scrolled(index, s.offset_y, y);
                moved = true;
            }
        }
        moved
    }

    // -- Segmented, charts --------------------------------------------------

    /// A Segmented item press: single = that value, multiple = toggled set.
    pub(crate) fn segmented_select(&mut self, index: u32, value: Value, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(n) = self.cache.node(index).cloned() else { return };
        let multiple = n.props.get("type").and_then(Value::as_str) == Some("multiple");
        let current = n.props.get("value").cloned().unwrap_or(Value::Null);
        let next = if multiple {
            let mut set: Vec<Value> = match &current {
                Value::Array(a) => a.clone(),
                Value::String(s) if !s.is_empty() => s.split(',').map(|v| Value::String(v.to_string())).collect(),
                _ => Vec::new(),
            };
            if let Some(p) = set.iter().position(|v| *v == value) {
                set.remove(p);
            } else {
                set.push(value);
            }
            Value::Array(set)
        } else {
            value
        };
        if let Some(items) = n.props.get("items").and_then(Value::as_array) {
            if let Some(k) = items.iter().position(|i| i.get("value") == Some(&next) || Some(&next).is_some_and(|v| v.is_array())) {
                self.group_focus.insert(n.id.clone(), k);
            }
        }
        self.fire(index, "change", Some(json!({"value": next})), cx);
    }

    // -- Markdown selection --------------------------------------------------

    /// The `(unit, byte)` under a window position in Markdown leaf `node`
    /// (the nearest unit by its vertical extent).
    fn md_hit(&self, node: u32, at: Point<gpui::Pixels>) -> Option<(usize, usize)> {
        let units = self.md_units.borrow();
        let list = units.get(&node)?;
        let mut best: Option<(f32, usize)> = None;
        for (u, unit) in list.iter().enumerate() {
            // #VAPP-258 step 5: an off-screen block of a long document was
            // never laid out this frame.
            if !unit.live.get() {
                continue;
            }
            let b = unit.layout.bounds();
            let d = if at.y < b.top() { f32::from(b.top() - at.y) } else if at.y > b.bottom() { f32::from(at.y - b.bottom()) } else { 0.0 };
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, u));
            }
        }
        let (_, u) = best?;
        let (layout, text) = (&list[u].layout, &list[u].text);
        let b = layout.bounds();
        let ix = if at.y > b.bottom() {
            text.len()
        } else {
            match layout.index_for_position(at) {
                Ok(i) | Err(i) => i.min(text.len()),
            }
        };
        Some((u, ix))
    }

    pub(crate) fn md_press(&mut self, node: u32, at: Point<gpui::Pixels>, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hit) = self.md_hit(node, at) else { return };
        let anchor = match self.md_selection.filter(|s| extend && s.node == node) {
            Some(s) => s.anchor,
            None => hit,
        };
        self.md_selection = Some(crate::paint::markdown::MdSelection { node, anchor, head: hit });
        self.md_selection_key = self.md_text_key(node);
        self.md_dragging = true;
        // Keys (copy) reach the surface while text is selected.
        window.focus(&self.root_focus, cx);
        cx.notify();
    }

    pub(crate) fn md_drag(&mut self, node: u32, at: Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if !self.md_dragging {
            return;
        }
        let Some(hit) = self.md_hit(node, at) else { return };
        if let Some(sel) = self.md_selection.as_mut().filter(|s| s.node == node && s.head != hit) {
            sel.head = hit;
            cx.notify();
        }
    }

    /// #VAPP-258 step 5 (tests): a Markdown leaf's (live, all) text units.
    #[cfg(test)]
    pub(crate) fn md_unit_counts(&self, id: &str) -> (usize, usize) {
        let Some(node) = self.cache.index_of(id) else { return (0, 0) };
        self.md_units.borrow().get(&node).map_or((0, 0), |u| (u.iter().filter(|u| u.live.get()).count(), u.len()))
    }

    /// The selected Markdown text (empty = none).
    pub fn selected_text(&self) -> String {
        let Some(sel) = self.md_selection.filter(|s| !s.is_empty()) else { return String::new() };
        self.md_units.borrow().get(&sel.node).map(|u| sel.text(u)).unwrap_or_default()
    }

    /// Select a Markdown leaf's whole text (automation; the platform's
    /// select-all inside it).
    pub fn select_all_markdown(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(node) = self.cache.index_of(id) else { return false };
        let end = self.md_units.borrow().get(&node).and_then(|u| u.last().map(|unit| (u.len() - 1, unit.text.len())));
        let Some(end) = end else { return false };
        self.md_selection = Some(crate::paint::markdown::MdSelection { node, anchor: (0, 0), head: end });
        self.md_selection_key = self.md_text_key(node);
        cx.notify();
        true
    }

    /// What a Markdown selection is valid for: the leaf's id and a hash of
    /// its text (`None` = no such Markdown leaf).
    pub(crate) fn md_text_key(&self, node: u32) -> Option<(String, u64)> {
        use std::hash::{Hash, Hasher};
        let n = self.cache.node(node).filter(|n| n.component == "Markdown")?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        n.props.get("text").and_then(Value::as_str).unwrap_or("").hash(&mut h);
        Some((n.id.clone(), h.finish()))
    }

    /// Copy the Markdown selection to the clipboard; `false` = nothing
    /// selected.
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.selected_text();
        if text.is_empty() {
            return false;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        true
    }

    /// The category / slice a Chart's tooltip shows (pointer or keyboard).
    pub fn chart_point(&self, id: &str) -> Option<usize> {
        self.cache.index_of(id).and_then(|i| self.chart_hover(i))
    }

    pub(crate) fn chart_hover(&self, index: u32) -> Option<usize> {
        self.chart_hover.get(&index).copied()
    }

    pub(crate) fn chart_hovered(&mut self, index: u32, hovered: Option<usize>, cx: &mut Context<Self>) {
        let before = self.chart_hover.get(&index).copied();
        if before != hovered {
            match hovered {
                Some(h) => self.chart_hover.insert(index, h),
                None => self.chart_hover.remove(&index),
            };
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_values_snap_and_clamp() {
        assert_eq!(slider_value_at(50.0, 0.0, 100.0, 0.0, 100.0, 5.0, false), 50.0);
        assert_eq!(slider_value_at(52.0, 0.0, 100.0, 0.0, 100.0, 5.0, false), 50.0);
        assert_eq!(slider_value_at(53.0, 0.0, 100.0, 0.0, 100.0, 5.0, false), 55.0);
        assert_eq!(slider_value_at(-10.0, 0.0, 100.0, 0.0, 100.0, 5.0, false), 0.0);
        assert_eq!(slider_value_at(500.0, 0.0, 100.0, 0.0, 100.0, 5.0, false), 100.0);
        assert_eq!(slider_value_at(10.0, 0.0, 0.0, 0.0, 100.0, 5.0, false), 0.0);
        assert_eq!(slider_value_at(20.0, 0.0, 100.0, 0.0, 100.0, 5.0, true), 80.0, "RTL: the minimum is on the right");
        assert_eq!(snap(0.30000000004, 0.0, 1.0, 0.1), 0.3);
    }

    #[test]
    fn arrows_mirror_in_rtl() {
        assert_eq!(arrow_step("right", false, false), Some(1));
        assert_eq!(arrow_step("right", true, false), Some(-1));
        assert_eq!(arrow_step("left", true, false), Some(1));
        assert_eq!(arrow_step("down", false, false), None);
        assert_eq!(arrow_step("down", true, true), Some(1));
        assert_eq!(arrow_step("up", false, true), Some(-1));
    }

    #[test]
    fn roving_targets_wrap_and_jump() {
        let set = [4, 7, 9];
        assert_eq!(SurfaceView::roving_target(&set, 9, "right", Some(1)), Some(4));
        assert_eq!(SurfaceView::roving_target(&set, 4, "left", Some(-1)), Some(9));
        assert_eq!(SurfaceView::roving_target(&set, 7, "home", None), Some(4));
        assert_eq!(SurfaceView::roving_target(&set, 7, "end", None), Some(9));
        assert_eq!(SurfaceView::roving_target(&set, 7, "x", None), None);
    }

    #[test]
    fn sheets_drag_toward_their_edge_only() {
        assert_eq!(sheet_offset("bottom", 5.0, 80.0), (0.0, 80.0));
        assert_eq!(sheet_offset("bottom", 0.0, -30.0), (0.0, 0.0));
        assert_eq!(sheet_offset("right", 70.0, 3.0), (70.0, 0.0));
        assert_eq!(sheet_offset("left", 70.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn dropped_files_report_a_mime_type() {
        assert_eq!(mime_of(std::path::Path::new("a/shot.PNG")), "image/png");
        assert_eq!(mime_of(std::path::Path::new("notes.md")), "text/plain");
        assert_eq!(mime_of(std::path::Path::new("blob")), "application/octet-stream");
    }
}
