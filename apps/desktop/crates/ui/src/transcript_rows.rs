//! EXP-776 — the steering transcript's cached ROW PROJECTION and the
//! bookkeeping that keeps a virtualised `gpui::list` in step with it.
//!
//! The transcript renders through [`gpui::ListState`], which knows rows only
//! by INDEX and by the heights it measured. The view owns the truth (the
//! feed's [`FeedRowSpec`]s), so every frame it derives one [`RowKey`] per row
//! and hands the DIFFERENCE against last frame's keys to the list as
//! [`ListOp`]s: appended rows are spliced in, rows whose content changed are
//! re-measured, rows the feed cap evicted are spliced out. Everything here is
//! pure and window-free, so the edge cases (a trimmed prefix, a tool run that
//! grew, the staged-replay swap, the synthetic "Working…" row) are unit
//! tests rather than something a reader discovers by scrolling.

use std::hash::{Hash, Hasher as _};
use std::ops::Range;

use steer::{FeedItem, FeedItemId, FeedKind, FeedRowSpec};

/// The synthetic trailing "Working…" row: not a feed item, so it wears an id
/// no feed item can ([`FeedItemId`]s count up from zero).
pub(crate) const WORKING_ROW_ID: FeedItemId = u64::MAX;

/// One list row's identity plus a cheap HEIGHT HEURISTIC over its content.
///
/// `id` is the feed row's stable key ([`FeedRowSpec::id`]); `fingerprint`
/// folds in whatever is likely to change the row's rendered height (text
/// lengths, item counts, answered/expanded states). It is deliberately NOT a
/// content identity: two rows with equal fingerprints may differ, and the
/// list re-renders every VISIBLE row each layout anyway — a mismatch only
/// costs an off-screen row its cached height, which the list recovers by
/// measuring it again when it comes back into the overdraw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowKey {
    pub id: FeedItemId,
    pub fingerprint: u64,
}

impl RowKey {
    /// The "Working…" row's key. Its content never changes, so its
    /// fingerprint is a constant.
    pub(crate) const WORKING: RowKey = RowKey {
        id: WORKING_ROW_ID,
        fingerprint: 0,
    };
}

/// The view-side facets of ONE item that move its rendered height and live
/// outside the feed (the view's expanded/answer/extras state). A bit set,
/// folded into the row's fingerprint verbatim.
pub(crate) type ItemFacets = u32;

/// [`ItemFacets`] bits.
pub(crate) mod facet {
    /// The long body is unfolded ("Show less").
    pub const BODY_EXPANDED: u32 = 1 << 0;
    /// The item has local extras cards (a diff, a terminal) hanging off it.
    pub const HAS_EXTRAS: u32 = 1 << 1;
    /// …and they are unfolded.
    pub const EXTRAS_EXPANDED: u32 = 1 << 2;
    /// The question card's answer is in flight (renders its labels, locked).
    pub const ANSWER_LOCKED: u32 = 1 << 3;
    /// The card's free-text row is open.
    pub const FREE_TEXT_OPEN: u32 = 1 << 4;
}

/// The height heuristic behind [`RowKey::fingerprint`]. `group_expanded` is
/// the view's expanded flag for the row itself (a tool run or subagent
/// group); `item_facets` yields each member item's view-side bits.
pub(crate) fn row_fingerprint(
    spec: &FeedRowSpec,
    items: &[FeedItem],
    group_expanded: bool,
    item_facets: impl Fn(&FeedItem) -> ItemFacets,
) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    std::mem::discriminant(spec).hash(&mut hasher);
    spec.id().hash(&mut hasher);
    group_expanded.hash(&mut hasher);
    let indices = spec.item_indices();
    indices.len().hash(&mut hasher);
    for &ix in indices {
        let Some(item) = items.get(ix) else {
            continue;
        };
        item.id.hash(&mut hasher);
        item_facets(item).hash(&mut hasher);
        std::mem::discriminant(&item.kind).hash(&mut hasher);
        match &item.kind {
            FeedKind::Narration { text, .. } | FeedKind::UserMessage { text, .. } => {
                text.len().hash(&mut hasher);
            }
            FeedKind::Tool { name, detail, .. } => {
                name.len().hash(&mut hasher);
                detail.as_ref().map(String::len).hash(&mut hasher);
            }
            FeedKind::Permission { tool, detail } => {
                tool.len().hash(&mut hasher);
                detail.as_ref().map(String::len).hash(&mut hasher);
            }
            FeedKind::Subagent {
                status,
                detail,
                tool_calls,
                ..
            } => {
                (*status as u8).hash(&mut hasher);
                detail.as_ref().map(String::len).hash(&mut hasher);
                tool_calls.hash(&mut hasher);
            }
            FeedKind::Question(card) => {
                card.text.len().hash(&mut hasher);
                card.options.len().hash(&mut hasher);
                card.resolved.hash(&mut hasher);
                card.dismissed.hash(&mut hasher);
                card.answer.as_ref().map(String::len).hash(&mut hasher);
                card.header.as_ref().map(String::len).hash(&mut hasher);
            }
            FeedKind::Compaction => {}
        }
    }
    hasher.finish()
}

/// What the list must be told to match a new key set (see [`plan_list_sync`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ListOp {
    /// `ListState::splice`: the rows at `range` (indices as they stand when
    /// this op is applied) are replaced by `count` new ones.
    Splice { range: Range<usize>, count: usize },
    /// `ListState::remeasure_items`: the rows at `range` kept their identity
    /// but their content moved.
    Remeasure(Range<usize>),
}

/// The ops that take a list holding `old` to one holding `new`, in the order
/// they must be applied.
///
/// Four regions, front to back: (a) a prefix of `old` the feed cap evicted
/// (ids below `new[0].id`) is spliced out; (b) a prefix of `new` with ids
/// below the surviving `old[0].id` is spliced in (a trim that cut INTO a
/// group re-keys that row to its next item, so the old row goes in (a) and
/// the re-keyed one arrives here, while everything after it still matches);
/// (c) the common id prefix stays, with fingerprint mismatches coalesced
/// into remeasure ranges; (d) whatever remains on either side is one splice
/// — an append, a tail that regrouped, the "Working…" row toggling, or a
/// staged-replay swap (which re-anchors ids, so its unchanged prefix falls
/// into (c)). The "Working…" row's id is above every feed id and never
/// counts in (a)/(b).
pub(crate) fn plan_list_sync(old: &[RowKey], new: &[RowKey]) -> Vec<ListOp> {
    let mut ops = Vec::new();
    // (a) the evicted prefix.
    let trimmed = match new.first() {
        Some(first) => old.iter().take_while(|key| key.id < first.id).count(),
        None => 0,
    };
    if trimmed > 0 {
        ops.push(ListOp::Splice {
            range: 0..trimmed,
            count: 0,
        });
    }
    let old = &old[trimmed..];
    // (b) the re-keyed head.
    let inserted = match old.first() {
        Some(first) if first.id != WORKING_ROW_ID => {
            new.iter().take_while(|key| key.id < first.id).count()
        }
        _ => 0,
    };
    if inserted > 0 {
        ops.push(ListOp::Splice {
            range: 0..0,
            count: inserted,
        });
    }
    let new = &new[inserted..];
    // (c) the common prefix; list indices from here on are offset by the
    // rows (b) put in front.
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(a, b)| a.id == b.id)
        .count();
    let mut pending: Option<Range<usize>> = None;
    for ix in 0..prefix {
        if old[ix].fingerprint == new[ix].fingerprint {
            if let Some(range) = pending.take() {
                ops.push(ListOp::Remeasure(range));
            }
            continue;
        }
        match pending.as_mut() {
            Some(range) => range.end = inserted + ix + 1,
            None => pending = Some(inserted + ix..inserted + ix + 1),
        }
    }
    if let Some(range) = pending {
        ops.push(ListOp::Remeasure(range));
    }
    // (d) the rest.
    if old.len() > prefix || new.len() > prefix {
        ops.push(ListOp::Splice {
            range: inserted + prefix..inserted + old.len(),
            count: new.len() - prefix,
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: u64, fingerprint: u64) -> RowKey {
        RowKey { id, fingerprint }
    }

    fn keys(ids: &[u64]) -> Vec<RowKey> {
        ids.iter().map(|&id| key(id, id)).collect()
    }

    /// The model list the ops are checked against: apply them to `old` and
    /// the result must be `new`, id for id.
    fn apply(old: &[RowKey], new: &[RowKey], ops: &[ListOp]) -> Vec<Option<u64>> {
        // `None` = a spliced-in row the list does not know yet; it gets
        // filled from `new` by position at the end.
        let mut list: Vec<Option<u64>> = old.iter().map(|key| Some(key.id)).collect();
        for op in ops {
            match op {
                ListOp::Splice { range, count } => {
                    list.splice(range.clone(), std::iter::repeat_n(None, *count));
                }
                ListOp::Remeasure(range) => {
                    assert!(range.end <= list.len(), "remeasure past the end: {range:?}");
                    for ix in range.clone() {
                        assert!(list[ix].is_some(), "remeasuring a row not yet spliced in");
                    }
                }
            }
        }
        assert_eq!(list.len(), new.len(), "the ops leave the wrong row count");
        list.iter()
            .enumerate()
            .map(|(ix, id)| id.or(Some(new[ix].id)))
            .collect()
    }

    fn check(old: &[RowKey], new: &[RowKey], ops: &[ListOp]) {
        let result = apply(old, new, ops);
        let expected: Vec<Option<u64>> = new.iter().map(|key| Some(key.id)).collect();
        assert_eq!(result, expected);
    }

    /// EXP-783 — a transcript window extension. The older page arrives ahead
    /// of rows the list already holds, and it must cost exactly one front
    /// splice and NO remeasures: a remeasure of the retained rows is what
    /// would move the reader's anchor while they sit at the top.
    #[test]
    fn a_window_extension_is_one_front_splice_and_no_remeasures() {
        let old = keys(&[500, 501, 502, 503]);
        let new = keys(&[100, 200, 300, 400, 500, 501, 502, 503]);
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 0..0,
                count: 4
            }]
        );
        assert!(!ops.iter().any(|op| matches!(op, ListOp::Remeasure(_))));
        check(&old, &new, &ops);
    }

    #[test]
    fn an_append_is_one_tail_splice() {
        let old = keys(&[1, 2, 3]);
        let new = keys(&[1, 2, 3, 4, 5]);
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 3..3,
                count: 2
            }]
        );
        check(&old, &new, &ops);
    }

    #[test]
    fn nothing_changed_is_no_op() {
        let old = keys(&[1, 2, 3]);
        assert_eq!(plan_list_sync(&old, &old), vec![]);
    }

    /// A tool run's row keeps its id (its first item's) while it grows, so a
    /// second call landing in it is a fingerprint change, not a new row.
    #[test]
    fn a_tail_regroup_remeasures_the_row_that_grew() {
        let old = vec![key(1, 10), key(2, 20)];
        let new = vec![key(1, 10), key(2, 21)];
        let ops = plan_list_sync(&old, &new);
        assert_eq!(ops, vec![ListOp::Remeasure(1..2)]);
        check(&old, &new, &ops);
        // …and a growing run followed by an append is both, in order.
        let new = vec![key(1, 10), key(2, 21), key(4, 40)];
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![
                ListOp::Remeasure(1..2),
                ListOp::Splice {
                    range: 2..2,
                    count: 1
                }
            ]
        );
        check(&old, &new, &ops);
    }

    #[test]
    fn adjacent_fingerprint_changes_coalesce_into_one_remeasure() {
        let old = vec![key(1, 10), key(2, 20), key(3, 30), key(4, 40), key(5, 50)];
        let new = vec![key(1, 11), key(2, 21), key(3, 30), key(4, 41), key(5, 50)];
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![ListOp::Remeasure(0..2), ListOp::Remeasure(3..4)]
        );
        check(&old, &new, &ops);
    }

    /// The feed cap evicted the oldest items: their rows leave the front.
    #[test]
    fn a_feed_cap_trim_splices_the_evicted_prefix_out() {
        let old = keys(&[1, 2, 3, 4]);
        let new = keys(&[3, 4, 5]);
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![
                ListOp::Splice {
                    range: 0..2,
                    count: 0
                },
                ListOp::Splice {
                    range: 2..2,
                    count: 1
                }
            ]
        );
        check(&old, &new, &ops);
    }

    /// A trim that cuts INTO a group: the run keyed 5 (items 5,6,7) loses
    /// item 5 and is now keyed 6 — the old row goes, the new one comes.
    #[test]
    fn a_trim_that_rekeys_the_first_row_replaces_it() {
        let old = vec![key(5, 50), key(8, 80)];
        let new = vec![key(6, 60), key(8, 80)];
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![
                ListOp::Splice {
                    range: 0..1,
                    count: 0
                },
                ListOp::Splice {
                    range: 0..0,
                    count: 1
                }
            ]
        );
        check(&old, &new, &ops);
        // The rows behind the re-keyed head keep their measured heights, and
        // a change among them is still a remeasure at its SHIFTED index.
        let old = vec![key(5, 50), key(8, 80), key(9, 90)];
        let new = vec![key(6, 60), key(8, 81), key(9, 90), key(11, 110)];
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![
                ListOp::Splice {
                    range: 0..1,
                    count: 0
                },
                ListOp::Splice {
                    range: 0..0,
                    count: 1
                },
                ListOp::Remeasure(1..2),
                ListOp::Splice {
                    range: 3..3,
                    count: 1
                }
            ]
        );
        check(&old, &new, &ops);
    }

    /// The EXP-656 swap re-anchors ids, so the replayed prefix matches the
    /// rows already on screen and only the tail is replaced.
    #[test]
    fn a_staged_swap_with_re_anchored_ids_keeps_the_prefix() {
        let old = vec![key(1, 10), key(2, 20), key(3, 30)];
        let new = vec![key(1, 10), key(2, 20), key(3, 31), key(4, 40), key(6, 60)];
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![
                ListOp::Remeasure(2..3),
                ListOp::Splice {
                    range: 3..3,
                    count: 2
                }
            ]
        );
        check(&old, &new, &ops);
        // A swap that regrouped the tail differently replaces from the first
        // id that disagrees.
        let new = vec![key(1, 10), key(2, 20), key(5, 50)];
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 2..3,
                count: 1
            }]
        );
        check(&old, &new, &ops);
    }

    #[test]
    fn the_working_row_toggles_as_a_tail_splice() {
        let rows = keys(&[1, 2]);
        let mut with_working = rows.clone();
        with_working.push(RowKey::WORKING);
        // On.
        let ops = plan_list_sync(&rows, &with_working);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 2..2,
                count: 1
            }]
        );
        check(&rows, &with_working, &ops);
        // Off.
        let ops = plan_list_sync(&with_working, &rows);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 2..3,
                count: 0
            }]
        );
        check(&with_working, &rows, &ops);
        // A row appended UNDER it: the working row is replaced along with
        // the new row (it sits last again).
        let mut appended = keys(&[1, 2, 3]);
        appended.push(RowKey::WORKING);
        let ops = plan_list_sync(&with_working, &appended);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 2..3,
                count: 2
            }]
        );
        check(&with_working, &appended, &ops);
        // The working row never counts as an "evicted" prefix, whatever the
        // first real id is.
        let only_working = vec![RowKey::WORKING];
        let ops = plan_list_sync(&only_working, &keys(&[7]));
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 0..1,
                count: 1
            }]
        );
    }

    #[test]
    fn empty_to_rows_and_back() {
        let rows = keys(&[1, 2, 3]);
        let ops = plan_list_sync(&[], &rows);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 0..0,
                count: 3
            }]
        );
        check(&[], &rows, &ops);
        let ops = plan_list_sync(&rows, &[]);
        assert_eq!(
            ops,
            vec![ListOp::Splice {
                range: 0..3,
                count: 0
            }]
        );
        check(&rows, &[], &ops);
        assert_eq!(plan_list_sync(&[], &[]), vec![]);
    }

    /// A feed that restarted from lower ids (a reset with no anchor) is a
    /// full replace — the new rows come in ahead, the old ones all go —
    /// never a partial match on position.
    #[test]
    fn a_feed_that_restarted_its_ids_is_replaced_whole() {
        let old = keys(&[10, 11, 12]);
        let new = keys(&[3, 4]);
        let ops = plan_list_sync(&old, &new);
        assert_eq!(
            ops,
            vec![
                ListOp::Splice {
                    range: 0..0,
                    count: 2
                },
                ListOp::Splice {
                    range: 2..5,
                    count: 0
                }
            ]
        );
        check(&old, &new, &ops);
    }

    // ── row_fingerprint ────────────────────────────────────────────────────

    fn narration(id: u64, text: &str) -> FeedItem {
        FeedItem {
            id,
            kind: FeedKind::Narration {
                text: text.to_string(),
                message_id: None,
                subagent_id: None,
            },
            seq: None,
        }
    }

    fn tool(id: u64, name: &str) -> FeedItem {
        FeedItem {
            id,
            kind: FeedKind::Tool {
                name: name.to_string(),
                detail: None,
                subagent_id: None,
            },
            seq: None,
        }
    }

    #[test]
    fn the_fingerprint_moves_with_height_relevant_facets_only() {
        let items = vec![narration(1, "hello"), tool(2, "Read"), tool(3, "Edit")];
        let single = FeedRowSpec::Single { id: 1, item: 0 };
        let base = row_fingerprint(&single, &items, false, |_| 0);
        assert_eq!(base, row_fingerprint(&single, &items, false, |_| 0));
        // Streaming text grows: the row must be re-measured.
        let grown = vec![narration(1, "hello world"), tool(2, "Read"), tool(3, "Edit")];
        assert_ne!(base, row_fingerprint(&single, &grown, false, |_| 0));
        // The view unfolding the body counts too.
        assert_ne!(
            base,
            row_fingerprint(&single, &items, false, |_| facet::BODY_EXPANDED)
        );
        // A group: one more call in the run, or expanding it.
        let run = FeedRowSpec::ToolRun {
            id: 2,
            items: vec![1, 2],
        };
        let run_base = row_fingerprint(&run, &items, false, |_| 0);
        let shorter = FeedRowSpec::ToolRun {
            id: 2,
            items: vec![1],
        };
        assert_ne!(run_base, row_fingerprint(&shorter, &items, false, |_| 0));
        assert_ne!(run_base, row_fingerprint(&run, &items, true, |_| 0));
        // Same content, same key: a frame with no change plans no ops.
        assert_eq!(run_base, row_fingerprint(&run, &items, false, |_| 0));
    }
}
