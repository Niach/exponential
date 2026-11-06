//! VAPP-99 round 2, the native painters' asks (VAPP-100) the core answers
//! for every painter: a leaf's display text for numbers, the host's `hover`
//! through recipes, the Stepper's step colours, the CodeBlock header's
//! language label, and the §7 heights (Chart, Slider, Badge) plus a
//! re-measure after a host load.

use std::collections::HashMap;

use eui_core::measure::{FixedMeasure, HeightRequest, Intrinsics, LeafRequest, Measure, TextStyle};
use eui_core::surface::{LayoutOutput, PlacedFrame, Surface, SurfaceOptions};
use eui_core::theme::Mode;
use eui_core::themes::builtin_theme;
use eui_core::types::NestedNode;
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
    for theme in ["neutral", "exponential", "playful", "orange"] {
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
    let mut s = themed(json!({"id": "pill", "component": "Chip", "props": {"label": "All", "pressable": true}}), "neutral", Mode::Light);
    s.set_viewport(300.0, 0.0, None);
    s.layout(&mut fixed());
    let rest = color(&s, "pill").0;
    let accent = builtin_theme("neutral").unwrap().modes.get(Mode::Light).color.get("accent").cloned();
    assert!(s.set_hover("pill", true));
    assert!(!s.set_hover("pill", true), "no change, no restyle");
    s.layout(&mut fixed());
    assert_eq!(color(&s, "pill").0, accent, "Chip/root `state: hover`");
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

/// #VAPP-286: the volume row. `icon` + `endLabel` put the track in a `row`
/// part (gap sm, centred): the iconSm glyph, the track growing from a zero
/// basis, the end label end-aligned at least `$control.buttonIcon` wide;
/// `showValue: false` prints no value, and without a label no header at all.
#[test]
fn slider_volume_row_icon_track_end_label() {
    let mut s = themed(
        json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
            {"id": "vol", "component": "Slider", "props": {"min": 0, "max": 100, "value": 34, "icon": "media-volume-up", "endLabel": "34%", "showValue": false}},
            {"id": "named", "component": "Slider", "props": {"label": "Volume", "min": 0, "max": 100, "value": 34, "showValue": false}},
            {"id": "plain", "component": "Slider", "props": {"label": "Volume", "min": 0, "max": 100, "value": 34}}
        ]}),
        "neutral",
        Mode::Dark,
    );
    s.set_viewport(400.0, 0.0, None);
    let out = s.layout(&mut Loading { extra: 0.0 });
    let t = builtin_theme("neutral").unwrap();
    let (thumb, sm, icon_sm, end_min) = (t.tokens.control["slider"] as f32, t.tokens.spacing["sm"] as f32, t.tokens.control["iconSm"] as f32, t.tokens.control["buttonIcon"] as f32);
    let has = |id: &str| s.index_of(id).is_some();
    // No label, no value: no header; the row is the whole control.
    assert!(!has("vol.header") && !has("vol.value"), "no header without a label and a value");
    let (vol, row, icon, track, end) = (frame(&s, &out, "vol"), frame(&s, &out, "vol.row"), frame(&s, &out, "vol.icon"), frame(&s, &out, "vol.track"), frame(&s, &out, "vol.end"));
    assert_eq!((row.x, row.w), (vol.x, vol.w), "the row spans the control");
    assert_eq!((icon.w, icon.h), (icon_sm, icon_sm), "iconSm");
    assert_eq!(icon.x, row.x, "the icon leads");
    assert_eq!(track.x, icon.x + icon.w + sm, "gap sm after the icon");
    assert!(end.w >= end_min, "the end label is at least buttonIcon wide: {end:?}");
    assert_eq!(end.x + end.w, row.x + row.w, "the end label ends the row");
    assert_eq!(end.x, track.x + track.w + sm, "gap sm before the end label");
    assert_eq!(track.h, 6.0, "the rail");
    let centre = |f: &PlacedFrame| f.y + f.h / 2.0;
    assert!((centre(&track) - centre(&row)).abs() < 0.01 && (centre(&icon) - centre(&row)).abs() < 0.01, "centred: {row:?} {icon:?} {track:?}");
    assert!(row.h >= thumb, "the row holds the thumb");
    // A label with showValue false: the header holds the label alone.
    assert!(has("named.label") && !has("named.value"), "showValue false hides the value");
    // The default still prints it, and no row without icon/endLabel.
    assert!(has("plain.value") && !has("plain.row"), "the default slider is unchanged");
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

/// #VAPP-277: Sidebar `rail` + `collapsed` = the icon rail: the column is
/// `$spacing.xl3` wide, each nav Row a `$control.buttonIcon` square centred
/// in it, its icon centred in the square, the `meta` count a badge
/// (`$control.iconSm` tall, at least as wide, primary fill) flush
/// in the square's top-end corner; the title is the
/// tooltip and the accessible name (`, <count>` with one). A bound
/// `collapsed` switches between the rail and the plain Rows at bind time.
#[test]
fn sidebar_rail_squares_icons_badges_and_tooltips() {
    let shell = |collapsed: Value| {
        json!({"id": "shell", "component": "Sidebar", "props": {"rail": true, "collapsed": collapsed, "collapseBelow": "never"},
            "slots": {"sidebar": {"id": "nav", "component": "Stack", "props": {"gap": "xs"}, "children": [
                {"id": "inbox", "component": "Row", "props": {"title": "Inbox", "icon": "nav-inbox", "meta": "3", "selected": true, "pressable": true}},
                {"id": "issues", "component": "Row", "props": {"title": "Issues", "icon": "nav-issues", "pressable": true}}
            ]}},
            "children": [{"id": "main", "component": "Text", "props": {"text": "Pick a board."}}]})
    };
    let t = builtin_theme("neutral").unwrap();
    let (rail_w, square, badge_h, inset) = (t.tokens.spacing["xl3"] as f32, t.tokens.control["buttonIcon"] as f32, t.tokens.control["iconSm"] as f32, t.tokens.spacing["none"] as f32);
    let mut s = themed(shell(json!(true)), "neutral", Mode::Dark);
    s.set_viewport(600.0, 400.0, None);
    let out = s.layout(&mut fixed());
    let column = frame(&s, &out, "shell.sidebar");
    assert_eq!(column.w, rail_w, "the rail is xl3 wide");
    for id in ["inbox", "issues"] {
        let row = frame(&s, &out, id);
        assert_eq!((row.w, row.h), (square, square), "{id}: a buttonIcon square");
        // Centred in the content box (the theme draws a hairline on the inner edge).
        assert!((row.x + row.w / 2.0 - (column.x + (column.w - 1.0) / 2.0)).abs() < 0.01, "{id}: centred in the rail {row:?} {column:?}");
        let icon = frame(&s, &out, &format!("{id}.tip.tile.tileIcon"));
        assert!((icon.x + icon.w / 2.0 - (row.x + row.w / 2.0)).abs() < 0.01 && (icon.y + icon.h / 2.0 - (row.y + row.h / 2.0)).abs() < 0.01, "{id}: the icon is centred");
        for part in ["icon", "body", "body.title", "meta"] {
            assert!(s.index_of(&format!("{id}.{part}")).is_none(), "{id}.{part} drops on the rail");
        }
    }
    let (row, badge) = (frame(&s, &out, "inbox"), frame(&s, &out, "inbox.tip.tile.badge"));
    assert_eq!(badge.h, badge_h, "the badge is iconSm tall");
    assert!(badge.w >= badge_h, "and at least as wide");
    assert_eq!((badge.y, badge.x + badge.w), (row.y + inset, row.x + row.w - inset), "flush in the top-end corner");
    assert!(s.index_of("issues.tip.tile.badge").is_none(), "no meta, no badge");
    assert_eq!(color(&s, "inbox.tip.tile.badge").0.as_deref(), Some(t.modes.dark.color["primary"].as_str()), "primary fill");
    assert_eq!(color(&s, "inbox.tip.tile.badge.count").1.as_deref(), Some(t.modes.dark.color["primaryForeground"].as_str()), "primary ink");
    let nodes = s.nodes();
    let a11y = |id: &str| nodes.iter().find(|n| n.id == id).and_then(|n| n.accessibility.clone());
    assert_eq!(a11y("inbox"), Some(json!({"role": "button", "selected": true, "label": "Inbox, 3"})), "the title (and count) is the name");
    assert_eq!(a11y("issues"), Some(json!({"role": "button", "selected": false, "label": "Issues"})));
    assert_eq!(a11y("inbox.tip.tile").and_then(|a| a.get("hidden").cloned()), Some(json!(true)), "the drawn tile stays out of the a11y tree");
    assert!(s.set_states("inbox.tip.anchor", vec!["hover".into()]));
    let out = s.layout(&mut fixed());
    let layer = out.layers.iter().find(|l| l.kind == "Tooltip").expect("hover opens the tooltip");
    assert_eq!(layer.owner, "inbox.tip");
    assert_eq!(s.nodes().iter().find(|n| n.id == "inbox.tip.label").map(|n| n.props["text"].clone()), Some(json!("Inbox")), "the tooltip is the title");

    // Bound: data decides between the rail and the plain Rows.
    let mut s = themed(shell(json!({"path": "/collapsed"})), "neutral", Mode::Dark);
    s.set_viewport(600.0, 400.0, None);
    s.set_data("/collapsed", Some(json!(true))).unwrap();
    let out = s.layout(&mut fixed());
    assert_eq!(frame(&s, &out, "shell.sidebar").w, rail_w);
    assert_eq!(frame(&s, &out, "inbox").w, square);
    assert!(s.index_of("inbox.body").is_none());
    s.set_data("/collapsed", Some(json!(false))).unwrap();
    let out = s.layout(&mut fixed());
    assert_eq!(frame(&s, &out, "shell.sidebar").w, 240.0, "expanded = the width");
    assert!(s.index_of("inbox.body.title").is_some() && s.index_of("inbox.tip").is_none(), "the plain Row");
    assert!(frame(&s, &out, "inbox").w > square, "the Row spans the column");
}

/// #VAPP-277: the rail tooltip points into the content (right of a left
/// rail, left of a right one, both mirrored under RTL); the Row is the ONE
/// focus stop and its keyboard focus opens the tooltip; the Row's slots and
/// a Section's header drop (the Section draws a full-width rule instead).
#[test]
fn sidebar_rail_tooltip_side_focus_slots_and_sections() {
    let shell = |side: &str, direction: &str| {
        json!({"id": "root", "component": "Box", "style": {"direction": direction, "display": "flex", "flexDirection": "column"}, "children": [
            {"id": "shell", "component": "Sidebar", "props": {"rail": true, "collapsed": true, "collapseBelow": "never", "side": side},
                "slots": {"sidebar": {"id": "nav", "component": "Stack", "props": {"gap": "xs"}, "children": [
                    {"id": "inbox", "component": "Row", "props": {"title": "Inbox", "icon": "nav-inbox", "pressable": true},
                     "slots": {"leading": {"id": "lead", "component": "Icon", "props": {"name": "nav-issues"}}, "trailing": {"id": "trail", "component": "Badge", "props": {"text": "New"}}}},
                    {"id": "more", "component": "Section", "props": {"title": "More", "collapsible": true, "open": false}, "children": [
                        {"id": "search", "component": "Row", "props": {"title": "Search", "icon": "nav-search", "pressable": true}}
                    ]}
                ]}},
                "children": [{"id": "main", "component": "Text", "props": {"text": "Pick a board."}}]}
        ]})
    };
    for (side, direction, expected) in [("left", "ltr", "right"), ("right", "ltr", "left"), ("left", "rtl", "left"), ("right", "rtl", "right")] {
        let mut s = themed(shell(side, direction), "neutral", Mode::Dark);
        s.set_viewport(600.0, 400.0, None);
        s.layout(&mut fixed());
        assert!(s.set_states("inbox.tip.anchor", vec!["hover".into()]));
        let out = s.layout(&mut fixed());
        let layer = out.layers.iter().find(|l| l.kind == "Tooltip").expect("hover opens the tooltip");
        assert_eq!(layer.position, expected, "{side} sidebar, {direction}");
    }
    let mut s = themed(shell("left", "ltr"), "neutral", Mode::Dark);
    s.set_viewport(600.0, 400.0, None);
    let out = s.layout(&mut fixed());
    for id in ["lead", "trail", "more.header", "more.header.title"] {
        assert!(s.index_of(id).is_none(), "{id} drops on the rail");
    }
    let (column, rule) = (frame(&s, &out, "shell.sidebar"), frame(&s, &out, "more.rule"));
    assert_eq!((rule.x, rule.h), (column.x, 1.0), "a hairline rule");
    assert_eq!(rule.w, column.w - 1.0, "across the rail (inside its hairline edge)");
    assert_eq!(frame(&s, &out, "search").w, 36.0, "a closed Section still shows its rows on the rail");
    // One focus stop per row: the Row; its keyboard focus opens the tooltip.
    let nodes = s.nodes();
    let pressables: Vec<&str> = nodes.iter().filter(|n| n.pressable).map(|n| n.id.as_str()).collect();
    assert_eq!(pressables, ["inbox", "search"], "the tooltip anchors add no stop");
    s.set_states("inbox", vec!["focus".into(), "focus-visible".into()]);
    let out = s.layout(&mut fixed());
    assert!(out.layers.iter().any(|l| l.owner == "inbox.tip"), "keyboard focus on the Row opens its tooltip");
    s.set_states("inbox", vec![]);
    assert!(s.layout(&mut fixed()).layers.is_empty(), "blur closes it");
    // A Tooltip around a Button inside a pressable Row is NOT hosted.
    let mut s = themed(json!({"id": "r", "component": "Row", "props": {"title": "Row", "pressable": true}, "slots": {"trailing": {"id": "tip", "component": "Tooltip", "props": {"content": "Copy"}, "children": [{"id": "b", "component": "Button", "props": {"label": "Copy"}}]}}}), "neutral", Mode::Dark);
    s.set_viewport(400.0, 300.0, None);
    s.layout(&mut fixed());
    s.set_states("r", vec!["focus".into(), "focus-visible".into()]);
    assert!(s.layout(&mut fixed()).layers.is_empty(), "the Button keeps its own tooltip focus");
}
