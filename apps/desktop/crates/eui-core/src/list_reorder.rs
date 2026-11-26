//! EXP-1832 (fixtures/list-reorder.json): the pure rules of a `reorderable`
//! List, the twin of `packages/exponential-ui/src/list-reorder.ts` (same
//! vectors). A List item may carry `depth` (0 = top) and `group` (true = a
//! row that holds rows, e.g. a folder); `maxDepth` caps how deep a drop may
//! nest. The surface's drag (`Surface::list_drag`) runs on these.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One RENDERED row of the list, in list order: its key, its box along the
/// list's axis (`top`, `height`), its `depth` and whether it holds rows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ListReorderRow {
    pub key: String,
    pub top: f32,
    pub height: f32,
    #[serde(default)]
    pub depth: f64,
    #[serde(default)]
    pub group: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListDropPosition {
    Before,
    After,
    Into,
}

impl ListDropPosition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
            Self::Into => "into",
        }
    }
}

/// Where a drag drops: relative to the row `key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListDrop {
    pub key: String,
    pub position: ListDropPosition,
}

/// The fraction of a row's height at each end that means "beside" when the
/// row also accepts a drop INTO it (the middle half = into).
pub const LIST_DROP_EDGE: f32 = 0.25;
/// The dragged row's opacity while it is lifted.
pub const LIST_DRAG_DIM: f32 = 0.4;
/// The drop line's thickness and its start ring's diameter (px).
pub const LIST_DROP_LINE: f32 = 2.0;
pub const LIST_DROP_RING: f32 = 8.0;
/// How far right of the dragged row the lifted copy rides (px).
pub const LIST_LIFT_OFFSET: f32 = 12.0;
/// The lifted copy's opacity.
pub const LIST_LIFT_OPACITY: f32 = 0.95;
/// The into highlight's fill: the primary at this alpha.
pub const LIST_INTO_FILL: f32 = 0.14;

fn depth_of(row: &ListReorderRow) -> i64 {
    if row.depth.is_finite() {
        row.depth.floor().max(0.0) as i64
    } else {
        0
    }
}

/// The dragged row's subtree: itself + the rows right after it that sit
/// deeper. `[start, end)`.
fn subtree(rows: &[ListReorderRow], index: usize) -> (usize, usize) {
    let depth = depth_of(&rows[index]);
    let mut end = index + 1;
    while end < rows.len() && depth_of(&rows[end]) > depth {
        end += 1;
    }
    (index, end)
}

/// Levels the dragged subtree spans below its own row: 1 for a `group`
/// (even folded), else the deepest rendered descendant.
fn height_of(rows: &[ListReorderRow], index: usize) -> i64 {
    let (start, end) = subtree(rows, index);
    let mut height = i64::from(rows[index].group);
    for row in &rows[start + 1..end] {
        height = height.max(depth_of(row) - depth_of(&rows[index]));
    }
    height
}

/// Where a drag over the list drops (see the fixture's `$comment`): the row
/// under `at` (above the first = the first, below the last = the last);
/// INTO when `depth + 1 + height < max_depth` (outer quarters = before /
/// after, middle half = into), else upper half = before, lower = after;
/// `after` an open group = `before` its first child; beside needs
/// `depth + height < max_depth`. `None` = no drop.
pub fn list_drop_slot(rows: &[ListReorderRow], drag_key: &str, at: f32, max_depth: f64) -> Option<ListDrop> {
    let drag_index = rows.iter().position(|r| r.key == drag_key)?;
    let index = rows.iter().position(|r| at >= r.top && at < r.top + r.height).unwrap_or(if at < rows[0].top { 0 } else { rows.len() - 1 });
    let (start, end) = subtree(rows, drag_index);
    if index >= start && index < end {
        return None;
    }
    let max_depth = if max_depth.is_finite() { max_depth } else { 1.0 };
    let row = &rows[index];
    let height = height_of(rows, drag_index);
    let depth = depth_of(row);
    let into = ((depth + 1 + height) as f64) < max_depth;
    let frac = if row.height > 0.0 { (at - row.top) / row.height } else { 0.0 };
    let mut position = if into {
        if frac < LIST_DROP_EDGE {
            ListDropPosition::Before
        } else if frac >= 1.0 - LIST_DROP_EDGE {
            ListDropPosition::After
        } else {
            ListDropPosition::Into
        }
    } else if frac < 0.5 {
        ListDropPosition::Before
    } else {
        ListDropPosition::After
    };
    if position == ListDropPosition::Into {
        return Some(ListDrop { key: row.key.clone(), position });
    }
    let mut target = row;
    if position == ListDropPosition::After && row.group && index + 1 < rows.len() && depth_of(&rows[index + 1]) > depth {
        target = &rows[index + 1];
        position = ListDropPosition::Before;
        if index + 1 >= start && index + 1 < end {
            return None;
        }
    }
    if ((depth_of(target) + height) as f64) >= max_depth {
        return None;
    }
    Some(ListDrop { key: target.key.clone(), position })
}

/// The `move` event's context for a drop: `{id, before | after | into}`.
pub fn list_move_context(drag_key: &str, drop: &ListDrop) -> Value {
    let mut context = json!({"id": drag_key});
    context[drop.position.as_str()] = Value::String(drop.key.clone());
    context
}

/// A reorderable List's live drag (the core's local state): the dragged
/// row's key, the pointer's offset inside it at the start, the pointer now
/// and the drop it points at.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListDragState {
    pub key: String,
    /// Pointer y minus the dragged row's top at the start.
    pub offset_y: f32,
    /// Pointer x minus the dragged row's left at the start (a Board card's
    /// lifted copy follows the pointer on both axes).
    pub offset_x: f32,
    pub x: f32,
    pub y: f32,
    pub drop: Option<ListDrop>,
    /// A row from OUTSIDE the List (`Surface::list_drag_external`), else
    /// None.
    pub external: Option<ListExternalRow>,
    /// A Board drag (the List has a `board`): the drop over its columns
    /// (`Some(None)` = no slot) and the column List under the pointer.
    pub board: Option<Option<BoardDrop>>,
    pub over: Option<u32>,
}

/// A row from outside a reorderable List dragged onto it (web's
/// `startListDrag` with its own `rows()`): it counts as the LAST row at its
/// own top and height (depth 0), and only the List's rows named in `keys`
/// (None = all) take the drop; a List row with the dragged key steps aside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListExternalRow {
    pub top: f32,
    pub height: f32,
    pub keys: Option<Vec<String>>,
}

/// genUI round 4 (fixtures/board-drop.json): one column of a Board = one of
/// its reorderable Lists: its key, its box across (`left`, `width`) and its
/// rendered cards.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoardColumn {
    pub key: String,
    pub left: f32,
    pub width: f32,
    pub rows: Vec<ListReorderRow>,
}

/// A Board drop: the column and the card it lands beside (None = an empty
/// column).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardDrop {
    pub column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<ListDropPosition>,
}

/// The index of the span nearest `at` (inside one = it; a tie = the earlier).
fn nearest<T>(items: &[T], at: f32, span: impl Fn(&T) -> (f32, f32)) -> Option<usize> {
    let mut best = None;
    let mut gap = f32::INFINITY;
    for (i, item) in items.iter().enumerate() {
        let (s, size) = span(item);
        let d = if at < s { s - at } else if at >= s + size { at - (s + size) } else { 0.0 };
        if d < gap {
            gap = d;
            best = Some(i);
        }
    }
    best
}

/// Where a card drag over a Board drops (the twin of `boardDropSlot`): the
/// column under `x` (else the nearer one), in it the card under `y` (else
/// the nearer one): its upper half = before, its lower half = after; over
/// the dragged card = None; a column with no cards = `{column}`.
pub fn board_drop_slot(columns: &[BoardColumn], drag_key: &str, x: f32, y: f32) -> Option<BoardDrop> {
    if !columns.iter().any(|c| c.rows.iter().any(|r| r.key == drag_key)) {
        return None;
    }
    let column = &columns[nearest(columns, x, |c| (c.left, c.width))?];
    let Some(i) = nearest(&column.rows, y, |r| (r.top, r.height)) else {
        return Some(BoardDrop { column: column.key.clone(), key: None, position: None });
    };
    let row = &column.rows[i];
    if row.key == drag_key {
        return None;
    }
    let frac = if row.height > 0.0 { (y - row.top) / row.height } else { 0.0 };
    let position = if frac < 0.5 { ListDropPosition::Before } else { ListDropPosition::After };
    Some(BoardDrop { column: column.key.clone(), key: Some(row.key.clone()), position: Some(position) })
}

/// A Board drop's `move` context: `{id, column, before | after}`.
pub fn board_move_context(drag_key: &str, drop: &BoardDrop) -> Value {
    let mut context = json!({"id": drag_key, "column": drop.column});
    if let (Some(key), Some(position)) = (&drop.key, drop.position) {
        context[position.as_str()] = Value::String(key.clone());
    }
    context
}
