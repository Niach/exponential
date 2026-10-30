//! S-3 STORIES (`fixtures/stories.json` STORY VIEWER; web
//! `packages/ui/src/stories.tsx` `StoryViewer`): THE story viewer of the
//! IDE: a centred modal (`viewer.width` × `viewer.height` max,
//! `viewer.radius`) over a scrim, layered over the window from the Chats
//! page and a session's Stories tab.
//!
//! #EXP-1432: the viewer steps FRAMES ([`story_frames`]: a picture story =
//! one frame per picture, else its surface, else the plan text, else the
//! bare title), never stories with an inner pager.
//! - Floating over a top black-to-clear scrim (`viewer.scrimHeight`, its
//!   last `viewer.scrimFade` px fading out): THE progress ([`story_progress`],
//!   one segment per frame, `progress.gap` within a story, `progress.storyGap`
//!   between stories), the header (the ring's avatar, the story's
//!   `sessionName`, its `meta` muted, the close glyph; a click on the avatar
//!   or the name opens that session and closes the viewer), then the frame's
//!   title (one line) and description (muted, up to 3 lines), each ONCE.
//! - Body: a picture fills the modal fitted (contain) on black; a surface
//!   frame = THE surface renderer with no card chrome; a text frame = the
//!   plan's markdown; a title frame = nothing. A scrolling body ends clear of
//!   the footer.
//! - Footer, on a story's LAST frame only, floating over the bottom on the
//!   mirrored scrim on every frame: a plan's / question's answer
//!   controls are THE card's ([`crate::plan_card::plan_story_sheet`] /
//!   [`crate::plan_card::question_story_sheet`]), answered through THE
//!   card-answer path (`sessions.answer`); a result has none.
//! - Navigation: → / the side arrow / a click on the right third (any
//!   frame, never on an interactive element: [`tap_to_step`]) =
//!   next frame (past a ring's last = the next ring, past the last ring =
//!   close); ← / the left arrow / the left third = previous; Escape, the
//!   close glyph or a press on the BARE scrim = close (a press on a side
//!   arrow steps, never closes). Every frame has its own timer (a result's
//!   `autoAdvanceMs`, a decision's never); holding a press pauses it.
//! - Every frame SHOWN raises the viewer's stories mark of its story's root
//!   (`sessions.markStoriesSeen`, the server keeps the greatest).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    anchored, deferred, div, img, linear_color_stop, linear_gradient, point, px, relative, AnyElement, App,
    AppContext as _, Div, Entity, FocusHandle, FontWeight, Hsla, InteractiveElement as _, IntoElement as _,
    MouseButton, ObjectFit, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use domain::rows::MessageRow;
use domain::session_tree::SessionThreadRow;
use domain::stories::{
    story_frame_index, story_frames, Story, StoryFooter, StoryFrame, StoryFrameKind, StoryKind, StoryRing, STORIES,
};

use crate::icons::registry;
use crate::markdown::{ImageCache, ImageSlot};
use crate::thread_screen::ThreadText;

/// A press held this long pauses the timer instead of navigating.
const HOLD: Duration = Duration::from_millis(250);

/// The viewer's place and timer.
struct Open {
    team_id: String,
    /// The Stories tab's root when opened from a session screen's tab (its
    /// ONE ring, `domain::stories::story_tab`); `None` = the team's rings.
    tab_root: Option<String>,
    ring_key: String,
    story_id: String,
    /// The frame's place among its story's frames.
    frame: usize,
    /// The story's index when last resolved (an answered story drops out:
    /// the viewer stays at the same place in the ring).
    story_index: usize,
    shown_at: Instant,
    paused_at: Option<Instant>,
    /// The press on the panel (its start and place: hold, drag or tap).
    pressed_at: Option<(Instant, gpui::Point<gpui::Pixels>)>,
}

#[derive(Default)]
struct Viewer {
    open: Option<Open>,
    focus: Option<FocusHandle>,
    images: Option<Entity<ImageCache>>,
    _images: Option<Subscription>,
    /// The greatest mark sent per root this run (never re-sent).
    marked: HashMap<String, i64>,
    /// The panel's last painted bounds (the tap thirds).
    panel_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    /// The floating footer's measured height per story (the scrolling body
    /// ends clear of it).
    footer_heights: HashMap<String, f32>,
    /// The focus when the current press began (capture phase): a press that
    /// moved it went to a text field, never a tap.
    press_focus: Option<Option<FocusHandle>>,
}

impl gpui::Global for Viewer {}

/// Opens THE viewer at `ring_key`'s first frame.
pub(crate) fn open(team_id: &str, ring_key: &str, window: &mut Window, cx: &mut App) {
    open_with(team_id, None, ring_key, None, 0, window, cx);
}

/// Opens THE viewer on a session screen's Stories tab ring (stories.json
/// STORIES TAB) at `story_id`'s frame of picture `picture` (a tile = its
/// first frame, a thumbnail = that picture's, `storyFrameIndex`).
pub(crate) fn open_tab(
    team_id: &str,
    root_id: &str,
    story_id: &str,
    picture: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(ring_key) = crate::story_ring::root_story_tab(root_id, cx).as_ref().as_ref().map(|ring| ring.key.clone())
    else {
        return;
    };
    open_with(team_id, Some(root_id), &ring_key, Some(story_id), picture, window, cx);
}

/// The rings the open viewer walks (the tab's ONE ring, else the team's).
fn rings_of(team_id: &str, tab_root: Option<&str>, cx: &mut App) -> Rc<Vec<StoryRing>> {
    match tab_root {
        Some(root_id) => Rc::new(crate::story_ring::root_story_tab(root_id, cx).as_ref().clone().into_iter().collect()),
        None => crate::story_ring::team_stories(team_id, cx),
    }
}

fn open_rings(cx: &mut App) -> Option<Rc<Vec<StoryRing>>> {
    let (team_id, tab_root) = cx
        .try_global::<Viewer>()
        .and_then(|viewer| viewer.open.as_ref())
        .map(|open| (open.team_id.clone(), open.tab_root.clone()))?;
    Some(rings_of(&team_id, tab_root.as_deref(), cx))
}

fn open_with(
    team_id: &str,
    tab_root: Option<&str>,
    ring_key: &str,
    story_id: Option<&str>,
    picture: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let rings = rings_of(team_id, tab_root, cx);
    let Some(ring) = rings.iter().find(|ring| ring.key == ring_key) else {
        return;
    };
    let frames = story_frames(&ring.stories);
    let at = story_id.and_then(|id| story_frame_index(&frames, id, picture)).unwrap_or(0);
    let Some(frame) = frames.get(at) else {
        return;
    };
    let focus = {
        let viewer = cx.default_global::<Viewer>();
        viewer.focus.clone()
    }
    .unwrap_or_else(|| {
        let handle = cx.focus_handle();
        cx.global_mut::<Viewer>().focus = Some(handle.clone());
        handle
    });
    cx.global_mut::<Viewer>().open = Some(Open {
        team_id: team_id.to_string(),
        tab_root: tab_root.map(str::to_string),
        ring_key: ring_key.to_string(),
        story_id: frame.story_id.clone(),
        frame: frame.index,
        story_index: frame.story,
        shown_at: Instant::now(),
        paused_at: None,
        pressed_at: None,
    });
    window.focus(&focus, cx);
    cx.refresh_windows();
}

/// Closes THE viewer.
pub(crate) fn close(cx: &mut App) {
    if let Some(viewer) = cx.try_global::<Viewer>() {
        if viewer.open.is_none() {
            return;
        }
    }
    cx.default_global::<Viewer>().open = None;
    cx.refresh_windows();
}

/// Where the viewer stands: the ring's index, its frames and the flat frame.
struct Place {
    ring: usize,
    frames: Vec<StoryFrame>,
    at: usize,
}

/// The open frame's place in `rings` (a frame that is gone = its story's
/// first; an answered story = the frame at the same story index, clamped; a
/// ring that is gone = `None`).
fn resolve(open: &Open, rings: &[StoryRing]) -> Option<Place> {
    let ring = rings.iter().position(|ring| ring.key == open.ring_key)?;
    let frames = story_frames(&rings[ring].stories);
    if frames.is_empty() {
        return None;
    }
    let at = frames
        .iter()
        .position(|frame| frame.story_id == open.story_id && frame.index == open.frame)
        .or_else(|| frames.iter().position(|frame| frame.story_id == open.story_id))
        .or_else(|| {
            let story = open.story_index.min(rings[ring].stories.len().saturating_sub(1));
            frames.iter().position(|frame| frame.story == story)
        })
        .unwrap_or(0);
    Some(Place { ring, frames, at })
}

/// A step over the frames of every ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// (ring index, flat frame index in that ring).
    To(usize, usize),
    Close,
    Stay,
}

/// NAVIGATION (frames): the next frame, past a ring's last = the next ring's
/// first, past the last ring = close; the previous frame, a ring's first =
/// the previous ring's last, the very first stays put.
fn step_frame(rings: &[StoryRing], place: &Place, forward: bool) -> Step {
    if forward {
        if place.at + 1 < place.frames.len() {
            return Step::To(place.ring, place.at + 1);
        }
        return (place.ring + 1..rings.len())
            .find(|&next| !rings[next].stories.is_empty())
            .map_or(Step::Close, |next| Step::To(next, 0));
    }
    if place.at > 0 {
        return Step::To(place.ring, place.at - 1);
    }
    (0..place.ring)
        .rev()
        .find(|&prev| !rings[prev].stories.is_empty())
        .map_or(Step::Stay, |prev| Step::To(prev, story_frames(&rings[prev].stories).len() - 1))
}

/// Moves to `to`, restarting the timer (`Close` closes).
fn go(to: Step, rings: &[StoryRing], cx: &mut App) {
    let viewer = cx.default_global::<Viewer>();
    match (to, viewer.open.as_mut()) {
        (Step::Stay, _) => return,
        (Step::To(ring, at), Some(open)) => {
            let target = &rings[ring];
            let frames = story_frames(&target.stories);
            let Some(frame) = frames.get(at) else { return };
            open.ring_key = target.key.clone();
            open.story_id = frame.story_id.clone();
            open.frame = frame.index;
            open.story_index = frame.story;
            open.shown_at = Instant::now();
            open.paused_at = None;
            open.pressed_at = None;
        }
        _ => viewer.open = None,
    }
    cx.refresh_windows();
}

/// Next frame (past the last ring = close).
pub(crate) fn next(cx: &mut App) {
    step(true, cx);
}

/// Previous frame (the very first stays put).
pub(crate) fn previous(cx: &mut App) {
    step(false, cx);
}

fn step(forward: bool, cx: &mut App) {
    let Some(rings) = open_rings(cx) else {
        return;
    };
    let Some(place) = cx.global::<Viewer>().open.as_ref().and_then(|open| resolve(open, &rings)) else {
        close(cx);
        return;
    };
    let to = step_frame(&rings, &place, forward);
    go(to, &rings, cx);
}

fn pause(paused: bool, cx: &mut App) {
    if let Some(open) = cx.default_global::<Viewer>().open.as_mut() {
        match (paused, open.paused_at) {
            (true, None) => open.paused_at = Some(Instant::now()),
            (false, Some(at)) => {
                open.shown_at += at.elapsed();
                open.paused_at = None;
            }
            _ => {}
        }
    }
    cx.refresh_windows();
}

/// Raises the viewer's stories mark of `story`'s root (once per mark).
fn mark_seen(story: &Story, cx: &mut App) {
    if story.seen {
        return;
    }
    let viewer = cx.default_global::<Viewer>();
    if viewer.marked.get(&story.root_id).is_some_and(|at| *at >= story.created_at) {
        return;
    }
    viewer.marked.insert(story.root_id.clone(), story.created_at);
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    let (root_id, at) = (story.root_id.clone(), story.created_at);
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::sessions::mark_stories_seen(&trpc, &root_id, at) {
                log::warn!("[stories] mark {root_id} seen failed: {err}");
            }
        })
        .detach();
}

/// The viewer's picture cache (the account's transport; repaints the
/// windows when bytes land).
fn images(cx: &mut App) -> Entity<ImageCache> {
    if let Some(images) = cx.default_global::<Viewer>().images.clone() {
        return images;
    }
    let transport = crate::queries::attachment_transport(cx);
    let images = cx.new(|_| ImageCache::new(transport));
    let subscription = cx.observe(&images, |_, cx| cx.refresh_windows());
    let viewer = cx.global_mut::<Viewer>();
    viewer.images = Some(images.clone());
    viewer._images = Some(subscription);
    images
}

// ---------------------------------------------------------------------------
// The overlay (the Chats page hosts it)
// ---------------------------------------------------------------------------

/// THE viewer as a layer over the window, while open: resolves the frame,
/// runs its timer, marks its story seen and paints the panel.
pub(crate) fn overlay(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let team_id = cx.try_global::<Viewer>()?.open.as_ref()?.team_id.clone();
    let mut rings = open_rings(cx)?;
    let mut place = cx.global::<Viewer>().open.as_ref().and_then(|open| resolve(open, &rings));
    // A result's frame past its time moves on (a decision's never does).
    if let Some(current) = place.as_ref() {
        let open = cx.global::<Viewer>().open.as_ref()?;
        if let (Some(ms), None) = (current.frames[current.at].auto_advance_ms, open.paused_at) {
            if open.shown_at.elapsed() >= Duration::from_millis(ms) {
                let to = step_frame(&rings, current, true);
                go(to, &rings, cx);
                rings = open_rings(cx)?;
                place = cx.global::<Viewer>().open.as_ref().and_then(|open| resolve(open, &rings));
            }
        }
    }
    let Some(Place { ring: ring_index, frames, at }) = place else {
        cx.default_global::<Viewer>().open = None;
        return None;
    };
    let ring = &rings[ring_index];
    let frame = &frames[at];
    let story = &ring.stories[frame.story];
    {
        let open = cx.global_mut::<Viewer>().open.as_mut()?;
        open.story_id = frame.story_id.clone();
        open.frame = frame.index;
        open.story_index = frame.story;
    }
    mark_seen(story, cx);
    let (fraction, timing) = {
        let open = cx.global::<Viewer>().open.as_ref()?;
        match frame.auto_advance_ms {
            Some(ms) => {
                let elapsed = open.paused_at.unwrap_or_else(Instant::now).duration_since(open.shown_at);
                ((elapsed.as_secs_f32() * 1000. / ms as f32).clamp(0., 1.), open.paused_at.is_none())
            }
            None => (1., false),
        }
    };
    if timing {
        window.request_animation_frame();
    }
    let images = images(cx);
    let message = message_of(&story.id, &story.session_id, cx);
    let text = ThreadText::team(Some(&team_id)).answering(true);
    let content = story_content(story, frame, message.as_ref(), &text, &images, window, cx);
    let avatar = crate::story_ring::store_ring_avatar(ring, HEADER_GLYPH, cx);
    let session_id = story.session_id.clone();
    let handlers = PanelHandlers {
        open_session: Some(Rc::new(move |window, cx| {
            close(cx);
            crate::thread_screen::open_session_card(&session_id, "", window, cx);
        })),
        close: Some(Rc::new(|_, cx| close(cx))),
        navigate: true,
    };
    let panel = story_panel(ring, &frames, at, fraction, avatar, content, handlers, cx);
    let focus = cx.global::<Viewer>().focus.clone()?;
    Some(layer(panel, focus, window))
}

/// A synced message by id, keyed to `session_id` (the fold's owner).
fn message_of(id: &str, session_id: &str, cx: &App) -> Option<MessageRow> {
    let store = sync::Store::try_global(cx)?;
    let message = store.collections().messages.read(cx).get(id).cloned()?;
    Some(MessageRow { session_id: session_id.to_string(), ..message })
}

/// A side arrow OUTSIDE the modal: a press on it steps and never reaches the
/// scrim (#EXP-1432: only a press on the BARE scrim closes).
fn side_arrow(id: &str, icon: crate::icons::ExpIcon, label: &str, forward: bool) -> gpui::Stateful<Div> {
    div()
        .id(SharedString::from(format!("{id}-hit")))
        .flex_shrink_0()
        .rounded_full()
        .bg(theme::tokens::glass::FILL_ROW.to_hsla())
        // The scrim closes on mouse DOWN: swallow it here first.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            Button::new(SharedString::from(id.to_string()))
                .ghost()
                .rounded_full()
                .cursor_pointer()
                .icon(Icon::new(icon))
                .tooltip(label.to_string())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if forward {
                        next(cx);
                    } else {
                        previous(cx);
                    }
                }),
        )
}

/// The scrim, the panel and the side arrows over the whole window.
fn layer(panel: gpui::Stateful<Div>, focus: FocusHandle, window: &mut Window) -> AnyElement {
    let viewport = window.viewport_size();
    let spec = &STORIES.viewer;
    let height = px(spec.height).min(viewport.height - px(32.));
    let width = px(spec.width).min(viewport.width - px(128.)).max(px(280.));
    let key_focus = focus.clone();
    deferred(
        anchored().position(point(px(0.), px(0.))).child(
            div()
                .id("story-viewer-layer")
                .relative()
                .w(viewport.width)
                .h(viewport.height)
                .occlude()
                .child(
                    div()
                        .id("story-viewer-scrim")
                        .absolute()
                        .inset_0()
                        .bg(gpui::black().opacity(0.6))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| close(cx)),
                )
                .child(
                    h_flex()
                        .absolute()
                        .inset_0()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child(side_arrow(&STORIES.test_ids.previous, registry::UI_CHEVRON_LEFT, &STORIES.copy.previous, false))
                        .child(
                            panel
                                .track_focus(&focus)
                                .key_context("StoryViewer")
                                .on_key_down(move |event, window, cx| {
                                    let typing = !key_focus.is_focused(window);
                                    match event.keystroke.key.as_str() {
                                        "escape" => {
                                            cx.stop_propagation();
                                            close(cx);
                                        }
                                        "right" if !typing => {
                                            cx.stop_propagation();
                                            next(cx);
                                        }
                                        "left" if !typing => {
                                            cx.stop_propagation();
                                            previous(cx);
                                        }
                                        _ => {}
                                    }
                                })
                                // The panel hides the scrim under it (a press on
                                // it never closes) and keeps its own taps.
                                .occlude()
                                .w(width)
                                .h(height)
                                .rounded(px(spec.radius))
                                .border_1()
                                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                .shadow_2xl(),
                        )
                        .child(side_arrow(&STORIES.test_ids.next, registry::UI_CHEVRON_RIGHT, &STORIES.copy.next, true)),
                ),
        ),
    )
    .with_priority(1)
    .into_any_element()
}

// ---------------------------------------------------------------------------
// The panel (the overlay's and the styleguide's)
// ---------------------------------------------------------------------------

type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// The panel's callbacks (the styleguide passes none).
#[derive(Default)]
pub(crate) struct PanelHandlers {
    /// A click on the avatar or the name.
    pub open_session: Option<Action>,
    /// The close glyph.
    pub close: Option<Action>,
    /// A press on the thirds steps (every frame, off the interactive
    /// elements) and a held press pauses.
    pub navigate: bool,
}

/// A frame's body and footer.
pub(crate) struct StoryContent {
    /// `true` = `body` is a picture filling the panel on black, the footer
    /// floats over it on the bottom scrim; `false` = `body` scrolls under the
    /// top band, the footer sits under it.
    pub fill: bool,
    pub body: AnyElement,
    pub footer: Option<AnyElement>,
}

/// THE progress (eUI `StoryProgress`'s look): one segment per FRAME,
/// `progress.height` tall; `groups[i]` = segment i's story: segments of one
/// story sit `progress.gap` apart, a new story starts `progress.storyGap`
/// after the last (`None` = one group). Done = foreground, the current = its
/// fill fraction, todo = the glass-stroke track.
pub(crate) fn story_progress(
    count: usize,
    index: usize,
    fraction: f32,
    groups: Option<&[usize]>,
    cx: &App,
) -> gpui::Stateful<Div> {
    let spec = &STORIES.progress;
    let foreground = cx.theme().foreground;
    h_flex()
        .id(SharedString::from(STORIES.test_ids.progress.clone()))
        .w_full()
        .children((0..count).map(|at| {
            let fill = if at < index {
                1.
            } else if at == index {
                fraction.clamp(0., 1.)
            } else {
                0.
            };
            let lead = match groups {
                _ if at == 0 => 0.,
                Some(groups) if groups.get(at) != groups.get(at - 1) => spec.story_gap,
                _ => spec.gap,
            };
            div()
                .relative()
                .flex_1()
                .ml(px(lead))
                .h(px(spec.height))
                .rounded_full()
                .overflow_hidden()
                .bg(theme::tokens::glass::STROKE_CARD.to_hsla())
                .child(div().absolute().top_0().left_0().h_full().w(relative(fill)).rounded_full().bg(foreground))
        }))
}

/// The header's ring (px): the avatar inside its StoryRing.
pub(crate) const HEADER_RING: f32 = 36.;
/// The mark inside the header ring (half its inner disc).
pub(crate) const HEADER_GLYPH: f32 = 12.;

/// The floating band's fill behind its text (the scrim's `0.8`–`0.88`).
fn scrim_fill() -> Hsla {
    gpui::black().opacity(0.84)
}

/// The frame's title (one line, semibold) and description (muted, up to 3
/// lines), each once; `ghost` = the same text, invisible (the scrolling
/// body's spacer under the floating band).
fn title_block(frame: &StoryFrame, ghost: bool, cx: &App) -> Option<Div> {
    if frame.title.is_none() && frame.description.is_none() {
        return None;
    }
    let (title, muted) = if ghost {
        (gpui::transparent_black(), gpui::transparent_black())
    } else {
        (cx.theme().foreground, cx.theme().muted_foreground)
    };
    Some(
        v_flex()
            .min_w_0()
            .gap_1()
            .px(px(2.))
            .when_some(frame.title.clone(), |block, text| {
                block.child(
                    div()
                        .id("story-title")
                        .w_full()
                        .truncate()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(title)
                        .child(SharedString::from(text)),
                )
            })
            .when_some(frame.description.clone(), |block, text| {
                block.child(
                    div()
                        .id("story-description")
                        .w_full()
                        .line_clamp(3)
                        .text_sm()
                        .text_color(muted)
                        .child(SharedString::from(text)),
                )
            }),
    )
}

/// THE panel: the frame's body on black, the progress + header + title +
/// description floating over the top scrim, the footer under it (over the
/// bottom scrim on a picture). `avatar` sits inside the ring's StoryRing at
/// the header size (stories.json VIEWER LOOK).
#[allow(clippy::too_many_arguments)]
pub(crate) fn story_panel(
    ring: &StoryRing,
    frames: &[StoryFrame],
    index: usize,
    fraction: f32,
    avatar: AnyElement,
    content: StoryContent,
    handlers: PanelHandlers,
    cx: &App,
) -> gpui::Stateful<Div> {
    let theme = cx.theme();
    let frame = &frames[index.min(frames.len() - 1)];
    let story = &ring.stories[frame.story];
    let groups: Vec<usize> = frames.iter().map(|frame| frame.story).collect();
    let arcs: Vec<crate::story_ring::RingArc> = ring.stories.iter().map(crate::story_ring::RingArc::from).collect();
    let header_open = handlers.open_session.clone();
    let close_action = handlers.close.clone();
    let header = h_flex()
        .id(SharedString::from(STORIES.test_ids.header.clone()))
        .min_w_0()
        .items_center()
        .gap_2p5()
        .child(
            h_flex()
                .id("story-header-session")
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_2p5()
                .rounded_md()
                .when(header_open.is_some(), |this| {
                    this.cursor_pointer().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                })
                .on_click(move |_, window, cx| {
                    if let Some(action) = &header_open {
                        cx.stop_propagation();
                        action(window, cx);
                    }
                })
                .child(crate::story_ring::story_ring(&arcs, HEADER_RING, avatar, cx))
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(story.session_name.clone())),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(SharedString::from(story.meta.clone())),
                        ),
                ),
        )
        .child(
            div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
            Button::new("story-close")
                .ghost()
                .small()
                .cursor_pointer()
                .icon(Icon::new(registry::UI_CLOSE).text_color(theme.muted_foreground))
                .tooltip(STORIES.copy.close.clone())
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(action) = &close_action {
                        action(window, cx);
                    }
                }),
            ),
        );
    let spec = &STORIES.viewer;
    let fade = px(spec.scrim_fade);
    // The top band: progress, header, title + description over the scrim
    // (near-opaque behind the text, clear over its last `scrimFade` px).
    let top = v_flex()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .child(
            v_flex()
                .id("story-top")
                .min_h(px(spec.scrim_height - spec.scrim_fade))
                .gap_3()
                .px_3()
                .pt_3()
                .pb_3()
                .bg(scrim_fill())
                .child(story_progress(frames.len(), index, fraction, Some(&groups), cx))
                .child(header)
                .children(title_block(frame, false, cx)),
        )
        .child(div().h(fade).bg(linear_gradient(
            180.,
            linear_color_stop(scrim_fill(), 0.),
            linear_color_stop(gpui::black().opacity(0.), 1.),
        )));
    let StoryContent { fill, body, footer } = content;
    let base = div()
        .id(SharedString::from(STORIES.test_ids.viewer.clone()))
        .relative()
        .overflow_hidden()
        .bg(gpui::black())
        .text_color(theme.foreground)
        // The panel's bounds (the tap thirds are measured against them).
        .child(
            gpui::canvas(
                |bounds, _, cx| {
                    if let Some(viewer) = cx.try_global::<Viewer>() {
                        if viewer.panel_bounds != Some(bounds) {
                            cx.global_mut::<Viewer>().panel_bounds = Some(bounds);
                        }
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .when(handlers.navigate, tap_to_step);
    // The footer floats over the bottom on the mirrored scrim (clear over
    // its first `scrimFade` px), on EVERY frame; its height is measured so a
    // scrolling body ends clear of it.
    let footer_key = story.id.clone();
    let footer_height = cx
        .try_global::<Viewer>()
        .and_then(|viewer| viewer.footer_heights.get(&footer_key).copied())
        .unwrap_or(0.);
    let bottom = footer.map(|footer| {
        v_flex()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .child(div().h(fade).bg(linear_gradient(
                180.,
                linear_color_stop(gpui::black().opacity(0.), 0.),
                linear_color_stop(scrim_fill(), 1.),
            )))
            .child(
                v_flex()
                    .id("story-footer")
                    .relative()
                    .max_h(px(spec.height * 0.55))
                    .bg(scrim_fill())
                    .occlude()
                    .cursor_default()
                    // THE footer is interactive: a press on it never steps.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(footer)
                    .child(
                        gpui::canvas(
                            move |bounds, window, cx| {
                                let height = f32::from(bounds.size.height);
                                let viewer = cx.default_global::<Viewer>();
                                if viewer.footer_heights.get(&footer_key).is_none_or(|at| (at - height).abs() > 0.5) {
                                    viewer.footer_heights.insert(footer_key.clone(), height);
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
    });
    if fill {
        // A picture: edge to edge on black.
        let picture = div().id("story-body").absolute().inset_0().child(body);
        return base.child(picture).child(top).children(bottom);
    }
    // A surface / the plan text / a bare title: the body scrolls from under
    // the band (an invisible twin of its text keeps the room) and ends clear
    // of the floating footer (its measured height).
    let spacer = v_flex()
        .flex_shrink_0()
        .gap_3()
        .px_3()
        .pt_3()
        .pb(px(12. + 8.))
        .child(div().h(px(STORIES.progress.height)))
        .child(div().h(px(HEADER_RING)))
        .children(title_block(frame, true, cx));
    let has_footer = bottom.is_some();
    base.child(
        v_flex()
            .absolute()
            .inset_0()
            .child(spacer)
            .child(div().id("story-body").flex_1().min_h_0().flex().flex_col().child(body))
            .when(has_footer, |column| column.child(div().flex_shrink_0().h(px(footer_height)))),
    )
    .child(top)
    .children(bottom)
}

/// Tap to step on EVERY frame (web `StoryViewerPanel` pointer handlers): a
/// press that no interactive element claimed (they stop the mouse-down: the
/// footer, the header's controls, eUI's pressable nodes, the side arrows)
/// and that moved the keyboard focus nowhere (a text field took it) is a
/// tap: the left third = previous, the right third = next; a held press
/// pauses the timer and never steps; a drag never steps.
fn tap_to_step(panel: gpui::Stateful<Div>) -> gpui::Stateful<Div> {
    panel
        .cursor_pointer()
        .capture_any_mouse_down(|_, window, cx| {
            let focused = window.focused(cx);
            cx.default_global::<Viewer>().press_focus = Some(focused);
        })
        .on_mouse_down(MouseButton::Left, |event, window, cx| {
            let viewer = cx.default_global::<Viewer>();
            let before = viewer.press_focus.take().flatten();
            let own = viewer.focus.clone();
            let now = window.focused(cx);
            if now != before {
                // A child took the focus (a text field): not a tap.
                return;
            }
            // A bare press while a field holds the keys hands them back.
            if let Some(own) = own.filter(|own| now.as_ref() != Some(own)) {
                window.focus(&own, cx);
            }
            if let Some(open) = cx.default_global::<Viewer>().open.as_mut() {
                open.pressed_at = Some((Instant::now(), event.position));
            }
            pause(true, cx);
        })
        .on_mouse_up(MouseButton::Left, |event, _, cx| {
            let pressed = cx.default_global::<Viewer>().open.as_mut().and_then(|open| open.pressed_at.take());
            let Some((at, from)) = pressed else { return };
            pause(false, cx);
            let moved = (f32::from(event.position.x - from.x)).abs() > 10. || (f32::from(event.position.y - from.y)).abs() > 10.;
            if at.elapsed() >= HOLD || moved {
                return;
            }
            let Some(bounds) = cx.global::<Viewer>().panel_bounds else { return };
            let width = f32::from(bounds.size.width).max(1.);
            let x = (f32::from(event.position.x - bounds.origin.x)) / width;
            if x < 1. / 3. {
                previous(cx);
            } else if x > 2. / 3. {
                next(cx);
            }
        })
        .on_mouse_up_out(MouseButton::Left, |_, _, cx| {
            let pressed = cx.default_global::<Viewer>().open.as_mut().and_then(|open| open.pressed_at.take());
            if pressed.is_some() {
                pause(false, cx);
            }
        })
}

/// One frame's body and footer: a picture fitted on black; a result's
/// surface bare (no card chrome); a plan's markdown; and, for a plan /
/// question, THE card sheet (its pages and options; its answer footer only
/// on the story's last frame while it waits).
pub(crate) fn story_content(
    story: &Story,
    frame: &StoryFrame,
    message: Option<&MessageRow>,
    text: &ThreadText,
    images: &Entity<ImageCache>,
    window: &mut Window,
    cx: &mut App,
) -> StoryContent {
    let row = message.and_then(|message| match story.kind {
        StoryKind::Result => domain::session_tree::surface_card_row(message),
        _ => domain::session_tree::ask_card_row(message),
    });
    let picture = match (frame.kind, frame.picture) {
        (StoryFrameKind::Picture, Some(index)) => story.pictures.get(index),
        _ => None,
    };
    // The plan text frame's markdown (it already starts with its title).
    let markdown = (frame.kind == StoryFrameKind::Text)
        .then_some(story.text.as_deref())
        .flatten()
        .map(|markdown| {
            let view = crate::thread_screen::message_markdown(
                SharedString::from(format!("story-text-{}", story.id)),
                &crate::plan_card::strip_pending_uploads(markdown),
                text,
                images,
            );
            crate::steer_viewer::ask_body_text(cx).child(view).into_any_element()
        });
    let answering = frame.footer != StoryFooter::None;
    // THE card sheet of a plan / question: every non-picture frame (its
    // body IS the sheet) and a picture frame that carries the footer.
    let sheet = if story.kind != StoryKind::Result && (picture.is_none() || answering) {
        let lead = markdown.unwrap_or_else(|| div().into_any_element());
        match &row {
            Some(row @ SessionThreadRow::Plan { .. }) => crate::thread_screen::plan_facts(row).map(|(_, _, facts)| {
                crate::dock::sheet_host(|| crate::plan_card::plan_story_sheet(&facts, lead, images, window, cx))
            }),
            Some(row @ SessionThreadRow::Question { .. }) => {
                crate::thread_screen::question_facts(row).map(|(_, _, facts)| {
                    crate::dock::sheet_host(|| crate::plan_card::question_story_sheet(&facts, lead, images, window, cx))
                })
            }
            _ => None,
        }
    } else {
        None
    };
    if let Some(picture) = picture {
        let footer = sheet.map(|sheet| {
            v_flex()
                .w_full()
                .min_h_0()
                .child(sheet.body)
                .children(sheet.footer.filter(|_| answering).map(footer_pad))
                .into_any_element()
        });
        return StoryContent { fill: true, body: picture_fill(picture, images, cx), footer };
    }
    if let Some(sheet) = sheet {
        return StoryContent { fill: false, body: sheet.body, footer: sheet.footer.filter(|_| answering).map(footer_pad) };
    }
    let body = match (&row, frame.kind) {
        (Some(SessionThreadRow::Surface { id, session_id, surface, .. }), StoryFrameKind::Surface) => {
            let surface = crate::surface_card::surface_bare(id, session_id, surface, window, cx);
            crate::dock::sheet_body("story-body-scroll", surface)
        }
        (_, StoryFrameKind::Text) => crate::dock::sheet_body(
            "story-body-scroll",
            text_or_empty(story, text, images, cx),
        ),
        _ => div().into_any_element(),
    };
    StoryContent { fill: false, body, footer: None }
}

/// The floating footer's inset (web `px-4 pt-3 pb-4`).
fn footer_pad(footer: AnyElement) -> AnyElement {
    div().w_full().px_4().pt_3().pb_4().child(footer).into_any_element()
}

/// A text frame's markdown without a card behind it (a settled plan).
fn text_or_empty(story: &Story, text: &ThreadText, images: &Entity<ImageCache>, cx: &App) -> AnyElement {
    match story.text.as_deref() {
        Some(markdown) => crate::steer_viewer::ask_body_text(cx)
            .child(crate::thread_screen::message_markdown(
                SharedString::from(format!("story-text-{}", story.id)),
                &crate::plan_card::strip_pending_uploads(markdown),
                text,
                images,
            ))
            .into_any_element(),
        None => div().into_any_element(),
    }
}

/// A picture frame: the picture fitted (contain) filling the panel on black.
fn picture_fill(picture: &domain::stories::StoryPicture, images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
    let url = format!("/api/attachments/{}", picture.attachment_id);
    let image = match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
        ImageSlot::Ready(image) => img(image).size_full().object_fit(ObjectFit::Contain).into_any_element(),
        _ => div().size_full().into_any_element(),
    };
    div()
        .id(SharedString::from(format!("story-picture-{}", picture.attachment_id)))
        .size_full()
        .child(image)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_rings() -> Vec<StoryRing> {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/stories.json")).unwrap();
        // Two rings out of the `frameCases` mix (its story list, split).
        let case = fixture["frameCases"]
            .as_array()
            .unwrap()
            .iter()
            .max_by_key(|case| case["input"]["stories"].as_array().map_or(0, Vec::len))
            .unwrap();
        let stories: Vec<Story> = serde_json::from_value(case["input"]["stories"].clone()).unwrap();
        let (first, second) = stories.split_at(2);
        let ring = |key: &str, stories: &[Story]| StoryRing {
            key: key.to_string(),
            agent_id: None,
            root_id: None,
            title: key.to_string(),
            assistant: false,
            waiting: 0,
            unseen: false,
            stories: stories.to_vec(),
        };
        vec![ring("a", first), ring("b", second)]
    }

    fn place(rings: &[StoryRing], ring: usize, at: usize) -> Place {
        Place { ring, frames: story_frames(&rings[ring].stories), at }
    }

    /// #EXP-1432 NAVIGATION (frames): within a ring frame by frame, across
    /// rings at the ends, past the very last = close, before the very first
    /// = stay.
    #[test]
    fn steps_walk_frames_across_rings() {
        let rings = fixture_rings();
        let first = story_frames(&rings[0].stories).len();
        assert!(first > 2, "the picture story gives the first ring several frames");
        assert_eq!(step_frame(&rings, &place(&rings, 0, 0), true), Step::To(0, 1));
        assert_eq!(step_frame(&rings, &place(&rings, 0, first - 1), true), Step::To(1, 0));
        assert_eq!(step_frame(&rings, &place(&rings, 1, 0), false), Step::To(0, first - 1));
        assert_eq!(step_frame(&rings, &place(&rings, 0, 0), false), Step::Stay);
        let last = story_frames(&rings[1].stories).len() - 1;
        assert_eq!(step_frame(&rings, &place(&rings, 1, last), true), Step::Close);
    }
}
