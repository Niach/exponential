//! VAPP-99 round 2, the native painters' asks (VAPP-100) the core answers
//! for every painter: a leaf's display text for numbers, the host's `hover`
//! through recipes, the Stepper's step colours, the CodeBlock header's
//! language label, and the §7 heights (Chart, Slider, Badge) plus a
//! re-measure after a host load.

use std::collections::HashMap;

use exponential_ui::measure::{FixedMeasure, HeightRequest, Intrinsics, LeafRequest, Measure, TextStyle};
use exponential_ui::surface::{LayoutOutput, PlacedFrame, Surface, SurfaceOptions};
use exponential_ui::theme::Mode;
use exponential_ui::themes::builtin_theme;
use exponential_ui::types::NestedNode;
use serde_json::{json, Map, Value};

fn themed(tree: Value, theme: &str, mode: Mode) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let options = SurfaceOptions { theme: Some(builtin_theme(theme).expect("theme")), mode, ..SurfaceOptions::default() };
    let mut s = Surface::new("t", options);
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

fn color(s: &Surface, id: &str) -> (Option<String>, Option<String>) {
    let v = s.visual(s.index_of(id).unwrap_or_else(|| panic!("no node {id}"))).expect("visual");
    (v.background_color.clone(), v.color.clone())
}

#[test]
fn a_number_or_boolean_in_a_text_prop_shows_as_its_display_string() {
    let style = TextStyle::default();
    for (props, want) in [
        (json!({"text": 412}), "412"),
        (json!({"text": 1.5}), "1.5"),
        (json!({"label": true}), "true"),
        (json!({"text": "Ship"}), "Ship"),
        (json!({"text": null}), ""),
        (json!({"text": [1]}), ""),
        (json!({}), ""),
    ] {
        let props: Map<String, Value> = props.as_object().cloned().unwrap();
        let leaf = LeafRequest { index: 0, id: "t", component: "Text", part: None, owner_component: None, props: &props, text_style: &style, control: Default::default(), lines: None };
        assert_eq!(leaf.display_text(), want, "{props:?}");
    }
}

#[test]
fn every_theme_pairs_the_step_number_with_its_marker() {
    // The current step's number sat in the Text recipe's foreground on the
    // marker's primary fill (white on white in dark): it now keys on the
    // step `status` like the marker, and an upcoming label is muted.
    let tree = json!({"id": "s", "component": "Stepper", "props": {"steps": [{"label": "Account"}, {"label": "Team"}, {"label": "Done"}], "current": 1}});
    for theme in ["neutral", "exponential", "playful"] {
        for mode in [Mode::Light, Mode::Dark] {
            let mut s = themed(tree.clone(), theme, mode);
            s.set_viewport(600.0, 0.0, None);
            s.layout(&mut fixed());
            let t = builtin_theme(theme).unwrap();
            let palette = &t.modes.get(mode).color;
            let (current_bg, current_ink) = color(&s, "s.step.1.marker");
            let (_, number) = color(&s, "s.step.1.marker.number");
            assert_eq!(number, current_ink, "{theme} {mode:?}: the current number takes the marker's ink");
            assert_ne!(number, current_bg, "{theme} {mode:?}: never ink on the same fill");
            let (_, upcoming_ink) = color(&s, "s.step.2.marker");
            assert_eq!(color(&s, "s.step.2.marker.number").1, upcoming_ink, "{theme} {mode:?}: an upcoming number takes its marker's ink");
            assert_eq!(color(&s, "s.step.2.body.label").1.as_ref(), palette.get("mutedForeground"), "{theme} {mode:?}: an upcoming label is muted");
            assert_eq!(color(&s, "s.step.1.body.label").1.as_ref(), palette.get("foreground"), "{theme} {mode:?}");
        }
    }
}

#[test]
fn an_untitled_code_block_names_its_language_in_the_header() {
    let mut s = themed(
        json!({"id": "root", "component": "Box", "children": [
            {"id": "ts", "component": "CodeBlock", "props": {"code": "const a = 1", "language": "ts"}},
            {"id": "file", "component": "CodeBlock", "props": {"code": "const a = 1", "language": "ts", "title": "answer.ts"}},
            {"id": "plain", "component": "CodeBlock", "props": {"code": "a b"}},
            {"id": "bare", "component": "CodeBlock", "props": {"code": "a b", "language": "rust", "copyable": false}}
        ]}),
        "neutral",
        Mode::Dark,
    );
    s.set_viewport(600.0, 0.0, None);
    let out = s.layout(&mut fixed());
    let text = |s: &mut Surface, id: &str| s.nodes().into_iter().find(|n| n.id == id).map(|n| n.props.get("text").cloned().unwrap_or(Value::Null));
    assert_eq!(text(&mut s, "ts.title"), Some(json!("ts")), "the language beside the copy button");
    assert!(frame(&s, &out, "ts.title").w > 0.0);
    assert!(frame(&s, &out, "ts.copy").w > 0.0, "the copy action stays");
    assert_eq!(text(&mut s, "file.title"), Some(json!("answer.ts")), "a title (the file name) wins");
    assert_eq!(text(&mut s, "plain.title"), None, "plain names nothing");
    assert!(s.index_of("plain.spacer").is_some() && s.index_of("plain.copy").is_some());
    assert_eq!(text(&mut s, "bare.title"), Some(json!("rust")), "a header for the label alone");
    assert!(s.index_of("bare.copy").is_none());
    // The region keeps its built-in name; the label is not the title.
    let a11y = s.nodes().into_iter().find(|n| n.id == "ts").and_then(|n| n.accessibility).unwrap_or(Value::Null);
    assert_eq!(a11y["label"], json!("Code"));
}

#[test]
fn the_host_hover_resolves_recipes_and_stays_beside_other_states() {
    let mut s = themed(json!({"id": "pill", "component": "Pill", "props": {"label": "All", "pressable": true}}), "neutral", Mode::Light);
    s.set_viewport(300.0, 0.0, None);
    s.layout(&mut fixed());
    let rest = color(&s, "pill").0;
    let accent = builtin_theme("neutral").unwrap().modes.get(Mode::Light).color.get("accent").cloned();
    assert!(s.set_hover("pill", true));
    assert!(!s.set_hover("pill", true), "no change, no restyle");
    s.layout(&mut fixed());
    assert_eq!(color(&s, "pill").0, accent, "Pill/root `state: hover`");
    assert_eq!(s.host_states("pill"), ["hover".to_string()]);
    assert!(s.set_pressed(&["pill".into()]));
    assert_eq!(s.host_states("pill"), ["hover".to_string(), "pressed".to_string()], "pressing keeps the hover");
    assert!(s.set_hovered(&[]));
    assert!(s.set_pressed(&[]));
    s.layout(&mut fixed());
    assert_eq!(color(&s, "pill").0, rest);
    assert!(s.host_states("pill").is_empty());
}

/// A measurer with text 8 px per char, lines of the style's height, and a
/// Markdown whose picture "loads": `extra` px once loaded.
struct Loading {
    extra: f32,
}

impl Measure for Loading {
    fn measure_id(&self) -> u64 {
        7
    }
    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        leaves
            .iter()
            .map(|l| {
                if l.component == "Image" {
                    // The contract's max-content width; the picture's own
                    // height once it loaded (a 4:3 photo), none before.
                    let natural = if self.extra > 0.0 { 240.0 } else { 0.0 };
                    return Intrinsics { min_content_width: 0.0, max_content_width: 320.0, height_at_max_content: natural, baseline: None };
                }
                let text = l.display_text();
                let w = l.control.width.unwrap_or(8.0 * text.chars().count() as f32 + l.control.padding[1] + l.control.padding[3]);
                let pad = l.control.padding[0] + l.control.padding[2];
                let h = l.control.height.unwrap_or(l.text_style.line_height + pad) + if l.component == "Markdown" { self.extra } else { 0.0 };
                let min = if l.component == "Markdown" { 8.0 * text.split_whitespace().map(|w| w.chars().count()).max().unwrap_or(0) as f32 } else { w };
                Intrinsics { min_content_width: min, max_content_width: w, height_at_max_content: h, baseline: None }
            })
            .collect()
    }
    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        requests
            .iter()
            .map(|r| {
                let l = leaves.iter().find(|l| l.index == r.index).expect("leaf");
                let chars = l.display_text().chars().count() as f32;
                let lines = (chars * 8.0 / r.width.max(8.0)).ceil().max(1.0);
                lines * l.text_style.line_height + if l.component == "Markdown" { self.extra } else { 0.0 }
            })
            .collect()
    }
}

#[test]
fn contract_heights_hold_for_chart_slider_and_badge() {
    let mut s = themed(
        json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start"}, "children": [
            {"id": "chart", "component": "Chart", "props": {"kind": "bar", "title": "This week", "height": 160, "categories": ["Mon", "Tue"], "series": [{"name": "Runs", "values": [3, 5]}, {"name": "Merges", "values": [1, 2]}]}},
            {"id": "vol", "component": "Slider", "props": {"label": "Length", "min": 0, "max": 100, "value": 40}},
            {"id": "badge", "component": "Badge", "props": {"text": "3", "variant": "secondary"}}
        ]}),
        "neutral",
        Mode::Dark,
    );
    s.set_viewport(600.0, 0.0, None);
    let out = s.layout(&mut Loading { extra: 0.0 });
    // Chart: `height` = the WHOLE box (title and legend inside).
    assert_eq!(frame(&s, &out, "chart").h, 160.0);
    // Slider: header + $spacing.xs + a track row as tall as the thumb, the
    // rail centred in it.
    let t = builtin_theme("neutral").unwrap();
    let (thumb, xs) = (t.tokens.control["slider"] as f32, t.tokens.spacing["xs"] as f32);
    let (vol, header, track) = (frame(&s, &out, "vol"), frame(&s, &out, "vol.header"), frame(&s, &out, "vol.track"));
    assert_eq!(vol.h, header.h + xs + thumb, "slider {vol:?}");
    assert_eq!(track.h, 6.0, "the rail");
    assert_eq!(track.y - (header.y + header.h + xs), (thumb - 6.0) / 2.0, "centred in the thumb's row");
    // Badge: its content + 2 × paddingVertical, no control minimum.
    let (badge, label) = (frame(&s, &out, "badge"), frame(&s, &out, "badge.label"));
    assert_eq!(badge.h, label.h + 2.0 * (label.y - badge.y), "badge {badge:?} label {label:?}");
    assert!(badge.h < 24.0, "{badge:?}");
}

#[test]
fn a_loaded_picture_remeasures_its_markdown_and_never_resizes_an_image() {
    let mut s = themed(
        json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
            {"id": "md", "component": "Markdown", "props": {"text": "Ship it ![chart](https://example.com/c.png)"}},
            {"id": "img", "component": "Image", "props": {"src": "https://example.com/1.jpg", "alt": "One", "aspectRatio": 2}},
            {"id": "row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start"}, "children": [
                {"id": "thumb", "component": "Image", "props": {"src": "https://example.com/2.jpg", "alt": "Two", "aspectRatio": 2}}
            ]},
            {"id": "after", "component": "Text", "props": {"text": "below"}}
        ]}),
        "neutral",
        Mode::Light,
    );
    s.set_viewport(400.0, 0.0, None);
    let out = s.layout(&mut Loading { extra: 0.0 });
    let before = frame(&s, &out, "md").h;
    assert_eq!(frame(&s, &out, "img").h, 200.0, "stretched: width / ratio, the box never waits for the picture");
    let thumb = frame(&s, &out, "thumb");
    assert_eq!((thumb.w, thumb.h), (320.0, 160.0), "in a row: the host's width, the ratio's height");
    // The host's pictures loaded: the memo answers until the host says so.
    let mut loaded = Loading { extra: 120.0 };
    let stale = s.layout(&mut loaded);
    assert_eq!(frame(&s, &stale, "md").h, before);
    for id in ["md", "img", "thumb"] {
        assert!(s.mark_dirty(s.index_of(id).unwrap()));
    }
    let out = s.layout(&mut loaded);
    assert_eq!(frame(&s, &out, "md").h, before + 120.0, "the loaded height, at the final width");
    assert_eq!(frame(&s, &out, "img").h, 200.0, "the ratio still decides");
    let thumb = frame(&s, &out, "thumb");
    assert_eq!((thumb.w, thumb.h), (320.0, 160.0), "the natural 240 px never leaks into the box");
    assert_eq!(frame(&s, &out, "after").y, frame(&s, &out, "row").y + 160.0);
    // A resize asks the height again at the new width (never a narrower one's).
    s.set_viewport(120.0, 0.0, None);
    let narrow = s.layout(&mut loaded);
    let md = frame(&s, &narrow, "md");
    let chars = "Ship it ![chart](https://example.com/c.png)".chars().count() as f32;
    assert_eq!(md.h, (chars * 8.0 / md.w).ceil() * 20.0 + 120.0, "{md:?}");
}
