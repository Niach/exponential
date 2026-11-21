//! #EDE-65 the tray's Exponential half: every signed-in account's inbox
//! merges into the bell next to the desktop's notifications, and Needs you
//! (the plans, questions and PRs waiting on the person) shows as a tray
//! button while anything waits. Both read the shell's OWN per-account
//! stores ([`crate::accounts::Workspaces`]), across all of the account's
//! teams, with the IDE's rules: the inbox's grouping and renderability
//! (`ui::queries::build_inbox_entries`: issue rows grouped by issue while the
//! issue and its board are synced, one entry per issue-less agent message,
//! blocked session or signed-out login), where a row opens
//! (`domain::inbox_route`, inbox-route.json) and Needs you's fold
//! (`domain::needs_you`, needs-you.json) per team. The bell lists what is
//! UNREAD (the inbox has no archive: dismissing a row marks it read, as the
//! IDE's row press and mark-all-read do; it stays in the IDE's inbox, dimmed).
//! Opening a row hands off to the Exponential app on that item
//! (`exponential://issue/<IDENT>`, `exponential://session/<id>`).

use std::collections::{HashMap, HashSet};

use domain::inbox_route::{inbox_row_target, InboxRowTarget};
use domain::needs_you::{needs_you, NeedsYouInput, NeedsYouMessageInput, NeedsYouOpens, NeedsYouPullRequestInput, NeedsYouRow, NeedsYouSessionInput};
use domain::rows::{AgentDefRow, Board, Issue, MessageRow, Notification, PullRequestRow, SessionIssueRow, SessionRow, Team};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

/// The shapes the tray reads besides the switcher's (`accounts::SHAPES`).
pub const SHAPES: &[&str] = &["notifications", "issues", "boards", "sessions", "session_issues", "agents", "pull_requests"];

/// Whether a delta on `shape` reads the tray again (at most once a second):
/// its inbox, Needs you and the agent spaces' titles (#EDE-194: a rename,
/// a sealed title or an issue linked lands on `sessions`, `session_issues`
/// or `issues`).
pub fn refreshes_tray(shape: &str) -> bool {
    shape == "teams" || SHAPES.contains(&shape)
}

/// A feed id of an inbox entry: never a number, so it never meets a desktop
/// notification's id in `ede_notify_*`.
pub const KEY_PREFIX: &str = "inbox:";

/// One unread inbox entry of one account, as the bell lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct InboxItem {
    /// `inbox:<the newest notification's id>`: a new row on the same issue is a new item (it toasts).
    pub key: String,
    pub account_id: String,
    /// The entry's unread notifications (marked read together, like the IDE's group).
    pub unread_ids: Vec<String>,
    /// The team's name (the row's `app`), else `Exponential`.
    pub team: String,
    pub summary: String,
    pub body: String,
    pub created_ms: i64,
    /// Where opening it hands off to (None = it only marks read).
    pub link: Option<String>,
}

/// One account's synced rows, as far as the tray reads them.
#[derive(Debug, Clone, Default)]
pub struct AccountRows {
    pub account_id: String,
    /// The account's user (Needs you's `me`).
    pub user_id: String,
    pub teams: Vec<Team>,
    pub notifications: Vec<Notification>,
    pub issues: Vec<Issue>,
    pub boards: Vec<Board>,
    pub sessions: Vec<SessionRow>,
    pub session_issues: Vec<SessionIssueRow>,
    pub agents: Vec<AgentDefRow>,
    pub pull_requests: Vec<PullRequestRow>,
}

/// A shape's rows off the account's store, typed (rows that do not parse
/// drop out). #EDE-141: the chat reads through [`crate::chat_rows`] now.
#[cfg(test)]
pub fn read_rows<T: DeserializeOwned>(store: &sync::store::ShapeStore, shape: &str) -> Vec<T> {
    rows(store, shape)
}

fn rows<T: DeserializeOwned>(store: &sync::store::ShapeStore, shape: &str) -> Vec<T> {
    let Some(spec) = sync::shapes::shape_by_name(shape) else { return Vec::new() };
    store.read_all(spec).unwrap_or_default().into_iter().filter_map(|row| serde_json::from_value(Value::Object(row)).ok()).collect()
}

impl AccountRows {
    /// The account's rows off its synced store.
    pub fn read(account_id: &str, user_id: &str, store: &sync::store::ShapeStore) -> Self {
        Self {
            account_id: account_id.to_string(),
            user_id: user_id.to_string(),
            teams: rows(store, "teams"),
            notifications: rows(store, "notifications"),
            issues: rows(store, "issues"),
            boards: rows(store, "boards"),
            sessions: rows(store, "sessions"),
            session_issues: rows(store, "session_issues"),
            agents: rows(store, "agents"),
            pull_requests: rows(store, "pull_requests"),
        }
    }
}

/// An ISO / Postgres timestamp in epoch ms (0 when unreadable).
pub fn ms(iso: Option<&str>) -> i64 {
    iso.and_then(domain::session_tree::iso_ms).unwrap_or(0)
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}

/// `exponential://issue/<IDENT>` (the IDE's `parse_issue_deep_link`).
pub fn issue_link(identifier: &str) -> String {
    format!("{}://issue/{identifier}", api::login::OAUTH_CALLBACK_SCHEME)
}

/// `exponential://session/<id>`, with the card to land on for a Needs you
/// row (the IDE's `parse_session_deep_link`).
pub fn session_link(session_id: &str, card: Option<(&str, NeedsYouOpens)>) -> String {
    let base = format!("{}://session/{session_id}", api::login::OAUTH_CALLBACK_SCHEME);
    match card {
        None => base,
        Some((message_id, opens)) => {
            let opens = match opens {
                NeedsYouOpens::Plan => "plan",
                NeedsYouOpens::Card => "card",
                NeedsYouOpens::Results => "results",
                NeedsYouOpens::Chat => "chat",
            };
            format!("{base}?opens={opens}&message={message_id}")
        }
    }
}

/// The issue-less kinds the inbox renders (the IDE's `issueless_kind_renderable`).
fn issueless_renderable(kind: Option<&str>) -> bool {
    matches!(
        kind,
        Some(domain::contract::NOTIFICATION_TYPE_AGENT_MESSAGE | domain::contract::NOTIFICATION_TYPE_SESSION_BLOCKED | domain::contract::NOTIFICATION_TYPE_AGENT_SIGNED_OUT)
    )
}

/// The account's unread inbox entries, newest first: the IDE's grouping
/// (issue rows by issue, only while the issue and its board are synced;
/// one entry per renderable issue-less row), minus what is read and what
/// `read_locally` holds (dismissed here, the server's echo still on its way).
pub fn inbox_items(rows: &AccountRows, read_locally: &HashSet<String>) -> Vec<InboxItem> {
    let teams: HashMap<&str, &str> = rows.teams.iter().map(|t| (t.id.as_str(), t.name.as_str())).collect();
    let boards: HashMap<&str, &Board> = rows.boards.iter().map(|b| (b.id.as_str(), b)).collect();
    let issues: HashMap<&str, &Issue> = rows.issues.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut unread: Vec<&Notification> = rows.notifications.iter().filter(|n| n.read_at.is_none() && !read_locally.contains(&n.id)).collect();
    unread.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let team_name = |id: Option<&str>| id.and_then(|id| teams.get(id)).map(|n| n.to_string()).unwrap_or_else(|| "Exponential".to_string());
    let mut out: Vec<InboxItem> = Vec::new();
    let mut by_issue: HashMap<&str, usize> = HashMap::new();
    for n in unread {
        let kind = n.kind.as_deref();
        match non_empty(n.issue_id.as_deref()) {
            Some(issue_id) => {
                let Some(issue) = issues.get(issue_id) else { continue };
                let Some(board) = boards.get(issue.board_id.as_str()) else { continue };
                if let Some(&at) = by_issue.get(issue_id) {
                    out[at].unread_ids.push(n.id.clone());
                    continue;
                }
                by_issue.insert(issue_id, out.len());
                out.push(InboxItem {
                    key: format!("{KEY_PREFIX}{}", n.id),
                    account_id: rows.account_id.clone(),
                    unread_ids: vec![n.id.clone()],
                    team: team_name(Some(&board.team_id)),
                    summary: format!("{} {}", issue.identifier, issue.title),
                    body: n.title.clone().unwrap_or_default(),
                    created_ms: ms(n.created_at.as_deref()),
                    link: Some(issue_link(&issue.identifier)),
                });
            }
            None => {
                if !issueless_renderable(kind) {
                    continue;
                }
                let link = match inbox_row_target(kind, None, n.session_id.as_deref(), n.device_id.as_deref()) {
                    InboxRowTarget::Session { session_id } => Some(session_link(&session_id, None)),
                    _ => None,
                };
                out.push(InboxItem {
                    key: format!("{KEY_PREFIX}{}", n.id),
                    account_id: rows.account_id.clone(),
                    unread_ids: vec![n.id.clone()],
                    team: team_name(n.team_id.as_deref()),
                    summary: n.title.clone().unwrap_or_default(),
                    body: n.body.clone().unwrap_or_default(),
                    created_ms: ms(n.created_at.as_deref()),
                    link,
                });
            }
        }
    }
    out
}

/// The toast's buttons on an inbox item.
fn inbox_actions() -> Value {
    json!([{"key": "open", "label": "Open"}, {"key": "read", "label": "Mark read"}])
}

impl InboxItem {
    /// The row as `/notifications` carries it (a desktop notification's fields).
    pub fn to_json(&self, now_ms: i64) -> Value {
        json!({
            "id": self.key,
            "app": self.team,
            "summary": self.summary,
            "body": self.body,
            "icon": "",
            "actions": inbox_actions(),
            "urgency": 1,
            "at": domain::results_tab::result_age(self.created_ms, now_ms),
        })
    }
}

/// `/notifications`: the desktop's notifications (each `(received ms, row)`,
/// newest first) and the inbox entries merged newest first; the toasts are
/// the desktop's still showing plus the inbox entries in `inbox_toasts`,
/// newest three; `count` is the bell's badge (everything listed). Every
/// action carries its row's `id` ([`with_action_ids`]).
pub fn merged_feed(desktop: &[(i64, Value)], desktop_toasts: &HashSet<u64>, inbox: &[InboxItem], inbox_toasts: &HashSet<String>, now_ms: i64) -> Value {
    let mut all: Vec<(i64, Value, bool)> = desktop
        .iter()
        .map(|(at, row)| (*at, with_action_ids(row.clone()), row["id"].as_u64().is_some_and(|id| desktop_toasts.contains(&id))))
        .chain(inbox.iter().map(|i| (i.created_ms, with_action_ids(i.to_json(now_ms)), inbox_toasts.contains(&i.key))))
        .collect();
    // Stable: equal times keep the desktop's own order.
    all.sort_by(|a, b| b.0.cmp(&a.0));
    let toasts: Vec<Value> = all.iter().filter(|(_, _, toast)| *toast).take(3).map(|(_, row, _)| row.clone()).collect();
    let list: Vec<Value> = all.into_iter().map(|(_, row, _)| row).collect();
    json!({"toasts": toasts, "count": list.len(), "list": list})
}

/// #EDE-196: the row's `id` on each of its actions. A toast's action button
/// is a template item inside the toast's template: its bindings read the
/// action (`key`, `label`) or the document's root, never the row around it,
/// so the press finds its notification through this copy.
fn with_action_ids(mut row: Value) -> Value {
    let id = row["id"].clone();
    if let Some(actions) = row.get_mut("actions").and_then(Value::as_array_mut) {
        for action in actions.iter_mut().filter_map(Value::as_object_mut) {
            action.insert("id".into(), id.clone());
        }
    }
    row
}

/// The inbox entries that toast now: unseen, and created since the shell
/// started (less a minute's grace), so the first sync of an old inbox
/// stays quiet. Marks every key seen.
pub fn fresh_items(inbox: &[InboxItem], seen: &mut HashSet<String>, since_ms: i64) -> Vec<String> {
    let mut fresh = Vec::new();
    for item in inbox {
        if seen.insert(item.key.clone()) && item.created_ms >= since_ms - 60_000 {
            fresh.push(item.key.clone());
        }
    }
    fresh
}

/// One Needs you row with the team and account it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct NeedsYouItem {
    pub account_id: String,
    pub team: String,
    pub row: NeedsYouRow,
}

impl NeedsYouItem {
    pub fn key(&self) -> String {
        format!("{}:{}", self.account_id, self.row.message_id)
    }

    /// Where the row opens in the Exponential app (needs-you.json `opens`).
    pub fn link(&self) -> String {
        let session = if self.row.opens == NeedsYouOpens::Plan { &self.row.session_id } else { &self.row.root_id };
        session_link(session, Some((&self.row.message_id, self.row.opens)))
    }

    fn to_json(&self) -> Value {
        let chat = if self.row.chat.is_empty() { self.team.clone() } else { format!("{} · {}", self.row.chat, self.team) };
        let icon = match self.row.kind {
            domain::needs_you::NeedsYouKind::Plan => domain::waiting_cards::WAITING_CARDS.icons.plan.clone(),
            domain::needs_you::NeedsYouKind::Question => domain::waiting_cards::WAITING_CARDS.icons.question.clone(),
            domain::needs_you::NeedsYouKind::Pr => "pr-open".to_string(),
        };
        json!({"id": self.key(), "title": self.row.title, "subtitle": chat, "meta": self.row.age, "icon": icon})
    }
}

/// A digest message the session title may start from (the IDE's
/// `is_first_prompt_candidate`).
fn first_prompt_candidate(message: &MessageRow, owner: &str) -> bool {
    matches!(message.card, None | Some(Value::Null))
        && (message.is_from_user() || message.author_session_id.as_deref().is_some_and(|a| !a.is_empty() && a != owner))
        && domain::session_title::first_line(message.body_text()).is_some()
}

/// #EXP-2024: the session rows of the agent spaces' sessions (`wanted`) this
/// account holds, and their roots (a child's chat), titled as every client
/// titles a session; sealed titles and digests opened with `opener`
/// (#EDE-194: the keyrings of the account's installs on this machine,
/// [`crate::sealed::space_opener`]), so the list names a sealed chat by its
/// own title. A title no rung yields (a sealed one no keyring here opens,
/// with no issue and a first prompt that is sealed too) is `None`: the
/// row reads `Agent space n`, never `Untitled`.
pub fn space_sessions(rows: &AccountRows, wanted: &HashSet<String>, opener: &sync::sealed::Opener) -> Vec<uicore_model::spaces::SpaceSession> {
    let by_id: HashMap<&str, &SessionRow> = rows.sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut ids: Vec<&str> = wanted.iter().filter_map(|w| by_id.get(w.as_str()).map(|s| s.id.as_str())).collect();
    let roots: Vec<&str> = ids.iter().filter_map(|id| by_id.get(id)).filter(|s| s.parent_id.is_some()).map(|s| s.root_id_or_self()).collect();
    ids.extend(roots);
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Vec::new();
    }
    let issues: HashMap<&str, &Issue> = rows.issues.iter().map(|i| (i.id.as_str(), i)).collect();
    ids.into_iter()
        .filter_map(|id| by_id.get(id))
        .map(|row| {
            let mut row = (*row).clone();
            opener.open_session(&mut row);
            let digest = domain::chats::ConversationDigest::decode(row.digest.as_ref());
            let messages = digest.map(|d| domain::chats::digest_messages(&row.id, Some(&d))).unwrap_or_default();
            let first: Vec<&MessageRow> = messages.iter().filter(|m| first_prompt_candidate(m, &row.id)).collect();
            let links: Vec<&SessionIssueRow> = rows.session_issues.iter().filter(|l| l.session_id == row.id).collect();
            let tree: Vec<domain::session_tree::TreeIssue> = links.iter().filter_map(|l| issues.get(l.issue_id.as_str())).map(|i| domain::session_tree::TreeIssue::from_issue(i)).collect();
            uicore_model::spaces::SpaceSession { id: row.id.clone(), title: domain::session_title::session_title_if_any(&row, &links, &tree, &first), parent_id: row.parent_id.clone(), root_id: Some(row.root_id_or_self().to_string()) }
        })
        .collect()
}

/// Every team's Needs you of one account (the IDE's `needs_you::input` per
/// team, the stuck-PR rows aside: they need the devices' presence), the
/// team's rows in the fold's order, the teams by name.
pub fn needs_you_items(rows: &AccountRows, now_ms: i64) -> Vec<NeedsYouItem> {
    let agents: HashMap<&str, &AgentDefRow> = rows.agents.iter().map(|a| (a.id.as_str(), a)).collect();
    let issues: HashMap<&str, &Issue> = rows.issues.iter().map(|i| (i.id.as_str(), i)).collect();
    // #EXP-1730: each session's digest rides on its own row.
    let digests: HashMap<&str, Vec<MessageRow>> = rows
        .sessions
        .iter()
        .filter_map(|s| {
            let digest = domain::chats::ConversationDigest::decode(s.digest.as_ref())?;
            Some((s.id.as_str(), domain::chats::digest_messages(&s.id, Some(&digest))))
        })
        .collect();
    let mut links_of: HashMap<&str, Vec<&SessionIssueRow>> = HashMap::new();
    for link in &rows.session_issues {
        links_of.entry(link.session_id.as_str()).or_default().push(link);
    }
    let title_of = |session: &SessionRow| -> String {
        let links: Vec<&SessionIssueRow> = links_of.get(session.id.as_str()).cloned().unwrap_or_default();
        let tree: Vec<domain::session_tree::TreeIssue> = links.iter().filter_map(|l| issues.get(l.issue_id.as_str())).map(|i| domain::session_tree::TreeIssue::from_issue(i)).collect();
        let first: Vec<&MessageRow> = digests.get(session.id.as_str()).into_iter().flatten().filter(|m| first_prompt_candidate(m, &session.id)).collect();
        domain::session_title::session_title(session, &links, &tree, &first)
    };
    let mut teams: Vec<&Team> = rows.teams.iter().collect();
    teams.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let mut out = Vec::new();
    for team in teams {
        let sessions: Vec<NeedsYouSessionInput> = rows
            .sessions
            .iter()
            .filter(|s| s.team_id.as_deref() == Some(team.id.as_str()))
            .map(|s| NeedsYouSessionInput {
                id: s.id.clone(),
                root_id: s.root_id_or_self().to_string(),
                name: s.agent_id.as_deref().and_then(|a| agents.get(a)).and_then(|a| a.name.clone()).unwrap_or_else(|| title_of(s)),
                status: s.status.clone().unwrap_or_default(),
                agent_id: s.agent_id.clone(),
                user_id: s.user_id.clone(),
                busy: s.busy,
            })
            .collect();
        if sessions.is_empty() {
            continue;
        }
        let ids: HashSet<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        let messages: Vec<NeedsYouMessageInput> = digests
            .iter()
            .filter(|(owner, _)| ids.contains(*owner))
            .flat_map(|(_, messages)| messages.iter())
            .filter(|m| matches!(m.card.as_ref().and_then(|c| c.get("kind")).and_then(Value::as_str), Some("plan" | "question" | "ask")))
            .map(|m| NeedsYouMessageInput {
                id: m.id.clone(),
                session_id: m.session_id.clone(),
                author_session_id: m.author_session_id.clone(),
                body: m.body.clone().unwrap_or_default(),
                created_at: m.created_at.clone().unwrap_or_default(),
                answered_at: m.answered_at.clone(),
                card: m.card.clone(),
            })
            .collect();
        let pull_requests: Vec<NeedsYouPullRequestInput> = rows
            .pull_requests
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(team.id.as_str()))
            .map(|p| NeedsYouPullRequestInput {
                id: p.id.clone(),
                root_session_id: p.root_session_id.clone(),
                number: p.number.unwrap_or_default(),
                title: p.title.clone().unwrap_or_default(),
                state: p.state.clone().unwrap_or_default(),
                checks_state: p.checks_state.clone(),
                base_pull_request_id: p.base_pull_request_id.clone(),
                created_at: p.created_at.clone().unwrap_or_default(),
            })
            .collect();
        let input = NeedsYouInput {
            now: now_ms,
            me: non_empty(Some(&rows.user_id)).map(str::to_string),
            sessions,
            messages,
            pull_requests,
            merge_mode: Some(team.merge_mode().to_string()),
        };
        out.extend(needs_you(&input).into_iter().map(|row| NeedsYouItem { account_id: rows.account_id.clone(), team: team.name.clone(), row }));
    }
    out
}

/// `/needsYou`: the tray button shows while `count` > 0; its list's rows.
pub fn needs_you_feed(items: &[NeedsYouItem]) -> Value {
    json!({"count": items.len(), "rows": items.iter().map(NeedsYouItem::to_json).collect::<Vec<_>>()})
}

/// `EDE_SHELL_DEMO_TRAY=1`: the demo's tray carries the sample inbox and
/// Needs you (only the views that show them set it, so the other goldens
/// keep their bell count and taskbar).
pub fn demo_tray() -> bool {
    std::env::var("EDE_SHELL_DEMO_TRAY").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// The demo's (the goldens') inbox: two unread entries of two teams.
pub fn demo_inbox(now_ms: i64) -> Vec<InboxItem> {
    if !demo_tray() {
        return Vec::new();
    }
    vec![
        InboxItem {
            key: format!("{KEY_PREFIX}demo-1"),
            account_id: "demo".into(),
            unread_ids: vec!["demo-1".into(), "demo-0".into()],
            team: "Feedback".into(),
            summary: "FEED-212 The switcher keeps the picked team".into(),
            body: "Mira commented: works on 0.2.7, closing after the release".into(),
            created_ms: now_ms - 4 * 60_000,
            link: Some(issue_link("FEED-212")),
        },
        InboxItem {
            key: format!("{KEY_PREFIX}demo-2"),
            account_id: "demo".into(),
            unread_ids: vec!["demo-2".into()],
            team: "Side project".into(),
            summary: "Assistant: the nightly build is green again".into(),
            body: "The flaky upload test now retries once.".into(),
            created_ms: now_ms - 75 * 60_000,
            link: Some(session_link("demo-session", None)),
        },
    ]
}

/// The demo's Needs you: a plan and a question.
pub fn demo_needs_you() -> Vec<NeedsYouItem> {
    if !demo_tray() {
        return Vec::new();
    }
    let row = |id: &str, kind, opens, title: &str, chat: &str, age: &str| NeedsYouRow {
        message_id: id.into(),
        session_id: "demo-session".into(),
        root_id: "demo-session".into(),
        kind,
        opens,
        title: title.into(),
        chat: chat.into(),
        meta: String::new(),
        age: age.into(),
        created_at: String::new(),
    };
    use domain::needs_you::NeedsYouKind::{Plan, Question};
    vec![
        NeedsYouItem { account_id: "demo".into(), team: "Feedback".into(), row: row("m1", Plan, NeedsYouOpens::Plan, "Plan: one tray for the inbox", "EDE-65 tray", "3m") },
        NeedsYouItem { account_id: "demo".into(), team: "Side project".into(), row: row("m2", Question, NeedsYouOpens::Card, "Which branch should the release cut from?", "Release 1.4", "1h") },
    ]
}

/// The Exponential app takes `url` on its single-instance socket
/// (`exp-desktop.sock` in its data dir, `EXP_DATA_DIR` overrides); false =
/// nothing listens there.
pub fn hand_off(url: &str) -> bool {
    hand_off_to(&api::default_data_dir().join("exp-desktop.sock"), url)
}

pub fn hand_off_to(sock: &std::path::Path, url: &str) -> bool {
    let Ok(socket) = std::os::unix::net::UnixDatagram::unbound() else { return false };
    match socket.connect(sock).and_then(|_| socket.send(url.as_bytes())) {
        Ok(_) => {
            tracing::info!(url, "handed off to the Exponential app");
            true
        }
        Err(e) => {
            tracing::info!(url, "exp-desktop.sock: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(id: &str, issue: Option<&str>, kind: &str, created: &str, read: bool) -> Notification {
        serde_json::from_value(json!({
            "id": id, "user_id": "u1", "issue_id": issue, "team_id": if issue.is_none() { Some("t2") } else { None },
            "session_id": if issue.is_none() { Some("s9") } else { None },
            "type": kind, "title": format!("title {id}"), "body": "body", "read_at": if read { Some("2026-11-08T10:00:00Z") } else { None },
            "created_at": created,
        }))
        .unwrap()
    }

    fn account() -> AccountRows {
        AccountRows {
            account_id: "a1".into(),
            user_id: "u1".into(),
            teams: vec![serde_json::from_value(json!({"id": "t1", "name": "Acme", "slug": "acme"})).unwrap(), serde_json::from_value(json!({"id": "t2", "name": "Side", "slug": "side"})).unwrap()],
            boards: vec![Board::seeded("b1", "t1", "Main")],
            issues: vec![
                serde_json::from_value(json!({"id": "i1", "board_id": "b1", "number": 12, "identifier": "ACME-12", "title": "Fix the tray", "status": "backlog"})).unwrap(),
                // Its board has not synced: the IDE skips it too.
                serde_json::from_value(json!({"id": "i2", "board_id": "b-gone", "number": 3, "identifier": "ACME-3", "title": "Hidden", "status": "backlog"})).unwrap(),
            ],
            notifications: vec![
                notification("n1", Some("i1"), "issue_comment", "2026-11-08T10:01:00Z", false),
                notification("n2", Some("i1"), "issue_comment", "2026-11-08T10:05:00Z", false),
                notification("n3", Some("i1"), "issue_comment", "2026-11-08T09:00:00Z", true),
                notification("n4", None, "agent_message", "2026-11-08T10:03:00Z", false),
                notification("n5", Some("i2"), "issue_comment", "2026-11-08T10:06:00Z", false),
                notification("n6", None, "support_reply", "2026-11-08T10:07:00Z", false),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn the_inbox_groups_unread_rows_by_issue_like_the_ide() {
        let items = inbox_items(&account(), &HashSet::new());
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "inbox:n2");
        assert_eq!(items[0].unread_ids, ["n2", "n1"]);
        assert_eq!(items[0].summary, "ACME-12 Fix the tray");
        assert_eq!(items[0].team, "Acme");
        assert_eq!(items[0].link.as_deref(), Some("exponential://issue/ACME-12"));
        assert_eq!(items[1].key, "inbox:n4");
        assert_eq!(items[1].team, "Side");
        assert_eq!(items[1].link.as_deref(), Some("exponential://session/s9"));
        // Dismissed here, before the server's echo.
        let read: HashSet<String> = ["n2".to_string(), "n1".to_string()].into();
        assert_eq!(inbox_items(&account(), &read).iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["inbox:n4"]);
    }

    #[test]
    fn the_bell_merges_desktop_and_inbox_newest_first() {
        let inbox = inbox_items(&account(), &HashSet::new());
        let at = |iso: &str| ms(Some(iso));
        let desktop = vec![(at("2026-11-08T10:04:00Z"), json!({"id": 7, "app": "Thunderbird", "summary": "Mail"})), (at("2026-11-08T09:30:00Z"), json!({"id": 6, "app": "Software", "summary": "Updates"}))];
        let toasts: HashSet<u64> = [7].into();
        let inbox_toasts: HashSet<String> = ["inbox:n2".to_string()].into();
        let feed = merged_feed(&desktop, &toasts, &inbox, &inbox_toasts, at("2026-11-08T10:10:00Z"));
        let ids: Vec<Value> = feed["list"].as_array().unwrap().iter().map(|r| r["id"].clone()).collect();
        assert_eq!(ids, [json!("inbox:n2"), json!(7), json!("inbox:n4"), json!(6)]);
        assert_eq!(feed["count"], 4);
        assert_eq!(feed["list"][0]["at"], "5m");
        assert_eq!(feed["list"][0]["app"], "Acme");
        let toast_ids: Vec<Value> = feed["toasts"].as_array().unwrap().iter().map(|r| r["id"].clone()).collect();
        assert_eq!(toast_ids, [json!("inbox:n2"), json!(7)]);
        // #EDE-196: an action button reads its row's id from the action itself.
        assert_eq!(feed["toasts"][0]["actions"], json!([{"id": "inbox:n2", "key": "open", "label": "Open"}, {"id": "inbox:n2", "key": "read", "label": "Mark read"}]));
    }

    #[test]
    fn only_items_new_since_the_start_toast() {
        let inbox = inbox_items(&account(), &HashSet::new());
        let mut seen = HashSet::new();
        // The shell started after both: the first sync is quiet.
        assert!(fresh_items(&inbox, &mut seen, ms(Some("2026-11-08T11:00:00Z"))).is_empty());
        let mut seen = HashSet::new();
        assert_eq!(fresh_items(&inbox, &mut seen, ms(Some("2026-11-08T10:05:00Z"))), ["inbox:n2"]);
        // Seen once = toasted once.
        assert!(fresh_items(&inbox, &mut seen, 0).is_empty());
    }

    fn needs_you_account(answered: bool) -> AccountRows {
        let card = json!({"kind": "question", "question": "Which branch?", "options": [{"label": "master"}]});
        let digest = json!({"waits": [{"id": "m1", "authorKind": "agent", "body": "", "card": card, "createdAt": "2026-11-08T10:00:00Z", "answeredAt": if answered { Some("2026-11-08T10:01:00Z") } else { None }}]});
        AccountRows {
            account_id: "a1".into(),
            user_id: "u1".into(),
            teams: vec![serde_json::from_value(json!({"id": "t1", "name": "Acme", "slug": "acme"})).unwrap()],
            sessions: vec![serde_json::from_value(json!({"id": "s1", "team_id": "t1", "root_id": "s1", "user_id": "u1", "status": "idle", "title": "Release", "digest": digest})).unwrap()],
            ..Default::default()
        }
    }

    #[test]
    fn needs_you_shows_only_while_something_waits() {
        let now = ms(Some("2026-11-08T10:05:00Z"));
        let items = needs_you_items(&needs_you_account(false), now);
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].row.title, "Which branch?");
        assert_eq!(items[0].team, "Acme");
        assert_eq!(items[0].link(), "exponential://session/s1?opens=card&message=m1");
        assert_eq!(needs_you_feed(&items)["count"], 1);
        // Answered: nothing waits, the tray button hides.
        let none = needs_you_items(&needs_you_account(true), now);
        assert!(none.is_empty());
        assert_eq!(needs_you_feed(&none)["count"], 0);
    }

    #[test]
    fn the_accounts_sync_what_the_tray_reads() {
        for shape in SHAPES {
            assert!(crate::accounts::SHAPES.contains(shape), "{shape}");
            assert!(sync::shapes::shape_by_name(shape).is_some(), "{shape}");
        }
    }

    /// #EDE-194: the agent spaces' titles, read as the shell reads them:
    /// one account's own synced store and the keyrings in `data_dir`.
    mod space_titles {
        use super::*;
        use crate::spaces::Spaces;
        use std::path::Path;
        use std::sync::Arc;
        use sync::protocol::{RowKey, ShapeMessage};
        use sync::sealed::Opener;
        use sync::store::ShapeStore;
        use uicore_model::spaces::Display;

        const DISPLAY: Display = Display { width: 1920, height: 1080, scale: 1.0 };

        /// A row as the sync writes it: an insert, or Electric's update
        /// (the changed columns alone).
        fn put(store: &ShapeStore, insert: bool, row: Value) {
            let key = RowKey::Single(row["id"].as_str().unwrap().to_string());
            let value = row.as_object().unwrap().clone();
            let msg = if insert { ShapeMessage::Insert { key, value } } else { ShapeMessage::Update { key, value } };
            store.apply_batch(sync::shapes::shape_by_name("sessions").unwrap(), &[msg], None).unwrap();
        }

        fn space(id: &str, session: &str) -> ede_protocol::SpaceInfo {
            ede_protocol::SpaceInfo { id: ede_protocol::SpaceId::new(id).unwrap(), session_id: ede_protocol::SessionId::new(session).unwrap(), output: id.into(), width: 1280, height: 800, wayland_display: String::new() }
        }

        /// The opener the shell reads the titles with, over `data_dir`.
        fn opener(data_dir: &Path) -> Opener {
            Opener::new(Arc::new(crate::sealed::Keyrings::new(crate::sealed::space_keyring_paths(data_dir, "a1"))))
        }

        /// main.rs `tray_read` + `exp_tray_refresh`: the titles read again
        /// into the spaces; `true` = they moved (the feeds go out).
        fn read_titles(spaces: &mut Spaces, store: &ShapeStore, opener: &Opener) -> bool {
            let rows = AccountRows::read("a1", "u1", store);
            let wanted: HashSet<String> = spaces.wanted_sessions().into_iter().collect();
            let titles = space_sessions(&rows, &wanted, opener);
            let moved = spaces.sessions != titles;
            spaces.sessions = titles;
            moved
        }

        /// The list's row and the pinned view's title bar.
        fn shown(spaces: &Spaces) -> (Value, Value) {
            (spaces.feed()["rows"][0]["title"].clone(), spaces.view_feed(DISPLAY)["title"].clone())
        }

        fn sealed_fixture() -> Value {
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
        }

        /// The fixture's tree keys under `root`, as a key holder writes its
        /// keyring file (the app's `seal-keys`, the daemon's `cli-seal-keys`).
        fn write_keyring(path: &Path, root: &str, fx: &Value) {
            let keys: serde_json::Map<String, Value> = fx["keys"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(epoch, hex)| (epoch.clone(), json!(enet_core::sealed::b64url_encode(&data_encoding::HEXLOWER.decode(hex.as_str().unwrap().as_bytes()).unwrap()))))
                .collect();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, json!({"v": 1, "keys": {root: keys}}).to_string()).unwrap();
        }

        /// A session retitled after its space exists (the CLI's auto-name
        /// lands on its row in the first minute): the next read carries
        /// the title into the list and the pinned view; before it, the row
        /// reads `Agent space n`, never `Untitled`.
        #[test]
        fn a_rename_reaches_the_list_and_the_pinned_view_at_the_next_read() {
            let dir = tempfile::tempdir().unwrap();
            let store = ShapeStore::open(&dir.path().join("sync.sqlite")).unwrap();
            put(&store, true, json!({"id": "s1", "team_id": "t1", "root_id": "s1", "user_id": "u1", "status": "running", "created_at": "2026-11-21T16:00:00Z"}));
            let opener = opener(dir.path());
            let mut spaces = Spaces { list: vec![space("space-1", "s1")], ..Default::default() };
            spaces.pin(Some("s1"), None);
            assert!(read_titles(&mut spaces, &store, &opener), "the first sighting reads the titles");
            assert_eq!(shown(&spaces), (json!("Agent space 1"), json!("Agent space 1")), "nothing titles it yet");
            // The rename: a delta on `sessions` reads the tray again (main.rs, at most once a second).
            for shape in ["sessions", "session_issues", "issues"] {
                assert!(refreshes_tray(shape), "{shape}");
            }
            put(&store, false, json!({"id": "s1", "title": "Wikipedia search box timing test"}));
            assert!(read_titles(&mut spaces, &store, &opener), "the titles moved: the feeds go out");
            assert_eq!(shown(&spaces), (json!("Wikipedia search box timing test"), json!("Wikipedia search box timing test")));
            put(&store, false, json!({"id": "s1", "title": "Wikipedia timing, second round"}));
            assert!(read_titles(&mut spaces, &store, &opener));
            assert_eq!(shown(&spaces), (json!("Wikipedia timing, second round"), json!("Wikipedia timing, second round")));
            assert!(!read_titles(&mut spaces, &store, &opener), "nothing moved: no feed");
        }

        /// The real-session proof's case: the host sealed the auto-name and
        /// the first prompt to the tree. With no keyring here holding that
        /// tree the row reads `Agent space n` (the title ladder alone ends
        /// at `Untitled`); the CLI daemon's keyring opens it like the app's.
        #[test]
        fn a_sealed_title_opens_with_either_installs_keyring_and_reads_agent_space_n_without() {
            let fx = sealed_fixture();
            let case = fx["bodyCases"].as_array().unwrap().iter().find(|c| c["name"] == "sealed, epoch 1").unwrap().clone();
            let (body, text) = (case["body"].as_str().unwrap(), case["text"].as_str().unwrap());
            let dir = tempfile::tempdir().unwrap();
            let store = ShapeStore::open(&dir.path().join("sync.sqlite")).unwrap();
            let first = json!({"id": "m1", "authorKind": "user", "authorId": "u1", "body": body, "createdAt": "2026-11-21T16:00:00Z"});
            put(&store, true, json!({"id": "s1", "team_id": "t1", "root_id": "s1", "user_id": "u1", "status": "running", "sealed_title": body, "digest": {"first": first, "preview": first}, "created_at": "2026-11-21T16:00:00Z"}));
            let opener = opener(dir.path());
            let mut spaces = Spaces { list: vec![space("space-3", "s1")], ..Default::default() };
            spaces.pin(Some("s1"), None);
            read_titles(&mut spaces, &store, &opener);
            assert_eq!(spaces.sessions[0].title, None, "no rung yields text");
            assert_eq!(shown(&spaces), (json!("Agent space 3"), json!("Agent space 3")));
            let mut row = AccountRows::read("a1", "u1", &store).sessions.remove(0);
            opener.open_session(&mut row);
            assert_eq!(domain::session_title::session_title(&row, &[], &[], &[]), "Untitled", "what the row read before #EDE-194");
            // The daemon hosts the session: its keyring holds the tree.
            let [app, daemon] = crate::sealed::space_keyring_paths(dir.path(), "a1");
            write_keyring(&daemon, "s1", &fx);
            assert!(read_titles(&mut spaces, &store, &opener));
            assert_eq!(shown(&spaces), (json!(text), json!(text)));
            // The app hosts it instead: its keyring opens it the same.
            std::fs::remove_file(&daemon).unwrap();
            assert!(read_titles(&mut spaces, &store, &opener), "the key gone: locked again");
            assert_eq!(shown(&spaces).0, json!("Agent space 3"));
            write_keyring(&app, "s1", &fx);
            assert!(read_titles(&mut spaces, &store, &opener));
            assert_eq!(shown(&spaces), (json!(text), json!(text)));
            // Its first prompt (sealed too) titles it once the title is gone.
            put(&store, false, json!({"id": "s1", "sealed_title": null}));
            read_titles(&mut spaces, &store, &opener);
            assert_eq!(shown(&spaces).0, json!(text), "the opened first prompt");
        }
    }

    #[test]
    fn a_hand_off_reaches_the_apps_socket_and_fails_quietly_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("exp-desktop.sock");
        assert!(!hand_off_to(&sock, "exponential://issue/ACME-12"));
        let listener = std::os::unix::net::UnixDatagram::bind(&sock).unwrap();
        assert!(hand_off_to(&sock, "exponential://issue/ACME-12"));
        let mut buf = [0u8; 128];
        let n = listener.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"exponential://issue/ACME-12");
    }
}
