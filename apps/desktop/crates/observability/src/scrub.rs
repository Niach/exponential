//! #LOGS-2: what leaves the machine. Every event, transaction, breadcrumb
//! and log passes through here first: `Authorization`, cookies and every key
//! matching `token|secret|password|key|ticket|session|dsn` lose their value,
//! bearer tokens, URL userinfo and secret-looking query parameters are cut
//! out of free text, and the host name never ships.

use sentry::protocol::{Breadcrumb, Event, Map, Value};

pub const FILTERED: &str = "[Filtered]";

/// Whether a key's VALUE must not leave the machine.
pub fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "authorization"
        || key.contains("cookie")
        || ["token", "secret", "password", "key", "ticket", "session", "dsn"]
            .iter()
            .any(|needle| key.contains(needle))
}

/// Free text with bearer tokens, URL credentials and secret query values cut.
pub fn scrub_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        // `Bearer <token>` (any case).
        if let Some(stripped) = strip_prefix_ci(rest, "bearer ") {
            out.push_str(&rest[..7]);
            out.push_str(FILTERED);
            rest = stripped.trim_start_matches(|c: char| !c.is_whitespace() && c != '"' && c != '\'');
            continue;
        }
        // `scheme://userinfo@host` → `scheme://[Filtered]@host`.
        if rest.starts_with("://") {
            let after = &rest[3..];
            let end = after.find(|c: char| c == '/' || c.is_whitespace() || c == '"').unwrap_or(after.len());
            if let Some(at) = after[..end].rfind('@') {
                out.push_str("://");
                out.push_str(FILTERED);
                rest = &after[at..];
                continue;
            }
        }
        // `?token=…` / `&session_id=…` / `secret: …` style pairs.
        if let Some(value_start) = sensitive_pair_at(rest, out.chars().last()) {
            // `Authorization: Bearer <token>` keeps its scheme.
            let value_start = match strip_prefix_ci(&rest[value_start..], "bearer ") {
                Some(_) => value_start + 7,
                None => value_start,
            };
            out.push_str(&rest[..value_start]);
            out.push_str(FILTERED);
            let value = &rest[value_start..];
            let end = value
                .find(|c: char| c == '&' || c == ',' || c == ';' || c == '"' || c == '\'' || c.is_whitespace())
                .unwrap_or(value.len());
            rest = &value[end..];
            continue;
        }
        let ch = rest.chars().next().expect("non-empty");
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

/// At a word start, `<key>=` or `<key>: ` with a sensitive key: the offset
/// of the value.
fn sensitive_pair_at(text: &str, previous: Option<char>) -> Option<usize> {
    if previous.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    let key_len = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(text.len());
    if key_len == 0 || !is_sensitive_key(&text[..key_len]) {
        return None;
    }
    let tail = &text[key_len..];
    let separator = if tail.starts_with('=') {
        1
    } else if tail.starts_with(": ") {
        2
    } else {
        return None;
    };
    let value_start = key_len + separator;
    // `key=` followed by nothing is no secret.
    let value = &text[value_start..];
    (!value.is_empty() && !value.starts_with(char::is_whitespace)).then_some(value_start)
}

/// A JSON value, recursively: sensitive keys filtered, strings scrubbed.
pub fn scrub_value(value: &mut Value) {
    match value {
        Value::String(text) => {
            let scrubbed = scrub_text(text);
            if scrubbed != *text {
                *text = scrubbed;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(scrub_value),
        Value::Object(map) => scrub_json_map(map),
        _ => {}
    }
}

fn scrub_json_map(map: &mut serde_json::Map<String, Value>) {
    for (key, value) in map.iter_mut() {
        if is_sensitive_key(key) {
            *value = Value::String(FILTERED.into());
        } else {
            scrub_value(value);
        }
    }
}

fn scrub_map(map: &mut Map<String, Value>) {
    for (key, value) in map.iter_mut() {
        if is_sensitive_key(key) {
            *value = Value::String(FILTERED.into());
        } else {
            scrub_value(value);
        }
    }
}

fn scrub_string_map(map: &mut Map<String, String>) {
    for (key, value) in map.iter_mut() {
        if is_sensitive_key(key) {
            *value = FILTERED.into();
        } else {
            *value = scrub_text(value);
        }
    }
}

/// `before_send` (errors AND transactions share the event's maps).
pub fn scrub_event(mut event: Event<'static>) -> Event<'static> {
    event.server_name = None;
    if let Some(user) = event.user.as_mut() {
        // The pseudonymous id only: never email, name or IP.
        user.email = None;
        user.username = None;
        user.ip_address = None;
        user.other.clear();
    }
    if let Some(request) = event.request.as_mut() {
        request.cookies = None;
        scrub_string_map(&mut request.headers);
        scrub_string_map(&mut request.env);
        if let Some(query) = request.query_string.as_mut() {
            *query = scrub_text(query);
        }
        if let Some(data) = request.data.as_mut() {
            *data = scrub_text(data);
        }
        if let Some(url) = request.url.as_mut() {
            if let Ok(parsed) = scrub_text(url.as_str()).parse() {
                *url = parsed;
            }
        }
    }
    if let Some(message) = event.message.as_mut() {
        *message = scrub_text(message);
    }
    if let Some(entry) = event.logentry.as_mut() {
        entry.message = scrub_text(&entry.message);
        entry.params.iter_mut().for_each(scrub_value);
    }
    for exception in event.exception.values.iter_mut() {
        if let Some(value) = exception.value.as_mut() {
            *value = scrub_text(value);
        }
    }
    scrub_map(&mut event.extra);
    scrub_string_map(&mut event.tags);
    for context in event.contexts.values_mut() {
        if let sentry::protocol::Context::Other(map) = context {
            scrub_map(map);
        }
    }
    for breadcrumb in event.breadcrumbs.values.iter_mut() {
        scrub_breadcrumb_in_place(breadcrumb);
    }
    event
}

/// A transaction's spans carry `data` and `tags` too.
pub fn scrub_transaction(mut transaction: sentry::protocol::Transaction<'static>) -> sentry::protocol::Transaction<'static> {
    transaction.server_name = None;
    if let Some(user) = transaction.user.as_mut() {
        user.email = None;
        user.username = None;
        user.ip_address = None;
        user.other.clear();
    }
    scrub_map(&mut transaction.extra);
    scrub_string_map(&mut transaction.tags);
    for span in transaction.spans.iter_mut() {
        scrub_map(&mut span.data);
        scrub_string_map(&mut span.tags);
        if let Some(description) = span.description.as_mut() {
            *description = scrub_text(description);
        }
    }
    transaction
}

fn scrub_breadcrumb_in_place(breadcrumb: &mut Breadcrumb) {
    if let Some(message) = breadcrumb.message.as_mut() {
        *message = scrub_text(message);
    }
    scrub_map(&mut breadcrumb.data);
}

/// `before_breadcrumb`.
pub fn scrub_breadcrumb(mut breadcrumb: Breadcrumb) -> Breadcrumb {
    scrub_breadcrumb_in_place(&mut breadcrumb);
    breadcrumb
}

/// `before_send_log`.
pub fn scrub_log(mut log: sentry::protocol::Log) -> sentry::protocol::Log {
    log.body = scrub_text(&log.body);
    for (key, value) in log.attributes.iter_mut() {
        if is_sensitive_key(key) {
            value.0 = Value::String(FILTERED.into());
        } else {
            scrub_value(&mut value.0);
        }
    }
    log
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Request, User};

    #[test]
    fn sensitive_keys() {
        for key in ["Authorization", "cookie", "Set-Cookie", "access_token", "client_secret", "password", "api_key", "apiKey", "ticket", "session_id", "SessionToken", "dsn", "X-Exp-Token"] {
            assert!(is_sensitive_key(key), "{key}");
        }
        for key in ["user_id", "view", "shape", "duration_ms", "status", "team"] {
            assert!(!is_sensitive_key(key), "{key}");
        }
    }

    #[test]
    fn free_text() {
        assert_eq!(scrub_text("sent Authorization: Bearer abc.def-123 ok"), "sent Authorization: Bearer [Filtered] ok");
        assert_eq!(scrub_text("GET https://x.test/api?token=s3cr3t&team=a"), "GET https://x.test/api?token=[Filtered]&team=a");
        assert_eq!(scrub_text("dsn https://pubkey@glitchtip.example/3"), "dsn https://[Filtered]@glitchtip.example/3");
        assert_eq!(scrub_text("password: hunter2, user: ann"), "password: [Filtered], user: ann");
        assert_eq!(scrub_text("session=abc; theme=dark"), "session=[Filtered]; theme=dark");
        assert_eq!(scrub_text("monkey=1 keyboard ok"), "monkey=[Filtered] keyboard ok");
        assert_eq!(scrub_text("render took 40ms in view=Board"), "render took 40ms in view=Board");
        assert_eq!(scrub_text("emoji ✓ stays"), "emoji ✓ stays");
    }

    #[test]
    fn events_lose_headers_cookies_user_details_and_host() {
        let mut request = Request::default();
        request.headers.insert("Authorization".into(), "Bearer abc".into());
        request.headers.insert("Accept".into(), "json".into());
        request.cookies = Some("better-auth.session_token=xyz".into());
        let mut event = Event {
            server_name: Some("dannys-macbook".into()),
            user: Some(User { id: Some("u1".into()), email: Some("a@b.c".into()), username: Some("ann".into()), ..Default::default() }),
            request: Some(request),
            message: Some("failed with token=abc".into()),
            ..Default::default()
        };
        event.extra.insert("ticket".into(), "t-123".into());
        event.extra.insert("nested".into(), serde_json::json!({"secret": "x", "ok": "Bearer zzz"}));
        event.tags.insert("steer_session".into(), "s-1".into());
        let mut crumb = Breadcrumb { message: Some("Authorization: Bearer q".into()), ..Default::default() };
        crumb.data.insert("api_key".into(), "k".into());
        event.breadcrumbs.values.push(crumb);

        let event = scrub_event(event);
        assert_eq!(event.server_name, None);
        let user = event.user.unwrap();
        assert_eq!(user.id.as_deref(), Some("u1"));
        assert_eq!((user.email, user.username), (None, None));
        let request = event.request.unwrap();
        assert_eq!(request.headers["Authorization"], FILTERED);
        assert_eq!(request.headers["Accept"], "json");
        assert_eq!(request.cookies, None);
        assert_eq!(event.message.as_deref(), Some("failed with token=[Filtered]"));
        assert_eq!(event.extra["ticket"], FILTERED);
        assert_eq!(event.extra["nested"], serde_json::json!({"secret": FILTERED, "ok": "Bearer [Filtered]"}));
        assert_eq!(event.tags["steer_session"], FILTERED);
        let crumb = &event.breadcrumbs.values[0];
        assert_eq!(crumb.message.as_deref(), Some("Authorization: Bearer [Filtered]"));
        assert_eq!(crumb.data["api_key"], FILTERED);
    }

    #[test]
    fn breadcrumbs() {
        let mut crumb = Breadcrumb { message: Some("GET /x?password=p".into()), ..Default::default() };
        crumb.data.insert("Cookie".into(), "c".into());
        crumb.data.insert("url".into(), "https://u:p@host/x".into());
        let crumb = scrub_breadcrumb(crumb);
        assert_eq!(crumb.message.as_deref(), Some("GET /x?password=[Filtered]"));
        assert_eq!(crumb.data["Cookie"], FILTERED);
        assert_eq!(crumb.data["url"], "https://[Filtered]@host/x");
    }
}
