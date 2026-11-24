//! #EDE-99 GLOBAL SEARCH FINDS CHATS (`packages/domain-contract/src/search-list.ts`'s
//! twin; the IDE search sheet and the eDE start menu share it): which
//! sections show, the Chats rows over the chat store's `search_all`, what a
//! pick opens and where find in chat starts. Byte-locked by
//! `fixtures/search-list.json` (its `$comment` tail from "CHATS (#EDE-99" is
//! the spec); pure.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::chats::chat_time_label;
use crate::find_in_chat::{self as find, FindStop};

const SEARCH_LIST_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/search-list.json");

/// The fixture's `chats.copy`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchListChatsCopy {
    pub issues: String,
    pub chats: String,
    pub you: String,
    pub no_results: String,
    pub placeholder: String,
}

/// The fixture's `chats.icons` (icon CONCEPTS).
#[derive(Debug, Deserialize)]
pub struct SearchListChatsIcons {
    pub chat: String,
    pub agent: String,
}

/// The fixture's `chats.row`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchListChatsRow {
    pub chat_height: f32,
    pub chat_height_phone: f32,
}

/// The fixture's `chats.testIds`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchListChatsTestIds {
    pub issues: String,
    pub chats: String,
    pub chat_row: String,
}

/// The fixture's `chats`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchListChats {
    pub debounce_ms: u64,
    pub chats_max: usize,
    pub row: SearchListChatsRow,
    pub icons: SearchListChatsIcons,
    pub copy: SearchListChatsCopy,
    pub test_ids: SearchListChatsTestIds,
}

/// The fixture's `suggestion` (#NEW-133): the action row a recognised
/// sentence puts on top of the global search.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchListSuggestionSpec {
    /// The row's icon CONCEPT.
    pub icon: String,
    /// The row's name, `{query}` = the translated query.
    pub label: String,
    pub test_id: String,
    /// #NEW-136: the same row offering the model fallback.
    pub translate: SearchListTranslateSpec,
}

/// The fixture's `suggestion.translate` (#NEW-136): the row's name, the
/// failure toast, its test id and the platforms that show it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchListTranslateSpec {
    pub label: String,
    pub failed: String,
    pub test_id: String,
    pub platforms: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    chats: SearchListChats,
    suggestion: SearchListSuggestionSpec,
}

static FIXTURE: LazyLock<Fixture> =
    LazyLock::new(|| serde_json::from_str(SEARCH_LIST_JSON).expect("search-list.json parses"));

/// The fixture's `chats` block (copy, icons, limits).
pub fn chats() -> &'static SearchListChats {
    &FIXTURE.chats
}

pub fn copy() -> &'static SearchListChatsCopy {
    &FIXTURE.chats.copy
}

/// The fixture's `suggestion` block (icon concept, label format, test id).
pub fn suggestion_spec() -> &'static SearchListSuggestionSpec {
    &FIXTURE.suggestion
}

/// #NEW-133: the global search's suggestion row: the query a recognised
/// sentence means and the row's name (`Search: <query>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchSuggestion {
    pub query: String,
    pub label: String,
}

/// The suggestion row for the typed `text` (`suggestion.cases`), or `None`
/// when the phrase grammar does not recognise a sentence. `now` = epoch ms.
pub fn suggestion(
    text: &str,
    members: &[crate::issue_search_phrase::PhraseMember<'_>],
    now: i64,
) -> Option<SearchSuggestion> {
    let query = crate::issue_search_phrase::translate(text, members, now)?;
    let label = FIXTURE.suggestion.label.replace("{query}", &query);
    Some(SearchSuggestion { query, label })
}

/// Which section a global list's section is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalSectionKey {
    Issues,
    Chats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalSection {
    pub key: GlobalSectionKey,
    pub count: usize,
    pub loading: bool,
}

/// The Chats side of [`global_sections`]' input (`None` = no store here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct GlobalChatsState {
    pub rows: usize,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlobalSections {
    /// Section headers show (else today's flat list).
    pub titled: bool,
    pub sections: Vec<GlobalSection>,
    /// The input's trailing count; `None` on a blank query.
    pub count: Option<usize>,
    /// The empty state (`copy.noResults`) shows.
    pub empty: bool,
}

/// THE sections of the global list (`sectionCases`).
pub fn global_sections(query: &str, issues: usize, chats: Option<GlobalChatsState>) -> GlobalSections {
    let blank = find::query(query).is_empty();
    let Some(chats) = chats.filter(|_| !blank) else {
        return GlobalSections {
            titled: false,
            sections: vec![GlobalSection { key: GlobalSectionKey::Issues, count: issues, loading: false }],
            count: (!blank).then_some(issues),
            empty: false,
        };
    };
    let mut sections = Vec::new();
    if issues > 0 {
        sections.push(GlobalSection { key: GlobalSectionKey::Issues, count: issues, loading: false });
    }
    if chats.rows > 0 {
        sections.push(GlobalSection { key: GlobalSectionKey::Chats, count: chats.rows, loading: chats.loading });
    }
    let count = issues + chats.rows;
    GlobalSections { titled: true, sections, count: Some(count), empty: count == 0 && !chats.loading }
}

/// One hit of the chat store (`chat_store::Hit`'s shape; `kind` = its
/// snake_case `DocKind`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalChatHit {
    pub root_id: String,
    pub session_id: String,
    #[serde(default)]
    pub device_id: Option<String>,
    pub kind: String,
    pub at: i64,
    #[serde(default)]
    pub seq: Option<u64>,
    #[serde(default)]
    pub message_id: Option<String>,
    pub snippet: String,
    #[serde(default)]
    pub match_start: usize,
    #[serde(default)]
    pub match_len: usize,
    /// #FEED-300 (chat-store.json SYSTEM INPUTS): not a person's own words.
    #[serde(default)]
    pub system: bool,
}

/// One chat of the store's `search_all` answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalChat {
    pub root_id: String,
    pub count: usize,
    pub hit: GlobalChatHit,
}

/// A synced root of the active team I can see.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GlobalChatRoot {
    pub title: String,
    pub agent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalChatRow {
    pub key: String,
    pub root_id: String,
    /// The icon CONCEPT.
    pub icon: String,
    pub title: String,
    pub subtitle: String,
    /// The query the subtitle's genUI highlight is bound to.
    pub highlight: String,
    pub trailing: String,
}

pub struct GlobalChatRowsInput<'a> {
    pub query: &'a str,
    /// ISO now.
    pub now: &'a str,
    pub utc_offset_minutes: i32,
    pub chats: &'a [GlobalChat],
    pub roots: &'a HashMap<String, GlobalChatRoot>,
    /// Authors (`user` | `agent`) of synced messages by id.
    pub message_authors: &'a HashMap<String, String>,
    pub limit: usize,
}

fn iso_of_ms(at: i64) -> String {
    chrono::DateTime::from_timestamp_millis(at)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// THE Chats rows (`rowCases`): the store's order, roots of the active team
/// only, at most `limit`.
pub fn global_chat_rows(input: GlobalChatRowsInput<'_>) -> Vec<GlobalChatRow> {
    let fixture = chats();
    let highlight = find::query(input.query);
    let mut rows = Vec::new();
    for chat in input.chats {
        if rows.len() >= input.limit {
            break;
        }
        let Some(root) = input.roots.get(&chat.root_id) else { continue };
        let hit = &chat.hit;
        // #FEED-300: a system input (a schedule tick, a relay, a teammate's
        // line) is never the person's own words.
        let person = !hit.system
            && (hit.kind == "user_message"
                || (hit.kind == "message"
                    && hit
                        .message_id
                        .as_deref()
                        .filter(|id| !id.is_empty())
                        .and_then(|id| input.message_authors.get(id))
                        .is_some_and(|author| author == "user")));
        rows.push(GlobalChatRow {
            key: format!("chat:{}", chat.root_id),
            root_id: chat.root_id.clone(),
            icon: if root.agent { fixture.icons.agent.clone() } else { fixture.icons.chat.clone() },
            title: root.title.clone(),
            subtitle: format!("{}{}", if person { fixture.copy.you.as_str() } else { "" }, hit.snippet),
            highlight: highlight.clone(),
            trailing: chat_time_label(&iso_of_ms(hit.at), input.now, input.utc_offset_minutes),
        });
    }
    rows
}

/// Find in chat's start target of a pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalChatTarget {
    pub key: String,
    pub output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlobalChatFind {
    pub query: String,
    pub target: GlobalChatTarget,
}

/// What a pick opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalChatPick {
    pub session_id: String,
    pub find: GlobalChatFind,
}

/// A PICK (`pickCases`): the hit's session with find in chat open on the
/// query, its start = the hit's stop.
pub fn global_chat_pick(query: &str, hit: &GlobalChatHit) -> GlobalChatPick {
    let key = if hit.kind == "message" {
        find::message_key(hit.message_id.as_deref().unwrap_or_default())
    } else {
        find::event_key(&hit.session_id, hit.seq.unwrap_or(0) as i64)
    };
    GlobalChatPick {
        session_id: hit.session_id.clone(),
        find: GlobalChatFind { query: find::query(query), target: GlobalChatTarget { key, output: hit.kind == "tool_output" } },
    }
}

/// Find in chat's start for a pick (`startCases`): the target's first stop,
/// else the newest; none while no stops.
pub fn global_chat_start(stops: &[FindStop], target: &GlobalChatTarget) -> Option<usize> {
    if stops.is_empty() {
        return None;
    }
    Some(stops.iter().position(|stop| stop.key == target.key && stop.output == target.output).unwrap_or(stops.len() - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn fixture() -> Value {
        serde_json::from_str(SEARCH_LIST_JSON).unwrap()
    }

    fn cases(name: &str) -> Vec<Value> {
        let cases = fixture()[name].as_array().unwrap().clone();
        assert!(!cases.is_empty(), "{name}");
        cases
    }

    #[test]
    fn suggestion_cases() {
        let cases = fixture()["suggestion"]["cases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let members: Vec<(Option<String>, Option<String>)> = case["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| (m["name"].as_str().map(str::to_string), m["email"].as_str().map(str::to_string)))
                .collect();
            let members: Vec<crate::issue_search_phrase::PhraseMember<'_>> = members
                .iter()
                .map(|(name, email)| crate::issue_search_phrase::PhraseMember {
                    name: name.as_deref(),
                    email: email.as_deref(),
                })
                .collect();
            let now = crate::issue_search::parse_instant(case["now"].as_str().unwrap()).unwrap();
            let got = suggestion(case["text"].as_str().unwrap(), &members, now);
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn section_cases() {
        for case in cases("sectionCases") {
            let input = &case["input"];
            let chats: Option<GlobalChatsState> = serde_json::from_value(input["chats"].clone()).unwrap();
            let got = global_sections(input["query"].as_str().unwrap(), input["issues"].as_u64().unwrap() as usize, chats);
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn row_cases() {
        for case in cases("rowCases") {
            let input = &case["input"];
            let chats: Vec<GlobalChat> = serde_json::from_value(input["chats"].clone()).unwrap();
            let roots: HashMap<String, GlobalChatRoot> = serde_json::from_value(input["roots"].clone()).unwrap();
            let authors: HashMap<String, String> = serde_json::from_value(input["messageAuthors"].clone()).unwrap();
            let got = global_chat_rows(GlobalChatRowsInput {
                query: input["query"].as_str().unwrap(),
                now: input["now"].as_str().unwrap(),
                utc_offset_minutes: input["utcOffsetMinutes"].as_i64().unwrap() as i32,
                chats: &chats,
                roots: &roots,
                message_authors: &authors,
                limit: input["limit"].as_u64().unwrap() as usize,
            });
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn pick_cases() {
        for case in cases("pickCases") {
            let input = &case["input"];
            let hit: GlobalChatHit = serde_json::from_value(input["hit"].clone()).unwrap();
            let got = global_chat_pick(input["query"].as_str().unwrap(), &hit);
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn start_cases() {
        for case in cases("startCases") {
            let stops: Vec<FindStop> = serde_json::from_value(case["stops"].clone()).unwrap();
            let target: GlobalChatTarget = serde_json::from_value(case["target"].clone()).unwrap();
            let got = global_chat_start(&stops, &target);
            assert_eq!(json!(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn copy_reads_the_fixture() {
        assert_eq!(copy().placeholder, "Search issues and chats");
        assert_eq!(copy().no_results, "No results");
        assert_eq!(chats().chats_max, 20);
        assert_eq!(chats().debounce_ms, 150);
        assert_eq!(chats().icons.chat, "search-chat");
    }
}
