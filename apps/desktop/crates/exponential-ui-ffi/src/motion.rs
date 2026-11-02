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
    machine: Option<eui_motion::state_machine::StateMachine>,
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
            inner: Mutex::new(Inner { motion, list: DisplayList::default(), packed: Vec::new(), playback: Playback::default(), timeline, machine: None }),
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

    /// Start a dotLottie state machine (JSON); its current state's playback
    /// replaces `set_playback`. False when it has no states.
    pub fn set_state_machine(&self, json: String) -> bool {
        let mut g = self.inner.lock().unwrap();
        g.machine = serde_json::from_str(&json).ok().and_then(|v| eui_motion::state_machine::StateMachine::parse(&v));
        sync(&mut g);
        g.machine.is_some()
    }

    /// Report an interaction (click, pointerDown, pointerUp, pointerEnter,
    /// pointerExit, complete, loopComplete; `layer` = the hit layer or "").
    /// True when the state changed (restart the clock).
    pub fn sm_post(&self, kind: String, layer: String) -> bool {
        let mut g = self.inner.lock().unwrap();
        let layer = (!layer.is_empty()).then_some(layer);
        let changed = match (&mut g.machine, eui_motion::state_machine::Interaction::parse(&kind, layer)) {
            (Some(m), Some(eui_motion::state_machine::Interaction::Complete)) => m.complete(),
            (Some(m), Some(i)) => m.post(&i),
            _ => false,
        };
        sync(&mut g);
        changed
    }

    /// Write state-machine inputs (a JSON object); true when the state changed.
    pub fn sm_set_inputs(&self, json: String) -> bool {
        let mut g = self.inner.lock().unwrap();
        let mut changed = false;
        if let (Some(m), Ok(serde_json::Value::Object(o))) = (&mut g.machine, serde_json::from_str::<serde_json::Value>(&json)) {
            for (k, v) in &o {
                changed |= m.set_input(k, v);
            }
        }
        sync(&mut g);
        changed
    }

    /// `{state, entered, final, inputs}` as JSON (`entered` drains), or "null".
    pub fn sm_status(&self) -> String {
        let mut g = self.inner.lock().unwrap();
        match &mut g.machine {
            Some(m) => serde_json::json!({"state": m.state(), "entered": m.take_entered(), "final": m.is_final(), "inputs": m.inputs_json()}).to_string(),
            None => "null".into(),
        }
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

/// Compile an authoring-dialect document (a Motion's inline `layers` plus
/// `size`, `fps`, `duration`, `start`, `time`, `assets`, `markers`) into Lottie
/// JSON; the issues as `UiError::Invalid` (JSON) otherwise.
#[uniffi::export]
pub fn motion_compile(dialect_json: String) -> Result<String, UiError> {
    let doc: serde_json::Value = serde_json::from_str(&dialect_json).map_err(invalid)?;
    eui_motion::dialect::compile(&doc)
        .map(|l| l.to_string())
        .map_err(|issues| UiError::Invalid { reason: serde_json::to_string(&issues).unwrap_or_default() })
}

/// The first animation of a `.lottie` archive as Lottie JSON (images inlined).
#[uniffi::export]
pub fn motion_read_dotlottie(data: Vec<u8>) -> Result<String, UiError> {
    let file = eui_motion::dotlottie::read(&data)
        .map_err(|issues| UiError::Invalid { reason: serde_json::to_string(&issues).unwrap_or_default() })?;
    file.animations.into_iter().next().map(|(_, a)| a.to_string()).ok_or_else(|| invalid("no animation in the archive"))
}

/// The first state machine of a `.lottie` archive as JSON, or "" (EXP-1605,
/// README › Interactivity §1).
#[uniffi::export]
pub fn motion_read_dotlottie_machine(data: Vec<u8>) -> Result<String, UiError> {
    let file = eui_motion::dotlottie::read(&data)
        .map_err(|issues| UiError::Invalid { reason: serde_json::to_string(&issues).unwrap_or_default() })?;
    // The manifest's first `stateMachines` entry, else the first file.
    let first = file.manifest["stateMachines"][0]["id"].as_str().and_then(|id| file.state_machines.get(id));
    Ok(first.or_else(|| file.state_machines.values().next()).map(|m| m.to_string()).unwrap_or_default())
}

fn sync(g: &mut Inner) {
    if let Some(m) = &g.machine {
        g.playback = m.playback().clone();
    }
}
