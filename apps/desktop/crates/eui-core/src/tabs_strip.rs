//! VAPP-279 (docs/round-7-contract.md, fixtures/tabs-strip.json): the pure
//! rules of a `variant: strip` Tabs, the twins of
//! `packages/exponential-ui/src/tabs-strip.ts` (same vectors): which tabs
//! keep an item (the rest fold into `+N`), where a dragged tab lands, the
//! keyboard move, and the `reorder` payload.

/// Which tabs keep an item, in order (the IDE's work-tab rule, EXP-1334):
/// tabs pack in order until the next would not fit; the `+N` width (and
/// its gap) is reserved only once something overflows; the ACTIVE tab is
/// committed first so it always shows; one tab always survives.
pub fn partition_tabs(widths: &[f32], available: f32, gap: f32, overflow_w: f32, active: Option<usize>) -> Vec<usize> {
    let count = widths.len();
    let room = available.max(0.0);
    let total: f32 = widths.iter().sum::<f32>() + gap * count.saturating_sub(1) as f32;
    if total <= room {
        return (0..count).collect();
    }
    let budget = (room - overflow_w - gap).max(0.0);
    let active = active.filter(|&a| a < count);
    let mut visible = Vec::new();
    let mut used = active.map(|a| widths[a]).unwrap_or(0.0);
    let mut tabs = usize::from(active.is_some());
    for (index, &width) in widths.iter().enumerate() {
        if Some(index) == active {
            visible.push(index);
            continue;
        }
        let next = used + width + if tabs > 0 { gap } else { 0.0 };
        if next > budget && tabs > 0 {
            break;
        }
        visible.push(index);
        used = next;
        tabs += 1;
    }
    if let Some(a) = active {
        if !visible.contains(&a) {
            visible.push(a);
        }
    }
    if visible.is_empty() && count > 0 {
        visible.push(0);
    }
    visible.sort_unstable();
    visible
}

/// Where a dragged tab lands: `centers` = each VISIBLE item's centre x
/// (screen px, strip order), `from` = the dragged slot, `delta` = px moved
/// since the drag started (screen direction). The slot = how many other
/// centres the dragged centre passed in reading order.
pub fn tab_drop_slot(centers: &[f32], from: usize, delta: f32, rtl: bool) -> usize {
    let Some(&start) = centers.get(from) else { return from };
    let at = start + delta;
    centers.iter().enumerate().filter(|&(k, &c)| k != from && if rtl { c > at } else { c < at }).count()
}

/// Ctrl/Cmd+Shift+`key` on slot `from` of `count`: one place in screen
/// direction; `None` past an end or for any other key.
pub fn tab_key_slot(count: usize, from: usize, key: &str, rtl: bool) -> Option<usize> {
    let step: i64 = match key {
        "ArrowRight" => 1,
        "ArrowLeft" => -1,
        _ => return None,
    };
    let to = from as i64 + if rtl { -step } else { step };
    (to >= 0 && (to as usize) < count).then_some(to as usize)
}

/// A slot move → the `reorder {from, to}` payload over the `tabs` array
/// (`visible` = the array index of each slot); `None` = no move.
pub fn tab_reorder(visible: &[usize], from_slot: usize, to_slot: usize) -> Option<(usize, usize)> {
    let from = *visible.get(from_slot)?;
    let to = *visible.get(to_slot)?;
    (from != to).then_some((from, to))
}

/// `order` with the item at `from` moved to `to` (the live preview).
pub fn move_tab<T: Clone>(order: &[T], from: usize, to: usize) -> Vec<T> {
    let mut out = order.to_vec();
    if from < out.len() && to < out.len() {
        let moved = out.remove(from);
        out.insert(to, moved);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_tab_is_the_preview_order() {
        assert_eq!(move_tab(&[0, 1, 2, 3], 0, 2), vec![1, 2, 0, 3]);
        assert_eq!(move_tab(&[0, 1, 2, 3], 3, 0), vec![3, 0, 1, 2]);
        assert_eq!(move_tab(&[0, 1], 0, 5), vec![0, 1]);
    }
}
