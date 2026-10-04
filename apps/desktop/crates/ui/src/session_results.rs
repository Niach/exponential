//! EXP-879 — the RESULTS face: the pictures a coding run published while it
//! worked, read off `coding_sessions.results`.
//!
//! ONE scrolling page, byte-for-byte the web/iOS/Android shape: per topic a
//! group band ([`crate::surface::glass_section_band`], the EXP-818 list
//! design) over a WRAPPING row of EQUAL-HEIGHT tiles — every tile
//! [`SESSION_RESULT_TILE_HEIGHT`] tall, or the one smaller height that makes
//! the widest of them fit a narrow window
//! ([`session_result_tile_height_fitting`], the shared ×4 rule), its width
//! from the probed aspect ([`session_result_tile_width`], 4:3 without one) —
//! each with a muted one-line caption under it. A shot of the same screen on
//! web, iOS and Android therefore reads side by side on one baseline.
//!
//! The image is the ordinary member-gated attachment route
//! (`/api/attachments/{id}`) fetched through the owning view's shared
//! [`ImageCache`], exactly like a comment's attachment; a click opens the
//! in-app lightbox at full size, never the browser.
//!
//! EXP-933: the page is the run's REPORT — a topic may carry GFM TEXT
//! (read-only [`crate::markdown::MarkdownView`], `#IDENT`/`@email` pills
//! resolved against `team_id`) rendered between its band and its tiles; a
//! text-only topic is a band + text.
//!
//! EXP-1172: a topic's INLINE pictures (`exponential_sessions_show` shots
//! the run filed while it worked) fold under the group's tiles into a
//! collapsed `Earlier · N` disclosure ([`EarlierBand`]) that expands IN PLACE
//! to the same tiles; the same tile ([`tile`]) also renders one such picture
//! under its call in the transcript.
//!
//! EXP-1154: the page is the GUIDE — the `Summary` topic leads unnumbered,
//! every other topic is a numbered section (`01 / 04` in its band) listing
//! the files it touched under its text; a file row opens the Changes face on
//! that file. An issue with an open PR and no report shows the PR body
//! instead ([`render_pr_body`]).
//!
//! This face carries NO Stop/Resume (the Run face owns it) and NO merge bar
//! (Changes owns it) — it is a page you look at.

use gpui::{
    div, hsla, img, linear_color_stop, linear_gradient, prelude::FluentBuilder as _, px,
    AnyElement, App, ElementId, Entity, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::session_results::{
    guide_file_rows, guide_section_caption, session_result_is_tall,
    session_result_tile_height_fitting, session_result_tile_width, session_results_guide,
    SessionResultEntry, SessionResultGroup, SESSION_RESULTS_EARLIER_LABEL,
    SESSION_RESULT_TILE_HEIGHT,
};

use crate::controls::{disclosure_header, ChevronSide};

use crate::issue_detail::{centered_column, DETAIL_GUTTER};
use crate::markdown::{placeholder_box, ImageCache, ImageSlot, MarkdownView, RefResolver};

/// EXP-1154 — what the Guide's FILE rows read: the loaded diff (counts for
/// a path that matches exactly; `None` = no diff loaded, every row without
/// counts) and what a row click does (`None` = plain rows). A row click opens
/// the host's Changes face with that file selected.
pub(crate) struct GuideFiles {
    pub(crate) loaded: Option<Vec<crate::diff_pane::PaneFile>>,
    pub(crate) on_open: Option<std::rc::Rc<dyn Fn(&str, &mut Window, &mut App)>>,
}

/// The whole page for one run's results, ready to drop into the pane slot
/// under the work header. Never called with an empty `groups` — an empty
/// Results face is not a face (the caller falls back to the transcript).
///
/// `available_width` is the width the TILES ROW actually gets (the centered
/// column's inner width, gutters already taken off). The page's tile height
/// is derived from it ONCE, by the shared ×4 rule
/// ([`session_result_tile_height_fitting`]): a narrow window scales the whole
/// page down by one factor rather than clipping its widest tile.
///
/// EXP-1154: the page reads as the GUIDE ([`session_results_guide`], the
/// shared ×4 rule): the `Summary` topic leads as a plain paragraph (no band,
/// no number); every other topic keeps the group band with a muted `01 / 04`
/// caption in its leading slot, then its text, the FILES it touched (flat
/// hairline rows: the dimmed-directory path + `+N −M` when `guide` has the
/// file loaded) and its tiles.
pub(crate) fn render(
    groups: &[SessionResultGroup],
    available_width: f32,
    images: &Entity<ImageCache>,
    team_id: Option<&str>,
    guide: Option<GuideFiles>,
    cx: &mut App,
) -> AnyElement {
    // ONE height for the page, not one per band — a shot's counterpart in the
    // NEXT topic has to sit on the same baseline too. The per-group minimum
    // IS the whole page's factor: the rule never grows a tile, so the group
    // holding the widest tile is the one that decides.
    // EXP-1172: the folded `earlier` pictures count too — expanding the
    // band never resizes the page.
    let height = groups
        .iter()
        .map(|group| {
            let pictures: Vec<SessionResultEntry> =
                group.entries.iter().chain(group.earlier.iter()).cloned().collect();
            session_result_tile_height_fitting(&pictures, available_width)
        })
        .fold(SESSION_RESULT_TILE_HEIGHT, f32::min);
    let guide = guide.unwrap_or(GuideFiles { loaded: None, on_open: None });
    let loaded: Option<Vec<(String, u32, u32)>> = guide.loaded.as_ref().map(|files| {
        files
            .iter()
            .map(|file| (file.path.to_string(), file.additions, file.deletions))
            .collect()
    });
    let (lead, sections) = session_results_guide(groups, |group| group.topic.as_str());
    let mut page = page_column();
    // Positions key the element ids: a topic may band twice.
    let position = |group: &SessionResultGroup| {
        groups
            .iter()
            .position(|candidate| std::ptr::eq(candidate, group))
            .unwrap_or(0)
    };
    if let Some(lead) = lead {
        page = page.child(
            v_flex().w_full().min_w_0().gap_2().children(group_body(
                lead,
                position(lead),
                height,
                images,
                team_id,
                loaded.as_deref(),
                guide.on_open.as_ref(),
                cx,
            )),
        );
    }
    for section in sections {
        let group_ix = position(section.group);
        let caption = div()
            .flex_shrink_0()
            .mr_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(guide_section_caption(section.index, section.total)))
            .into_any_element();
        page = page.child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(crate::surface::glass_section_band(
                    Some(caption),
                    SharedString::from(section.group.topic.clone()),
                    None,
                    cx,
                ))
                .children(group_body(
                    section.group,
                    group_ix,
                    height,
                    images,
                    team_id,
                    loaded.as_deref(),
                    guide.on_open.as_ref(),
                    cx,
                )),
        );
    }
    scroll_page(page)
}

/// The page's column: the work column's own gutter, so the bands line up
/// with the header's title above them.
fn page_column() -> gpui::Div {
    v_flex()
        .w_full()
        .min_w_0()
        .px(px(DETAIL_GUTTER))
        .pt_4()
        .pb_8()
        .gap_5()
}

fn scroll_page(page: gpui::Div) -> AnyElement {
    div()
        .id("session-results")
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .child(centered_column(page))
        .into_any_element()
}

/// A topic's report text as read-only markdown, `#IDENT`/`@email` pills
/// resolved against `team_id`.
fn report_text(id: SharedString, text: &str, images: Option<&Entity<ImageCache>>, team_id: Option<&str>) -> gpui::Div {
    let mut view = MarkdownView::new(id, text.to_string()).selectable(true);
    if let Some(images) = images {
        view = view.images(images.clone());
    }
    if let Some(team_id) = team_id {
        let team = team_id.to_string();
        view = view
            .resolver(RefResolver::from_store(team_id))
            .on_open_issue(move |identifier, window, cx| {
                crate::description_editor::open_issue_by_identifier(&team, identifier, window, cx);
            });
    }
    div().w_full().min_w_0().text_sm().child(view)
}

/// Everything under a group's band (or the lead's whole body): the text, the
/// files, the tiles and the `Earlier` fold.
#[allow(clippy::too_many_arguments)]
fn group_body(
    group: &SessionResultGroup,
    group_ix: usize,
    height: f32,
    images: &Entity<ImageCache>,
    team_id: Option<&str>,
    loaded: Option<&[(String, u32, u32)]>,
    on_open: Option<&std::rc::Rc<dyn Fn(&str, &mut Window, &mut App)>>,
    cx: &mut App,
) -> Vec<AnyElement> {
    let mut out: Vec<AnyElement> = Vec::new();
    if let Some(text) = group.text.as_ref() {
        out.push(
            report_text(
                SharedString::from(format!("session-result-text-{group_ix}")),
                text,
                Some(images),
                team_id,
            )
            .into_any_element(),
        );
    }
    if !group.files.is_empty() {
        out.push(file_list(group_ix, &group.files, loaded, on_open, cx));
    }
    if !group.entries.is_empty() {
        let mut tiles = h_flex().w_full().min_w_0().flex_wrap().items_start().gap_3();
        for (tile_ix, entry) in group.entries.iter().enumerate() {
            tiles = tiles.child(tile(
                entry,
                height,
                SharedString::from(format!("session-result-{group_ix}-{tile_ix}")),
                entry.label.clone(),
                images,
                cx,
            ));
        }
        out.push(tiles.into_any_element());
    }
    if !group.earlier.is_empty() {
        out.push(
            EarlierBand {
                // Keyed by topic, so a fold survives a re-snapshot that moves
                // the group.
                id: SharedString::from(format!("session-results-earlier-{}", group.topic)),
                entries: group.earlier.clone(),
                height,
                images: images.clone(),
            }
            .into_any_element(),
        );
    }
    out
}

/// EXP-1154 — a Guide section's files: flat hairline-divided rows
/// ([`crate::surface::list_row`] over [`crate::surface::flat_row`]), each the
/// dimmed directory + basename ([`crate::diff::split_path`]) and, when the
/// loaded diff has the path, its `+N −M` ([`crate::diff_pane::counts`]). A
/// row with somewhere to go washes on hover and opens the Changes face there.
fn file_list(
    group_ix: usize,
    paths: &[String],
    loaded: Option<&[(String, u32, u32)]>,
    on_open: Option<&std::rc::Rc<dyn Fn(&str, &mut Window, &mut App)>>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let hover = cx.theme().list_hover;
    let mut list = v_flex()
        .w_full()
        .min_w_0()
        .border_t_1()
        .border_b_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla());
    for (ix, row) in guide_file_rows(paths, loaded).into_iter().enumerate() {
        let (dir, name) = crate::diff::split_path(&row.path);
        let mut line = crate::surface::flat_row()
            .id(SharedString::from(format!("guide-file-{group_ix}-{ix}")))
            .flex()
            .w_full()
            .min_w_0()
            .h(px(32.))
            .px_1()
            .gap_3()
            .items_center()
            .justify_between()
            .text_xs()
            .child(
                h_flex()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_shrink_1()
                            .min_w_0()
                            .truncate()
                            .text_color(muted)
                            .child(SharedString::from(dir)),
                    )
                    .child(div().flex_shrink_0().child(SharedString::from(name))),
            )
            .children(
                row.counts
                    .map(|(additions, deletions)| crate::diff_pane::counts(additions, deletions, cx)),
            );
        if let Some(on_open) = on_open {
            let on_open = on_open.clone();
            let path = row.path.clone();
            line = line
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(move |_, window, cx| on_open(&path, window, cx));
        }
        list = list.child(crate::surface::list_row(line, ix));
    }
    list.into_any_element()
}

/// EXP-1154 — what the PR-body fallback shows under its band.
pub(crate) enum PrBodyContent<'a> {
    Loading,
    Ready(&'a str),
    Error(&'a str),
}

/// EXP-1154 — the Results face of an issue with an OPEN pull request and no
/// run report: the GitHub PR body (`issues.prDescription`) as ONE unnumbered
/// group, its band labelled by the PR title (`Pull request` without one) and
/// `No description.` under it when the body is blank.
pub(crate) fn render_pr_body(
    title: Option<&str>,
    body: PrBodyContent<'_>,
    team_id: Option<&str>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let label = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(PR_BODY_FALLBACK_LABEL)
        .to_string();
    let content: AnyElement = match body {
        PrBodyContent::Loading => div().text_sm().text_color(muted).child("Loading…").into_any_element(),
        PrBodyContent::Error(message) => div()
            .text_sm()
            .text_color(cx.theme().danger)
            .child(SharedString::from(message.to_string()))
            .into_any_element(),
        PrBodyContent::Ready(text) if text.trim().is_empty() => div()
            .text_sm()
            .text_color(muted)
            .child(PR_BODY_EMPTY)
            .into_any_element(),
        PrBodyContent::Ready(text) => {
            report_text(SharedString::from("session-results-pr-body"), text, None, team_id).into_any_element()
        }
    };
    scroll_page(
        page_column().child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(crate::surface::glass_section_band(None, SharedString::from(label), None, cx))
                .child(content),
        ),
    )
}

/// The PR-body fallback's band label when the PR has no title (×4).
pub(crate) const PR_BODY_FALLBACK_LABEL: &str = "Pull request";

/// The PR-body fallback's line for a blank body (×4).
pub(crate) const PR_BODY_EMPTY: &str = "No description.";

/// EXP-1172 — a group's folded INLINE pictures: a muted one-line
/// `Earlier · N` disclosure (the shared [`disclosure_header`]), collapsed by
/// default, that expands IN PLACE to the same tiles at the page's height.
/// The open flag lives in the window's keyed element state, so it survives
/// repaints without either host (the run's face, the issue's face) owning it.
#[derive(IntoElement)]
struct EarlierBand {
    id: SharedString,
    entries: Vec<SessionResultEntry>,
    height: f32,
    images: Entity<ImageCache>,
}

impl RenderOnce for EarlierBand {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(
            ElementId::from(SharedString::from(format!("{}-open", self.id))),
            cx,
            |_, _| false,
        );
        let open = *state.read(cx);
        let header = disclosure_header(
            ElementId::from(self.id.clone()),
            open,
            ChevronSide::Leading,
            div().min_w_0().truncate().text_xs().child(SharedString::from(format!(
                "{SESSION_RESULTS_EARLIER_LABEL} · {}",
                self.entries.len()
            ))),
            cx,
        )
        .on_click(move |_, _, cx| {
            state.update(cx, |open, cx| {
                *open = !*open;
                cx.notify();
            });
        });
        let mut band = v_flex().w_full().min_w_0().gap_2().child(header);
        if open {
            let mut tiles = h_flex().w_full().min_w_0().flex_wrap().items_start().gap_3();
            for (tile_ix, entry) in self.entries.iter().enumerate() {
                tiles = tiles.child(tile(
                    entry,
                    self.height,
                    SharedString::from(format!("{}-{tile_ix}", self.id)),
                    // Results tiles keep the LABEL (×4); the caption line is
                    // the transcript tile's alone.
                    entry.label.clone(),
                    &self.images,
                    cx,
                ));
            }
            band = band.child(tiles);
        }
        band
    }
}

/// One tile: the picture at the page's shared `height`, `caption` under it
/// (a Results tile's label, the Earlier band's included; EXP-1172: a
/// transcript tile's `session_result_tile_caption`). `id` is unique per position — an
/// attachment may be published under two topics.
/// The loading and unavailable states paint the neutral placeholder at the
/// SAME box, so the row never reflows when the bytes land.
///
/// EXP-1128: a TALL picture ([`session_result_is_tall`], the shared ×4 rule)
/// gets the 4:3 frame from [`session_result_tile_width`] and is laid out at
/// the tile's WIDTH, so its top shows (a top crop, never a sliver), under a
/// bottom fade and a `Tall` pill; the lightbox scrolls the whole of it.
pub(crate) fn tile(
    entry: &SessionResultEntry,
    height: f32,
    id: SharedString,
    caption: String,
    images: &Entity<ImageCache>,
    cx: &mut App,
) -> AnyElement {
    // The canonical relative form — the same key the cache fetches by and
    // the lightbox opens.
    let url = format!("/api/attachments/{}", entry.attachment_id);
    let natural = match (entry.width, entry.height) {
        (Some(width), Some(height)) => Some((width as f32, height as f32)),
        _ => None,
    };
    let width = session_result_tile_width(entry, height);
    let tall = session_result_is_tall(entry);
    // Tall implies probed dims; the fallback only guards a zero height.
    let aspect = natural
        .filter(|(_, h)| *h > 0.)
        .map(|(w, h)| w / h)
        .unwrap_or(4. / 3.);
    let slot = images.update(cx, |cache, cx| cache.slot(&url, cx));
    let label = caption;
    let muted = cx.theme().muted_foreground;
    v_flex()
        .flex_shrink_0()
        .w(px(width))
        .gap_1()
        .child(
            div()
                .id(id)
                .w(px(width))
                .h(px(height))
                .flex_shrink_0()
                .rounded(px(theme::tokens::radius::LG))
                .overflow_hidden()
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .cursor_pointer()
                .relative()
                .map(|tile| match slot {
                    // Tall: laid out at the tile's width, aspect-true, so the
                    // box's overflow clip shows its TOP.
                    ImageSlot::Ready(image) if tall => tile.child(
                        img(image)
                            .flex_shrink_0()
                            .w(px(width))
                            .h(px(width / aspect))
                            .object_fit(ObjectFit::Fill),
                    ),
                    // Cover: every tile is the same height, so a shot whose
                    // probe was wrong crops rather than letterboxes.
                    ImageSlot::Ready(image) => {
                        tile.child(img(image).size_full().object_fit(ObjectFit::Cover))
                    }
                    // Over the texture cap: strips at the tile's width — the
                    // same top crop (a >16k px WIDE picture just shows its
                    // top-left band at this width).
                    ImageSlot::ReadyTall(strips) => {
                        tile.child(crate::tall_image::render_tall(&strips, width))
                    }
                    // A label would only clip — the neutral box IS the
                    // loading/unavailable state here.
                    ImageSlot::Loading | ImageSlot::Failed(_) | ImageSlot::Unrenderable => {
                        tile.child(placeholder_box("", cx))
                    }
                })
                .when(tall, |tile| tile.child(tall_fade()).child(tall_pill()))
                .on_click({
                    let images = images.clone();
                    let url = url.clone();
                    let label = label.clone();
                    move |_, window, cx| {
                        crate::image_preview::open_image_preview(
                            url.clone(),
                            label.clone(),
                            natural,
                            Some(images.clone()),
                            window,
                            cx,
                        );
                    }
                }),
        )
        .child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(label)),
        )
        .into_any_element()
}

/// The bottom fade over a tall tile: the picture continues below the crop.
/// Paint-only (no mouse handlers), so the click reaches the tile.
fn tall_fade() -> impl IntoElement {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(48.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(hsla(0., 0., 0., 0.), 0.),
            linear_color_stop(hsla(0., 0., 0., 0.5), 1.),
        ))
}

/// The `Tall` pill (same copy ×4) in the tile's bottom-right corner.
fn tall_pill() -> impl IntoElement {
    div()
        .absolute()
        .bottom_1p5()
        .right_1p5()
        .rounded_full()
        .px_1p5()
        .py_0p5()
        .text_xs()
        .text_color(hsla(0., 0., 1., 0.9))
        .bg(hsla(0., 0., 0., 0.6))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .child("Tall")
}
