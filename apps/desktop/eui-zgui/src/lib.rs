//! #EDE-279: an eUI renderer on zgui (zortax/zgui), behind eui-core.
//!
//! eui-core stays the engine (reducer, theme, taffy layout, windowed List);
//! this crate is only the MEASURER ([`measure::ZMeasure`]: eui-core's
//! `Measure` answered by zgui's own text stack, parley through
//! `zgui-text-parley`), the PAINTER ([`doc::Doc`]: every `PlacedNode` +
//! frame + resolved `Visual` mirrored as one absolutely positioned zgui
//! element whose inline style is the visual, leaves' content as text /
//! vector pieces) and the WINDOW HOST ([`host`]: headless harness,
//! offscreen wgpu readback, a real winit window).
//!
//! Why a retained element mirror and not a hand-built `zgui_scene::Scene`:
//! the scene's glyph path (atlas tiles, `MonoSprite`s) is only fed by
//! `zgui-paint`'s document walk; a scene built outside it has rectangles
//! and vector items but no text. The mirror keeps every layout decision in
//! eui-core (zgui's layout only places absolute boxes at the core's frames)
//! and lets zgui's paint stage do what it is good at (damage, replay, text).

pub mod bench;
pub mod doc;
pub mod fonts;
pub mod host;
pub mod icons;
pub mod measure;
pub mod paint;
pub mod specimens;

pub use doc::Doc;

/// The repo root (this crate is apps/desktop/eui-zgui).
pub fn repo() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Resident set size in MB (Linux `/proc`, macOS `getrusage` max RSS).
pub fn rss_mb() -> f64 {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|k| k.parse::<f64>().ok()).unwrap_or(0.0) / 1024.0
    }
    #[cfg(not(target_os = "linux"))]
    {
        0.0
    }
}

/// Milliseconds since `$SPIKE_T0` (ns since the epoch, set by the caller
/// right before exec): process start → now.
pub fn since_t0_ms() -> Option<f64> {
    let t0: u128 = std::env::var("SPIKE_T0").ok()?.parse().ok()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some(now.saturating_sub(t0) as f64 / 1e6)
}

/// mean, p95, max, median.
pub fn stats(mut v: Vec<f64>) -> (f64, f64, f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.sort_by(f64::total_cmp);
    let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).min(v.len()) - 1];
    (mean, p95, v[v.len() - 1], v[v.len() / 2])
}
