//! Shared by `host_fixtures.rs` and `conformance.rs`: the host fixtures
//! replayed case by case (the structure of `src/host/host.test.ts`), each case
//! a name and `Err(first difference)` on failure.

#![allow(dead_code)]

pub mod carousel;
pub mod round2;

use eui_core::host::{
    client_capabilities, combine_decisions, decide_function, decide_url, mcp_action_call, media_request, messages_from_mcp_result, parse_source,
    source_with_params, supported_catalog_ids, validate_package, validate_record, Decoded, FunctionDecision, FunctionPolicy, HostRouter, JsonlDecoder, MediaOptions, SseDecoder, UrlPolicy,
};
use eui_core::json;
use serde_json::Value;

pub const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui");

pub fn read(rel: &str) -> Value {
    let path = format!("{PKG}/{rel}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

pub fn fixture(name: &str) -> Value {
    read(&format!("fixtures/{name}"))
}

/// The first path where two documents differ, or `None`.
pub fn first_diff(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                match (x.get(k), y.get(k)) {
                    (Some(xv), Some(yv)) => {
                        if let Some(d) = first_diff(xv, yv, &format!("{at}.{k}")) {
                            return Some(d);
                        }
                    }
                    (xv, yv) => {
                        let show = |v: Option<&Value>| v.map(|v| v.to_string()).unwrap_or_else(|| "(absent)".into());
                        return Some(format!("{at}.{k}: got {} / expected {}", show(xv), show(yv)));
                    }
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (xv, yv)) in x.iter().zip(y).enumerate() {
                if let Some(d) = first_diff(xv, yv, &format!("{at}[{i}]")) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| format!("{at}: got {} items / expected {}", x.len(), y.len()))
        }
        _ => (!json::equal(a, b)).then(|| format!("{at}: got {a} / expected {b}")),
    }
}

pub fn same(got: &Value, expected: &Value) -> Result<(), String> {
    if json::equal(got, expected) {
        Ok(())
    } else {
        Err(first_diff(got, expected, "$").unwrap_or_else(|| "differs".into()))
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serializable")
}

pub type Case = (String, Result<(), String>);

pub trait Decoder {
    fn push_chunk(&mut self, chunk: &str) -> Decoded;
    fn finish(&mut self) -> Decoded;
}

impl Decoder for JsonlDecoder {
    fn push_chunk(&mut self, chunk: &str) -> Decoded {
        self.push(chunk)
    }
    fn finish(&mut self) -> Decoded {
        self.end()
    }
}

impl Decoder for SseDecoder {
    fn push_chunk(&mut self, chunk: &str) -> Decoded {
        self.push(chunk)
    }
    fn finish(&mut self) -> Decoded {
        self.end()
    }
}

/// Every chunk through ONE decoder, then `end`.
pub fn feed(d: &mut dyn Decoder, chunks: &[Value]) -> Decoded {
    let mut out = Decoded::default();
    for c in chunks {
        out.extend(d.push_chunk(c.as_str().expect("chunk")));
    }
    out.extend(d.finish());
    out
}

fn list<'a>(f: &'a Value, key: &str) -> &'a Vec<Value> {
    f[key].as_array().unwrap_or_else(|| panic!("{key}"))
}

/// `host-transport.json`: jsonl, sse, mcp, mcpAction.
pub fn transport_cases() -> Vec<Case> {
    let f = fixture("host-transport.json");
    let mut out = Vec::new();
    for c in list(&f, "jsonl") {
        let got = feed(&mut JsonlDecoder::new(), list(c, "chunks"));
        out.push((format!("jsonl: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
    }
    for c in list(&f, "sse") {
        let got = feed(&mut SseDecoder::new(), list(c, "chunks"));
        out.push((format!("sse: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
    }
    for c in list(&f, "mcp") {
        out.push((format!("mcp: {}", c["name"].as_str().unwrap()), same(&to_value(&messages_from_mcp_result(&c["result"])), &c["expected"])));
    }
    for c in list(&f, "mcpAction") {
        let got = mcp_action_call(&c["message"], c.get("tool").and_then(Value::as_str));
        out.push((format!("mcpAction: {}", c["name"].as_str().unwrap()), same(&got, &c["expected"])));
    }
    out
}

fn decision(v: &Value) -> FunctionDecision {
    FunctionDecision::parse(v.as_str().expect("decision")).expect("a decision")
}

/// `host-policy.json`: functions, combine, urls, media, sources, negotiation.
pub fn policy_cases() -> Vec<Case> {
    let f = fixture("host-policy.json");
    let mut out = Vec::new();
    for c in list(&f, "functions") {
        let policy: Option<FunctionPolicy> = c.get("policy").map(|p| serde_json::from_value(p.clone()).expect("policy"));
        let got = decide_function(policy.as_ref(), c["fn"].as_str().unwrap(), c["registered"].as_bool().unwrap());
        out.push((format!("function: {}", c["name"].as_str().unwrap()), same(&Value::from(got.as_str()), &c["expected"])));
    }
    for c in list(&f, "combine") {
        let got = combine_decisions(decision(&c["a"]), decision(&c["b"]));
        out.push((format!("combine: {} + {}", c["a"].as_str().unwrap(), c["b"].as_str().unwrap()), same(&Value::from(got.as_str()), &c["expected"])));
    }
    for c in list(&f, "urls") {
        let policy: Option<UrlPolicy> = c.get("policy").map(|p| serde_json::from_value(p.clone()).expect("url policy"));
        let got = decide_url(policy.as_ref(), c["url"].as_str().unwrap());
        out.push((format!("url: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
    }
    for c in list(&f, "media") {
        let options: MediaOptions = serde_json::from_value(c["options"].clone()).expect("media options");
        let got = media_request(c["url"].as_str().unwrap(), &options);
        out.push((format!("media: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
    }
    for c in list(&f, "sources") {
        let uri = c["uri"].as_str().unwrap();
        out.push((format!("source: {uri}"), same(&to_value(&parse_source(uri)), &c["expected"])));
    }
    for c in list(&f, "sourceParams") {
        let got = source_with_params(c["source"].as_str().unwrap(), c.get("params"));
        out.push((format!("sourceParams: {}", c["expected"].as_str().unwrap_or("")), same(&Value::String(got), &c["expected"])));
    }
    for c in list(&f, "negotiation") {
        let ids: Vec<String> = serde_json::from_value(c["extensionIds"].clone()).unwrap();
        let result = same(&to_value(&supported_catalog_ids(&ids)), &c["expected"]["supportedCatalogIds"])
            .and_then(|_| same(&client_capabilities(&ids), &c["expected"]["clientCapabilities"]));
        out.push((format!("negotiation: {} extension ids", ids.len()), result));
    }
    out
}

/// `host-router.json`: validation, then flows.
pub fn router_cases() -> Vec<Case> {
    let f = fixture("host-router.json");
    let packages = &f["packages"];
    let mut out = Vec::new();
    for v in list(&f, "validation") {
        let id = v["package"].as_str().unwrap();
        let got = validate_package(&packages[id], &supported_catalog_ids::<&str>(&[]));
        out.push((format!("validation: {id}"), same(&to_value(&got), &v["expected"])));
    }
    for c in list(&f, "records") {
        let schema = &packages[c["package"].as_str().unwrap()]["records"][c["kind"].as_str().unwrap()]["schema"];
        let got = validate_record(schema, &c["data"]);
        out.push((format!("records: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
    }
    for flow in list(&f, "flows") {
        let name = flow["name"].as_str().unwrap();
        let ext: Vec<String> = flow.get("extensionIds").map(|e| serde_json::from_value(e.clone()).unwrap()).unwrap_or_default();
        let mut router = HostRouter::new(&ext);
        let mut result = Ok(());
        for id in flow.get("packages").and_then(Value::as_array).into_iter().flatten() {
            let id = id.as_str().unwrap();
            let got = router.install_package(&packages[id]);
            result = result.and_then(|_| same(&to_value(&got), &flow["installIssues"][id]).map_err(|e| format!("install {id}: {e}")));
        }
        for (i, step) in list(flow, "steps").iter().enumerate() {
            let got = Value::Array(router.route(&step["message"]));
            result = result.and_then(|_| same(&got, &step["expected"]).map_err(|e| format!("step {i}: {e}")));
        }
        out.push((format!("flow: {name}"), result));
    }
    out
}

fn same_text(got: &str, expected: &str) -> Result<(), String> {
    if got == expected {
        Ok(())
    } else {
        Err(format!("got {got:?} / expected {expected:?}"))
    }
}

/// VAPP-161 `host-package.json`: recordsQuery, consent (rows + keys),
/// toolCalls, extensions, canonical and signatures (`signedText`; the
/// Ed25519 verdicts stay with the hosts' own curves).
pub fn package_cases() -> Vec<Case> {
    use eui_core::host::{canonical_json, package_file_signed_bytes, parse_records_source, tool_call, tool_consent, tool_consent_key, validate_package_with, AvailableExtension};
    let f = fixture("host-package.json");
    let mut out = Vec::new();
    for c in list(&f, "recordsQuery") {
        let source = c["source"].as_str().unwrap();
        let got = parse_source(source).and_then(|s| parse_records_source(&s));
        out.push((format!("recordsQuery: {source}"), same(&to_value(&got), &c["expected"])));
    }
    for c in list(&f, "consent") {
        let package_id = c["packageId"].as_str().unwrap();
        let mut result = same(&to_value(&tool_consent(&c["package"])), &c["expected"]);
        let tools = c["package"].get("tools").and_then(Value::as_object).cloned().unwrap_or_default();
        let keys: serde_json::Map<String, Value> = tools.iter().map(|(id, tool)| (id.clone(), Value::String(tool_consent_key(package_id, id, tool)))).collect();
        result = result.and_then(|_| same(&Value::Object(keys), &c["keys"]).map_err(|e| format!("keys: {e}")));
        out.push((format!("consent: {package_id}"), result));
    }
    for c in list(&f, "toolCalls") {
        let name = c["name"].as_str().unwrap();
        out.push((format!("toolCalls: {name}"), same(&to_value(&tool_call(name)), &c["expected"])));
    }
    for c in list(&f, "extensions") {
        let ids: Vec<String> = serde_json::from_value(c["extensionIds"].clone()).unwrap();
        let available: Option<Vec<AvailableExtension>> = c.get("available").map(|a| serde_json::from_value(a.clone()).unwrap());
        let catalog_ids = supported_catalog_ids(&ids);
        let pkg = serde_json::json!({"id": "x", "name": "X", "version": "1.0.0", "catalogId": catalog_ids[0], "templates": {"t": {"components": [{"id": "root", "component": "Text", "text": "Hi"}]}}, "extensions": c["extensions"]});
        let got = validate_package_with(&pkg, &catalog_ids, available.as_deref());
        out.push((format!("extensions: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
    }
    for (i, c) in list(&f, "canonical").iter().enumerate() {
        let value: Value = serde_json::from_str(c["text"].as_str().unwrap()).unwrap();
        out.push((format!("canonical: {i}"), same_text(&canonical_json(&value), c["expected"].as_str().unwrap())));
    }
    for c in list(&f, "signatures") {
        let bytes = package_file_signed_bytes(c["signed"].as_object().unwrap());
        out.push((format!("signedText: {}", c["name"].as_str().unwrap()), same_text(&String::from_utf8(bytes).unwrap(), c["signedText"].as_str().unwrap())));
    }
    out
}
