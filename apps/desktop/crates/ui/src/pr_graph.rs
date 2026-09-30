//! EXP-897 §4 — the ONE stack/batch badge and its graph dialog.
//!
//! Every face of a top tab (Issue · Run · Changes) shares one work header, so
//! it shares ONE badge. SLOP-16 round 2: the badge is a muted ICON BUTTON
//! (the header `…` button's box) whose glyph names the SHAPE
//! ([`domain::pr_graph::badge_shape`], face-independent since EXP-1097):
//! `pr-stack` for a stack (with or without a batch), `pr-batch`,
//! `session-tree` for a run family, `relation-blocked-by` for open blockers
//! alone; a mono `+N` beside it counts the others
//! ([`domain::pr_graph::badge_chip`], ×4) and the shape's name rides the
//! tooltip. It no longer repeats the title the header already shows.
//!
//! SLOP-16 round 3: a click opens "Related work", ONE layout ×4 — the
//! standard native dialog, per section ([`domain::pr_graph::overlay_sections`],
//! the face that is up choosing which LEADS) a group band over the flat rows
//! the product already draws: "Blocked by" = the COMPACT blocks graph, "In
//! batch with"/"Issues" = the relations card's issue rows, "Runs" = the
//! session tree's rows, "Pull requests" = the Reviews queue's rows bottom-up
//! ("Merge stack" on the bottom one). A batch's issues show ONCE: under
//! their band, else folded under their PR row. Copy:
//! [`domain::pr_graph::overlay_copy`].
//!
//! Every click inside the dialog closes it and acts in the opener. The
//! Reviews list keeps its batch popover ([`batch_glyph`]), the same rows.
//!
//! The model is [`domain::pr_graph`] — this module is presentation only.

use gpui::{
    div, prelude::FluentBuilder as _, px, size, AnyElement, App, AppContext as _, ClickEvent,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::Button,
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use domain::pr_graph::{self, overlay_copy, BadgeChip, BadgeShape, PrGraph};
use domain::pr_stack;
use domain::rows::{CodingSession, Issue};

use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};
use crate::issue_relations::IssueRowOpts;
use crate::surface::{glass_pill_button, PillSize};

/// Which face the badge is rendered on — it orders the overlay's sections
/// (the domain's `PrGraphFace`; the chip rule itself ignores it, EXP-1097).
pub(crate) use domain::pr_graph::PrGraphFace as BadgeFace;

/// Everything one badge draws: the graph (its `blocked_by` included,
/// EXP-1097) and the face.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BadgeSpec {
    pub graph: PrGraph,
    pub face: BadgeFace,
    /// EXP-980: the transitive `blocks` graph around the subject issue — the
    /// "Blocked by" section draws THIS where a flat chip list used to sit.
    /// Empty when the subject has no open blocker (and for an issue-less run).
    pub blocks_graph: domain::issue_graph::IssueGraph,
    /// The issue the badge is ON (its batch band lists its PARTNERS).
    pub subject_issue_id: Option<String>,
    /// The run the badge is ON (its row wears the active fill).
    pub subject_session_id: Option<String>,
}

impl BadgeSpec {
    /// The badge's count source — [`pr_graph::badge_chip`], the SAME on
    /// every face (EXP-1097).
    pub(crate) fn chip(&self) -> Option<BadgeChip> {
        pr_graph::badge_chip(&self.graph)
    }
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Every issue of the team `issue` belongs to — the scope the branch-matching
/// stack rule needs (the caller owns the scoping, exactly like the web twin).
fn team_issues(issue: &Issue, cx: &App) -> Vec<Issue> {
    let Some(store) = sync::Store::try_global(cx) else {
        return vec![issue.clone()];
    };
    let collections = store.collections();
    let boards = collections.boards.read(cx);
    let Some(team_id) = boards.get(&issue.board_id).map(|board| board.team_id.clone()) else {
        return vec![issue.clone()];
    };
    collections
        .issues
        .read(cx)
        .iter()
        .filter(|row| {
            boards
                .get(&row.board_id)
                .is_some_and(|board| board.team_id == team_id)
        })
        .cloned()
        .collect()
}

/// EXP-736 — the issues BLOCKING `issue_id`: canonical `blocks` rows whose
/// `related_issue_id` is this issue (the inverse side, "blocked by"). Rows
/// whose blocker has not synced are dropped; a blocker that is done,
/// cancelled or a duplicate no longer blocks anything.
pub(crate) fn blocked_by(issue_id: &str, cx: &App) -> Vec<Issue> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    // EXP-980: the ONE `openBlockers` rule (`chat_launch::blockers_of`), not
    // a second copy of it — this used to re-derive the inverse-side filter.
    let relations = collections.relations_for_issue(issue_id, cx);
    let mut blockers: Vec<Issue> =
        crate::chat_launch::blockers_of(issue_id, &relations, |id| issues.get(id))
            .into_iter()
            .filter(|blocker| !status_is_closed(blocker))
            .cloned()
            .collect();
    // EXP-1097: plain identifier order, byte-for-byte web `openBlockers`
    // (`lib/stack-start.ts`) and the natives' — the `blocked` chip's front
    // issue is the FIRST blocker, so a natural sort here led with a
    // different issue than the other three clients.
    blockers.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    blockers.dedup_by(|a, b| a.id == b.id);
    blockers
}

/// A blocker that is done, cancelled or a duplicate blocks nothing any more
/// (the ×4 `openBlockers` rule).
fn status_is_closed(issue: &Issue) -> bool {
    matches!(
        issue.status,
        domain::IssueStatus::Done | domain::IssueStatus::Cancelled | domain::IssueStatus::Duplicate
    )
}

/// The badge spec for an issue-bound face. `session` contributes the run tree.
pub(crate) fn issue_spec(
    issue: &Issue,
    session: Option<&CodingSession>,
    face: BadgeFace,
    cx: &App,
) -> BadgeSpec {
    let issues = team_issues(issue, cx);
    let sessions: Vec<CodingSession> = sync::Store::try_global(cx)
        .map(|store| store.collections().coding_sessions.read(cx).iter().cloned().collect())
        .unwrap_or_default();
    let mut graph = pr_graph::pr_graph(Some(issue), session, &issues, &sessions);
    // EXP-1097: the open blockers ride the GRAPH (web `blockedBy`) — they
    // earn the chip on every face, not just the Issue one.
    graph.blocked_by = blocked_by(&issue.id, cx);
    let blocks_graph = if graph.blocked_by.is_empty() {
        domain::issue_graph::IssueGraph::default()
    } else {
        crate::issue_graph::graph_for(&[issue.id.as_str()], cx)
    };
    BadgeSpec {
        graph,
        face,
        blocks_graph,
        subject_issue_id: Some(issue.id.clone()),
        subject_session_id: session.map(|session| session.id.clone()),
    }
}

/// The badge spec for an issue-LESS run (a chat / action / batch run): no
/// issue face, so only the run tree and the run's own pull request.
pub(crate) fn session_spec(session: &CodingSession, face: BadgeFace, cx: &App) -> BadgeSpec {
    let (issues, sessions) = match sync::Store::try_global(cx) {
        Some(store) => {
            let collections = store.collections();
            let issues: Vec<Issue> = collections.issues.read(cx).iter().cloned().collect();
            let sessions: Vec<CodingSession> =
                collections.coding_sessions.read(cx).iter().cloned().collect();
            (issues, sessions)
        }
        None => (Vec::new(), Vec::new()),
    };
    BadgeSpec {
        graph: pr_graph::pr_graph(None, Some(session), &issues, &sessions),
        face,
        blocks_graph: domain::issue_graph::IssueGraph::default(),
        subject_issue_id: None,
        subject_session_id: Some(session.id.clone()),
    }
}

// ---------------------------------------------------------------------------
// The badge
// ---------------------------------------------------------------------------

/// The runs-only shape's name — byte-identical with web `RUNS_BADGE_NAME`.
pub(crate) const RUNS_BADGE_NAME: &str = "The runs around this one";

/// The badge's NAME per shape (web `PrGraphBadge`'s `name`): the icon
/// button's tooltip and the graph dialog's title.
pub(crate) fn badge_name(shape: BadgeShape) -> &'static str {
    match shape {
        BadgeShape::Stack => "Pull request stack",
        BadgeShape::Batch => "Batch pull request",
        BadgeShape::StackAndBatch => "Stack and batch",
        BadgeShape::Runs => RUNS_BADGE_NAME,
        BadgeShape::Blocked => "Blocked by",
    }
}

/// The badge's glyph per shape — a concept, never a raw glyph.
pub(crate) fn badge_icon(shape: BadgeShape) -> ExpIcon {
    match shape {
        BadgeShape::Stack | BadgeShape::StackAndBatch => registry::PR_STACK,
        BadgeShape::Batch => registry::PR_BATCH,
        BadgeShape::Runs => registry::SESSION_TREE,
        BadgeShape::Blocked => registry::RELATION_BLOCKED_BY,
    }
}

/// What the icon button shows: its shape and the `+N` beside the glyph
/// ([`pr_graph::badge_chip`]'s count; 0 = no count).
pub(crate) fn badge_face(spec: &BadgeSpec) -> Option<(BadgeShape, usize)> {
    let shape = pr_graph::badge_shape(&spec.graph)?;
    Some((shape, spec.chip().map_or(0, |chip| chip.count)))
}

/// SLOP-16 round 2 — the header badge: a muted ICON BUTTON (the header `…`
/// button's box), the glyph naming the shape, a mono `+N` beside it; the
/// name rides the tooltip. A click opens the graph DIALOG. `None` when there
/// is nothing to show.
pub(crate) fn badge(id: &'static str, spec: BadgeSpec, cx: &App) -> Option<AnyElement> {
    let (shape, count) = badge_face(&spec)?;
    let name = badge_name(shape);
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    let hover = cx.theme().list_hover;
    let size = crate::controls::CTL_MD_H;
    Some(
        h_flex()
            .id(SharedString::from(format!("{id}-button")))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap_1()
            .h(px(size))
            .min_w(px(size))
            .when(count > 0, |button| button.px_2())
            .rounded(px(theme::tokens::radius::MD))
            .cursor_pointer()
            .text_color(muted)
            .hover(move |style| style.bg(hover).text_color(foreground))
            .child(Icon::new(badge_icon(shape)).with_size(px(BADGE_GLYPH)))
            .when(count > 0, |button| {
                button.child(
                    div()
                        .text_xs()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(SharedString::from(format!("+{count}"))),
                )
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(name).build(window, cx)
            })
            .on_click(move |_: &ClickEvent, window, cx| {
                cx.stop_propagation();
                open_graph_dialog(spec.clone(), window, cx);
            })
            .into_any_element(),
    )
}

/// The header icon buttons' glyph (the `…` button's small-button glyph).
const BADGE_GLYPH: f32 = 14.;

// ---------------------------------------------------------------------------
// The "Related work" dialog
// ---------------------------------------------------------------------------

/// The dialog's content width (the standard dialog width).
const DIALOG_W: f32 = 560.;
/// The dialog shell's standard padding around a padded content view.
const DIALOG_PAD: f32 = 16.;
/// The floor of the fitted height.
const DIALOG_MIN_H: f32 = 160.;
/// Height estimates the fitted height sums (the dialog scrolls past them).
const BAND_H: f32 = 36.;
const ISSUE_ROW_H: f32 = 28.;
const TALL_ROW_H: f32 = 58.;
const SECTION_GAP: f32 = 12.;

/// SLOP-16 round 3 — open "Related work": the platform's standard modal,
/// 560 wide, as tall as its content (capped at 85% of the opener).
fn open_graph_dialog(spec: BadgeSpec, window: &mut Window, cx: &mut App) {
    let viewport = window.viewport_size();
    let height = px(content_height(&spec).max(DIALOG_MIN_H)).min(viewport.height * 0.85);
    let dialog = crate::native_dialog::DialogSpec::new(
        overlay_copy::RELATED_WORK_TITLE,
        size(px(DIALOG_W).min(viewport.width * 0.9), height),
    )
    .resizable(size(px(420.), px(DIALOG_MIN_H)));
    crate::native_dialog::open_dialog_window(window, cx, dialog, move |_window, cx| {
        crate::native_dialog::DialogContent::new(cx.new(|_| GraphDialog { spec }))
    });
}

/// The fitted content height: the padding, then per section its band and
/// its rows.
fn content_height(spec: &BadgeSpec) -> f32 {
    use pr_graph::OverlaySection;
    let sections = pr_graph::overlay_sections(&spec.graph, spec.face);
    if sections.is_empty() {
        return 2. * DIALOG_PAD + ISSUE_ROW_H;
    }
    let mut height = 2. * DIALOG_PAD + SECTION_GAP * (sections.len() - 1) as f32;
    for section in &sections {
        height += BAND_H
            + match section {
                OverlaySection::Blocked => {
                    // SLOP-16 r4: vertical, so the waves set the height.
                    let nodes = &spec.blocks_graph.nodes;
                    let waves = nodes.iter().map(|node| node.wave + 1).max().unwrap_or(1);
                    let lanes = nodes.iter().map(|node| node.lane + 1).max().unwrap_or(1);
                    use domain::issue_graph::geometry::*;
                    size_with(COMPACT_NODE_WIDTH, waves, lanes).view_height + 8.
                }
                OverlaySection::Batch => batch_issues(spec).len() as f32 * ISSUE_ROW_H,
                OverlaySection::Runs => spec.graph.tree.len() as f32 * TALL_ROW_H,
                OverlaySection::Stack => {
                    let folds = !sections.contains(&OverlaySection::Batch);
                    stack_members(spec)
                        .iter()
                        .map(|member| {
                            TALL_ROW_H
                                + if folds && member.entry.is_batch() {
                                    member.entry.issues.len() as f32 * ISSUE_ROW_H
                                } else {
                                    0.
                                }
                        })
                        .sum()
                }
            };
    }
    height
}

/// The dialog body: every section the subject has ([`overlay`]).
struct GraphDialog {
    spec: BadgeSpec,
}

impl Render for GraphDialog {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The content box follows the window (it resizes); a (rare) wide
        // graph scrolls sideways past it, the dialog's scroller the height.
        let width = (f32::from(window.viewport_size().width) - 2. * DIALOG_PAD).max(240.);
        overlay(&self.spec, width, cx)
    }
}

/// Run `f` in the window that opened this dialog, closing the dialog first
/// (a dialog window has no navigation of its own).
fn in_opener(window: &mut Window, cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    crate::native_dialog::close_then(window, cx, f);
}

// ---------------------------------------------------------------------------
// The overlay
// ---------------------------------------------------------------------------

/// SLOP-16 round 3 — every section the subject has, the face's own first
/// ([`pr_graph::overlay_sections`]): each the GROUP BAND over FLAT rows the
/// product already draws — the relations card's issue row, the Reviews
/// queue's PR row, the session tree's run row, the compact blocks graph.
fn overlay(spec: &BadgeSpec, width: f32, cx: &mut App) -> AnyElement {
    use pr_graph::OverlaySection;
    let sections = pr_graph::overlay_sections(&spec.graph, spec.face);
    if sections.is_empty() {
        return div()
            .px_3()
            .py_1()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(overlay_copy::EMPTY)
            .into_any_element();
    }
    // A batch's issues show ONCE: under the batch band when it is drawn,
    // else folded under their PR row.
    let fold_batches = !sections.contains(&OverlaySection::Batch);
    let mut column = v_flex().w_full().min_w_0().gap_3();
    for section in sections.iter().copied() {
        let rows: Vec<AnyElement> = match section {
            // EXP-980: the transitive blocks GRAPH, compact: small chips,
            // vertical (waves top to bottom), sized to the graph.
            OverlaySection::Blocked => vec![crate::issue_graph::graph_in_dialog_compact(
                &spec.blocks_graph,
                width,
                cx,
            )],
            OverlaySection::Batch => batch_issues(spec)
                .iter()
                .map(|issue| {
                    crate::issue_relations::issue_row(
                        &format!("pr-graph-batch-{}", issue.id),
                        issue,
                        dialog_issue_row(None),
                        cx,
                    )
                })
                .collect(),
            OverlaySection::Runs => run_rows(spec, cx),
            OverlaySection::Stack => stack_rows(spec, fold_batches, cx),
        };
        column = column.child(
            v_flex()
                .w_full()
                .min_w_0()
                .child(crate::surface::glass_section_band(
                    None,
                    overlay_copy::band_label(section, spec.face),
                    None,
                    cx,
                ))
                .children(rows),
        );
    }
    column.into_any_element()
}

/// The options of an issue row drawn in the dialog.
fn dialog_issue_row(guides: Option<domain::tree_guides::Guides>) -> IssueRowOpts {
    IssueRowOpts {
        title: None,
        open: true,
        remove: None,
        guides,
        in_dialog: true,
    }
}

/// The batch band's issues: on a run the covered set IS the subject, so all
/// of them; on an issue (or its changes) the subject's PARTNERS.
fn batch_issues(spec: &BadgeSpec) -> Vec<Issue> {
    let Some(batch) = spec.graph.batch.as_ref() else {
        return Vec::new();
    };
    batch
        .issues
        .iter()
        .filter(|issue| {
            spec.face == BadgeFace::Run || spec.subject_issue_id.as_deref() != Some(issue.id.as_str())
        })
        .cloned()
        .collect()
}

/// The run tree — the session lists' own rows, nested; a click opens the run.
fn run_rows(spec: &BadgeSpec, cx: &App) -> Vec<AnyElement> {
    let now = chrono::Utc::now().timestamp();
    // EXP-965: the connector, off the tree's depth sequence.
    let guides = domain::tree_guides::guides_for(
        &spec.graph.tree.iter().map(|row| row.depth).collect::<Vec<_>>(),
    );
    spec.graph
        .tree
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let session_id = row.session.id.clone();
            let active = spec.subject_session_id.as_deref() == Some(session_id.as_str());
            crate::run_rows::render_run_list_row(
                "pr-graph-run",
                index,
                guides.get(index).cloned().unwrap_or_default(),
                None,
                crate::run_rows::RunListFacts::derive(&row.session, now, cx),
                active,
                Box::new(move |_: &ClickEvent, window, cx| {
                    let session_id = session_id.clone();
                    in_opener(window, cx, move |window, cx| {
                        crate::session_screen::open_session_with_origin(&session_id, None, window, cx);
                    });
                }),
                cx,
            )
        })
        .collect()
}

/// The PR stack BOTTOM first — or, on the Changes face, its lone pull
/// request (EXP-1097: that face leads with its PR even unstacked).
fn stack_members(spec: &BadgeSpec) -> Vec<pr_graph::StackEntry> {
    if spec.graph.stack.is_empty() {
        spec.graph
            .entry
            .iter()
            .map(|entry| pr_graph::StackEntry {
                entry: entry.clone(),
                depth: 0,
            })
            .collect()
    } else {
        spec.graph.stack.clone()
    }
}

/// The PR stack, BOTTOM first, as the Reviews queue's rows: a batch entry
/// folds its issues underneath (unless the batch band lists them), "Merge
/// stack" rides the bottom row of a real stack; a click opens the diff.
fn stack_rows(spec: &BadgeSpec, fold_batches: bool, cx: &mut App) -> Vec<AnyElement> {
    let stack = stack_members(spec);
    let size = stack.len();
    let muted = cx.theme().muted_foreground;
    // EXP-965: the connector needs the WHOLE visible sequence up front — a
    // folded batch puts its issues one level deeper, interleaved.
    let folds = |member: &pr_graph::StackEntry| fold_batches && member.entry.is_batch();
    let mut depths: Vec<usize> = Vec::new();
    for member in &stack {
        depths.push(member.depth);
        if folds(member) {
            depths.extend(member.entry.issues.iter().map(|_| member.depth + 1));
        }
    }
    let guides = domain::tree_guides::guides_for(&depths);
    let guide_at = |position: usize| guides.get(position).cloned().unwrap_or_default();
    let mut rows: Vec<AnyElement> = Vec::with_capacity(depths.len());
    for (index, member) in stack.iter().enumerate() {
        let entry = &member.entry;
        let representative = entry.representative();
        let issue_id = representative.id.clone();
        // The member the dialog is about wears the active fill.
        let current =
            entry.branch().is_some() && entry.branch() == spec.graph.subject_branch.as_deref();
        let state = representative
            .pr_state
            .clone()
            .unwrap_or_else(|| "open".to_string());
        let (identifier, title) = crate::reviews_view::pr_row_texts(&entry.issues);
        let mut trailing = h_flex().flex_shrink_0().items_center().gap_2().child(
            div()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(state.clone())),
        );
        // The BOTTOM entry carries the whole-stack merge.
        if index == 0 && size > 1 {
            trailing = trailing.child(merge_stack_control(&issue_id, size));
        }
        let nav_id = issue_id.clone();
        rows.push(crate::reviews_view::pr_row(
            crate::reviews_view::PrRowSpec {
                id: SharedString::from(format!("pr-graph-stack-{issue_id}")),
                identifier,
                title,
                pr_state: Some(state),
                guides: guide_at(rows.len()),
                selected: current,
                fold: None,
                batch_glyph: entry.is_batch().then(|| {
                    Icon::new(registry::PR_BATCH)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(muted)
                        .into_any_element()
                }),
                stacked_on: None,
                sub: entry.branch().map(str::to_string),
                error: None,
                trailing: trailing.into_any_element(),
                on_click: Box::new(move |_: &ClickEvent, window, cx| {
                    let screen = crate::navigation::Screen::PrDiff {
                        issue_id: nav_id.clone(),
                    };
                    in_opener(window, cx, move |window, cx| {
                        crate::navigation::navigate(window, cx, screen);
                    });
                }),
            },
            cx,
        ));
        if folds(member) {
            for issue in &entry.issues {
                let guides = guide_at(rows.len());
                rows.push(crate::issue_relations::issue_row(
                    &format!("pr-graph-fold-{issue_id}-{}", issue.id),
                    issue,
                    dialog_issue_row(Some(guides)),
                    cx,
                ));
            }
        }
    }
    rows
}

/// "Merge stack" — the whole chain, bottom-up, behind the shared confirm
/// dialog (the Reviews queue's button). The call rides the BOTTOM member's
/// issue id; the server resolves the top itself. Dialogs never nest, so the
/// graph dialog closes and the confirm opens over the opener.
fn merge_stack_control(issue_id: &str, size: usize) -> AnyElement {
    let issue_id = issue_id.to_string();
    Button::new(SharedString::from(format!("pr-graph-merge-{issue_id}")))
        .web_sm()
        .outline()
        .cursor_pointer()
        .icon(Icon::new(registry::PR_STACK))
        .label(pr_stack::MERGE_STACK_LABEL)
        .on_click(move |_: &ClickEvent, window, cx| {
            cx.stop_propagation();
            let issue_id = issue_id.clone();
            in_opener(window, cx, move |window, cx| {
                crate::native_dialog::open_alert(
                    window,
                    cx,
                    crate::native_dialog::AlertSpec::new(
                        pr_stack::MERGE_STACK_DIALOG_TITLE,
                        pr_stack::merge_stack_dialog_body(size),
                        pr_stack::MERGE_STACK_LABEL,
                    )
                    .on_ok(move |_, cx| {
                        crate::pr_merge::fire_confirmed(
                            crate::pr_merge::MergeOp::MergeIssuePr {
                                issue_id: issue_id.clone(),
                                merge_stack: true,
                            },
                            cx,
                        );
                        true
                    }),
                )
            });
        })
        .into_any_element()
}

/// The Reviews popover's width — issue rows, not a graph.
const POPOVER_W: f32 = 360.;

/// The Reviews list's batch glyph: the `pr-batch` concept with the batch's
/// issues in a popover — the SAME issue rows the "Related work" dialog draws.
pub(crate) fn batch_glyph(id: SharedString, issues: Vec<Issue>, cx: &App) -> AnyElement {
    let count = issues.len();
    let muted = cx.theme().muted_foreground;
    let trigger = glass_pill_button(id.clone(), PillSize::Sm, cx)
        .icon(
            Icon::new(registry::PR_BATCH)
                .with_size(px(PillSize::Sm.glyph()))
                .text_color(muted),
        )
        .label(SharedString::from(format!("{count} issues")))
        .tooltip(SharedString::from(format!(
            "This pull request closes {count} issues"
        )));
    gpui_component::popover::Popover::new(SharedString::from(format!("{id}-batch")))
        .p_1()
        .trigger(trigger)
        .content(move |_, _window, cx| {
            let rows: Vec<AnyElement> = issues
                .iter()
                .map(|issue| {
                    crate::issue_relations::issue_row(
                        &format!("review-batch-issue-{}", issue.id),
                        issue,
                        IssueRowOpts {
                            title: None,
                            open: true,
                            remove: None,
                            guides: None,
                            in_dialog: false,
                        },
                        cx,
                    )
                })
                .collect();
            v_flex().w(px(POPOVER_W)).min_w_0().children(rows)
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(identifier: &str, head: Option<&str>, base: Option<&str>) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": format!("id-{identifier}"),
            "board_id": "board-1",
            "number": 1,
            "identifier": identifier,
            "title": identifier,
            "status": "in_review",
            "branch": head,
            "pr_base_branch": base,
            "pr_state": "open",
            "pr_url": head.map(|head| format!("https://github.com/o/r/pull/{head}")),
        }))
        .unwrap()
    }

    fn spec(face: BadgeFace, issues: &[Issue]) -> BadgeSpec {
        BadgeSpec {
            graph: pr_graph::pr_graph(Some(&issues[0]), None, issues, &[]),
            face,
            blocks_graph: domain::issue_graph::IssueGraph::default(),
            subject_issue_id: Some(issues[0].id.clone()),
            subject_session_id: None,
        }
    }

    const FACES: [BadgeFace; 3] = [BadgeFace::Issue, BadgeFace::Run, BadgeFace::Changes];

    /// SLOP-16 round 2 — every shape names itself (the tooltip + the dialog
    /// title, byte-identical with web) and wears its own concept glyph.
    #[test]
    fn every_shape_has_a_name_and_a_glyph() {
        assert_eq!(badge_name(BadgeShape::Runs), RUNS_BADGE_NAME);
        assert_eq!(badge_name(BadgeShape::Blocked), "Blocked by");
        assert_eq!(badge_name(BadgeShape::Stack), "Pull request stack");
        use gpui_component::IconNamed as _;
        let path = |icon: ExpIcon| icon.path().to_string();
        assert_eq!(path(badge_icon(BadgeShape::Stack)), path(registry::PR_STACK));
        assert_eq!(path(badge_icon(BadgeShape::StackAndBatch)), path(registry::PR_STACK));
        assert_eq!(path(badge_icon(BadgeShape::Batch)), path(registry::PR_BATCH));
        assert_eq!(path(badge_icon(BadgeShape::Runs)), path(registry::SESSION_TREE));
        assert_eq!(
            path(badge_icon(BadgeShape::Blocked)),
            path(registry::RELATION_BLOCKED_BY)
        );
    }

    /// EXP-1097: with nothing around the issue the badge stays away — on
    /// every face alike; open blockers alone earn it on every face too.
    #[test]
    fn the_badge_is_face_independent() {
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        for face in FACES {
            assert!(badge_face(&spec(face, &lone)).is_none(), "{face:?}");
            let mut blocked = spec(face, &lone);
            blocked.graph.blocked_by = vec![issue("EXP-29", None, None)];
            assert_eq!(badge_face(&blocked), Some((BadgeShape::Blocked, 0)), "{face:?}");
        }
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        for face in FACES {
            assert_eq!(
                badge_face(&spec(face, &stacked)),
                badge_face(&spec(BadgeFace::Issue, &stacked))
            );
        }
    }

    /// EXP-1058 — `+N` counts every other issue on the stack; EXP-1097:
    /// blockers alone count all but the first.
    #[test]
    fn the_count_is_everything_but_the_front() {
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        assert_eq!(
            badge_face(&spec(BadgeFace::Changes, &stacked)),
            Some((BadgeShape::Stack, 1))
        );
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        let mut blocked = spec(BadgeFace::Run, &lone);
        blocked.graph.blocked_by = vec![issue("EXP-28", None, None), issue("EXP-29", None, None)];
        assert_eq!(badge_face(&blocked), Some((BadgeShape::Blocked, 1)));
    }

    /// SLOP-16 round 3 — a batch's issues show ONCE: the band lists the
    /// subject's partners (all of them on a run), and a drawn band stops
    /// the PR row from folding them again.
    #[test]
    fn a_batch_lists_its_issues_once() {
        let pr = "https://github.com/o/r/pull/20";
        let mut a = issue("EXP-20", Some("exp/batch-1"), Some("master"));
        let mut b = issue("EXP-21", Some("exp/batch-1"), Some("master"));
        a.pr_url = Some(pr.to_string());
        b.pr_url = Some(pr.to_string());
        let batch = vec![a, b];
        let on_issue = spec(BadgeFace::Issue, &batch);
        let partners: Vec<_> =
            batch_issues(&on_issue).into_iter().map(|row| row.identifier).collect();
        assert_eq!(partners, vec!["EXP-21".to_string()]);
        let mut on_run = spec(BadgeFace::Run, &batch);
        on_run.subject_issue_id = None;
        assert_eq!(batch_issues(&on_run).len(), 2);
        let sections = pr_graph::overlay_sections(&on_issue.graph, BadgeFace::Changes);
        assert!(sections.contains(&pr_graph::OverlaySection::Batch));
    }
}
