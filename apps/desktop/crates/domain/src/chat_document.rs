//! The agent chat (#EDE-65 lane I, shared by lane O): one conversation as
//! the shared chat document draws it (eDE and the IDE) (`apps/ede/ui/chat.json`, the model
//! `uicore_model::chat`). The domain builds the chat (session-chat.json:
//! the rows, their order, the cards' facts); this resolves the words the
//! other fixtures own (a divider's label, a plan's status, the locked
//! pill) and the clock, so the model only decides how the chat is drawn.

use std::collections::HashMap;

use crate::rows::MessageRow;
use crate::session_tree::{CardSurface, SessionChatRow};
use uicore_model::chat::{ChatGroup, ChatOption, ChatRowInput};

/// `HH:MM` (local) of a wire timestamp; empty when unparseable.
pub fn clock(at: Option<&str>) -> String {
    at.and_then(crate::session_tree::iso_ms)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// A card's surface as the Embed draws it (`{version?, root, data}`);
/// `answered` = the data model as the person answered it, which a closed
/// plan or question card shows instead of the card's own (the IDE's
/// `ensure_closed_surface`).
fn embed_surface(surface: Option<&CardSurface>, answered: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let surface = surface?;
    let surface = CardSurface { data: answered.filter(|data| data.is_object()).cloned().or_else(|| surface.data.clone()), ..surface.clone() };
    serde_json::to_value(surface).ok()
}

/// The domain's chat rows as the chat model's; `messages` = the rows'
/// messages by id (their time), `name` = a person's display name.
pub fn rows(rows: &[SessionChatRow], messages: &HashMap<&str, &MessageRow>, name: impl Fn(Option<&str>) -> Option<String>) -> Vec<ChatRowInput> {
    rows.iter().filter_map(|row| chat_row(row, messages, &name)).collect()
}

fn chat_row(row: &SessionChatRow, messages: &HashMap<&str, &MessageRow>, name: &impl Fn(Option<&str>) -> Option<String>) -> Option<ChatRowInput> {
    let at = |id: &str| clock(messages.get(id).and_then(|m| m.created_at.as_deref()));
    Some(match row {
        SessionChatRow::Message { id, author, author_id, body, queued, .. } => ChatRowInput::Message {
            id: id.clone(),
            author: author.clone(),
            body: body.clone(),
            name: if author == "user" { name(author_id.as_deref()) } else { None },
            time: at(id),
            queued: *queued,
            group: None,
        },
        SessionChatRow::Session { id, card } => ChatRowInput::Card {
            id: id.clone(),
            icon: card.icon.clone(),
            title: card.name.clone(),
            text: card.kind_word.clone(),
            target: Some(card.session_id.clone()),
            surface: None,
        },
        SessionChatRow::Report { id, session_id, text, .. } => {
            ChatRowInput::Card { id: id.clone(), icon: "session-tree".into(), title: text.clone(), text: String::new(), target: Some(session_id.clone()), surface: None }
        }
        SessionChatRow::Suggestions { id, session_id, header, .. } => {
            ChatRowInput::Card { id: id.clone(), icon: "list-checks".into(), title: header.clone(), text: String::new(), target: Some(session_id.clone()), surface: None }
        }
        SessionChatRow::Question { id, question, options, custom, multi, surface, answer, answers, surface_data, answered, dismissed, .. } => ChatRowInput::Question {
            id: id.clone(),
            question: question.clone(),
            options: options.iter().map(|o| ChatOption { label: o.label.clone(), description: o.description.clone() }).collect(),
            custom: *custom,
            multi: *multi,
            waiting: !*answered && *dismissed != Some(true),
            answer: answers.as_ref().filter(|a| !a.is_empty()).map(|a| a.join(", ")).or_else(|| answer.clone()),
            surface: embed_surface(surface.as_ref(), surface_data.as_ref()),
        },
        // The plan row (session_chat `plan_row`): the plan's label, its status word.
        SessionChatRow::Plan { id, session_id, plan, surface, decision, fresh, surface_data, .. } => ChatRowInput::Card {
            id: id.clone(),
            icon: if decision.as_deref() == Some("approved") { "ui-check" } else { "coding-plan" }.into(),
            title: crate::plan_page::plan_row_label(plan),
            text: match decision {
                None => crate::plan_page::PLAN_PAGE.copy.needs_you.clone(),
                Some(decision) => crate::ask_dock::plan_decision_label(decision, *fresh),
            },
            target: Some(session_id.clone()),
            surface: embed_surface(surface.as_ref(), surface_data.as_ref()),
        },
        SessionChatRow::Subagent { id, title, status } => ChatRowInput::Line {
            id: id.clone(),
            icon: "coding-subagent".into(),
            text: [title.as_str(), status.as_str()].into_iter().filter(|p| !p.trim().is_empty()).collect::<Vec<_>>().join(" · "),
        },
        SessionChatRow::Picture { id, attachment_id, caption, .. } => {
            ChatRowInput::Picture { id: id.clone(), attachment_id: attachment_id.clone(), caption: caption.clone() }
        }
        // #EXP-1772: an event line is THE inline marker.
        SessionChatRow::Event { id, text, .. } => ChatRowInput::Marker { id: id.clone(), label: text.clone() },
        SessionChatRow::Divider { id, kind, at } => {
            let time = clock(Some(at));
            let label = if time.is_empty() { kind.label().to_string() } else { format!("{} · {time}", kind.label()) };
            ChatRowInput::Pill { id: id.clone(), icon: kind.icon().into(), label }
        }
        SessionChatRow::Locked { id, reason, .. } => {
            let placeholder = &*crate::sealed::PLACEHOLDER;
            ChatRowInput::Pill { id: id.clone(), icon: placeholder.icon.clone(), label: placeholder.text(*reason).to_string() }
        }
        SessionChatRow::Surface { id, session_id, title, surface } => ChatRowInput::Card {
            id: id.clone(),
            icon: "ui-widget".into(),
            title: title.clone().unwrap_or_else(|| crate::plan_card::PLAN_CARD.copy.surface.clone()),
            text: String::new(),
            target: Some(session_id.clone()),
            surface: embed_surface(Some(surface), None),
        },
    })
}

/// What an Embed's inner `event` on a chat card asks of the host (lane Q,
/// `ede_chat {action: card, messageId, name, context}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardReply {
    /// The person's next message into `session_id` (the IDE's surface card:
    /// `chats::surface_reply`).
    Send { session_id: String, text: String },
}

/// THE card rule the IDE follows: an `exponential_show` surface's `reply`
/// event with text posts that text as the person's next message; a plan's or
/// question's inline surface has no transport (plan-card.json: its events
/// stay in the card, the options and the plan page answer it), and every
/// other event is ignored.
pub fn card_reply(rows: &[SessionChatRow], message_id: &str, name: Option<&str>, context: Option<&serde_json::Value>) -> Option<CardReply> {
    let text = crate::chats::surface_reply(name, context)?;
    match rows.iter().find(|row| row.id() == message_id)? {
        SessionChatRow::Surface { session_id, .. } => Some(CardReply::Send { session_id: session_id.clone(), text }),
        _ => None,
    }
}

/// A conversation row as the grouping reads it (transcript-grouping.json):
/// a card-less message, a child's report (a message from its session), a
/// subagent line (the agent's trace), every other card full.
pub fn transcript_item(row: &SessionChatRow) -> crate::transcript_grouping::TranscriptItem {
    use crate::transcript_grouping::TranscriptItem;
    match row {
        SessionChatRow::Message { id, author, author_id, author_session_id, .. } => TranscriptItem::message(id.as_str(), author, author_id.as_deref(), author_session_id.as_deref()),
        SessionChatRow::Report { id, session_id, .. } => TranscriptItem::message(id.as_str(), crate::contract::MESSAGE_AUTHOR_KIND_AGENT, None, Some(session_id.as_str())),
        SessionChatRow::Subagent { id, .. } => TranscriptItem::trace(id.as_str()),
        other => TranscriptItem::full(other.id()),
    }
}

/// The prefix of an avatar icon name a host paints as THE sender avatar
/// disc (the IDE's `agent_avatar`: the inner icon on the secondary disc).
pub const AVATAR_ICON_PREFIX: &str = "avatar:";

/// #EDE-65 lane O: the IDE transcript's grouping (#EXP-1736,
/// transcript-grouping.json) on the chat's message rows, so eDE's bubbles
/// sit like the IDE's: `out` = the viewer's own, `in` under the sender's
/// avatar (`avatar` names its icon, a group's first row only) with the
/// sender's name over a group's first bubble, the group gaps; #EXP-1772: a
/// cross-session sender's bubble carries its session as `link` (a relay:
/// the document draws its name above the bubble as the press). `transcript`
/// = who is who (its `items` are filled from `rows`).
pub fn group(out: &mut [ChatRowInput], rows: &[SessionChatRow], mut transcript: crate::transcript_grouping::TranscriptInput, avatar: impl Fn(&crate::transcript_grouping::TranscriptSender) -> Option<String>) {
    use crate::transcript_grouping::{transcript_layout, TranscriptSide};
    transcript.items = rows.iter().map(transcript_item).collect();
    let layouts: HashMap<String, crate::transcript_grouping::TranscriptRowLayout> = transcript_layout(&transcript).into_iter().map(|l| (l.id.clone(), l)).collect();
    for row in out.iter_mut() {
        let ChatRowInput::Message { id, name, group, .. } = row else { continue };
        let Some(layout) = layouts.get(id.as_str()) else { continue };
        let side = match layout.side {
            TranscriptSide::Own => "out",
            TranscriptSide::Other => "in",
            TranscriptSide::Full => continue,
        };
        let sender = layout.sender.as_ref();
        if side == "in" {
            // #EXP-1772: the model adds a relay's chevron itself.
            *name = sender.filter(|_| layout.name).map(|s| s.name.clone());
        }
        *group = Some(ChatGroup {
            side: Some(side.into()),
            avatar: sender.filter(|_| side == "in" && layout.avatar).and_then(&avatar),
            indent: side == "in",
            link: sender.filter(|_| side == "in").and_then(|s| s.link.clone()),
            gap: layout.gap.round().max(0.0) as u32,
        });
    }
}

/// The agent CLI a session's `cli` names for the `/` menu (the IDE's
/// `agent_of`): `codex`, else `claude` (an absent or unknown one too).
pub fn session_agent(cli: Option<&str>) -> &'static str {
    match cli.map(str::trim) {
        Some(id) if id.eq_ignore_ascii_case("codex") => "codex",
        _ => "claude",
    }
}

/// The team's prompts in their order (sort order, then name), the menu's
/// second group.
pub fn ordered_prompts(mut prompts: Vec<crate::rows::PromptRow>) -> Vec<crate::rows::PromptRow> {
    prompts.sort_by(|a, b| a.sort_order.unwrap_or(0.0).partial_cmp(&b.sort_order.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.name.cmp(&b.name)));
    prompts
}

/// THE `/` menu (`crate::slash_menu`, the IDE's rules) as `/chatMenu`
/// binds it: open while the draft is a bare command token with matches:
/// the builtins of `cli` (`None` = no session yet: none), then the team's
/// prompts (NEW-2: icon + name; a pick loads its body into the composer).
pub fn slash_menu(draft: &str, cli: Option<&str>, prompts: &[crate::rows::PromptRow]) -> serde_json::Value {
    let Some(query) = crate::slash_menu::slash_query(draft) else { return serde_json::json!({"open": false, "rows": []}) };
    let mut rows: Vec<serde_json::Value> = cli
        .map(|cli| crate::slash_menu::menu_matches(draft, cli))
        .unwrap_or_default()
        .into_iter()
        .map(|c| plain_row(serde_json::json!({"key": format!("command:{}", c.name), "command": format!("/{}", c.name), "description": c.description, "hint": c.arg_hint, "icon": null, "insert": crate::slash_menu::insertion(c.name, c.arg_hint), "prompt": ""})))
        .collect();
    let named: Vec<&crate::rows::PromptRow> = prompts.iter().filter(|p| p.name.as_deref().is_some_and(|n| !n.trim().is_empty())).collect();
    rows.extend(crate::slash_menu::filter_prompts(&named, |p| p.name.as_deref().unwrap_or_default(), query).into_iter().map(|p| {
        plain_row(serde_json::json!({"key": format!("prompt:{}", p.id), "command": "", "description": p.name, "hint": "", "icon": p.icon.clone().filter(|i| !i.is_empty()).unwrap_or_else(|| PROMPT_ICON.into()), "insert": "", "prompt": p.id}))
    }));
    serde_json::json!({"open": !rows.is_empty(), "rows": rows})
}

/// A person `@` can name: their label and email.
pub struct Member {
    pub name: String,
    pub email: String,
    /// #EDE-65 lane T: the member's picture, for the `@` row's avatar.
    pub image: Option<String>,
}

/// #EDE-65 lane T: one `#` row: the issue, its status glyph and tone
/// (`""` = a plain row, a host that has no status to hand).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MentionIssue {
    pub identifier: String,
    pub title: String,
    pub icon: String,
    pub tone: String,
}

impl From<(String, String)> for MentionIssue {
    fn from((identifier, title): (String, String)) -> Self {
        Self { identifier, title, ..Self::default() }
    }
}

/// The fields a menu row's leading slot binds (a template reads every one,
/// the ones its kind does not use blank): the `@` avatar, the `:` emoji, the
/// `#` status glyph and tone.
fn row_leading(mut row: serde_json::Value, avatar: Option<(&str, Option<&str>)>, emoji: &str, status: (&str, &str)) -> serde_json::Value {
    let (name, src) = avatar.unwrap_or(("", None));
    row["hasLeading"] = serde_json::json!(avatar.is_some() || !emoji.is_empty() || !status.0.is_empty());
    row["avatarName"] = serde_json::json!(name);
    row["avatarSrc"] = serde_json::json!(src.unwrap_or(""));
    row["emoji"] = serde_json::json!(emoji);
    row["statusIcon"] = serde_json::json!(status.0);
    row["statusTone"] = serde_json::json!(status.1);
    row
}

/// A row with nothing in its leading slot (`/`, a host's own rows).
pub fn plain_row(row: serde_json::Value) -> serde_json::Value {
    row_leading(row, None, "", ("", ""))
}

/// THE `@`/`#` typeahead (`crate::typeahead`, the IDE's rules) over the
/// token at the END of the draft (the chat's composer reports its text, not
/// its caret: the token being typed is the last one). `@` = the members by
/// name or email, inserting `@<email>`; `#` = the issues the engine ranks
/// (`issues`: the caller's already ranked `(identifier, title)`), inserting
/// `#<IDENTIFIER>`. Rows have the `/` menu's shape; `insert` is the whole
/// new draft (the token replaced, a space after it).
pub fn mention_menu(draft: &str, members: &[Member], issues: &[MentionIssue]) -> Option<serde_json::Value> {
    use crate::typeahead::{detect_trigger, mention_matches, CompletionTrigger, MAX_ITEMS};
    let token = detect_trigger(draft, draft.len())?;
    let head = &draft[..token.start];
    let rows: Vec<serde_json::Value> = match token.trigger {
        CompletionTrigger::Mention => {
            let needle = token.query.to_lowercase();
            let mut picks: Vec<&Member> = members.iter().filter(|m| mention_matches(&m.name, &m.email, &needle)).collect();
            picks.sort_by(|a, b| a.name.cmp(&b.name));
            picks.dedup_by(|a, b| a.email == b.email);
            picks
                .into_iter()
                .take(MAX_ITEMS)
                .map(|m| row_leading(serde_json::json!({"key": format!("member:{}", m.email), "command": "", "description": m.name, "hint": m.email, "icon": null, "insert": format!("{head}@{} ", m.email), "prompt": ""}), Some((&m.name, m.image.as_deref())), "", ("", "")))
                .collect()
        }
        CompletionTrigger::IssueRef => issues
            .iter()
            .take(MAX_ITEMS)
            .map(|issue| row_leading(serde_json::json!({"key": format!("issue:{}", issue.identifier), "command": issue.identifier, "description": issue.title, "hint": "", "icon": null, "insert": format!("{head}#{} ", issue.identifier), "prompt": ""}), None, "", (&issue.icon, &issue.tone)))
            .collect(),
        // EXP-551: glyph · `:shortcode:` · label; a pick inserts the UNICODE.
        CompletionTrigger::Emoji => {
            let catalog = crate::emoji::catalog();
            catalog
                .search(&token.query, MAX_ITEMS)
                .into_iter()
                .filter_map(|index| catalog.get(index))
                .map(|emoji| {
                    let code = emoji.shortcodes.first().cloned().unwrap_or_else(|| emoji.label.clone());
                    row_leading(serde_json::json!({"key": format!("emoji:{code}"), "command": "", "description": format!(":{code}:"), "hint": emoji.label, "icon": null, "insert": format!("{head}{}", emoji.unicode), "prompt": ""}), None, &emoji.unicode, ("", ""))
                })
                .collect()
        }
    };
    Some(serde_json::json!({"open": !rows.is_empty(), "rows": rows}))
}

/// EXP-551: a CLOSED exact `:shortcode:` at the end of the draft commits
/// to its unicode at once (the IDE's `exact_emoji`): the new draft, or `None`.
pub fn commit_emoji(draft: &str) -> Option<String> {
    let token = crate::typeahead::detect_trigger(draft, draft.len()).filter(|t| t.trigger == crate::typeahead::CompletionTrigger::Emoji && t.closed)?;
    let emoji = crate::emoji::catalog().find_shortcode(&token.query)?;
    Some(format!("{}{}", &draft[..token.start], emoji.unicode))
}

/// What a key the composer hands the open menu does (genUI's Composer
/// `menuKeys` → `key` {key}), the IDE's composer menus (`move_slash`,
/// `move_completion`, `on_slash_*`): the arrows move the highlight over the
/// menu's `len` rows, wrapping; Enter and Tab pick the highlighted row (the
/// same path as its press); Escape shuts the menu for the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Highlight(usize),
    Pick(usize),
    Dismiss,
    Ignore,
}

pub fn menu_key(key: &str, selected: usize, len: usize) -> MenuKey {
    if len == 0 {
        return MenuKey::Ignore;
    }
    let selected = selected.min(len - 1);
    match key {
        "ArrowUp" => MenuKey::Highlight((selected + len - 1) % len),
        "ArrowDown" => MenuKey::Highlight((selected + 1) % len),
        "Enter" | "Tab" => MenuKey::Pick(selected),
        "Escape" => MenuKey::Dismiss,
        _ => MenuKey::Ignore,
    }
}

/// Marks the highlighted row of a `/chatMenu` (`selected`, the Row's active
/// state) and returns its index. A rebuilt menu keeps the IDE's rules: the
/// `/` menu its row, clamped to the new rows (`keep`, `refresh_slash`); the
/// typeahead starts over at its first (`refresh_completion`).
pub fn highlight(menu: &mut serde_json::Value, previous: usize, keep: bool) -> usize {
    let Some(rows) = menu.get_mut("rows").and_then(serde_json::Value::as_array_mut) else { return 0 };
    let selected = if keep { previous.min(rows.len().saturating_sub(1)) } else { 0 };
    for (index, row) in rows.iter_mut().enumerate() {
        row["selected"] = serde_json::Value::Bool(index == selected);
    }
    selected
}

/// #EDE-65 lane O: composer-menu.json's row ids the chat document's "+"
/// Menu picks (`ede_chat plus {value}`, `uicore_model::chat::plus_pick`).
pub const PLUS_ADD_FILE: &str = "add-file";
pub const PLUS_PROMPTS: &str = "prompts";
pub const PLUS_ISSUE: &str = "implement-issue";
pub const PLUS_TEAMS: &str = "teams";
pub const PLUS_DEVICE: &str = "device";
pub const PLUS_ACCOUNT: &str = "account";
pub const PLUS_MODEL: &str = "model";

/// composer-menu.json `rows[id].label` / `.icon` (byte-identical ×4).
pub fn plus_row(id: &str) -> uicore_model::chat::ChatPlusRow {
    let (label, icon) = match id {
        PLUS_ADD_FILE => ("Photo or file", "ui-attach"),
        PLUS_ISSUE => ("Issue", "editor-issue-ref"),
        PLUS_PROMPTS => ("Prompts", "coding-command"),
        PLUS_TEAMS => ("Teams", "ui-team"),
        PLUS_DEVICE => ("Device", "nav-devices"),
        PLUS_ACCOUNT => ("Account", "settings-account"),
        PLUS_MODEL => ("Model", "coding-assistant"),
        _ => ("", ""),
    };
    uicore_model::chat::ChatPlusRow { id: id.to_string(), label: label.into(), icon: icon.into(), ..Default::default() }
}

/// The team's prompts as the Prompts › entries (the `/` menu's order and
/// glyphs; a pick loads the body like the `/` menu's).
pub fn plus_prompt_items(prompts: &[crate::rows::PromptRow]) -> Vec<uicore_model::chat::ChatPlusItem> {
    prompts
        .iter()
        .filter_map(|p| {
            let name = p.name.as_deref().filter(|n| !n.trim().is_empty())?;
            Some(uicore_model::chat::ChatPlusItem {
                label: name.to_string(),
                value: p.id.clone(),
                icon: Some(p.icon.clone().filter(|i| !i.is_empty()).unwrap_or_else(|| PROMPT_ICON.into())),
                ..Default::default()
            })
        })
        .collect()
}

/// The "+" rows a composer that can attach and load prompts serves (eDE's):
/// Photo or file, then Prompts › while the team has one.
pub fn plus_rows(prompts: &[crate::rows::PromptRow]) -> Vec<uicore_model::chat::ChatPlusRow> {
    let mut rows = vec![plus_row(PLUS_ADD_FILE)];
    let items = plus_prompt_items(prompts);
    if !items.is_empty() {
        rows.push(uicore_model::chat::ChatPlusRow { items: Some(items), searchable: true, ..plus_row(PLUS_PROMPTS) });
    }
    rows
}

/// A prompt without a curated icon (the IDE's generic prompt glyph).
pub const PROMPT_ICON: &str = "action-default";

#[cfg(test)]
mod tests {
    use super::*;

    /// composer-menu.json: every "+" row's label and icon, byte-identical.
    #[test]
    fn the_plus_rows_are_the_composer_menus() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/composer-menu.json")).unwrap();
        for id in [PLUS_ADD_FILE, PLUS_ISSUE, PLUS_PROMPTS, PLUS_TEAMS, PLUS_DEVICE, PLUS_ACCOUNT, PLUS_MODEL] {
            let spec = fixture["rows"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("{id}"));
            let row = plus_row(id);
            assert_eq!(row.label, spec["label"].as_str().unwrap(), "{id}");
            assert_eq!(row.icon, spec["icon"].as_str().unwrap(), "{id}");
        }
    }

    /// eDE's "+": Photo or file always, Prompts › (searchable, the named
    /// prompts with their glyphs) only while there is one.
    #[test]
    fn edes_plus_serves_the_file_chooser_and_the_prompts() {
        let prompt = |id: &str, name: Option<&str>, icon: Option<&str>| -> crate::rows::PromptRow { serde_json::from_value(serde_json::json!({"id": id, "name": name, "icon": icon})).unwrap() };
        let rows = plus_rows(&[]);
        assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), [PLUS_ADD_FILE]);
        assert!(rows[0].items.is_none(), "an action row");
        let rows = plus_rows(&[prompt("p1", Some("Triage"), None), prompt("p2", Some("  "), None), prompt("p3", Some("Ship"), Some("ui-rocket"))]);
        assert_eq!(rows[1].id, PLUS_PROMPTS);
        assert!(rows[1].searchable);
        let items = rows[1].items.as_ref().unwrap();
        assert_eq!(items.iter().map(|i| (i.label.as_str(), i.value.as_str(), i.icon.as_deref())).collect::<Vec<_>>(), [("Triage", "p1", Some(PROMPT_ICON)), ("Ship", "p3", Some("ui-rocket"))]);
        let menu = uicore_model::chat::plus_menu(&rows);
        assert_eq!(menu[1]["items"][1]["value"], "prompts:p3");
    }

    #[test]
    fn the_arrows_wrap_the_highlight_and_enter_tab_pick_escape_dismisses() {
        assert_eq!(menu_key("ArrowDown", 0, 3), MenuKey::Highlight(1));
        assert_eq!(menu_key("ArrowDown", 2, 3), MenuKey::Highlight(0), "down wraps to the first");
        assert_eq!(menu_key("ArrowUp", 0, 3), MenuKey::Highlight(2), "up wraps to the last");
        assert_eq!(menu_key("Enter", 1, 3), MenuKey::Pick(1));
        assert_eq!(menu_key("Tab", 1, 3), MenuKey::Pick(1));
        assert_eq!(menu_key("Enter", 7, 3), MenuKey::Pick(2), "a stale highlight clamps");
        assert_eq!(menu_key("Escape", 1, 3), MenuKey::Dismiss);
        assert_eq!(menu_key("Enter", 0, 0), MenuKey::Ignore, "no rows, no menu");
        assert_eq!(menu_key("Home", 0, 3), MenuKey::Ignore);
    }

    #[test]
    fn the_slash_menu_keeps_its_highlight_the_typeahead_starts_over() {
        let mut menu = slash_menu("/", Some("claude"), &[]);
        assert_eq!(highlight(&mut menu, 2, true), 2);
        assert_eq!((menu["rows"][2]["selected"].as_bool(), menu["rows"][0]["selected"].as_bool()), (Some(true), Some(false)));
        let mut narrowed = slash_menu("/co", Some("claude"), &[]);
        let len = narrowed["rows"].as_array().unwrap().len();
        assert_eq!(highlight(&mut narrowed, 99, true), len - 1, "clamped to the new rows");
        let mut emoji = mention_menu("nice :ta", &[], &[]).unwrap();
        assert_eq!(highlight(&mut emoji, 3, false), 0);
        assert_eq!(emoji["rows"][0]["selected"], true);
        assert_eq!(highlight(&mut serde_json::json!({"open": false, "rows": []}), 3, true), 0);
    }

    #[test]
    fn colon_lists_emoji_and_a_closed_shortcode_commits() {
        let menu = mention_menu("nice :tad", &[], &[]).expect(":");
        assert_eq!(menu["rows"][0]["description"], "🎉 :tada:");
        assert_eq!(menu["rows"][0]["insert"], "nice 🎉");
        assert_eq!(commit_emoji("nice :tada:").as_deref(), Some("nice 🎉"));
        assert_eq!(commit_emoji("nice :tad"), None);
        assert!(mention_menu("at 12:30", &[], &[]).is_none(), "times never open it");
    }

    #[test]
    fn at_lists_members_and_hash_the_ranked_issues_replacing_the_last_token() {
        let members = vec![Member { name: "Dana Reyes".into(), email: "dana@x.com".into(), image: Some("https://x/d.png".into()) }, Member { name: "Alex".into(), email: "alex@x.com".into(), image: None }];
        let menu = mention_menu("ping @da", &members, &[]).expect("@");
        assert_eq!(menu["rows"].as_array().unwrap().len(), 1);
        assert_eq!(menu["rows"][0]["insert"], "ping @dana@x.com ");
        let issues = vec![MentionIssue { identifier: "EXP-12".into(), title: "Fix the tray".into(), icon: "status-in-progress".into(), tone: "warning".into() }];
        let hash = mention_menu("see #EX", &members, &issues).expect("#");
        assert_eq!((hash["rows"][0]["command"].as_str(), hash["rows"][0]["insert"].as_str()), (Some("EXP-12"), Some("see #EXP-12 ")));
        // #EDE-65 lane T: the leading slot's fields, every row carrying them.
        assert_eq!((hash["rows"][0]["statusIcon"].as_str(), hash["rows"][0]["statusTone"].as_str(), hash["rows"][0]["hasLeading"].as_bool()), (Some("status-in-progress"), Some("warning"), Some(true)));
        assert_eq!((menu["rows"][0]["avatarName"].as_str(), menu["rows"][0]["avatarSrc"].as_str(), menu["rows"][0]["icon"].is_null()), (Some("Dana Reyes"), Some("https://x/d.png"), true));
        let emoji = mention_menu("nice :tada", &[], &[]).expect(":");
        assert_eq!((emoji["rows"][0]["emoji"].as_str(), emoji["rows"][0]["hasLeading"].as_bool()), (Some("🎉"), Some(true)));
        assert!(emoji["rows"][0]["description"].as_str().unwrap().starts_with(':'));
        let slash = slash_menu("/", Some("claude"), &[]);
        assert_eq!((slash["rows"][0]["hasLeading"].as_bool(), slash["rows"][0]["emoji"].as_str()), (Some(false), Some("")));
        assert!(mention_menu("plain text", &members, &issues).is_none());
        assert_eq!(mention_menu("@zzz", &members, &[]).unwrap()["open"], false);
    }

    #[test]
    fn the_team_s_prompts_follow_the_builtins_by_their_slash_name() {
        let prompt = |id: &str, name: &str, order: f64| -> crate::rows::PromptRow { serde_json::from_value(serde_json::json!({"id": id, "name": name, "sort_order": order})).unwrap() };
        let prompts = ordered_prompts(vec![prompt("p2", "Release notes", 2.0), prompt("p1", "Fix flaky tests", 1.0)]);
        let menu = slash_menu("/", Some("claude"), &prompts);
        let rows = menu["rows"].as_array().unwrap();
        assert_eq!(rows.len(), crate::contract::STEER_COMMAND_NAMES.len() + 2);
        assert_eq!((rows[rows.len() - 2]["description"].as_str(), rows[rows.len() - 2]["prompt"].as_str()), (Some("Fix flaky tests"), Some("p1")));
        let home = slash_menu("/rel", None, &prompts);
        assert_eq!(home["rows"].as_array().unwrap().len(), 1, "the home lists prompts alone");
        assert_eq!(home["rows"][0]["icon"], PROMPT_ICON);
    }

    #[test]
    fn the_slash_menu_lists_the_session_cli_s_builtins_while_the_draft_is_a_command() {
        let menu = slash_menu("/co", Some("claude"), &[]);
        assert_eq!(menu["open"], true);
        assert_eq!(menu["rows"][0]["command"], "/compact");
        assert_eq!(menu["rows"][0]["insert"], "/compact ");
        assert_eq!(slash_menu("/co ", Some("claude"), &[])["open"], false, "a space closes it");
        assert_eq!(slash_menu("/", None, &[])["open"], false, "the home has no builtins");
        assert_eq!(session_agent(Some("Codex")), "codex");
        assert_eq!(session_agent(None), "claude");
    }
    use crate::chats::DividerKind;

    fn message(id: &str, at: &str) -> MessageRow {
        MessageRow { id: id.into(), created_at: Some(at.into()), ..Default::default() }
    }

    #[test]
    fn a_person_s_message_carries_their_name_and_the_time() {
        let m = message("m1", "2026-11-08T09:14:00.000Z");
        let messages = HashMap::from([("m1", &m)]);
        let row = SessionChatRow::Message { id: "m1".into(), author: "user".into(), author_id: Some("u1".into()), body: "hi".into(), queued: false, author_session_id: None, xref: None };
        let out = rows(&[row], &messages, |id| (id == Some("u1")).then(|| "Dana".to_string()));
        let ChatRowInput::Message { name, time, .. } = &out[0] else { panic!("{out:?}") };
        assert_eq!(name.as_deref(), Some("Dana"));
        assert_eq!(time, &clock(Some("2026-11-08T09:14:00.000Z")));
        assert_eq!(time.len(), 5, "HH:MM");
    }

    #[test]
    fn an_agent_has_no_name_and_a_divider_reads_its_label_and_time() {
        let agent = SessionChatRow::Message { id: "m2".into(), author: "agent".into(), author_id: None, body: "done".into(), queued: false, author_session_id: None, xref: None };
        let divider = SessionChatRow::Divider { id: "d1".into(), kind: DividerKind::Compact, at: "bad".into() };
        let out = rows(&[agent, divider], &HashMap::new(), |_| Some("never".into()));
        assert!(matches!(&out[0], ChatRowInput::Message { name: None, .. }));
        assert_eq!(out[1], ChatRowInput::Pill { id: "d1".into(), icon: DividerKind::Compact.icon().into(), label: DividerKind::Compact.label().into() });
    }

    #[test]
    fn a_question_waits_until_answered_or_dismissed_and_names_its_picks() {
        let q = |answered: bool, dismissed: Option<bool>, answers: Option<Vec<String>>| SessionChatRow::Question {
            id: "q".into(),
            session_id: "s".into(),
            question: "Ship?".into(),
            options: vec![],
            custom: true,
            multi: true,
            surface: None,
            answer: Some("Yes".into()),
            answers,
            surface_data: None,
            answered,
            dismissed,
        };
        let waiting = |row| matches!(rows(&[row], &HashMap::new(), |_| None)[0], ChatRowInput::Question { waiting: true, .. });
        assert!(waiting(q(false, None, None)));
        assert!(!waiting(q(true, None, None)));
        assert!(!waiting(q(false, Some(true), None)));
        let ChatRowInput::Question { answer, .. } = &rows(&[q(true, None, Some(vec!["A".into(), "B".into()]))], &HashMap::new(), |_| None)[0] else { panic!() };
        assert_eq!(answer.as_deref(), Some("A, B"));
    }

    /// #EDE-65 lane O: eDE's rows group like the IDE's: the viewer's own
    /// out, the agent's in under its avatar with its name on a group's first
    /// bubble, the in-group gap after it.
    #[test]
    fn the_rows_group_like_the_ide_transcript() {
        use crate::transcript_grouping::{SenderKind, TranscriptAgent, TranscriptChat, TranscriptInput};
        let msg = |id: &str, author: &str, author_id: Option<&str>| SessionChatRow::Message { id: id.into(), author: author.into(), author_id: author_id.map(str::to_string), body: id.into(), queued: false, author_session_id: None, xref: None };
        let chat = vec![msg("m1", "user", Some("u1")), msg("m2", "agent", None), msg("m3", "agent", None)];
        let mut out = rows(&chat, &HashMap::new(), |_| None);
        let transcript = TranscriptInput {
            viewer_id: Some("u1".into()),
            chat: TranscriptChat { session_id: "s1".into(), title: "Triage".into(), agent_id: Some("a1".into()), ..Default::default() },
            agents: vec![TranscriptAgent { id: "a1".into(), name: "Triage".into() }],
            ..Default::default()
        };
        group(&mut out, &chat, transcript, |s| (s.kind == SenderKind::Agent).then(|| format!("{AVATAR_ICON_PREFIX}ide:agent:{}", s.id)));
        let groups: Vec<_> = out.iter().map(|r| match r { ChatRowInput::Message { group, name, .. } => (group.clone().unwrap(), name.clone()), _ => panic!() }).collect();
        assert_eq!(groups[0].0.side.as_deref(), Some("out"));
        assert_eq!(groups[0].0.avatar, None);
        assert_eq!((groups[1].0.side.as_deref(), groups[1].0.avatar.as_deref(), groups[1].0.indent), (Some("in"), Some("avatar:ide:agent:a1"), true));
        assert_eq!(groups[1].1.as_deref(), Some("Triage"), "the name over the group's first bubble");
        assert_eq!((groups[2].0.avatar.as_deref(), groups[2].1.as_deref(), groups[2].0.gap), (None, None, crate::transcript_grouping::layout().gap_in_group as u32));
    }

    #[test]
    fn a_waiting_plan_needs_you_and_opens_its_session() {
        let plan = SessionChatRow::Plan {
            id: "p".into(),
            session_id: "s".into(),
            plan: "# Lazy-load the editor\n\nsteps".into(),
            questions: vec![],
            surface: None,
            decision: None,
            fresh: false,
            answers: None,
            feedback: None,
            surface_data: None,
        };
        let ChatRowInput::Card { title, text, target, .. } = &rows(&[plan], &HashMap::new(), |_| None)[0] else { panic!() };
        assert_eq!(title, &crate::plan_page::plan_row_label("# Lazy-load the editor\n\nsteps"));
        assert_eq!(text, &crate::plan_page::PLAN_PAGE.copy.needs_you);
        assert_eq!(target.as_deref(), Some("s"));
    }

    fn card_surface() -> CardSurface {
        CardSurface {
            version: Some("1.0".into()),
            root: serde_json::json!({"id": "r", "component": "Text", "props": {"text": {"@path": "/pick"}}}),
            data: Some(serde_json::json!({"pick": "none"})),
        }
    }

    #[test]
    fn a_show_card_carries_its_surface_with_its_own_data() {
        let row = SessionChatRow::Surface { id: "m1".into(), session_id: "s".into(), title: None, surface: card_surface() };
        let ChatRowInput::Card { title, surface, target, .. } = &rows(&[row], &HashMap::new(), |_| None)[0] else { panic!() };
        assert_eq!(title, &crate::plan_card::PLAN_CARD.copy.surface, "no title = the Preview word");
        assert_eq!(target.as_deref(), Some("s"));
        let surface = surface.as_ref().expect("a surface");
        assert_eq!((surface["version"].as_str(), &surface["root"]["component"], &surface["data"]["pick"]), (Some("1.0"), &serde_json::json!("Text"), &serde_json::json!("none")));
    }

    #[test]
    fn a_closed_card_s_surface_shows_what_the_person_answered() {
        let answered = Some(serde_json::json!({"pick": "debug"}));
        let plan = SessionChatRow::Plan {
            id: "p".into(),
            session_id: "s".into(),
            plan: "# Plan".into(),
            questions: vec![],
            surface: Some(card_surface()),
            decision: Some("approved".into()),
            fresh: false,
            answers: None,
            feedback: None,
            surface_data: answered.clone(),
        };
        let question = SessionChatRow::Question {
            id: "q".into(),
            session_id: "s".into(),
            question: "Which?".into(),
            options: vec![],
            custom: false,
            multi: false,
            surface: Some(card_surface()),
            answer: None,
            answers: None,
            surface_data: None,
            answered: false,
            dismissed: None,
        };
        let session = SessionChatRow::Report { id: "r".into(), session_id: "s".into(), text: "Report".into(), summary: String::new(), xref: crate::session_tree::SessionXref { kind: "report".into(), session_id: "s".into(), name: "Child".into(), agent_id: None } };
        let out = rows(&[plan, question, session], &HashMap::new(), |_| None);
        let ChatRowInput::Card { surface: Some(plan), .. } = &out[0] else { panic!("{out:?}") };
        assert_eq!(plan["data"], answered.unwrap(), "the answered model wins");
        let ChatRowInput::Question { surface: Some(question), .. } = &out[1] else { panic!("{out:?}") };
        assert_eq!(question["data"]["pick"], "none", "an open card shows its own data");
        assert!(matches!(&out[2], ChatRowInput::Card { surface: None, .. }), "a report has no surface");
    }

    #[test]
    fn a_show_card_s_reply_posts_its_text_and_a_plan_s_surface_has_no_transport() {
        let surface = SessionChatRow::Surface { id: "m1".into(), session_id: "s1".into(), title: None, surface: card_surface() };
        let plan = SessionChatRow::Plan {
            id: "p".into(),
            session_id: "s1".into(),
            plan: "# Plan".into(),
            questions: vec![],
            surface: Some(card_surface()),
            decision: None,
            fresh: false,
            answers: None,
            feedback: None,
            surface_data: None,
        };
        let rows = [surface, plan];
        let reply = serde_json::json!({"text": " Ship "});
        assert_eq!(card_reply(&rows, "m1", Some("reply"), Some(&reply)), Some(CardReply::Send { session_id: "s1".into(), text: "Ship".into() }));
        assert_eq!(card_reply(&rows, "m1", Some("other"), Some(&reply)), None, "only reply answers");
        assert_eq!(card_reply(&rows, "m1", Some("reply"), Some(&serde_json::json!({"text": " "}))), None, "a blank reply posts nothing");
        assert_eq!(card_reply(&rows, "p", Some("reply"), Some(&reply)), None, "a plan's surface stays in its card");
        assert_eq!(card_reply(&rows, "gone", Some("reply"), Some(&reply)), None);
    }
}
