//! S-3 STORIES (`fixtures/stories.json` STORY VIEWER; web
//! `packages/ui/src/stories.tsx` `StoryViewer`): THE story viewer of the
//! IDE: a centred modal (`viewer.width` × `viewer.height` max,
//! `viewer.radius`) over a scrim, layered over the window from the Chats
//! page.
//!
//! - Top: THE progress ([`story_progress`], one segment per story of the
//!   ring); then the header: the ring's avatar, the story's `sessionName`,
//!   its `meta` muted and the close ghost glyph. A click on the avatar or
//!   the name opens that session and closes the viewer.
//! - Body: the title in one line, the pictures (fitted, a pager of dots when
//!   several, the caption under it) and/or the surface (THE surface card
//!   renderer, [`crate::surface_card::surface_row`]), else the plan's
//!   markdown.
//! - Footer: a plan's / question's answer controls are THE card's
//!   ([`crate::plan_card::plan_sheet`] / [`crate::plan_card::question_sheet`],
//!   the Dock sheet's pages and footer), answered through THE card-answer
//!   path (`sessions.answer`); a result has none.
//! - Navigation: → / the right arrow / a click on the right third = next
//!   (past a ring's last = the next ring, past the last ring = close); ← /
//!   the left arrow / the left third = previous; Escape, the scrim or the
//!   close glyph = close. Only a result advances on its own after
//!   `autoAdvanceMs`; holding a press pauses it.
//! - Every story SHOWN raises the viewer's stories mark of its root
//!   (`sessions.markStoriesSeen`, the server keeps the greatest).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    anchored, deferred, div, img, point, px, relative, AnyElement, App, AppContext as _, Div, Entity, FocusHandle,
    FontWeight, InteractiveElement as _, IntoElement as _, MouseButton, ObjectFit, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use domain::rows::MessageRow;
use domain::session_tree::SessionThreadRow;
use domain::stories::{story_next, story_previous, Story, StoryKind, StoryRing, STORIES};

use crate::icons::registry;
use crate::markdown::{ImageCache, ImageSlot};
use crate::thread_screen::ThreadText;

/// A press held this long pauses the timer instead of navigating.
const HOLD: Duration = Duration::from_millis(250);

/// The viewer's place and timer.
struct Open {
    team_id: String,
    ring_key: String,
    story_id: String,
    /// The story's index when last resolved (an answered story drops out:
    /// the viewer stays at the same place in the ring).
    index: usize,
    shown_at: Instant,
    paused_at: Option<Instant>,
    /// The press on the body (its start, for hold vs. click).
    pressed_at: Option<Instant>,
    /// The shown picture of a story with several.
    picture: usize,
}

#[derive(Default)]
struct Viewer {
    open: Option<Open>,
    focus: Option<FocusHandle>,
    images: Option<Entity<ImageCache>>,
    _images: Option<Subscription>,
    /// The greatest mark sent per root this run (never re-sent).
    marked: HashMap<String, i64>,
}

impl gpui::Global for Viewer {}

/// Opens THE viewer at `ring_key`'s first story.
pub(crate) fn open(team_id: &str, ring_key: &str, window: &mut Window, cx: &mut App) {
    let rings = crate::story_ring::team_stories(team_id, cx);
    let Some(story) = rings.iter().find(|ring| ring.key == ring_key).and_then(|ring| ring.stories.first()) else {
        return;
    };
    let story_id = story.id.clone();
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
        ring_key: ring_key.to_string(),
        story_id,
        index: 0,
        shown_at: Instant::now(),
        paused_at: None,
        pressed_at: None,
        picture: 0,
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

/// The open story's place in `rings` (an answered story = the same index
/// in its ring, clamped; a ring that is gone = `None`).
fn resolve(open: &Open, rings: &[StoryRing]) -> Option<(usize, usize)> {
    let ring = rings.iter().position(|ring| ring.key == open.ring_key)?;
    let stories = &rings[ring].stories;
    if stories.is_empty() {
        return None;
    }
    let story = stories
        .iter()
        .position(|story| story.id == open.story_id)
        .unwrap_or_else(|| open.index.min(stories.len() - 1));
    Some((ring, story))
}

/// Moves to `to` (or closes on `None`), restarting the timer.
fn go(to: Option<(usize, usize)>, rings: &[StoryRing], cx: &mut App) {
    let viewer = cx.default_global::<Viewer>();
    match (to, viewer.open.as_mut()) {
        (Some((ring, story)), Some(open)) => {
            let target = &rings[ring];
            open.ring_key = target.key.clone();
            open.story_id = target.stories[story].id.clone();
            open.index = story;
            open.shown_at = Instant::now();
            open.paused_at = None;
            open.pressed_at = None;
            open.picture = 0;
        }
        _ => viewer.open = None,
    }
    cx.refresh_windows();
}

/// Next story (past the last ring = close).
pub(crate) fn next(cx: &mut App) {
    step(true, cx);
}

/// Previous story (the very first stays put).
pub(crate) fn previous(cx: &mut App) {
    step(false, cx);
}

fn step(forward: bool, cx: &mut App) {
    let Some(team_id) = cx.try_global::<Viewer>().and_then(|viewer| viewer.open.as_ref()).map(|open| open.team_id.clone())
    else {
        return;
    };
    let rings = crate::story_ring::team_stories(&team_id, cx);
    let Some(at) = cx.global::<Viewer>().open.as_ref().and_then(|open| resolve(open, &rings)) else {
        close(cx);
        return;
    };
    let to = if forward { story_next(&rings, at) } else { Some(story_previous(&rings, at)) };
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

fn set_picture(index: usize, cx: &mut App) {
    if let Some(open) = cx.default_global::<Viewer>().open.as_mut() {
        open.picture = index;
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

/// THE viewer as a layer over the window, while open: resolves the place,
/// runs the result's timer, marks the story seen and paints the panel.
pub(crate) fn overlay(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let team_id = cx.try_global::<Viewer>()?.open.as_ref()?.team_id.clone();
    let rings = crate::story_ring::team_stories(&team_id, cx);
    let mut at = cx.global::<Viewer>().open.as_ref().and_then(|open| resolve(open, &rings));
    // A result past its time moves on (a decision never does).
    if let Some((ring, story)) = at {
        let open = cx.global::<Viewer>().open.as_ref()?;
        if let (Some(ms), None) = (rings[ring].stories[story].auto_advance_ms, open.paused_at) {
            if open.shown_at.elapsed() >= Duration::from_millis(ms) {
                let to = story_next(&rings, (ring, story));
                go(to, &rings, cx);
                at = to;
            }
        }
    }
    let Some((ring_index, story_index)) = at else {
        cx.default_global::<Viewer>().open = None;
        return None;
    };
    let ring = &rings[ring_index];
    let story = &ring.stories[story_index];
    {
        let open = cx.global_mut::<Viewer>().open.as_mut()?;
        if open.story_id != story.id {
            open.story_id = story.id.clone();
            open.index = story_index;
        }
    }
    mark_seen(story, cx);
    let (fraction, timing) = {
        let open = cx.global::<Viewer>().open.as_ref()?;
        match story.auto_advance_ms {
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
    let picture = cx.global::<Viewer>().open.as_ref().map_or(0, |open| open.picture);
    let images = images(cx);
    let message = message_of(&story.id, &story.session_id, cx);
    let text = ThreadText::team(Some(&team_id)).answering(true);
    let content = story_content(story, message.as_ref(), picture, &text, &images, window, cx);
    let avatar = crate::story_ring::store_ring_avatar(ring, 18., cx);
    let session_id = story.session_id.clone();
    let handlers = PanelHandlers {
        open_session: Some(Rc::new(move |window, cx| {
            close(cx);
            crate::thread_screen::open_session_card(&session_id, "", window, cx);
        })),
        close: Some(Rc::new(|_, cx| close(cx))),
        tap: (story.kind == StoryKind::Result).then(|| Rc::new(|_: &mut Window, _: &mut App| {}) as Action),
    };
    let panel = story_panel(ring, story, story_index, fraction, avatar, content, handlers, cx);
    let focus = cx.global::<Viewer>().focus.clone()?;
    Some(layer(panel, focus, window))
}

/// A synced message by id, keyed to `session_id` (the fold's owner).
fn message_of(id: &str, session_id: &str, cx: &App) -> Option<MessageRow> {
    let store = sync::Store::try_global(cx)?;
    let message = store.collections().messages.read(cx).get(id).cloned()?;
    Some(MessageRow { session_id: session_id.to_string(), ..message })
}

/// The scrim, the panel and the side arrows over the whole window.
fn layer(panel: gpui::Stateful<Div>, focus: FocusHandle, window: &mut Window) -> AnyElement {
    let viewport = window.viewport_size();
    let spec = &STORIES.viewer;
    let height = px(spec.height).min(viewport.height - px(32.));
    let width = px(spec.width).min(viewport.width - px(128.)).max(px(280.));
    let key_focus = focus.clone();
    let arrow = |id: &str, icon, label: &str, forward: bool| {
        Button::new(SharedString::from(id.to_string()))
            .ghost()
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
            })
    };
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
                        .child(arrow(&STORIES.test_ids.previous, registry::UI_CHEVRON_LEFT, &STORIES.copy.previous, false))
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
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .w(width)
                                .h(height)
                                .rounded(px(spec.radius))
                                .border_1()
                                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                .shadow_2xl(),
                        )
                        .child(arrow(&STORIES.test_ids.next, registry::UI_CHEVRON_RIGHT, &STORIES.copy.next, true)),
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
    /// `Some` = the body navigates by thirds and a held press pauses (a
    /// result: no controls in its body).
    pub tap: Option<Action>,
}

/// The story's body and footer.
pub(crate) struct StoryContent {
    pub body: AnyElement,
    pub footer: Option<AnyElement>,
}

/// THE progress (eUI `StoryProgress`'s look): one segment per story,
/// `progress.gap` apart, `progress.height` tall; done = foreground, the
/// current = its fill fraction, todo = the glass-stroke track.
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

/// THE panel: progress, header, body, footer. `avatar` fills the header's
/// 32px disc.
#[allow(clippy::too_many_arguments)]
pub(crate) fn story_panel(
    ring: &StoryRing,
    story: &Story,
    index: usize,
    fraction: f32,
    avatar: AnyElement,
    content: StoryContent,
    handlers: PanelHandlers,
    cx: &App,
) -> gpui::Stateful<Div> {
    let theme = cx.theme();
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
                .when(header_open.is_some(), |this| this.cursor_pointer())
                .on_click(move |_, window, cx| {
                    if let Some(action) = &header_open {
                        cx.stop_propagation();
                        action(window, cx);
                    }
                })
                .child(div().size(px(32.)).flex_shrink_0().rounded_full().overflow_hidden().child(avatar))
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
        );
    let tapping = handlers.tap.is_some();
    let body = div()
        .id("story-body")
        .relative()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(content.body)
        .when(tapping, |body| {
            body.cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    let viewer = cx.default_global::<Viewer>();
                    if let Some(open) = viewer.open.as_mut() {
                        open.pressed_at = Some(Instant::now());
                    }
                    pause(true, cx);
                })
                .on_mouse_up(MouseButton::Left, |event, window, cx| {
                    let pressed = cx
                        .default_global::<Viewer>()
                        .open
                        .as_mut()
                        .and_then(|open| open.pressed_at.take());
                    pause(false, cx);
                    let Some(pressed) = pressed else { return };
                    if pressed.elapsed() >= HOLD {
                        return;
                    }
                    // The thirds of the WINDOW's panel column: the panel is
                    // centred, so its thirds sit around the viewport's middle.
                    let width = f32::from(STORIES.viewer.width);
                    let middle = f32::from(window.viewport_size().width) / 2.;
                    let x = f32::from(event.position.x) - (middle - width / 2.);
                    if x < width / 3. {
                        previous(cx);
                    } else if x > width * 2. / 3. {
                        next(cx);
                    }
                })
        });
    v_flex()
        .id(SharedString::from(STORIES.test_ids.viewer.clone()))
        .relative()
        .overflow_hidden()
        .bg(theme.background)
        .text_color(theme.foreground)
        .child(
            v_flex()
                .flex_shrink_0()
                .gap_3()
                .px_3()
                .pt_3()
                .pb_2()
                .child(story_progress(ring.stories.len(), index, fraction, cx))
                .child(header),
        )
        .child(body)
        .children(content.footer)
}

/// The story's body and footer: an ask's = THE card sheet's (its lead =
/// the title, the pictures and the plan's markdown), a result's = the
/// title, the pictures and the surface card.
pub(crate) fn story_content(
    story: &Story,
    message: Option<&MessageRow>,
    picture: usize,
    text: &ThreadText,
    images: &Entity<ImageCache>,
    window: &mut Window,
    cx: &mut App,
) -> StoryContent {
    let mut lead = v_flex().w_full().min_w_0().gap_3();
    // The plan text already starts with its title (stories.json).
    if let Some(title) = story.title.as_deref().filter(|_| story.text.is_none()) {
        lead = lead.child(
            div()
                .id("story-title")
                .w_full()
                .truncate()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(title.to_string())),
        );
    }
    if !story.pictures.is_empty() {
        lead = lead.child(pictures(story, picture, images, cx));
    }
    let row = message.and_then(|message| match story.kind {
        StoryKind::Result => domain::session_tree::surface_card_row(message),
        _ => domain::session_tree::ask_card_row(message),
    });
    if let Some(markdown) = story.text.as_deref() {
        let view = crate::thread_screen::message_markdown(
            SharedString::from(format!("story-text-{}", story.id)),
            &crate::plan_card::strip_pending_uploads(markdown),
            text,
            images,
        );
        lead = lead.child(crate::steer_viewer::ask_body_text(cx).child(view));
    }
    match (&row, story.kind) {
        (Some(row @ SessionThreadRow::Plan { .. }), _) => {
            let facts = crate::thread_screen::plan_facts(row).map(|(_, _, facts)| facts);
            if let Some(facts) = facts {
                let sheet = crate::dock::sheet_host(|| {
                    crate::plan_card::plan_sheet(&facts, lead.into_any_element(), images, window, cx)
                });
                return StoryContent { body: sheet.body, footer: sheet.footer };
            }
            StoryContent { body: div().into_any_element(), footer: None }
        }
        (Some(row @ SessionThreadRow::Question { .. }), _) => {
            let facts = crate::thread_screen::question_facts(row).map(|(_, _, facts)| facts);
            if let Some(facts) = facts {
                let sheet = crate::dock::sheet_host(|| {
                    crate::plan_card::question_sheet(&facts, lead.into_any_element(), images, window, cx)
                });
                return StoryContent { body: sheet.body, footer: sheet.footer };
            }
            StoryContent { body: div().into_any_element(), footer: None }
        }
        (Some(SessionThreadRow::Surface { id, session_id, title, surface }), StoryKind::Result) if story.surface => {
            let card = crate::surface_card::surface_row(id, session_id, title.as_deref(), surface, window, cx);
            StoryContent { body: crate::dock::sheet_body("story-body-scroll", lead.child(card)), footer: None }
        }
        _ => StoryContent { body: crate::dock::sheet_body("story-body-scroll", lead), footer: None },
    }
}

/// The pictures: the shown one fitted, the dots when several, its caption.
fn pictures(story: &Story, index: usize, images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
    let index = index.min(story.pictures.len() - 1);
    let shown = &story.pictures[index];
    let url = format!("/api/attachments/{}", shown.attachment_id);
    let image = match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
        ImageSlot::Ready(image) => img(image).max_w_full().max_h(px(360.)).object_fit(ObjectFit::Contain).into_any_element(),
        _ => div()
            .w_full()
            .h(px(220.))
            .bg(theme::tokens::glass::FILL_ROW.to_hsla())
            .into_any_element(),
    };
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    v_flex()
        .id(SharedString::from(format!("story-picture-{}", shown.attachment_id)))
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .rounded(px(12.))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .overflow_hidden()
                .child(image),
        )
        .when(story.pictures.len() > 1, |column| {
            column.child(h_flex().gap_1p5().children(story.pictures.iter().enumerate().map(|(at, picture)| {
                div()
                    .id(SharedString::from(format!("story-picture-dot-{}", picture.attachment_id)))
                    .size(px(6.))
                    .rounded_full()
                    .cursor_pointer()
                    .bg(if at == index { foreground } else { theme::tokens::glass::STROKE_STRONG.to_hsla() })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        set_picture(at, cx);
                    })
            })))
        })
        .when_some(shown.caption.clone(), |column, caption| {
            column.child(div().text_xs().text_center().text_color(muted).child(SharedString::from(caption)))
        })
        .into_any_element()
}
