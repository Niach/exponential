//! EXP-1604: eUI Motion for the SwiftUI and Compose painters. One coarse
//! object: load once, then `tick` (the shared playback clock) and `sample`
//! (the packed display list, `eui-motion/README.md` › Packed frames) per frame.

use std::sync::Mutex;

use eui_motion::playback::{Playback, Timeline};
use eui_motion::{DisplayList, Limits, Motion};

use crate::{invalid, UiError};

struct Inner {
    motion: Motion,
    list: DisplayList,
    packed: Vec<f32>,
    playback: Playback,
    timeline: Timeline,
}

#[derive(uniffi::Object)]
pub struct MotionPlayer {
    inner: Mutex<Inner>,
}

/// The frame a clock reading lands on.
#[derive(uniffi::Record)]
pub struct MotionTick {
    pub frame: f64,
    pub done: bool,
}

#[uniffi::export]
impl MotionPlayer {
    /// Load Lottie JSON (`slots_json` = slot overrides `{id: value}`, or "").
    #[uniffi::constructor]
    pub fn new(lottie_json: String, slots_json: String) -> Result<std::sync::Arc<Self>, UiError> {
        let slots: serde_json::Map<String, serde_json::Value> =
            if slots_json.is_empty() { Default::default() } else { serde_json::from_str(&slots_json).map_err(invalid)? };
        let motion = Motion::load_with(lottie_json.as_bytes(), &Limits::default(), &slots)
            .map_err(|issues| UiError::Invalid { reason: serde_json::to_string(&issues).unwrap_or_default() })?;
        let timeline = motion.timeline();
        Ok(std::sync::Arc::new(Self {
            inner: Mutex::new(Inner { motion, list: DisplayList::default(), packed: Vec::new(), playback: Playback::default(), timeline }),
        }))
    }

    /// Composition width and height.
    pub fn size(&self) -> Vec<f32> {
        let (w, h) = self.inner.lock().unwrap().motion.size();
        vec![w, h]
    }

    /// Image asset locations, in `image` op order.
    pub fn images(&self) -> Vec<String> {
        self.inner.lock().unwrap().motion.images().into_iter().map(|(_, l)| l).collect()
    }

    /// Register a font (TTF/OTF bytes) for text layers without glyphs.
    pub fn add_font(&self, family: String, data: Vec<u8>) {
        self.inner.lock().unwrap().motion.add_font(&family, data);
    }

    /// Set the playback props (JSON: autoplay, loop, direction, speed, segment, poster).
    pub fn set_playback(&self, playback_json: String) {
        self.inner.lock().unwrap().playback = serde_json::from_str(&playback_json).map(|v| eui_motion::playback::Playback::from_json(&v)).unwrap_or_default();
    }

    /// The frame `elapsed_ms` after play started.
    pub fn tick(&self, elapsed_ms: f64) -> MotionTick {
        let g = self.inner.lock().unwrap();
        let t = g.timeline.tick(&g.playback, elapsed_ms);
        MotionTick { frame: t.frame, done: t.done }
    }

    /// The still frame (reduced motion, paused, before play).
    pub fn poster_frame(&self) -> f64 {
        let g = self.inner.lock().unwrap();
        g.timeline.poster(&g.playback)
    }

    /// The frame a bound progress (0–1) shows.
    pub fn progress_frame(&self, progress: f64) -> f64 {
        let g = self.inner.lock().unwrap();
        g.timeline.at_progress(&g.playback, progress)
    }

    /// The packed display list of `frame`.
    pub fn sample(&self, frame: f64) -> Vec<f32> {
        let mut g = self.inner.lock().unwrap();
        let Inner { motion, list, packed, .. } = &mut *g;
        motion.sample(frame, list);
        list.encode(packed);
        packed.clone()
    }
}
