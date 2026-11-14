//! EXP-1832 (fixtures/list-reorder.json): the core OWNS a `reorderable`
//! List's drag, as it does a strip Tabs' (`tabs.rs`). A renderer decides
//! WHEN a drag starts (a pointer past `tabDragThreshold`, a touch after
//! `tabLongPressMs`) and forwards its phases with the pointer
//! ([`Surface::list_drag`]); the core finds the row, keeps the live drop
//! ([`crate::list_reorder::list_drop_slot`] over the rendered rows) and
//! fires `move {id, before | after | into}` on a drop with a slot (nothing
//! on a cancel or a release with no slot). The renderers paint what
//! [`Surface::list_drag_paint`] answers: the dragged row dimmed, the lifted
//! copy, the drop line + ring or the into highlight. The List never
//! reorders its data: the host applies the move.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Frame, OutEvent, Surface};
use crate::list_reorder::{list_drop_slot, list_move_context, ListDrop, ListDragState, ListDropPosition, ListReorderRow, LIST_DROP_LINE, LIST_DROP_RING, LIST_LIFT_OFFSET};

/// One drag input on a reorderable List (`x`, `y` = the pointer in the
/// space of the layout's frames, which are UNSCROLLED: add the scroll
/// offsets the painter applies between the surface and the List). `start` names the pressed node (any node
/// inside a row); `move` / `end` / `cancel` may name any node of the
/// surface (the drag is found by its List).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "lowercase")]
pub enum ListDragInput {
    Start { x: f32, y: f32 },
    Move { x: f32, y: f32 },
    End { x: f32, y: f32 },
    Cancel,
}

/// What a renderer paints for one live List drag (frames in the layout's
/// space, computed from the LAST pass).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDragPaint {
    /// The List's id and slot.
    pub list: String,
    pub list_index: u32,
    /// The dragged row's key and slot: paint it at `LIST_DRAG_DIM` (0.4).
    pub key: String,
    pub row: u32,
    /// The lifted copy of the dragged row: the row painted again at this
    /// frame (`LIST_LIFT_OFFSET` right of it, following the pointer), on the
    /// theme's popover colour, radius `$radius.md`, the overlay shadow.
    pub lift: Frame,
    /// The live drop (None = no slot here: nothing drawn, a release fires
    /// nothing).
    pub drop: Option<ListDrop>,
    /// The row the drop is relative to.
    pub drop_row: Option<u32>,
    /// before / after: the 2px primary line across the row's edge from its
    /// indent (`depth × treeGuideColumn`), radius 1.
    pub line: Option<Frame>,
    /// before / after: the ring at the line's start: an `LIST_DROP_RING`
    /// circle, a `LIST_DROP_LINE` primary stroke, filled with the
    /// background colour.
    pub ring: Option<Frame>,
    /// into: the row's box: a `LIST_DROP_LINE` primary ring (inset), the
    /// primary at `LIST_INTO_FILL` as fill, radius `$radius.md`.
    pub into: Option<Frame>,
}

struct ListRows {
    list: String,
    list_index: u32,
    max_depth: f64,
    rows: Vec<ListReorderRow>,
    slots: Vec<u32>,
}

fn reorderable(props: &crate::types::Props) -> bool {
    props.get("reorderable").and_then(Value::as_bool) == Some(true)
}

impl Surface {
    /// The reorderable List row `index` sits in: (List slot, row slot).
    /// None = the node is in no reorderable List's row (or inside a nested
    /// reorderable List, whose own row it is then).
    pub fn list_reorder_row(&self, index: u32) -> Option<(u32, u32)> {
        let mut at = index;
        let mut guard = 0;
        loop {
            let n = self.layout_node(at)?;
            let parent = n.parent?;
            let p = self.layout_node(parent)?;
            if p.component == "List" && p.part.is_none() && reorderable(&p.props) && !(n.part.is_some() && n.owner.as_deref() == Some(p.id.as_str())) {
                return Some((parent, at));
            }
            at = parent;
            guard += 1;
            if guard > 4096 {
                return None;
            }
        }
    }

    /// The rendered rows of a reorderable List (slot `list`), in order.
    fn list_rows(&self, list: u32) -> Option<ListRows> {
        let owner = self.layout_node(list)?;
        if owner.component != "List" || !reorderable(&owner.props) {
            return None;
        }
        let max_depth = owner.props.get("maxDepth").and_then(Value::as_f64).unwrap_or(1.0);
        let source = self.source_node(&owner.id);
        let template = source.as_ref().and_then(|s| s.template.clone());
        let (base, keys) = match &template {
            Some(t) => {
                let base = crate::data::absolute_path(&t.path, &owner.scope);
                let items = crate::data::get_pointer(&self.data, &base).and_then(Value::as_array).cloned().unwrap_or_default();
                (Some(base), crate::list::template_item_keys(&items, t.key.as_deref()))
            }
            None => (None, vec![]),
        };
        let mut rows = vec![];
        let mut slots = vec![];
        for &c in &owner.children {
            let Some(child) = self.layout_node(c) else { continue };
            if child.part.is_some() && child.owner.as_deref() == Some(owner.id.as_str()) {
                continue;
            }
            let frame = self.last_frame(c).unwrap_or_default();
            let item_index = base.as_ref().and_then(|b| child.scope.strip_prefix(b.as_str())).and_then(|rest| rest.strip_prefix('/')).and_then(|i| i.parse::<usize>().ok());
            let (key, item) = match item_index {
                Some(i) => (keys.get(i).cloned().unwrap_or_else(|| i.to_string()), crate::data::get_pointer(&self.data, &child.scope)),
                None => (source.as_ref().and_then(|s| s.children.iter().find(|u| child.id.starts_with(u.id.as_str()))).map(|u| u.id.clone()).unwrap_or_else(|| child.id.clone()), None),
            };
            let depth = item.and_then(|v| v.get("depth")).and_then(Value::as_f64).unwrap_or(0.0);
            let group = item.and_then(|v| v.get("group")).and_then(Value::as_bool).unwrap_or(false);
            rows.push(ListReorderRow { key, top: frame.y, height: frame.h, depth, group });
            slots.push(c);
        }
        Some(ListRows { list: owner.id.clone(), list_index: list, max_depth, rows, slots })
    }

    /// The List whose drag is live (by slot), if any.
    fn live_list_drag(&self) -> Option<u32> {
        self.local.list_drags.keys().find_map(|id| self.index_of(id))
    }

    /// EXP-1832: one drag input on a reorderable List (see [`ListDragInput`]).
    /// `start` on a node inside a row begins the drag (a second start
    /// restarts it); `move` updates the live drop; `end` fires `move {id,
    /// before | after | into}` when the drop has a slot; `cancel` (Esc, a
    /// lost pointer) drops it silently. Returns the fired events; ask
    /// [`Self::list_drag_paint`] after every call and repaint.
    pub fn list_drag(&mut self, index: u32, input: &ListDragInput) -> Vec<OutEvent> {
        self.ensure_reduced();
        match input {
            ListDragInput::Start { x, y } => {
                let Some((list, row)) = self.list_reorder_row(index) else { return vec![] };
                let Some(rows) = self.list_rows(list) else { return vec![] };
                let Some(i) = rows.slots.iter().position(|&s| s == row) else { return vec![] };
                if self.layout_node(list).is_some_and(|n| self.is_inert(n)) {
                    return vec![];
                }
                let key = rows.rows[i].key.clone();
                let offset_y = y - rows.rows[i].top;
                self.local.list_drags.clear();
                self.local.list_drags.insert(rows.list.clone(), ListDragState { key, offset_y, x: *x, y: *y, drop: None });
                vec![]
            }
            ListDragInput::Move { x, y } => {
                let Some(list) = self.live_list_drag() else { return vec![] };
                let Some(rows) = self.list_rows(list) else {
                    self.local.list_drags.clear();
                    return vec![];
                };
                let Some(state) = self.local.list_drags.get_mut(&rows.list) else { return vec![] };
                state.x = *x;
                state.y = *y;
                state.drop = list_drop_slot(&rows.rows, &state.key, *y, rows.max_depth);
                vec![]
            }
            ListDragInput::End { y, .. } => {
                let Some(list) = self.live_list_drag() else { return vec![] };
                let rows = self.list_rows(list);
                let id = self.layout_node(list).map(|n| n.id.clone()).unwrap_or_default();
                let Some(state) = self.local.list_drags.remove(&id) else { return vec![] };
                let Some(rows) = rows else { return vec![] };
                match list_drop_slot(&rows.rows, &state.key, *y, rows.max_depth) {
                    Some(drop) => self.fire(&id, "move", Some(list_move_context(&state.key, &drop))),
                    None => vec![],
                }
            }
            ListDragInput::Cancel => {
                self.local.list_drags.clear();
                vec![]
            }
        }
    }

    /// Is a List drag live (a renderer swallows the release's press and
    /// takes Esc while it is)?
    pub fn list_dragging(&self) -> bool {
        !self.local.list_drags.is_empty()
    }

    /// EXP-1832: what to paint for the live List drag(s), from the last
    /// pass's frames (empty = none).
    pub fn list_drag_paint(&self) -> Vec<ListDragPaint> {
        let mut out = vec![];
        for (id, state) in &self.local.list_drags {
            let Some(list) = self.index_of(id) else { continue };
            let Some(rows) = self.list_rows(list) else { continue };
            let Some(i) = rows.rows.iter().position(|r| r.key == state.key) else { continue };
            let row = rows.slots[i];
            let frame = self.last_frame(row).unwrap_or_default();
            let lift = Frame { x: frame.x + LIST_LIFT_OFFSET, y: state.y - state.offset_y, w: frame.w, h: frame.h };
            let mut paint = ListDragPaint { list: id.clone(), list_index: list, key: state.key.clone(), row, lift, drop: state.drop.clone(), drop_row: None, line: None, ring: None, into: None };
            if let Some(drop) = &state.drop {
                if let Some(j) = rows.rows.iter().position(|r| r.key == drop.key) {
                    let target = self.last_frame(rows.slots[j]).unwrap_or_default();
                    paint.drop_row = Some(rows.slots[j]);
                    match drop.position {
                        ListDropPosition::Into => paint.into = Some(target),
                        position => {
                            let indent = (rows.rows[j].depth.max(0.0).floor() * crate::layout::TREE_GUIDE_COLUMN) as f32;
                            let edge = if position == ListDropPosition::Before { target.y } else { target.y + target.h };
                            let x = target.x + indent;
                            paint.line = Some(Frame { x, y: edge - LIST_DROP_LINE / 2.0, w: (target.w - indent).max(0.0), h: LIST_DROP_LINE });
                            paint.ring = Some(Frame { x: x - LIST_DROP_RING / 2.0, y: edge - LIST_DROP_RING / 2.0, w: LIST_DROP_RING, h: LIST_DROP_RING });
                        }
                    }
                }
            }
            out.push(paint);
        }
        out
    }
}
