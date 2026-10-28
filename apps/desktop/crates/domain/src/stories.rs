//! S-3 STORIES ×4 (`packages/domain-contract/src/stories.ts`'s twin, iOS
//! ExpCore `Stories.swift`, Android `domain/Stories.kt`): a session's own
//! show cards (a plan, a question, a result) folded into one ring per agent
//! and per chat, for the story ring + viewer. Byte-locked by
//! `fixtures/stories.json` (its `$comment` is the spec); pure.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ask_dock::ask_dock_title;

const STORIES_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/stories.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoriesConstants {
    pub seen_hours: i64,
    pub unseen_hours: i64,
    pub auto_advance_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoriesCopy {
    pub plan: String,
    pub question: String,
    pub result: String,
    pub now: String,
    pub approve: String,
    pub change: String,
    pub reply: String,
    pub close: String,
    pub next: String,
    pub previous: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct StoryRingSpec {
    pub width: f32,
    pub gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct StoryProgressSpec {
    pub height: f32,
    pub gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct StoryViewerSpec {
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoriesTestIds {
    pub ring: String,
    pub viewer: String,
    pub progress: String,
    pub header: String,
    pub next: String,
    pub previous: String,
}

/// `fixtures/stories.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoriesSpec {
    pub constants: StoriesConstants,
    pub copy: StoriesCopy,
    pub ring: StoryRingSpec,
    pub progress: StoryProgressSpec,
    pub viewer: StoryViewerSpec,
    pub test_ids: StoriesTestIds,
}

pub static STORIES: LazyLock<StoriesSpec> =
    LazyLock::new(|| serde_json::from_str(STORIES_JSON).expect("stories.json parses"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryKind {
    Plan,
    Question,
    Result,
}

impl StoryKind {
    /// The copy word (`Plan` / `Question` / `Result`).
    pub fn label(self) -> &'static str {
        match self {
            StoryKind::Plan => &STORIES.copy.plan,
            StoryKind::Question => &STORIES.copy.question,
            StoryKind::Result => &STORIES.copy.result,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryFooter {
    Plan,
    Choices,
    Reply,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorySessionInput {
    pub id: String,
    pub root_id: String,
    /// The session's display name (session-title).
    pub name: String,
    pub agent_id: Option<String>,
    pub started_by_agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoryAgentInput {
    pub id: String,
    pub name: String,
    pub assistant: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryMessageInput {
    pub id: String,
    pub session_id: String,
    pub author_session_id: Option<String>,
    /// Epoch ms.
    pub created_at: i64,
    pub answered_at: Option<i64>,
    /// The raw `messages.card`.
    pub card: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryReadInput {
    /// The root session of the read mark.
    pub session_id: String,
    pub read_at: Option<i64>,
    pub story_seen_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StoriesInput {
    pub now: i64,
    pub sessions: Vec<StorySessionInput>,
    pub agents: Vec<StoryAgentInput>,
    pub messages: Vec<StoryMessageInput>,
    pub reads: Vec<StoryReadInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryPicture {
    pub label: String,
    pub attachment_id: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    pub id: String,
    pub session_id: String,
    pub root_id: String,
    pub session_name: String,
    pub kind: StoryKind,
    pub waiting: bool,
    pub seen: bool,
    pub created_at: i64,
    pub meta: String,
    pub title: Option<String>,
    pub pictures: Vec<StoryPicture>,
    pub surface: bool,
    pub text: Option<String>,
    pub footer: StoryFooter,
    pub auto_advance_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryRing {
    pub key: String,
    pub agent_id: Option<String>,
    pub root_id: Option<String>,
    pub title: String,
    pub assistant: bool,
    pub waiting: usize,
    pub unseen: bool,
    pub stories: Vec<Story>,
}

fn s(card: &Value, key: &str) -> Option<String> {
    card.get(key).and_then(Value::as_str).map(str::to_string)
}

fn number(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_f64().map(|v| v as i64))
}

fn resolved_pictures(card: &Value) -> Vec<StoryPicture> {
    let Some(list) = card.get("pictures").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter(|raw| raw.is_object())
        .filter_map(|picture| {
            let attachment_id = s(picture, "attachmentId").filter(|id| !id.is_empty())?;
            Some(StoryPicture {
                label: s(picture, "label").unwrap_or_default(),
                attachment_id,
                width: picture.get("width").and_then(number),
                height: picture.get("height").and_then(number),
                caption: s(picture, "caption"),
            })
        })
        .collect()
}

/// The card's story flag: explicit, else ask/plan or a resolved picture.
pub fn is_story_card(card: &Value) -> bool {
    if !card.is_object() {
        return false;
    }
    let kind = card.get("kind").and_then(Value::as_str);
    if !matches!(kind, Some("plan" | "question" | "surface")) {
        return false;
    }
    if s(card, "questionId").is_some_and(|id| !id.is_empty()) {
        return false;
    }
    if let Some(story) = card.get("story").and_then(Value::as_bool) {
        return story;
    }
    kind != Some("surface") || !resolved_pictures(card).is_empty()
}

/// `<m>m` / `<h>h` / `<d>d`, floored; under a minute = `now`.
pub fn story_age(created_at: i64, now: i64) -> String {
    let minutes = (now - created_at).max(0) / 60_000;
    if minutes < 1 {
        return STORIES.copy.now.clone();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

const HOUR: i64 = 3_600_000;

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// THE fold: the viewer's story rings, in display order.
pub fn stories(input: &StoriesInput) -> Vec<StoryRing> {
    let sessions: HashMap<&str, &StorySessionInput> = input.sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let agents: HashMap<&str, &StoryAgentInput> = input.agents.iter().map(|a| (a.id.as_str(), a)).collect();
    let mut marks: HashMap<&str, Option<i64>> = HashMap::new();
    for read in &input.reads {
        marks.insert(read.session_id.as_str(), read.read_at.max(read.story_seen_at));
    }
    let mut rings: Vec<StoryRing> = Vec::new();

    for message in &input.messages {
        if message.author_session_id.as_deref() != Some(message.session_id.as_str()) {
            continue;
        }
        let card = &message.card;
        if !is_story_card(card) {
            continue;
        }
        let Some(session) = sessions.get(message.session_id.as_str()) else { continue };
        let Some(root) = sessions.get(session.root_id.as_str()) else { continue };
        let kind = match card.get("kind").and_then(Value::as_str) {
            Some("plan") => StoryKind::Plan,
            Some("question") => StoryKind::Question,
            _ => StoryKind::Result,
        };
        let present = |key: &str| card.get(key).is_some_and(|v| !v.is_null());
        let waiting = match kind {
            StoryKind::Plan => !present("decision"),
            StoryKind::Question => message.answered_at.is_none() && !present("answer"),
            StoryKind::Result => false,
        };
        if kind != StoryKind::Result && !waiting {
            continue;
        }
        let seen = marks
            .get(root.id.as_str())
            .copied()
            .flatten()
            .is_some_and(|mark| message.created_at <= mark);
        let age = input.now - message.created_at;
        if kind == StoryKind::Result {
            let keep = if seen { STORIES.constants.seen_hours } else { STORIES.constants.unseen_hours };
            if age >= keep * HOUR {
                continue;
            }
        }
        let pictures = resolved_pictures(card);
        let surface = card.get("surface").is_some_and(Value::is_object);
        let options = card.get("options").and_then(Value::as_array).map_or(0, Vec::len);
        let title = match kind {
            StoryKind::Result => non_empty(s(card, "title")),
            StoryKind::Question => non_empty(s(card, "question")),
            StoryKind::Plan => Some(ask_dock_title(&s(card, "plan").unwrap_or_default())).filter(|t| !t.is_empty()),
        };
        let story = Story {
            id: message.id.clone(),
            session_id: session.id.clone(),
            root_id: root.id.clone(),
            session_name: session.name.clone(),
            kind,
            waiting,
            seen,
            created_at: message.created_at,
            meta: format!("{} · {}", kind.label(), story_age(message.created_at, input.now)),
            title,
            text: if kind == StoryKind::Plan && pictures.is_empty() && !surface { s(card, "plan") } else { None },
            pictures,
            surface,
            footer: match kind {
                StoryKind::Plan => StoryFooter::Plan,
                StoryKind::Question if options > 0 => StoryFooter::Choices,
                StoryKind::Question => StoryFooter::Reply,
                StoryKind::Result => StoryFooter::None,
            },
            auto_advance_ms: (kind == StoryKind::Result).then_some(STORIES.constants.auto_advance_ms),
        };
        let agent_id = root.agent_id.clone().or_else(|| root.started_by_agent_id.clone());
        let agent = agent_id.as_deref().and_then(|id| agents.get(id));
        let key = match &agent_id {
            Some(id) => format!("agent:{id}"),
            None => format!("root:{}", root.id),
        };
        let index = match rings.iter().position(|ring| ring.key == key) {
            Some(index) => index,
            None => {
                rings.push(StoryRing {
                    key,
                    agent_id: agent_id.clone(),
                    root_id: if agent_id.is_some() { None } else { Some(root.id.clone()) },
                    title: match (&agent_id, agent) {
                        (Some(_), Some(agent)) => agent.name.clone(),
                        _ => root.name.clone(),
                    },
                    assistant: agent.is_some_and(|agent| agent.assistant),
                    waiting: 0,
                    unseen: false,
                    stories: Vec::new(),
                });
                rings.len() - 1
            }
        };
        rings[index].stories.push(story);
    }

    let by_created = |a: &Story, b: &Story| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id));
    for ring in &mut rings {
        let all = std::mem::take(&mut ring.stories);
        let (mut waiting, rest): (Vec<_>, Vec<_>) = all.into_iter().partition(|story| story.waiting);
        let (mut unseen, mut seen): (Vec<_>, Vec<_>) = rest.into_iter().partition(|story| !story.seen);
        waiting.sort_by(by_created);
        unseen.sort_by(by_created);
        seen.sort_by(by_created);
        ring.waiting = waiting.len();
        ring.stories = waiting.into_iter().chain(unseen).chain(seen).collect();
        ring.unseen = ring.stories.iter().any(|story| story.waiting || !story.seen);
    }
    let rank = |ring: &StoryRing| {
        if ring.assistant {
            0
        } else if ring.waiting > 0 {
            1
        } else if ring.unseen {
            2
        } else {
            3
        }
    };
    let newest = |ring: &StoryRing| ring.stories.iter().map(|story| story.created_at).max().unwrap_or(i64::MIN);
    rings.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| newest(b).cmp(&newest(a)))
            .then_with(|| a.key.cmp(&b.key))
    });
    rings
}

/// The viewer's place: (ring index, story index).
pub type StoryPosition = (usize, usize);

/// NAVIGATION: the next story (past the last of a ring = the next ring's
/// first; past the last ring = `None`, close).
pub fn story_next(rings: &[StoryRing], (ring, story): StoryPosition) -> Option<StoryPosition> {
    let current = rings.get(ring)?;
    if story + 1 < current.stories.len() {
        return Some((ring, story + 1));
    }
    (ring + 1..rings.len()).find(|&next| !rings[next].stories.is_empty()).map(|next| (next, 0))
}

/// NAVIGATION: the previous story (the first of a ring = the previous ring's
/// last; the very first stays put).
pub fn story_previous(rings: &[StoryRing], (ring, story): StoryPosition) -> StoryPosition {
    if story > 0 {
        return (ring, story - 1);
    }
    (0..ring)
        .rev()
        .find(|&prev| !rings[prev].stories.is_empty())
        .map(|prev| (prev, rings[prev].stories.len() - 1))
        .unwrap_or((ring, story))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_parses() {
        assert_eq!(STORIES.constants.auto_advance_ms, 6000);
        assert_eq!(STORIES.viewer.width, 440.0);
        assert_eq!(STORIES.test_ids.ring, "story-ring");
    }

    #[test]
    fn stories_match_the_fixture() {
        let fixture: Value = serde_json::from_str(STORIES_JSON).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input: StoriesInput = serde_json::from_value(case["input"].clone()).unwrap();
            let actual = serde_json::to_value(stories(&input)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn navigation_crosses_rings() {
        let fixture: Value = serde_json::from_str(STORIES_JSON).unwrap();
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["expected"].as_array().unwrap().len() > 2)
            .unwrap();
        let input: StoriesInput = serde_json::from_value(case["input"].clone()).unwrap();
        let rings = stories(&input);
        assert_eq!(story_next(&rings, (0, 0)), Some((1, 0)));
        assert_eq!(story_previous(&rings, (1, 0)), (0, 0));
        assert_eq!(story_previous(&rings, (0, 0)), (0, 0));
        assert_eq!(story_next(&rings, (rings.len() - 1, 0)), None);
    }
}
