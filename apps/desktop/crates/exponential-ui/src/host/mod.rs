//! # The host API (VAPP-91)
//!
//! What a surface needs from its host, the same on all four platforms
//! (`packages/exponential-ui/catalog/host.json`): the router turning server
//! messages into ops, the wire decoders (JSONL, SSE, A2UI-over-MCP), the
//! function / URL / media policy, binding sources, catalog negotiation and
//! declarative vapp packages. A line-by-line port of the TypeScript reference
//! (`packages/exponential-ui/src/host/`) over `serde_json::Value`; every
//! output is JSON-equal to it, locked by `fixtures/host-*.json`. The I/O
//! (transports, the runtime host) stays on each platform; the Swift and
//! Kotlin painters reach this module through `exponential-ui-ffi`.

pub mod contract;
pub mod decoders;
pub mod package;
pub mod policy;
pub mod router;
pub mod sources;

pub use contract::*;
pub use decoders::{decode_jsonl, mcp_action_call, messages_from_mcp_result, DecodeIssue, Decoded, JsonlDecoder, SseDecoder};
pub use package::{client_capabilities, merge_data, supported_catalog_ids, template_messages, validate_package, PackageIssue};
pub use policy::{
    combine_decisions, decide_function, decide_url, image_dimensions, image_frames, image_header, is_builtin_function, media_image_check, ImageHeader, matches_pattern, media_image_within_limits, media_request, media_within_limits, package_policy, svg_dimensions,
    safe_href, FunctionDecision, FunctionPolicy, MediaOptions, MediaRequest, MediaRule, UrlDecision, UrlPolicy, BUILTIN_FUNCTIONS,
};
pub use router::{normalize_path, HostRouter, SurfaceInfo};
pub use sources::{parse_source, ParsedSource};

/// JavaScript's `String.prototype.trim` (WhiteSpace + LineTerminator).
pub(crate) fn js_trim(s: &str) -> &str {
    fn ws(c: char) -> bool {
        matches!(c, '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    }
    s.trim_matches(ws)
}

/// JavaScript truthiness of a JSON value.
pub(crate) fn js_truthy(value: &serde_json::Value) -> bool {
    use serde_json::Value;
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
