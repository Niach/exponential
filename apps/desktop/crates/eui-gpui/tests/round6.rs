//! Round 6 (`packages/exponential-ui/docs/round-6-contract.md`, EXP-1495)
//! through the painter in a headless gpui window: the Carousel `counter`
//! line (EXP-1493: the root's first flow child ABOVE the track, on the
//! start edge, mirrored under RTL, plain muted text, hidden from AT) and
//! the `thumbnails` strip
//! (one 52 x 52 `thumbnail` per page, the `selected` recipe, a press goes to
//! its page = `change`), and the two new macros (Outline, Mosaic) painting
//! from Box / Text / Image: the Box `aspectRatio`, `position: absolute` +
//! `inset` / `insetInlineStart`, Image `fit: cover` clipped in a rounded
//! box, the hover opacity on a Mosaic tile, press → `select`.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, App, Entity, Modifiers, TestAppContext, VisualTestContext};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }
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

fn mounted(cx: &mut TestAppContext, mut tree: Value, log: Recorder, rtl: bool) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    if rtl {
        tree["style"]["direction"] = json!("rtl");
    }
    cx.update(gpui_component::init);
    let host = Rc::new(log);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.record_bounds(true);
        });
        Pane { view }
    });
    draw(vcx);
    draw(vcx);
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, vcx)
}

fn surface(cx: &mut TestAppContext, tree: Value) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    let log = Recorder::default();
    let (view, vcx) = mounted(cx, tree, log.clone(), false);
    (view, log, vcx)
}

fn frame(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> eui_core::surface::Frame {
    view.read_with(cx, |v, _| v.frame(v.index_of(id).unwrap_or_else(|| panic!("no {id}"))).unwrap())
}

fn painted(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> gpui::Bounds<gpui::Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} not painted"))
}

fn style(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> eui_gpui::paint::PaintStyle {
    view.read_with(cx, |v, _| v.painted_style(id)).unwrap_or_else(|| panic!("{id} has no painted style"))
}

fn text_of(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> String {
    view.read_with(cx, |v, _| v.surface().layout_node(v.index_of(id).unwrap()).and_then(|n| n.props.get("text").and_then(Value::as_str).map(str::to_string))).unwrap_or_default()
}

fn states(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> Vec<String> {
    view.read_with(cx, |v, _| {
        let mut s: Vec<String> = v.surface().layout_node(v.index_of(id).unwrap()).and_then(|n| n.part_query.as_ref().map(|q| q.states.clone())).unwrap_or_default();
        s.extend(v.node_states(id));
        s
    })
}

fn exists(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> bool {
    view.read_with(cx, |v, _| v.placed_nodes().iter().any(|n| n.id == id && !n.removed))
}

fn specimen(id: &str) -> Value {
    let doc: Value = serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/specimens.json")).unwrap();
    doc["specimens"].as_array().unwrap().iter().find(|s| s["id"] == id).unwrap_or_else(|| panic!("no specimen {id}"))["node"].clone()
}

fn hex(c: gpui::Hsla) -> String {
    let r = gpui::Rgba::from(c);
    format!("#{:02x}{:02x}{:02x}{:02x}", (r.r * 255.0).round() as u8, (r.g * 255.0).round() as u8, (r.b * 255.0).round() as u8, (r.a * 255.0).round() as u8)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.6
}

fn carousel(counter: bool, thumbnails: Value) -> Value {
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": 480}, "children": [
        {"id": "deck", "component": "Carousel", "props": {"counter": counter, "thumbnails": thumbnails}, "on": {"change": {"event": {"name": "paged", "context": {"page": {"path": "page"}}}}}, "children": [
            {"id": "p1", "component": "Box", "style": {"height": 120, "backgroundColor": "$color.muted"}},
            {"id": "p2", "component": "Box", "style": {"height": 120}},
            {"id": "p3", "component": "Box", "style": {"height": 120}}
        ]}
    ]})
}

#[gpui::test]
fn the_carousel_counter_paints_its_own_line_above_the_track(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, carousel(true, json!(null)));
    let (deck, track, line) = (frame(&view, cx, "deck"), frame(&view, cx, "deck.page"), frame(&view, cx, "deck.counter"));
    assert!(near(line.x, deck.x) && near(line.y, deck.y), "the root's first flow child, on the start edge: {line:?} in {deck:?}");
    assert!(near(line.h, 20.0), "a 20 px line: {line:?}");
    assert!(near(track.y - (line.y + line.h), 8.0), "the root's gap above the track: {line:?} {track:?}");
    assert!(line.w < deck.w, "hugs its text: {line:?}");
    assert_eq!(text_of(&view, cx, "deck.counter"), "1 / 3");
    let (b, t) = (painted(&view, cx, "deck.counter"), painted(&view, cx, "deck.page"));
    assert!(b.origin.y + b.size.height <= t.origin.y, "painted above the track, never over it");
    let s = style(&view, cx, "deck.counter");
    assert!(s.bg.is_none_or(|c| c.a == 0.0), "no fill (the pill is gone): {:?}", s.bg);
    assert!(s.radii.iter().all(|r| *r == 0.0) && s.padding.iter().all(|p| *p == 0.0), "no pill chrome: {:?} {:?}", s.radii, s.padding);
    assert_eq!(s.color.map(hex).as_deref(), Some("#737373ff"), "$color.mutedForeground (neutral light)");
    assert!(view.read_with(cx, |v, _| v.accessible_info("deck.counter")).is_none(), "hidden from AT");
}

#[gpui::test]
fn the_counter_mirrors_under_rtl(cx: &mut TestAppContext) {
    let (view, cx) = mounted(cx, carousel(true, json!(null)), Recorder::default(), true);
    let (deck, line) = (frame(&view, cx, "deck"), frame(&view, cx, "deck.counter"));
    assert!(near(deck.x + deck.w, line.x + line.w), "the right edge under RTL: {line:?} in {deck:?}");
}

#[gpui::test]
fn each_dot_paints_its_own_indicator_recipe_the_current_one_primary(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, carousel(false, json!(null)));
    let fills: Vec<Option<String>> = view.read_with(cx, |v, _| (0..3).map(|i| v.painted_fill(&format!("deck.indicator.dot.{i}")).flatten().map(hex)).collect());
    assert_eq!(fills, [Some("#171717ff".to_string()), Some("#e5e5e5ff".to_string()), Some("#e5e5e5ff".to_string())], "page 1 = $color.primary, the others $color.border");
    let d = painted(&view, cx, "deck.indicator.dot.0");
    assert!(near(f32::from(d.size.width), 8.0) && near(f32::from(d.size.height), 8.0), "8 x 8: {d:?}");
}

#[gpui::test]
fn the_thumbnail_strip_paints_one_square_per_page_and_a_press_goes_to_its_page(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, carousel(true, json!(["https://example.com/a.png", "", "https://example.com/c.png", "https://example.com/ignored.png"])));
    let (track, strip, controls) = (frame(&view, cx, "deck.page"), frame(&view, cx, "deck.thumbnails"), frame(&view, cx, "deck.controls"));
    assert!(strip.y >= track.y + track.h - 0.5 && controls.y >= strip.y + strip.h - 0.5, "track, strip, controls: {track:?} {strip:?} {controls:?}");
    for i in 0..3 {
        let f = frame(&view, cx, &format!("deck.thumbnail.{i}"));
        assert_eq!((f.w, f.h), (52.0, 52.0), "thumbnail {i}");
        painted(&view, cx, &format!("deck.thumbnail.{i}"));
    }
    assert!(!exists(&view, cx, "deck.thumbnail.3"), "URLs past the page count are ignored");
    assert!(exists(&view, cx, "deck.thumbnailImage.0") && exists(&view, cx, "deck.thumbnailImage.2"));
    assert!(!exists(&view, cx, "deck.thumbnailImage.1"), "an empty URL = the empty muted square");
    let img = frame(&view, cx, "deck.thumbnailImage.0");
    let t0 = frame(&view, cx, "deck.thumbnail.0");
    assert!(img.x >= t0.x && img.y >= t0.y && img.x + img.w <= t0.x + t0.w + 0.5 && img.y + img.h <= t0.y + t0.h + 0.5, "the picture inside the border: {img:?} in {t0:?}");
    painted(&view, cx, "deck.thumbnailImage.0");
    let (sel, other) = (style(&view, cx, "deck.thumbnail.0"), style(&view, cx, "deck.thumbnail.1"));
    assert_eq!(sel.border, [2.0; 4], "a 2 px border");
    assert_eq!(sel.border_color.map(hex).as_deref(), Some("#0a0a0aff"), "the selected border = foreground");
    assert!(other.border_color.is_none_or(|c| c.a == 0.0), "the others' border is transparent: {:?}", other.border_color);
    assert!(sel.opacity.is_none_or(|o| o >= 0.99), "selected = opaque: {:?}", sel.opacity);
    assert!(other.opacity.is_some_and(|o| (o - 0.7).abs() < 0.01), "the others at 0.7: {:?}", other.opacity);
    assert!(other.bg.is_some(), "the muted fill");
    let info = view.read_with(cx, |v, _| v.accessible_info("deck.thumbnail.1")).expect("a button");
    assert_eq!((info.role, info.label.as_deref()), (gpui::Role::Button, Some("Page 2 of 3")));
    // A click on the third square goes to page 3.
    let b = painted(&view, cx, "deck.thumbnail.2");
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
    draw(cx);
    let fired: Vec<(String, Value)> = log.actions.borrow().iter().map(|a| (a.name.clone(), a.context.clone())).collect();
    assert!(fired.iter().any(|(n, c)| n == "paged" && c["page"] == json!(2)), "change {{page: 2}}: {fired:?}");
    assert_eq!(text_of(&view, cx, "deck.counter"), "3 / 3");
    assert!(states(&view, cx, "deck.thumbnail.2").iter().any(|s| s == "selected"), "the third square is now selected");
    assert!(!states(&view, cx, "deck.thumbnail.0").iter().any(|s| s == "selected"));
    view.update(cx, |v, cx| v.advance_clock(std::time::Duration::from_secs(1), cx));
    draw(cx);
    assert_eq!(style(&view, cx, "deck.thumbnail.2").border_color.map(hex).as_deref(), Some("#0a0a0aff"), "the new selection repaints its border");
    let painted_page = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == "p3").map(|n| !n.hidden));
    assert_eq!(painted_page, Some(true), "the third page shows");
}

#[gpui::test]
fn a_single_page_has_no_counter_and_no_strip(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "deck", "component": "Carousel", "props": {"counter": true, "thumbnails": ["https://example.com/a.png"]}, "children": [{"id": "p1", "component": "Box", "style": {"height": 80}}]}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    assert!(!exists(&view, cx, "deck.counter") && !exists(&view, cx, "deck.thumbnails"));
}

/// Outline: the caption, one 32 px row per item, the 2 px bar on the active
/// row only (absolute, inset-inline-start 0), the count, press → select.
#[gpui::test]
fn the_outline_specimen_paints_rows_with_the_active_bar_and_selects_on_press(cx: &mut TestAppContext) {
    let mut tree = specimen("exponential-ui-outline");
    tree["children"][0]["children"][1]["on"] = json!({"select": {"event": {"name": "jump", "context": {"id": {"path": "id"}}}}});
    let (view, log, cx) = surface(cx, tree);
    let unknown = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| !n.removed && n.component == "Unknown").count());
    assert_eq!(unknown, 0);
    let rows: Vec<String> = (0..3).map(|i| format!("c0-outline.row.{i}")).collect();
    for r in &rows {
        assert!(near(frame(&view, cx, r).h, 32.0), "{r}: $control.rowCompact");
        painted(&view, cx, r);
    }
    let bars: Vec<String> = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| !n.removed && n.id.starts_with("c0-outline.") && n.id.ends_with(".bar")).map(|n| n.id.clone()).collect());
    assert_eq!(bars, vec!["c0-outline.row.1.bar".to_string()], "one bar, on the active row");
    let (bar, active) = (frame(&view, cx, &bars[0]), frame(&view, cx, &rows[1]));
    assert!(near(bar.w, 2.0) && near(bar.x, active.x) && near(bar.y, active.y) && near(bar.h, active.h), "2 px at the start edge, full height: {bar:?} in {active:?}");
    painted(&view, cx, &bars[0]);
    assert!(style(&view, cx, &bars[0]).bg.is_some(), "the bar paints its fill");
    let b = painted(&view, cx, &rows[2]);
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
    let fired: Vec<(String, Value)> = log.actions.borrow().iter().map(|a| (a.name.clone(), a.context.clone())).collect();
    assert!(fired.iter().any(|(n, c)| n == "jump" && c["id"] == json!("part-b")), "select {{id: part-b}}: {fired:?}");
}

/// Mosaic: the 1.6 aspect root, the lead at 60 %, the side column, the
/// `+N` overlay over the bottom tile (absolute inset 0), each picture an
/// Image `fit: cover` clipped in its rounded tile, press → select {index}.
#[gpui::test]
fn the_mosaic_specimen_paints_tiles_and_the_more_overlay_and_selects_on_press(cx: &mut TestAppContext) {
    let mut tree = specimen("exponential-ui-mosaic");
    tree["children"][0]["children"][1]["on"] = json!({"select": {"event": {"name": "open", "context": {"index": {"path": "index"}}}}});
    let (view, log, cx) = surface(cx, tree);
    let unknown = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| !n.removed && n.component == "Unknown").count());
    assert_eq!(unknown, 0);
    let root = frame(&view, cx, "c0-mosaic");
    assert!(root.w > 0.0 && near(root.w / root.h, 1.6), "aspectRatio 1.6: {root:?}");
    let (lead, side, top, bottom) = (frame(&view, cx, "c0-mosaic.lead"), frame(&view, cx, "c0-mosaic.side"), frame(&view, cx, "c0-mosaic.side.top"), frame(&view, cx, "c0-mosaic.side.bottom"));
    assert!(near(lead.w, root.w * 0.6) && near(lead.h, root.h), "the lead = flexBasis 60 %, full height: {lead:?}");
    assert!(near(side.x, lead.x + lead.w + 10.0) && near(side.x + side.w, root.x + root.w), "the side column after a 10 px gap: {side:?}");
    assert!(near(top.h, bottom.h) && near(bottom.y, top.y + top.h + 10.0), "two equal halves, 10 px apart: {top:?} {bottom:?}");
    for tile in ["c0-mosaic.lead", "c0-mosaic.side.top", "c0-mosaic.side.bottom"] {
        let (t, img) = (frame(&view, cx, tile), frame(&view, cx, &format!("{tile}.image")));
        // Absolute inset 0 inside the hairline border: the picture fills the tile.
        assert!(near(img.x, t.x + 1.0) && near(img.y, t.y + 1.0) && near(img.w, t.w - 2.0) && near(img.h, t.h - 2.0), "{tile}: the picture covers the tile: {img:?} in {t:?}");
        let st = style(&view, cx, tile);
        assert_eq!(st.radii, [10.0; 4], "{tile}: $radius.lg");
        assert!(st.clip_x && st.clip_y, "{tile}: clips its picture (rounded corners inherited by the Image)");
        assert!(st.bg.is_some(), "{tile}: the muted fill under the picture");
        painted(&view, cx, &format!("{tile}.image"));
        let fit = view.read_with(cx, |v, _| v.surface().layout_node(v.index_of(&format!("{tile}.image")).unwrap()).and_then(|n| n.props.get("fit").cloned()));
        assert_eq!(fit, Some(json!("cover")), "{tile}");
        assert!(view.read_with(cx, |v, _| v.accessible_info(&format!("{tile}.image"))).is_none(), "{tile}: the picture is hidden from AT");
    }
    let lead_info = view.read_with(cx, |v, _| v.accessible_info("c0-mosaic.lead")).expect("a button");
    assert_eq!((lead_info.role, lead_info.label.as_deref()), (gpui::Role::Button, Some("Editor")), "a tile = a button named by its alt");
    let more_info = view.read_with(cx, |v, _| v.accessible_info("c0-mosaic.side.bottom")).expect("a button");
    assert_eq!(more_info.label.as_deref(), Some("Data, 1 more"), "$string.morePicturesLabel");
    // The `+N` overlay: absolute inset 0 over the bottom tile, the scrim, the label centred.
    let (more, label) = (frame(&view, cx, "c0-mosaic.side.bottom.more"), frame(&view, cx, "c0-mosaic.side.bottom.more.moreLabel"));
    assert!(near(more.w, bottom.w - 2.0) && near(more.h, bottom.h - 2.0), "inset 0: {more:?}");
    assert!(near(label.x + label.w / 2.0, more.x + more.w / 2.0) && near(label.y + label.h / 2.0, more.y + more.h / 2.0), "the count centred: {label:?} in {more:?}");
    assert_eq!(style(&view, cx, "c0-mosaic.side.bottom.more").bg.map(hex).as_deref(), Some("#0000008c"));
    assert_eq!(text_of(&view, cx, "c0-mosaic.side.bottom.more.moreLabel"), "+1");
    assert!(view.read_with(cx, |v, _| v.accessible_info("c0-mosaic.side.bottom.more")).is_none(), "the overlay is hidden from AT");
    // Hover: the tile fades to 0.92.
    assert!(style(&view, cx, "c0-mosaic.side.top").opacity.is_none_or(|o| o >= 0.99));
    let b = painted(&view, cx, "c0-mosaic.side.top");
    cx.simulate_mouse_move(b.center(), None, Modifiers::default());
    draw(cx);
    view.update(cx, |v, cx| v.advance_clock(std::time::Duration::from_secs(1), cx));
    draw(cx);
    let hovered = style(&view, cx, "c0-mosaic.side.top").opacity;
    assert!(hovered.is_some_and(|o| (o - 0.92).abs() < 0.01), "hover opacity 0.92: {hovered:?}");
    // Press → select {index}.
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
    let b = painted(&view, cx, "c0-mosaic.side.bottom");
    cx.simulate_click(b.center(), Modifiers::default());
    draw(cx);
    let fired: Vec<(String, Value)> = log.actions.borrow().iter().map(|a| (a.name.clone(), a.context.clone())).collect();
    let picked: Vec<Value> = fired.iter().filter(|(n, _)| n == "open").map(|(_, c)| c["index"].clone()).collect();
    assert_eq!(picked, vec![json!(1), json!(2)], "select {{index}}: {fired:?}");
}

#[gpui::test]
fn the_outline_reads_as_navigation_with_link_rows(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, specimen("exponential-ui-outline"));
    let info = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.accessible_info(id));
    let root = info(&view, cx, "c0-outline").expect("navigation");
    assert_eq!((root.role, root.label.as_deref()), (gpui::Role::Navigation, Some("Outline")));
    let row = info(&view, cx, "c0-outline.row.1").expect("a link");
    assert_eq!(row.role, gpui::Role::Link);
    assert!(row.label.as_deref().is_some_and(|l| l.starts_with("Part A: editor-first creation")), "{:?}", row.label);
    assert!(info(&view, cx, "c0-outline.caption").is_none(), "the caption is hidden from AT");
    assert!(info(&view, cx, "c0-outline.row.1.count").is_none(), "the count is hidden from AT");
    assert_eq!(text_of(&view, cx, "c0-outline.row.1.count"), "· 5");
    assert!(!view.read_with(cx, |v, _| v.placed_nodes().iter().any(|n| n.id == "c0-outline.row.0.count")), "no count = no count text");
}

/// The current thumbnail scrolls into view inside the STRIP (round 6 §3):
/// on the first pass and on every page change.
#[gpui::test]
fn the_strip_scrolls_the_current_thumbnail_into_view(cx: &mut TestAppContext) {
    let pages: Vec<Value> = (0..12).map(|i| json!({"id": format!("p{i}"), "component": "Box", "style": {"height": 60}})).collect();
    let urls: Vec<Value> = (0..12).map(|i| json!(format!("https://example.com/{i}.png"))).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": 240}, "children": [
        {"id": "deck", "component": "Carousel", "props": {"page": 9, "thumbnails": urls}, "children": pages}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    let visible = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, page: usize| {
        let (strip, thumb) = (painted(view, cx, "deck.thumbnails"), painted(view, cx, &format!("deck.thumbnail.{page}")));
        thumb.origin.x >= strip.origin.x - gpui::px(0.5) && thumb.origin.x + thumb.size.width <= strip.origin.x + strip.size.width + gpui::px(0.5)
    };
    let offset = view.read_with(cx, |v, _| v.surface().scroll_offset("deck.thumbnails"));
    assert!(offset.0 > 0.0, "the strip scrolled to page 10 on the first pass: {offset:?}");
    assert!(visible(&view, cx, 9), "thumbnail 10 is in view");
    for _ in 0..2 {
        let next = view.read_with(cx, |v, _| v.index_of("deck.next").unwrap());
        cx.update(|window, cx| view.update(cx, |v, cx| v.press(next, window, cx)));
        draw(cx);
        draw(cx);
    }
    assert!(visible(&view, cx, 11), "the last thumbnail followed the page");
    // Back to the first page: the strip scrolls back to the start.
    for _ in 0..11 {
        let prev = view.read_with(cx, |v, _| v.index_of("deck.previous").unwrap());
        cx.update(|window, cx| view.update(cx, |v, cx| v.press(prev, window, cx)));
        draw(cx);
    }
    draw(cx);
    assert!(visible(&view, cx, 0), "the first thumbnail back in view");
    assert_eq!(view.read_with(cx, |v, _| v.surface().scroll_offset("deck.thumbnails")).0, 0.0);
}
