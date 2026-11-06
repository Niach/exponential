//! What reaches the runner from outside the Engine, mapped onto it: the
//! shell's own events (runner.json `EVENTS`), the window's width (the form
//! factor), deep links `<config id>://<screen>?<params>` and notification taps.

use eengine::consent::ConsentAnswer;
use eengine::{FormFactor, LifecycleEvent};
use serde_json::{Map, Value};

/// An event the runner handles itself; `None` from [`runner_event`] = not one.
#[derive(Clone, Debug, PartialEq)]
pub enum RunnerEvent {
    /// `runner.navigate {screen}`: the Engine's `app.navigate`.
    Navigate { screen: String },
    /// `runner.permissions`: paint surface `permissions`.
    Permissions,
    /// `consent.toggle {scope}`: the grant write, then repaint `permissions`.
    ConsentToggle { scope: String },
    /// `consent.answer {answer}`: the pending call's answer, then close `consent`.
    ConsentAnswer { answer: ConsentAnswer },
}

/// A client `action` message (genUI's `actionMessage`) → the runner's event.
pub fn runner_event(message: &Value) -> Option<RunnerEvent> {
    let action = message.get("action")?;
    let context = action.get("context");
    let text = |key: &str| context.and_then(|c| c.get(key)).and_then(Value::as_str).map(str::to_string);
    match action.get("name")?.as_str()? {
        "runner.navigate" => Some(RunnerEvent::Navigate { screen: text("screen")? }),
        "runner.permissions" => Some(RunnerEvent::Permissions),
        "consent.toggle" => Some(RunnerEvent::ConsentToggle { scope: text("scope")? }),
        "consent.answer" => {
            let answer = serde_json::from_value(Value::String(text("answer")?)).ok()?;
            Some(RunnerEvent::ConsentAnswer { answer })
        }
        _ => None,
    }
}

/// `wide` from genUI's `md` breakpoint up, else `compact`.
pub fn form_factor(width: f32) -> FormFactor {
    if width >= eui_core::conditions::DEFAULT_FORM_FACTOR_BREAKPOINT {
        FormFactor::Wide
    } else {
        FormFactor::Compact
    }
}

/// `<config id>://<screen>?<params>` → a deep link (`None` = not this app's
/// scheme). The scheme compares without case (URL schemes are lowercased).
pub fn deep_link(id: &str, link: &str) -> Option<LifecycleEvent> {
    let url = url::Url::parse(link).ok()?;
    if !url.scheme().eq_ignore_ascii_case(id) {
        return None;
    }
    let screen = url.host_str().map(str::to_string).filter(|s| !s.is_empty()).or_else(|| Some(url.path().trim_matches('/').to_string()).filter(|p| !p.is_empty()));
    let params: Map<String, Value> = url.query_pairs().map(|(k, v)| (k.into_owned(), Value::String(v.into_owned()))).collect();
    Some(LifecycleEvent::DeepLink { screen, params: Some(Value::Object(params)) })
}

/// The deep links among the process arguments (Linux and Windows hand a
/// URL-scheme launch its link as an argument; macOS sends it as an event).
pub fn links_in_args(id: &str, args: impl IntoIterator<Item = String>) -> Vec<LifecycleEvent> {
    args.into_iter().skip(1).filter_map(|a| deep_link(id, &a)).collect()
}

/// A notification's `tap` (the bridge stores the lifecycle event itself).
pub fn notification_tap(tap: &Value) -> Option<LifecycleEvent> {
    match serde_json::from_value(tap.clone()).ok()? {
        event @ LifecycleEvent::NotificationTap { .. } => Some(event),
        _ => None,
    }
}

/// The window became active (`foreground`) or not (`background`).
pub fn activation(active: bool) -> LifecycleEvent {
    if active {
        LifecycleEvent::Foreground
    } else {
        LifecycleEvent::Background
    }
}

/// lifecycle.json `copy.refused`: the toast for a refused link.
pub fn refused_link_copy() -> String {
    eengine::contract::LIFECYCLE["copy"]["refused"].as_str().unwrap_or("Couldn't open the link").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn action(name: &str, context: Value) -> Value {
        eui_core::host::action_message("app", "c", name, context, None, "2026-11-06T00:00:00.000Z")
    }

    #[test]
    fn the_shell_events() {
        assert_eq!(runner_event(&action("runner.navigate", json!({"screen": "albums"}))), Some(RunnerEvent::Navigate { screen: "albums".into() }));
        assert_eq!(runner_event(&action("runner.permissions", Value::Null)), Some(RunnerEvent::Permissions));
        assert_eq!(runner_event(&action("consent.toggle", json!({"scope": "photos.read"}))), Some(RunnerEvent::ConsentToggle { scope: "photos.read".into() }));
        assert_eq!(runner_event(&action("consent.answer", json!({"answer": "once"}))), Some(RunnerEvent::ConsentAnswer { answer: ConsentAnswer::Once }));
        assert_eq!(runner_event(&action("consent.answer", json!({"answer": "maybe"}))), None);
        assert_eq!(runner_event(&action("app.save", json!({}))), None);
        assert_eq!(runner_event(&json!({"error": {}})), None);
    }

    #[test]
    fn the_form_factor_crosses_at_md() {
        assert_eq!(form_factor(767.0), FormFactor::Compact);
        assert_eq!(form_factor(768.0), FormFactor::Wide);
    }

    #[test]
    fn deep_links_of_this_app_only() {
        assert_eq!(
            deep_link("at.acme.Album", "at.acme.album://album?id=a1&x=%20y"),
            Some(LifecycleEvent::DeepLink { screen: Some("album".into()), params: Some(json!({"id": "a1", "x": " y"})) })
        );
        assert_eq!(deep_link("at.acme.album", "at.acme.album://"), Some(LifecycleEvent::DeepLink { screen: None, params: Some(json!({})) }));
        assert_eq!(deep_link("at.acme.album", "at.acme.other://album"), None);
        assert_eq!(deep_link("at.acme.album", "not a link"), None);
        let args = ["runner", "--flag", "at.acme.album://albums"].map(String::from);
        assert_eq!(links_in_args("at.acme.album", args).len(), 1);
    }

    #[test]
    fn a_tap_is_the_stored_event() {
        let tap = json!({"type": "notificationTap", "app": "acme.album", "screen": "album", "params": {"id": "a1"}});
        assert_eq!(notification_tap(&tap), Some(LifecycleEvent::NotificationTap { app: "acme.album".into(), screen: Some("album".into()), params: Some(json!({"id": "a1"})) }));
        assert_eq!(notification_tap(&json!({"type": "launch"})), None);
        assert_eq!(activation(false), LifecycleEvent::Background);
        assert_eq!(refused_link_copy(), "Couldn't open the link");
    }
}
