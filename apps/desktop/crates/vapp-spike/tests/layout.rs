use std::collections::HashMap;

use serde_json::json;
use vapp_spike::{bench, style, Frame, FixedMeasure, NodeSpec, PlacedNode, Surface, KITCHEN_SINK_JSON};

fn frames(json: &str, width: f32, pressed: &[&str]) -> (HashMap<String, PlacedNode>, Surface) {
    let mut s = Surface::new(json).expect("parse");
    s.set_rounding(false);
    s.set_viewport(width, 0.0);
    let pressed: Vec<String> = pressed.iter().map(|p| p.to_string()).collect();
    s.set_pressed(&pressed);
    let r = s.layout(&mut FixedMeasure);
    (r.nodes.into_iter().map(|n| (n.id.clone(), n)).collect(), s)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.51
}

fn f(m: &HashMap<String, PlacedNode>, id: &str) -> Frame {
    m.get(id).unwrap_or_else(|| panic!("no node {id}")).frame
}

#[test]
fn fixture_uses_only_whitelisted_keys() {
    let spec: NodeSpec = serde_json::from_str(KITCHEN_SINK_JSON).unwrap();
    fn walk(n: &NodeSpec, bad: &mut Vec<String>) {
        for k in style::unknown_keys(&n.style) {
            bad.push(format!("{}: {k}", n.id));
        }
        n.children.iter().for_each(|c| walk(c, bad));
    }
    let mut bad = Vec::new();
    walk(&spec, &mut bad);
    assert!(bad.is_empty(), "non-whitelisted style keys: {bad:?}");
}

#[test]
fn phone_width_stacks_the_grid() {
    let (m, _) = frames(KITCHEN_SINK_JSON, 390.0, &[]);
    let nav = f(&m, "nav-card");
    let main = f(&m, "main-card");
    let footer = f(&m, "footer-card");
    assert!(near(nav.x, main.x) && near(main.x, footer.x), "single column");
    assert!(nav.y + nav.h <= main.y + 0.5 && main.y + main.h <= footer.y + 0.5, "stacked in area order");
    let grid = f(&m, "grid");
    assert!(near(nav.w, grid.w), "cards fill the one column");
}

#[test]
fn wide_width_uses_named_areas() {
    let (m, _) = frames(KITCHEN_SINK_JSON, 900.0, &[]);
    let nav = f(&m, "nav-card");
    let main = f(&m, "main-card");
    let footer = f(&m, "footer-card");
    assert!(nav.x + nav.w <= main.x + 0.5, "nav left of main");
    assert!(near(nav.y, main.y), "nav and main share the first row");
    assert!(near(footer.x, main.x) && near(footer.w, main.w), "footer under main, same column");
    assert!(footer.y >= main.y + main.h - 0.5, "footer below main");
    // nav spans both rows: its bottom reaches the footer's bottom.
    assert!(near(nav.y + nav.h, footer.y + footer.h), "nav spans two rows");
    // minmax(180px, 1fr) 2fr → main ≈ 2× nav (gap aside).
    assert!(main.w > nav.w * 1.8 && main.w < nav.w * 2.2, "2fr vs 1fr: nav {} main {}", nav.w, main.w);
}

#[test]
fn aspect_ratio_and_absolute_badge() {
    let (m, _) = frames(KITCHEN_SINK_JSON, 900.0, &[]);
    let media = f(&m, "media");
    assert!(near(media.w / media.h, 1.7777778), "16:9 media, got {} x {}", media.w, media.h);
    let badge = f(&m, "media-badge");
    assert!(near(badge.x + badge.w, media.x + media.w - 8.0), "badge right inset 8");
    assert!(near(badge.y, media.y + 8.0), "badge top inset 8");
    let img = f(&m, "media-img");
    assert!(near(img.w, media.w) && near(img.h, media.h), "image fills the media box");
}

#[test]
fn percentages_and_flex_factors() {
    let (m, _) = frames(KITCHEN_SINK_JSON, 900.0, &[]);
    let row = f(&m, "flex-demo");
    let fx4 = f(&m, "fx-4");
    assert!(near(fx4.w, row.w * 0.25), "25% of {} = {}, got {}", row.w, row.w * 0.25, fx4.w);
    let fx2 = f(&m, "fx-2");
    assert!(near(fx2.w, 160.0), "shrink 0 keeps 160");
    let fx3 = f(&m, "fx-3");
    assert!(fx3.w <= row.w * 0.5 + 0.5, "max 50%");
    // form-actions uses marginLeft:auto → flush right inside form-row.
    let form_row = f(&m, "form-row");
    let actions = f(&m, "form-actions");
    assert!(near(actions.x + actions.w, form_row.x + form_row.w), "marginLeft auto pushes right");
}

#[test]
fn rtl_mirrors_every_frame() {
    let mut v: serde_json::Value = serde_json::from_str(KITCHEN_SINK_JSON).unwrap();
    v["style"]["direction"] = json!("rtl");
    let rtl_json = v.to_string();
    for width in [390.0, 900.0] {
        let (ltr, _) = frames(KITCHEN_SINK_JSON, width, &[]);
        let (rtl, _) = frames(&rtl_json, width, &[]);
        let root_w = ltr["root"].frame.w;
        // Physical properties stay physical under RTL, exactly like CSS: the
        // badge's `right: 8` and the actions' `marginLeft: auto` do NOT flip
        // (only logical `inset-inline-*` / `margin-inline-*` would, and those
        // are not in the v1 whitelist). Flow (flex, grid, block) mirrors.
        let physical = ["media-badge", "form-actions", "form-cancel", "form-send"];
        for (label, set) in [("ltr", &ltr), ("rtl", &rtl)] {
            let badge = set["media-badge"].frame;
            let media = set["media"].frame;
            assert!(
                near(badge.x + badge.w, media.x + media.w - 8.0),
                "{label}: physical `right: 8` keeps the badge at the media box's right edge"
            );
        }
        for (id, l) in &ltr {
            if physical.contains(&id.as_str()) {
                continue;
            }
            let r = &rtl[id];
            let mirrored_x = root_w - l.frame.x - l.frame.w;
            assert!(
                near(r.frame.x, mirrored_x) && near(r.frame.y, l.frame.y) && near(r.frame.w, l.frame.w),
                "{id} @ {width}: ltr {:?} rtl {:?}",
                l.frame,
                r.frame
            );
        }
    }
}

#[test]
fn pressed_changes_only_the_visual() {
    let (base, _) = frames(KITCHEN_SINK_JSON, 900.0, &[]);
    let (pressed, _) = frames(KITCHEN_SINK_JSON, 900.0, &["hdr-scan"]);
    assert_eq!(pressed["hdr-scan"].visual.opacity, Some(0.6));
    assert_eq!(base["hdr-scan"].visual.opacity, None);
    for (id, b) in &base {
        assert_eq!(b.frame, pressed[id].frame, "{id} frame unchanged by :pressed");
    }
}

#[test]
fn incremental_restyle_matches_fresh_layout() {
    let mut s = Surface::new(KITCHEN_SINK_JSON).unwrap();
    s.set_rounding(false);
    s.set_viewport(390.0, 0.0);
    let _ = s.layout(&mut FixedMeasure);
    assert!(s.set_viewport(900.0, 0.0));
    let incremental = s.layout(&mut FixedMeasure);
    let (fresh, _) = frames(KITCHEN_SINK_JSON, 900.0, &[]);
    for n in &incremental.nodes {
        assert_eq!(n.frame, fresh[&n.id].frame, "{}", n.id);
    }
}

#[test]
fn preorder_is_paint_and_a11y_order() {
    let (_, s) = frames(KITCHEN_SINK_JSON, 900.0, &[]);
    let ids: Vec<&str> = s.nodes().iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids[0], "root");
    assert_eq!(ids[1], "header");
    let media = ids.iter().position(|i| *i == "media").unwrap();
    assert_eq!(ids[media + 1], "media-img");
    assert_eq!(ids[media + 2], "media-badge");
}

#[test]
fn bench_tree_has_200_nodes_and_lays_out() {
    let json = bench::bench_tree(200);
    let mut s = Surface::new(&json).unwrap();
    assert!(s.node_count() >= 200, "{} nodes", s.node_count());
    s.set_viewport(390.0, 0.0);
    let r = s.layout(&mut FixedMeasure);
    assert_eq!(r.nodes.len(), s.node_count());
    assert!(r.measure_calls > 0);
    eprintln!(
        "bench 200 @390 (fixed measure, macOS): build {} µs, layout {} µs, {} measure calls",
        s.build_ns / 1000,
        r.layout_ns / 1000,
        r.measure_calls
    );
}
