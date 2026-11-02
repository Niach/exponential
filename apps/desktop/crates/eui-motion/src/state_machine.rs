//! dotLottie state machines (EXP-1605), the ONE interpreter every renderer
//! uses (WASM, uniffi, gpui), pinned by `fixtures/motion-state.json`.
//!
//! The dotLottie 2 format: `initial`, `states` (playback states: segment or
//! marker, loop, speed, direction, autoplay, `final`), `transitions` per
//! state with `guards` on `inputs`, `inputs` (numeric, string, boolean,
//! event) and `interactions` (pointer, completion) running `actions`.
//! Hosts feed it interactions (`post`) and input writes (`set_input`), read
//! the current state's playback props, and turn `entered` states into
//! eUI `on.state` actions. Unknown kinds are ignored, never fatal.

use serde_json::{json, Map, Value};

use crate::playback::Playback;

/// An input value.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Numeric(f64),
    String(String),
    Boolean(bool),
    /// Event inputs carry no value; `fired` holds within one evaluation.
    Event,
}

/// What a host reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Interaction {
    /// Pointer down / up / click on the animation (`layer` = a hit layer name when known).
    PointerDown { layer: Option<String> },
    PointerUp { layer: Option<String> },
    Click { layer: Option<String> },
    PointerEnter { layer: Option<String> },
    PointerExit { layer: Option<String> },
    /// The current state's playback finished (a finite loop played out).
    Complete,
    /// A loop of the current state completed.
    LoopComplete,
}

impl Interaction {
    fn kind(&self) -> &'static str {
        match self {
            Self::PointerDown { .. } => "PointerDown",
            Self::PointerUp { .. } => "PointerUp",
            Self::Click { .. } => "Click",
            Self::PointerEnter { .. } => "PointerEnter",
            Self::PointerExit { .. } => "PointerExit",
            Self::Complete => "OnComplete",
            Self::LoopComplete => "OnLoopComplete",
        }
    }

    fn layer(&self) -> Option<&str> {
        match self {
            Self::PointerDown { layer } | Self::PointerUp { layer } | Self::Click { layer } | Self::PointerEnter { layer } | Self::PointerExit { layer } => layer.as_deref(),
            _ => None,
        }
    }

    /// From a host string: `pointerDown`, `pointerUp`, `click`, `pointerEnter`,
    /// `pointerExit`, `complete`, `loopComplete` (eUI events `press`/`hover` map here).
    pub fn parse(kind: &str, layer: Option<String>) -> Option<Self> {
        Some(match kind {
            "pointerDown" | "PointerDown" => Self::PointerDown { layer },
            "pointerUp" | "PointerUp" => Self::PointerUp { layer },
            "click" | "Click" | "press" => Self::Click { layer },
            "pointerEnter" | "PointerEnter" | "hoverStart" => Self::PointerEnter { layer },
            "pointerExit" | "PointerExit" | "hoverEnd" => Self::PointerExit { layer },
            "complete" | "OnComplete" => Self::Complete,
            "loopComplete" | "OnLoopComplete" => Self::LoopComplete,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
struct State {
    name: String,
    playback: Playback,
    is_final: bool,
    transitions: Vec<Transition>,
    entry_actions: Vec<Value>,
    exit_actions: Vec<Value>,
}

#[derive(Clone, Debug)]
struct Transition {
    to: String,
    guards: Vec<Value>,
}

/// A running state machine.
#[derive(Clone, Debug)]
pub struct StateMachine {
    states: Vec<State>,
    current: usize,
    inputs: Vec<(String, Input)>,
    interactions: Vec<Value>,
    fired: Vec<String>,
    /// States entered since the host last drained them (`take_entered`).
    entered: Vec<String>,
    /// Guards against transition loops within one evaluation.
    max_hops: usize,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn state_playback(st: &Value) -> Playback {
    // dotLottie keys → the eUI Motion props `Playback::from_json` reads.
    let mut p = Map::new();
    if let Some(v) = st.get("autoplay") {
        p.insert("autoplay".into(), v.clone());
    }
    if let Some(v) = st.get("loop") {
        p.insert("loop".into(), v.clone());
    }
    if let Some(v) = st.get("speed") {
        p.insert("speed".into(), v.clone());
    }
    if let Some(m) = st.get("marker").and_then(Value::as_str) {
        p.insert("marker".into(), json!(m));
    }
    if let Some(Value::Array(seg)) = st.get("segment") {
        // dotLottie segments are FRAMES; the eUI segment is ms: kept as frames
        // under `segmentFrames`, resolved by `Timeline` through `frames()`.
        p.insert("segmentFrames".into(), Value::Array(seg.clone()));
    }
    match st.get("mode").and_then(Value::as_str) {
        Some("Reverse") => {
            p.insert("direction".into(), json!("reverse"));
        }
        Some("Bounce") | Some("ReverseBounce") => {
            p.insert("direction".into(), json!("alternate"));
        }
        _ => {}
    }
    let mut pb = Playback::from_json(&Value::Object(p.clone()));
    if let Some(Value::Array(seg)) = p.get("segmentFrames") {
        if seg.len() == 2 {
            pb.segment_frames = Some([seg[0].as_f64().unwrap_or(0.0), seg[1].as_f64().unwrap_or(0.0)]);
        }
    }
    pb
}

impl StateMachine {
    /// Parse a dotLottie state machine (JSON). `None` when it has no states.
    pub fn parse(v: &Value) -> Option<Self> {
        let states_json = v.get("states")?.as_array()?;
        let mut states = Vec::new();
        for st in states_json {
            let name = s(st, "name")?;
            let transitions = st
                .get("transitions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|t| Some(Transition { to: s(t, "toState")?, guards: t.get("guards").and_then(Value::as_array).cloned().unwrap_or_default() }))
                .collect();
            let actions = |k: &str| st.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
            states.push(State {
                name,
                playback: state_playback(st),
                is_final: st.get("final").and_then(Value::as_bool).unwrap_or(false),
                transitions,
                entry_actions: actions("entryActions"),
                exit_actions: actions("exitActions"),
            });
        }
        if states.is_empty() {
            return None;
        }
        let initial = s(v, "initial").and_then(|n| states.iter().position(|st| st.name == n)).unwrap_or(0);
        let inputs = v
            .get("inputs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|i| {
                let name = s(i, "name")?;
                let val = match i.get("type").and_then(Value::as_str)? {
                    "Numeric" => Input::Numeric(i.get("value").and_then(Value::as_f64).unwrap_or(0.0)),
                    "String" => Input::String(s(i, "value").unwrap_or_default()),
                    "Boolean" => Input::Boolean(i.get("value").and_then(Value::as_bool).unwrap_or(false)),
                    "Event" => Input::Event,
                    _ => return None,
                };
                Some((name, val))
            })
            .collect();
        let mut sm = Self {
            states,
            current: initial,
            inputs,
            interactions: v.get("interactions").and_then(Value::as_array).cloned().unwrap_or_default(),
            fired: Vec::new(),
            entered: Vec::new(),
            max_hops: 16,
        };
        let name = sm.states[initial].name.clone();
        sm.entered.push(name);
        let entry = sm.states[initial].entry_actions.clone();
        sm.run_actions(&entry);
        sm.evaluate();
        Some(sm)
    }

    /// The current state's name.
    pub fn state(&self) -> &str {
        &self.states[self.current].name
    }

    /// The current state's playback props.
    pub fn playback(&self) -> &Playback {
        &self.states[self.current].playback
    }

    /// Whether the machine sits in a final state.
    pub fn is_final(&self) -> bool {
        self.states[self.current].is_final
    }

    /// States entered since the last call (oldest first); hosts turn them into `on.state` actions.
    pub fn take_entered(&mut self) -> Vec<String> {
        std::mem::take(&mut self.entered)
    }

    /// Current input values as JSON `{name: value}` (events omitted).
    pub fn inputs_json(&self) -> Value {
        let mut o = Map::new();
        for (n, v) in &self.inputs {
            match v {
                Input::Numeric(x) => o.insert(n.clone(), json!(x)),
                Input::String(x) => o.insert(n.clone(), json!(x)),
                Input::Boolean(x) => o.insert(n.clone(), json!(x)),
                Input::Event => None,
            };
        }
        Value::Object(o)
    }

    /// Write an input from the host (a bound eUI `inputs` value): numbers,
    /// strings and booleans set; `true` on an event input fires it. Returns
    /// whether the state changed.
    pub fn set_input(&mut self, name: &str, value: &Value) -> bool {
        let before = self.current;
        let Some(slot) = self.inputs.iter_mut().find(|(n, _)| n == name) else { return false };
        match (&mut slot.1, value) {
            (Input::Numeric(x), Value::Number(n)) => *x = n.as_f64().unwrap_or(*x),
            (Input::String(x), Value::String(v)) => *x = v.clone(),
            (Input::Boolean(x), Value::Bool(b)) => *x = *b,
            (Input::Event, Value::Bool(true)) | (Input::Event, Value::Null) => self.fired.push(name.to_string()),
            _ => return false,
        }
        self.evaluate();
        self.fired.clear();
        before != self.current
    }

    /// Report an interaction. Returns whether the state changed.
    pub fn post(&mut self, interaction: &Interaction) -> bool {
        let before = self.current;
        let interactions = self.interactions.clone();
        for i in &interactions {
            if i.get("type").and_then(Value::as_str) != Some(interaction.kind()) {
                continue;
            }
            if let Some(want) = i.get("layerName").and_then(Value::as_str) {
                if interaction.layer() != Some(want) {
                    continue;
                }
            }
            if let Some(want) = i.get("stateName").and_then(Value::as_str) {
                if self.state() != want {
                    continue;
                }
            }
            let actions = i.get("actions").and_then(Value::as_array).cloned().unwrap_or_default();
            self.run_actions(&actions);
        }
        self.evaluate();
        self.fired.clear();
        before != self.current
    }

    fn input_mut(&mut self, name: &str) -> Option<&mut Input> {
        self.inputs.iter_mut().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    fn run_actions(&mut self, actions: &[Value]) {
        for a in actions {
            let name = s(a, "inputName").unwrap_or_default();
            match a.get("type").and_then(Value::as_str).unwrap_or("") {
                "Fire" => self.fired.push(name),
                "SetNumeric" => {
                    if let Some(Input::Numeric(x)) = self.input_mut(&name) {
                        *x = a.get("value").and_then(Value::as_f64).unwrap_or(*x);
                    }
                }
                "Increment" | "Decrement" => {
                    let sign = if a["type"] == "Increment" { 1.0 } else { -1.0 };
                    let by = a.get("value").and_then(Value::as_f64).unwrap_or(1.0);
                    if let Some(Input::Numeric(x)) = self.input_mut(&name) {
                        *x += sign * by;
                    }
                }
                "SetString" => {
                    if let Some(Input::String(x)) = self.input_mut(&name) {
                        *x = s(a, "value").unwrap_or_default();
                    }
                }
                "SetBoolean" => {
                    if let Some(Input::Boolean(x)) = self.input_mut(&name) {
                        *x = a.get("value").and_then(Value::as_bool).unwrap_or(true);
                    }
                }
                "Toggle" => {
                    if let Some(Input::Boolean(x)) = self.input_mut(&name) {
                        *x = !*x;
                    }
                }
                "Reset" => {
                    if let Some(i) = self.input_mut(&name) {
                        *i = match i {
                            Input::Numeric(_) => Input::Numeric(0.0),
                            Input::String(_) => Input::String(String::new()),
                            Input::Boolean(_) => Input::Boolean(false),
                            Input::Event => Input::Event,
                        };
                    }
                }
                _ => {}
            }
        }
    }

    fn guard_holds(&self, g: &Value) -> bool {
        let name = s(g, "inputName").unwrap_or_default();
        let cond = g.get("conditionType").and_then(Value::as_str).unwrap_or("Equal");
        let to = g.get("compareTo");
        match g.get("type").and_then(Value::as_str).unwrap_or("") {
            "Event" => self.fired.iter().any(|f| *f == name),
            "Numeric" => {
                let Some(Input::Numeric(x)) = self.inputs.iter().find(|(n, _)| *n == name).map(|(_, v)| v) else { return false };
                let Some(y) = to.and_then(|t| t.as_f64().or_else(|| t.as_str().and_then(|n| match self.inputs.iter().find(|(k, _)| k == n) { Some((_, Input::Numeric(v))) => Some(*v), _ => None }))) else { return false };
                match cond {
                    "Equal" => x == &y,
                    "NotEqual" => x != &y,
                    "GreaterThan" => *x > y,
                    "GreaterThanOrEqual" => *x >= y,
                    "LessThan" => *x < y,
                    "LessThanOrEqual" => *x <= y,
                    _ => false,
                }
            }
            "String" => {
                let Some(Input::String(x)) = self.inputs.iter().find(|(n, _)| *n == name).map(|(_, v)| v) else { return false };
                let y = to.and_then(Value::as_str).unwrap_or("");
                match cond {
                    "Equal" => x == y,
                    "NotEqual" => x != y,
                    _ => false,
                }
            }
            "Boolean" => {
                let Some(Input::Boolean(x)) = self.inputs.iter().find(|(n, _)| *n == name).map(|(_, v)| v) else { return false };
                let y = to.and_then(Value::as_bool).unwrap_or(true);
                match cond {
                    "Equal" => *x == y,
                    "NotEqual" => *x != y,
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Take guarded transitions whose guards all hold, up to `max_hops`
    /// (loop guard). Guard-less transitions wait for completion (`complete`).
    fn evaluate(&mut self) {
        for _ in 0..self.max_hops {
            if self.states[self.current].is_final {
                return;
            }
            let next = self.states[self.current]
                .transitions
                .iter()
                .find(|t| !t.guards.is_empty() && t.guards.iter().all(|g| self.guard_holds(g)))
                .map(|t| t.to.clone());
            let Some(to) = next else { return };
            let Some(idx) = self.states.iter().position(|st| st.name == to) else { return };
            let exit = self.states[self.current].exit_actions.clone();
            self.run_actions(&exit);
            self.current = idx;
            self.entered.push(to);
            // An event fires one transition.
            self.fired.clear();
            let entry = self.states[idx].entry_actions.clone();
            self.run_actions(&entry);
        }
    }

    /// The current state's playback completed: take its guard-less transition.
    pub fn complete(&mut self) -> bool {
        let before = self.current;
        if let Some(to) = self.states[self.current].transitions.iter().find(|t| t.guards.is_empty()).map(|t| t.to.clone()) {
            if let Some(idx) = self.states.iter().position(|st| st.name == to) {
                let exit = self.states[self.current].exit_actions.clone();
                self.run_actions(&exit);
                self.current = idx;
                self.entered.push(to);
                let entry = self.states[idx].entry_actions.clone();
                self.run_actions(&entry);
            }
        }
        self.post(&Interaction::Complete);
        before != self.current
    }
}
