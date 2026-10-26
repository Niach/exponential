//! NEW-77: the VoiceWave rings (catalog/style.json `voiceWave`), a port of
//! `src/voice-wave.ts`; `fixtures/voice-wave.json` locks it. A painter fills
//! and strokes each ring's closed path with its part colour (`outer`, `mid`,
//! `inner`) at the ring's alphas and centres the child at `child_size`.

use std::f64::consts::PI;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::generated::catalog as g;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceWaveSpec {
    pub steps: usize,
    pub radius: Vec<f64>,
    pub harmonics: Vec<Vec<f64>>,
    pub phase0: Vec<f64>,
    pub ring_amp: Vec<f64>,
    pub amp_base: f64,
    pub amp_level: f64,
    pub thinking_amp: f64,
    pub phase_speed: f64,
    pub fill_alpha: Vec<f64>,
    pub stroke_alpha: Vec<f64>,
    pub stroke_width: Vec<f64>,
    pub child_scale: f64,
    pub child_scale_large: f64,
    pub max_bands: usize,
}

pub static VOICE_WAVE: LazyLock<VoiceWaveSpec> = LazyLock::new(|| {
    let style: serde_json::Value = serde_json::from_str(g::STYLE_JSON).expect("style.json");
    serde_json::from_value(style["voiceWave"].clone()).expect("style.json voiceWave")
});

/// The ring parts, outermost first.
pub const RINGS: [&str; 3] = ["outer", "mid", "inner"];

/// The `activity` prop (catalog enum `voiceWaveActivity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Activity {
    #[default]
    Idle,
    Listening,
    Thinking,
    Speaking,
    Muted,
}

impl Activity {
    pub fn parse(s: &str) -> Self {
        match s {
            "listening" => Self::Listening,
            "thinking" => Self::Thinking,
            "speaking" => Self::Speaking,
            "muted" => Self::Muted,
            _ => Self::Idle,
        }
    }

    /// Whether the wave moves (a painter keeps requesting frames).
    pub fn animates(self) -> bool {
        matches!(self, Self::Listening | Self::Thinking | Self::Speaking)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceWaveInput {
    pub size: f64,
    pub activity: Activity,
    #[serde(default)]
    pub level: Option<f64>,
    #[serde(default)]
    pub bands: Option<Vec<f64>>,
    #[serde(default)]
    pub elapsed: Option<f64>,
    #[serde(default)]
    pub reduced_motion: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceWaveRing {
    pub part: String,
    /// `steps + 1` points in px from the box's top-left; the last repeats the first.
    pub points: Vec<[f64; 2]>,
    pub fill_alpha: f64,
    pub stroke_alpha: f64,
    pub stroke_width: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceWave {
    pub rings: Vec<VoiceWaveRing>,
    /// The centre child's edge in px; 0 = not painted (`sm`).
    pub child_size: f64,
    pub phase: f64,
}

fn clamp01(n: f64) -> f64 {
    if n.is_finite() { n.clamp(0.0, 1.0) } else { 0.0 }
}

fn round2(n: f64) -> f64 {
    // JS Math.round (half up): every value here is >= 0 except a stray
    // negative coordinate, where floor(x + 0.5) keeps the two equal.
    (n * 100.0 + 0.5).floor() / 100.0
}

/// The phase (radians) a painter draws at `elapsed` seconds.
pub fn phase(activity: Activity, elapsed: f64, reduced_motion: bool) -> f64 {
    if reduced_motion || !activity.animates() {
        return 0.0;
    }
    let e = if elapsed.is_finite() { elapsed.max(0.0) } else { 0.0 };
    e * VOICE_WAVE.phase_speed
}

/// The child's edge for a box `size` (sm 32 · md 56 · lg 160 · xl 200).
pub fn child_size(size: f64) -> f64 {
    let v = &*VOICE_WAVE;
    if size < 40.0 {
        return 0.0;
    }
    round2(size * if size >= 100.0 { v.child_scale_large } else { v.child_scale })
}

/// The three rings for one frame (coordinates rounded to 0.01 px).
pub fn voice_wave(input: &VoiceWaveInput) -> VoiceWave {
    let v = &*VOICE_WAVE;
    let size = input.size.max(0.0);
    let c = size / 2.0;
    let level = clamp01(input.level.unwrap_or(0.0));
    let bands: Vec<f64> = input.bands.as_deref().unwrap_or(&[]).iter().take(v.max_bands).map(|b| clamp01(*b)).collect();
    let ph = phase(input.activity, input.elapsed.unwrap_or(0.0), input.reduced_motion.unwrap_or(false));
    let drive = match input.activity {
        Activity::Listening | Activity::Speaking => v.amp_base + v.amp_level * level,
        Activity::Thinking => v.thinking_amp,
        Activity::Idle | Activity::Muted => 0.0,
    };
    let flat = bands.is_empty() || input.activity == Activity::Thinking;
    let rings = RINGS
        .iter()
        .enumerate()
        .map(|(k, part)| {
            let amp = v.ring_amp[k] * size * drive / 200.0;
            let points = (0..=v.steps)
                .map(|i| {
                    let t = i as f64 / v.steps as f64 * PI * 2.0;
                    let mut r = size * v.radius[k];
                    for (j, h) in v.harmonics[k].iter().enumerate() {
                        let band = if flat { 1.0 } else { bands[(3 * k + j) % bands.len()] };
                        r += amp / (j as f64 + 1.0) * band * (h * t + (v.phase0[k] + ph) * (j as f64 + 1.0)).sin();
                    }
                    [round2(c + r * t.cos()), round2(c + r * t.sin())]
                })
                .collect();
            VoiceWaveRing {
                part: (*part).to_string(),
                points,
                fill_alpha: v.fill_alpha[k],
                stroke_alpha: v.stroke_alpha[k],
                stroke_width: round2((v.stroke_width[k] * size / 200.0).max(1.0)),
            }
        })
        .collect();
    VoiceWave { rings, child_size: child_size(size), phase: round2(ph) }
}

