//! S-3: `story-ring` (`fixtures/stories.json` STORY RING; web entry
//! `story-ring`): THE story ring ([`crate::story_ring`]) case for case with
//! the web entry: the Assistant's ring (a waiting plan + an unseen result),
//! a chat waiting on a question, an unseen result and a seen one, each in
//! the Chats page's row footprint with its state word under it.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::stories::{Story, StoryFooter, StoryKind, StoryRing};

use crate::story_ring::{ring_avatar, ring_state, story_ring_button, RingArc, RING_GLYPH_PX};

pub(crate) const ID: &str = "story-ring";
pub(crate) const OWNER: &str = "S-3";

/// A demo story (only the arc's facts matter here).
pub(crate) fn story(id: &str, kind: StoryKind, waiting: bool, seen: bool) -> Story {
    Story {
        id: id.into(),
        session_id: "sg-story-session".into(),
        root_id: "sg-story-root".into(),
        session_name: "Checkout redesign".into(),
        kind,
        waiting,
        seen,
        created_at: 0,
        meta: format!("{} · 5m", kind.label()),
        title: None,
        pictures: Vec::new(),
        surface: kind == StoryKind::Result,
        text: None,
        footer: match kind {
            StoryKind::Plan => StoryFooter::Plan,
            StoryKind::Question => StoryFooter::Choices,
            StoryKind::Result => StoryFooter::None,
        },
        auto_advance_ms: (kind == StoryKind::Result).then_some(domain::stories::STORIES.constants.auto_advance_ms),
    }
}

pub(crate) fn ring(key: &str, title: &str, agent: bool, stories: Vec<Story>) -> StoryRing {
    let waiting = stories.iter().filter(|story| story.waiting).count();
    let unseen = stories.iter().any(|story| story.waiting || !story.seen);
    StoryRing {
        key: key.into(),
        agent_id: agent.then(|| format!("{key}-agent")),
        root_id: (!agent).then(|| format!("{key}-root")),
        title: title.into(),
        assistant: key == "assistant",
        waiting,
        unseen,
        stories,
    }
}

/// The four rings of the entry.
pub(crate) fn rings() -> Vec<(StoryRing, Option<(&'static str, &'static str)>)> {
    vec![
        (
            ring(
                "assistant",
                "Assistant",
                true,
                vec![story("a1", StoryKind::Plan, true, false), story("a2", StoryKind::Result, false, false)],
            ),
            Some(("sparkles", "#8b5cf6")),
        ),
        (ring("waiting", "Fix the checkout", false, vec![story("w1", StoryKind::Question, true, false)]), None),
        (
            ring(
                "unseen",
                "Big Brother",
                true,
                vec![
                    story("u1", StoryKind::Result, false, false),
                    story("u2", StoryKind::Result, false, false),
                    story("u3", StoryKind::Result, false, true),
                ],
            ),
            Some(("bot", "#22c55e")),
        ),
        (ring("seen", "Batch 2", false, vec![story("s1", StoryKind::Result, false, true)]), None),
    ]
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    h_flex().gap_6().items_start().children(rings().into_iter().map(|(ring, agent)| {
        let arcs: Vec<RingArc> = ring.stories.iter().map(RingArc::from).collect();
        let avatar = ring_avatar(agent.map(|(icon, color)| (Some(icon), Some(color))), None, RING_GLYPH_PX, cx);
        v_flex()
            .items_center()
            .gap_2()
            .child(story_ring_button(&ring, avatar, cx))
            .child(div().text_xs().text_color(muted).child(ring_state(&arcs)))
    }))
}
