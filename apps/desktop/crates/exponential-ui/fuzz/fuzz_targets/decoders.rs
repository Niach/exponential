//! The transport decoders on arbitrary bytes: JSONL pushed in arbitrary
//! chunks equals the same text pushed once (`decode_jsonl` reads a whole
//! document differently: one JSON array is its messages); SSE in chunks
//! never panics; the MCP carrier on arbitrary JSON. Never a panic.
#![no_main]

use arbitrary::Arbitrary;
use exponential_ui::host::{decode_jsonl, mcp_action_call, messages_from_mcp_result, JsonlDecoder, SseDecoder};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    text: String,
    cuts: Vec<u8>,
}

fn chunks<'a>(text: &'a str, cuts: &[u8]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut at = 0;
    for c in cuts.iter().take(64) {
        let mut next = at + *c as usize;
        if next >= text.len() {
            break;
        }
        while !text.is_char_boundary(next) {
            next += 1;
        }
        if next > at {
            out.push(&text[at..next]);
            at = next;
        }
    }
    out.push(&text[at..]);
    out
}

fuzz_target!(|input: Input| {
    let _ = decode_jsonl(&input.text);
    let mut once = JsonlDecoder::new();
    let mut whole = once.push(&input.text);
    whole.extend(once.end());
    let mut jsonl = JsonlDecoder::new();
    let mut streamed = exponential_ui::host::Decoded::default();
    for c in chunks(&input.text, &input.cuts) {
        streamed.extend(jsonl.push(c));
    }
    streamed.extend(jsonl.end());
    assert_eq!(whole.messages, streamed.messages, "chunking never changes the messages");
    assert_eq!(whole.issues.len(), streamed.issues.len(), "chunking never changes the issue count");
    let mut sse = SseDecoder::new();
    for c in chunks(&input.text, &input.cuts) {
        let _ = sse.push(c);
    }
    let _ = sse.end();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&input.text) {
        let _ = messages_from_mcp_result(&value);
        let _ = mcp_action_call(&value, None);
    }
});
