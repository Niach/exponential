//! Group D (genUI round 4): the Ring arcs (`sweep`, `rings`) and the
//! AudioPlayer waveform (`peaks`), the TS reference's src/data-viz.ts;
//! `fixtures/data-viz.json` locks both.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One drawn ring: centre-line radius, filled fraction, tone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingArc {
    pub r: f64,
    pub fraction: f64,
    pub tone: String,
}

/// Every ring's arc: `start`/`sweep` in degrees clockwise from 12 o'clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingArcs {
    pub start: f64,
    pub sweep: f64,
    pub rings: Vec<RingArc>,
}

/// The waveform's locked bar geometry (px): bar width, gap, min height, box height.
pub const WAVEFORM_BAR_WIDTH: f64 = 2.0;
pub const WAVEFORM_GAP: f64 = 2.0;
pub const WAVEFORM_MIN_HEIGHT: f64 = 2.0;
pub const WAVEFORM_HEIGHT: f64 = 24.0;

/// One waveform bar in the WAVEFORM_HEIGHT box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaveBar {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub played: bool,
}

/// JS `Math.round`.
fn js_round(n: f64) -> f64 {
    (n + 0.5).floor()
}

fn round2(n: f64) -> f64 {
    js_round(n * 100.0) / 100.0
}

fn finite(v: Option<&Value>, d: f64) -> f64 {
    v.and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(d)
}

fn tone_of(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// The Ring's arcs (see the TS `ringArcs`): `sweep` 360 starts at the top,
/// less is centred on the top; `rings` outermost first (value / max, max 1)
/// else the one `value`; ring i's radius = size/2 - stroke/2 - 2i·stroke,
/// dropped under `stroke`.
pub fn ring_arcs(size: f64, stroke: f64, value: Option<&Value>, tone: Option<&Value>, sweep: Option<&Value>, rings: Option<&Value>) -> RingArcs {
    let sweep = finite(sweep, 360.0).clamp(0.0, 360.0);
    let start = if sweep >= 360.0 { 0.0 } else { 180.0 + (360.0 - sweep) / 2.0 };
    let tone = tone_of(tone).unwrap_or_else(|| "primary".into());
    let single = [serde_json::json!({ "value": value.cloned().unwrap_or(Value::Null), "max": 1 })];
    let list: &[Value] = match rings.and_then(Value::as_array) {
        Some(items) if !items.is_empty() => items,
        _ => &single,
    };
    let mut out = Vec::new();
    for (i, item) in list.iter().enumerate() {
        let r = size / 2.0 - stroke / 2.0 - i as f64 * stroke * 2.0;
        if r < stroke {
            continue;
        }
        let max = finite(item.get("max"), 1.0);
        let fraction = if max > 0.0 { (finite(item.get("value"), 0.0) / max).clamp(0.0, 1.0) } else { 0.0 };
        out.push(RingArc { r: round2(r), fraction: js_round(fraction * 10000.0) / 10000.0, tone: tone_of(item.get("tone")).unwrap_or_else(|| tone.clone()) });
    }
    RingArcs { start: round2(start), sweep: round2(sweep), rings: out }
}

/// The AudioPlayer waveform (see the TS `waveformBars`).
pub fn waveform_bars(width: f64, peaks: Option<&Value>, progress: Option<f64>) -> Vec<WaveBar> {
    let peaks: Vec<f64> = peaks.and_then(Value::as_array).map(|a| a.iter().map(|p| finite(Some(p), 0.0).clamp(0.0, 1.0)).collect()).unwrap_or_default();
    if peaks.is_empty() {
        return Vec::new();
    }
    let n = ((width + WAVEFORM_GAP) / (WAVEFORM_BAR_WIDTH + WAVEFORM_GAP)).floor().max(0.0) as usize;
    if n == 0 {
        return Vec::new();
    }
    let len = peaks.len();
    let x0 = (width - (n as f64 * (WAVEFORM_BAR_WIDTH + WAVEFORM_GAP) - WAVEFORM_GAP)) / 2.0;
    let at = progress.filter(|p| p.is_finite()).unwrap_or(0.0).clamp(0.0, 1.0) * width;
    (0..n)
        .map(|i| {
            let peak = if len > n {
                let from = i * len / n;
                let to = ((i + 1) * len / n).max(from + 1);
                peaks[from..to].iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            } else {
                peaks[i * len / n]
            };
            let h = (peak * WAVEFORM_HEIGHT).max(WAVEFORM_MIN_HEIGHT);
            let x = x0 + i as f64 * (WAVEFORM_BAR_WIDTH + WAVEFORM_GAP);
            WaveBar { x: round2(x), y: round2((WAVEFORM_HEIGHT - h) / 2.0), width: WAVEFORM_BAR_WIDTH, height: round2(h), played: at > 0.0 && x + WAVEFORM_BAR_WIDTH / 2.0 <= at }
        })
        .collect()
}

/// A press or drag at `x` on a `width` track → the seek fraction 0..1.
pub fn seek_fraction(x: f64, width: f64) -> f64 {
    if width > 0.0 {
        (x / width).clamp(0.0, 1.0)
    } else {
        0.0
    }
}
