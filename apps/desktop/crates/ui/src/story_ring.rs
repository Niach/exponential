//! S-3 STORIES (`fixtures/stories.json`; web `packages/ui/src/stories.tsx`
//! `StoryRing` / `StoryRingRow`, iOS ExpUI Stories, Android ui/stories): THE
//! story ring of the IDE and the Chats page's ring row.
//!
//! - [`story_ring`]: the avatar inside a ring split into one arc per story,
//!   `ring.gap` degrees apart, `ring.width` px; a waiting story's arc = the
//!   ask-dock tint (plan = primary, question = yellow), an unseen one = the
//!   foreground, a seen one = the glass-stroke hairline (1px).
//! - [`story_ring_button`]: a ring with its title under it (the chats
//!   bubble's footprint); [`story_ring_row`] the rings side by side.
//! - [`team_stories`]: THE fold (`domain::stories::stories`) over the synced
//!   sessions, agents, show cards and the viewer's read marks, memoised per
//!   store change; [`stories_row`] the Chats page's `StoryRingRow` (a tap
//!   opens THE viewer, [`crate::story_viewer`]).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    canvas, div, point, px, AnyElement, App, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Sizable as _};

use domain::chats::CHATS;
use domain::stories::{Story, StoryKind, StoryRing, STORIES};

/// One arc's facts (a [`Story`]'s, or the styleguide's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RingArc {
    pub kind: StoryKind,
    pub waiting: bool,
    pub seen: bool,
}

impl From<&Story> for RingArc {
    fn from(story: &Story) -> Self {
        Self { kind: story.kind, waiting: story.waiting, seen: story.seen }
    }
}

/// The ring's arcs as `(start°, sweep°)` from 12 o'clock, clockwise (web
/// `storyArcs`): one story = the whole circle, else `360 / n` slots with
/// `gap` degrees between them.
pub(crate) fn story_arcs(count: usize, gap: f32) -> Vec<(f32, f32)> {
    match count {
        0 => Vec::new(),
        1 => vec![(0., 360.)],
        _ => {
            let slot = 360. / count as f32;
            let sweep = (slot - gap).max(0.);
            (0..count).map(|index| (index as f32 * slot + gap / 2., sweep)).collect()
        }
    }
}

/// An arc's stroke: (colour, width).
fn arc_look(arc: RingArc, cx: &App) -> (Hsla, f32) {
    let width = STORIES.ring.width;
    if arc.waiting {
        let tint = match arc.kind {
            StoryKind::Question => theme::tokens::YELLOW.to_hsla(),
            _ => cx.theme().primary,
        };
        (tint, width)
    } else if arc.seen {
        (theme::tokens::glass::STROKE_CARD.to_hsla(), 1.)
    } else {
        (cx.theme().foreground, width)
    }
}

/// THE story ring: `avatar` (sized by the caller to fill) inside the
/// segmented ring of outer diameter `size`.
pub(crate) fn story_ring(arcs: &[RingArc], size: f32, avatar: AnyElement, cx: &App) -> Div {
    let width = STORIES.ring.width;
    let looks: Vec<(f32, f32, Hsla, f32)> = story_arcs(arcs.len(), STORIES.ring.gap)
        .into_iter()
        .zip(arcs.iter())
        .map(|((start, sweep), arc)| {
            let (color, stroke) = arc_look(*arc, cx);
            (start, sweep, color, stroke)
        })
        .collect();
    let inner = (size - 4. * width).max(0.);
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let center = bounds.center();
                    let radius = (f32::from(bounds.size.width) - width) / 2.;
                    for (start, sweep, color, stroke) in &looks {
                        paint_arc(window, center, radius, *start, *sweep, *stroke, *color);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(
            div()
                .size(px(inner))
                .rounded_full()
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .child(avatar),
        )
}

/// One arc as a stroked polyline (`start`/`sweep` in degrees from 12
/// o'clock, clockwise).
fn paint_arc(
    window: &mut Window,
    center: gpui::Point<gpui::Pixels>,
    radius: f32,
    start: f32,
    sweep: f32,
    width: f32,
    color: Hsla,
) {
    if sweep <= 0.01 || radius <= 0. {
        return;
    }
    let from = (start - 90.).to_radians();
    let sweep_rad = sweep.to_radians();
    let steps = ((sweep / 360.) * 96.).ceil().max(2.) as usize;
    let at = |index: usize| {
        let angle = from + sweep_rad * index as f32 / steps as f32;
        point(center.x + px(radius * angle.cos()), center.y + px(radius * angle.sin()))
    };
    let mut path = gpui::PathBuilder::stroke(px(width));
    path.move_to(at(0));
    for index in 1..=steps {
        path.line_to(at(index));
    }
    if sweep >= 360. {
        path.close();
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// A ring's avatar (web `StoryAvatar`, stories.json AVATARS): an agent ring
/// = the agent's icon in its colour (the bubble's look), a chat ring = the
/// chat row's BARE brand mark (its CLI; no badge, no PR glyph), both on the
/// muted disc.
pub(crate) fn ring_avatar(agent: Option<(Option<&str>, Option<&str>)>, cli: Option<coding::CodingAgent>, glyph: f32, cx: &App) -> AnyElement {
    let mark = match agent {
        Some((icon, color)) => crate::icons::agent_icon(icon, color).with_size(px(glyph)).into_any_element(),
        None => crate::chat_list::chat_brand_mark(cli, glyph),
    };
    div()
        .size_full()
        .rounded_full()
        .bg(cx.theme().muted)
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
        .into_any_element()
}

/// The avatar of `ring` off the store (the agent definition, else the
/// root's CLI).
pub(crate) fn store_ring_avatar(ring: &StoryRing, glyph: f32, cx: &App) -> AnyElement {
    if let Some(agent_id) = ring.agent_id.as_deref() {
        let def = crate::session_data::agent_def(agent_id, cx);
        let icon = def.as_ref().and_then(|def| def.icon.clone());
        let color = def.as_ref().and_then(|def| def.color.clone());
        return ring_avatar(Some((icon.as_deref(), color.as_deref())), None, glyph, cx);
    }
    let cli = ring
        .root_id
        .as_deref()
        .and_then(|root| crate::session_data::session(root, cx))
        .and_then(|root| root.cli)
        .as_deref()
        .and_then(coding::CodingAgent::parse);
    ring_avatar(None, cli, glyph, cx)
}

/// The glyph inside a bubble-sized ring (the chats bubble's).
pub(crate) const RING_GLYPH_PX: f32 = 24.;

/// One ring of the row (web `StoryRingButton`): the ring with its title
/// under it, bold while it holds anything unseen. The caller hangs the tap.
pub(crate) fn story_ring_button(ring: &StoryRing, avatar: AnyElement, cx: &App) -> Div {
    let theme = cx.theme();
    let size = CHATS.shell.bubble;
    let arcs: Vec<RingArc> = ring.stories.iter().map(RingArc::from).collect();
    v_flex()
        .w(px(size + 6.))
        .flex_shrink_0()
        .items_center()
        .gap_1p5()
        .child(story_ring(&arcs, size, avatar, cx))
        .child(
            div()
                .w_full()
                .truncate()
                .text_center()
                .text_xs()
                .when(ring.unseen, |this| this.font_weight(FontWeight::MEDIUM))
                .text_color(if ring.unseen { theme.foreground } else { theme.muted_foreground })
                .child(SharedString::from(ring.title.clone())),
        )
}

/// The rings side by side, scrolling sideways (web `StoryRingRow`); none =
/// nothing. `wrap` hangs each ring's tap (its index and key).
pub(crate) fn story_ring_row(
    rings: &[StoryRing],
    avatar_for: impl Fn(&StoryRing) -> AnyElement,
    wrap: impl Fn(usize, &StoryRing, Div) -> AnyElement,
    cx: &App,
) -> Option<AnyElement> {
    if rings.is_empty() {
        return None;
    }
    Some(
        h_flex()
            .id("story-ring-row")
            .w_full()
            .gap_3()
            .pb_1()
            .overflow_x_scroll()
            .children(
                rings
                    .iter()
                    .enumerate()
                    .map(|(index, ring)| wrap(index, ring, story_ring_button(ring, avatar_for(ring), cx))),
            )
            .into_any_element(),
    )
}

/// THE Chats page's `StoryRingRow`: the team's rings, a tap opening THE
/// viewer at that ring. S-3: it sits above the agent bubbles until the
/// bubbles wear their agent's ring (chats.json BUBBLES).
pub(crate) fn stories_row(team_id: &str, cx: &mut App) -> Option<AnyElement> {
    let rings = team_stories(team_id, cx);
    let team = team_id.to_string();
    story_ring_row(
        &rings,
        |ring| store_ring_avatar(ring, RING_GLYPH_PX, cx),
        |index, ring, element| {
            let key = ring.key.clone();
            let team = team.clone();
            div()
                .id((SharedString::from(STORIES.test_ids.ring.clone()), index))
                .cursor_pointer()
                .child(element)
                .on_click(move |_, window, cx| {
                    crate::story_viewer::open(&team, &key, window, cx);
                })
                .into_any_element()
        },
        cx,
    )
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq)]
struct StoriesKey {
    team_id: String,
    revisions: [u64; 5],
    /// The metas' ages and the results' expiry move with the minute.
    minute: i64,
}

#[derive(Default)]
struct StoriesMemo(Option<(StoriesKey, Rc<Vec<StoryRing>>)>);

impl gpui::Global for StoriesMemo {}

/// THE team's story rings (`domain::stories::stories`), memoised per store
/// change and minute.
pub(crate) fn team_stories(team_id: &str, cx: &mut App) -> Rc<Vec<StoryRing>> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Rc::new(Vec::new());
    };
    let collections = store.collections();
    let key = StoriesKey {
        team_id: team_id.to_string(),
        revisions: [
            collections.sessions.read(cx).revision(),
            collections.messages.read(cx).revision(),
            collections.conversations.read(cx).revision(),
            collections.session_reads.read(cx).revision(),
            collections.agents.read(cx).revision(),
        ],
        minute: crate::session_data::now_ms() / 60_000,
    };
    if let Some((cached, rings)) = &cx.default_global::<StoriesMemo>().0 {
        if *cached == key {
            return rings.clone();
        }
    }
    let rings = Rc::new(domain::stories::stories(&stories_input(team_id, cx)));
    cx.global_mut::<StoriesMemo>().0 = Some((key, rings.clone()));
    rings
}

fn iso(value: Option<&str>) -> Option<i64> {
    value.and_then(domain::session_tree::iso_ms)
}

/// The fold's input off the store: only the sessions that posted a story
/// card (and their roots) are named.
fn stories_input(team_id: &str, cx: &App) -> domain::stories::StoriesInput {
    use domain::stories::{StoriesInput, StoryAgentInput, StoryMessageInput, StoryReadInput, StorySessionInput};
    let now = crate::session_data::now_ms();
    let empty = StoriesInput { now, sessions: Vec::new(), agents: Vec::new(), messages: Vec::new(), reads: Vec::new() };
    let Some(store) = sync::Store::try_global(cx) else {
        return empty;
    };
    let collections = store.collections();
    let sessions = collections.sessions.read(cx);
    let team: HashMap<&str, &domain::rows::SessionRow> = sessions
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .map(|session| (session.id.as_str(), session))
        .collect();
    let messages: Vec<StoryMessageInput> = crate::session_data::keyed_messages(cx, |owner, message| {
        team.contains_key(owner)
            && message.author_session_id.as_deref() == Some(owner)
            && message.card.as_ref().is_some_and(domain::stories::is_story_card)
    })
    .into_iter()
    .filter_map(|message| {
        Some(StoryMessageInput {
            created_at: iso(message.created_at.as_deref())?,
            answered_at: iso(message.answered_at.as_deref()),
            id: message.id,
            session_id: message.session_id,
            author_session_id: message.author_session_id,
            card: message.card?,
        })
    })
    .collect();
    let root_of = |session: &domain::rows::SessionRow| session.root_id.clone().unwrap_or_else(|| session.id.clone());
    let mut named: HashSet<String> = HashSet::new();
    for message in &messages {
        if let Some(session) = team.get(message.session_id.as_str()) {
            named.insert(session.id.clone());
            named.insert(root_of(session));
        }
    }
    let story_sessions: Vec<StorySessionInput> = named
        .iter()
        .filter_map(|id| team.get(id.as_str()))
        .map(|session| StorySessionInput {
            id: session.id.clone(),
            root_id: root_of(session),
            name: crate::session_data::session_title_of(&session.id, cx).unwrap_or_default(),
            agent_id: session.agent_id.clone(),
            started_by_agent_id: session.started_by_agent_id.clone(),
        })
        .collect();
    let agents: Vec<StoryAgentInput> = collections
        .agents
        .read(cx)
        .iter()
        .map(|agent| StoryAgentInput {
            id: agent.id.clone(),
            name: agent.name.clone().unwrap_or_default(),
            // No `role` column on the IDE's agents yet: nobody is the
            // Assistant (STORIES-BRIEF fold input).
            assistant: false,
        })
        .collect();
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let reads: Vec<StoryReadInput> = collections
        .session_reads
        .read(cx)
        .iter()
        .filter(|read| named.contains(&read.session_id))
        .filter(|read| match (&me, read.user_id.as_deref()) {
            (Some(me), Some(user)) => me == user,
            _ => true,
        })
        .map(|read| StoryReadInput {
            session_id: read.session_id.clone(),
            read_at: iso(read.read_at.as_deref()),
            story_seen_at: iso(read.story_seen_at.as_deref()),
        })
        .collect();
    StoriesInput { now, sessions: story_sessions, agents, messages, reads }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// web `storyArcs`: one story = the whole circle; n = n slots `gap`
    /// apart, the first centred on 12 o'clock + half a gap.
    #[test]
    fn arcs_split_the_circle() {
        assert!(story_arcs(0, 6.).is_empty());
        assert_eq!(story_arcs(1, 6.), vec![(0., 360.)]);
        assert_eq!(story_arcs(2, 6.), vec![(3., 174.), (183., 174.)]);
        assert_eq!(story_arcs(3, 6.).len(), 3);
    }
}
