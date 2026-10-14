//! EXP-879 — the Guide: the pictures a coding run published while it
//! worked. EXP-1294: read off the thread root's `sessions.guide`, the
//! thread's Guide tab.
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
//! EXP-1154/EXP-1251: the page is the GUIDE (Changes + Results merged into
//! ONE face): the Stack card, the `Summary` lead, numbered sections with ONE
//! Changes row each (the shared coverage rule), the automatic `Other
//! changes` section and `Show complete diff`; a row opens the diff page
//! filtered to its files ([`GuidePage`], [`guide_page_view`]).

use gpui::{
    div, hsla, img, linear_color_stop, linear_gradient, prelude::FluentBuilder as _, px,
    AnyElement, App, ElementId, Entity, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Sizable as _};

use domain::session_results::{
    guide_coverage, guide_file_count_label, guide_section_caption, session_result_is_tall,
    session_result_tile_height_fitting, session_result_tile_width, GuideChangeSet, GuideDiffFile,
    SessionResultEntry, SessionResultGroup, SESSION_RESULTS_EARLIER_LABEL,
    SESSION_RESULT_TILE_HEIGHT,
};

use crate::controls::{disclosure_header, ChevronSide};

use crate::issue_detail::centered_column;
use crate::work_header::WORK_GUTTER;
use crate::markdown::{placeholder_box, ImageCache, ImageSlot, MarkdownView};

/// EXP-1251 — the diff page a Guide row opens (web `?view=guide&section=`):
/// a numbered section, the lead's files, the automatic `Other changes`
/// section, or the complete diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuidePage {
    Lead,
    /// 1-based, the section's `01 / 06` number.
    Section(usize),
    Other,
    All,
}

/// A Guide row was clicked: open that diff page.
pub(crate) type OnOpenGuidePage = std::rc::Rc<dyn Fn(GuidePage, &mut Window, &mut App)>;

/// EXP-1251 — everything the Guide reads beyond the report.
#[derive(Default)]
pub(crate) struct GuideSpec {
    /// The loaded diff (the run's worktree diff, else the PR's files);
    /// `None` = nothing loaded: no Changes rows.
    pub(crate) diff: Option<Vec<crate::diff_pane::PaneFile>>,
    /// What a Changes / Show complete diff row does (`None` = inert rows).
    pub(crate) on_open: Option<OnOpenGuidePage>,
    /// The Stack card ([`stack_card`]) over everything.
    pub(crate) stack: Option<AnyElement>,
    /// The lead drawn when the run filed NO report.
    pub(crate) fallback_lead: Option<AnyElement>,
    /// Wave D (web `numbered={false}`): bands without the `01 / 04`
    /// caption (a single unnumbered group).
    pub(crate) unnumbered: bool,
    /// EXP-1261: the page's scroll, for a host that reads its offset (the
    /// work body's top fade shows only once it left the top). `None` = an
    /// untracked scroll.
    pub(crate) scroll: Option<gpui::ScrollHandle>,
    /// EXP-1283: the raw Guide (`sessions.guide`): a tile opens THE picture
    /// viewer over it ([`crate::image_preview::open_guide_viewer`]) instead
    /// of the bare lightbox. `None` = the lightbox.
    pub(crate) viewer: Option<GuideViewerSource>,
}

/// EXP-1283: what a Guide tile hands THE picture viewer — the raw Guide and
/// the team its text pills resolve against.
#[derive(Clone)]
pub(crate) struct GuideViewerSource {
    pub(crate) guide: std::rc::Rc<serde_json::Value>,
    pub(crate) team_id: Option<String>,
}

/// The coverage rule's view of a pane file.
pub(crate) fn guide_diff_files(files: &[crate::diff_pane::PaneFile]) -> Vec<GuideDiffFile> {
    files
        .iter()
        .map(|file| GuideDiffFile {
            path: file.path.to_string(),
            previous_path: file.previous_path.as_ref().map(|path| path.to_string()),
            additions: file.additions,
            deletions: file.deletions,
        })
        .collect()
}

/// EXP-1251 — what a Guide diff page shows: its back-row caption (`02 / 06`
/// for a numbered section), its title, the paths it filters the diff to (in
/// coverage order) and their summed counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuidePageView {
    pub(crate) caption: Option<String>,
    pub(crate) title: String,
    pub(crate) paths: Vec<String>,
    pub(crate) additions: u32,
    pub(crate) deletions: u32,
}

/// Resolve `page` against the report and the loaded diff (the shared
/// coverage rule). `None` = the page names nothing any more (a section that
/// went away): the host falls back to the Guide.
pub(crate) fn guide_page_view(
    groups: &[SessionResultGroup],
    diff: &[GuideDiffFile],
    page: GuidePage,
    // An unnumbered Guide: its page carries no
    // `01 / 01` caption, as its band carries no number.
    unnumbered: bool,
) -> Option<GuidePageView> {
    let coverage = guide_coverage(
        groups,
        |group| group.topic.as_str(),
        |group| group.files.as_slice(),
        Some(diff),
    );
    let view = |caption: Option<String>, title: String, set: &GuideChangeSet| GuidePageView {
        caption,
        title,
        paths: set.files.iter().map(|&ix| diff[ix].path.clone()).collect(),
        additions: set.additions,
        deletions: set.deletions,
    };
    match page {
        GuidePage::All => coverage.complete.as_ref().map(|set| {
            view(None, domain::contract::DIFF_UI_GUIDE_CHANGES_ROW.to_string(), set)
        }),
        GuidePage::Other => coverage
            .other
            .as_ref()
            .map(|(title, set)| view(None, title.to_string(), set)),
        GuidePage::Lead => coverage.lead.as_ref().and_then(|lead| {
            lead.changes
                .as_ref()
                .map(|set| view(None, lead.group.topic.clone(), set))
        }),
        GuidePage::Section(index) => coverage
            .sections
            .iter()
            .find(|section| section.index == index)
            .and_then(|section| {
                section.changes.as_ref().map(|set| {
                    view(
                        (!unnumbered).then(|| guide_section_caption(section.index, section.total)),
                        section.group.topic.clone(),
                        set,
                    )
                })
            }),
    }
}

/// The whole Guide for one run's results, ready to drop into the pane slot
/// under the work header.
///
/// `available_width` is the width the TILES ROW actually gets (the centered
/// column's inner width, gutters already taken off). The page's tile height
/// is derived from it ONCE, by the shared ×4 rule
/// ([`session_result_tile_height_fitting`]): a narrow window scales the whole
/// page down by one factor rather than clipping its widest tile.
///
/// EXP-1251: the GUIDE (web `GuideBody`): the Stack card, the `Summary` lead
/// as a plain paragraph, every other topic a numbered band (`01 / 04`) over
/// its text, ONE Changes row (`<>` Changes · N files · +A −D ›, the files the
/// shared coverage rule matched) and its tiles; then the automatic `Other
/// changes` section for every diff file no topic names and the final `Show
/// complete diff` row. No report = the fallback lead (an open PR's body) and
/// ONE `Changes` section.
pub(crate) fn render(
    groups: &[SessionResultGroup],
    available_width: f32,
    images: &Entity<ImageCache>,
    team_id: Option<&str>,
    guide: GuideSpec,
    cx: &mut App,
) -> AnyElement {
    // ONE height for the page, not one per band — a shot's counterpart in the
    // NEXT topic has to sit on the same baseline too. EXP-1172: the folded
    // `earlier` pictures count too — expanding the band never resizes it.
    let height = groups
        .iter()
        .map(|group| {
            let pictures: Vec<SessionResultEntry> =
                group.entries.iter().chain(group.earlier.iter()).cloned().collect();
            session_result_tile_height_fitting(&pictures, available_width)
        })
        .fold(SESSION_RESULT_TILE_HEIGHT, f32::min);
    let GuideSpec { diff, on_open, stack, fallback_lead, unnumbered, scroll, viewer } = guide;
    let viewer = viewer.as_ref();
    let diff = diff.as_deref().map(guide_diff_files);
    let coverage = guide_coverage(
        groups,
        |group| group.topic.as_str(),
        |group| group.files.as_slice(),
        diff.as_deref(),
    );
    let mut page = page_column().children(stack);
    // Positions key the element ids: a topic may band twice.
    let position = |group: &SessionResultGroup| {
        groups
            .iter()
            .position(|candidate| std::ptr::eq(candidate, group))
            .unwrap_or(0)
    };
    let row = |id: String, page: GuidePage, set: &GuideChangeSet, cx: &App| {
        (set.file_count() > 0).then(|| {
            changes_row(
                SharedString::from(id),
                domain::contract::DIFF_UI_GUIDE_CHANGES_ROW,
                Some(crate::icons::registry::GUIDE_CHANGES),
                set,
                on_open.clone().map(|on_open| (page, on_open)),
                cx,
            )
        })
    };
    if let Some(lead) = coverage.lead.as_ref() {
        let group_ix = position(lead.group);
        page = page.child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .children(group_body(lead.group, group_ix, height, images, team_id, viewer, cx, |cx| {
                    lead.changes
                        .as_ref()
                        .and_then(|set| row(format!("guide-changes-lead-{group_ix}"), GuidePage::Lead, set, cx))
                })),
        );
    } else if groups.is_empty() {
        page = page.children(fallback_lead);
    }
    for section in &coverage.sections {
        let group_ix = position(section.group);
        let caption = (!unnumbered).then(|| {
            div()
                .flex_shrink_0()
                .mr_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(guide_section_caption(section.index, section.total)))
                .into_any_element()
        });
        let band = crate::surface::glass_section_band(
            caption,
            SharedString::from(section.group.topic.clone()),
            None,
            cx,
        );
        page = page.child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(band)
                .children(group_body(section.group, group_ix, height, images, team_id, viewer, cx, |cx| {
                    section.changes.as_ref().and_then(|set| {
                        row(
                            format!("guide-changes-{}", section.index),
                            GuidePage::Section(section.index),
                            set,
                            cx,
                        )
                    })
                })),
        );
    }
    if let Some((title, set)) = coverage.other.as_ref() {
        // The automatic section: a muted title, never a number.
        let band = h_flex()
            .w_full()
            .min_w_0()
            .px_3()
            .py_1p5()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(title.to_string()));
        page = page.child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(band)
                .children(row("guide-changes-other".to_string(), GuidePage::Other, set, cx)),
        );
    }
    // The complete diff is its own row only when a report split it up.
    if let (Some(set), false) = (coverage.complete.as_ref(), groups.is_empty()) {
        if set.file_count() > 0 {
            page = page.child(changes_row(
                SharedString::from("guide-show-complete-diff"),
                domain::contract::DIFF_UI_GUIDE_SHOW_COMPLETE_DIFF,
                None,
                set,
                on_open.clone().map(|on_open| (GuidePage::All, on_open)),
                cx,
            ));
        }
    }
    scroll_page(page, scroll)
}

/// The page's column: the work column's own gutter, so the bands line up
/// with the header's title above them.
fn page_column() -> gpui::Div {
    v_flex()
        .w_full()
        .min_w_0()
        .px(px(WORK_GUTTER))
        .pt_4()
        .pb_8()
        .gap_5()
}

fn scroll_page(page: gpui::Div, scroll: Option<gpui::ScrollHandle>) -> AnyElement {
    div()
        .id("session-results")
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .when_some(scroll, |page, scroll| page.track_scroll(&scroll))
        .child(centered_column(page))
        .into_any_element()
}

/// A topic's report text as read-only markdown, `#IDENT` / bare `ABC-12` /
/// `@email` pills resolved against `team_id` (a Guide section = a thread
/// text, [`crate::thread_screen::thread_text_resolver`], EXP-1309).
fn report_text(id: SharedString, text: &str, images: Option<&Entity<ImageCache>>, team_id: Option<&str>) -> gpui::Div {
    // EXP-1188: a Guide link to a run or issue on this instance opens in
    // the app; bare URLs in the report link like the feed's.
    let mut view = MarkdownView::new(id, text.to_string())
        .selectable(true)
        .autolink(true)
        .on_open_link(crate::markdown::open_agent_link);
    if let Some(images) = images {
        view = view.images(images.clone());
    }
    if let Some(team_id) = team_id {
        let team = team_id.to_string();
        view = view
            .resolver(crate::thread_screen::thread_text_resolver(team_id))
            .on_open_issue(move |identifier, window, cx| {
                crate::description_editor::open_issue_by_identifier(&team, identifier, window, cx);
            });
    }
    div().w_full().min_w_0().text_sm().child(view)
}

/// Everything under a group's band (or the lead's whole body): the text, the
/// Changes row (`changes`), the tiles and the `Earlier` fold.
fn group_body(
    group: &SessionResultGroup,
    group_ix: usize,
    height: f32,
    images: &Entity<ImageCache>,
    team_id: Option<&str>,
    viewer: Option<&GuideViewerSource>,
    cx: &mut App,
    changes: impl FnOnce(&App) -> Option<AnyElement>,
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
    out.extend(changes(cx));
    if !group.entries.is_empty() {
        let mut tiles = h_flex().w_full().min_w_0().flex_wrap().items_start().gap_3();
        for (tile_ix, entry) in group.entries.iter().enumerate() {
            tiles = tiles.child(tile(
                entry,
                height,
                SharedString::from(format!("session-result-{group_ix}-{tile_ix}")),
                entry.label.clone(),
                images,
                viewer,
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
                viewer: viewer.cloned(),
            }
            .into_any_element(),
        );
    }
    out
}

/// EXP-1251 — ONE Guide row (web `GuideChangesRow` / the final `Show
/// complete diff` row): an optional lead glyph (`guide-changes`, the `<>`
/// tag), the label, the muted `N files`, `+A −D` and a chevron, over a
/// hairline. A row with somewhere to go washes on hover and opens that diff
/// page.
fn changes_row(
    id: SharedString,
    label: &'static str,
    glyph: Option<crate::icons::ExpIcon>,
    set: &GuideChangeSet,
    on_open: Option<(GuidePage, OnOpenGuidePage)>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let hover = cx.theme().list_hover;
    let mut line = crate::surface::flat_row()
        .id(id)
        .flex()
        .w_full()
        .min_w_0()
        .h(px(36.))
        .px_3()
        .gap_3()
        .items_center()
        .border_b_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        .text_sm()
        .children(glyph.map(|glyph| {
            gpui_component::Icon::from(glyph)
                .with_size(px(14.))
                .text_color(muted)
        }))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(guide_file_count_label(set.file_count()))),
        )
        .child(crate::diff_pane::counts(set.additions, set.deletions, cx))
        .child(
            gpui_component::Icon::from(crate::icons::registry::UI_CHEVRON_RIGHT)
                .with_size(px(14.))
                .text_color(muted),
        );
    if let Some((page, on_open)) = on_open {
        line = line
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, window, cx| on_open(page, window, cx));
    }
    line.into_any_element()
}

/// EXP-1251 — the back row over a Guide diff page (web `GuideSectionDiff`'s
/// header): `← Guide` · `02 / 06` · the section title · `+A −D · N files`.
pub(crate) fn guide_page_header(
    view: &GuidePageView,
    on_back: std::rc::Rc<dyn Fn(&mut Window, &mut App)>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let back = crate::controls::text_button(
        "guide-page-back",
        domain::contract::DIFF_UI_GUIDE_FACE,
        crate::controls::TextButtonVariant::Text,
        cx,
    )
    .flex_shrink_0()
    .text_sm()
    .on_click(move |_, window, cx| on_back(window, cx));
    let back = h_flex()
        .flex_shrink_0()
        .gap_1()
        .items_center()
        .child(
            gpui_component::Icon::from(crate::icons::registry::UI_BACK)
                .with_size(px(14.))
                .text_color(muted),
        )
        .child(back);
    h_flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .px(px(WORK_GUTTER))
        .py_2()
        .gap_3()
        .items_center()
        .child(back)
        .child(div().w(px(1.)).h(px(14.)).bg(theme::tokens::glass::STROKE_ROW.to_hsla()))
        .children(view.caption.clone().map(|caption| {
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(caption))
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .child(SharedString::from(view.title.clone())),
        )
        .child(crate::diff_pane::counts(view.additions, view.deletions, cx))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(guide_file_count_label(view.paths.len()))),
        )
        .into_any_element()
}

/// A stack row was picked (its pull request id, EXP-1294).
pub(crate) type OnPickStackRow = std::rc::Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The pointer entered a stack row (`Some(pull request id)`) or left it
/// (`None`).
pub(crate) type OnHoverStackRow = std::rc::Rc<dyn Fn(Option<String>, &mut Window, &mut App)>;

/// EXP-1248 — the Guide's Stack card (web `GuideBody`'s Stack card): a
/// `Stack` band (the `pr-stack` glyph, NO count) over the shared stack rail
/// ([`crate::pr_rows::stack_rail`]) TOP first, the current member wearing
/// the active wash, down to the muted base-branch row. A click REPLACES the
/// subject (`on_pick`); the HOVERED member (`hovered`, the host's view
/// state written by `on_hover`) offers the ghost `Merge through here`.
/// Web `STACK_BASE_FALLBACK`: never an assumed `main` (default branches
/// resolve live).
pub(crate) const STACK_BASE_FALLBACK: &str = "default branch";

pub(crate) fn stack_card(
    view: &domain::pr_stack::StackView,
    // The board's default branch: the base row's label when the bottom
    // pull request's `base` is not synced (then `default branch`).
    default_branch: Option<String>,
    on_pick: Option<OnPickStackRow>,
    on_merge_through: Option<OnPickStackRow>,
    hovered: Option<String>,
    on_hover: Option<OnHoverStackRow>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let members = view
        .rows
        .iter()
        .map(|row| crate::pr_rows::StackRailMember {
            key: row.pull_request_id.clone(),
            identifier: SharedString::from(row.identifier.clone()),
            title: SharedString::from(row.title.clone()),
            current: row.is_current,
            on_open: on_pick.clone().filter(|_| !row.is_current).map(|on_pick| {
                let pull_request_id = row.pull_request_id.clone();
                Box::new(move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
                    on_pick(&pull_request_id, window, cx)
                }) as crate::run_rows::RunRowAction
            }),
            trailing: None,
        })
        .collect();
    let rail = crate::pr_rows::stack_rail(
        crate::pr_rows::StackRailSpec {
            id_prefix: SharedString::from("guide-stack"),
            members,
            base_branch: SharedString::from(
                view.base_branch
                    .clone()
                    .or(default_branch)
                    .unwrap_or_else(|| STACK_BASE_FALLBACK.to_string()),
            ),
            word: None,
            hovered,
            on_hover,
            on_merge_through,
        },
        cx,
    );
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(crate::surface::glass_section_band(
            Some(
                gpui_component::Icon::from(crate::icons::registry::PR_STACK)
                    .with_size(px(14.))
                    .text_color(muted)
                    .into_any_element(),
            ),
            SharedString::from("Stack"),
            None,
            cx,
        ))
        .child(rail)
        .into_any_element()
}

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
    viewer: Option<GuideViewerSource>,
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
                    self.viewer.as_ref(),
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
    viewer: Option<&GuideViewerSource>,
    cx: &mut App,
) -> AnyElement {
    let viewer = viewer.cloned();
    let attachment_id = entry.attachment_id.clone();
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
                        // EXP-1283: a Guide picture opens THE viewer.
                        if let Some(viewer) = viewer.clone() {
                            crate::image_preview::open_guide_viewer(
                                viewer,
                                attachment_id.clone(),
                                images.clone(),
                                window,
                                cx,
                            );
                            return;
                        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff_pane::PaneFile;
    use domain::diff::DiffStatus;

    fn pane_file(path: &str, previous: Option<&str>, additions: u32, deletions: u32) -> PaneFile {
        let status = if previous.is_some() { DiffStatus::Renamed } else { DiffStatus::Modified };
        PaneFile::from_parts(path, status, additions, deletions).with_previous_path(previous)
    }

    /// EXP-1294: the Guide's Changes rows — a section claims the diff files
    /// its report names (a rename by its source too), every other file lands
    /// in Other changes, and Show complete diff takes them all.
    #[test]
    fn guide_sections_claim_their_files_and_the_rest_is_other_changes() {
        let raw = serde_json::json!([
            {"topic": "Summary", "text": "What changed."},
            {"topic": "Sidebar", "text": "The rail.", "files": ["ui/src/sidebar.rs", "ui/src/old_name.rs"]},
        ]);
        let groups = domain::session_results::parse_session_result_groups(Some(&raw));
        let files = vec![
            pane_file("ui/src/sidebar.rs", None, 10, 2),
            pane_file("ui/src/new_name.rs", Some("ui/src/old_name.rs"), 1, 1),
            pane_file("ui/src/lib.rs", None, 3, 0),
        ];
        let diff = guide_diff_files(&files);
        assert_eq!(diff[1].previous_path.as_deref(), Some("ui/src/old_name.rs"));

        let section = guide_page_view(&groups, &diff, GuidePage::Section(1), false).expect("section");
        assert_eq!(section.title, "Sidebar");
        assert_eq!(section.paths, vec!["ui/src/sidebar.rs", "ui/src/new_name.rs"]);
        assert_eq!((section.additions, section.deletions), (11, 3));

        let other = guide_page_view(&groups, &diff, GuidePage::Other, false).expect("other");
        assert_eq!(other.title, domain::session_results::GUIDE_OTHER_CHANGES_TOPIC);
        assert_eq!(other.paths, vec!["ui/src/lib.rs"]);

        let all = guide_page_view(&groups, &diff, GuidePage::All, false).expect("all");
        assert_eq!(all.paths.len(), 3);
        assert_eq!((all.additions, all.deletions), (14, 3));
    }
}
