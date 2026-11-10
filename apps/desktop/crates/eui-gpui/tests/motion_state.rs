//! EXP-1605: Motion interactivity in the gpui painter (`eui-motion/README.md`
//! › "Interactivity (M4)"), in a headless window.
//!
//! - `fixtures/motion-state.json` replayed through the view: each case's
//!   machine is a Motion's `stateMachine`, `post` steps reach it through the
//!   view's interaction entry, `set` steps write the bound `inputs` in the
//!   data model, `complete` steps let the clock play the state out; after
//!   each step the state, the states reported as `on.state` actions and
//!   the playback range the painter plays match.
//! - Real input: a click and Enter toggle a state machine button (`on.press`
//!   + `on.state`, the clock restarts at the new state's first frame, a11y
//!   button named by `label`), hovering plays `in` / `out` with `on.hover`,
//!   `on.complete` and `on.visible`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use eui_motion::state_machine::Interaction;
use gpui::{div, point, prelude::*, px, App, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use serde_json::{json, Value};

fn fixture() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/fixtures/motion-state.json");
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }
}

impl Recorder {
    fn take(&self) -> Vec<ActionEvent> {
        std::mem::take(&mut *self.actions.borrow_mut())
    }
}

/// The `on.state` actions' states, oldest first.
fn states_of(actions: &[ActionEvent]) -> Vec<String> {
    actions.iter().filter(|a| a.event == "state").map(|a| a.context["state"].as_str().unwrap().to_string()).collect()
}

fn events_of(actions: &[ActionEvent]) -> Vec<String> {
    actions.iter().map(|a| a.event.clone()).collect()
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn on_all() -> Value {
    json!({
        "press": {"event": {"name": "pressed"}},
        "hover": {"event": {"name": "hovered"}},
        "visible": {"event": {"name": "seen"}},
        "complete": {"event": {"name": "completed"}},
        "state": {"event": {"name": "state"}}
    })
}

fn surface(cx: &mut TestAppContext, motion: Value, data: Value) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start", "width": 480, "padding": 40}, "children": [motion]});
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            v.set_pause_motion_in_background(false);
            v.record_bounds(true);
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.set_data("/sm", Some(data), cx).unwrap();
        });
        Pane { view }
    });
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    draw(vcx);
    draw(vcx);
    (view, log, vcx)
}

fn advance(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, ms: u64) {
    view.update(cx, |v, cx| v.advance_clock(Duration::from_millis(ms), cx));
    draw(cx);
    draw(cx);
}

fn state(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| v.motion_state("m")).expect("a state machine")
}

#[gpui::test]
fn the_state_fixture_replays_through_the_view(cx: &mut TestAppContext) {
    let f = fixture();
    let lottie = f["lottie"].clone();
    for c in f["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let motion = json!({"id": "m", "component": "Motion", "props": {"lottie": lottie, "stateMachine": c["machine"], "inputs": {"@path": "/sm"}, "size": [40, 40]}, "on": on_all()});
        let mut data = c["initial"]["inputs"].clone();
        let (view, log, vcx) = surface(cx, motion, data.clone());
        let check = |view: &Entity<SurfaceView>, vcx: &mut VisualTestContext, want: &Value, at: &str| {
            assert_eq!(state(view, vcx), want["state"].as_str().unwrap(), "{at}: state");
            let entered: Vec<String> = want["entered"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
            assert_eq!(states_of(&log.take()), entered, "{at}: on.state");
            let range = view.read_with(vcx, |v, _| v.motion_range("m")).unwrap();
            let r = want["range"].as_array().unwrap();
            assert_eq!([range.0, range.1], [r[0].as_f64().unwrap(), r[1].as_f64().unwrap()], "{at}: range");
        };
        check(&view, vcx, &c["initial"], &format!("{id} initial"));
        for (i, s) in c["steps"].as_array().unwrap().iter().enumerate() {
            let step = &s["step"];
            if let Some(kind) = step["post"].as_str() {
                let layer = step["layer"].as_str().map(str::to_string);
                let interaction = Interaction::parse(kind, layer).unwrap();
                view.update(vcx, |v, cx| v.motion_interact("m", &interaction, cx));
                draw(vcx);
            }
            if let Some(Value::Object(set)) = step.get("set") {
                for (k, v) in set {
                    data[k] = v.clone();
                }
                let d = data.clone();
                view.update(vcx, |v, cx| v.set_data("/sm", Some(d), cx).unwrap());
                draw(vcx);
                draw(vcx);
            }
            if step["complete"] == json!(true) {
                // The state plays out on the clock; the painter posts `complete`.
                let before = state(&view, vcx);
                for _ in 0..40 {
                    advance(&view, vcx, 50);
                    if state(&view, vcx) != before {
                        break;
                    }
                }
            }
            check(&view, vcx, s, &format!("{id} step {i}"));
        }
    }
}

/// The styleguide's like button: off / on segments toggled by clicks.
fn like() -> Value {
    let f = fixture();
    let machine = f["cases"].as_array().unwrap().iter().find(|c| c["id"] == "toggle-on-click").unwrap()["machine"].clone();
    json!({"id": "m", "component": "Motion", "props": {"lottie": f["lottie"], "stateMachine": machine, "label": "Like", "size": [40, 40]}, "on": on_all()})
}

#[gpui::test]
fn a_click_and_enter_toggle_a_state_machine_button(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, like(), json!({}));
    assert_eq!(state(&view, cx), "off");
    let first = log.take();
    assert_eq!(states_of(&first), vec!["off"], "the initial state reports");
    assert!(events_of(&first).contains(&"visible".to_string()), "on.visible on the first paint: {:?}", events_of(&first));
    let info = view.read_with(cx, |v, _| v.accessible_info("m")).unwrap();
    assert_eq!(info.role, gpui::Role::Button);
    assert_eq!(info.label.as_deref(), Some("Like"));
    // `off` (frames 0..10 at 30 fps) plays out: `complete` once.
    advance(&view, cx, 500);
    assert_eq!(events_of(&log.take()), vec!["complete"]);
    advance(&view, cx, 500);
    assert!(log.take().is_empty(), "complete fires once per play");

    let b = view.read_with(cx, |v, _| v.painted_bounds("m")).unwrap();
    cx.simulate_mouse_move(b.center(), None, Modifiers::default());
    draw(cx);
    assert_eq!(events_of(&log.take()), vec!["hover"]);
    cx.simulate_mouse_down(b.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(b.center(), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(state(&view, cx), "on");
    let a = log.take();
    assert_eq!(states_of(&a), vec!["on"]);
    assert!(a.iter().any(|e| e.event == "press" && e.name == "pressed"), "on.press: {:?}", events_of(&a));
    // The clock restarted at the new state's first frame.
    assert_eq!(view.read_with(cx, |v, _| v.motion_frame("m")), Some(10.0));
    advance(&view, cx, 100);
    let f = view.read_with(cx, |v, _| v.motion_frame("m")).unwrap();
    assert!((f - 13.0).abs() < 0.01, "plays `on` from 10: {f}");

    // Enter on the focused button: the same click.
    let handled = cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.focus_next(false, window, cx);
            v.handle_key("enter", false, None, window, cx)
        })
    });
    draw(cx);
    assert!(handled);
    assert_eq!(state(&view, cx), "off");
    let a = log.take();
    assert_eq!(states_of(&a), vec!["off"]);
    assert!(a.iter().any(|e| e.event == "press"));
}

#[gpui::test]
fn hovering_plays_in_and_out_and_completes(cx: &mut TestAppContext) {
    let f = fixture();
    let machine = f["cases"].as_array().unwrap().iter().find(|c| c["id"] == "hover-in-out").unwrap()["machine"].clone();
    let motion = json!({"id": "m", "component": "Motion", "props": {"lottie": f["lottie"], "stateMachine": machine, "size": [40, 40]}, "on": on_all()});
    let (view, log, cx) = surface(cx, motion, json!({}));
    assert_eq!(state(&view, cx), "idle");
    log.take();
    let b = view.read_with(cx, |v, _| v.painted_bounds("m")).unwrap();
    cx.simulate_mouse_move(b.center(), None, Modifiers::default());
    draw(cx);
    assert_eq!(state(&view, cx), "in");
    assert_eq!(events_of(&log.take()), vec!["hover", "state"]);
    advance(&view, cx, 600);
    assert_eq!(state(&view, cx), "in", "`in` waits for the pointer to leave");
    assert_eq!(events_of(&log.take()), vec!["complete"]);
    cx.simulate_mouse_move(point(px(470.0), px(400.0)), None, Modifiers::default());
    draw(cx);
    assert_eq!(state(&view, cx), "out");
    assert_eq!(states_of(&log.take()), vec!["out"]);
    advance(&view, cx, 600);
    assert_eq!(state(&view, cx), "idle", "`out` played out → idle");
    let a = log.take();
    assert_eq!(events_of(&a), vec!["complete", "state"]);
    assert_eq!(states_of(&a), vec!["idle"]);
}

/// The styleguide's `interactive` specimens (web + desktop): the bar's
/// states follow the bound `inputs.level` the Slider writes.
#[gpui::test]
fn the_styleguide_level_bar_follows_its_bound_input(cx: &mut TestAppContext) {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/fixtures/motion.specimens.json");
    let doc: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("exponential"), mode: eui_core::theme::Mode::Dark, ..Default::default() };
    let tree = doc["interactive"].clone();
    let data = doc["data"].clone();
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            v.set_pause_motion_in_background(false);
            for (k, val) in data.as_object().unwrap() {
                v.set_data(&format!("/{k}"), Some(val.clone()), cx).unwrap();
            }
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
        });
        Pane { view }
    });
    let view = pane.read_with(cx, |p, _| p.view.clone());
    draw(cx);
    let at = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.motion_state(id)).unwrap();
    assert_eq!(at(&view, cx, "sg_motion_level"), "low");
    assert_eq!(at(&view, cx, "sg_motion_like"), "off");
    for (level, want) in [(60, "high"), (100, "done"), (20, "low")] {
        view.update(cx, |v, cx| v.set_data("/meter/level", Some(json!(level)), cx).unwrap());
        draw(cx);
        assert_eq!(at(&view, cx, "sg_motion_level"), want, "level {level}");
    }
    view.update(cx, |v, cx| v.motion_interact("sg_motion_like", &Interaction::Click { layer: None }, cx));
    draw(cx);
    assert_eq!(at(&view, cx, "sg_motion_like"), "on");
}
