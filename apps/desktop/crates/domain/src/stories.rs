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
use crate::plan_page::{PlanCounts, plan_counts};

const STORIES_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/stories.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoriesConstants {
    /// #EXP-1461: a ring keeps a story younger than this, waiting or not.
    pub window_hours: i64,
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
#[serde(rename_all = "camelCase")]
pub struct StoryProgressSpec {
    pub height: f32,
    /// #EXP-1493: between the segments (one per story).
    pub gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryViewerSpec {
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    /// #EXP-1432: the floating header's scrim (at least this tall)…
    pub scrim_height: f32,
    /// …fading to clear over its last `scrim_fade` px.
    pub scrim_fade: f32,
    /// #EXP-1463: the wide modal's room to the viewport edge (x each side,
    /// y top and bottom).
    pub inset: StoryViewerInset,
    /// #EXP-1463: a picture frame's modal is at least this wide.
    pub min_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct StoryViewerInset {
    pub x: f32,
    pub y: f32,
}

/// #EXP-1461 THE STORY ROW of the chats header (`row`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryRowSpec {
    /// Every item is EXACTLY this wide (never shrunk by its neighbours).
    pub item_width: f32,
    pub gap: f32,
    /// The name under the ring.
    pub label_gap: f32,
    pub pad_bottom: f32,
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
    /// #EXP-1462: line 1 → the description.
    pub text_gap: f32,
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

/// #EXP-1493 THE kind badge ×4 (`badge`): THE Pill (sm, readonly) with the
/// concept leading and the kind word, tinted per kind.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StoryBadgeSpec {
    pub icons: StoryKindWords,
    pub tints: StoryKindWords,
    pub fill: f32,
    pub stroke: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoryKindWords {
    pub plan: String,
    pub question: String,
    pub result: String,
}

impl StoryKindWords {
    pub fn of(&self, kind: StoryKind) -> &str {
        match kind {
            StoryKind::Plan => &self.plan,
            StoryKind::Question => &self.question,
            StoryKind::Result => &self.result,
        }
    }
}

/// `fixtures/stories.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoriesSpec {
    pub constants: StoriesConstants,
    pub badge: StoryBadgeSpec,
    pub copy: StoriesCopy,
    pub ring: StoryRingSpec,
    pub progress: StoryProgressSpec,
    pub viewer: StoryViewerSpec,
    pub row: StoryRowSpec,
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
    /// #EXP-1493: a plan opens THE plan page (never a decision in a story).
    Open,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryPicture {
    pub label: String,
    pub attachment_id: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// #EXP-1432: the card's `description` (one line under the title).
    pub description: Option<String>,
    pub pictures: Vec<StoryPicture>,
    pub surface: bool,
    /// #EXP-1493: a plan's counts (plan-page `planCounts`), else `None`.
    pub counts: Option<PlanCounts>,
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
        description: non_empty(s(card, "description")),
        counts: (kind == StoryKind::Plan).then(|| plan_counts(card)),
        pictures,
        surface,
        footer: match kind {
            StoryKind::Plan => StoryFooter::Open,
            _ if resolution.is_some() => StoryFooter::None,
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

/// #EXP-1493: newest first (createdAt desc, then id desc).
fn by_newest(a: &Story, b: &Story) -> std::cmp::Ordering {
    b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id))
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
        // EXP-1461: a ring holds the last `windowHours` only, waiting or not.
        if input.now - story.created_at >= STORIES.constants.window_hours * HOUR {
            continue;
        }
        let ring = empty_ring(root, &agents);
        match rings.iter_mut().find(|existing| existing.key == ring.key) {
            Some(existing) => existing.stories.push(story),
            None => rings.push(StoryRing { stories: vec![story], ..ring }),
        }
    }

    for ring in &mut rings {
        // #EXP-1493: a session's newer result replaces its older ones in a ring.
        let mut newest_result: HashMap<&str, &Story> = HashMap::new();
        for story in ring.stories.iter().filter(|story| story.kind == StoryKind::Result) {
            let kept = newest_result.entry(story.session_id.as_str()).or_insert(story);
            if by_newest(story, kept).is_lt() {
                *kept = story;
            }
        }
        let kept_ids: Vec<String> = newest_result.values().map(|story| story.id.clone()).collect();
        let all = std::mem::take(&mut ring.stories);
        let kept = all.into_iter().filter(|story| story.kind != StoryKind::Result || kept_ids.contains(&story.id));
        let (mut waiting, mut rest): (Vec<_>, Vec<_>) = kept.partition(|story| story.waiting);
        waiting.sort_by(by_newest);
        rest.sort_by(by_newest);
        ring.waiting = waiting.len();
        ring.stories = waiting.into_iter().chain(rest).collect();
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
/// settled plans/questions stay (footer none, a plan keeps `open`; `meta`
/// names the outcome), a superseded plan drops. Waiting first, then the
/// rest, each newest first (#EXP-1493). ONE ring (the root's, as `stories` keys it) so the viewer opens on
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
    waiting.sort_by(by_newest);
    rest.sort_by(by_newest);
    let unseen = waiting.iter().chain(&rest).any(|story| story.waiting || !story.seen);
    Some(StoryRing {
        waiting: waiting.len(),
        unseen,
        stories: waiting.into_iter().chain(rest).collect(),
        ..empty_ring(root, &agents)
    })
}

/// #EXP-1493: what one viewer frame draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryFrameKind {
    /// A plan's cover (the first picture, the counts, Open plan).
    Plan,
    /// The story's pictures in THE carousel (pictures win over a surface).
    Gallery,
    Surface,
    Title,
}

/// #EXP-1493: THE viewer's ONE step per story (stories.json `frameCases`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryFrame {
    /// The story's index in the ring and its id.
    pub story: usize,
    pub story_id: String,
    pub kind: StoryFrameKind,
    /// The story's resolved pictures (a plan's cover = the first).
    pub pictures: usize,
    pub title: Option<String>,
    pub description: Option<String>,
    pub footer: StoryFooter,
    /// Per picture on a gallery (the frame lasts pictures × it).
    pub auto_advance_ms: Option<u64>,
}

/// A story's frame: a plan = its cover; else pictures win (a gallery: the
/// surface stays in the chat), else the surface, else the bare title.
fn frame_of(story: &Story, at: usize) -> StoryFrame {
    let kind = if story.kind == StoryKind::Plan {
        StoryFrameKind::Plan
    } else if !story.pictures.is_empty() {
        StoryFrameKind::Gallery
    } else if story.surface {
        StoryFrameKind::Surface
    } else {
        StoryFrameKind::Title
    };
    StoryFrame {
        story: at,
        story_id: story.id.clone(),
        kind,
        pictures: story.pictures.len(),
        title: story.title.clone(),
        description: story.description.clone(),
        footer: story.footer,
        auto_advance_ms: story.auto_advance_ms,
    }
}

/// #EXP-1493: THE viewer's steps over a ring (`frameCases`): ONE frame per
/// story, in ring order.
pub fn story_frames(stories: &[Story]) -> Vec<StoryFrame> {
    stories.iter().enumerate().map(|(at, story)| frame_of(story, at)).collect()
}

/// The frame index of a story; `None` = none. A thumbnail opens its story's
/// frame with the gallery at that picture (the host's state).
pub fn story_frame_index(frames: &[StoryFrame], story_id: &str) -> Option<usize> {
    frames.iter().position(|frame| frame.story_id == story_id)
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

/// THE STORY ROW of the chats header (#EXP-1461, `rowCases`): ONLY rings.
/// `bubbles` = the agent bubbles (chats.json BUBBLES, in listAgents order)
/// whose agent has a ring, each wearing it (`by_agent`, the first wins);
/// `rest` = the rings no bubble wears (the root rings, an agent ring with no
/// bubble), in ring order. A bubble without a ring is not drawn; both empty
/// = no row at all.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoryRow {
    pub bubbles: Vec<String>,
    pub by_agent: HashMap<String, StoryRing>,
    pub rest: Vec<StoryRing>,
}

impl StoryRow {
    pub fn is_empty(&self) -> bool {
        self.bubbles.is_empty() && self.rest.is_empty()
    }
}

/// web `storyRow`.
pub fn story_row(bubble_agent_ids: &[&str], rings: &[StoryRing]) -> StoryRow {
    let mut out = StoryRow::default();
    for ring in rings {
        match ring.agent_id.as_deref() {
            Some(agent) if bubble_agent_ids.contains(&agent) && !out.by_agent.contains_key(agent) => {
                out.by_agent.insert(agent.to_string(), ring.clone());
            }
            _ => out.rest.push(ring.clone()),
        }
    }
    for id in bubble_agent_ids {
        if out.by_agent.contains_key(*id) && !out.bubbles.iter().any(|seen| seen == id) {
            out.bubbles.push((*id).to_string());
        }
    }
    out
}

/// A wide viewer modal's size in px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryViewerSize {
    pub width: i64,
    pub height: i64,
}

/// THE wide viewer's modal size (#EXP-1463, `sizeCases`; web
/// `storyViewerSize`): a picture with a known size = fitted (contain) into
/// the viewport less `viewer.inset`, at least `viewer.minWidth` wide; any
/// other frame = `viewer.width` x `viewer.height`, each capped by the same
/// box. Whole px.
pub fn story_viewer_size(picture: Option<(Option<f64>, Option<f64>)>, viewport: (f64, f64)) -> StoryViewerSize {
    let viewer = &STORIES.viewer;
    let box_width = (viewport.0 - 2.0 * viewer.inset.x as f64).max(0.0);
    let box_height = (viewport.1 - 2.0 * viewer.inset.y as f64).max(0.0);
    let sized = picture.and_then(|(w, h)| match (w, h) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Some((w, h)),
        _ => None,
    });
    let Some((w, h)) = sized else {
        return StoryViewerSize {
            width: js_round((viewer.width as f64).min(box_width)),
            height: js_round((viewer.height as f64).min(box_height)),
        };
    };
    let scale = (box_width / w).min(box_height / h);
    let width = js_round(w * scale) as f64;
    StoryViewerSize {
        width: js_round(box_width.min((viewer.min_width as f64).max(width))),
        height: js_round(h * scale),
    }
}

/// JS `Math.round` (half up, towards +inf).
fn js_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
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
        assert_eq!(STORIES.tab.tile.thumb_height.wide, 400.0);
        assert_eq!(STORIES.tab.tile.thumb_max_width.wide, 720.0);
        assert_eq!(STORIES.tab.tile.text_gap, 4.0);
        assert_eq!(STORIES.row.item_width, 72.0);
        assert_eq!(STORIES.constants.window_hours, 24);
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
    fn story_frames_match_the_fixture() {
        let fixture: Value = serde_json::from_str(STORIES_JSON).unwrap();
        let cases = fixture["frameCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let stories: Vec<Story> = serde_json::from_value(case["input"]["stories"].clone()).unwrap();
            let actual = serde_json::to_value(story_frames(&stories)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
        let stories: Vec<Story> = serde_json::from_value(cases[1]["input"]["stories"].clone()).unwrap();
        let frames = story_frames(&stories);
        assert_eq!(story_frame_index(&frames, "missing"), None);
        assert_eq!(story_frame_index(&frames, &stories[1].id), Some(1));
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

    #[test]
    fn story_row_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(STORIES_JSON).unwrap();
        let cases = fixture["rowCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let ids: Vec<&str> =
                case["input"]["bubbleAgentIds"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            let rings: Vec<StoryRing> = case["input"]["rings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|ring| StoryRing {
                    key: ring["key"].as_str().unwrap().to_string(),
                    agent_id: ring["agentId"].as_str().map(str::to_string),
                    root_id: None,
                    title: String::new(),
                    assistant: false,
                    waiting: 0,
                    unseen: false,
                    stories: Vec::new(),
                })
                .collect();
            let row = story_row(&ids, &rings);
            let actual = serde_json::json!({
                "bubbles": row.bubbles,
                "rest": row.rest.iter().map(|ring| ring.key.clone()).collect::<Vec<_>>(),
            });
            assert_eq!(actual, case["expected"], "{}", case["name"]);
            assert_eq!(row.is_empty(), row.bubbles.is_empty() && row.rest.is_empty());
        }
    }

    #[test]
    fn story_viewer_size_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(STORIES_JSON).unwrap();
        let cases = fixture["sizeCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let picture = if input["picture"].is_null() {
                None
            } else {
                Some((input["picture"]["width"].as_f64(), input["picture"]["height"].as_f64()))
            };
            let viewport = (input["viewport"]["width"].as_f64().unwrap(), input["viewport"]["height"].as_f64().unwrap());
            let actual = serde_json::to_value(story_viewer_size(picture, viewport)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }
}
