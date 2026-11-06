//! VAPP-279 (docs/round-7-contract.md): a `variant: strip` Tabs: the fit
//! (which tabs keep an item, the rest fold into `+N`) after each pass, the
//! `+N` press (`more {values}`), and the reorder drag + keyboard move
//! (`reorder {from, to, value}`). The pure rules live in
//! [`crate::tabs_strip`].

use serde_json::{json, Value};
use taffy::style::Direction;

use super::{Frame, OutEvent, Surface};
use crate::layout_tree::TabDrag;
use crate::tabs_strip::{move_tab, partition_tabs, tab_drop_slot, tab_key_slot, tab_reorder};

/// One strip as laid out: the array indexes in strip order and each one's
/// item frame.
struct StripView {
    order: Vec<usize>,
    frames: Vec<Option<Frame>>,
    active: Option<usize>,
    rtl: bool,
}

impl Surface {
    pub(super) fn is_tab_strip(&self, id: &str) -> bool {
        self.node_by_id(id).is_some_and(|n| n.component == "Tabs" && n.part.is_none() && n.props.get("variant").and_then(Value::as_str) == Some("strip"))
    }

    fn strip_view(&self, id: &str, frames: &[Frame]) -> Option<StripView> {
        let owner = self.node_by_id(id)?;
        let tabs = owner.props.get("tabs").and_then(Value::as_array)?;
        let n = tabs.len();
        let order: Vec<usize> = self.local.tab_order.get(id).filter(|o| o.len() == n).cloned().unwrap_or_else(|| (0..n).collect());
        let frame = |i: usize| self.slot_of.get(&format!("{id}.item.{i}")).and_then(|&s| frames.get(s as usize).copied());
        let selected = |i: usize| self.slot_of.get(&format!("{id}.item.{i}")).and_then(|&s| self.nodes[s as usize].part_query.as_ref()).is_some_and(|q| q.states.iter().any(|s| s == "selected"));
        let frames_in_order = order.iter().map(|&i| frame(i)).collect();
        let active = order.iter().position(|&i| selected(i));
        let rtl = self.slot_of.get(id).and_then(|&s| self.node_state.get(s as usize)).is_some_and(|s| s.direction == Direction::Rtl);
        Some(StripView { order, frames: frames_in_order, active, rtl })
    }

    /// After a pass: refit every strip (not while its drag is live). True
    /// when a strip's visible set changed (its subtree is queued for a
    /// rebuild; the pass runs again).
    pub(super) fn fit_tab_strips(&mut self, frames: &[Frame]) -> bool {
        let ids: Vec<String> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| self.live.get(*i).copied().unwrap_or(false) && n.component == "Tabs" && n.part.is_none() && n.props.get("variant").and_then(Value::as_str) == Some("strip"))
            .map(|(_, n)| n.id.clone())
            .collect();
        let mut changed = false;
        for id in ids {
            if self.local.tab_drags.contains_key(&id) {
                continue;
            }
            let Some(view) = self.strip_view(&id, frames) else { continue };
            if view.frames.iter().any(Option::is_none) {
                continue;
            }
            let Some(list) = self.slot_of.get(&format!("{id}.list")).copied() else { continue };
            let list_frame = frames.get(list as usize).copied().unwrap_or_default();
            let gap = self.engine.style(list).gap.width.into_raw().value();
            let width_of = |part: &str| self.slot_of.get(&format!("{id}.{part}")).and_then(|&s| frames.get(s as usize)).map(|f| f.w);
            let new_w = width_of("new").map(|w| w + gap).unwrap_or(0.0);
            let more_w = width_of("more").unwrap_or(0.0);
            let widths: Vec<f32> = view.frames.iter().map(|f| f.map(|f| f.w).unwrap_or(0.0)).collect();
            let slots = partition_tabs(&widths, list_frame.w - new_w, gap, more_w, view.active);
            let mut visible: Vec<usize> = slots.iter().map(|&k| view.order[k]).collect();
            visible.sort_unstable();
            let current = self.local.tab_fit.get(&id).cloned().unwrap_or_else(|| (0..view.order.len()).collect());
            if visible != current {
                if visible.len() == view.order.len() {
                    self.local.tab_fit.remove(&id);
                } else {
                    self.local.tab_fit.insert(id.clone(), visible);
                }
                self.subtree_pending.insert(id);
                changed = true;
            }
        }
        changed
    }

    /// The visible slots of a strip: (array index per slot, item centres).
    fn strip_slots(&self, id: &str) -> Option<(StripView, Vec<usize>, Vec<f32>)> {
        let view = self.strip_view(id, &self.last_frames)?;
        let fit = self.local.tab_fit.get(id);
        let mut slots = Vec::new();
        let mut centers = Vec::new();
        for (k, &i) in view.order.iter().enumerate() {
            if fit.is_some_and(|f| !f.contains(&i)) {
                continue;
            }
            slots.push(i);
            centers.push(view.frames[k].map(|f| f.x + f.w / 2.0).unwrap_or(0.0));
        }
        Some((view, slots, centers))
    }

    /// The `+N` press: `more {values}` = the folded tabs' values in order.
    pub(super) fn tab_more(&mut self, owner_id: &str) -> Vec<OutEvent> {
        let Some(fit) = self.local.tab_fit.get(owner_id).cloned() else { return vec![] };
        let Some(owner) = self.node_by_id(owner_id) else { return vec![] };
        let tabs = owner.props.get("tabs").and_then(Value::as_array).cloned().unwrap_or_default();
        let values: Vec<Value> = tabs.iter().enumerate().filter(|(i, _)| !fit.contains(i)).map(|(_, t)| t.get("value").cloned().unwrap_or(Value::Null)).collect();
        self.fire(owner_id, "more", Some(json!({"values": values})))
    }

    fn tab_index(n_id: &str, owner_id: &str) -> Option<usize> {
        n_id.strip_prefix(&format!("{owner_id}.tab.")).and_then(|i| i.parse().ok())
    }

    fn fire_reorder(&mut self, owner_id: &str, from: usize, to: usize) -> Vec<OutEvent> {
        let value = self.node_by_id(owner_id).and_then(|o| o.props.get("tabs")).and_then(|t| t.get(from)).and_then(|t| t.get("value")).cloned().unwrap_or(Value::Null);
        self.fire(owner_id, "reorder", Some(json!({"from": from, "to": to, "value": value})))
    }

    /// `drag {phase: start|move|end, delta}` on a strip tab (`delta` = px
    /// on the row since the drag STARTED, screen direction): the move
    /// previews the order, the end drops the preview and fires `reorder`
    /// when the tab changed place (the host owns `tabs`).
    pub(super) fn tab_drag(&mut self, n_id: &str, owner_id: &str, payload: Option<&Value>) -> Vec<OutEvent> {
        let Some(index) = Self::tab_index(n_id, owner_id) else { return vec![] };
        let phase = payload.and_then(|p| p.get("phase")).and_then(Value::as_str).unwrap_or("move");
        let delta = payload.and_then(|p| p.get("delta")).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        if phase == "start" || !self.local.tab_drags.contains_key(owner_id) {
            let Some((view, slots, centers)) = self.strip_slots(owner_id) else { return vec![] };
            let Some(from_slot) = slots.iter().position(|&i| i == index) else { return vec![] };
            self.local.tab_drags.insert(owner_id.to_string(), TabDrag { from_slot, slots, centers, order: view.order, rtl: view.rtl });
            if phase == "start" {
                return vec![];
            }
        }
        let drag = self.local.tab_drags.get(owner_id).cloned().unwrap_or_default();
        let slot = tab_drop_slot(&drag.centers, drag.from_slot, delta, drag.rtl);
        let moved = tab_reorder(&drag.slots, drag.from_slot, slot);
        if phase != "end" {
            let preview = match moved {
                Some((from, to)) => {
                    let a = drag.order.iter().position(|&i| i == from).unwrap_or(0);
                    let b = drag.order.iter().position(|&i| i == to).unwrap_or(0);
                    move_tab(&drag.order, a, b)
                }
                None => drag.order.clone(),
            };
            let current = self.local.tab_order.get(owner_id).cloned().unwrap_or_else(|| drag.order.clone());
            if preview == current {
                return vec![];
            }
            self.local.tab_order.insert(owner_id.to_string(), preview);
            self.subtree_pending.insert(owner_id.to_string());
            return vec![OutEvent::Relayout];
        }
        self.local.tab_drags.remove(owner_id);
        let had_preview = self.local.tab_order.remove(owner_id).is_some();
        let mut out = vec![];
        if had_preview {
            self.subtree_pending.insert(owner_id.to_string());
            out.push(OutEvent::Relayout);
        }
        if let Some((from, to)) = moved {
            out.extend(self.fire_reorder(owner_id, from, to));
        }
        out
    }

    /// `key {key}` on a strip tab: the painter sends ArrowLeft/ArrowRight
    /// for Ctrl/Cmd+Shift+Arrow; one place in screen direction, none past
    /// an end.
    pub(super) fn tab_key(&mut self, n_id: &str, owner_id: &str, payload: Option<&Value>) -> Vec<OutEvent> {
        let Some(index) = Self::tab_index(n_id, owner_id) else { return vec![] };
        let key = payload.and_then(|p| p.get("key")).and_then(Value::as_str).unwrap_or("");
        let Some((view, slots, _)) = self.strip_slots(owner_id) else { return vec![] };
        let Some(from_slot) = slots.iter().position(|&i| i == index) else { return vec![] };
        let Some(to_slot) = tab_key_slot(slots.len(), from_slot, key, view.rtl) else { return vec![] };
        match tab_reorder(&slots, from_slot, to_slot) {
            Some((from, to)) => self.fire_reorder(owner_id, from, to),
            None => vec![],
        }
    }
}
