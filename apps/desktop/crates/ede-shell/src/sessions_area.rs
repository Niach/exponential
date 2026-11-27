//! #EDE-239: the taskbar's session area, a Cinnamon applet at the right of
//! the taskbar before the status icons. Its sessions are the holder's feed
//! `sessions`, THE IDE sidebar's Active band (`ui::ede_sessions`), shaped
//! by uicore shell.json `sessionArea` into `/sessions`: the chips (agent
//! mark, title, a spinner while it runs, the attention dot while it needs
//! you), `+N` past them, and the panel's rows (sessions-panel.express). A
//! left click opens the panel (a popup anchored at the area), a right click
//! the applet menu (All chats, Needs you). The panel's presses go to the
//! holder over the session bus: `stop {root}`, `answer {messageId,
//! decision|answer}` and `page {key}` (Open).

use api::bus::Request;
use serde_json::{json, Value};

use crate::holder_feeds::HolderFeeds;

/// `/sessions` from the holder's feeds.
pub fn feed(feeds: &HolderFeeds) -> Value {
    let sessions: Vec<uicore_model::session_area::SessionAreaInput> =
        feeds.feeds["sessions"].as_array().into_iter().flatten().filter_map(|s| serde_json::from_value(s.clone()).ok()).collect();
    let area = uicore_model::session_area::session_area(&sessions);
    let mut value = serde_json::to_value(&area).unwrap_or(Value::Null);
    value["moreLabel"] = json!(format!("+{}", area.more));
    value
}

/// What a press in the area or its panel asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    /// The applet menu's entry or the panel's All chats: a page of the holder.
    Menu(String),
    /// A row or its Open: the chat's window.
    Open(String),
    Stop(String),
    Answer { message_id: String, decision: Option<String>, answer: Option<String> },
}

/// `ede_sessions {menu | open | stop | answer, decision?, text?}`.
pub fn press(args: &Value) -> Option<Press> {
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    if let Some(menu) = text("menu") {
        return matches!(menu.as_str(), "chats" | "needs-you").then_some(Press::Menu(menu));
    }
    if let Some(key) = text("open") {
        return Some(Press::Open(key));
    }
    if let Some(root) = text("stop") {
        return Some(Press::Stop(root));
    }
    let message_id = text("answer")?;
    let decision = text("decision").filter(|d| matches!(d.as_str(), "approved" | "rejected"));
    let answer = text("text");
    (decision.is_some() || answer.is_some()).then_some(Press::Answer { message_id, decision, answer })
}

/// The bus request a press sends (Menu and Open are pages: `page {key}`).
pub fn request(press: &Press, token: Option<String>) -> Request {
    match press {
        Press::Menu(key) | Press::Open(key) => Request::Page { key: key.clone(), activation_token: token },
        Press::Stop(root) => Request::Stop { root: root.clone() },
        Press::Answer { message_id, decision, answer } => Request::Answer { message_id: message_id.clone(), decision: decision.clone(), answer: answer.clone() },
    }
}

/// Sends `press` to the holder off the main loop.
pub fn send(press: Press, token: Option<String>) {
    std::thread::spawn(move || {
        let path = api::bus::socket_path(&api::default_data_dir());
        match api::bus::call(&path, &request(&press, token)) {
            Ok(api::bus::Reply::Done) => {}
            Ok(reply) => tracing::warn!(?press, ?reply, "session area: the holder refused"),
            Err(e) => tracing::warn!(?press, "session area: {e}"),
        }
    });
}

/// The sessions under `EDE_SHELL_DEMO` (the goldens): the planning draft's.
pub fn demo() -> Value {
    let trace = json!([
        {"title": "Read sidebar-tree.json", "icon": "trace-read", "status": "done"},
        {"title": "Edited start-menu.express", "icon": "trace-edit", "status": "done"},
        {"title": "Running cargo test -p uicore-model", "icon": "trace-terminal", "status": "running"},
    ]);
    json!([
        {"id": "s1", "title": "EDE-232 start menu tree", "agent": "claude", "state": "running", "team": "Feedback", "device": "mint", "key": "chat:s1", "stop": ["s1"], "ask": null, "trace": trace},
        {"id": "s2", "title": "Fix CI on #3590", "agent": "codex", "state": "needs", "team": "Feedback", "device": "homelab", "key": "chat:s2", "stop": [], "trace": [],
         "ask": {"messageId": "m2", "kind": "question", "title": "Run `bun run migrate` against the disposable database?", "options": [{"answer": "Allow"}, {"answer": "Decline"}]}},
        {"id": "s3", "title": "Release notes", "agent": "claude", "state": "running", "team": "Feedback", "device": "mint", "key": "chat:s3", "stop": ["s3"], "ask": null, "trace": []},
        {"id": "s4", "title": "Sketch the idea board", "agent": "claude", "state": "running", "team": "Yourev", "device": "mint", "key": "chat:s4", "stop": ["s4"], "ask": null, "trace": []},
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_is_the_selector_with_its_labels() {
        let feeds = HolderFeeds { feeds: json!({"sessions": demo()}) };
        let value = feed(&feeds);
        assert_eq!(value["count"], 4);
        assert_eq!(value["needs"], 1);
        assert_eq!(value["chips"].as_array().unwrap().len(), 3);
        assert_eq!((value["more"].as_u64(), value["moreLabel"].as_str()), (Some(1), Some("+1")));
        assert_eq!(value["rows"][1]["actions"][0]["label"], "Allow");
        assert_eq!(value["rows"][0]["subtitle"], "Claude · Feedback · mint");
        assert_eq!(feed(&HolderFeeds::default())["count"], 0, "no holder: no area");
    }

    #[test]
    fn presses_become_bus_requests() {
        assert_eq!(press(&json!({"menu": "chats"})), Some(Press::Menu("chats".into())));
        assert_eq!(press(&json!({"menu": "settings"})), None, "only the applet menu's pages");
        assert_eq!(press(&json!({"open": "chat:r1"})), Some(Press::Open("chat:r1".into())));
        assert_eq!(press(&json!({"stop": "r1"})), Some(Press::Stop("r1".into())));
        let approve = press(&json!({"answer": "m1", "decision": "approved", "text": null})).unwrap();
        assert_eq!(request(&approve, None), Request::Answer { message_id: "m1".into(), decision: Some("approved".into()), answer: None });
        let pick = press(&json!({"answer": "m2", "decision": null, "text": "Allow"})).unwrap();
        assert_eq!(request(&pick, None), Request::Answer { message_id: "m2".into(), decision: None, answer: Some("Allow".into()) });
        assert_eq!(press(&json!({"answer": "m3"})), None, "an answer says what");
        assert_eq!(request(&Press::Open("chat:r1".into()), Some("t".into())), Request::Page { key: "chat:r1".into(), activation_token: Some("t".into()) });
    }
}
