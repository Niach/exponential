//! #EDE-65: the `exponential` theme's MessageBubble is the IDE's chat bubble
//! (crates/ui `session_chat::message_bubble`) through the LAYOUT CORE, which
//! every native painter reads: SwiftUI and Compose paint the core's frames
//! and its `Visual` (`corner_radii` → PaintStyle.swift / PaintStyle.kt over
//! exponential-ui-ffi), gpui the same in-process. So the part recipe's
//! `maxWidth` (62 % in, #FEED-239: 100 % out) and `borderTopLeftRadius`
//! reach every native here.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::theme::Mode;
use eui_core::themes::builtin_theme;
use eui_core::types::NestedNode;
use serde_json::json;

const COLUMN: f32 = 400.0;
const LONG: &str = "Send me the photo from the Wiesn I put on the desktop, the one with the whole table in it.";

fn laid_out(theme: &str, side: &str) -> (Surface, eui_core::surface::LayoutOutput) {
    laid_out_with(theme, side, None)
}

fn laid_out_with(theme: &str, side: &str, avatar: Option<serde_json::Value>) -> (Surface, eui_core::surface::LayoutOutput) {
    let mut bubble = json!({"id": "m", "component": "MessageBubble", "props": {"side": side, "author": "spezl", "text": LONG, "time": "20:20"}});
    if let Some(avatar) = avatar {
        bubble["slots"] = json!({ "avatar": avatar });
    }
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": COLUMN},
        "children": [bubble]
    }))
    .expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: Some(builtin_theme(theme).expect("theme")), mode: Mode::Dark, ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(COLUMN, 0.0, None);
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    (s, out)
}

fn bubble_width(s: &Surface, out: &eui_core::surface::LayoutOutput) -> f32 {
    let i = s.index_of("m.bubble").expect("the bubble part");
    out.frames.iter().find(|f| f.index == i).expect("bubble drawn").w
}

#[test]
fn the_exponential_bubble_caps_at_62_percent_with_four_16px_corners() {
    for side in ["in", "out"] {
        let (s, out) = laid_out("exponential", side);
        let w = bubble_width(&s, &out);
        // A long message fills the cap: 62 % of the row for an `in` bubble;
        // #FEED-239: an `out` one has no leading inset (100 %).
        let cap = if side == "out" { 1.0 } else { 0.62 };
        assert!((w - COLUMN * cap).abs() < 1.0, "{side}: bubble {w} wide, want {}", COLUMN * cap);
        let v = s.visual(s.index_of("m.bubble").unwrap()).unwrap();
        let corners = v.corner_radii.unwrap_or([v.border_radius.unwrap_or(0.0); 4]);
        assert_eq!(corners, [16.0; 4], "{side}: no 6 px leading corner");
        assert_eq!(v.border_color.as_deref(), Some(if side == "out" { "#ffffff1f" } else { "#ffffff1a" }), "{side}");
    }
}

/// #FEED-239: the viewer's own bubble wears its avatar on the RIGHT and takes
/// the whole row left of it (the avatar's 32 px + the 8 px gap).
#[test]
fn an_out_bubble_wears_its_avatar_on_the_right() {
    let avatar = json!({"id": "me", "component": "Avatar", "props": {"name": "Mia Berger"}});
    let (s, out) = laid_out_with("exponential", "out", Some(avatar));
    let frame = |id: &str| {
        let i = s.index_of(id).unwrap_or_else(|| panic!("{id} drawn"));
        out.frames.iter().find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} framed"))
    };
    let (bubble, me) = (frame("m.bubble"), frame("me"));
    assert!(me.x > bubble.x, "the avatar sits right of the bubble: avatar x {} vs bubble x {}", me.x, bubble.x);
    assert!((me.x + me.w - COLUMN).abs() < 1.0, "the avatar ends the row: {} + {}", me.x, me.w);
    assert!((bubble.w - (COLUMN - me.w - 8.0)).abs() < 1.0, "the bubble fills the row left of its avatar: {} wide", bubble.w);
}

#[test]
fn neutral_keeps_the_macros_box() {
    let (s, out) = laid_out("neutral", "in");
    let w = bubble_width(&s, &out);
    assert!((w - COLUMN * 0.8).abs() < 1.0, "neutral bubble {w} wide, want the macro's 80 %");
    let v = s.visual(s.index_of("m.bubble").unwrap()).unwrap();
    assert_eq!(v.corner_radii.map(|c| c[0]), Some(6.0), "neutral `in` keeps the 6 px corner");
}
