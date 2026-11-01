//! #EXP-1493 THE PLAN PAGE (`fixtures/plan-page.json`, its `$comment` is
//! the spec; web `packages/ui/src/plan-page.tsx`, iOS ExpUI `PlanPage.swift`,
//! Android `ui/plan/PlanPage.kt`): the EXPANDED FORM of THE plan card, never
//! a second plan UI. The IDE is WIDE: ONE header row (`layout.headerHeight`,
//! a hairline under it: back, the chat's name muted, `/`, `copy.title`, THE
//! kind badge + the waiting dot, the counts line, `copy.openChat`, close)
//! over three columns: THE Outline ([`crate::outline`], scroll-spy, the
//! Questions row), THE document (one scroll, a `layout.columnMaxWidth`
//! column: THE chat markdown, h2 anchors, picture groups as THE Mosaic
//! [`crate::mosaic`]) and THE rail (the plan card's stepper + decision,
//! [`crate::plan_card::plan_rail`], sticky). Any picture opens THE lightbox
//! over every picture of the plan ([`crate::image_preview::open_picture_gallery`]).
//!
//! [`Screen::PlanPage`] is a detail that shares its session's tab (like
//! [`Screen::Session`]); [`open`] is every way in (the story viewer's
//! `Open plan`, the plan card header, the Dock sheet glyph, an inbox row,
//! the Needs-you row).

use std::rc::Rc;

use domain::plan_page::{plan_blocks, plan_counts_line, plan_outline, PlanBlock, PlanCounts, PLAN_PAGE};
use domain::rows::MessageRow;
use domain::session_tree::SessionThreadRow;
use domain::stories::StoryKind;
use gpui::{
    div, img, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::image_preview::GalleryPicture;
use crate::markdown::{attachment_natural_size, ImageCache, ImageSlot};
use crate::navigation::Screen;
use crate::outline::{OnOutlineSelect, OutlineEntry};
use crate::thread_screen::ThreadText;

/// How far under the column's top a heading still reads as passed (web
/// `PlanPage`'s scroll-spy slack).
const SPY_SLACK: f32 = 16.;

// ---------------------------------------------------------------------------
// Ways in
// ---------------------------------------------------------------------------

/// Opens THE plan page for `message_id` (a plan card `session_id` posted).
/// Every way in (the story viewer's `Open plan`, the plan card header, the
/// Dock sheet glyph, an inbox row, the Needs-you row) calls this. A message
/// that is not a plan card = the session instead (one still syncing opens
/// the page, which waits for it).
pub(crate) fn open(session_id: &str, message_id: &str, window: &mut Window, cx: &mut App) {
    if message_of(message_id, session_id, cx).is_some_and(|message| !is_plan_card(&message)) {
        crate::thread_screen::open_session_card(session_id, window, cx);
        return;
    }
    crate::navigation::navigate(
        window,
        cx,
        Screen::PlanPage { session_id: session_id.to_string(), message_id: message_id.to_string() },
    );
}

/// What an inbox row / push naming `session_id` opens (plan-page.json WAYS
/// IN): THE plan page when the session waits on its own plan
/// ([`domain::plan_page::plan_page_for`]), else the session's screen
/// ([`crate::navigation::session_screen_for`]).
pub(crate) fn screen_for_session(session_id: &str, cx: &App) -> Screen {
    match waiting_plan_of(session_id, cx) {
        Some(message_id) => Screen::PlanPage { session_id: session_id.to_string(), message_id },
        None => crate::navigation::session_screen_for(session_id, cx),
    }
}

/// The plan `session_id` waits on (`planPageFor` over its synced messages,
/// else its conversation's digest).
pub(crate) fn waiting_plan_of(session_id: &str, cx: &App) -> Option<String> {
    let mut messages = crate::session_data::messages_of_session(session_id, cx);
    if messages.is_empty() {
        messages = crate::session_data::digest_messages(cx, |id| id == session_id);
    }
    let candidates: Vec<domain::plan_page::PlanPageMessage> = messages
        .into_iter()
        .filter_map(|message| {
            Some(domain::plan_page::PlanPageMessage {
                created_at: message
                    .created_at
                    .as_deref()
                    .and_then(crate::inbox::parse_timestamp)
                    .map_or(0, |at| at.timestamp_millis()),
                card: message.card?,
                id: message.id,
                session_id: message.session_id,
                author_session_id: message.author_session_id,
            })
        })
        .collect();
    domain::plan_page::plan_page_for(session_id, &candidates).map(str::to_string)
}

/// A synced message by id, keyed to `session_id`.
fn message_of(id: &str, session_id: &str, cx: &App) -> Option<MessageRow> {
    let store = sync::Store::try_global(cx)?;
    let message = store.collections().messages.read(cx).get(id).cloned()?;
    Some(MessageRow { session_id: session_id.to_string(), ..message })
}

fn is_plan_card(message: &MessageRow) -> bool {
    message.card.as_ref().and_then(|card| card.get("kind")).and_then(|kind| kind.as_str()) == Some("plan")
}

/// Back / close (web: history back) and `Open chat` (the session).
fn leave_to_session(session_id: &str, back: bool, window: &mut Window, cx: &mut App) {
    let screen = crate::navigation::session_screen_for(session_id, cx);
    if back {
        crate::navigation::go_back_to(window, cx, screen);
    } else {
        crate::navigation::navigate(window, cx, screen);
    }
}

// ---------------------------------------------------------------------------
// The pure document helpers (tested)
// ---------------------------------------------------------------------------

fn is_fence(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

/// `![alt](src)` at the head of `text`: (alt, src, the rest).
fn picture_at(text: &str) -> Option<(&str, &str, &str)> {
    let rest = text.strip_prefix("![")?;
    let close = rest.find(']')?;
    let (alt, rest) = (&rest[..close], &rest[close + 1..]);
    let rest = rest.strip_prefix('(')?;
    let end = rest.find(|c: char| c == ')' || c.is_whitespace())?;
    if end == 0 || !rest[end..].starts_with(')') {
        return None;
    }
    Some((alt, &rest[..end], &rest[end + 1..]))
}

/// A picture-only line (`![alt](src)` alone on it).
fn picture_line(line: &str) -> Option<(String, String)> {
    let (alt, src, rest) = picture_at(line.trim())?;
    rest.trim().is_empty().then(|| (alt.to_string(), src.to_string()))
}

/// Every picture in a line, in order.
fn line_pictures(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = line[at..].find("![") {
        let start = at + found;
        match picture_at(&line[start..]) {
            Some((alt, src, rest)) => {
                out.push((alt.to_string(), src.to_string()));
                at = line.len() - rest.len();
            }
            None => at = start + 2,
        }
    }
    out
}

/// One piece of a prose block as the page paints it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    /// THE chat markdown.
    Text(String),
    /// A lone picture (its own line): the page's own tile, so a press opens
    /// THE lightbox over every picture.
    Picture { alt: String, src: String },
}

/// A `markdown` block split at its picture-only lines (outside a fence).
fn prose_pieces(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut fenced = false;
    let flush = |lines: &mut Vec<&str>, out: &mut Vec<Piece>| {
        let joined = lines.join("\n");
        if !joined.trim().is_empty() {
            out.push(Piece::Text(joined.trim_matches('\n').to_string()));
        }
        lines.clear();
    };
    for line in text.split('\n') {
        if is_fence(line) {
            fenced = !fenced;
        } else if !fenced {
            if let Some((alt, src)) = picture_line(line) {
                flush(&mut lines, &mut out);
                out.push(Piece::Picture { alt, src });
                continue;
            }
        }
        lines.push(line);
    }
    flush(&mut lines, &mut out);
    out
}

/// The header's waiting dot: THE needs-you yellow (#EXP-1480, the rail's
/// needs-you dot), never the primary (white on the forced dark theme).
fn waiting_dot_color() -> gpui::Hsla {
    theme::tokens::YELLOW.to_hsla()
}

/// The lightbox index of a pressed picture's `src` (a mosaic tile, a lone
/// picture or an INLINE one): its first place in [`document_pictures`].
fn picture_index(pictures: &[GalleryPicture], src: &str) -> usize {
    pictures.iter().position(|picture| picture.url == src).unwrap_or(0)
}

/// Every picture the document shows, in order (groups and inline; a fenced
/// code block shows none): THE lightbox's set.
fn document_pictures(blocks: &[PlanBlock]) -> Vec<GalleryPicture> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            PlanBlock::Pictures { pictures } => {
                out.extend(pictures.iter().map(|p| GalleryPicture { url: p.src.clone(), alt: p.alt.clone() }));
            }
            PlanBlock::Markdown { text } => {
                let mut fenced = false;
                for line in text.split('\n') {
                    if is_fence(line) {
                        fenced = !fenced;
                    } else if !fenced {
                        out.extend(line_pictures(line).into_iter().map(|(alt, url)| GalleryPicture { url, alt }));
                    }
                }
            }
            PlanBlock::Section { .. } => {}
        }
    }
    out
}

/// The scroll-spy (plan-page.json): the LAST section whose heading passed
/// the top of the column (`tops` = each section's offset in the content,
/// `scrolled` = the column's scroll); none yet = no active row.
fn spy<'a>(tops: impl IntoIterator<Item = (&'a str, f32)>, scrolled: f32) -> Option<&'a str> {
    tops.into_iter().filter(|(_, top)| *top <= scrolled + SPY_SLACK).map(|(id, _)| id).last()
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

/// A header / page action.
pub(crate) type PageAction = Rc<dyn Fn(&mut Window, &mut App)>;

/// What THE plan page paints (the live view's and the styleguide's).
pub(crate) struct PlanPageProps<'a> {
    /// A `SessionThreadRow::Plan`.
    pub row: &'a SessionThreadRow,
    pub chat_name: SharedString,
    pub text: &'a ThreadText,
    pub images: &'a Entity<ImageCache>,
    /// The styleguide specimen: every picture = the muted placeholder.
    pub placeholders: bool,
    pub scroll: &'a ScrollHandle,
    /// The outline's active row; `None` = the scroll-spy's.
    pub active: Option<String>,
    pub on_back: Option<PageAction>,
    pub on_close: Option<PageAction>,
    pub on_open_chat: Option<PageAction>,
}

/// THE plan page, wide.
pub(crate) fn plan_page(props: PlanPageProps, window: &Window, cx: &mut App) -> AnyElement {
    let Some((message_id, plan, facts)) = crate::thread_screen::plan_facts(props.row) else {
        return div().into_any_element();
    };
    let layout = &PLAN_PAGE.layout;
    let ids = &PLAN_PAGE.test_ids;
    let copy = &PLAN_PAGE.copy;
    let blocks = plan_blocks(plan);
    let outline = plan_outline(plan);
    let pictures = Rc::new(document_pictures(&blocks));
    let counts = plan_counts_line(&PlanCounts {
        sections: outline.len(),
        pictures: pictures.len(),
        questions: facts.questions.len(),
    });
    let waiting = facts.decision.is_none();
    let (muted, foreground, border, sidebar) = {
        let theme = cx.theme();
        (theme.muted_foreground, theme.foreground, theme.border, theme.sidebar)
    };

    // Section → its child index in the document's scroll.
    let sections: Vec<(String, usize)> = blocks
        .iter()
        .enumerate()
        .filter_map(|(ix, block)| match block {
            PlanBlock::Section { id, .. } => Some((id.clone(), ix)),
            _ => None,
        })
        .collect();
    let active = props.active.clone().or_else(|| {
        let column_top = props.scroll.bounds().top();
        let tops: Vec<(&str, f32)> = sections
            .iter()
            .filter_map(|(id, ix)| {
                props.scroll.bounds_for_item(*ix).map(|bounds| (id.as_str(), f32::from(bounds.top() - column_top)))
            })
            .collect();
        spy(tops, -f32::from(props.scroll.offset().y)).map(str::to_string)
    });

    // ── the lightbox over every picture ──
    let open_picture: Rc<dyn Fn(&str, &mut Window, &mut App)> = {
        let (pictures, images) = (pictures.clone(), props.images.clone());
        Rc::new(move |src: &str, window: &mut Window, cx: &mut App| {
            let index = picture_index(&pictures, src);
            crate::image_preview::open_picture_gallery((*pictures).clone(), index, images.clone(), window, cx);
        })
    };
    let open_picture = (!props.placeholders).then_some(open_picture);

    // ── the header ──
    let header = h_flex()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(layout.header_height))
        .px_4()
        .gap_3()
        .items_center()
        .border_b_1()
        .border_color(border)
        .when_some(props.on_back.clone(), |row, back| {
            row.child(
                crate::controls::ghost_icon_button("plan-page-back", Icon::new(registry::UI_BACK).small(), cx)
                    .tooltip(copy.open_chat.clone())
                    .on_click(move |_, window, cx| back(window, cx)),
            )
        })
        .child(div().min_w_0().truncate().text_xs().text_color(muted).child(props.chat_name.clone()))
        .child(div().flex_shrink_0().text_xs().text_color(muted).child("/"))
        .child(
            div()
                .flex_shrink_0()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(foreground)
                .child(SharedString::from(copy.title.clone())),
        )
        .child(div().flex_shrink_0().child(crate::story_badge::kind_badge("plan-page-badge", StoryKind::Plan, cx)))
        .when(waiting, |row| {
            row.child(div().id("plan-page-waiting").flex_shrink_0().size(px(6.)).rounded_full().bg(waiting_dot_color()))
        })
        .child(div().flex_1())
        .when(!counts.is_empty(), |row| {
            row.child(div().min_w_0().truncate().text_xs().text_color(muted).child(SharedString::from(counts.clone())))
        })
        .when_some(props.on_open_chat.clone(), |row, open_chat| {
            row.child(
                Button::new("plan-page-open-chat")
                    .ghost()
                    .web_sm()
                    .cursor_pointer()
                    .label(copy.open_chat.clone())
                    .on_click(move |_, window, cx| open_chat(window, cx)),
            )
        })
        .when_some(props.on_close.clone(), |row, close| {
            row.child(
                crate::controls::ghost_icon_button("plan-page-close", Icon::new(registry::UI_CLOSE).small(), cx)
                    .tooltip(copy.open_chat.clone())
                    .on_click(move |_, window, cx| close(window, cx)),
            )
        });

    // ── the outline ──
    let entries: Vec<OutlineEntry> = outline
        .iter()
        .map(|section| OutlineEntry { id: section.id.clone(), title: section.title.clone(), count: section.pictures })
        .collect();
    let question_count = facts.questions.len();
    let has_outline = !entries.is_empty() || question_count > 0;
    let outline_column = has_outline.then(|| {
        let on_select: OnOutlineSelect = {
            let (scroll, sections) = (props.scroll.clone(), sections.clone());
            Rc::new(move |id: &str, window: &mut Window, _: &mut App| {
                if let Some((_, ix)) = sections.iter().find(|(section, _)| section == id) {
                    scroll.scroll_to_top_of_item(*ix);
                    window.refresh();
                }
            })
        };
        let footer = (question_count > 0).then(|| crate::outline::outline_footer_row(&copy.questions, question_count, cx));
        v_flex()
            .id(SharedString::from(ids.outline.clone()))
            .flex_shrink_0()
            .w(px(layout.outline_width))
            .h_full()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(border)
            .px_3()
            .py_5()
            .child(crate::outline::outline(
                &ids.outline_row,
                Some(&copy.outline),
                &entries,
                active.as_deref(),
                Some(on_select),
                footer,
                cx,
            ))
    });

    // ── the document ──
    let pad = layout.column_padding.wide;
    let column = |element: AnyElement| div().flex_shrink_0().w_full().max_w(px(layout.column_max_width)).min_w_0().child(element);
    let mut document = v_flex()
        .id(SharedString::from(ids.document.clone()))
        .size_full()
        .overflow_y_scroll()
        .track_scroll(props.scroll)
        .items_center()
        .p(px(pad))
        .gap(px(layout.block_gap))
        // The scroll-spy re-reads the headings after a scroll.
        .on_scroll_wheel(|_, window, _| window.refresh());
    for (ix, block) in blocks.iter().enumerate() {
        let element = match block {
            PlanBlock::Section { id, title } => {
                let heading = crate::thread_screen::message_markdown(
                    SharedString::from(format!("plan-page-{message_id}-h2-{id}")),
                    &format!("## {title}"),
                    props.text,
                    props.images,
                );
                div()
                    .id(SharedString::from(format!("plan-section-{id}")))
                    .w_full()
                    .pt_2()
                    .child(crate::steer_viewer::ask_body_text(cx).child(heading))
                    .into_any_element()
            }
            PlanBlock::Pictures { pictures } => {
                let urls: Vec<String> = pictures.iter().map(|picture| picture.src.clone()).collect();
                let on_open = open_picture.clone().map(|open| {
                    let urls = urls.clone();
                    Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
                        if let Some(url) = urls.get(index) {
                            open(url, window, cx);
                        }
                    }) as crate::mosaic::OnMosaicOpen
                });
                let images = (!props.placeholders).then_some(props.images);
                crate::mosaic::mosaic(&format!("{message_id}-{ix}"), &urls, images, on_open, cx)
            }
            PlanBlock::Markdown { text } => {
                let mut prose = v_flex().w_full().min_w_0().gap(px(layout.block_gap));
                for (at, piece) in prose_pieces(text).into_iter().enumerate() {
                    prose = prose.child(match piece {
                        Piece::Text(text) => {
                            let mut view = crate::thread_screen::message_markdown(
                                SharedString::from(format!("plan-page-{message_id}-{ix}-{at}")),
                                &crate::plan_card::strip_pending_uploads(&text),
                                props.text,
                                props.images,
                            );
                            // An inline picture (mid-sentence) opens THE plan
                            // lightbox too, never the lone image preview.
                            if let Some(open) = open_picture.clone() {
                                view = view.on_open_image(move |src, window, cx| open(src, window, cx));
                            }
                            crate::steer_viewer::ask_body_text(cx).child(view).into_any_element()
                        }
                        Piece::Picture { alt, src } => {
                            lone_picture(&format!("{message_id}-{ix}-{at}"), &alt, &src, &props, open_picture.clone(), cx)
                        }
                    });
                }
                prose.into_any_element()
            }
        };
        document = document.child(column(element));
    }
    if facts.surface.is_some() {
        document = document.child(column(crate::plan_card::surface_block(message_id, true, cx)));
    }
    let document = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(document)
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(Scrollbar::new(props.scroll).axis(ScrollbarAxis::Vertical)),
        );

    // ── the rail ──
    let rail = v_flex()
        .flex_shrink_0()
        .w(px(layout.rail_width))
        .h_full()
        .min_h_0()
        .border_l_1()
        .border_color(border)
        .bg(sidebar)
        .px_4()
        .py_5()
        .child(crate::plan_card::plan_rail(&facts, props.images, window, cx));

    v_flex()
        .id(SharedString::from(format!("{}-{message_id}", ids.page)))
        .size_full()
        .min_h_0()
        .bg(cx.theme().background)
        .child(header)
        .child(
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_start()
                .children(outline_column)
                .child(document)
                .child(rail),
        )
        .into_any_element()
}

/// A lone picture of the prose: full column width at its own aspect (the
/// synced size, else the decoded one, else the Mosaic's), the Mosaic's
/// radius + hairline over the muted fill; a press opens THE lightbox.
fn lone_picture(
    id: &str,
    alt: &str,
    src: &str,
    props: &PlanPageProps,
    open: Option<Rc<dyn Fn(&str, &mut Window, &mut App)>>,
    cx: &mut App,
) -> AnyElement {
    let spec = &PLAN_PAGE.mosaic;
    let slot = (!props.placeholders).then(|| props.images.update(cx, |cache, cx| cache.slot(src, cx)));
    let natural = attachment_natural_size(src, cx).or_else(|| match &slot {
        Some(ImageSlot::Ready(image)) => image.decoded_size().map(|(w, h)| (w as f32, h as f32)),
        _ => None,
    });
    let aspect = natural.filter(|(w, h)| *w > 0. && *h > 0.).map_or(spec.aspect, |(w, h)| w / h);
    let picture = match slot {
        Some(ImageSlot::Ready(image)) => Some(img(image).size_full().object_fit(ObjectFit::Cover).into_any_element()),
        _ => None,
    };
    let src = src.to_string();
    div()
        .id(SharedString::from(format!("plan-picture-{id}")))
        .w_full()
        .aspect_ratio(aspect)
        .overflow_hidden()
        .rounded(px(spec.radius))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        .bg(cx.theme().muted)
        .cursor_pointer()
        .children(picture)
        .tooltip({
            let alt = alt.to_string();
            move |window, cx| gpui_component::tooltip::Tooltip::new(alt.clone()).build(window, cx)
        })
        .when_some(open, move |tile, open| {
            tile.on_click(move |_, window, cx| {
                cx.stop_propagation();
                open(&src, window, cx);
            })
        })
        .into_any_element()
}

// ---------------------------------------------------------------------------
// The live screen
// ---------------------------------------------------------------------------

/// [`Screen::PlanPage`]'s view: the synced plan card painted by
/// [`plan_page`]; the document's scroll and the scroll-spy live here.
pub(crate) struct PlanPageView {
    session_id: String,
    message_id: String,
    scroll: ScrollHandle,
    images: Entity<ImageCache>,
    _subscriptions: Vec<Subscription>,
}

impl PlanPageView {
    pub(crate) fn new(session_id: String, message_id: String, _window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let transport = crate::queries::attachment_transport(cx);
        let images = cx.new(|_| ImageCache::new(transport));
        let mut subscriptions = crate::session_data::observe_session_collections(cx);
        subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
        Self { session_id, message_id, scroll: ScrollHandle::new(), images, _subscriptions: subscriptions }
    }
}

impl Render for PlanPageView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let message = message_of(&self.message_id, &self.session_id, cx);
        let row = message.as_ref().and_then(domain::session_tree::ask_card_row);
        let Some(row @ SessionThreadRow::Plan { .. }) = row else {
            return v_flex()
                .id(SharedString::from(PLAN_PAGE.test_ids.page.clone()))
                .size_full()
                .p_6()
                .gap_2()
                .child(crate::feedback::skeleton_value("plan-page-skeleton-1", 240., cx))
                .child(crate::feedback::skeleton_value("plan-page-skeleton-2", 360., cx))
                .into_any_element();
        };
        let team_id = message
            .as_ref()
            .and_then(|message| message.team_id.clone())
            .or_else(|| crate::session_data::session(&self.session_id, cx).and_then(|session| session.team_id));
        let text = ThreadText::team(team_id.as_deref()).answering(true);
        let root_id = crate::session_data::session(&self.session_id, cx)
            .map(|session| session.root_id_or_self().to_string())
            .unwrap_or_else(|| self.session_id.clone());
        let chat_name = crate::navigation::screen_title(&Screen::Thread { session_id: root_id }, cx);
        let action = |back: bool, session_id: &str| -> PageAction {
            let session_id = session_id.to_string();
            Rc::new(move |window, cx| leave_to_session(&session_id, back, window, cx))
        };
        plan_page(
            PlanPageProps {
                row: &row,
                chat_name,
                text: &text,
                images: &self.images,
                placeholders: false,
                scroll: &self.scroll,
                active: None,
                on_back: Some(action(true, &self.session_id)),
                on_close: Some(action(true, &self.session_id)),
                on_open_chat: Some(action(false, &self.session_id)),
            },
            window,
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_page_picture_lines_parse() {
        assert_eq!(picture_line("  ![a b](/api/attachments/x)  "), Some(("a b".into(), "/api/attachments/x".into())));
        assert_eq!(picture_line("see ![a](/x)"), None);
        assert_eq!(picture_line("![a](/x) and more"), None);
        assert_eq!(picture_line("![a]( /x)"), None);
        assert_eq!(
            line_pictures("one ![a](/x) two ![b](/y) ![bad]("),
            vec![("a".into(), "/x".into()), ("b".into(), "/y".into())]
        );
    }

    #[test]
    fn plan_page_prose_splits_at_lone_pictures_outside_fences() {
        let pieces = prose_pieces("Intro\n\n![shot](/api/attachments/a)\n\nAfter\n```\n![code](/z)\n```");
        assert_eq!(
            pieces,
            vec![
                Piece::Text("Intro".into()),
                Piece::Picture { alt: "shot".into(), src: "/api/attachments/a".into() },
                Piece::Text("After\n```\n![code](/z)\n```".into()),
            ]
        );
    }

    /// THE lightbox's set = every picture in document order (groups, lone
    /// and inline ones; none from a fence, none pending).
    #[test]
    fn plan_page_document_pictures_follow_the_document() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/plan-page.json")).unwrap();
        let big = fixture["blockCases"].as_array().unwrap().iter().find(|case| case["name"] == "big").unwrap();
        let blocks = plan_blocks(big["markdown"].as_str().unwrap());
        let urls: Vec<String> = document_pictures(&blocks).into_iter().map(|picture| picture.url).collect();
        let expected: Vec<String> = ["a1", "a2", "a3", "a4", "a5", "a6", "a7"].iter().map(|id| format!("/api/attachments/{id}")).collect();
        assert_eq!(urls, expected);
    }

    /// #EXP-1493: an INLINE picture (mid-sentence) opens THE lightbox at
    /// its own document index, like a lone one.
    #[test]
    fn plan_page_inline_picture_opens_the_lightbox_at_its_index() {
        let blocks = plan_blocks("Intro\n\n![lone](/api/attachments/a)\n\nSee ![inline](/api/attachments/b) here.");
        let pictures = document_pictures(&blocks);
        assert_eq!(picture_index(&pictures, "/api/attachments/a"), 0);
        assert_eq!(picture_index(&pictures, "/api/attachments/b"), 1);
    }

    /// #EXP-1493: the header's waiting dot is THE needs-you yellow.
    #[test]
    fn plan_page_waiting_dot_is_the_needs_you_yellow() {
        assert_eq!(waiting_dot_color(), theme::tokens::YELLOW.to_hsla());
    }

    #[test]
    fn plan_page_scroll_spy_is_the_last_passed_heading() {
        let tops = [("a", 28.), ("b", 400.), ("c", 900.)];
        assert_eq!(spy(tops, 0.), None);
        assert_eq!(spy(tops, 20.), Some("a"));
        assert_eq!(spy(tops, 500.), Some("b"));
        assert_eq!(spy(tops, 2000.), Some("c"));
    }
}
