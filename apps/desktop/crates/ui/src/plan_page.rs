//! #EXP-1493 → #EXP-1662 THE PLAN IS A TAB (`fixtures/plan-page.json`, its
//! `$comment` is the spec; web `packages/ui/src/plan-page.tsx`, iOS ExpUI
//! `PlanPage.swift`, Android `ui/plan/PlanPage.kt`): the BODY of a chat's
//! plan tab, under the chat's own header + strip, on its ground, no page
//! header. The IDE is WIDE: THE Outline ([`crate::outline`], scroll-spy, the
//! Questions row) + THE document (one scroll, a `layout.columnMaxWidth`
//! column: the big title, THE chat markdown, h2 anchors, picture groups as
//! THE Mosaic [`crate::mosaic`]). #FEED-191: THE questions = THE RAIL on the
//! right (`layout.railWidth`, [`crate::plan_card::plan_rail`]). Any picture opens THE
//! lightbox over every picture of the plan
//! ([`crate::image_preview::open_picture_gallery`]).
//!
//! [`open`] is every way in that names a plan (a Conversation row, a Needs you
//! row, the Dock sheet glyph); [`prime_session`] + [`screen_for_session`]
//! are an inbox row's and a push's. All of them land on the chat's tab
//! ([`crate::session_chat::request_chat_plan`]). #FEED-191: a decision taken
//! on the tab returns the chat to its Conversation, the ONE way back besides
//! the Conversation tab itself.

use std::rc::Rc;

use domain::plan_page::{plan_blocks, plan_outline, PlanBlock, PLAN_PAGE};
use domain::rows::MessageRow;
use domain::session_tree::SessionChatRow;
use gpui::{
    div, img, prelude::FluentBuilder as _, px, relative, AnyElement, App, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Window,
};
use gpui_component::{
    h_flex,
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _,
};

use crate::image_preview::GalleryPicture;
use crate::markdown::{attachment_natural_size, ImageCache, ImageSlot};
use crate::navigation::Screen;
use crate::outline::{OnOutlineSelect, OutlineEntry};
use crate::session_chat::ChatText;

/// How far under the column's top a heading still reads as passed (web
/// `PlanPage`'s scroll-spy slack).
const SPY_SLACK: f32 = 16.;

// ---------------------------------------------------------------------------
// Ways in
// ---------------------------------------------------------------------------

/// The screen that shows `session_id`'s chat (its root's id too): the
/// viewer's own agent conversation = the agent's page, every other tree = its
/// chat screen (web: the deep link's redirect); `None` while unsynced.
fn chat_of(session_id: &str, cx: &App) -> Option<(String, Screen)> {
    let root_id = crate::session_data::session(session_id, cx)?.root_id_or_self().to_string();
    let screen = match crate::navigation::session_screen_for(&root_id, cx) {
        agent @ Screen::Agent { .. } => agent,
        _ => Screen::Chat { session_id: root_id.clone() },
    };
    Some((root_id, screen))
}

/// Opens the chat's plan tab on `message_id` (a plan card `session_id`
/// posted; a plan of another chat opens that chat on it). Every way in
/// that names a plan calls this. A message that is not a plan card = the
/// session instead, like a session that has not synced.
pub(crate) fn open(session_id: &str, message_id: &str, window: &mut Window, cx: &mut App) {
    let not_a_plan = message_of(message_id, session_id, cx).is_some_and(|message| !is_plan_card(&message));
    match chat_of(session_id, cx).filter(|_| !not_a_plan) {
        Some((root_id, screen)) => {
            crate::session_chat::request_chat_plan(&root_id, message_id, cx);
            crate::navigation::navigate(window, cx, screen);
        }
        None => crate::session_chat::open_session_card(session_id, window, cx),
    }
}

/// What an inbox row / push naming `session_id` opens (plan-page.json WAYS
/// IN): its chat while the session waits on its own plan
/// ([`domain::plan_page::plan_page_for`]), else the session's screen
/// ([`crate::navigation::session_screen_for`]). The caller pairs it with
/// [`prime_session`] when it navigates.
pub(crate) fn screen_for_session(session_id: &str, cx: &App) -> Screen {
    waiting_plan_of(session_id, cx)
        .and_then(|_| chat_of(session_id, cx))
        .map_or_else(|| crate::navigation::session_screen_for(session_id, cx), |(_, screen)| screen)
}

/// Selects the plan tab on the plan `session_id` waits on, for the navigation
/// to [`screen_for_session`] that follows.
pub(crate) fn prime_session(session_id: &str, cx: &mut App) {
    if let Some(message_id) = waiting_plan_of(session_id, cx) {
        if let Some((root_id, _)) = chat_of(session_id, cx) {
            crate::session_chat::request_chat_plan(&root_id, &message_id, cx);
        }
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

/// One document block in THE column: the full width, the block centred at
/// `max_width`. #EXP-1728: a flex item with `w_full` + `max_w` takes its
/// HEIGHT from its content laid out at the unclamped width (taffy), so a
/// long plan's wrapped paragraphs, tables and code painted taller than their
/// boxes and over the next block. The display-block column inside a
/// full-width item resolves its width first (`issue_detail::centered_column`,
/// EXP-179).
pub(crate) fn plan_column(element: AnyElement, max_width: f32) -> gpui::Div {
    div().flex_shrink_0().w_full().child(div().w_full().max_w(px(max_width)).mx_auto().min_w_0().child(element))
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
            PlanBlock::Title { .. } | PlanBlock::Section { .. } => {}
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

/// The body's own state, kept by its host: the document's scroll.
#[derive(Clone, Default)]
pub(crate) struct PlanPageState {
    pub scroll: ScrollHandle,
}

/// What THE plan tab's body paints (the chat's and the styleguide's).
pub(crate) struct PlanPageProps<'a> {
    /// A `SessionChatRow::Plan`.
    pub row: &'a SessionChatRow,
    pub text: &'a ChatText,
    pub images: &'a Entity<ImageCache>,
    pub state: &'a PlanPageState,
    /// The outline's active row; `None` = the scroll-spy's.
    pub active: Option<String>,
    /// The reader may decide (a non-member reads).
    pub answer: bool,
}

/// THE plan tab's body, wide.
pub(crate) fn plan_page(props: PlanPageProps, window: &Window, cx: &mut App) -> AnyElement {
    let Some((message_id, plan, facts)) = crate::session_chat::plan_facts(props.row) else {
        return div().into_any_element();
    };
    let facts = crate::plan_card::PlanFacts { on_tab: true, ..facts };
    let layout = &PLAN_PAGE.layout;
    let panel = &PLAN_PAGE.panel;
    let ids = &PLAN_PAGE.test_ids;
    let copy = &PLAN_PAGE.copy;
    let blocks = plan_blocks(plan);
    let outline = plan_outline(plan);
    let pictures = Rc::new(document_pictures(&blocks));
    let (foreground, border) = {
        let theme = cx.theme();
        (theme.foreground, theme.border)
    };
    let scroll = &props.state.scroll;

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
        let column_top = scroll.bounds().top();
        let tops: Vec<(&str, f32)> = sections
            .iter()
            .filter_map(|(id, ix)| {
                scroll.bounds_for_item(*ix).map(|bounds| (id.as_str(), f32::from(bounds.top() - column_top)))
            })
            .collect();
        spy(tops, -f32::from(scroll.offset().y)).map(str::to_string)
    });

    // ── the lightbox over every picture ──
    let open_picture: Rc<dyn Fn(&str, &mut Window, &mut App)> = {
        let (pictures, images) = (pictures.clone(), props.images.clone());
        Rc::new(move |src: &str, window: &mut Window, cx: &mut App| {
            let index = picture_index(&pictures, src);
            crate::image_preview::open_picture_gallery((*pictures).clone(), index, images.clone(), window, cx);
        })
    };
    let open_picture = Some(open_picture);

    // ── the outline ──
    let entries: Vec<OutlineEntry> = outline
        .iter()
        .map(|section| OutlineEntry { id: section.id.clone(), title: section.title.clone(), count: section.pictures })
        .collect();
    let question_count = facts.questions.len();
    let has_outline = !entries.is_empty() || question_count > 0;
    let outline_column = has_outline.then(|| {
        let on_select: OnOutlineSelect = {
            let (scroll, sections) = (scroll.clone(), sections.clone());
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
    let column = |element: AnyElement| plan_column(element, layout.column_max_width);
    let mut document = v_flex()
        .id(SharedString::from(ids.document.clone()))
        .size_full()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .items_center()
        .p(px(pad))
        .gap(px(layout.block_gap))
        // The scroll-spy re-reads the headings after a scroll.
        .on_scroll_wheel(|_, window, _| window.refresh());
    for (ix, block) in blocks.iter().enumerate() {
        let element = match block {
            PlanBlock::Title { text } => div()
                .id(SharedString::from(format!("plan-title-{message_id}")))
                .w_full()
                .text_size(px(layout.title_size))
                .line_height(relative(1.25))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(foreground)
                .child(SharedString::from(text.clone()))
                .into_any_element(),
            PlanBlock::Section { id, title } => {
                let heading = crate::session_chat::message_markdown(
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
                crate::mosaic::mosaic(&format!("{message_id}-{ix}"), pictures, Some(props.images), on_open, cx)
            }
            PlanBlock::Markdown { text } => {
                let mut prose = v_flex().w_full().min_w_0().gap(px(layout.block_gap));
                for (at, piece) in prose_pieces(text).into_iter().enumerate() {
                    prose = prose.child(match piece {
                        Piece::Text(text) => {
                            let mut view = crate::session_chat::message_markdown(
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
        document = document.child(column(crate::plan_card::surface_block(message_id, cx)));
    }

    // ── #FEED-191: the questions = THE RAIL on the right (the IDE is always
    // wide): full height beside the document, sticky, the sidebar ground. ──
    let rail = crate::plan_card::plan_rail(&facts, props.answer, props.images, window, cx).map(|rail| {
        v_flex()
            .id(SharedString::from(format!("plan-questions-rail-{message_id}")))
            .flex_shrink_0()
            .w(px(layout.rail_width))
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .border_l_1()
            .border_color(border)
            .bg(cx.theme().sidebar)
            .p(px(panel.padding))
            .child(rail)
    });
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
                .child(Scrollbar::new(scroll).axis(ScrollbarAxis::Vertical)),
        );

    h_flex()
        .id(SharedString::from(format!("{}-{message_id}", ids.page)))
        .size_full()
        .min_h_0()
        .items_start()
        .children(outline_column)
        .child(document)
        .children(rail)
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
    let slot = props.images.update(cx, |cache, cx| cache.slot(src, cx));
    let natural = attachment_natural_size(src, cx).or_else(|| match &slot {
        ImageSlot::Ready(image) => image.decoded_size().map(|(w, h)| (w as f32, h as f32)),
        _ => None,
    });
    let aspect = natural.filter(|(w, h)| *w > 0. && *h > 0.).map_or(spec.aspect, |(w, h)| w / h);
    let picture = match slot {
        ImageSlot::Ready(image) => Some(img(image).size_full().object_fit(ObjectFit::Cover).into_any_element()),
        _ => None,
    };
    let src = src.to_string();
    div()
        .id(SharedString::from(format!("plan-picture-{id}")))
        .role(gpui::Role::Button)
        .aria_label(alt.to_string())
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

    #[test]
    fn plan_page_scroll_spy_is_the_last_passed_heading() {
        let tops = [("a", 28.), ("b", 400.), ("c", 900.)];
        assert_eq!(spy(tops, 0.), None);
        assert_eq!(spy(tops, 20.), Some("a"));
        assert_eq!(spy(tops, 500.), Some("b"));
        assert_eq!(spy(tops, 2000.), Some("c"));
    }

    /// #EXP-1728: every plan block's box is as tall as its content at the
    /// column width (a long plan's blocks never paint over each other), at
    /// a wide window and after a resize that narrows the column.
    #[gpui::test]
    async fn plan_blocks_are_as_tall_as_their_content(cx: &mut gpui::TestAppContext) {
        use gpui::{size, Bounds, Pixels, Render, VisualTestContext};
        use std::cell::RefCell;

        const PIECES: [&str; 4] = [
            "Its primitives are quads, shadows, paths and glyph sprites from an atlas. So gpui is already \"one engine, drawn on the GPU\", and it now has iOS and web ports. **It has no Android port.**",
            "## 2. The three options",
            "| | A today | B shared scene | C shared renderer (vello) |\n|---|---|---|---|\n| Same look ×5 | by conformance (drifts, gets fixed) | by construction except anti-aliasing | pixel-identical |\n\n```\nA  TODAY: shared core, native everything\n   core -> 5 painters -> SwiftUI views / Compose nodes / DOM / gpui elements\n```\n\n**Is one engine faster than four native ones?** Our own benchmarks say the shared core is cheap and **the native toolkits are the cost**.",
            "## 3. The recommendation: B now, C where there is no platform to replay on",
        ];
        const PAD: f32 = 32.;
        const MAX: f32 = 720.;

        /// The plan document (`reference` = each block at a fixed width,
        /// the height its content takes).
        struct Doc {
            reference: bool,
            boxes: Rc<RefCell<Vec<Bounds<Pixels>>>>,
        }
        impl Render for Doc {
            fn render(&mut self, window: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Doc").entered();
                let column_w = (f32::from(window.viewport_size().width) - 2. * PAD).min(MAX);
                let mut doc = v_flex().size_full().items_center().p(px(PAD)).gap(px(16.));
                for (at, piece) in PIECES.iter().enumerate() {
                    let view = crate::markdown::MarkdownView::new(format!("plan-height-{}-{at}", self.reference), *piece).chat(true).into_any_element();
                    doc = doc.child(if self.reference { div().flex_shrink_0().w(px(column_w)).child(view) } else { plan_column(view, MAX) });
                }
                let boxes = self.boxes.clone();
                doc.on_children_prepainted(move |bounds, _, _| *boxes.borrow_mut() = bounds)
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let heights = |cx: &mut gpui::TestAppContext, reference: bool, width: f32| {
            let boxes = Rc::new(RefCell::new(Vec::new()));
            let held = boxes.clone();
            let (_doc, vcx) = cx.add_window_view(move |_, _| Doc { reference, boxes: held });
            let vcx: &mut VisualTestContext = vcx;
            let mut out = Vec::new();
            for w in [1400., width] {
                vcx.simulate_resize(size(px(w), px(900.)));
                for _ in 0..3 {
                    vcx.update(|window, cx| window.draw(cx).clear(cx));
                    vcx.run_until_parked();
                }
                out.push(boxes.borrow().iter().map(|b| f32::from(b.size.height)).collect::<Vec<_>>());
            }
            out
        };
        let reference = heights(cx, true, 560.);
        let plan = heights(cx, false, 560.);
        assert!(reference[1][0] > reference[0][0], "the narrow column wraps more: {reference:?}");
        assert_eq!(plan, reference, "each block's box = its content's height");
    }
}
