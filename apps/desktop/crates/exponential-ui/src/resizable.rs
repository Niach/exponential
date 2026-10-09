//! Round 2 (docs/round-2-contract.md §1, Resizable): the panel arithmetic
//! every renderer shares, so a drag or a key press gives the same sizes
//! everywhere. Sizes are PERCENTAGES of the group's main axis minus its
//! handles (they sum to 100); only the two panels beside the moved handle
//! change. Results are rounded to 1e-6. Mirrors `src/resizable.ts`;
//! `fixtures/resizable.json` locks it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::layout::{PANEL_MIN, RESIZE_HANDLE_HIT, RESIZE_STEP};

/// One panel's limits (`Resizable.panels[i]`), percent.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PanelLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Dragging below half its `min` collapses it to 0; Enter toggles.
    #[serde(default)]
    pub collapsible: bool,
}

impl PanelLimits {
    /// `panels` as authored (a JSON array of `{min?, max?, collapsible?}`).
    pub fn list(v: Option<&Value>) -> Vec<PanelLimits> {
        v.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|p| PanelLimits {
                        min: p.get("min").and_then(Value::as_f64),
                        max: p.get("max").and_then(Value::as_f64),
                        collapsible: p.get("collapsible") == Some(&Value::Bool(true)),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// The panel group's axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

impl Orientation {
    pub fn parse(s: &str) -> Option<Orientation> {
        match s {
            "horizontal" => Some(Orientation::Horizontal),
            "vertical" => Some(Orientation::Vertical),
            _ => None,
        }
    }
}

const EPS: f64 = 1e-9;

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.min(hi).max(lo)
}

/// Rounded to 1e-6 (every platform prints the same numbers), `-0` → 0.
fn tidy(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

struct Lim {
    min: f64,
    max: f64,
    collapsible: bool,
}

fn limits_of(limits: &[PanelLimits], i: usize, count: usize) -> Lim {
    let l = limits.get(i).copied().unwrap_or_default();
    let cap = 100.0 / count.max(1) as f64;
    let min = clamp(l.min.filter(|m| m.is_finite()).unwrap_or(PANEL_MIN), 0.0, cap);
    let max = clamp(l.max.filter(|m| m.is_finite()).unwrap_or(100.0), min, 100.0);
    Lim { min, max, collapsible: l.collapsible }
}

/// The sizes a group of `count` panels starts with: the valid given sizes
/// (finite, ≥ 0), the missing ones sharing what is left of 100 (equal split
/// when none is given), scaled to sum 100, each clamped into its [min, max]
/// (a collapsible panel may stay 0) with the difference handed to the
/// panels with room, in order. A `min` above 100 / count is lowered to it.
pub fn normalize_sizes(sizes: Option<&[Value]>, count: usize, limits: &[PanelLimits]) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let given: Vec<Option<f64>> = (0..count).map(|i| sizes.and_then(|s| s.get(i)).and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.0)).collect();
    let missing = given.iter().filter(|v| v.is_none()).count();
    let mut out: Vec<f64> = if missing == count {
        vec![100.0 / count as f64; count]
    } else {
        let sum: f64 = given.iter().map(|v| v.unwrap_or(0.0)).sum();
        let rest = (100.0 - sum).max(0.0);
        given.iter().map(|v| v.unwrap_or(rest / missing as f64)).collect()
    };
    let total: f64 = out.iter().sum();
    out = if total > EPS { out.iter().map(|v| v * 100.0 / total).collect() } else { vec![100.0 / count as f64; count] };
    let lim: Vec<Lim> = (0..count).map(|i| limits_of(limits, i, count)).collect();
    for (i, v) in out.iter_mut().enumerate() {
        *v = if lim[i].collapsible && *v <= EPS { 0.0 } else { clamp(*v, lim[i].min, lim[i].max) };
    }
    let mut diff = 100.0 - out.iter().sum::<f64>();
    for i in 0..count {
        if diff.abs() <= EPS {
            break;
        }
        if lim[i].collapsible && out[i] == 0.0 {
            continue;
        }
        let room = if diff > 0.0 { lim[i].max - out[i] } else { lim[i].min - out[i] };
        let take = if diff > 0.0 { diff.min(room) } else { diff.max(room) };
        out[i] += take;
        diff -= take;
    }
    out.into_iter().map(tidy).collect()
}

/// Move handle `handle` (between panels `handle` and `handle + 1`) by
/// `delta` percentage points (positive = the first panel grows), from the
/// sizes at the START of the drag. Clamped so both panels keep their
/// limits; a collapsible panel dragged below half its min collapses to 0,
/// a collapsed one dragged past half its min reopens at max(min, the
/// pointer), clamped the same way. Other panels never change.
pub fn resize_panels(sizes: &[f64], handle: usize, delta: f64, limits: &[PanelLimits]) -> Vec<f64> {
    let mut out = sizes.to_vec();
    let (a, b) = (handle, handle + 1);
    if b >= sizes.len() || !delta.is_finite() {
        return out.into_iter().map(tidy).collect();
    }
    let la = limits_of(limits, a, sizes.len());
    let lb = limits_of(limits, b, sizes.len());
    let lo = (la.min - sizes[a]).max(sizes[b] - lb.max);
    let hi = (la.max - sizes[a]).min(sizes[b] - lb.min);
    let mut d = clamp(delta, lo.min(0.0), hi.max(0.0));
    if la.collapsible && delta < 0.0 && sizes[a] > 0.0 && sizes[a] + delta < la.min / 2.0 && sizes[b] + sizes[a] <= lb.max + EPS {
        d = -sizes[a];
    } else if lb.collapsible && delta > 0.0 && sizes[b] > 0.0 && sizes[b] - delta < lb.min / 2.0 && sizes[a] + sizes[b] <= la.max + EPS {
        d = sizes[b];
    } else if la.collapsible && sizes[a] == 0.0 {
        // A collapsed panel dragged past half its min reopens at its min at
        // least, then follows the pointer (up to its max and the
        // neighbour's min).
        d = if delta >= la.min / 2.0 && hi >= la.min - EPS { clamp(delta, la.min, hi) } else { 0.0 };
    } else if lb.collapsible && sizes[b] == 0.0 {
        d = if -delta >= lb.min / 2.0 && -lo >= lb.min - EPS { -clamp(-delta, lb.min, -lo) } else { 0.0 };
    }
    out[a] = sizes[a] + d;
    out[b] = sizes[b] - d;
    out.into_iter().map(tidy).collect()
}

/// The keys a focused handle handles ([`keyboard_resize`]); hosts may
/// forward every key, the rest are ignored.
pub const RESIZE_KEYS: [&str; 7] = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "Enter"];

/// A key on a focused handle (`ArrowLeft|ArrowRight|ArrowUp|ArrowDown|Home|
/// End|Enter`). Arrows move the HANDLE on screen by [`RESIZE_STEP`]
/// (horizontal: Left/Right, rtl flips; vertical: Up/Down); Home / End take
/// the first panel to its min / max; Enter collapses the first panel when
/// collapsible (else the second), reopening a collapsed one at its min.
/// Other keys change nothing.
pub fn keyboard_resize(sizes: &[f64], handle: usize, key: &str, orientation: Orientation, rtl: bool, limits: &[PanelLimits]) -> Vec<f64> {
    let same = || sizes.iter().copied().map(tidy).collect::<Vec<_>>();
    let (a, b) = (handle, handle + 1);
    if b >= sizes.len() {
        return same();
    }
    let la = limits_of(limits, a, sizes.len());
    let lb = limits_of(limits, b, sizes.len());
    let flip = if orientation == Orientation::Horizontal && rtl { -1.0 } else { 1.0 };
    let step = |sign: f64| resize_panels(sizes, handle, sign * RESIZE_STEP, limits);
    let horizontal = orientation == Orientation::Horizontal;
    match key {
        "ArrowRight" if horizontal => step(flip),
        "ArrowLeft" if horizontal => step(-flip),
        "ArrowDown" if !horizontal => step(1.0),
        "ArrowUp" if !horizontal => step(-1.0),
        "Home" => resize_panels(sizes, handle, la.min - sizes[a], limits),
        "End" => resize_panels(sizes, handle, la.max - sizes[a], limits),
        "Enter" => {
            let mut out = sizes.to_vec();
            if la.collapsible {
                let d = if sizes[a] > 0.0 { -sizes[a] } else { la.min.min(sizes[b] - lb.min) };
                if sizes[b] - d > lb.max + EPS || d == 0.0 {
                    return same();
                }
                out[a] += d;
                out[b] -= d;
            } else if lb.collapsible {
                let d = if sizes[b] > 0.0 { sizes[b] } else { -lb.min.min(sizes[a] - la.min) };
                if sizes[a] + d > la.max + EPS || d == 0.0 {
                    return same();
                }
                out[a] += d;
                out[b] -= d;
            }
            out.into_iter().map(tidy).collect()
        }
        _ => same(),
    }
}

/// Panel lengths in px on the main axis: the container minus every handle
/// (`handle_extent` each, `$control.hairline`), shared by the sizes.
pub fn panel_extents(sizes: &[f64], container: f64, handle_extent: f64) -> Vec<f64> {
    let avail = (container - sizes.len().saturating_sub(1) as f64 * handle_extent).max(0.0);
    sizes.iter().map(|s| tidy(avail * s / 100.0)).collect()
}

/// A pointer movement (px on the main axis, screen direction) as the
/// percentage delta [`resize_panels`] takes (rtl horizontal groups flip).
pub fn drag_delta(px: f64, container: f64, panel_count: usize, orientation: Orientation, rtl: bool, handle_extent: f64) -> f64 {
    let avail = (container - panel_count.saturating_sub(1) as f64 * handle_extent).max(1.0);
    let flip = if orientation == Orientation::Horizontal && rtl { -1.0 } else { 1.0 };
    tidy(px * 100.0 / avail * flip)
}
