//! The agent chat (#EDE-65 lane I, shared by lane O): one conversation as
//! the shared chat document draws it (eDE and the IDE) (`apps/ede/ui/chat.json`, the model
//! `uicore_model::chat`). The domain builds the chat (session-chat.json:
//! the rows, their order, the cards' facts); this resolves the words the
//! other fixtures own (a divider's label, a plan's status, the locked
//! pill) and the clock, so the model only decides how the chat is drawn.

use std::collections::HashMap;

use crate::rows::MessageRow;
use crate::session_tree::{CardSurface, SessionChatRow};
use uicore_model::chat::{ChatOption, ChatRowInput};

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
        SessionChatRow::Event { id, text, .. } => ChatRowInput::Line { id: id.clone(), icon: "ui-info".into(), text: text.clone() },
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
        .map(|c| serde_json::json!({"key": format!("command:{}", c.name), "command": format!("/{}", c.name), "description": c.description, "hint": c.arg_hint, "icon": null, "insert": crate::slash_menu::insertion(c.name, c.arg_hint), "prompt": ""}))
        .collect();
    let named: Vec<&crate::rows::PromptRow> = prompts.iter().filter(|p| p.name.as_deref().is_some_and(|n| !n.trim().is_empty())).collect();
    rows.extend(crate::slash_menu::filter_prompts(&named, |p| p.name.as_deref().unwrap_or_default(), query).into_iter().map(|p| {
        serde_json::json!({"key": format!("prompt:{}", p.id), "command": "", "description": p.name, "hint": "", "icon": p.icon.clone().filter(|i| !i.is_empty()).unwrap_or_else(|| PROMPT_ICON.into()), "insert": "", "prompt": p.id})
    }));
    serde_json::json!({"open": !rows.is_empty(), "rows": rows})
}

/// A person `@` can name: their label and email.
pub struct Member {
    pub name: String,
    pub email: String,
}

/// THE `@`/`#` typeahead (`crate::typeahead`, the IDE's rules) over the
/// token at the END of the draft (the chat's composer reports its text, not
/// its caret: the token being typed is the last one). `@` = the members by
/// name or email, inserting `@<email>`; `#` = the issues the engine ranks
/// (`issues`: the caller's already ranked `(identifier, title)`), inserting
/// `#<IDENTIFIER>`. Rows have the `/` menu's shape; `insert` is the whole
/// new draft (the token replaced, a space after it).
pub fn mention_menu(draft: &str, members: &[Member], issues: &[(String, String)]) -> Option<serde_json::Value> {
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
                .map(|m| serde_json::json!({"key": format!("member:{}", m.email), "command": "", "description": m.name, "hint": m.email, "icon": "user", "insert": format!("{head}@{} ", m.email), "prompt": ""}))
                .collect()
        }
        CompletionTrigger::IssueRef => issues
            .iter()
            .take(MAX_ITEMS)
            .map(|(identifier, title)| serde_json::json!({"key": format!("issue:{identifier}"), "command": identifier, "description": title, "hint": "", "icon": null, "insert": format!("{head}#{identifier} "), "prompt": ""}))
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
                    serde_json::json!({"key": format!("emoji:{code}"), "command": "", "description": format!("{} :{code}:", emoji.unicode), "hint": emoji.label, "icon": null, "insert": format!("{head}{}", emoji.unicode), "prompt": ""})
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

/// A prompt without a curated icon (the IDE's generic prompt glyph).
const PROMPT_ICON: &str = "action-default";

#[cfg(test)]
mod tests {
    use super::*;

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
        let members = vec![Member { name: "Dana Reyes".into(), email: "dana@x.com".into() }, Member { name: "Alex".into(), email: "alex@x.com".into() }];
        let menu = mention_menu("ping @da", &members, &[]).expect("@");
        assert_eq!(menu["rows"].as_array().unwrap().len(), 1);
        assert_eq!(menu["rows"][0]["insert"], "ping @dana@x.com ");
        let issues = vec![("EXP-12".to_string(), "Fix the tray".to_string())];
        let hash = mention_menu("see #EX", &members, &issues).expect("#");
        assert_eq!((hash["rows"][0]["command"].as_str(), hash["rows"][0]["insert"].as_str()), (Some("EXP-12"), Some("see #EXP-12 ")));
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
}
