//! #AXC-12 (VAPP-299): `fixtures/express-demo.json`, the recorded Express
//! response every guide app replays (guides/agent `GET /express`), compiles
//! through the native core: its chunks, read in order by one wrapped
//! ExpressDecoder, give exactly `expected` (the A2UI messages and the
//! prose), with no issue. The TS twin is src/express/express-demo.test.ts.
#![cfg(feature = "express")]

use eui_core::host::{Decoded, ExpressDecoder};
use serde_json::Value;

#[test]
fn the_express_demo_compiles_to_the_recorded_messages() {
    let path = format!("{}/../../../../packages/exponential-ui/fixtures/express-demo.json", env!("CARGO_MANIFEST_DIR"));
    let demo: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap();
    let mut decoder = ExpressDecoder::new(true, demo["surfaceId"].as_str().unwrap());
    let mut out = Decoded::default();
    let chunks = demo["chunks"].as_array().unwrap();
    assert!(chunks.len() > 100, "a real stream has many chunks");
    for c in chunks {
        out.extend(decoder.push(c["text"].as_str().unwrap()));
    }
    out.extend(decoder.end());
    assert!(out.issues.is_empty(), "issues: {:?}", out.issues);
    assert_eq!(out.text.concat(), demo["expected"]["text"].as_str().unwrap());
    assert_eq!(Value::Array(out.messages), demo["expected"]["messages"]);
}
