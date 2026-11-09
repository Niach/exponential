//! #EDE-65 lane O: a scrolling List that keeps the reader's place, settled
//! in the frame it lays out (prepaint, after the frame's pass): the host's
//! claimed rows measured first ([`HostPlugin::host_leaf_prepaint`]), then
//! the list held at its END while it follows (the old gpui `list`'s
//! `FollowMode::Tail`: a new or growing row never paints the old bottom), or
//! with the row at its top where it was on screen (rows loading or measured
//! above it never move what the reader looks at), and laid out again until
//! nothing moves. A reader's wheel, scrollbar drag or key up stops following;
//! reaching the end follows again.
//!
//! [`HostPlugin::host_leaf_prepaint`]: crate::host::HostPlugin::host_leaf_prepaint

use gpui::{Context, Window};

use super::SurfaceView;

/// How many passes one frame's settling may take (a measured row moves
/// the window, the rows entering it measure in turn).
const SETTLE_PASSES: usize = 6;

/// A list kept in place ([`SurfaceView::keep_list_place`]).
#[derive(Debug, Clone, Default)]
pub(crate) struct KeepPlace {
    follow: bool,
    /// Leading rows never anchored (a paging sentinel).
    skip: usize,
    /// The row at the reader's top: its key and its top's distance from
    /// the viewport's top.
    anchor: Option<(String, f32)>,
    /// The offset the last frame settled at (`None` = the reader moved it).
    settled: Option<f32>,
    /// Rows that do not fill the viewport sit at its END (a chat's short
    /// transcript ends right above its composer), not at its top.
    end: bool,
    /// The space above the first row this frame (`end` only).
    slack: f32,
}

impl SurfaceView {
    /// #EDE-65 lane O: the List `id` (its own scroll) keeps the reader's
    /// place from now on: `follow` = it stays at its end, else the row at
    /// its top stays where it is on screen; the first `skip_leading` rows
    /// are never that row (a paging sentinel above the history).
    pub fn keep_list_place(&mut self, id: &str, follow: bool, skip_leading: usize, cx: &mut Context<Self>) {
        let keep = self.keeps.entry(id.to_string()).or_default();
        keep.skip = skip_leading;
        if keep.follow != follow {
            keep.follow = follow;
            keep.anchor = None;
            keep.settled = None;
            cx.notify();
        }
    }

    /// #EDE-65 lane O: the kept List `id`'s rows, when they do not fill its
    /// viewport, sit at its end (paint only: the slack goes above the first
    /// row; a list that overflows scrolls as before).
    pub fn anchor_list_end(&mut self, id: &str, end: bool, cx: &mut Context<Self>) {
        let keep = self.keeps.entry(id.to_string()).or_default();
        if keep.end != end {
            keep.end = end;
            cx.notify();
        }
    }

    /// The space above the end-anchored List `index`'s first row (0 when it
    /// is not, or its rows fill it).
    pub(crate) fn end_slack(&self, index: u32) -> f32 {
        if self.keeps.is_empty() {
            return 0.0;
        }
        let Some(node) = self.cache.node(index) else { return 0.0 };
        self.keeps.get(&node.id).filter(|keep| keep.end).map_or(0.0, |keep| keep.slack)
    }

    /// Whether the kept List `id` follows its end (`None` = not kept).
    pub fn list_follows(&self, id: &str) -> Option<bool> {
        self.keeps.get(id).map(|keep| keep.follow)
    }

    /// The reader moved the scroll container `index` from `from` to `to`
    /// (wheel, scrollbar, keys): up and off the end stops following, the
    /// end follows again; the row at the top is taken anew.
    pub(crate) fn reader_scrolled(&mut self, index: u32, from: f32, to: f32) {
        if self.keeps.is_empty() {
            return;
        }
        let Some(id) = self.cache.node(index).map(|n| n.id.clone()) else { return };
        let Some((_, viewport, content)) = self.scroll_extent(&id) else { return };
        let Some(keep) = self.keeps.get_mut(&id) else { return };
        let at_end = to >= (content - viewport).max(0.0) - 1.0;
        if at_end {
            keep.follow = true;
        } else if to < from - 0.5 {
            keep.follow = false;
        }
        keep.anchor = None;
        keep.settled = None;
    }

    /// Prepaint, after the frame's pass: the host's rows measured, the kept
    /// lists held in place, laid out again until nothing moves.
    pub(crate) fn settle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.keeps.keys().cloned().collect();
        // Moved since the last frame by the host (a scroll to a row): its
        // new place is the reader's; at the end it follows again.
        for id in &ids {
            let extent = self.scroll_extent(id);
            let Some(keep) = self.keeps.get_mut(id) else { continue };
            if let (Some(settled), Some((offset, viewport, content))) = (keep.settled, extent) {
                if (settled - offset).abs() > 0.5 {
                    keep.anchor = None;
                    if offset >= (content - viewport).max(0.0) - 1.0 {
                        keep.follow = true;
                    }
                }
            }
        }
        for _ in 0..SETTLE_PASSES {
            let measured = self.prepare_host_leaves(window, cx);
            if measured {
                self.pass(window, cx);
            }
            let mut moved = false;
            for id in &ids {
                moved |= self.hold(id);
            }
            if moved {
                self.pass(window, cx);
            }
            if !measured && !moved {
                break;
            }
        }
        for id in &ids {
            self.take_place(id);
        }
    }

    /// The host's claimed rows on this pass brought up to date; `true` =
    /// one changed height (marked to lay out again).
    fn prepare_host_leaves(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let leaves: Vec<(u32, String, f32)> = {
            let embeds = self.embeds.borrow();
            self.cache
                .nodes
                .iter()
                .filter(|n| n.component == "Embed" && !n.removed && !n.hidden && !embeds.contains_key(&n.id))
                .filter_map(|n| Some((n.index, n.id.clone(), self.frames.get(n.index as usize)?.w)))
                .collect()
        };
        let mut changed = false;
        for (index, id, width) in leaves {
            if width > 0.0 && self.host.host_leaf_prepaint(&id, width, window, cx) {
                self.surface.mark_dirty(index);
                changed = true;
            }
        }
        changed
    }

    /// The kept List `id` moved to its place (the end, or its anchor row's);
    /// `true` = it scrolled.
    fn hold(&mut self, id: &str) -> bool {
        let Some(keep) = self.keeps.get(id).cloned() else { return false };
        let Some((offset, viewport, content)) = self.scroll_extent(id) else { return false };
        let max = (content - viewport).max(0.0);
        let target = if keep.follow {
            max
        } else if let Some((key, y)) = &keep.anchor {
            let Some((keys, tops)) = self.surface.list_rows(id) else { return false };
            let Some(pos) = keys.iter().position(|k| k == key) else { return false };
            (tops[pos] - y).clamp(0.0, max)
        } else {
            return false;
        };
        if (target - offset).abs() <= 0.5 {
            return false;
        }
        let x = self.surface.scroll_offset(id).0;
        self.surface.scroll_to(id, x, target)
    }

    /// After settling: where the List `id` sits, and the row at its top.
    fn take_place(&mut self, id: &str) {
        let Some((offset, _, _)) = self.scroll_extent(id) else { return };
        let follow = self.keeps.get(id).is_some_and(|keep| keep.follow);
        let anchor = if follow {
            None
        } else {
            let skip = self.keeps.get(id).map_or(0, |keep| keep.skip);
            self.surface.list_rows(id).and_then(|(keys, tops)| {
                (skip..keys.len()).find(|&pos| tops[pos + 1] > offset + 0.5).map(|pos| (keys[pos].clone(), tops[pos] - offset))
            })
        };
        // The rows' own extent (the scroll's content is never shorter than
        // its viewport): what an end-anchored list leaves above them.
        let slack = match (self.keeps.get(id).is_some_and(|keep| keep.end), self.scroll_extent(id)) {
            (true, Some((_, viewport, _))) => self.surface.list_rows(id).and_then(|(_, tops)| tops.last().copied()).map_or(0.0, |rows| (viewport - rows).max(0.0)),
            _ => 0.0,
        };
        if let Some(keep) = self.keeps.get_mut(id) {
            keep.settled = Some(offset);
            keep.anchor = anchor;
            keep.slack = slack;
        }
    }
}
