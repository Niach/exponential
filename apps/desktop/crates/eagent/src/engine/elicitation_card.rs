//! #NEW-146 (`packages/domain-contract/fixtures/elicitation-card.json`, its
//! `$comment` is the spec): a THIRD-PARTY MCP server's elicitation as THE
//! question card. The device builds the card here ([`elicitation_card`])
//! from the ACP elicitation ([`card_request`]) and posts it like every engine
//! ask (sealed, Needs you, push); the answered card maps back to MCP's
//! `{action, content?}` ([`elicitation_action`], [`elicitation_content`]).
//! Never an auto-accept. Mirrors the TS reference
//! `packages/domain-contract/src/elicitation-card.ts`; the tests replay the
//! fixture's `cardCases`, `contentCases`, `actionCases` and `urlCases`.

use agent_client_protocol::v1::schema::{CreateElicitationRequest, ElicitationMode};
use serde_json::{json, Map, Value};

/// `serverMetaKey`: the `_meta` key of an ACP elicitation naming the
/// THIRD-PARTY MCP server that asked (the name Exponential configured it
/// under). Its presence makes the ask THE elicitation card instead of the
/// question stepper. THE one definition in this crate (the codex adapter and
/// the device hub's [`super::external`] both stamp it).
pub const SERVER_META_KEY: &str = "exponential.at/mcpServer";
/// `orderMetaKey`: the server's property order (ACP's schema is a sorted
/// map), stamped by the sources from the raw MCP request.
pub const ORDER_META_KEY: &str = "exponential.at/fieldOrder";
/// `formKey`: every field binds `/<formKey>/<name>`.
pub const FORM_KEY: &str = "form";

pub const COPY_SUBMIT: &str = "Submit";
pub const COPY_DECLINE: &str = "Decline";
pub const COPY_CANCEL: &str = "Cancel";
pub const COPY_DONE: &str = "I'm done";
pub const COPY_UNKNOWN_SERVER: &str = "MCP server";
const WARN_PUNYCODE: &str = "This address uses look-alike characters.";
const WARN_IP: &str = "This link goes to an IP address, not a domain.";
const WARN_INSECURE: &str = "This link is not encrypted (no https).";

/// `multiPartSuffixes`: the public suffixes of two labels the registrable
/// domain looks one label further for.
const MULTI_PART_SUFFIXES: [&str; 22] = [
    "co.uk", "org.uk", "ac.uk", "gov.uk", "com.au", "net.au", "org.au", "co.nz", "co.jp", "ne.jp", "or.jp", "com.br",
    "com.cn", "com.mx", "co.in", "co.za", "com.tr", "co.kr", "com.sg", "co.at", "or.at", "gv.at",
];

/// The URL review (url mode).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct UrlReview {
    /// The URL exactly as the server sent it (trimmed).
    pub url: String,
    pub host: String,
    /// The registrable domain (eTLD+1; an IP or a two-label host = itself).
    pub domain: String,
    /// `punycode`, `ip`, `insecure`, in that order.
    pub warnings: Vec<&'static str>,
}

/// What an answered (or dismissed) elicitation card carries back.
#[derive(Clone, Debug, PartialEq)]
pub enum ElicitationReply {
    /// `content` = the accepted form ([`elicitation_content`]); `None` for a
    /// url elicitation.
    Accept(Option<Map<String, Value>>),
    Decline,
    Cancel,
}

impl ElicitationReply {
    /// MCP's `ElicitResult`.
    pub fn mcp(&self) -> Value {
        match self {
            Self::Accept(Some(content)) => json!({ "action": "accept", "content": content }),
            Self::Accept(None) => json!({ "action": "accept" }),
            Self::Decline => json!({ "action": "decline" }),
            Self::Cancel => json!({ "action": "cancel" }),
        }
    }
}

/// The action label a card answer names (`accept` | `decline` | `cancel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElicitationAction {
    Accept,
    Decline,
    Cancel,
}

impl ElicitationAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }
}

/// Trimmed non-blank text, else `None`.
fn text(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

fn bind(name: &str) -> Value {
    json!({ "@path": format!("/{FORM_KEY}/{name}") })
}

fn kind(property: &Value) -> &str {
    property.get("type").and_then(Value::as_str).unwrap_or("string")
}

/// `[{label, value}]` from `[{const, title}]`.
fn titled(options: &Value) -> Vec<(String, String)> {
    options
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|o| {
                    let value = o.get("const")?.as_str()?.to_string();
                    let label = o.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| value.clone());
                    Some((label, value))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// The property's choices as `(label, value)` (empty = not a choice).
fn choice_options(property: &Value) -> Vec<(String, String)> {
    if kind(property) == "array" {
        let items = property.get("items").cloned().unwrap_or(Value::Null);
        let any_of = titled(items.get("anyOf").unwrap_or(&Value::Null));
        if !any_of.is_empty() {
            return any_of;
        }
        return strings(items.get("enum")).into_iter().map(|v| (v.clone(), v)).collect();
    }
    let one_of = titled(property.get("oneOf").unwrap_or(&Value::Null));
    if !one_of.is_empty() {
        return one_of;
    }
    let names = strings(property.get("enumNames"));
    strings(property.get("enum"))
        .into_iter()
        .enumerate()
        .map(|(i, v)| (names.get(i).cloned().unwrap_or_else(|| v.clone()), v))
        .collect()
}

/// A property's starting value: its default when of the right type, else
/// empty (`""` / null / false / []).
fn start_value(property: &Value) -> Value {
    let given = property.get("default");
    match kind(property) {
        "boolean" => Value::Bool(given.and_then(Value::as_bool).unwrap_or(false)),
        "number" | "integer" => given.filter(|v| v.is_number()).cloned().unwrap_or(Value::Null),
        "array" => Value::Array(strings(given).into_iter().map(Value::String).collect()),
        _ => given.filter(|v| v.is_string()).cloned().unwrap_or_else(|| Value::String(String::new())),
    }
}

/// ONE genUI field per property (the spec's FIELD MAP).
fn field(name: &str, property: &Value) -> Value {
    let label = text(property.get("title")).unwrap_or_else(|| name.to_string());
    let description = text(property.get("description"));
    let id = format!("f_{name}");
    let options = choice_options(property);
    let ty = kind(property);
    let mut props = Map::new();
    props.insert("label".into(), json!(label));
    props.insert("name".into(), json!(name));
    let component = if ty == "boolean" {
        props.insert("checked".into(), bind(name));
        if let Some(d) = description {
            props.insert("description".into(), json!(d));
        }
        "Switch"
    } else if ty == "array" || !options.is_empty() {
        props.insert(
            "options".into(),
            Value::Array(options.into_iter().map(|(label, value)| json!({ "label": label, "value": value })).collect()),
        );
        props.insert("value".into(), bind(name));
        if ty == "array" {
            props.insert("multiple".into(), json!(true));
        }
        if let Some(d) = description {
            props.insert("description".into(), json!(d));
        }
        "Select"
    } else if ty == "number" || ty == "integer" {
        props.insert("value".into(), bind(name));
        if let Some(min) = property.get("minimum").filter(|v| v.is_number()) {
            props.insert("min".into(), min.clone());
        }
        if let Some(max) = property.get("maximum").filter(|v| v.is_number()) {
            props.insert("max".into(), max.clone());
        }
        if ty == "integer" {
            props.insert("step".into(), json!(1));
            props.insert("precision".into(), json!(0));
        }
        if let Some(d) = description {
            props.insert("placeholder".into(), json!(d));
        }
        "NumberField"
    } else if property.get("format").and_then(Value::as_str) == Some("date") {
        props.insert("value".into(), bind(name));
        if let Some(d) = description {
            props.insert("placeholder".into(), json!(d));
        }
        "DatePicker"
    } else {
        let input = match property.get("format").and_then(Value::as_str) {
            Some("email") => "email",
            Some("uri") => "url",
            _ => "text",
        };
        props.insert("type".into(), json!(input));
        props.insert("value".into(), bind(name));
        if let Some(d) = description {
            props.insert("placeholder".into(), json!(d));
        }
        "Input"
    };
    json!({ "id": id, "component": component, "props": props })
}

fn properties(schema: &Value) -> Map<String, Value> {
    schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default()
}

/// The property names in the server's order (`order`, the names it lists
/// first), then any the order missed, by name.
pub fn elicitation_field_order(schema: &Value, order: &[String]) -> Vec<String> {
    let props = properties(schema);
    let mut first: Vec<String> = Vec::new();
    for name in order {
        if props.contains_key(name) && !first.contains(name) {
            first.push(name.clone());
        }
    }
    let mut rest: Vec<String> = props.keys().filter(|name| !first.contains(name)).cloned().collect();
    rest.sort();
    first.extend(rest);
    first
}

/// The URL review: `None` = not an absolute http(s) URL (the device
/// declines it; nothing else is ever opened).
pub fn elicitation_url(raw: &str) -> Option<UrlReview> {
    let url = raw.trim();
    let parsed = url::Url::parse(url).ok()?;
    let https = match parsed.scheme() {
        "https" => true,
        "http" => false,
        _ => return None,
    };
    let host = parsed.host_str()?.to_lowercase();
    if host.is_empty() {
        return None;
    }
    let ip = matches!(parsed.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)));
    let labels: Vec<&str> = host.split('.').collect();
    let mut warnings = Vec::new();
    if !ip && labels.iter().any(|l| l.starts_with("xn--")) {
        warnings.push("punycode");
    }
    if ip {
        warnings.push("ip");
    }
    if !https {
        warnings.push("insecure");
    }
    let domain = if !ip && labels.len() > 2 {
        let tail2 = labels[labels.len() - 2..].join(".");
        if MULTI_PART_SUFFIXES.contains(&tail2.as_str()) {
            labels[labels.len() - 3..].join(".")
        } else {
            tail2
        }
    } else {
        host.clone()
    };
    Some(UrlReview { url: url.to_string(), host, domain, warnings })
}

fn warning_copy(warning: &str) -> &'static str {
    match warning {
        "punycode" => WARN_PUNYCODE,
        "ip" => WARN_IP,
        _ => WARN_INSECURE,
    }
}

/// THE card a device posts for an elicitation request (the fixture's
/// `request` shape: `{server, message, mode, schema, order}` or `{server,
/// message, mode: "url", url, elicitationId}`); `None` = it cannot be shown
/// honestly (a url that is not http(s), an unknown mode): the device
/// declines.
pub fn elicitation_card(request: &Value) -> Option<Value> {
    let server = text(request.get("server")).unwrap_or_else(|| COPY_UNKNOWN_SERVER.to_string());
    let question = text(request.get("message")).unwrap_or_default();
    match request.get("mode").and_then(Value::as_str).unwrap_or("form") {
        "url" => {
            let review = elicitation_url(request.get("url")?.as_str()?)?;
            let mut children = vec![
                json!({ "id": "domain", "component": "Heading", "props": { "text": review.domain, "level": "h4" } }),
                json!({ "id": "url", "component": "Text", "props": { "text": review.url, "variant": "code" } }),
            ];
            for warning in &review.warnings {
                children.push(json!({
                    "id": format!("warn_{warning}"),
                    "component": "Alert",
                    "props": { "title": warning_copy(warning), "type": "warning" },
                }));
            }
            Some(json!({
                "kind": "question",
                "question": question,
                "options": [COPY_DONE, COPY_DECLINE],
                "custom": false,
                "multi": false,
                "surface": {
                    "version": "1.0",
                    "root": { "id": "root", "component": "Stack", "props": { "direction": "vertical", "gap": "sm" }, "children": children },
                    "data": {},
                },
                "elicitation": { "server": server, "mode": "url", "required": [], "url": review.url },
            }))
        }
        "form" => {
            let schema = request.get("schema")?;
            let props = properties(schema);
            let names = elicitation_field_order(schema, &strings(request.get("order")));
            let mut values = Map::new();
            let mut children = Vec::new();
            for name in &names {
                let property = &props[name];
                values.insert(name.clone(), start_value(property));
                children.push(field(name, property));
            }
            let required: Vec<String> = strings(schema.get("required")).into_iter().filter(|n| names.contains(n)).collect();
            Some(json!({
                "kind": "question",
                "question": question,
                "options": [COPY_SUBMIT, COPY_DECLINE, COPY_CANCEL],
                "custom": false,
                "multi": false,
                "surface": {
                    "version": "1.0",
                    "root": { "id": "root", "component": "Stack", "props": { "direction": "vertical", "gap": "md" }, "children": children },
                    "data": { FORM_KEY: values },
                },
                "elicitation": { "server": server, "mode": "form", "required": required, "url": null },
            }))
        }
        _ => None,
    }
}

/// The card request an ACP elicitation stands for: `Some` when it is THE
/// elicitation card's (it names its server under [`SERVER_META_KEY`], or it
/// is in url mode), `None` when it stays the question stepper.
pub fn card_request(request: &CreateElicitationRequest) -> Option<Value> {
    let meta = request.meta.as_ref();
    let server = meta.and_then(|m| m.get(SERVER_META_KEY)).and_then(Value::as_str).map(str::to_string);
    let order = strings(meta.and_then(|m| m.get(ORDER_META_KEY)));
    match &request.mode {
        ElicitationMode::Form(form) => {
            let server = server?;
            let schema = serde_json::to_value(&form.requested_schema).ok()?;
            Some(json!({ "server": server, "message": request.message, "mode": "form", "schema": schema, "order": order }))
        }
        ElicitationMode::Url(url) => Some(json!({
            "server": server.unwrap_or_default(),
            "message": request.message,
            "mode": "url",
            "url": url.url,
            "elicitationId": url.elicitation_id.to_string(),
        })),
        // A newer mode with the server named: nothing the card can render
        // honestly (the caller declines it).
        _ => server.map(|server| json!({ "server": server, "message": request.message, "mode": "unknown" })),
    }
}

/// An ACP elicitation that IS the card's but cannot be drawn honestly (a
/// url that is not http(s), an unknown mode): the host DECLINES it at once,
/// never posting a card.
pub fn undrawable(request: &CreateElicitationRequest) -> bool {
    card_request(request).is_some_and(|card| elicitation_card(&card).is_none())
}

/// The property names of a raw MCP `requestedSchema` in the order the
/// server sent them (`orderMetaKey`'s value).
pub fn schema_order(requested_schema: Option<&Value>) -> Vec<String> {
    requested_schema
        .and_then(|s| s.get("properties"))
        .and_then(Value::as_object)
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default()
}

/// The action an answered (or dismissed) card carries back: Submit / I'm
/// done = accept, Decline = decline, anything else (Cancel, the ×, a label
/// never offered) = cancel. Never an accept by default.
pub fn elicitation_action(card: &Value) -> ElicitationAction {
    if card.get("dismissed").and_then(Value::as_bool) == Some(true) {
        return ElicitationAction::Cancel;
    }
    let pick = card
        .get("answers")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .or_else(|| card.get("answer").and_then(Value::as_str))
        .unwrap_or("")
        .trim();
    if pick == COPY_SUBMIT || pick == COPY_DONE {
        ElicitationAction::Accept
    } else if pick == COPY_DECLINE {
        ElicitationAction::Decline
    } else {
        ElicitationAction::Cancel
    }
}

/// A value counts as given: a non-blank string, a number, any boolean, a
/// non-empty list.
fn given(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Number(_)) | Some(Value::Bool(_)) => true,
        Some(Value::Array(a)) => !a.is_empty(),
        _ => false,
    }
}

fn form_values(surface_data: Option<&Value>) -> Map<String, Value> {
    surface_data.and_then(|d| d.get(FORM_KEY)).and_then(Value::as_object).cloned().unwrap_or_default()
}

/// A JS-like number: a JSON number, or a string that parses as one.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
}

fn number_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

/// The accepted content: each property's value from the surface data,
/// coerced to its type; empty optional values left out. `None` = a required
/// value is missing or a value has the wrong type (never sent).
pub fn elicitation_content(schema: &Value, surface_data: Option<&Value>) -> Option<Map<String, Value>> {
    let form = form_values(surface_data);
    let props = properties(schema);
    let required = strings(schema.get("required"));
    let mut out = Map::new();
    for name in elicitation_field_order(schema, &[]) {
        let property = &props[&name];
        let value = form.get(&name);
        if !given(value) {
            if required.contains(&name) {
                return None;
            }
            continue;
        }
        let value = value?;
        match kind(property) {
            "boolean" => {
                out.insert(name, Value::Bool(value.as_bool()?));
            }
            ty @ ("integer" | "number") => {
                let n = number(value)?;
                if ty == "integer" && n.fract() != 0.0 {
                    return None;
                }
                out.insert(name, if value.is_number() && ty == "number" { value.clone() } else { number_value(n) });
            }
            "array" => {
                let items = value.as_array()?;
                let picks: Vec<&str> = items.iter().map(Value::as_str).collect::<Option<_>>()?;
                let allowed: Vec<String> = choice_options(property).into_iter().map(|(_, v)| v).collect();
                if !allowed.is_empty() && picks.iter().any(|p| !allowed.iter().any(|a| a == p)) {
                    return None;
                }
                out.insert(name, value.clone());
            }
            _ => {
                let s = value.as_str()?;
                let allowed: Vec<String> = choice_options(property).into_iter().map(|(_, v)| v).collect();
                if !allowed.is_empty() && !allowed.iter().any(|a| a == s) {
                    return None;
                }
                out.insert(name, Value::String(s.trim().to_string()));
            }
        }
    }
    Some(out)
}

/// The reply an answered card carries back for the card `request`
/// ([`card_request`]'s value). `None` = an accepted form whose content is
/// not valid ([`elicitation_content`]): the card stays waiting.
pub fn elicitation_reply(request: &Value, card: &Value) -> Option<ElicitationReply> {
    match elicitation_action(card) {
        ElicitationAction::Decline => Some(ElicitationReply::Decline),
        ElicitationAction::Cancel => Some(ElicitationReply::Cancel),
        ElicitationAction::Accept => match request.get("mode").and_then(Value::as_str) {
            Some("form") => {
                let schema = request.get("schema")?;
                let content = elicitation_content(schema, card.get("surfaceData").filter(|d| d.is_object()))?;
                Some(ElicitationReply::Accept(Some(content)))
            }
            _ => Some(ElicitationReply::Accept(None)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../../packages/domain-contract/fixtures/elicitation-card.json")).unwrap()
    }

    #[test]
    fn the_constants_are_the_fixtures() {
        let f = fixture();
        assert_eq!(f["serverMetaKey"], SERVER_META_KEY);
        assert_eq!(f["orderMetaKey"], ORDER_META_KEY);
        assert_eq!(f["formKey"], FORM_KEY);
        let copy = &f["copy"];
        assert_eq!(copy["submit"], COPY_SUBMIT);
        assert_eq!(copy["decline"], COPY_DECLINE);
        assert_eq!(copy["cancel"], COPY_CANCEL);
        assert_eq!(copy["done"], COPY_DONE);
        assert_eq!(copy["unknownServer"], COPY_UNKNOWN_SERVER);
        for w in ["punycode", "ip", "insecure"] {
            assert_eq!(copy["warnings"][w], warning_copy(w));
        }
        assert_eq!(f["multiPartSuffixes"], json!(MULTI_PART_SUFFIXES));
    }

    #[test]
    fn elicitation_card_cases() {
        for case in fixture()["cardCases"].as_array().unwrap() {
            let got = elicitation_card(&case["request"]).unwrap_or(Value::Null);
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn elicitation_url_cases() {
        for case in fixture()["urlCases"].as_array().unwrap() {
            let got = elicitation_url(case["url"].as_str().unwrap()).map(|r| serde_json::to_value(r).unwrap()).unwrap_or(Value::Null);
            assert_eq!(got, case["expected"], "{}", case["url"]);
        }
    }

    #[test]
    fn elicitation_action_cases() {
        for case in fixture()["actionCases"].as_array().unwrap() {
            assert_eq!(elicitation_action(&case["card"]).as_str(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn elicitation_content_cases() {
        for case in fixture()["contentCases"].as_array().unwrap() {
            let data = Some(&case["surfaceData"]).filter(|d| !d.is_null());
            let got = elicitation_content(&case["schema"], data).map(Value::Object).unwrap_or(Value::Null);
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn elicitation_an_acp_request_round_trips_to_the_card() {
        use agent_client_protocol::v1::schema::{ElicitationFormMode, ElicitationSchema, ElicitationSessionScope, ElicitationUrlMode, SessionId};
        // The fixture's first form, through ACP's schema (a sorted map: the
        // order rides `orderMetaKey`).
        let f = fixture();
        let case = &f["cardCases"][0];
        let mut schema_json = case["request"]["schema"].clone();
        schema_json["type"] = json!("object");
        // MCP's multi-select names its items' type (the fixture's request is
        // the TS reference's shape, which leaves it out).
        schema_json["properties"]["labels"]["items"]["type"] = json!("string");
        let schema: ElicitationSchema = serde_json::from_value(schema_json).unwrap();
        let scope = ElicitationSessionScope::new(SessionId::new("s1"));
        let mut meta = Map::new();
        meta.insert(SERVER_META_KEY.into(), json!("linear"));
        meta.insert(ORDER_META_KEY.into(), case["request"]["order"].clone());
        let request = CreateElicitationRequest::new(ElicitationFormMode::new(scope.clone(), schema.clone()), "Where should the issue go?").meta(meta);
        let card = elicitation_card(&card_request(&request).unwrap()).unwrap();
        assert_eq!(card["surface"]["root"]["children"], case["expected"]["surface"]["root"]["children"]);
        assert_eq!(card["surface"]["data"], case["expected"]["surface"]["data"]);
        assert_eq!(card["elicitation"], case["expected"]["elicitation"]);

        // Without the server it stays the stepper; a url is always the card.
        let plain = CreateElicitationRequest::new(ElicitationFormMode::new(scope.clone(), schema), "m");
        assert_eq!(card_request(&plain), None);
        let url = CreateElicitationRequest::new(ElicitationUrlMode::new(scope, "e-1", "https://github.com/login"), "Sign in");
        let card = elicitation_card(&card_request(&url).unwrap()).unwrap();
        assert_eq!(card["elicitation"]["server"], COPY_UNKNOWN_SERVER);
        assert_eq!(card["options"], json!([COPY_DONE, COPY_DECLINE]));
    }

    #[test]
    fn elicitation_reply_never_accepts_by_default() {
        let request = json!({ "mode": "form", "schema": { "properties": { "t": { "type": "string" } }, "required": ["t"] } });
        let data = json!({ "form": { "t": "x" } });
        assert_eq!(
            elicitation_reply(&request, &json!({ "answers": ["Submit"], "surfaceData": data })),
            Some(ElicitationReply::Accept(Some(serde_json::from_value(json!({ "t": "x" })).unwrap())))
        );
        // An invalid submit leaves the card waiting.
        assert_eq!(elicitation_reply(&request, &json!({ "answers": ["Submit"], "surfaceData": { "form": { "t": "" } } })), None);
        assert_eq!(elicitation_reply(&request, &json!({ "answers": ["Decline"] })), Some(ElicitationReply::Decline));
        assert_eq!(elicitation_reply(&request, &json!({ "answers": ["Submit"], "dismissed": true })), Some(ElicitationReply::Cancel));
        let url = json!({ "mode": "url", "url": "https://x.example" });
        assert_eq!(elicitation_reply(&url, &json!({ "answers": ["I'm done"] })), Some(ElicitationReply::Accept(None)));
        assert_eq!(ElicitationReply::Accept(None).mcp(), json!({ "action": "accept" }));
        assert_eq!(ElicitationReply::Decline.mcp(), json!({ "action": "decline" }));
    }
}
