//! S-3 STORIES (`fixtures/stories.json` STORY VIEWER; web
//! `packages/ui/src/stories.tsx` `StoryViewer`): THE story viewer of the
//! IDE: a centred modal (`viewer.radius`) over a scrim, #EXP-1463 SIZED BY
//! ITS FRAME ([`story_viewer_size`]: a gallery's CURRENT picture or a plan's
//! cover fitted into the window less `viewer.inset`, else `viewer.width` ×
//! `viewer.height` capped), layered over the window from the Chats page and
//! a session's Stories tab.
//!
//! #EXP-1493: the viewer steps ONE FRAME PER STORY ([`story_frames`]): a
//! plan = its COVER, a picture story = ONE gallery frame (its pictures
//! stepped inside it), else its surface, else the bare title.
//! - Floating over a top black-to-clear scrim (`viewer.scrimHeight`, its
//!   last `viewer.scrimFade` px fading out): THE progress ([`story_progress`],
//!   one segment per STORY, `progress.gap` apart; a gallery's current
//!   segment fills across its pictures), the header (the ring's avatar, the
//!   story's `sessionName`, its `meta` muted, the close glyph; a click on the
//!   avatar or the name opens that session and closes the viewer), then THE
//!   kind badge leading the frame's title (one line) and the description
//!   (muted, up to 3 lines), each ONCE.
//! - Body: a PLAN = its cover (the first picture fitted on black, none = the
//!   bare black body; a click ANYWHERE on the cover = Open plan: its
//!   prev/next stay on the arrows and the keys); a GALLERY =
//!   the current picture fitted on black, the `i / n` counter top-right, its
//!   caption (else its label) muted under it; a surface frame = THE surface
//!   renderer with no card chrome; a title frame = nothing. A scrolling body
//!   ends clear of the footer.
//! - Footer, floating over the bottom on the mirrored scrim: a plan = the
//!   counts line, up to 4 thumbnails (+N) and ONE full-width `Open plan`
//!   (THE plan page, [`crate::plan_page::open`]; the viewer closes, a story
//!   never decides a plan); a gallery = its filmstrip (n > 1); a question's
//!   answer controls are THE card's
//!   ([`crate::plan_card::question_story_sheet`]), answered through THE
//!   card-answer path (`sessions.answer`); a result has none else.
//! - Navigation: → / the side arrow / a click on the right third (never on
//!   an interactive element: [`tap_to_step`]) = a gallery's next PICTURE,
//!   past its last = the next story (past a ring's last = the next ring,
//!   past the last ring = close); ← / the left arrow / the left third = back
//!   the same way; Escape, the close glyph or a press on the BARE scrim =
//!   close (a press on a side arrow steps, never closes). A result's timer
//!   steps its pictures every `autoAdvanceMs`, then the story (a decision's
//!   never moves on its own); holding a press pauses it.
//! - Every story SHOWN raises the viewer's stories mark of its root
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

use domain::plan_page::{plan_counts_line, PLAN_PAGE};
use domain::rows::MessageRow;
use domain::session_tree::SessionThreadRow;
use domain::stories::{
    story_frame_index, story_frames, story_viewer_size, Story, StoryFooter, StoryFrame, StoryFrameKind, StoryKind,
    StoryPicture, StoryRing, STORIES,
};

use crate::icons::registry;
use crate::markdown::{ImageCache, ImageSlot};

/// A press held this long pauses the timer instead of navigating.
const HOLD: Duration = Duration::from_millis(250);
/// A plan cover's thumbnails: at most this many, then `+N` (stories.json
/// STORY VIEWER).
const COVER_THUMBS: usize = 4;
/// A cover thumbnail's size (px: 40 tall, web `h-10 w-14`).
const COVER_THUMB: (f32, f32) = (56., 40.);
/// A gallery filmstrip square (px).
const FILMSTRIP_THUMB: f32 = 52.;

/// The viewer's place and timer.
struct Open {
    team_id: String,
    /// The Stories tab's root when opened from a session screen's tab (its
    /// ONE ring, `domain::stories::story_tab`); `None` = the team's rings.
    tab_root: Option<String>,
    ring_key: String,
    story_id: String,
    /// A gallery's picture on show (0 otherwise).
    picture: usize,
    /// The story's index when last resolved (an answered story drops out:
    /// the viewer stays at the same place in the ring).
    story_index: usize,
    /// When the current picture / story came on (its timer).
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
    /// and a gallery's caption end clear of it).
    footer_heights: HashMap<String, f32>,
    /// The focus when the current press began (capture phase): a press that
    /// moved it went to a text field, never a tap.
    press_focus: Option<Option<FocusHandle>>,
}

impl gpui::Global for Viewer {}

/// Opens THE viewer at `ring_key`'s first story.
pub(crate) fn open(team_id: &str, ring_key: &str, window: &mut Window, cx: &mut App) {
    open_with(team_id, None, ring_key, None, 0, window, cx);
}

/// Opens THE viewer on a session screen's Stories tab ring (stories.json
/// STORIES TAB) at `story_id`'s frame, a gallery at picture `picture` (a
/// tile = its first, a thumbnail = that picture, `storyFrameIndex`).
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

/// The steps inside a frame: a gallery's pictures, else one.
fn picture_count(frame: &StoryFrame) -> usize {
    match frame.kind {
        StoryFrameKind::Gallery => frame.pictures.max(1),
        _ => 1,
    }
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
    let hit = story_id.and_then(|id| story_frame_index(&frames, id));
    let Some(frame) = frames.get(hit.unwrap_or(0)) else {
        return;
    };
    // A thumbnail opens its story's frame with the gallery at that picture.
    let picture = if hit.is_some() { picture.min(picture_count(frame) - 1) } else { 0 };
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
        picture,
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

/// Where the viewer stands: the ring's index, its frames (one per story),
/// the story's frame and a gallery's picture.
struct Place {
    ring: usize,
    frames: Vec<StoryFrame>,
    at: usize,
    picture: usize,
}

/// The open story's place in `rings` (an answered story = the frame at the
/// same story index, clamped; a ring that is gone = `None`); the picture is
/// clamped to the frame's.
fn resolve(open: &Open, rings: &[StoryRing]) -> Option<Place> {
    let ring = rings.iter().position(|ring| ring.key == open.ring_key)?;
    let frames = story_frames(&rings[ring].stories);
    if frames.is_empty() {
        return None;
    }
    let at = story_frame_index(&frames, &open.story_id)
        .unwrap_or_else(|| open.story_index.min(frames.len() - 1));
    let picture = if frames[at].story_id == open.story_id { open.picture.min(picture_count(&frames[at]) - 1) } else { 0 };
    Some(Place { ring, frames, at, picture })
}

/// A step over the stories (and a gallery's pictures) of every ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// (ring index, story index in that ring, picture).
    To(usize, usize, usize),
    Close,
    Stay,
}

/// NAVIGATION (#EXP-1493): forward = a gallery's next picture, past its last
/// = the next story, past a ring's last = the next ring's first, past the
/// last ring = close; back = the previous picture, a gallery's first = the
/// previous story, a ring's first = the previous ring's last, the very
/// first stays put. A story is entered at its first picture.
fn step_place(rings: &[StoryRing], place: &Place, forward: bool) -> Step {
    let count = picture_count(&place.frames[place.at]);
    if forward {
        if place.picture + 1 < count {
            return Step::To(place.ring, place.at, place.picture + 1);
        }
        if place.at + 1 < place.frames.len() {
            return Step::To(place.ring, place.at + 1, 0);
        }
        return (place.ring + 1..rings.len())
            .find(|&next| !rings[next].stories.is_empty())
            .map_or(Step::Close, |next| Step::To(next, 0, 0));
    }
    if place.picture > 0 {
        return Step::To(place.ring, place.at, place.picture - 1);
    }
    if place.at > 0 {
        return Step::To(place.ring, place.at - 1, 0);
    }
    (0..place.ring)
        .rev()
        .find(|&prev| !rings[prev].stories.is_empty())
        .map_or(Step::Stay, |prev| Step::To(prev, rings[prev].stories.len() - 1, 0))
}

/// Moves to `to`, restarting the timer (`Close` closes).
fn go(to: Step, rings: &[StoryRing], cx: &mut App) {
    let viewer = cx.default_global::<Viewer>();
    match (to, viewer.open.as_mut()) {
        (Step::Stay, _) => return,
        (Step::To(ring, at, picture), Some(open)) => {
            let target = &rings[ring];
            let Some(story) = target.stories.get(at) else { return };
            open.ring_key = target.key.clone();
            open.story_id = story.id.clone();
            open.story_index = at;
            open.picture = picture;
            open.shown_at = Instant::now();
            open.paused_at = None;
            open.pressed_at = None;
        }
        _ => viewer.open = None,
    }
    cx.refresh_windows();
}

/// Next picture / story (past the last ring = close).
pub(crate) fn next(cx: &mut App) {
    step(true, cx);
}

/// Previous picture / story (the very first stays put).
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
    let to = step_place(&rings, &place, forward);
    go(to, &rings, cx);
}

/// A gallery's filmstrip pick: that picture, its timer restarted.
fn pick_picture(picture: usize, cx: &mut App) {
    if let Some(open) = cx.default_global::<Viewer>().open.as_mut() {
        open.picture = picture;
        open.shown_at = Instant::now();
        open.paused_at = None;
        open.pressed_at = None;
    }
    cx.refresh_windows();
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

/// The picture that sizes the modal and fills the body: a gallery's current
/// one, a plan's cover (its first); else none.
fn shown_picture<'a>(story: &'a Story, frame: &StoryFrame, picture: usize) -> Option<&'a StoryPicture> {
    match frame.kind {
        StoryFrameKind::Gallery => story.pictures.get(picture),
        StoryFrameKind::Plan => story.pictures.first(),
        _ => None,
    }
}

/// THE viewer as a layer over the window, while open: resolves the frame,
/// runs its timer, marks its story seen and paints the panel.
pub(crate) fn overlay(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let mut rings = open_rings(cx)?;
    let mut place = cx.global::<Viewer>().open.as_ref().and_then(|open| resolve(open, &rings));
    // A result past its time moves on: a gallery's next picture, then the
    // next story (a decision's never does).
    if let Some(current) = place.as_ref() {
        let open = cx.global::<Viewer>().open.as_ref()?;
        if let (Some(ms), None) = (current.frames[current.at].auto_advance_ms, open.paused_at) {
            if open.shown_at.elapsed() >= Duration::from_millis(ms) {
                let to = step_place(&rings, current, true);
                go(to, &rings, cx);
                rings = open_rings(cx)?;
                place = cx.global::<Viewer>().open.as_ref().and_then(|open| resolve(open, &rings));
            }
        }
    }
    let Some(Place { ring: ring_index, frames, at, picture }) = place else {
        cx.default_global::<Viewer>().open = None;
        return None;
    };
    let ring = &rings[ring_index];
    let frame = &frames[at];
    let story = &ring.stories[frame.story];
    {
        let open = cx.global_mut::<Viewer>().open.as_mut()?;
        open.story_id = frame.story_id.clone();
        open.story_index = frame.story;
        open.picture = picture;
    }
    mark_seen(story, cx);
    let (fraction, timing) = {
        let open = cx.global::<Viewer>().open.as_ref()?;
        match frame.auto_advance_ms {
            Some(ms) => {
                let elapsed = open.paused_at.unwrap_or_else(Instant::now).duration_since(open.shown_at);
                let own = (elapsed.as_secs_f32() * 1000. / ms as f32).clamp(0., 1.);
                // A gallery fills its segment across its pictures.
                ((picture as f32 + own) / picture_count(frame) as f32, open.paused_at.is_none())
            }
            None => (1., false),
        }
    };
    if timing {
        window.request_animation_frame();
    }
    let images = images(cx);
    // #EXP-1463: the modal takes its frame's size (a gallery's current
    // picture / a plan's cover: the card's size, else the synced
    // attachment's, else the loaded bytes').
    let picture_size = shown_picture(story, frame, picture).map(|shown| picture_size(shown, &images, cx));
    let viewport = window.viewport_size();
    let size = story_viewer_size(picture_size, (f32::from(viewport.width) as f64, f32::from(viewport.height) as f64));
    let message = message_of(&story.id, &story.session_id, cx);
    let avatar = crate::story_ring::store_ring_avatar(ring, HEADER_GLYPH, cx);
    let session_id = story.session_id.clone();
    let (plan_session, plan_message) = (story.session_id.clone(), story.id.clone());
    let handlers = PanelHandlers {
        open_session: Some(Rc::new(move |window, cx| {
            close(cx);
            crate::thread_screen::open_session_card(&session_id, window, cx);
        })),
        close: Some(Rc::new(|_, cx| close(cx))),
        open_plan: Some(Rc::new(move |window, cx| {
            close(cx);
            crate::plan_page::open(&plan_session, &plan_message, window, cx);
        })),
        pick_picture: Some(Rc::new(|picture, _, cx| pick_picture(picture, cx))),
        navigate: true,
    };
    let content = story_content(story, frame, picture, message.as_ref(), &handlers, &images, window, cx);
    let panel = story_panel(ring, &frames, at, picture, fraction, avatar, content, handlers, cx);
    let focus = cx.global::<Viewer>().focus.clone()?;
    Some(layer(panel, focus, (px(size.width as f32), px(size.height as f32)), window))
}

/// A picture's pixel size for [`story_viewer_size`]: the card's, else the
/// synced attachment row's, else the loaded image's natural size (`None`s
/// while unknown: the fixed frame).
fn picture_size(picture: &StoryPicture, images: &Entity<ImageCache>, cx: &mut App) -> (Option<f64>, Option<f64>) {
    if let (Some(width), Some(height)) = (picture.width, picture.height) {
        if width > 0 && height > 0 {
            return (Some(width as f64), Some(height as f64));
        }
    }
    let url = format!("/api/attachments/{}", picture.attachment_id);
    let natural = crate::markdown::attachment_natural_size(&url, cx).or_else(|| {
        match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
            ImageSlot::Ready(image) => image.decoded_size(),
            _ => None,
        }
    });
    match natural {
        Some((width, height)) => (Some(width as f64), Some(height as f64)),
        None => (None, None),
    }
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

/// The scrim, the panel (`size`: [`story_viewer_size`]) and the side
/// arrows over the whole window.
fn layer(panel: gpui::Stateful<Div>, focus: FocusHandle, size: (gpui::Pixels, gpui::Pixels), window: &mut Window) -> AnyElement {
    let viewport = window.viewport_size();
    let spec = &STORIES.viewer;
    let (width, height) = size;
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
    /// #EXP-1493: a plan's `Open plan` (and a click on its cover): THE plan
    /// page, the viewer closes.
    pub open_plan: Option<Action>,
    /// A gallery filmstrip thumbnail: that picture.
    pub pick_picture: Option<Rc<dyn Fn(usize, &mut Window, &mut App)>>,
    /// A press on the thirds steps (every frame, off the interactive
    /// elements) and a held press pauses.
    pub navigate: bool,
}

/// A frame's body and footer.
pub(crate) struct StoryContent {
    /// `true` = `body` fills the panel on black (a plan's cover, a gallery's
    /// picture), the footer floats over it on the bottom scrim; `false` =
    /// `body` scrolls under the top band, the footer sits under it.
    pub fill: bool,
    pub body: AnyElement,
    pub footer: Option<AnyElement>,
}

/// THE progress (eUI `StoryProgress`'s look, #EXP-1493): one segment per
/// STORY, `progress.gap` apart, `progress.height` tall. Done = foreground,
/// the current = its fill fraction (a gallery: (picture + its fraction) /
/// n), todo = the glass-stroke track.
pub(crate) fn story_progress(count: usize, index: usize, fraction: f32, cx: &App) -> gpui::Stateful<Div> {
    let spec = &STORIES.progress;
    let foreground = cx.theme().foreground;
    h_flex()
        .id(SharedString::from(STORIES.test_ids.progress.clone()))
        .w_full()
        .gap(px(spec.gap))
        .children((0..count).map(|at| {
            let fill = if at < index {
                1.
            } else if at == index {
                fraction.clamp(0., 1.)
            } else {
                0.
            };
            div()
                .relative()
                .flex_1()
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

/// THE kind badge leading the frame's title (one line, semibold), then the
/// description (muted, up to 3 lines), each once; `ghost` = the same block,
/// invisible (the room under the floating band).
fn title_block(story: &Story, frame: &StoryFrame, ghost: bool, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .min_w_0()
        .gap_1()
        .px(px(2.))
        .when(ghost, |block| block.opacity(0.))
        .child(
            h_flex()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(crate::story_badge::kind_badge(
                    SharedString::from(format!("story-badge-{}{}", story.id, if ghost { "-ghost" } else { "" })),
                    story.kind,
                    cx,
                ))
                .when_some(frame.title.clone(), |line, text| {
                    line.child(
                        div()
                            .id(if ghost { "story-title-ghost" } else { "story-title" })
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(SharedString::from(text)),
                    )
                }),
        )
        .when_some(frame.description.clone(), |block, text| {
            block.child(
                div()
                    .id(if ghost { "story-description-ghost" } else { "story-description" })
                    .w_full()
                    .line_clamp(3)
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(text)),
            )
        })
}

/// The room the floating top band takes (its invisible twin): a scrolling
/// body and a gallery's counter start under it.
fn band_spacer(story: &Story, frame: &StoryFrame, cx: &App) -> Div {
    v_flex()
        .flex_shrink_0()
        .gap_3()
        .px_3()
        .pt_3()
        .pb(px(12. + 8.))
        .child(div().h(px(STORIES.progress.height)))
        .child(div().h(px(HEADER_RING)))
        .child(title_block(story, frame, true, cx))
}

/// A gallery's `i / n` counter (n > 1): small white digits on a dark pill.
fn gallery_counter(picture: usize, count: usize) -> Div {
    div()
        .flex_shrink_0()
        .px(px(7.))
        .py(px(2.))
        .rounded_full()
        .bg(gpui::black().opacity(0.55))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(gpui::white())
        .child(SharedString::from(format!("{} / {count}", picture + 1)))
}

/// THE panel: the frame's body on black, the progress + header + badge +
/// title + description floating over the top scrim, the footer under it
/// (over the bottom scrim on a cover / picture). `avatar` sits inside the
/// ring's StoryRing at the header size (stories.json VIEWER LOOK).
#[allow(clippy::too_many_arguments)]
pub(crate) fn story_panel(
    ring: &StoryRing,
    frames: &[StoryFrame],
    index: usize,
    picture: usize,
    fraction: f32,
    avatar: AnyElement,
    content: StoryContent,
    handlers: PanelHandlers,
    cx: &App,
) -> gpui::Stateful<Div> {
    let theme = cx.theme();
    let frame = &frames[index.min(frames.len() - 1)];
    let story = &ring.stories[frame.story];
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
    // The top band: progress, header, badge + title + description over the
    // scrim (near-opaque behind the text, clear over its last `scrimFade` px).
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
                .child(story_progress(frames.len(), index, fraction, cx))
                .child(header)
                .child(title_block(story, frame, false, cx)),
        )
        .child(div().h(fade).bg(linear_gradient(
            180.,
            linear_color_stop(scrim_fill(), 0.),
            linear_color_stop(gpui::black().opacity(0.), 1.),
        )));
    let StoryContent { fill, body, footer } = content;
    // A click anywhere on a plan's cover (picture or not) = Open plan (no thirds:
    // the side arrows and the keys still step).
    let cover_open = (frame.kind == StoryFrameKind::Plan)
        .then(|| handlers.open_plan.clone())
        .flatten();
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
        .when(handlers.navigate, |panel| tap_to_step(panel, cover_open));
    // The footer floats over the bottom on the mirrored scrim (clear over
    // its first `scrimFade` px); its height is measured so a scrolling body
    // and a gallery's caption end clear of it.
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
    let has_footer = bottom.is_some();
    if fill {
        // A cover / a picture: edge to edge on black; a gallery's counter
        // sits top-right under the band, its caption over the footer.
        let filled = div().id("story-body").absolute().inset_0().child(body);
        let shown = (frame.kind == StoryFrameKind::Gallery).then(|| story.pictures.get(picture)).flatten();
        let chrome = shown.map(|shown| {
            let count = story.pictures.len();
            v_flex()
                .absolute()
                .inset_0()
                .child(band_spacer(story, frame, cx))
                .when(count > 1, |column| {
                    column.child(
                        h_flex()
                            .id("story-gallery-counter")
                            .w_full()
                            .justify_end()
                            .px_4()
                            .child(gallery_counter(picture, count)),
                    )
                })
                .child(div().flex_1())
                .child(
                    div()
                        .id("story-caption")
                        .w_full()
                        .px_4()
                        .pb(px(8.))
                        .line_clamp(2)
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(SharedString::from(shown.caption.clone().unwrap_or_else(|| shown.label.clone()))),
                )
                .when(has_footer, |column| column.child(div().flex_shrink_0().h(px(footer_height + 12.))))
        });
        return base.child(filled).children(chrome).child(top).children(bottom);
    }
    // A surface / a bare title: the body scrolls from under the band (an
    // invisible twin of its text keeps the room) and ends clear of the
    // floating footer (its measured height).
    base.child(
        v_flex()
            .absolute()
            .inset_0()
            .child(band_spacer(story, frame, cx))
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
/// tap: the left third = previous, the right third = next ([`tap_at`]); on
/// a plan's cover (`cover` set) the WHOLE panel = Open plan
/// (stories.json NAVIGATION: 'a tap ANYWHERE on a plan cover'); a held press
/// pauses the timer and never steps; a drag never steps.
fn tap_to_step(panel: gpui::Stateful<Div>, cover: Option<Action>) -> gpui::Stateful<Div> {
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
        .on_mouse_up(MouseButton::Left, move |event, window, cx| {
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
            match tap_at(x, cover.is_some()) {
                Tap::Previous => previous(cx),
                Tap::Next => next(cx),
                Tap::Cover => {
                    if let Some(action) = &cover {
                        action(window, cx);
                    }
                }
                Tap::Nothing => {}
            }
        })
        .on_mouse_up_out(MouseButton::Left, |_, _, cx| {
            let pressed = cx.default_global::<Viewer>().open.as_mut().and_then(|open| open.pressed_at.take());
            if pressed.is_some() {
                pause(false, cx);
            }
        })
}

/// What a tap at `x` (0..1 across the panel) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tap {
    Previous,
    Next,
    Cover,
    Nothing,
}

/// #EXP-1493: a plan's cover takes the WHOLE panel (Open plan);
/// else the left third = previous, the right third = next, the middle =
/// nothing.
fn tap_at(x: f32, cover: bool) -> Tap {
    if cover {
        Tap::Cover
    } else if x < 1. / 3. {
        Tap::Previous
    } else if x > 2. / 3. {
        Tap::Next
    } else {
        Tap::Nothing
    }
}

/// One frame's body and footer (#EXP-1493): a plan's cover + its Open plan
/// footer; a gallery's current picture + its filmstrip; a result's surface
/// bare (no card chrome); and, for a question, THE card sheet (its options
/// and answer footer while it waits).
#[allow(clippy::too_many_arguments)]
pub(crate) fn story_content(
    story: &Story,
    frame: &StoryFrame,
    picture: usize,
    message: Option<&MessageRow>,
    handlers: &PanelHandlers,
    images: &Entity<ImageCache>,
    window: &mut Window,
    cx: &mut App,
) -> StoryContent {
    if frame.kind == StoryFrameKind::Plan {
        let body = match story.pictures.first() {
            Some(cover) => picture_fill(cover, images, cx),
            None => div().size_full().into_any_element(),
        };
        let footer = footer_pad(plan_footer(story, handlers.open_plan.clone(), images, cx));
        return StoryContent { fill: true, body, footer: Some(footer) };
    }
    let row = message.and_then(|message| match story.kind {
        StoryKind::Result => domain::session_tree::surface_card_row(message),
        _ => domain::session_tree::ask_card_row(message),
    });
    let answering = frame.footer != StoryFooter::None;
    // THE card sheet of a question (its body IS the options).
    let sheet = match &row {
        Some(row @ SessionThreadRow::Question { .. }) if story.kind == StoryKind::Question => {
            crate::thread_screen::question_facts(row).map(|(_, _, facts)| {
                crate::dock::sheet_host(|| {
                    crate::plan_card::question_story_sheet(&facts, div().into_any_element(), images, window, cx)
                })
            })
        }
        _ => None,
    };
    if frame.kind == StoryFrameKind::Gallery {
        let shown = story.pictures.get(picture.min(story.pictures.len().saturating_sub(1)));
        let strip = (story.pictures.len() > 1).then(|| filmstrip(story, picture, handlers.pick_picture.clone(), images, cx));
        let controls = sheet.map(|sheet| {
            v_flex()
                .w_full()
                .min_h_0()
                .child(sheet.body)
                .children(sheet.footer.filter(|_| answering))
                .into_any_element()
        });
        let footer = (strip.is_some() || controls.is_some()).then(|| {
            footer_pad(v_flex().w_full().min_h_0().gap_3().children(strip).children(controls).into_any_element())
        });
        let body = match shown {
            Some(shown) => picture_fill(shown, images, cx),
            None => div().size_full().into_any_element(),
        };
        return StoryContent { fill: true, body, footer };
    }
    if let Some(sheet) = sheet {
        return StoryContent { fill: false, body: sheet.body, footer: sheet.footer.filter(|_| answering).map(footer_pad) };
    }
    let body = match (&row, frame.kind) {
        (Some(SessionThreadRow::Surface { id, session_id, surface, .. }), StoryFrameKind::Surface) => {
            let surface = crate::surface_card::surface_bare(id, session_id, surface, window, cx);
            crate::dock::sheet_body("story-body-scroll", surface)
        }
        _ => div().into_any_element(),
    };
    StoryContent { fill: false, body, footer: None }
}

/// The floating footer's inset (web `px-4 pt-3 pb-4`).
fn footer_pad(footer: AnyElement) -> AnyElement {
    div().w_full().px_4().pt_3().pb_4().child(footer).into_any_element()
}

/// One loaded picture covering its box (cropped), else the muted fill.
fn thumb_image(picture: &StoryPicture, images: &Entity<ImageCache>, cx: &mut App) -> Option<AnyElement> {
    let url = format!("/api/attachments/{}", picture.attachment_id);
    match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
        ImageSlot::Ready(image) => Some(img(image).size_full().object_fit(ObjectFit::Cover).into_any_element()),
        _ => None,
    }
}

/// A thumbnail's box: `tab.tile.radius`, the current one a 2 px foreground
/// ring, else the glass-stroke hairline.
fn thumb_box(current: bool, cx: &App) -> Div {
    let frame = div().flex_shrink_0().rounded(px(STORIES.tab.tile.radius)).overflow_hidden().bg(cx.theme().muted);
    if current {
        frame.border_2().border_color(cx.theme().foreground)
    } else {
        frame.border_1().border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
    }
}

/// A plan cover's footer (#EXP-1493): the counts line, up to 4 thumbnails
/// (the first = current) + `+N` muted, then ONE full-width primary `Open
/// plan` (plan-page `copy.openPlan`); a thumbnail = the same.
fn plan_footer(story: &Story, open_plan: Option<Action>, images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
    let line = story.counts.as_ref().map(plan_counts_line).filter(|line| !line.is_empty());
    let more = story.pictures.len().saturating_sub(COVER_THUMBS);
    let thumbs: Vec<AnyElement> = story
        .pictures
        .iter()
        .take(COVER_THUMBS)
        .enumerate()
        .map(|(at, picture)| {
            let open = open_plan.clone();
            thumb_box(at == 0, cx)
                .id(SharedString::from(format!("story-plan-thumb-{}-{at}", story.id)))
                .w(px(COVER_THUMB.0))
                .h(px(COVER_THUMB.1))
                .when(open.is_some(), |thumb| thumb.cursor_pointer())
                .on_click(move |_, window, cx| {
                    if let Some(open) = &open {
                        open(window, cx);
                    }
                })
                .children(thumb_image(picture, images, cx))
                .into_any_element()
        })
        .collect();
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    v_flex()
        .w_full()
        .gap_3()
        .when_some(line, |column, line| {
            column.child(
                div()
                    .id("story-plan-counts")
                    .text_xs()
                    .text_color(foreground.opacity(0.85))
                    .child(SharedString::from(line)),
            )
        })
        .when(!thumbs.is_empty(), |column| {
            column.child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .children(thumbs)
                    .when(more > 0, |row| {
                        row.child(div().text_xs().text_color(muted).child(SharedString::from(format!("+{more}"))))
                    }),
            )
        })
        .child(
            Button::new(SharedString::from(PLAN_PAGE.test_ids.open_plan.clone()))
                .primary()
                .w_full()
                .cursor_pointer()
                .label(PLAN_PAGE.copy.open_plan.clone())
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(open) = &open_plan {
                        open(window, cx);
                    }
                }),
        )
        .into_any_element()
}

/// A gallery's filmstrip (n > 1): `FILMSTRIP_THUMB` px squares, the current
/// one a 2 px foreground ring; a click = that picture.
fn filmstrip(
    story: &Story,
    current: usize,
    pick: Option<Rc<dyn Fn(usize, &mut Window, &mut App)>>,
    images: &Entity<ImageCache>,
    cx: &mut App,
) -> AnyElement {
    let thumbs: Vec<AnyElement> = story
        .pictures
        .iter()
        .enumerate()
        .map(|(at, picture)| {
            let pick = pick.clone();
            thumb_box(at == current, cx)
                .id(SharedString::from(format!("story-filmstrip-{}-{at}", story.id)))
                .size(px(FILMSTRIP_THUMB))
                .when(pick.is_some(), |thumb| thumb.cursor_pointer())
                .on_click(move |_, window, cx| {
                    if let Some(pick) = &pick {
                        pick(at, window, cx);
                    }
                })
                .children(thumb_image(picture, images, cx))
                .into_any_element()
        })
        .collect();
    h_flex()
        .id(SharedString::from(format!("story-filmstrip-{}", story.id)))
        .w_full()
        .justify_center()
        .gap_1p5()
        .overflow_x_scroll()
        .children(thumbs)
        .into_any_element()
}

/// A picture fitted (contain) filling the panel on black.
fn picture_fill(picture: &StoryPicture, images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
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

    /// The #EXP-1493 `frameCases` mix (a plan with a picture, a question, a
    /// two-picture result, a surface result), split into two rings.
    fn fixture_rings() -> Vec<StoryRing> {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/stories.json")).unwrap();
        let case = fixture["frameCases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["input"]["stories"].as_array().is_some_and(|stories| stories.iter().any(|s| s["id"] == "res-new")))
            .unwrap();
        let stories: Vec<Story> = serde_json::from_value(case["input"]["stories"].clone()).unwrap();
        let (first, second) = stories.split_at(3);
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

    fn place(rings: &[StoryRing], ring: usize, at: usize, picture: usize) -> Place {
        Place { ring, frames: story_frames(&rings[ring].stories), at, picture }
    }

    /// #EXP-1493 NAVIGATION: one step per story; a gallery steps its
    /// pictures first (forward and back), across rings at the ends, past the
    /// very last = close, before the very first = stay.
    #[test]
    fn steps_walk_stories_and_gallery_pictures() {
        let rings = fixture_rings();
        let frames = story_frames(&rings[0].stories);
        assert_eq!(frames.len(), 3, "one frame per story");
        assert_eq!(frames[0].kind, StoryFrameKind::Plan);
        assert_eq!(frames[2].kind, StoryFrameKind::Gallery);
        assert_eq!(frames[2].pictures, 2);
        // A plan's cover is one step even with a picture.
        assert_eq!(step_place(&rings, &place(&rings, 0, 0, 0), true), Step::To(0, 1, 0));
        // The gallery: its next picture first, then the next ring.
        assert_eq!(step_place(&rings, &place(&rings, 0, 2, 0), true), Step::To(0, 2, 1));
        assert_eq!(step_place(&rings, &place(&rings, 0, 2, 1), true), Step::To(1, 0, 0));
        assert_eq!(step_place(&rings, &place(&rings, 0, 2, 1), false), Step::To(0, 2, 0));
        assert_eq!(step_place(&rings, &place(&rings, 0, 2, 0), false), Step::To(0, 1, 0));
        assert_eq!(step_place(&rings, &place(&rings, 1, 0, 0), false), Step::To(0, 2, 0));
        assert_eq!(step_place(&rings, &place(&rings, 0, 0, 0), false), Step::Stay);
        assert_eq!(step_place(&rings, &place(&rings, 1, 0, 0), true), Step::Close);
    }

    /// #EXP-1493: a plan's cover opens the plan from ANY x (the
    /// arrows and keys still step); other frames keep the thirds.
    #[test]
    fn a_plan_cover_opens_the_plan_from_anywhere() {
        for x in [0., 0.1, 0.5, 0.9, 1.] {
            assert_eq!(tap_at(x, true), Tap::Cover);
        }
        assert_eq!(tap_at(0.1, false), Tap::Previous);
        assert_eq!(tap_at(0.5, false), Tap::Nothing);
        assert_eq!(tap_at(0.9, false), Tap::Next);
    }

    /// The modal sizes by a gallery's current picture / a plan's cover.
    #[test]
    fn the_shown_picture_is_the_current_or_the_cover() {
        let rings = fixture_rings();
        let frames = story_frames(&rings[0].stories);
        let stories = &rings[0].stories;
        assert_eq!(shown_picture(&stories[0], &frames[0], 3).map(|p| &p.attachment_id), stories[0].pictures.first().map(|p| &p.attachment_id));
        assert_eq!(shown_picture(&stories[2], &frames[2], 1), stories[2].pictures.get(1));
        assert_eq!(shown_picture(&stories[1], &frames[1], 0), None);
    }
}
