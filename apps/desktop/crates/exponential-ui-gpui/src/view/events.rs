//! Interaction: presses (pointer and keyboard), the OutEvent dispatch to the
//! host, keyboard focus in tree order with the layer focus trap, Escape,
//! overlay dismissal, the Tooltip hover timer and Slider drags.

use std::time::Duration;

use exponential_ui::surface::{Layer, OutEvent};
use gpui::{Context, KeyDownEvent, MouseMoveEvent, MouseUpEvent, Window};
use serde_json::{json, Value};

use super::state::{focus_order, is_text_field, next_focus};
use super::{Drag, Mirror, SurfaceView};
use crate::host::{ActionEvent, FunctionCallEvent, InputEvent, InputKind};
use crate::paint::date;

/// The Tooltip open delay.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(300);

/// A slider value at pointer x: clamped to [min, max], snapped to `step`.
pub fn slider_value_at(x: f32, left: f32, width: f32, min: f64, max: f64, step: f64) -> f64 {
    let frac = if width > 0.0 { ((x - left) / width).clamp(0.0, 1.0) as f64 } else { 0.0 };
    snap(min + frac * (max - min), min, max, step)
}

/// `v` snapped to `min + k·step` and clamped.
pub fn snap(v: f64, min: f64, max: f64, step: f64) -> f64 {
    let v = if step > 0.0 { min + ((v - min) / step).round() * step } else { v };
    let v = v.clamp(min.min(max), max.max(min));
    (v * 1e9).round() / 1e9
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

impl SurfaceView {
    /// Forward the core's OutEvents to the host. `input` = the revision and
    /// kind of a host-owned text edit (else a per-component counter).
    pub(crate) fn dispatch(&mut self, events: Vec<OutEvent>, input: Option<u64>, cx: &mut Context<Self>) {
        let host = self.host.clone();
        let surface_id = self.surface.id.clone();
        for e in events {
            match e {
                OutEvent::Action { name, component_id, event, context, payload } => host.on_action(&ActionEvent { surface_id: surface_id.clone(), event, name, component_id, context, payload }, cx),
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
                OutEvent::DataChanged { .. } | OutEvent::Relayout => {}
            }
        }
        self.nodes_dirty = true;
        cx.notify();
    }

    /// Fire `event` on node `index` through the core and dispatch the result.
    pub fn fire(&mut self, index: u32, event: &str, payload: Option<Value>, cx: &mut Context<Self>) {
        let events = self.surface.event(index, event, payload);
        self.dispatch(events, None, cx);
    }

    fn disabled(&self, index: u32) -> bool {
        let Some(n) = self.cache.node(index) else { return true };
        let off = |p: &serde_json::Map<String, Value>| matches!(p.get("disabled"), Some(Value::Bool(true)));
        off(&n.props) || n.owner.as_deref().and_then(|o| self.cache.index_of(o)).and_then(|o| self.cache.node(o)).is_some_and(|o| off(&o.props) && n.part.as_deref() != Some("label"))
    }

    /// A mirrored control value: the local one while the prop is unchanged.
    pub(crate) fn mirrored(&self, owner_id: &str, external: &Value) -> Value {
        match self.mirrors.get(owner_id) {
            Some(m) if m.external == *external => m.local.clone(),
            _ => external.clone(),
        }
    }

    pub(crate) fn set_mirror(&mut self, owner_id: &str, external: Value, local: Value) {
        self.mirrors.insert(owner_id.to_string(), Mirror { external, local });
    }

    /// A full press of node `index` (pointer up inside, Enter / Space).
    pub fn press(&mut self, index: u32, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled(index) {
            return;
        }
        let Some(n) = self.cache.node(index).cloned() else { return };
        let owner_index = self.cache.owner_of(index);
        let owner = self.cache.node(owner_index).cloned().unwrap_or_else(|| n.clone());
        if let Some(target) = n.trigger_for.clone() {
            if self.just_dismissed.take().as_deref() == Some(target.as_str()) {
                cx.notify();
                return;
            }
            self.layer_return.insert(target, n.id.clone());
            self.fire(index, "press", None, cx);
            return;
        }
        match (n.component.as_str(), n.part.as_deref()) {
            ("Select", Some("field")) => return self.toggle_popup(index, false, window, cx),
            ("DatePicker", Some("field")) => return self.toggle_popup(index, true, window, cx),
            ("Input" | "Textarea", Some("field")) | ("Slider", Some("track")) | ("Composer", None) | ("ToggleGroup", _) => return,
            _ => {}
        }
        match owner.component.as_str() {
            "Checkbox" | "Switch" => {
                let external = owner.props.get("checked").cloned().unwrap_or(Value::Bool(false));
                let current = self.mirrored(&owner.id, &external).as_bool().unwrap_or(false);
                self.set_mirror(&owner.id, external, Value::Bool(!current));
                let events = self.surface.event(owner_index, "change", Some(json!({"checked": !current})));
                return self.dispatch(events, None, cx);
            }
            "Radio" => {
                let suffix = n.id.rsplit('.').next().unwrap_or_default();
                let row = self.cache.index_of(&format!("{}.item.{suffix}", owner.id)).and_then(|r| self.cache.node(r));
                let row_index = row.map(|r| r.index);
                if let Some(value) = row.and_then(|r| r.props.get("value").cloned()) {
                    let external = owner.props.get("value").cloned().unwrap_or(Value::Null);
                    self.set_mirror(&owner.id, external, value);
                }
                // The core resolves the value from the ROW (a dot or label
                // press would find none).
                if let Some(r) = row_index {
                    return self.fire(r, "press", None, cx);
                }
            }
            "Toggle" => {
                let external = owner.props.get("pressed").cloned().unwrap_or(Value::Bool(false));
                let current = self.mirrored(&owner.id, &external).as_bool().unwrap_or(false);
                self.set_mirror(&owner.id, external, Value::Bool(!current));
            }
            _ => {}
        }
        self.fire(index, "press", None, cx);
    }

    /// Pointer down on a pressable: the `pressed` state.
    pub(crate) fn press_down(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(prev) = self.pressed.take() {
            self.set_interaction(&prev, |s| s.pressed = false);
        }
        if self.cache.index_of(id).is_some_and(|i| self.disabled(i)) {
            return;
        }
        self.pressed = Some(id.to_string());
        if self.set_interaction(id, |s| s.pressed = true) {
            cx.notify();
        }
    }

    /// Pointer up inside: press it when it was the one pressed down.
    pub(crate) fn press_up(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let was = self.pressed.take();
        self.set_interaction(id, |s| s.pressed = false);
        cx.notify();
        if was.as_deref() == Some(id) {
            if let Some(index) = self.cache.index_of(id) {
                self.press(index, window, cx);
            }
        }
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
        if self.set_interaction(id, |s| s.hover = hovered) {
            cx.notify();
        }
    }

    /// Hover on a Tooltip anchor: open after [`TOOLTIP_DELAY`], close on leave.
    pub(crate) fn tooltip_hover(&mut self, owner: &str, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        if hovered {
            let owner_id = owner.to_string();
            let task = cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(TOOLTIP_DELAY).await;
                let _ = this.update(cx, |this, cx| {
                    let events = this.surface.set_open(&owner_id, true);
                    this.dispatch(events, None, cx);
                });
            });
            self.tooltip = Some((owner.to_string(), task));
        } else {
            self.tooltip = None;
            if self.layers.iter().any(|l| l.owner == owner) {
                let events = self.surface.set_open(owner, false);
                self.dispatch(events, None, cx);
            }
        }
    }

    /// Close the overlay `owner` (outside click, scrim, Escape).
    pub(crate) fn dismiss_layer(&mut self, owner: &str, cx: &mut Context<Self>) {
        if !self.layers.iter().any(|l| l.owner == owner) {
            return;
        }
        let events = self.surface.set_open(owner, false);
        self.just_dismissed = Some(owner.to_string());
        self.dispatch(events, None, cx);
    }

    /// Escape: close the select/date popup, else the TOP layer (focus goes
    /// back to its trigger). Returns whether something closed.
    pub fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.popup.take().is_some() {
            cx.notify();
            return true;
        }
        let Some(top) = self.layers.last().cloned() else { return false };
        let events = self.surface.set_open(&top.owner, false);
        self.dispatch(events, None, cx);
        if let Some(trigger) = self.layer_return.get(&top.owner).cloned() {
            self.focus_id(&trigger, window, cx);
        }
        true
    }

    /// Focus the node with this id (a field's own handle for text fields).
    pub(crate) fn focus_id(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.focus_handles.get(id).cloned() {
            h.focus(window, cx);
        } else if let Some(f) = self.fields.get(id) {
            f.focus_handle(cx).focus(window, cx);
        }
    }

    /// Which focusable node holds focus now; its `focus` state follows.
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
        if now != self.focused {
            if let Some(prev) = self.focused.and_then(|i| self.cache.node(i)).map(|n| n.id.clone()) {
                self.set_interaction(&prev, |s| s.focus = false);
            }
            if let Some(id) = now.and_then(|i| self.cache.node(i)).map(|n| n.id.clone()) {
                self.set_interaction(&id, |s| s.focus = true);
            }
            self.focused = now;
        }
    }

    /// The Tab order in force: the top non-tooltip layer's (focus trap), else
    /// the main tree's.
    pub fn focus_order(&self) -> Vec<u32> {
        let layer = self.layers.iter().rev().find(|l| l.kind != "Tooltip").map(|l| l.layer).unwrap_or(0);
        focus_order(&self.cache.nodes, layer)
    }

    /// Move focus to the next (previous) focusable node.
    pub fn focus_next(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) -> Option<u32> {
        let order = self.focus_order();
        let next = next_focus(&order, self.focused, backwards)?;
        let id = self.cache.node(next)?.id.clone();
        self.focus_id(&id, window, cx);
        self.focused = Some(next);
        cx.notify();
        Some(next)
    }

    pub(crate) fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        let focused = self.focused.and_then(|i| self.cache.node(i)).cloned();
        let in_field = focused.as_ref().is_some_and(is_text_field);
        match key {
            "tab" => {
                self.focus_next(ev.keystroke.modifiers.shift, window, cx);
                cx.stop_propagation();
            }
            "escape" => {
                if self.escape(window, cx) {
                    cx.stop_propagation();
                }
            }
            "enter" | "space" if !in_field => {
                if let Some(n) = focused {
                    self.press(n.index, window, cx);
                    cx.stop_propagation();
                }
            }
            "left" | "right" | "down" | "up" if !in_field => {
                let Some(n) = focused else { return };
                let delta = if matches!(key, "right" | "up") { 1.0 } else { -1.0 };
                if n.component == "Slider" && n.part.as_deref() == Some("track") {
                    let (min, max, step) = (num(n.props.get("min")).unwrap_or(0.0), num(n.props.get("max")).unwrap_or(100.0), num(n.props.get("step")).unwrap_or(1.0));
                    let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
                    let external = n.props.get("value").cloned().unwrap_or(Value::Null);
                    let current = num(Some(&self.mirrored(&owner, &external))).unwrap_or(min);
                    let value = snap(current + delta * step, min, max, step);
                    self.set_mirror(&owner, external, json!(value));
                    self.fire(n.index, "change", Some(json!({"value": value})), cx);
                    cx.stop_propagation();
                } else if n.part.as_deref() == Some("indicator") {
                    let owner_index = self.cache.owner_of(n.index);
                    let count = num(n.props.get("count")).unwrap_or(0.0) as i64;
                    let page = num(n.props.get("page")).unwrap_or(0.0) as i64;
                    if count > 0 {
                        self.fire(owner_index, "change", Some(json!({"page": (page + delta as i64).rem_euclid(count)})), cx);
                        cx.stop_propagation();
                    }
                }
            }
            _ => {}
        }
    }

    /// Open-layer bookkeeping after a pass: focus the first focusable of a
    /// newly opened modal/menu layer, hand focus back when one closes.
    pub(crate) fn layers_changed(&mut self, layers: &[Layer], window: &mut Window, cx: &mut Context<Self>) {
        let now: Vec<String> = layers.iter().filter(|l| l.kind != "Tooltip").map(|l| l.owner.clone()).collect();
        if now == self.open_layers {
            return;
        }
        let opened: Vec<&Layer> = layers.iter().filter(|l| l.kind != "Tooltip" && !self.open_layers.contains(&l.owner)).collect();
        let closed: Vec<String> = self.open_layers.iter().filter(|o| !now.contains(o)).cloned().collect();
        let target = if let Some(top) = opened.last() {
            focus_order(&self.cache.nodes, top.layer).first().and_then(|i| self.cache.node(*i)).map(|n| n.id.clone())
        } else {
            closed.last().and_then(|o| self.layer_return.get(o).cloned())
        };
        self.open_layers = now;
        if let Some(id) = target {
            // Text fields are focused by the user, never by a layer opening
            // (and a focused gpui-component input needs a native window).
            let is_field = self.cache.index_of(&id).and_then(|i| self.cache.node(i)).is_some_and(is_text_field);
            if !is_field {
                cx.defer_in(window, move |this, window, cx| this.focus_id(&id, window, cx));
            }
        }
    }

    // -- Slider drags -------------------------------------------------------

    pub(crate) fn drag_start(&mut self, track_id: &str, x: f32, cx: &mut Context<Self>) {
        let Some(index) = self.cache.index_of(track_id) else { return };
        if self.disabled(index) {
            return;
        }
        let Some(bounds) = self.slider_bounds.borrow().get(track_id).copied() else { return };
        let n = &self.cache.nodes[index as usize];
        let (min, max, step) = (num(n.props.get("min")).unwrap_or(0.0), num(n.props.get("max")).unwrap_or(100.0), num(n.props.get("step")).unwrap_or(1.0));
        let value = slider_value_at(x, f32::from(bounds.origin.x), f32::from(bounds.size.width), min, max, step);
        self.drag = Some(Drag { track: track_id.to_string(), value });
        cx.notify();
    }

    pub(crate) fn on_drag_move(&mut self, ev: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.clone() else { return };
        if ev.pressed_button != Some(gpui::MouseButton::Left) {
            return;
        }
        let Some(index) = self.cache.index_of(&drag.track) else { return };
        let Some(bounds) = self.slider_bounds.borrow().get(&drag.track).copied() else { return };
        let n = &self.cache.nodes[index as usize];
        let (min, max, step) = (num(n.props.get("min")).unwrap_or(0.0), num(n.props.get("max")).unwrap_or(100.0), num(n.props.get("step")).unwrap_or(1.0));
        let value = slider_value_at(f32::from(ev.position.x), f32::from(bounds.origin.x), f32::from(bounds.size.width), min, max, step);
        if (value - drag.value).abs() > f64::EPSILON {
            self.drag = Some(Drag { value, ..drag });
            cx.notify();
        }
    }

    pub(crate) fn on_drag_end(&mut self, _ev: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // A dismissal remembered for the trigger under this click is spent.
        self.just_dismissed = None;
        let Some(drag) = self.drag.take() else { return };
        let Some(index) = self.cache.index_of(&drag.track) else { return };
        let n = self.cache.nodes[index as usize].clone();
        let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
        let external = n.props.get("value").cloned().unwrap_or(Value::Null);
        self.set_mirror(&owner, external, json!(drag.value));
        self.fire(index, "change", Some(json!({"value": drag.value})), cx);
    }

    /// The value a slider track shows (drag > mirror > prop).
    pub(crate) fn slider_value(&self, index: u32) -> f64 {
        let Some(n) = self.cache.node(index) else { return 0.0 };
        if let Some(d) = self.drag.as_ref().filter(|d| d.track == n.id) {
            return d.value;
        }
        let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
        let external = n.props.get("value").cloned().unwrap_or(Value::Null);
        num(Some(&self.mirrored(&owner, &external))).or_else(|| num(n.props.get("min"))).unwrap_or(0.0)
    }

    // -- ToggleGroup, Carousel, DatePicker ------------------------------------

    /// A ToggleGroup item press: single = that value, multiple = toggled set.
    pub(crate) fn toggle_group_select(&mut self, index: u32, value: Value, cx: &mut Context<Self>) {
        let Some(n) = self.cache.node(index).cloned() else { return };
        let multiple = n.props.get("type").and_then(Value::as_str) == Some("multiple");
        let external = n.props.get("value").cloned().unwrap_or(Value::Null);
        let current = self.mirrored(&n.id, &external);
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
        self.set_mirror(&n.id, external, next.clone());
        self.fire(index, "change", Some(json!({"value": next})), cx);
    }

    /// A DatePicker day pick.
    pub(crate) fn pick_date(&mut self, field_index: u32, (y, m, d): (i32, u32, u32), cx: &mut Context<Self>) {
        let Some(n) = self.cache.node(field_index).cloned() else { return };
        let value = Value::String(date::iso(y, m, d));
        let owner = n.owner.clone().unwrap_or_else(|| n.id.clone());
        let external = n.props.get("value").cloned().unwrap_or(Value::Null);
        self.set_mirror(&owner, external, value.clone());
        self.popup = None;
        self.fire(field_index, "change", Some(json!({"value": value})), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_values_snap_and_clamp() {
        assert_eq!(slider_value_at(50.0, 0.0, 100.0, 0.0, 100.0, 5.0), 50.0);
        assert_eq!(slider_value_at(52.0, 0.0, 100.0, 0.0, 100.0, 5.0), 50.0);
        assert_eq!(slider_value_at(53.0, 0.0, 100.0, 0.0, 100.0, 5.0), 55.0);
        assert_eq!(slider_value_at(-10.0, 0.0, 100.0, 0.0, 100.0, 5.0), 0.0);
        assert_eq!(slider_value_at(500.0, 0.0, 100.0, 0.0, 100.0, 5.0), 100.0);
        assert_eq!(slider_value_at(10.0, 0.0, 0.0, 0.0, 100.0, 5.0), 0.0);
        assert_eq!(snap(0.30000000004, 0.0, 1.0, 0.1), 0.3);
    }
}
