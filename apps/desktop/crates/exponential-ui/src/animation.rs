//! Round 2 (docs/round-2-contract.md §2, animation): the token-only
//! `animation` style key. `catalog/style.json` `animations` holds the
//! keyframe sets; this module turns one into what a painter needs at a
//! moment — the duration from the theme's motion tokens, per-segment easing,
//! the channel values (opacity, translate, rotate, scale, the shimmer band)
//! — and into CSS `@keyframes`. Mirrors `src/animation.ts`;
//! `fixtures/animations.json` locks the sampled frames.
//!
//! A painter keeps the moment the node entered the tree (first paint,
//! `visible` turning true, a template item appearing) and asks
//! [`animation_frame`] each frame; channels compose OUTSIDE the node's own
//! `transform`, about the box centre; opacity multiplies.

use std::sync::LazyLock;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::generated::catalog as g;
use crate::theme::ResolvedTheme;

/// One keyframe (absent channels = the rest value).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyframe {
    pub offset: f64,
    pub opacity: Option<f64>,
    pub translate_x: Option<f64>,
    pub translate_y: Option<f64>,
    pub rotate: Option<f64>,
    pub scale: Option<f64>,
    pub band: Option<f64>,
}

/// The shimmer band: its colour token and alpha stops across the band.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Band {
    pub color: String,
    pub stops: Vec<BandStop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct BandStop {
    pub offset: f64,
    pub alpha: f64,
}

/// A keyframe set (`style.json animations.<name>`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AnimationDef {
    pub duration: String,
    pub factor: f64,
    pub easing: String,
    /// A count or `"infinite"`.
    pub iterations: Value,
    /// The reduced-motion keyframe offset (0 = first, 1 = last).
    pub rest: f64,
    pub keyframes: Vec<Keyframe>,
    #[serde(default)]
    pub band: Option<Band>,
}

/// The keyframe sets by name, in `style.json` order.
pub static ANIMATIONS: LazyLock<IndexMap<String, AnimationDef>> = LazyLock::new(|| {
    let style: Value = serde_json::from_str(g::STYLE_JSON).expect("style.json");
    style["animations"]
        .as_object()
        .map(|m| m.iter().filter(|(k, _)| !k.starts_with('$')).map(|(k, v)| (k.clone(), serde_json::from_value(v.clone()).expect("animation"))).collect())
        .unwrap_or_default()
});

/// The names (`pulse spin fade-in slide-in-up/down/left/right shimmer`).
pub const ANIMATION_NAMES: &[&str] = g::ANIMATION_NAMES;

/// An easing: a cubic bezier `[x1, y1, x2, y2]` or linear.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnimationEasing {
    Bezier([f64; 4]),
    Linear(LinearTag),
}

/// The literal `"linear"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinearTag {
    Linear,
}

impl AnimationEasing {
    pub const LINEAR: AnimationEasing = AnimationEasing::Linear(LinearTag::Linear);
}

/// Iterations: a count or infinite.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Iterations {
    Count(f64),
    Infinite(InfiniteTag),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfiniteTag {
    Infinite,
}

impl Iterations {
    pub const INFINITE: Iterations = Iterations::Infinite(InfiniteTag::Infinite);

    pub fn count(self) -> f64 {
        match self {
            Iterations::Count(n) => n,
            Iterations::Infinite(_) => f64::INFINITY,
        }
    }
}

/// Duration and easing of an animation under a theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTiming {
    pub duration_ms: f64,
    pub easing: AnimationEasing,
    pub iterations: Iterations,
}

/// One frame: every channel at its value (`band` `None` = no shimmer band).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationFrame {
    pub opacity: f64,
    pub translate_x: f64,
    pub translate_y: f64,
    pub rotate: f64,
    pub scale: f64,
    pub band: Option<f64>,
}

pub const REST: AnimationFrame = AnimationFrame { opacity: 1.0, translate_x: 0.0, translate_y: 0.0, rotate: 0.0, scale: 1.0, band: None };

fn token(s: &str) -> Option<(&str, &str)> {
    s.strip_prefix('$')?.split_once('.')
}

/// Duration and easing of `name` under `theme`: the `duration` token (or
/// `duration_token`, the node's `animationDuration`) × `factor`.
pub fn animation_timing(name: &str, theme: &ResolvedTheme, duration_token: Option<&str>) -> Option<AnimationTiming> {
    let def = ANIMATIONS.get(name)?;
    let motion = &theme.tokens.motion;
    let base = token(duration_token.unwrap_or(&def.duration)).filter(|(g, _)| *g == "motion").and_then(|(_, n)| motion.get(n).copied());
    let fallback = token(&def.duration).and_then(|(_, n)| motion.get(n).copied());
    let ms = base.or(fallback).unwrap_or(0.0) * def.factor;
    let easing = match token(&def.easing) {
        Some(("ease", n)) if def.easing != "linear" => theme.tokens.ease.get(n).filter(|c| c.len() == 4).map(|c| AnimationEasing::Bezier([c[0], c[1], c[2], c[3]])).unwrap_or(AnimationEasing::LINEAR),
        _ => AnimationEasing::LINEAR,
    };
    let iterations = match &def.iterations {
        Value::Number(n) => Iterations::Count(n.as_f64().unwrap_or(1.0)),
        _ => Iterations::INFINITE,
    };
    Some(AnimationTiming { duration_ms: ms, easing, iterations })
}

/// [`animation_timing`] with the node's `animationDuration` already
/// resolved to ms (`$motion.<name>` → its value): it replaces the set's
/// duration token, the factor is kept.
pub fn animation_timing_ms(name: &str, theme: &ResolvedTheme, duration_ms: Option<f64>) -> Option<AnimationTiming> {
    let mut t = animation_timing(name, theme, None)?;
    if let Some(ms) = duration_ms.filter(|m| m.is_finite()) {
        t.duration_ms = ms * ANIMATIONS.get(name)?.factor;
    }
    Some(t)
}

/// The frame of `name` under a RESOLVED timing (what a painter keeps from
/// [`crate::style::Visual::animation`]; no theme needed).
pub fn frame_with_timing(name: &str, timing: &AnimationTiming, elapsed_ms: f64, reduced_motion: bool) -> Option<AnimationFrame> {
    let def = ANIMATIONS.get(name)?;
    if reduced_motion {
        return Some(AnimationFrame { band: None, ..frame_at(def, def.rest, &AnimationEasing::LINEAR) });
    }
    let d = timing.duration_ms;
    if d <= 0.0 {
        return Some(frame_at(def, 1.0, &AnimationEasing::LINEAR));
    }
    let t = elapsed_ms.max(0.0);
    if t >= d * timing.iterations.count() {
        return Some(frame_at(def, 1.0, &timing.easing));
    }
    Some(frame_at(def, (t % d) / d, &timing.easing))
}

/// y of a CSS cubic-bezier at x (bisection on x, 60 steps: deterministic).
pub fn cubic_bezier(curve: [f64; 4], x: f64) -> f64 {
    let [x1, y1, x2, y2] = curve;
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bx = |t: f64| 3.0 * x1 * t * (1.0 - t).powi(2) + 3.0 * x2 * t * t * (1.0 - t) + t.powi(3);
    let by = |t: f64| 3.0 * y1 * t * (1.0 - t).powi(2) + 3.0 * y2 * t * t * (1.0 - t) + t.powi(3);
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if bx(mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    by((lo + hi) / 2.0)
}

fn r4(v: f64) -> f64 {
    let r = (v * 1e4).round() / 1e4;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

fn frame_at(def: &AnimationDef, progress: f64, easing: &AnimationEasing) -> AnimationFrame {
    let ks = &def.keyframes;
    let mut k = 0;
    while k + 2 < ks.len() && progress > ks[k + 1].offset {
        k += 1;
    }
    let a = ks[k];
    let b = ks[(k + 1).min(ks.len() - 1)];
    let span = b.offset - a.offset;
    let local = if span > 0.0 { ((progress - a.offset) / span).clamp(0.0, 1.0) } else { 1.0 };
    let t = match easing {
        AnimationEasing::Bezier(c) => cubic_bezier(*c, local),
        AnimationEasing::Linear(_) => local,
    };
    let lerp = |va: Option<f64>, vb: Option<f64>, rest: f64| {
        let (va, vb) = (va.unwrap_or(rest), vb.unwrap_or(rest));
        r4(va + (vb - va) * t)
    };
    AnimationFrame {
        opacity: lerp(a.opacity, b.opacity, REST.opacity),
        translate_x: lerp(a.translate_x, b.translate_x, REST.translate_x),
        translate_y: lerp(a.translate_y, b.translate_y, REST.translate_y),
        rotate: lerp(a.rotate, b.rotate, REST.rotate),
        scale: lerp(a.scale, b.scale, REST.scale),
        band: match (a.band, b.band) {
            (Some(x), Some(y)) => Some(r4(x + (y - x) * t)),
            _ => None,
        },
    }
}

/// The frame `elapsed_ms` after the node entered the tree. Finished finite
/// animations hold their last keyframe; `reduced_motion` = the `rest`
/// keyframe, no band.
pub fn animation_frame(name: &str, elapsed_ms: f64, theme: &ResolvedTheme, reduced_motion: bool, duration_token: Option<&str>) -> Option<AnimationFrame> {
    let timing = animation_timing(name, theme, duration_token)?;
    frame_with_timing(name, &timing, elapsed_ms, reduced_motion)
}

/// Whether the animation still moves at `elapsed_ms` (a painter stops
/// requesting frames once a finite one finished).
pub fn animation_running(name: &str, elapsed_ms: f64, theme: &ResolvedTheme, reduced_motion: bool, duration_token: Option<&str>) -> bool {
    match animation_timing(name, theme, duration_token) {
        Some(t) if !reduced_motion && t.duration_ms > 0.0 => elapsed_ms < t.duration_ms * t.iterations.count(),
        _ => false,
    }
}

fn css(n: f64) -> String {
    crate::json::number_to_string(r4(n))
}

/// The painted opacity of an animating node: its own (default 1) × the
/// frame's, never replaced (a finished fade-in keeps the author's
/// opacity). `paintedOpacity`.
pub fn painted_opacity(own: Option<f64>, frame: Option<&AnimationFrame>) -> f64 {
    r4(own.unwrap_or(1.0) * frame.map(|f| f.opacity).unwrap_or(1.0))
}

/// The custom properties the keyframes animate, registered so they
/// interpolate (`--xui-a-opacity`, the opacity factor; `--xui-band`, the
/// shimmer band). A CSS renderer adds this ONCE per document.
pub const ANIMATION_PROPERTIES_CSS: &str = r#"@property --xui-a-opacity{syntax:"<number>";inherits:false;initial-value:1}@property --xui-band{syntax:"<number>";inherits:true;initial-value:1}"#;

/// Whether a keyframe set animates opacity (`animatesOpacity`).
pub fn animates_opacity(name: &str) -> bool {
    ANIMATIONS.get(name).is_some_and(|d| d.keyframes.iter().any(|k| k.opacity.is_some()))
}

/// The CSS `@keyframes xui-<name>` rule (`keyframesCss`): opacity moves
/// `--xui-a-opacity` (the node's `opacity` multiplies it), the shimmer band
/// `--xui-band`; needs [`ANIMATION_PROPERTIES_CSS`].
pub fn keyframes_css(name: &str) -> String {
    let Some(def) = ANIMATIONS.get(name) else { return String::new() };
    let steps: Vec<String> = def
        .keyframes
        .iter()
        .map(|k| {
            let mut decl = Vec::new();
            if let Some(o) = k.opacity {
                decl.push(format!("--xui-a-opacity:{}", css(o)));
            }
            if k.translate_x.is_some() || k.translate_y.is_some() {
                decl.push(format!("translate:{}px {}px", css(k.translate_x.unwrap_or(0.0)), css(k.translate_y.unwrap_or(0.0))));
            }
            if let Some(r) = k.rotate {
                decl.push(format!("rotate:{}deg", css(r)));
            }
            if let Some(s) = k.scale {
                decl.push(format!("scale:{}", css(s)));
            }
            if let Some(b) = k.band {
                decl.push(format!("--xui-band:{}", css(b)));
            }
            format!("{}%{{{}}}", css(k.offset * 100.0), decl.join(";"))
        })
        .collect();
    format!("@keyframes xui-{name}{{{}}}", steps.join(""))
}
