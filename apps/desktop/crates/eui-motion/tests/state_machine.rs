//! EXP-1605: dotLottie state machines, pinned by
//! packages/exponential-ui/fixtures/motion-state.json (replayed over WASM in
//! TS and over uniffi in Swift and Kotlin). Each case = a machine + steps
//! (`post` an interaction, `set` inputs, `complete`), each step's expected
//! state, entered states and the state's resolved playback range.
//! `UPDATE_MOTION_FIXTURES=1` rewrites the expectations.

use eui_motion::playback::Timeline;
use eui_motion::state_machine::{Interaction, StateMachine};
use serde_json::{json, Value};
use std::path::PathBuf;

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/fixtures/motion-state.json")
}

fn cases() -> Vec<(&'static str, &'static str, Value, Vec<Value>)> {
    vec![
        (
            "toggle-on-click",
            "a button that plays `on` / `off` segments, toggled by clicks",
            json!({"initial": "off", "inputs": [{"type": "Boolean", "name": "on", "value": false}],
                "interactions": [{"type": "Click", "actions": [{"type": "Toggle", "inputName": "on"}]}],
                "states": [
                    {"name": "off", "type": "PlaybackState", "segment": [0, 10], "loop": false, "autoplay": true,
                     "transitions": [{"type": "Transition", "toState": "on", "guards": [{"type": "Boolean", "inputName": "on", "conditionType": "Equal", "compareTo": true}]}]},
                    {"name": "on", "type": "PlaybackState", "segment": [10, 20], "loop": false, "autoplay": true,
                     "transitions": [{"type": "Transition", "toState": "off", "guards": [{"type": "Boolean", "inputName": "on", "conditionType": "Equal", "compareTo": false}]}]}]}),
            vec![json!({"post": "click"}), json!({"post": "click"}), json!({"post": "pointerEnter"})],
        ),
        (
            "hover-in-out",
            "hover plays `in`, leaving plays `out`, then idles",
            json!({"initial": "idle", "inputs": [{"type": "Event", "name": "enter"}, {"type": "Event", "name": "leave"}],
                "interactions": [
                    {"type": "PointerEnter", "actions": [{"type": "Fire", "inputName": "enter"}]},
                    {"type": "PointerExit", "actions": [{"type": "Fire", "inputName": "leave"}]}],
                "states": [
                    {"name": "idle", "type": "PlaybackState", "segment": [0, 1], "autoplay": false,
                     "transitions": [{"type": "Transition", "toState": "in", "guards": [{"type": "Event", "inputName": "enter"}]}]},
                    {"name": "in", "type": "PlaybackState", "segment": [0, 15], "loop": false, "autoplay": true,
                     "transitions": [{"type": "Transition", "toState": "out", "guards": [{"type": "Event", "inputName": "leave"}]}]},
                    {"name": "out", "type": "PlaybackState", "segment": [15, 30], "loop": false, "autoplay": true,
                     "transitions": [{"type": "Transition", "toState": "idle"}]}]}),
            vec![json!({"post": "pointerEnter"}), json!({"post": "pointerExit"}), json!({"complete": true}), json!({"post": "pointerExit"})],
        ),
        (
            "progress-thresholds",
            "a bound numeric input picks a state by threshold (eUI `inputs` binding)",
            json!({"initial": "low", "inputs": [{"type": "Numeric", "name": "level", "value": 0}],
                "states": [
                    {"name": "low", "type": "PlaybackState", "marker": "low",
                     "transitions": [{"type": "Transition", "toState": "high", "guards": [{"type": "Numeric", "inputName": "level", "conditionType": "GreaterThanOrEqual", "compareTo": 50}]}]},
                    {"name": "high", "type": "PlaybackState", "marker": "high",
                     "transitions": [
                        {"type": "Transition", "toState": "done", "guards": [{"type": "Numeric", "inputName": "level", "conditionType": "Equal", "compareTo": 100}]},
                        {"type": "Transition", "toState": "low", "guards": [{"type": "Numeric", "inputName": "level", "conditionType": "LessThan", "compareTo": 50}]}]},
                    {"name": "done", "type": "PlaybackState", "segment": [29, 30], "final": true, "transitions": [{"type": "Transition", "toState": "low", "guards": [{"type": "Numeric", "inputName": "level", "conditionType": "LessThan", "compareTo": 50}]}]}]}),
            vec![json!({"set": {"level": 30}}), json!({"set": {"level": 60}}), json!({"set": {"level": 10}}), json!({"set": {"level": 60}}), json!({"set": {"level": 100}}), json!({"set": {"level": 0}})],
        ),
        (
            "counter-actions",
            "clicks increment a counter; three clicks reach `burst`; a layer-scoped interaction ignores other layers",
            json!({"initial": "rest", "inputs": [{"type": "Numeric", "name": "n", "value": 0}],
                "interactions": [
                    {"type": "Click", "layerName": "button", "actions": [{"type": "Increment", "inputName": "n", "value": 1}]}],
                "states": [
                    {"name": "rest", "type": "PlaybackState", "segment": [0, 5],
                     "transitions": [{"type": "Transition", "toState": "burst", "guards": [{"type": "Numeric", "inputName": "n", "conditionType": "GreaterThanOrEqual", "compareTo": 3}]}]},
                    {"name": "burst", "type": "PlaybackState", "segment": [5, 30], "loop": false, "mode": "Reverse",
                     "entryActions": [{"type": "SetNumeric", "inputName": "n", "value": 0}]}]}),
            vec![json!({"post": "click", "layer": "button"}), json!({"post": "click", "layer": "background"}), json!({"post": "click", "layer": "button"}), json!({"post": "click", "layer": "button"})],
        ),
    ]
}

#[test]
fn state_machine_fixture() {
    let timeline = Timeline { frame_rate: 30.0, in_point: 0.0, out_point: 30.0, markers: vec![("low".into(), 0.0, 10.0), ("high".into(), 10.0, 20.0)] };
    let mut out = Vec::new();
    for (id, covers, machine, steps) in cases() {
        let mut sm = StateMachine::parse(&machine).expect(id);
        let snap = |sm: &mut StateMachine| {
            let p = sm.playback().clone();
            let (a, b) = timeline.range_of(&p);
            json!({"state": sm.state(), "entered": sm.take_entered(), "final": sm.is_final(), "range": [a, b], "autoplay": p.autoplay, "inputs": sm.inputs_json()})
        };
        let initial = snap(&mut sm);
        let mut expected = Vec::new();
        for step in &steps {
            if let Some(kind) = step.get("post").and_then(Value::as_str) {
                let i = Interaction::parse(kind, step.get("layer").and_then(Value::as_str).map(str::to_string)).expect(kind);
                sm.post(&i);
            }
            if let Some(Value::Object(set)) = step.get("set") {
                for (k, v) in set {
                    sm.set_input(k, v);
                }
            }
            if step.get("complete").is_some() {
                sm.complete();
            }
            let mut s = snap(&mut sm);
            s["step"] = step.clone();
            expected.push(s);
        }
        out.push(json!({"id": id, "covers": covers, "machine": machine, "initial": initial, "steps": expected}));
    }
    let doc = json!({
        "$comment": "EXP-1605: dotLottie state machines in eUI Motion (apps/desktop/crates/eui-motion/src/state_machine.rs), the ONE interpreter every renderer uses. A case = a machine (the dotLottie 2 JSON a Motion's `stateMachine` prop or a .lottie `s/` file carries) + steps; each step: `post` an interaction (click|pointerDown|pointerUp|pointerEnter|pointerExit|complete|loopComplete, `layer` = the hit layer) and/or `set` inputs (a bound eUI `inputs` value) and/or `complete` (the state's playback finished: its guard-less transition), then the expected `state`, the states `entered` since the previous step (hosts send each as the Motion's `on.state` action, context {state}), `final`, the state's playback `range` [from, to) in frames and `autoplay` on the timeline 30 fps / ip 0 / op 30 with markers low = 0..10 and high = 10..30, and the `inputs` after the step. Replayed by eui-motion tests/state_machine.rs (UPDATE_MOTION_FIXTURES=1 rewrites), over WASM (TS) and uniffi (Swift, Kotlin).",
        "lottie": {"v": "5.12.0", "fr": 30, "ip": 0, "op": 30, "w": 10, "h": 10, "layers": [], "markers": [{"cm": "low", "tm": 0, "dr": 10}, {"cm": "high", "tm": 10, "dr": 20}]},
        "cases": out,
    });
    let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
    if std::env::var("UPDATE_MOTION_FIXTURES").is_ok() {
        std::fs::write(path(), &text).unwrap();
    }
    let on_disk = std::fs::read_to_string(path()).expect("fixtures/motion-state.json (UPDATE_MOTION_FIXTURES=1 writes it)");
    assert_eq!(on_disk, text, "motion-state.json drifted from the engine: rerun with UPDATE_MOTION_FIXTURES=1");
}
