//! #EDE-258: the start menu's feeds. In eDE mode the holder publishes what
//! the shell's start menu navigates to the session bus's [`FeedBoard`]
//! ([`api::bus::Request::Feeds`]): one JSON object
//!
//! - `tree`: the IDE sidebar's roots in sidebar order (the Assistant, Home,
//!   Needs you, Chats, Drafts, then Devices, then each team), from the same
//!   input the sidebar draws (`shell_regions::sidebar_input` →
//!   `uicore_model::ide::ide_sidebar`, every team and folder open);
//! - `recents`: the newest root sessions of every team (the Chats model);
//! - `assistant`: the viewer's Assistant and the last messages of its chat;
//! - `account`: the signed-in account; `onLocal`: the store is a Local
//!   desktop's.
//!
//! Every key is an #EDE-237 window key (`ede_windows::window_key`'s
//! scheme), so a row's press is the bus's `page {key}`. Rebuilt ~100 ms
//! after any synced change; [`FeedBoard::set`] drops an equal object.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use api::bus::FeedBoard;
use gpui::{App, AppContext as _, Context, Entity, Global, Subscription};
use serde_json::{json, Value};
use sync::Store;
use uicore_model::ide::{IdeRow, IdeSidebar, IdeSidebarInput, IdeTone};

/// How long a change waits for the next before the feeds rebuild.
const DEBOUNCE: Duration = Duration::from_millis(100);

/// The most recent chats listed.
pub(crate) const RECENTS_MAX: usize = 10;

/// The Assistant's last messages listed.
pub(crate) const ASSISTANT_MESSAGES: usize = 4;

/// The feeds as the bus hands them out.
pub fn feed_board() -> Arc<FeedBoard> {
    static BOARD: OnceLock<Arc<FeedBoard>> = OnceLock::new();
    BOARD.get_or_init(Default::default).clone()
}

// ---- The pure mapping -----------------------------------------------------

/// What [`feed_tree`] maps besides the sidebar.
pub(crate) struct FeedTreeInput<'a> {
    pub(crate) sidebar: &'a IdeSidebar,
    /// The viewer's Assistant (its row's key).
    pub(crate) assistant_id: Option<&'a str>,
    /// Team id → its image URL.
    pub(crate) team_images: &'a HashMap<String, String>,
    /// Home's and Chats' items ([`recent_item`]s).
    pub(crate) recents: &'a [Value],
    /// Needs you's items.
    pub(crate) needs_you: &'a [Value],
}

/// A count label (`7`, `99+`) as a number; none = 0.
fn count_of(label: Option<&str>) -> usize {
    label.and_then(|label| label.trim_end_matches('+').parse().ok()).unwrap_or(0)
}

fn tone_of(tone: Option<IdeTone>, count: usize) -> Value {
    match tone {
        _ if count == 0 => Value::Null,
        Some(IdeTone::Attention) => json!("attention"),
        Some(IdeTone::Muted) => json!("muted"),
        None => Value::Null,
    }
}

/// A team row's kind as its concept icon (the sidebar paints its own glyph).
fn item_icon(kind: &str) -> &'static str {
    match kind {
        "board" => "nav-boards",
        "agent" => "settings-agents",
        "app" => "nav-apps",
        "session" => "action-chat",
        _ => "ui-folder",
    }
}

/// A team row's page key (`ede_windows::window_key`'s scheme).
fn item_key(kind: &str, id: &str) -> Option<String> {
    Some(match kind {
        "board" => format!("board:{id}"),
        "agent" => format!("agent:{id}"),
        "app" => format!("app:{id}"),
        "session" => format!("chat:{id}"),
        _ => return None,
    })
}

/// One item of a node (§2 Item).
#[allow(clippy::too_many_arguments)]
fn item(id: String, kind: &str, title: &str, subtitle: Option<&str>, icon: &str, depth: u8, count: usize, tone: Value, key: Option<String>) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "title": title,
        "subtitle": subtitle,
        "icon": icon,
        "image": Value::Null,
        "depth": depth,
        "count": count,
        "tone": tone,
        "key": key,
    })
}

/// One root of the tree (§2 Node).
#[allow(clippy::too_many_arguments)]
fn node(id: &str, kind: &str, title: &str, icon: &str, image: Option<&str>, count: usize, tone: Value, key: Option<String>, items: Vec<Value>) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "title": title,
        "icon": icon,
        "image": image,
        "count": count,
        "tone": tone,
        "key": key,
        "items": items,
    })
}

/// A team's rows under its team row: folders (`folder:<id>`, no key) and
/// items, one level up (the team row is the node).
fn team_items(rows: &[IdeRow]) -> Vec<Value> {
    rows.iter()
        .filter(|row| row.depth > 0)
        .map(|row| {
            let count = count_of(row.count.as_deref());
            let tone = tone_of(row.tone, count);
            let depth = row.depth - 1;
            match row.item.as_deref() {
                Some(kind) if row.kind == "item" => {
                    let key = item_key(kind, &row.id);
                    item(key.clone().unwrap_or_else(|| format!("{kind}:{}", row.id)), kind, &row.title, None, item_icon(kind), depth, count, tone, key)
                }
                _ => item(format!("folder:{}", row.id), "folder", &row.title, None, "ui-folder", depth, count, tone, None),
            }
        })
        .collect()
}

/// The tree (§2 `tree`): the sidebar's nav roots, Devices, then the teams.
pub(crate) fn feed_tree(input: &FeedTreeInput<'_>) -> Vec<Value> {
    let mut out = Vec::new();
    for row in &input.sidebar.nav {
        let count = count_of(row.count.as_deref());
        let tone = tone_of(row.tone, count);
        let (key, items) = match row.id.as_str() {
            "assistant" => match input.assistant_id {
                Some(id) => (Some(format!("agent:{id}")), Vec::new()),
                None => continue,
            },
            "home" => (Some("home".to_string()), input.recents.to_vec()),
            "needs-you" => (Some("needs-you".to_string()), input.needs_you.to_vec()),
            "chats" => (Some("chats".to_string()), input.recents.to_vec()),
            "drafts" => (Some("drafts".to_string()), Vec::new()),
            other => (Some(other.to_string()), Vec::new()),
        };
        out.push(node(&row.id, &row.id, &row.title, &row.icon, None, count, tone, key, items));
    }
    if let Some((head, devices)) = input.sidebar.devices.split_first() {
        let items = devices
            .iter()
            .map(|device| {
                let key = format!("device:{}", device.id);
                let tone = if device.muted { json!("muted") } else { Value::Null };
                item(key.clone(), "device", &device.title, None, &device.icon, 0, 0, tone, Some(key))
            })
            .collect();
        out.push(node("devices", "devices", &head.title, &head.icon, None, 0, Value::Null, None, items));
    }
    for team in &input.sidebar.teams {
        let Some(head) = team.rows.first() else { continue };
        let count = count_of(head.count.as_deref());
        let tone = tone_of(head.tone, count);
        let image = input.team_images.get(&team.id).map(String::as_str);
        out.push(node(&format!("team:{}", team.id), "team", &head.title, "ui-team", image, count, tone, None, team_items(&team.rows)));
    }
    out
}

/// One recent chat (§2 Item, kind `chat`).
pub(crate) fn recent_item(row: &domain::chats::ChatRow) -> Value {
    let count = if row.needs_you { row.unread_count.max(1) } else { row.unread_count };
    let tone = if row.needs_you { json!("attention") } else if count > 0 { json!("muted") } else { Value::Null };
    let key = format!("chat:{}", row.session_id);
    let subtitle = Some(row.preview.text.trim()).filter(|text| !text.is_empty());
    item(key.clone(), "chat", &row.title, subtitle, "action-chat", 0, count, tone, Some(key))
}

/// Needs you's items: the roots that need the viewer, once each, in the
/// rows' order (the chat's title, the ask under it).
pub(crate) fn needs_you_items(rows: &[domain::needs_you::NeedsYouRow]) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter(|row| seen.insert(row.root_id.clone()))
        .map(|row| {
            let key = format!("chat:{}", row.root_id);
            let title = if row.chat.trim().is_empty() { row.title.as_str() } else { row.chat.as_str() };
            let subtitle = Some(row.title.as_str()).filter(|t| !t.trim().is_empty() && *t != title);
            item(key.clone(), "chat", title, subtitle, "action-chat", 0, 0, json!("attention"), Some(key))
        })
        .collect()
}

/// The Assistant's last [`ASSISTANT_MESSAGES`] messages of its chat
/// (oldest first): the person's (`out`) and the agent's (`in`) with a body;
/// system rows, cards without text and locked rows are left out.
pub(crate) fn assistant_messages(root_id: &str, rows: &[domain::rows::MessageRow]) -> Vec<Value> {
    let kept: Vec<Value> = rows
        .iter()
        .filter(|row| row.session_id == root_id)
        .filter_map(|row| {
            let side = match row.author_kind.as_deref() {
                Some("user") => "out",
                Some("agent") => "in",
                _ => return None,
            };
            let text = row.body.as_deref().map(str::trim).filter(|text| !text.is_empty())?;
            Some(json!({"id": row.id, "side": side, "text": text, "at": row.created_at}))
        })
        .collect();
    let skip = kept.len().saturating_sub(ASSISTANT_MESSAGES);
    kept.into_iter().skip(skip).collect()
}

/// §2's account: the name (else the email), a login (the email's local
/// part: nothing synced names one) and the email.
pub(crate) fn account_value(name: Option<&str>, email: &str) -> Value {
    let name = name.map(str::trim).filter(|name| !name.is_empty()).unwrap_or(email);
    let login = email.split('@').next().unwrap_or(email);
    json!({"name": name, "login": login, "email": email})
}

// ---- The holder's publisher -----------------------------------------------

struct Publisher {
    pending: bool,
    _subscriptions: Vec<Subscription>,
}

struct Feeds(#[allow(dead_code)] Entity<Publisher>);

impl Global for Feeds {}

/// eDE mode: publish the feeds now and after every synced change.
pub(crate) fn install(cx: &mut App) {
    if !crate::ede_component::is_component() || cx.has_global::<Feeds>() {
        return;
    }
    let Some(store) = Store::try_global(cx) else { return };
    let collections = store.collections().clone();
    let state = store.state();
    let publisher = cx.new(|cx: &mut Context<Publisher>| {
        let mut subscriptions = collections.observe_all_with(cx, |this: &mut Publisher, cx| this.schedule(cx));
        subscriptions.push(cx.observe(&state, |this: &mut Publisher, _, cx| this.schedule(cx)));
        let mut this = Publisher { pending: false, _subscriptions: subscriptions };
        this.schedule(cx);
        this
    });
    cx.set_global(Feeds(publisher));
}

impl Publisher {
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        self.pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                this.pending = false;
                let feeds = gather(cx);
                feed_board().set(feeds);
            });
        })
        .detach();
    }
}

/// The feeds object (§2) from the store now.
fn gather(cx: &mut App) -> Value {
    let _span = tracing::trace_span!("ede.feeds").entered();
    let Some(store) = Store::try_global(cx) else { return Value::Null };
    let collections = store.collections().clone();
    let signed_in = crate::queries::active_account(cx);
    let account = signed_in.as_ref().map(|account| account_value(account.name.as_deref(), &account.email)).unwrap_or(Value::Null);
    let on_local = crate::local_desktops::client(cx).is_some();
    if signed_in.is_none() {
        return json!({"tree": [], "recents": [], "assistant": null, "account": account, "onLocal": on_local});
    }
    // The sidebar with every team, folder and Devices open.
    let mut input: IdeSidebarInput = crate::shell_regions::sidebar_input(cx);
    input.devices_open = true;
    input.closed.clear();
    input.editing_folder = None;
    for team in &mut input.teams {
        team.open = true;
    }
    let sidebar = uicore_model::ide::ide_sidebar(&input);
    let recents: Vec<Value> = crate::session_data::chats_model_all(cx).list.rows.iter().take(RECENTS_MAX).map(recent_item).collect();
    let needs_you = needs_you_items(&crate::needs_you::rows_in_all_teams(cx).rows);
    let team_images: HashMap<String, String> = collections
        .teams
        .read(cx)
        .iter()
        .filter_map(|team| Some((team.id.clone(), team.icon_url.clone().filter(|url| !url.is_empty())?)))
        .collect();
    let assistant_row = input.assistant.clone();
    let tree = feed_tree(&FeedTreeInput { sidebar: &sidebar, assistant_id: assistant_row.as_ref().map(|a| a.id.as_str()), team_images: &team_images, recents: &recents, needs_you: &needs_you });
    let assistant = assistant_row.map(|assistant| {
        let conversation = crate::session_data::my_conversation(&assistant.id, cx);
        let messages = conversation
            .as_ref()
            .map(|root| assistant_messages(&root.id, &crate::chat_window::chat_messages(&root.id, cx).rows))
            .unwrap_or_default();
        let busy = conversation.as_ref().is_some_and(|root| {
            crate::session_data::tree_of_root(&root.id, cx).iter().any(crate::session_data::session_is_working)
        });
        json!({"id": assistant.id, "name": assistant.name, "key": format!("agent:{}", assistant.id), "messages": messages, "busy": busy})
    });
    json!({"tree": tree, "recents": recents, "assistant": assistant, "account": account, "onLocal": on_local})
}

/// The bus's `search {query}`: the synced issues of every member team
/// ranked by `domain::issue_search` in one pass, at most
/// [`uicore_model::START_MENU_ISSUE_LIMIT`], each a start menu issue's JSON
/// plus its page key. An empty query = none.
pub(crate) fn search(query: &str, cx: &App) -> Vec<Value> {
    let Some(store) = Store::try_global(cx) else { return Vec::new() };
    if query.trim().is_empty() {
        return Vec::new();
    }
    let collections = store.collections();
    let teams = collections.teams_sorted(cx);
    let boards = collections.boards.read(cx);
    // (issue, its team) across every team, with each team's statuses.
    let mut issues: Vec<(&domain::rows::Issue, &domain::rows::Team)> = Vec::new();
    let mut statuses: HashMap<String, Vec<domain::rows::IssueStatusRow>> = HashMap::new();
    for team in &teams {
        statuses.insert(team.id.clone(), crate::queries::team_statuses(cx, &team.id));
        issues.extend(collections.issue_refs_in_team(&team.id, cx).into_iter().map(|issue| (issue, team)));
    }
    let rows: Vec<domain::issue_search::SearchRow<'_>> = issues
        .iter()
        .map(|(issue, team)| {
            let status_name = issue
                .status_id
                .as_deref()
                .and_then(|id| statuses.get(&team.id)?.iter().find(|status| status.id == id))
                .map(|status| status.name.as_str());
            crate::picker::search_list::issue_search_row(issue, cx).with_status_name(status_name)
        })
        .collect();
    domain::issue_search::rank(&rows, query, uicore_model::START_MENU_ISSUE_LIMIT, &std::collections::HashSet::new())
        .into_iter()
        .take(uicore_model::START_MENU_ISSUE_LIMIT)
        .map(|ix| {
            let (issue, team) = issues[ix];
            let board = boards.get(&issue.board_id).map(|board| board.name.clone()).unwrap_or_default();
            json!({
                "id": issue.id,
                "identifier": issue.identifier,
                "title": issue.title,
                "status": issue.status.as_wire().unwrap_or("backlog"),
                "source": "team",
                "sourceId": team.id,
                "sourceName": team.name,
                "board": board,
                "key": format!("issue:{}", issue.id),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uicore_model::ide::{IdeAssistantInput, IdeDeviceInput, IdeItemInput, IdeLayoutEntry, IdeTeamInput};

    fn sidebar() -> (IdeSidebarInput, IdeSidebar) {
        let input = IdeSidebarInput {
            assistant: Some(IdeAssistantInput { id: "as1".into(), name: "Assistant".into(), unread: 2, needs_you: false }),
            needs_you: 3,
            drafts: 0,
            devices: vec![
                IdeDeviceInput { id: "d1".into(), name: "mint".into(), icon: "device-desktop".into(), online: true, team_mark: None },
                IdeDeviceInput { id: "d2".into(), name: "box".into(), icon: "device-server".into(), online: false, team_mark: None },
            ],
            devices_open: true,
            teams: vec![IdeTeamInput {
                id: "t1".into(),
                name: "Exponential".into(),
                mark: "ide:team:t1".into(),
                open: true,
                owner: true,
                needs_you: 0,
                items: vec![
                    IdeItemInput { kind: "board".into(), id: "b1".into(), name: "Core".into(), icon: "ide:board:b1".into(), ..Default::default() },
                    IdeItemInput { kind: "agent".into(), id: "g1".into(), name: "Reviewer".into(), icon: "ide:agent:g1".into(), unread: 1, ..Default::default() },
                    IdeItemInput { kind: "session".into(), id: "s1".into(), name: "Fix it".into(), icon: "ide:run:s1".into(), needs_you: true, ..Default::default() },
                ],
                layout: vec![
                    IdeLayoutEntry { id: "f1".into(), kind: "folder".into(), target_id: None, parent_id: None, name: Some("Work".into()), sort_order: 0.0 },
                    IdeLayoutEntry { id: "e1".into(), kind: "item".into(), target_id: Some("s1".into()), parent_id: Some("f1".into()), name: None, sort_order: 1.0 },
                ],
            }],
            ..Default::default()
        };
        let sidebar = uicore_model::ide::ide_sidebar(&input);
        (input, sidebar)
    }

    #[test]
    fn the_tree_is_the_sidebars_roots_in_order_with_page_keys() {
        let (_, sidebar) = sidebar();
        let images = HashMap::from([("t1".to_string(), "https://x/t1.png".to_string())]);
        let recents = vec![json!({"id": "chat:r1"})];
        let needs = vec![json!({"id": "chat:s1"})];
        let tree = feed_tree(&FeedTreeInput { sidebar: &sidebar, assistant_id: Some("as1"), team_images: &images, recents: &recents, needs_you: &needs });
        let ids: Vec<&str> = tree.iter().map(|n| n["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["assistant", "home", "needs-you", "chats", "devices", "team:t1"], "drafts only while it counts");
        assert_eq!(tree[0]["key"], "agent:as1");
        assert_eq!(tree[0]["count"], 2);
        assert_eq!(tree[0]["tone"], "muted");
        assert_eq!(tree[0]["items"], json!([]));
        assert_eq!(tree[1]["key"], "home");
        assert_eq!(tree[1]["items"], json!(recents));
        assert_eq!(tree[2]["items"], json!(needs));
        assert_eq!(tree[2]["tone"], "attention");
        assert_eq!(tree[3]["items"], json!(recents));
        // Devices: no key, its rows keyed by device.
        assert_eq!(tree[4]["key"], Value::Null);
        assert_eq!(tree[4]["items"][0]["key"], "device:d1");
        assert_eq!(tree[4]["items"][0]["icon"], "device-desktop");
        assert_eq!(tree[4]["items"][1]["tone"], "muted", "offline");
        // A team: no key, its image, folders open, depth one up.
        let team = &tree[5];
        assert_eq!((team["kind"].as_str(), team["icon"].as_str(), team["image"].as_str()), (Some("team"), Some("ui-team"), Some("https://x/t1.png")));
        assert_eq!(team["key"], Value::Null);
        let rows: Vec<(String, String, u64, Value)> = team["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| (i["id"].as_str().unwrap().to_string(), i["kind"].as_str().unwrap().to_string(), i["depth"].as_u64().unwrap(), i["key"].clone()))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("folder:f1".to_string(), "folder".to_string(), 0, Value::Null),
                ("chat:s1".into(), "session".into(), 1, json!("chat:s1")),
                ("board:b1".into(), "board".into(), 0, json!("board:b1")),
                ("agent:g1".into(), "agent".into(), 0, json!("agent:g1")),
            ]
        );
        let session = &team["items"][1];
        assert_eq!((session["count"].as_u64(), session["tone"].as_str(), session["icon"].as_str()), (Some(1), Some("attention"), Some("action-chat")));
        let board = &team["items"][2];
        assert_eq!((board["count"].as_u64(), board["tone"].clone(), board["icon"].as_str()), (Some(0), Value::Null, Some("nav-boards")));
        // Every key reopens a page.
        for node in &tree {
            for key in std::iter::once(&node["key"]).chain(node["items"].as_array().unwrap().iter().map(|i| &i["key"])).filter_map(Value::as_str) {
                assert!(crate::ede_windows::screen_of_key(key).is_some(), "{key}");
            }
        }
    }

    #[test]
    fn no_assistant_no_assistant_root() {
        let (mut input, _) = sidebar();
        input.assistant = None;
        input.drafts = 2;
        let sidebar = uicore_model::ide::ide_sidebar(&input);
        let tree = feed_tree(&FeedTreeInput { sidebar: &sidebar, assistant_id: None, team_images: &HashMap::new(), recents: &[], needs_you: &[] });
        let ids: Vec<&str> = tree.iter().map(|n| n["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["home", "needs-you", "chats", "drafts", "devices", "team:t1"]);
        assert_eq!(tree[3]["count"], 2);
        assert_eq!(tree[3]["items"], json!([]));
        assert_eq!(tree[5]["image"], Value::Null);
    }

    #[test]
    fn counts_read_their_labels() {
        assert_eq!(count_of(None), 0);
        assert_eq!(count_of(Some("7")), 7);
        assert_eq!(count_of(Some("99+")), 99);
    }

    fn message(id: &str, session: &str, author: &str, body: &str) -> domain::rows::MessageRow {
        domain::rows::MessageRow {
            id: id.into(),
            session_id: session.into(),
            author_kind: Some(author.into()),
            body: Some(body.into()),
            created_at: Some(format!("2026-11-27T10:00:0{id}Z")),
            ..Default::default()
        }
    }

    #[test]
    fn the_assistants_last_four_messages_oldest_first() {
        let rows = vec![
            message("1", "r", "user", "one"),
            message("2", "r", "agent", "two"),
            message("3", "r", "system", "joined"),
            message("4", "r", "agent", "  "),
            message("5", "child", "agent", "elsewhere"),
            message("6", "r", "user", "three"),
            message("7", "r", "agent", "four"),
            message("8", "r", "user", "five"),
        ];
        let out = assistant_messages("r", &rows);
        let texts: Vec<(&str, &str)> = out.iter().map(|m| (m["side"].as_str().unwrap(), m["text"].as_str().unwrap())).collect();
        assert_eq!(texts, [("in", "two"), ("out", "three"), ("in", "four"), ("out", "five")]);
        assert_eq!(out[0]["id"], "2");
        assert_eq!(out[0]["at"], "2026-11-27T10:00:02Z");
    }

    #[test]
    fn the_account_names_itself() {
        assert_eq!(account_value(Some("Danny"), "danny@x.at"), json!({"name": "Danny", "login": "danny", "email": "danny@x.at"}));
        assert_eq!(account_value(Some(" "), "a@b"), json!({"name": "a@b", "login": "a", "email": "a@b"}));
    }
}
