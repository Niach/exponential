//! What never leaves the machine (#LOGS-2): `Authorization`, cookies, and
//! every value under a key matching token|secret|password|key|ticket|
//! session|dsn are replaced with `[Filtered]`, in events (request, tags,
//! extra, contexts, messages, exceptions), breadcrumbs and logs. Free text
//! loses `Bearer …`/`Basic …` credentials, a URL's userinfo
//! (`https://key@host`) and `token=…`-style pairs. The hostname and every
//! user field but the pseudonymous id are dropped.

use sentry::protocol::{Breadcrumb, Context, Event, Log, Map, Value};

pub const FILTERED: &str = "[Filtered]";

const SENSITIVE: &[&str] = &["token", "secret", "password", "passwd", "key", "ticket", "session", "dsn", "authorization", "cookie", "auth"];

/// Whether a value under `key` is a credential.
pub fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    SENSITIVE.iter().any(|s| k.contains(s))
}

pub fn event(mut e: Event<'static>) -> Event<'static> {
    e.server_name = None;
    if let Some(user) = e.user.as_mut() {
        user.email = None;
        user.ip_address = None;
        user.username = None;
        user.other.clear();
    }
    if let Some(req) = e.request.as_mut() {
        req.cookies = None;
        req.data = None;
        req.env.clear();
        req.query_string = req.query_string.take().map(|q| text(&q));
        req.url = req.url.take().and_then(|u| text(u.as_str()).parse().ok());
        for (k, v) in req.headers.iter_mut() {
            *v = if is_sensitive_key(k) { FILTERED.to_owned() } else { text(v) };
        }
    }
    e.message = e.message.take().map(|m| text(&m));
    if let Some(entry) = e.logentry.as_mut() {
        entry.message = text(&entry.message);
        entry.params.iter_mut().for_each(value);
    }
    for ex in e.exception.values.iter_mut() {
        ex.value = ex.value.take().map(|v| text(&v));
    }
    for (k, v) in e.tags.iter_mut() {
        *v = if is_sensitive_key(k) { FILTERED.to_owned() } else { text(v) };
    }
    map(&mut e.extra);
    for ctx in e.contexts.values_mut() {
        if let Context::Other(m) = ctx {
            map(m);
        }
    }
    for b in e.breadcrumbs.values.iter_mut() {
        breadcrumb_in_place(b);
    }
    e
}

pub fn breadcrumb(mut b: Breadcrumb) -> Breadcrumb {
    breadcrumb_in_place(&mut b);
    b
}

fn breadcrumb_in_place(b: &mut Breadcrumb) {
    b.message = b.message.take().map(|m| text(&m));
    map(&mut b.data);
}

pub fn log(mut l: Log) -> Log {
    l.body = text(&l.body);
    for (k, a) in l.attributes.iter_mut() {
        if is_sensitive_key(k) {
            a.0 = Value::String(FILTERED.to_owned());
        } else {
            value(&mut a.0);
        }
    }
    l
}

/// Every value under a sensitive key filtered, the rest scrubbed as text.
pub fn map(m: &mut Map<String, Value>) {
    for (k, v) in m.iter_mut() {
        if is_sensitive_key(k) {
            *v = Value::String(FILTERED.to_owned());
        } else {
            value(v);
        }
    }
}

pub fn value(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, v) in o.iter_mut() {
                if is_sensitive_key(k) {
                    *v = Value::String(FILTERED.to_owned());
                } else {
                    value(v);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(value),
        Value::String(s) => *s = text(s),
        _ => {}
    }
}

/// Free text without credentials.
pub fn text(input: &str) -> String {
    redact_pairs(&redact_userinfo(&redact_auth_schemes(input)))
}

fn is_value_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '&' | ';' | ',' | '"' | '\'' | ')' | ']' | '}' | '>')
}

/// `Bearer abc` / `Basic abc` → `Bearer [Filtered]`.
fn redact_auth_schemes(s: &str) -> String {
    // ASCII lowercasing keeps every byte offset (and char boundary) of `s`.
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let word_start = i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric();
        let scheme = ["bearer ", "basic "].into_iter().find(|p| word_start && lower[i..].starts_with(p));
        match scheme {
            Some(p) => {
                out.push_str(&s[i..i + p.len()]);
                i += p.len();
                let end = s[i..].find(is_value_end).map_or(s.len(), |e| i + e);
                if end > i {
                    out.push_str(FILTERED);
                }
                i = end;
            }
            None => {
                let ch = s[i..].chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

/// `https://user:pass@host/…` → `https://[Filtered]@host/…`.
fn redact_userinfo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("://") {
        let (head, tail) = rest.split_at(pos + 3);
        out.push_str(head);
        let end = tail.find(|c: char| matches!(c, '/' | '?' | '#') || is_value_end(c)).unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) => {
                out.push_str(FILTERED);
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// `token=abc&x=1` → `token=[Filtered]&x=1`.
fn redact_pairs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(off) = s[i..].find('=') {
        let eq = i + off;
        let key_start = s[..eq]
            .char_indices()
            .rev()
            .find(|&(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
            .map_or(0, |(p, c)| p + c.len_utf8());
        let key = &s[key_start..eq];
        out.push_str(&s[i..=eq]);
        let start = eq + 1;
        let end = s[start..].find(is_value_end).map_or(s.len(), |e| start + e);
        let v = &s[start..end];
        if is_sensitive_key(key) && !v.is_empty() && v != FILTERED {
            out.push_str(FILTERED);
        } else {
            out.push_str(v);
        }
        i = end;
    }
    out.push_str(&s[i..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Exception, LogAttribute, LogLevel, Request, User, Values};
    use serde_json::json;

    #[test]
    fn sensitive_keys() {
        for k in ["Authorization", "cookie", "Set-Cookie", "x-api-key", "access_token", "client_secret", "password", "relay_ticket", "session_id", "EXP_SENTRY_DSN", "apiKey"] {
            assert!(is_sensitive_key(k), "{k}");
        }
        for k in ["output", "frame_ms", "component", "status"] {
            assert!(!is_sensitive_key(k), "{k}");
        }
    }

    #[test]
    fn free_text() {
        assert_eq!(text("Authorization: Bearer abc.def-123 sent"), "Authorization: Bearer [Filtered] sent");
        assert_eq!(text("basic dXNlcjpwYXNz"), "basic [Filtered]");
        assert_eq!(text("posting to https://pubkey@glitchtip.example/3 now"), "posting to https://[Filtered]@glitchtip.example/3 now");
        assert_eq!(text("https://u:p@h.example:8443/x?y=1"), "https://[Filtered]@h.example:8443/x?y=1");
        assert_eq!(text("GET /api?token=abc123&page=2"), "GET /api?token=[Filtered]&page=2");
        assert_eq!(text("ticket=xyz; mode=fast"), "ticket=[Filtered]; mode=fast");
        assert_eq!(text("a basicity check"), "a basicity check", "only whole words");
        assert_eq!(text("plain words, ünïcödé = fine"), "plain words, ünïcödé = fine");
        assert_eq!(text("session=ü1"), "session=[Filtered]");
        assert_eq!(text(""), "");
    }

    #[test]
    fn an_event_loses_credentials_hostname_and_personal_fields() {
        let mut e = Event::default();
        e.server_name = Some("dennis-laptop".into());
        e.user = Some(User { id: Some("usr_1".into()), email: Some("a@b.c".into()), username: Some("dennis".into()), ip_address: Some(Default::default()), ..Default::default() });
        let mut req = Request::default();
        req.headers.insert("Authorization".into(), "Bearer abc".into());
        req.headers.insert("Cookie".into(), "s=1".into());
        req.headers.insert("Accept".into(), "*/*".into());
        req.cookies = Some("s=1".into());
        e.request = Some(req);
        e.message = Some("failed with token=abc".into());
        e.exception = Values::from(vec![Exception { ty: "Error".into(), value: Some("Bearer zzz rejected".into()), ..Default::default() }]);
        e.tags.insert("session".into(), "abc".into());
        e.tags.insert("component".into(), "compositor".into());
        e.extra.insert("nested".into(), json!({"password": "hunter2", "list": [{"secret": 1}, "ok"], "url": "https://k@h/1"}));
        let mut ctx = Map::new();
        ctx.insert("dsn".into(), json!("https://k@h/1"));
        e.contexts.insert("Rust Tracing Fields".into(), Context::Other(ctx));
        let mut crumb = Breadcrumb::default();
        crumb.message = Some("relay ticket=abc".into());
        crumb.data.insert("api_key".into(), json!("k"));
        e.breadcrumbs = Values::from(vec![crumb]);

        let e = event(e);
        assert_eq!(e.server_name, None);
        let user = e.user.unwrap();
        assert_eq!(user.id.as_deref(), Some("usr_1"));
        assert!(user.email.is_none() && user.username.is_none() && user.ip_address.is_none());
        let req = e.request.unwrap();
        assert_eq!(req.headers["Authorization"], FILTERED);
        assert_eq!(req.headers["Cookie"], FILTERED);
        assert_eq!(req.headers["Accept"], "*/*");
        assert!(req.cookies.is_none());
        assert_eq!(e.message.as_deref(), Some("failed with token=[Filtered]"));
        assert_eq!(e.exception.values[0].value.as_deref(), Some("Bearer [Filtered] rejected"));
        assert_eq!(e.tags["session"], FILTERED);
        assert_eq!(e.tags["component"], "compositor");
        assert_eq!(e.extra["nested"], json!({"password": FILTERED, "list": [{"secret": FILTERED}, "ok"], "url": "https://[Filtered]@h/1"}));
        match &e.contexts["Rust Tracing Fields"] {
            Context::Other(m) => assert_eq!(m["dsn"], FILTERED),
            other => panic!("{other:?}"),
        }
        assert_eq!(e.breadcrumbs.values[0].message.as_deref(), Some("relay ticket=[Filtered]"));
        assert_eq!(e.breadcrumbs.values[0].data["api_key"], FILTERED);
    }

    #[test]
    fn a_breadcrumb_on_its_own() {
        let mut b = Breadcrumb::default();
        b.data.insert("Authorization".into(), json!("Bearer x"));
        b.data.insert("output".into(), json!("DP-1"));
        let b = breadcrumb(b);
        assert_eq!(b.data["Authorization"], FILTERED);
        assert_eq!(b.data["output"], "DP-1");
    }

    #[test]
    fn a_log_loses_credentials() {
        let mut attributes = Map::new();
        attributes.insert("token".into(), LogAttribute(json!("abc")));
        attributes.insert("output".into(), LogAttribute(json!("DP-1")));
        let l = Log {
            level: LogLevel::Warn,
            body: "connect https://u:p@relay.example failed, cookie=abc".into(),
            trace_id: None,
            timestamp: std::time::SystemTime::now(),
            severity_number: None,
            attributes,
        };
        let l = log(l);
        assert_eq!(l.body, "connect https://[Filtered]@relay.example failed, cookie=[Filtered]");
        assert_eq!(l.attributes["token"].0, json!(FILTERED));
        assert_eq!(l.attributes["output"].0, json!("DP-1"));
    }
}
