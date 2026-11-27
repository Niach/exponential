//! #EDE-258: what the start menu navigates, from the holder. The login's
//! holder (`exponential-app`) is the one store: it long-polls its feeds to
//! the shell over the session bus ([`api::bus::Request::Feeds`]): the IDE
//! sidebar tree's roots with their nested items (`tree`), the recent chats
//! (`recents`), the global Assistant's last answer (`assistant`), the
//! person (`account`). The shell keeps the last ones and builds the menu
//! from them on every change, so opening it does no work.
//!
//! Opening a page (`page`), the issue search (`search`) and the
//! Assistant's composer (`ask`) are one-shot calls on the same bus.

use std::{sync::mpsc, time::Duration};

use api::bus::{Client, Reply, Request};
use serde_json::{json, Value};

/// The holder's feeds as last told (`Value::Null` while no holder answers).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HolderFeeds {
    pub feeds: Value,
}

impl HolderFeeds {
    /// The tree's roots (none without a holder).
    pub fn tree(&self) -> &[Value] {
        self.feeds["tree"].as_array().map_or(&[], Vec::as_slice)
    }

    /// The holder is on a Local desktop: its tree is the desktop's.
    pub fn on_local(&self) -> bool {
        self.feeds["onLocal"].as_bool() == Some(true)
    }
}

/// What the shell keeps of the feeds: on a Local desktop the holder's store
/// is that desktop's, so the menu keeps the account's last tree (Local is
/// its own node, from the machine's desktops).
pub fn keep(kept: &mut HolderFeeds, now: HolderFeeds) {
    if now.on_local() && !kept.tree().is_empty() {
        let tree = kept.feeds["tree"].clone();
        *kept = now;
        kept.feeds["tree"] = tree;
    } else {
        *kept = now;
    }
}

const RETRY: Duration = Duration::from_secs(2);
const RETRY_UNSUPPORTED: Duration = Duration::from_secs(60);

/// Follows the holder's feeds on a thread of its own; every change is sent
/// on `tx` (an empty one while no holder answers).
pub fn follow(tx: mpsc::Sender<HolderFeeds>) {
    let spawned = std::thread::Builder::new().name("ede-shell-holder-feeds".into()).spawn(move || {
        let path = api::bus::socket_path(&api::default_data_dir());
        let mut last = HolderFeeds::default();
        loop {
            let pause = match Client::connect(&path, api::bus::FEEDS_TIMEOUT) {
                Ok(mut client) => {
                    let mut after = None;
                    loop {
                        match client.call(&Request::Feeds { after }) {
                            Ok(Reply::Feeds { generation, feeds }) => {
                                after = Some(generation);
                                let now = HolderFeeds { feeds };
                                if now != last {
                                    last = now.clone();
                                    if tx.send(now).is_err() {
                                        return;
                                    }
                                }
                            }
                            Ok(_) => break RETRY_UNSUPPORTED,
                            Err(_) => break RETRY,
                        }
                    }
                }
                Err(_) => RETRY,
            };
            if last != HolderFeeds::default() {
                last = HolderFeeds::default();
                if tx.send(last.clone()).is_err() {
                    return;
                }
            }
            std::thread::sleep(pause);
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("holder feeds: {e}");
    }
}

fn path() -> std::path::PathBuf {
    api::bus::socket_path(&api::default_data_dir())
}

/// Opens the page of a window key (`board:<id>`, `home`, …) in the holder,
/// its window coming forward with `token`. Off the main loop.
pub fn open_page(key: String, token: Option<String>) {
    std::thread::spawn(move || {
        if let Err(e) = api::bus::call(&path(), &Request::Page { key: key.clone(), activation_token: token }) {
            tracing::warn!(%key, "holder page: {e}");
        }
    });
}

/// The composer's text to the global Assistant.
pub fn ask(text: String) {
    std::thread::spawn(move || {
        if let Err(e) = api::bus::call(&path(), &Request::Ask { text, activation_token: None }) {
            tracing::warn!("holder ask: {e}");
        }
    });
}

/// The holder's issue hits for `query` (blocking: call it off the main
/// loop), as the start menu's issues (`uicore_model::StartMenuIssue`).
pub fn search(query: &str) -> Vec<uicore_model::StartMenuIssue> {
    match api::bus::call(&path(), &Request::Search { query: query.to_string() }) {
        Ok(Reply::Search { issues }) => issues.into_iter().filter_map(|issue| serde_json::from_value(issue).ok()).collect(),
        _ => Vec::new(),
    }
}

/// `/shell/startMenu/assistant`: the Assistant pane (null without one).
pub fn assistant_json(feeds: &HolderFeeds) -> Value {
    let assistant = &feeds.feeds["assistant"];
    if !assistant.is_object() {
        return Value::Null;
    }
    let messages: Vec<Value> = assistant["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            let out = m["side"] == "out";
            json!({"id": m["id"], "text": m["text"], "out": out, "answer": !out})
        })
        .collect();
    json!({"name": assistant["name"], "key": assistant["key"], "busy": assistant["busy"].as_bool() == Some(true), "messages": messages, "empty": messages.is_empty()})
}

/// The feeds under `EDE_SHELL_DEMO` (the goldens): a fixed sample, the
/// planning draft's, so the pictures never depend on an account.
pub fn demo() -> HolderFeeds {
    let item = |id: &str, kind: &str, title: &str, icon: &str, depth: u8, key: Option<&str>| json!({"id": id, "kind": kind, "title": title, "subtitle": null, "icon": icon, "image": null, "depth": depth, "count": 0, "tone": null, "key": key});
    let recents = vec![
        item("chat:s1", "chat", "eDE plan: the app folded in", "action-chat", 0, Some("chat:s1")),
        item("chat:s2", "chat", "Fix CI on #3590", "action-chat", 0, Some("chat:s2")),
    ];
    let node = |id: &str, kind: &str, title: &str, icon: &str, count: u32, tone: Option<&str>, key: Option<&str>, items: Vec<Value>| json!({"id": id, "kind": kind, "title": title, "icon": icon, "image": null, "count": count, "tone": tone, "key": key, "items": items});
    HolderFeeds {
        feeds: json!({
            "tree": [
                node("assistant", "assistant", "Assistant", "brand-exponential", 0, None, Some("agent:a0"), vec![]),
                node("home", "home", "Home", "nav-home", 0, None, Some("home"), recents.clone()),
                node("needs-you", "needs-you", "Needs you", "nav-needs-you", 2, Some("attention"), Some("needs-you"), vec![recents[1].clone()]),
                node("chats", "chats", "Chats", "nav-chats", 0, None, Some("chats"), recents.clone()),
                node("devices", "devices", "Devices", "nav-devices", 0, None, None, vec![item("device:d1", "device", "mint", "computer", 0, Some("device:d1")), item("device:d2", "device", "MacBook", "laptop", 0, Some("device:d2"))]),
                node("team:t1", "team", "Feedback", "ui-team", 4, Some("muted"), None, vec![
                    item("board:b1", "board", "Desktop", "nav-boards", 0, Some("board:b1")),
                    item("board:b2", "board", "Exponential", "nav-boards", 0, Some("board:b2")),
                    item("folder:f1", "folder", "Releases", "ui-folder", 0, None),
                    item("board:b3", "board", "Android", "nav-boards", 1, Some("board:b3")),
                    item("board:b4", "board", "iOS", "nav-boards", 1, Some("board:b4")),
                    item("agent:a1", "agent", "Release shepherd", "settings-agents", 0, Some("agent:a1")),
                    item("app:p1", "app", "Standup", "nav-apps", 0, Some("app:p1")),
                    item("chat:s1", "session", "eDE plan: the app folded in", "action-chat", 0, Some("chat:s1")),
                ]),
                node("team:t2", "team", "Yourev", "ui-team", 0, None, None, vec![item("board:b5", "board", "Website", "nav-boards", 0, Some("board:b5"))]),
            ],
            "recents": recents,
            "assistant": {"id": "a0", "name": "Assistant", "key": "agent:a0", "busy": false, "messages": [
                {"id": "m1", "side": "out", "text": "Anything blocked on the Desktop board?", "at": "2026-11-08T11:48:00Z"},
                {"id": "m2", "side": "in", "text": "One thing: **#3590** fails its gate on a stale migration test. I started **Fix CI on #3590** on the homelab.", "at": "2026-11-08T11:49:00Z"},
            ]},
            "account": {"name": "Ada", "login": "ada", "email": "ada@example.com"},
            "onLocal": false,
        }),
    }
}

/// The Local node under `EDE_SHELL_DEMO`.
pub fn demo_local() -> Vec<uicore_model::StartMenuLocalRow> {
    let row = |kind: &str, id: &str, desktop: Option<&str>, depth: u8, title: &str| uicore_model::StartMenuLocalRow { kind: kind.into(), id: id.into(), desktop_id: desktop.map(String::from), depth, title: title.into() };
    vec![row("local", "local", None, 0, "Local"), row("desktop", "l1", Some("l1"), 1, "Scratch"), row("board", "lb1", Some("l1"), 2, "Issues")]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feeds(v: Value) -> HolderFeeds {
        HolderFeeds { feeds: v }
    }

    #[test]
    fn on_a_local_desktop_the_account_tree_stays() {
        let mut kept = HolderFeeds::default();
        keep(&mut kept, feeds(json!({"tree": [{"id": "team:t1"}], "onLocal": false})));
        keep(&mut kept, feeds(json!({"tree": [{"id": "team:local"}], "onLocal": true, "recents": []})));
        assert_eq!(kept.tree()[0]["id"], "team:t1");
        assert!(kept.on_local());
        keep(&mut kept, feeds(json!({"tree": [{"id": "team:t2"}], "onLocal": false})));
        assert_eq!(kept.tree()[0]["id"], "team:t2");
    }

    #[test]
    fn the_assistant_pane_marks_the_persons_messages() {
        let pane = assistant_json(&feeds(json!({"assistant": {"name": "Assistant", "key": "agent:a1", "messages": [{"id": "m1", "side": "out", "text": "Hi"}, {"id": "m2", "side": "in", "text": "**Hello**"}], "busy": false}})));
        assert_eq!(pane["messages"][0]["out"], true);
        assert_eq!(pane["messages"][1]["answer"], true);
        assert_eq!(pane["empty"], false);
        assert_eq!(assistant_json(&HolderFeeds::default()), Value::Null);
    }
}
