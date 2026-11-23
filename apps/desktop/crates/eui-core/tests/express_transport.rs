//! #AXC-12 (VAPP-295): `fixtures/host-transport.json` `express*`: the Rust
//! ExpressDecoder, the SSE `a2ui-express` events, the `{express}` WebSocket
//! frames, the Express MCP resources, the Content-Type sniffing and the
//! `inferenceFormats` capability equal the TS reference (over the vendored
//! wasm of the same a2ui-express rev). Also: the contract keys are the
//! ones catalog/host.json `inference` names.
#![cfg(feature = "express")]

use eui_core::host::{express_frame_chunk, inference_format_of, messages_from_mcp_result_with, renderer_capabilities_with, Decoded, ExpressDecoder, FrameDecoder, SseDecoder};
use serde_json::{json, Value};

fn fixture() -> Value {
    let path = format!("{}/../../../../packages/exponential-ui/fixtures/host-transport.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap()
}

fn cases<'a>(f: &'a Value, key: &str) -> &'a Vec<Value> {
    let list = f[key].as_array().unwrap_or_else(|| panic!("host-transport.json has no `{key}`"));
    assert!(!list.is_empty(), "`{key}` is empty");
    list
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn feed(mut push: impl FnMut(&str) -> Decoded, end: impl FnOnce() -> Decoded, chunks: &[String]) -> Value {
    let mut out = Decoded::default();
    for c in chunks {
        out.extend(push(c));
    }
    out.extend(end());
    out.to_json()
}

fn check(failures: &mut Vec<String>, what: &str, got: &Value, want: &Value) {
    if got != want {
        failures.push(format!("{what}:\n  got  {got}\n  want {want}"));
    }
}

#[test]
fn express_transport_cases_equal_the_ts_reference() {
    let f = fixture();
    let mut failures = Vec::new();
    for c in cases(&f, "express") {
        let surface = c["surfaceId"].as_str().unwrap_or("main");
        let mut d = ExpressDecoder::new(c["wrapped"].as_bool().unwrap(), surface);
        let d = std::cell::RefCell::new(&mut d);
        let got = feed(|s| d.borrow_mut().push(s), || d.borrow_mut().end(), &strs(&c["chunks"]));
        check(&mut failures, &format!("express: {}", c["name"]), &got, &c["expected"]);
    }
    // #VAPP-300: the open block after every push, then after end().
    for c in cases(&f, "expressOpen") {
        let chunks = strs(&c["chunks"]);
        let sid = c["surfaceId"].as_str();
        let reads: Vec<Decoded> = match c["decoder"].as_str() {
            Some("sse") => {
                let mut d = SseDecoder::with_surface(sid);
                chunks.iter().map(|s| d.push(s)).chain(std::iter::once(d.end())).collect()
            }
            Some("ws") => {
                let mut d = FrameDecoder::new(sid);
                chunks.iter().map(|s| d.push(s)).chain(std::iter::once(d.end())).collect()
            }
            _ => {
                let mut d = ExpressDecoder::new(c["wrapped"].as_bool().unwrap(), sid.unwrap_or("main"));
                chunks.iter().map(|s| d.push(s)).chain(std::iter::once(d.end())).collect()
            }
        };
        let got = json!(reads.iter().map(|d| d.open.clone()).collect::<Vec<_>>());
        check(&mut failures, &format!("expressOpen: {}", c["name"]), &got, &c["expected"]);
    }
    for c in cases(&f, "expressSse") {
        let d = std::cell::RefCell::new(SseDecoder::with_surface(c["surfaceId"].as_str()));
        let got = feed(|s| d.borrow_mut().push(s), || d.borrow_mut().end(), &strs(&c["chunks"]));
        check(&mut failures, &format!("expressSse: {}", c["name"]), &got, &c["expected"]);
    }
    for c in cases(&f, "expressWs") {
        let d = std::cell::RefCell::new(FrameDecoder::new(c["surfaceId"].as_str()));
        let got = feed(|s| d.borrow_mut().push(s), || d.borrow_mut().end(), &strs(&c["frames"]));
        check(&mut failures, &format!("expressWs: {}", c["name"]), &got, &c["expected"]);
    }
    for c in cases(&f, "expressMcp") {
        let got = messages_from_mcp_result_with(&c["result"], c["surfaceId"].as_str()).to_json();
        check(&mut failures, &format!("expressMcp: {}", c["name"]), &got, &c["expected"]);
    }
    for c in cases(&f, "expressContentTypes") {
        check(&mut failures, &format!("expressContentTypes: {}", c["name"]), &json!(inference_format_of(c["contentType"].as_str().unwrap())), &c["expected"]);
    }
    for c in cases(&f, "expressCapabilities") {
        let ids = strs(&c["extensionIds"]);
        let formats = c.get("inferenceFormats").map(strs);
        let formats: Option<Vec<&str>> = formats.as_ref().map(|f| f.iter().map(String::as_str).collect());
        let got = renderer_capabilities_with(&ids, None, formats.as_deref());
        check(&mut failures, &format!("expressCapabilities: {}", c["name"]), &got, &c["expected"]);
    }
    assert!(failures.is_empty(), "{} express transport case(s) differ:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn an_express_frame_is_exactly_one_string_key() {
    assert_eq!(express_frame_chunk(r#"{"express": "a"}"#).as_deref(), Some("a"));
    assert_eq!(express_frame_chunk(r#"{"express": 1}"#), None);
    assert_eq!(express_frame_chunk(r#"{"express": "a", "x": 1}"#), None);
    assert_eq!(express_frame_chunk(r#"{"version": "1.0"}"#), None);
}
