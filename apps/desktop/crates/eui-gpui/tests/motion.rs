//! EXP-1604: the Motion native in the gpui painter.
//!
//! - Pixel parity: every case × frame of `fixtures/motion-frames.json`
//!   through the painter's own path ([`MotionPlayer`] → the BGRA
//!   `RenderImage` gpui receives) at 160×160 equals
//!   `conformance/motion/parity/<id>@<frame>.png` within 2/255 per channel
//!   on ≥ 99 % of pixels (README › "Painting a Motion" §7).
//! - The playback rules in a headless window: `progress` scrubs, the clock
//!   plays and loops, `play: false` freezes, reduced motion and
//!   `autoplay: false` show the poster, at most 8 play per surface, the box
//!   (composition px, shrinking with its aspect), a11y, a broken source
//!   paints nothing and reports.

use std::path::PathBuf;
use std::time::Duration;

use eui_core::NestedNode;
use eui_gpui::motion::{Fit, MotionPlayer};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, Entity, TestAppContext, VisualTestContext};
use serde_json::{json, Map, Value};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

#[test]
fn every_parity_frame_matches_the_raster_reference() {
    let root = repo().join("packages/exponential-ui");
    let frames: Value = serde_json::from_slice(&std::fs::read(root.join("fixtures/motion-frames.json")).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;
    for c in frames["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let mut player = MotionPlayer::load(&c["lottie"], &Map::new(), &Map::new()).unwrap_or_else(|e| panic!("{id}: {e}"));
        for f in c["frames"].as_array().unwrap() {
            let frame = f["frame"].as_f64().unwrap();
            let reference = tiny_skia::Pixmap::load_png(root.join(format!("conformance/motion/parity/{id}@{frame}.png"))).unwrap_or_else(|e| panic!("{id}@{frame}: {e}"));
            let mut replaced = None;
            let image = player.render_image(frame, 160, 160, Fit::Contain, &mut replaced).expect("an image");
            let bgra = image.as_bytes(0).unwrap();
            let want = eui_gpui::motion::straight_rgba(&reference);
            let total = want.len() / 4;
            let close = (0..total)
                .filter(|i| {
                    let (g, w) = (&bgra[i * 4..i * 4 + 4], &want[i * 4..i * 4 + 4]);
                    // Fully transparent pixels carry no colour.
                    (g[3] == 0 && w[3] == 0) || [(g[2], w[0]), (g[1], w[1]), (g[0], w[2]), (g[3], w[3])].iter().all(|(a, b)| a.abs_diff(*b) <= 2)
                })
                .count();
            checked += 1;
            let share = close as f64 / total as f64;
            if share < 0.99 {
                failures.push(format!("{id}@{frame}: {:.2} %", share * 100.0));
            }
        }
    }
    assert!(checked >= 30, "only {checked} parity frames");
    assert!(failures.is_empty(), "gpui Motion frames off the raster reference: {failures:?}");
}

/// The 600×400 @2x paint budget (≤ 4 ms per frame, release): measured and
/// printed; asserted only loosely (debug builds and shared runners).
#[test]
fn a_frame_paints_within_the_budget() {
    let lottie = eui_motion::dialect::compile(&spinner_doc()).unwrap();
    let mut player = MotionPlayer::load(&lottie, &Map::new(), &Map::new()).unwrap();
    let mut worst = 0u64;
    let mut sum = 0u64;
    for i in 0..60 {
        let mut replaced = None;
        player.render_image(i as f64, 1200, 800, Fit::Contain, &mut replaced).unwrap();
        worst = worst.max(player.last_paint_us);
        sum += player.last_paint_us;
    }
    eprintln!("Motion paint 1200×800 px (600×400 @2x): mean {} µs, worst {worst} µs", sum / 60);
    // Debug builds run ~10× slower than release; the release budget is 4 ms.
    let cap = if cfg!(debug_assertions) { 200_000 } else { 4_000 };
    assert!(sum / 60 < cap, "mean paint {} µs over {cap} µs", sum / 60);
}

/// The catalog example: a 48×48 ring that turns once a second.
fn spinner_doc() -> Value {
    json!({"size": [48, 48], "duration": 1000, "layers": [{"type": "shape", "transform": {"position": [24, 24], "rotation": {"keys": [{"t": 0, "v": 0}, {"t": 1000, "v": 360}]}}, "shapes": [
        {"type": "ellipse", "size": [36, 36]}, {"type": "trim", "end": 70}, {"type": "stroke", "color": "$color.primary", "width": 4}
    ]}]})
}

fn motion(id: &str, extra: Value) -> Value {
    let mut props = spinner_doc();
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        props[k] = v;
    }
    json!({"id": id, "component": "Motion", "props": props})
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

fn surface(cx: &mut TestAppContext, children: Vec<Value>, reduced: bool) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start", "width": 480}, "children": children});
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            if reduced {
                let mut s = eui_core::surface::SurfaceSettings::default();
                s.reduced_motion = true;
                v.surface_mut().set_settings(s);
            }
            v.set_pause_motion_in_background(false);
            v.record_bounds(true);
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
        });
        Pane { view }
    });
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    draw(vcx);
    (view, vcx)
}

fn frame_of(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> Option<f64> {
    view.read_with(cx, |v, _| v.motion_frame(id))
}

fn advance(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, ms: u64) {
    view.update(cx, |v, cx| v.advance_clock(Duration::from_millis(ms), cx));
    draw(cx);
}

#[gpui::test]
fn progress_scrubs_and_the_clock_plays(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx, vec![motion("scrub", json!({"progress": 0.25, "label": "Loading"})), motion("play", json!({}))], false);
    // 60 fps × 1 s: progress 0.25 = frame 15.
    assert_eq!(frame_of(&view, cx, "scrub"), Some(15.0));
    assert_eq!(frame_of(&view, cx, "play"), Some(0.0));
    advance(&view, cx, 500);
    assert_eq!(frame_of(&view, cx, "scrub"), Some(15.0), "progress never ticks");
    let f = frame_of(&view, cx, "play").unwrap();
    assert!((f - 30.0).abs() < 0.01, "half a second in: {f}");
    advance(&view, cx, 1000);
    let f = frame_of(&view, cx, "play").unwrap();
    assert!((f - 30.0).abs() < 0.01, "it loops: {f}");
    // The box: the composition's px; a11y: an img named by its label.
    let b = view.read_with(cx, |v, _| v.painted_bounds("scrub")).unwrap();
    assert_eq!((f32::from(b.size.width), f32::from(b.size.height)), (48.0, 48.0));
    let info = view.read_with(cx, |v, _| v.accessible_info("scrub")).unwrap();
    assert_eq!(info.role, gpui::Role::Image);
    assert_eq!(info.label.as_deref(), Some("Loading"));
    assert!(view.read_with(cx, |v, _| v.accessible_info("play")).is_none(), "decorative without a label");
    assert!(view.read_with(cx, |v, _| v.motion_paint_us("play")).is_some());
}

#[gpui::test]
fn play_false_freezes_and_the_poster_shows_when_it_should(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx, vec![motion("frozen", json!({"play": false})), motion("still", json!({"autoplay": false, "poster": 250})), motion("once", json!({"loop": false}))], false);
    advance(&view, cx, 400);
    assert_eq!(frame_of(&view, cx, "frozen"), Some(0.0));
    assert_eq!(frame_of(&view, cx, "still"), Some(15.0), "the poster (250 ms)");
    advance(&view, cx, 2000);
    assert_eq!(frame_of(&view, cx, "once"), Some(59.0), "a finite loop holds its last frame");
}

#[gpui::test]
fn reduced_motion_shows_the_poster(cx: &mut TestAppContext) {
    let (view, cx) = surface(cx, vec![motion("m", json!({"poster": 500}))], true);
    advance(&view, cx, 300);
    assert_eq!(frame_of(&view, cx, "m"), Some(30.0));
}

#[gpui::test]
fn at_most_eight_play_per_surface(cx: &mut TestAppContext) {
    let kids: Vec<Value> = (0..10).map(|i| motion(&format!("m{i}"), json!({"size": [16, 16]}))).collect();
    let (view, cx) = surface(cx, kids, false);
    advance(&view, cx, 250);
    for i in 0..10 {
        let f = frame_of(&view, cx, &format!("m{i}")).unwrap();
        if i < 8 {
            assert!((f - 15.0).abs() < 0.01, "m{i} plays: {f}");
        } else {
            assert_eq!(f, 0.0, "m{i} shows its poster");
        }
    }
}

#[gpui::test]
fn the_box_shrinks_with_its_aspect_and_a_broken_source_reports(cx: &mut TestAppContext) {
    let wide = motion("wide", json!({"size": [960, 480]}));
    let broken = json!({"id": "broken", "component": "Motion", "props": {"lottie": {"v": "5.7.0", "fr": 30, "ip": 0, "op": 10, "w": 10, "h": 10, "layers": [{"ty": 4, "ks": {"o": {"a": 0, "k": 100, "x": "time"}}, "shapes": []}]}}});
    let (view, cx) = surface(cx, vec![wide, broken], false);
    let b = view.read_with(cx, |v, _| v.painted_bounds("wide")).unwrap();
    assert_eq!((f32::from(b.size.width), f32::from(b.size.height)), (480.0, 240.0));
    let error = view.read_with(cx, |v, _| v.motion_error("broken"));
    assert!(error.is_some(), "an expression is refused");
    assert_eq!(frame_of(&view, cx, "broken"), None, "nothing painted");
}

/// The shared motion specimens (`packages/exponential-ui/fixtures/
/// motion.specimens.json`, web + desktop) compile, load and name only
/// colour tokens the exponential theme resolves.
#[test]
fn the_styleguide_specimens_compile_with_known_tokens() {
    let doc: Value = serde_json::from_slice(&std::fs::read(repo().join("packages/exponential-ui/fixtures/motion.specimens.json")).unwrap()).unwrap();
    let theme = eui_core::themes::builtin_theme("exponential").unwrap();
    let mut motions = Vec::new();
    fn walk(n: &Value, out: &mut Vec<Value>) {
        if n["component"] == "Motion" {
            out.push(n.clone());
        }
        for c in n["children"].as_array().into_iter().flatten() {
            walk(c, out);
        }
    }
    walk(&doc["cases"], &mut motions);
    walk(&doc["illustration"], &mut motions);
    walk(&doc["interactive"], &mut motions);
    assert_eq!(motions.len(), 6);
    for m in motions {
        let id = m["id"].as_str().unwrap();
        let props = m["props"].as_object().unwrap();
        let lottie = eui_gpui::motion::inline_lottie(props).unwrap_or_else(|e| panic!("{id}: {e}")).unwrap();
        for sid in lottie["slots"].as_object().into_iter().flatten().map(|(k, _)| k) {
            let token = sid.strip_prefix("$color.").unwrap();
            for mode in [eui_core::theme::Mode::Light, eui_core::theme::Mode::Dark] {
                assert!(theme.modes.get(mode).color.contains_key(token), "{id}: no `{token}` colour");
            }
        }
        let slots = eui_gpui::motion::theme_slots(&lottie, Some(&theme), eui_core::theme::Mode::Dark, None);
        let mut player = MotionPlayer::load(&lottie, &slots, props).unwrap_or_else(|e| panic!("{id}: {e}"));
        let mut replaced = None;
        assert!(player.render_image(10.0, 320, 240, Fit::Contain, &mut replaced).is_some());
    }
}
