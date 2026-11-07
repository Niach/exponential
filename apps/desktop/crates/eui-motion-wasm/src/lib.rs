//! `eui-motion-wasm`: the browser (and server-side validation) facade over
//! eui-motion. Frames cross as a `Float32Array` in the packed display-list
//! encoding (`eui-motion/README.md` § Packed frames).

use eui_motion::{canvas, dialect, display::DisplayList, validate, Limits, Motion};
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;

fn issues_json(issues: &[eui_motion::Issue]) -> String {
    serde_json::to_string(issues).unwrap_or_else(|_| "[]".into())
}

/// A loaded animation; `sample(frame)` returns the packed frame.
#[wasm_bindgen]
pub struct MotionPlayer {
    motion: Motion,
    list: DisplayList,
    packed: Vec<f32>,
    playback: eui_motion::playback::Playback,
    timeline: eui_motion::playback::Timeline,
    machine: Option<eui_motion::state_machine::StateMachine>,
}

#[wasm_bindgen]
impl MotionPlayer {
    /// Load Lottie JSON (`slots` = a JSON object of slot overrides, or "").
    /// Throws a JSON array of issues when refused.
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, slots: &str) -> Result<MotionPlayer, JsValue> {
        let slots: Map<String, Value> = if slots.is_empty() { Map::new() } else { serde_json::from_str(slots).unwrap_or_default() };
        let motion = Motion::load_with(source.as_bytes(), &Limits::default(), &slots).map_err(|i| JsValue::from_str(&issues_json(&i)))?;
        let timeline = motion.timeline();
        Ok(Self { motion, list: DisplayList::default(), packed: Vec::new(), playback: Default::default(), timeline, machine: None })
    }

    pub fn width(&self) -> f32 {
        self.motion.size().0
    }
    pub fn height(&self) -> f32 {
        self.motion.size().1
    }
    #[wasm_bindgen(js_name = frameRate)]
    pub fn frame_rate(&self) -> f64 {
        self.motion.frame_rate()
    }
    #[wasm_bindgen(js_name = inPoint)]
    pub fn in_point(&self) -> f64 {
        self.motion.frames().0
    }
    #[wasm_bindgen(js_name = outPoint)]
    pub fn out_point(&self) -> f64 {
        self.motion.frames().1
    }
    /// Markers as JSON `[{name, time, duration}]` (frames).
    pub fn markers(&self) -> String {
        serde_json::to_string(self.motion.markers()).unwrap_or_else(|_| "[]".into())
    }
    /// Image asset locations, in `Image` op order.
    pub fn images(&self) -> Vec<String> {
        self.motion.images().into_iter().map(|(_, location)| location).collect()
    }
    /// The non-fatal findings as a JSON array.
    pub fn warnings(&self) -> String {
        issues_json(&self.motion.warnings)
    }
    /// Register a font (TTF/OTF bytes) for text layers without glyphs.
    #[wasm_bindgen(js_name = addFont)]
    pub fn add_font(&mut self, family: &str, data: Vec<u8>) {
        self.motion.add_font(family, data);
    }
    #[wasm_bindgen(js_name = needsFonts)]
    pub fn needs_fonts(&self) -> bool {
        self.motion.needs_fonts()
    }

    /// Set the playback props (JSON: autoplay, loop, direction, speed, segment, poster).
    #[wasm_bindgen(js_name = setPlayback)]
    pub fn set_playback(&mut self, json: &str) {
        self.playback = serde_json::from_str(json).map(|v| eui_motion::playback::Playback::from_json(&v)).unwrap_or_default();
    }
    /// The frame `elapsedMs` after play started; NaN-free. `done()` tells a finished finite loop.
    pub fn tick(&self, elapsed_ms: f64) -> f64 {
        self.timeline.tick(&self.playback, elapsed_ms).frame
    }
    /// Whether a finite loop count has played out at `elapsedMs`.
    pub fn done(&self, elapsed_ms: f64) -> bool {
        self.timeline.tick(&self.playback, elapsed_ms).done
    }
    /// The still frame (reduced motion, paused, before play).
    #[wasm_bindgen(js_name = posterFrame)]
    pub fn poster_frame(&self) -> f64 {
        self.timeline.poster(&self.playback)
    }
    /// The frame a bound progress (0–1) shows.
    #[wasm_bindgen(js_name = progressFrame)]
    pub fn progress_frame(&self, progress: f64) -> f64 {
        self.timeline.at_progress(&self.playback, progress)
    }

    /// Start a dotLottie state machine (JSON); its current state's playback
    /// replaces `setPlayback`. Returns false when it has no states.
    #[wasm_bindgen(js_name = setStateMachine)]
    pub fn set_state_machine(&mut self, json: &str) -> bool {
        self.machine = serde_json::from_str(json).ok().and_then(|v| eui_motion::state_machine::StateMachine::parse(&v));
        self.sync_machine();
        self.machine.is_some()
    }
    /// Report an interaction (click, pointerDown, pointerUp, pointerEnter,
    /// pointerExit, complete, loopComplete; `layer` = the hit layer or "").
    /// Returns whether the state changed (restart the clock).
    #[wasm_bindgen(js_name = smPost)]
    pub fn sm_post(&mut self, kind: &str, layer: &str) -> bool {
        let layer = (!layer.is_empty()).then(|| layer.to_string());
        let changed = match (&mut self.machine, eui_motion::state_machine::Interaction::parse(kind, layer)) {
            (Some(m), Some(eui_motion::state_machine::Interaction::Complete)) => m.complete(),
            (Some(m), Some(i)) => m.post(&i),
            _ => false,
        };
        self.sync_machine();
        changed
    }
    /// Write state-machine inputs (JSON object); returns whether the state changed.
    #[wasm_bindgen(js_name = smSetInputs)]
    pub fn sm_set_inputs(&mut self, json: &str) -> bool {
        let mut changed = false;
        if let (Some(m), Ok(Value::Object(o))) = (&mut self.machine, serde_json::from_str::<Value>(json)) {
            for (k, v) in &o {
                changed |= m.set_input(k, v);
            }
        }
        self.sync_machine();
        changed
    }
    /// `{state, entered: [...], final, inputs}`; `entered` drains (→ `on.state` actions).
    #[wasm_bindgen(js_name = smStatus)]
    pub fn sm_status(&mut self) -> String {
        match &mut self.machine {
            Some(m) => json!({"state": m.state(), "entered": m.take_entered(), "final": m.is_final(), "inputs": m.inputs_json()}).to_string(),
            None => "null".into(),
        }
    }

    /// Sample `frame` into the packed f32 encoding (a copy into JS memory).
    pub fn sample(&mut self, frame: f64) -> js_sys::Float32Array {
        self.motion.sample(frame, &mut self.list);
        self.list.encode(&mut self.packed);
        js_sys::Float32Array::from(self.packed.as_slice())
    }
}

impl MotionPlayer {
    fn sync_machine(&mut self) {
        if let Some(m) = &self.machine {
            self.playback = m.playback().clone();
        }
    }
}

/// Validate a Lottie JSON string: `{"errors":[…],"warnings":[…]}`.
#[wasm_bindgen]
pub fn validate(source: &str) -> String {
    let report = validate::check(source.as_bytes(), &Limits::default());
    json!({ "errors": report.errors, "warnings": report.warnings }).to_string()
}

/// Compile a dialect document: `{"lottie": {…}}` or `{"errors": […]}`.
#[wasm_bindgen]
pub fn compile(source: &str) -> String {
    let doc: Value = match serde_json::from_str(source) {
        Ok(v) => v,
        Err(e) => return json!({"errors": [{"code": "parse", "path": "", "message": e.to_string()}]}).to_string(),
    };
    match dialect::compile(&doc) {
        Ok(l) => json!({ "lottie": l }).to_string(),
        Err(issues) => json!({ "errors": issues }).to_string(),
    }
}

/// Round 8: a compiled Canvas (`{size, shapes}`); `sample(slots)` returns
/// the packed frame (shapes, then the `text` ops).
#[wasm_bindgen]
pub struct CanvasPlayer {
    canvas: canvas::Canvas,
    list: DisplayList,
    packed: Vec<f32>,
}

#[wasm_bindgen]
impl CanvasPlayer {
    /// Compile a Canvas node's `{size, shapes}` JSON. Throws a JSON array of
    /// issues when refused.
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<CanvasPlayer, JsValue> {
        let doc: Value = serde_json::from_str(source)
            .map_err(|e| JsValue::from_str(&json!([{"code": "parse", "path": "", "message": e.to_string()}]).to_string()))?;
        let canvas = canvas::Canvas::compile(&doc).map_err(|i| JsValue::from_str(&issues_json(&i)))?;
        Ok(Self { canvas, list: DisplayList::default(), packed: Vec::new() })
    }

    /// Every slot the canvas declares (`$color.<token>`, `ld:<light>:<dark>`):
    /// the ids a host fills from its theme, exactly like a Motion's.
    /// A JSON array of strings (a string crosses cheaper than a `string[]`).
    #[wasm_bindgen(js_name = slotIds)]
    pub fn slot_ids(&self) -> String {
        serde_json::to_string(&self.canvas.slot_ids()).unwrap_or_else(|_| "[]".into())
    }
    /// The packed frame with these slot overrides (JSON object, or "").
    pub fn sample(&mut self, slots: &str) -> js_sys::Float32Array {
        let slots: Map<String, Value> = if slots.is_empty() { Map::new() } else { serde_json::from_str(slots).unwrap_or_default() };
        self.canvas.render(&slots, &mut self.list);
        self.list.encode(&mut self.packed);
        js_sys::Float32Array::from(self.packed.as_slice())
    }
}

