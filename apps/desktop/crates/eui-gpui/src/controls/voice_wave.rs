//! NEW-77 — THE VoiceWave for app chrome (the IDE's voice overlay capsule):
//! the same rings as the catalog `VoiceWave` (`eui_core::voice_wave`,
//! `fixtures/voice-wave.json`) and the same CHILD RULE (catalog/style.json
//! `voiceWave`): the child sits centred, drawn at its own size scaled about
//! the centre by min(1, childSize / its longer edge), never up; at `sm`
//! (an edge under 40 px) it is not painted at all.

use std::f64::consts::TAU;
use std::time::Duration;

use eui_core::voice_wave::{child_box, voice_wave as rings_of, Activity, ChildSize, VoiceWaveInput, VOICE_WAVE};
use gpui::{div, px, Animation, AnyElement, App, ElementId, Hsla, InteractiveElement as _, IntoElement as _, ParentElement as _, StatefulInteractiveElement as _, SharedString, Styled as _};
use gpui_component::{Icon, Sizable as _};

use super::animation::{SteppedAnimationExt as _, SMOOTH_STEP};
use crate::paint::natives::{voice_wave_canvas, voice_wave_paths};

/// The ring colours (the catalog's `outer`, `mid`, `inner` parts); each
/// ring applies its own fill / stroke alphas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceWaveColors {
    pub outer: Hsla,
    pub mid: Hsla,
    pub inner: Hsla,
}

impl VoiceWaveColors {
    /// All three rings in one colour.
    pub fn uniform(color: Hsla) -> Self {
        VoiceWaveColors { outer: color, mid: color, inner: color }
    }

    fn of(&self, part: &str) -> Hsla {
        match part {
            "outer" => self.outer,
            "mid" => self.mid,
            _ => self.inner,
        }
    }
}

/// The centre child: its own laid-out size and a builder that lays it out
/// at `scale` × that size (gpui scales no arbitrary element, so the child
/// draws itself smaller: an icon at a smaller glyph, a text at a smaller
/// font). The builder runs only when the child paints.
pub struct VoiceWaveChild {
    pub width: f32,
    pub height: f32,
    pub build: Box<dyn FnOnce(f32) -> AnyElement>,
}

impl VoiceWaveChild {
    /// Any child of its own `width` × `height` drawn by `build(scale)`.
    pub fn new(width: f32, height: f32, build: impl FnOnce(f32) -> AnyElement + 'static) -> Self {
        VoiceWaveChild { width, height, build: Box::new(build) }
    }

    /// An icon glyph of edge `size` in `color` (the overlay's mic / speaker).
    pub fn icon(icon: Icon, size: f32, color: Hsla) -> Self {
        Self::new(size, size, move |scale| icon.with_size(px(size * scale)).text_color(color).into_any_element())
    }
}

/// One repeat of the moving wave: the phase enters every harmonic at an
/// integer multiple, so a 2π turn of it loops seamlessly.
fn period() -> Duration {
    Duration::from_secs_f64(TAU / VOICE_WAVE.phase_speed)
}

/// NEW-77 — the VoiceWave at `size` px: three rings driven by `activity`,
/// `level` (0..1) and up to 8 `bands` (0..1), in `colors`, around the
/// optional centre `child` (THE CHILD RULE above). While the activity
/// moves (listening, thinking, speaking) it repaints on the
/// [`SMOOTH_STEP`] budget; under reduced motion and when idle or muted it
/// rests at phase 0 and schedules nothing. `label` names the image for
/// assistive tech (`"<label>, <activity>"`, the catalog's a11y name).
#[allow(clippy::too_many_arguments)]
pub fn voice_wave(
    id: impl Into<ElementId>,
    activity: Activity,
    level: f32,
    bands: &[f32],
    size: f32,
    child: Option<VoiceWaveChild>,
    colors: VoiceWaveColors,
    label: Option<SharedString>,
    cx: &App,
) -> AnyElement {
    let id: ElementId = id.into();
    let size = size.max(0.0);
    let input = VoiceWaveInput {
        size: size as f64,
        activity,
        level: Some(level as f64),
        bands: Some(bands.iter().map(|b| *b as f64).collect()),
        elapsed: Some(0.0),
        reduced_motion: Some(cx.reduce_motion()),
        child: child.as_ref().map(|c| ChildSize { width: c.width as f64, height: c.height as f64 }),
    };
    let rings = move |elapsed: f64| {
        let wave = rings_of(&VoiceWaveInput { elapsed: Some(elapsed), ..input.clone() });
        voice_wave_canvas(voice_wave_paths(&wave, |p| colors.of(p))).absolute().top_0().left_0().size(px(size))
    };
    // The child at its rule box (none at `sm`: never built, so no hit
    // target and nothing for assistive tech).
    let mut centre = div().absolute().top_0().left_0().size(px(size));
    if let Some(c) = child {
        let b = child_box(size as f64, ChildSize { width: c.width as f64, height: c.height as f64 });
        if b.scale > 0.0 {
            let el = (c.build)(b.scale as f32);
            centre = centre.child(div().absolute().left(px(b.x as f32)).top(px(b.y as f32)).w(px(b.width as f32)).h(px(b.height as f32)).flex().items_center().justify_center().child(el));
        }
    }
    let mut root = div().id(id).relative().flex_none().size(px(size));
    if let Some(l) = label {
        root = root.role(gpui::Role::Image).aria_label(SharedString::from(format!("{l}, {}", activity_name(activity))));
    }
    if !activity.animates() || cx.reduce_motion() {
        return root.child(rings(0.0)).child(centre).into_any_element();
    }
    let secs = period().as_secs_f64();
    root.child(centre.with_stepped_animation("voice-wave-phase", Animation::new(period()).repeat(), SMOOTH_STEP, move |this, delta| {
        div().absolute().top_0().left_0().size(px(size)).child(rings(delta as f64 * secs)).child(this)
    }))
    .into_any_element()
}

fn activity_name(a: Activity) -> &'static str {
    match a {
        Activity::Idle => "idle",
        Activity::Listening => "listening",
        Activity::Thinking => "thinking",
        Activity::Speaking => "speaking",
        Activity::Muted => "muted",
    }
}
