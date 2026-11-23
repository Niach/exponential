//! #EXP-2067: `express::unwrap` splits a whole response like the wasm
//! `unwrap` of the same a2ui-express rev (the JSON below is the wasm's own
//! output), and serializes to the same JSON.
#![cfg(feature = "express")]

use eui_core::express::{unwrap, RawPart};
use serde_json::{json, Value};

fn split(text: &str) -> Value {
    serde_json::to_value(unwrap(text)).unwrap()
}

#[test]
fn prose_and_blocks_in_order() {
    let text = "Here is the build:\n<a2ui>\nroot = Text(text=\"Green\")\n</a2ui>\nShip it?";
    assert_eq!(
        split(text),
        json!([{"text": "Here is the build:"}, {"a2ui_raw": "root = Text(text=\"Green\")"}, {"text": "Ship it?"}])
    );
}

#[test]
fn a_fence_around_a_block_is_dropped() {
    let text = "Look:\n```\n<a2ui>\nroot = Text(\"hi\")\n</a2ui>\n```\nafter";
    assert_eq!(split(text), json!([{"text": "Look:"}, {"a2ui_raw": "root = Text(\"hi\")"}, {"text": "after"}]));
}

#[test]
fn an_open_block_is_not_final() {
    let parts = unwrap("end <a2ui>\nroot = Text(\"open");
    assert_eq!(parts[0], RawPart::Text { text: "end".into() });
    assert_eq!(parts[1], RawPart::Block { a2ui_raw: "root = Text(\"open".into(), is_final: false });
    assert_eq!(split("end <a2ui>\nroot = Text(\"open"), json!([{"text": "end"}, {"a2ui_raw": "root = Text(\"open", "is_final": false}]));
}

#[test]
fn round_trips_through_json() {
    let parts = unwrap("a\n<a2ui>\nroot = Text(\"x\")\n</a2ui>");
    let back: Vec<RawPart> = serde_json::from_value(serde_json::to_value(&parts).unwrap()).unwrap();
    assert_eq!(back, parts);
}
