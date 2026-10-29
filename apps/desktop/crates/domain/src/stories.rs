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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoryTabCopy {
    pub label: String,
    pub empty: String,
    pub approved: String,
    pub changed: String,
    pub answered: String,
    pub superseded: String,
    pub changes: String,
    pub stack: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoryTabTestIds {
    pub tab: String,
    pub list: String,
    pub tile: String,
    pub changes: String,
    pub stack: String,
}

/// A per-form-factor length (`wide` = web md+ and the IDE, `phone`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct FormFactorLength {
    pub wide: f32,
    pub phone: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryTileSpec {
    pub thumb_height: FormFactorLength,
    pub thumb_max_width: FormFactorLength,
    pub gap: f32,
    pub radius: f32,
    pub list_gap: f32,
    pub lead_gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct StoryTabSpecimen {
    pub case: usize,
}

/// The `tab` block: the session screen's Stories tab.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryTabSpec {
    pub copy: StoryTabCopy,
    pub test_ids: StoryTabTestIds,
    pub tile: StoryTileSpec,
    pub specimen: StoryTabSpecimen,
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
    pub tab: StoryTabSpec,
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

/// How a plan/question was settled (`None` while waiting, and for results).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoryResolution {
    Approved,
    Changed,
    Answered,
    Superseded,
}

impl StoryResolution {
    fn label(self) -> &'static str {
        match self {
            StoryResolution::Approved => &STORIES.tab.copy.approved,
            StoryResolution::Changed => &STORIES.tab.copy.changed,
            StoryResolution::Answered => &STORIES.tab.copy.answered,
            StoryResolution::Superseded => &STORIES.tab.copy.superseded,
        }
    }
}

type Marks<'a> = HashMap<&'a str, Option<i64>>;

fn marks_of(reads: &[StoryReadInput]) -> Marks<'_> {
    reads.iter().map(|read| (read.session_id.as_str(), read.read_at.max(read.story_seen_at))).collect()
}

/// One message as a story (`None` = not a story of a known root), with its
/// root and how it was settled.
fn story_of<'a>(
    message: &StoryMessageInput,
    sessions: &HashMap<&str, &'a StorySessionInput>,
    marks: &Marks<'_>,
    now: i64,
) -> Option<(Story, &'a StorySessionInput, Option<StoryResolution>)> {
    if message.author_session_id.as_deref() != Some(message.session_id.as_str()) {
        return None;
    }
    let card = &message.card;
    if !is_story_card(card) {
        return None;
    }
    let session = *sessions.get(message.session_id.as_str())?;
    let root = *sessions.get(session.root_id.as_str())?;
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
    let resolution = match kind {
        _ if waiting => None,
        StoryKind::Result => None,
        StoryKind::Question => Some(StoryResolution::Answered),
        StoryKind::Plan => Some(match card.get("decision").and_then(Value::as_str) {
            Some("approved") => StoryResolution::Approved,
            Some("superseded") => StoryResolution::Superseded,
            _ => StoryResolution::Changed,
        }),
    };
    // A settled plan/question was acted on: seen.
    let seen = resolution.is_some()
        || marks
            .get(root.id.as_str())
            .copied()
            .flatten()
            .is_some_and(|mark| message.created_at <= mark);
    let pictures = resolved_pictures(card);
    let surface = card.get("surface").is_some_and(Value::is_object);
    let options = card.get("options").and_then(Value::as_array).map_or(0, Vec::len);
    let title = match kind {
        StoryKind::Result => non_empty(s(card, "title")),
        StoryKind::Question => non_empty(s(card, "question")),
        StoryKind::Plan => Some(ask_dock_title(&s(card, "plan").unwrap_or_default())).filter(|t| !t.is_empty()),
    };
    let age = story_age(message.created_at, now);
    let story = Story {
        id: message.id.clone(),
        session_id: session.id.clone(),
        root_id: root.id.clone(),
        session_name: session.name.clone(),
        kind,
        waiting,
        seen,
        created_at: message.created_at,
        meta: match resolution {
            Some(resolution) if resolution != StoryResolution::Superseded => {
                format!("{} · {} · {age}", kind.label(), resolution.label())
            }
            _ => format!("{} · {age}", kind.label()),
        },
        title,
        text: if kind == StoryKind::Plan && pictures.is_empty() && !surface { s(card, "plan") } else { None },
        pictures,
        surface,
        footer: match kind {
            _ if resolution.is_some() => StoryFooter::None,
            StoryKind::Plan => StoryFooter::Plan,
            StoryKind::Question if options > 0 => StoryFooter::Choices,
            StoryKind::Question => StoryFooter::Reply,
            StoryKind::Result => StoryFooter::None,
        },
        auto_advance_ms: (kind == StoryKind::Result).then_some(STORIES.constants.auto_advance_ms),
    };
    Some((story, root, resolution))
}

/// The ring a root's stories ride (an agent's, else the chat's), empty.
fn empty_ring(root: &StorySessionInput, agents: &HashMap<&str, &StoryAgentInput>) -> StoryRing {
    let agent_id = root.agent_id.clone().or_else(|| root.started_by_agent_id.clone());
    let agent = agent_id.as_deref().and_then(|id| agents.get(id));
    StoryRing {
        key: match &agent_id {
            Some(id) => format!("agent:{id}"),
            None => format!("root:{}", root.id),
        },
        root_id: if agent_id.is_some() { None } else { Some(root.id.clone()) },
        title: match (&agent_id, agent) {
            (Some(_), Some(agent)) => agent.name.clone(),
            _ => root.name.clone(),
        },
        assistant: agent.is_some_and(|agent| agent.assistant),
        agent_id,
        waiting: 0,
        unseen: false,
        stories: Vec::new(),
    }
}

fn by_created(a: &Story, b: &Story) -> std::cmp::Ordering {
    a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id))
}

/// THE fold: the viewer's story rings, in display order.
pub fn stories(input: &StoriesInput) -> Vec<StoryRing> {
    let sessions: HashMap<&str, &StorySessionInput> = input.sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let agents: HashMap<&str, &StoryAgentInput> = input.agents.iter().map(|a| (a.id.as_str(), a)).collect();
    let marks = marks_of(&input.reads);
    let mut rings: Vec<StoryRing> = Vec::new();

    for message in &input.messages {
        // A settled plan/question leaves the ring (the Stories tab keeps it).
        let Some((story, root, None)) = story_of(message, &sessions, &marks, input.now) else { continue };
        if story.kind == StoryKind::Result {
            let keep = if story.seen { STORIES.constants.seen_hours } else { STORIES.constants.unseen_hours };
            if input.now - story.created_at >= keep * HOUR {
                continue;
            }
        }
        let ring = empty_ring(root, &agents);
        match rings.iter_mut().find(|existing| existing.key == ring.key) {
            Some(existing) => existing.stories.push(story),
            None => rings.push(StoryRing { stories: vec![story], ..ring }),
        }
    }

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

/// THE Stories tab of a session screen (stories.json `tab`, web
/// `storyTab`): every story the root's tree ever posted, never expiring;
/// settled plans/questions stay (footer none, `meta` names the outcome), a
/// superseded plan drops. Waiting first (oldest first), then the rest newest
/// first. ONE ring (the root's, as `stories` keys it) so the viewer opens on
/// it; `None` = the empty state. `unseen` = the tab chip's dot.
pub fn story_tab(input: &StoriesInput, root_id: &str) -> Option<StoryRing> {
    let sessions: HashMap<&str, &StorySessionInput> = input.sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let agents: HashMap<&str, &StoryAgentInput> = input.agents.iter().map(|a| (a.id.as_str(), a)).collect();
    let root = *sessions.get(root_id)?;
    if root.root_id != root.id {
        return None;
    }
    let marks = marks_of(&input.reads);
    let list: Vec<Story> = input
        .messages
        .iter()
        .filter_map(|message| story_of(message, &sessions, &marks, input.now))
        .filter(|(_, story_root, resolution)| {
            story_root.id == root.id && *resolution != Some(StoryResolution::Superseded)
        })
        .map(|(story, _, _)| story)
        .collect();
    if list.is_empty() {
        return None;
    }
    let (mut waiting, mut rest): (Vec<_>, Vec<_>) = list.into_iter().partition(|story| story.waiting);
    waiting.sort_by(by_created);
    rest.sort_by(|a, b| by_created(b, a));
    let unseen = waiting.iter().chain(&rest).any(|story| story.waiting || !story.seen);
    Some(StoryRing {
        waiting: waiting.len(),
        unseen,
        stories: waiting.into_iter().chain(rest).collect(),
        ..empty_ring(root, &agents)
    })
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

/// The chats header row (chats.json BUBBLES + STORY RING): each agent
/// bubble wears its agent's ring (`by_agent`, keyed by agent id, the first
/// wins); `rest` = the rings no bubble wears (the root rings, an agent ring
/// with no bubble), in ring order, painted after the bubbles in the same row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BubbleRings {
    pub by_agent: HashMap<String, StoryRing>,
    pub rest: Vec<StoryRing>,
}

/// web `bubbleRings`.
pub fn bubble_rings(bubble_agent_ids: &[&str], rings: &[StoryRing]) -> BubbleRings {
    let mut out = BubbleRings::default();
    for ring in rings {
        match ring.agent_id.as_deref() {
            Some(agent) if bubble_agent_ids.contains(&agent) && !out.by_agent.contains_key(agent) => {
                out.by_agent.insert(agent.to_string(), ring.clone());
            }
            _ => out.rest.push(ring.clone()),
        }
    }
    out
}

/// THE styleguide specimen of the Stories tab ×4 (`tab.specimen`): the
/// expected ring of `tabCases[tab.specimen.case]`, folded from its input.
pub fn story_tab_specimen() -> StoryRing {
    let fixture: Value = serde_json::from_str(STORIES_JSON).expect("stories.json parses");
    let case = &fixture["tabCases"][STORIES.tab.specimen.case];
    let input: StoriesInput = serde_json::from_value(case["input"].clone()).expect("tab case input");
    let root_id = case["input"]["rootId"].as_str().unwrap_or_default();
    story_tab(&input, root_id).expect("the specimen case has stories")
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
    fn tab_spec_parses() {
        assert_eq!(STORIES.tab.copy.label, "Stories");
        assert_eq!(STORIES.tab.tile.thumb_height.wide, 260.0);
        assert_eq!(STORIES.tab.tile.thumb_max_width.wide, 560.0);
        assert_eq!(STORIES.tab.test_ids.tab, "thread-stories-tab");
    }

    #[test]
    fn story_tab_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(STORIES_JSON).unwrap();
        let cases = fixture["tabCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input: StoriesInput = serde_json::from_value(case["input"].clone()).unwrap();
            let root_id = case["input"]["rootId"].as_str().unwrap();
            let actual = serde_json::to_value(story_tab(&input, root_id)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
        let specimen = serde_json::to_value(story_tab_specimen()).unwrap();
        assert_eq!(specimen, cases[STORIES.tab.specimen.case]["expected"]);
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

    /// The specimen's agent rings ride the bubbles they match; the chat
    /// rings and an unmatched agent ring follow in ring order.
    #[test]
    fn bubbles_wear_their_agent_rings() {
        let fixture: Value = serde_json::from_str(STORIES_JSON).unwrap();
        let input: StoriesInput = serde_json::from_value(fixture["specimen"]["input"].clone()).unwrap();
        let rings = stories(&input);
        let split = bubble_rings(&["as", "ag1", "ag3"], &rings);
        assert_eq!(split.by_agent.len(), 1);
        assert_eq!(split.by_agent["ag1"].title, "Triage");
        let rest: Vec<&str> = split.rest.iter().map(|ring| ring.title.as_str()).collect();
        assert_eq!(rest, ["Board header polish", "Designer", "Ship the launch checklist"]);
        assert_eq!(bubble_rings(&[], &rings).rest, rings);
    }
}
