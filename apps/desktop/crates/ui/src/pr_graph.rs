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
//! A click opens a native DIALOG window titled by that name, listing EVERY
//! relation the subject has ([`domain::pr_graph::overlay_sections`]), the
//! face that is up choosing which section LEADS:
//!
//! * **Issue** — "Blocked by" (EXP-980: the WHOLE transitive `blocks` graph
//!   in full boxes, as wide as the dialog and scrolling past it) and "In
//!   batch with" (the other issues on its pull request, small chips);
//! * **Run** — the run's session tree, nested, live dots, a click opens a run;
//! * **Changes / review** — the PR stack BOTTOM-UP: identifier(s), PR state, a
//!   batch glyph with the batch's issues folded underneath as small chips,
//!   and "Merge stack" on the bottom entry.
//!
//! Every click inside the dialog closes it and acts in the opener. The
//! Reviews list keeps its batch popover ([`batch_glyph`]).
//!
//! The model is [`domain::pr_graph`] — this module is presentation only.

use gpui::{
    div, prelude::FluentBuilder as _, px, size, AnyElement, App, AppContext as _, ClickEvent,
    Hsla, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::pr_graph::{self, BadgeChip, BadgeShape, PrGraph};
use domain::pr_stack;
use domain::rows::{CodingSession, Issue};

use crate::icons::{registry, ExpIcon};
use crate::issue_chip::issue_chip;
use crate::surface::{glass_pill, glass_pill_button, PillMode, PillSize};

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
// The graph dialog
// ---------------------------------------------------------------------------

/// The dialog's content width — the whole graph, not a popover's glance.
const DIALOG_W: f32 = 720.;
/// The dialog's opening height.
const DIALOG_H: f32 = 560.;
/// The dialog shell's standard padding around a padded content view.
const DIALOG_PAD: f32 = 16.;

/// Open the badge's graph as a native dialog window titled by its name.
fn open_graph_dialog(spec: BadgeSpec, window: &mut Window, cx: &mut App) {
    let Some(shape) = pr_graph::badge_shape(&spec.graph) else {
        return;
    };
    let viewport = window.viewport_size();
    let dialog = crate::native_dialog::DialogSpec::new(
        badge_name(shape),
        size(
            px(DIALOG_W).min(viewport.width * 0.9),
            px(DIALOG_H).min(viewport.height * 0.85),
        ),
    )
    .resizable(size(px(420.), px(280.)));
    crate::native_dialog::open_dialog_window(window, cx, dialog, move |_window, cx| {
        crate::native_dialog::DialogContent::new(cx.new(|_| GraphDialog { spec }))
    });
}

/// The dialog body: every section the subject has ([`overlay`]).
struct GraphDialog {
    spec: BadgeSpec,
}

impl Render for GraphDialog {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The content box follows the window (it resizes); the graph scrolls
        // sideways past it, the dialog's scroller takes the height.
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

/// The Reviews batch popover's width — an identifier row, not a graph.
const OVERLAY_W: f32 = 320.;

/// The "Blocked by" graph never scrolls vertically inside the dialog: the
/// dialog's own scroller takes the height.
const DIALOG_GRAPH_MAX_H: f32 = 100_000.;

/// EXP-1097: every section the subject has, the face's own first
/// ([`pr_graph::overlay_sections`]), `width` wide (the dialog's content box).
fn overlay(spec: &BadgeSpec, width: f32, cx: &App) -> AnyElement {
    use pr_graph::OverlaySection;
    let mut column = v_flex().w_full().min_w_0().gap_3();
    for part in pr_graph::overlay_sections(&spec.graph, spec.face) {
        column = column.child(match part {
            // EXP-980: the transitive blocks GRAPH, not a flat chip list —
            // "blocked by EXP-11" never said what blocks EXP-11.
            OverlaySection::Blocked => section(
                "Blocked by",
                // SLOP-16 round 2: the WHOLE graph, full boxes, as wide
                // as the dialog; it scrolls sideways past it.
                vec![crate::issue_graph::graph_in_wide_dialog(
                    &spec.blocks_graph,
                    width,
                    DIALOG_GRAPH_MAX_H,
                    cx,
                )],
                cx,
            ),
            OverlaySection::Batch => section(
                "In batch with",
                spec.graph
                    .batch
                    .as_ref()
                    .map(|batch| issue_rows(&batch.issues, true, cx))
                    .unwrap_or_default(),
                cx,
            ),
            OverlaySection::Runs => section("Runs", run_rows(spec, cx), cx),
            OverlaySection::Stack => section(
                if spec.face == BadgeFace::Changes {
                    "Pull request stack"
                } else {
                    "In this stack"
                },
                stack_rows(spec, cx),
                cx,
            ),
        });
    }
    column.into_any_element()
}

/// One overlay section: a muted caption over its rows.
fn section(label: &'static str, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    v_flex()
        .min_w_0()
        .gap(px(ROW_GAP))
        .child(
            div()
                .px_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground.opacity(0.8))
                .child(label),
        )
        .children(rows)
        .into_any_element()
}

/// The overlay's base left padding — EXP-965's gutters are measured off it.
const ROW_PAD: f32 = 8.;

/// The space [`section`] leaves between two overlay rows. The connector
/// bridges it (EXP-965), so the two numbers are ONE — a `gap_0p5` here and a
/// literal there would drift the moment either moved.
const ROW_GAP: f32 = 0.125 * theme::FONT_SIZE_PX;

/// The shared row shell every section draws — the glass list row, hovered.
/// EXP-965: a NESTED row paints its tree connector in the gutter its indent
/// reserves, exactly like the session lists' rows.
fn row_shell(
    id: SharedString,
    guides: &domain::tree_guides::Guides,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let hover = cx.theme().list_hover;
    crate::surface::flat_row()
        .id(id)
        .flex()
        .w_full()
        .min_w_0()
        .relative()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .pl(px(ROW_PAD + crate::tree_guides::LEVEL_PITCH * guides.depth() as f32))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .children(crate::tree_guides::guide_layer(guides, ROW_PAD, ROW_GAP))
}

fn mono(text: impl Into<SharedString>, color: Hsla) -> gpui::Div {
    div()
        .flex_shrink_0()
        .text_xs()
        .text_color(color)
        .font_family(theme::terminal::FONT_FAMILY)
        .child(text.into())
}

/// SLOP-15/16 — the issues as ONE wrapped row of SMALL chips (the web's
/// `flex-wrap gap-1.5`), each opening its issue; the title rides in the
/// chip's tooltip. `in_dialog` = drawn in the graph dialog, so a click opens
/// the issue in the opener.
fn small_chip_row(issues: &[Issue], in_dialog: bool, cx: &App) -> gpui::Div {
    h_flex()
        .flex_wrap()
        .min_w_0()
        .gap(px(6.))
        .children(issues.iter().map(|issue| {
            let issue_id = issue.id.clone();
            issue_chip(
                SharedString::from(format!("pr-graph-issue-{issue_id}")),
                issue.identifier.clone(),
                issue.title.clone(),
            )
            .small()
            .status(crate::queries::resolve_issue_status(cx, issue))
            .on_click(move |_: &ClickEvent, window, cx| {
                let screen = crate::navigation::Screen::IssueDetail {
                    issue_id: issue_id.clone(),
                };
                if in_dialog {
                    in_opener(window, cx, move |window, cx| {
                        crate::navigation::navigate(window, cx, screen);
                    });
                } else {
                    crate::navigation::navigate(window, cx, screen);
                }
            })
        }))
}

/// "In batch with" (and the Reviews batch popover): ONE wrapped row of small
/// chips, a click opens the issue.
fn issue_rows(issues: &[Issue], in_dialog: bool, cx: &App) -> Vec<AnyElement> {
    vec![small_chip_row(issues, in_dialog, cx)
        .px(px(ROW_PAD))
        .into_any_element()]
}

/// The run tree — nested rows with the list's own live dot; a click opens the
/// run.
fn run_rows(spec: &BadgeSpec, cx: &App) -> Vec<AnyElement> {
    let now = chrono::Utc::now().timestamp();
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    // EXP-965: the connector, off the tree's depth sequence.
    let guides = domain::tree_guides::guides_for(
        &spec.graph.tree.iter().map(|row| row.depth).collect::<Vec<_>>(),
    );
    spec.graph
        .tree
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let session = &row.session;
            let session_id = session.id.clone();
            let live = crate::queries::coding_session_is_live(session, now);
            let dot = if live {
                theme::tokens::GREEN.to_hsla()
            } else {
                muted.opacity(0.4)
            };
            let issue = session.issue_id.as_deref().and_then(|issue_id| {
                sync::Store::try_global(cx)?
                    .collections()
                    .issues
                    .read(cx)
                    .get(issue_id)
                    .cloned()
            });
            // EXP-876: a batch run in the tree names its issues, not "Batch run".
            let batch_issues = crate::run_rows::batch_run_issues(session, cx);
            let title = crate::run_rows::run_title(session, issue.as_ref(), &batch_issues);
            row_shell(
                SharedString::from(format!("pr-graph-run-{session_id}")),
                &guides.get(index).cloned().unwrap_or_default(),
                cx,
            )
            .child(div().flex_shrink_0().size_1p5().rounded_full().bg(dot))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .truncate()
                    .text_color(foreground)
                    .child(title),
            )
            .on_click(move |_: &ClickEvent, window, cx| {
                let session_id = session_id.clone();
                in_opener(window, cx, move |window, cx| {
                    crate::session_screen::open_session_with_origin(&session_id, None, window, cx);
                });
            })
            .into_any_element()
        })
        .collect()
}

/// The PR stack, BOTTOM first: identifier(s), the PR state, the batch glyph
/// over its folded-out issues, and "Merge stack" on the bottom entry.
fn stack_rows(spec: &BadgeSpec, cx: &App) -> Vec<AnyElement> {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    // EXP-1097: the Changes face leads with its pull request even when it is
    // not stacked — a lone entry is then the one row.
    let lone: Vec<pr_graph::StackEntry>;
    let stack: &[pr_graph::StackEntry] = if spec.graph.stack.is_empty() {
        lone = spec
            .graph
            .entry
            .iter()
            .map(|entry| pr_graph::StackEntry {
                entry: entry.clone(),
                depth: 0,
            })
            .collect();
        &lone
    } else {
        &spec.graph.stack
    };
    let size = stack.len();
    // EXP-965: the connector needs the WHOLE visible sequence up front — a
    // batch member folds its issues out one level deeper, so the rows are
    // interleaved and the depths cannot be read off the stack alone.
    let mut depths: Vec<usize> = Vec::with_capacity(size * 2);
    for member in stack.iter() {
        depths.push(member.depth);
        if member.entry.is_batch() {
            // SLOP-16: its folded issues are ONE wrapped row of small chips.
            depths.push(member.depth + 1);
        }
    }
    let guides = domain::tree_guides::guides_for(&depths);
    let guide_at = |position: usize| guides.get(position).cloned().unwrap_or_default();
    let mut rows: Vec<AnyElement> = Vec::with_capacity(size * 2);
    for (index, member) in stack.iter().enumerate() {
        let entry = &member.entry;
        let issue_id = entry.representative().id.clone();
        // The member the badge is ON reads in full foreground.
        let current =
            entry.branch().is_some() && entry.branch() == spec.graph.subject_branch.as_deref();
        let state = entry
            .representative()
            .pr_state
            .clone()
            .unwrap_or_else(|| "open".to_string());
        let mut row = row_shell(
            SharedString::from(format!("pr-graph-stack-{issue_id}")),
            &guide_at(rows.len()),
            cx,
        )
        .child(mono(entry.identifiers(), if current { foreground } else { muted }))
        .child(div().flex_1().min_w_0())
        .when(entry.is_batch(), |row| {
            row.child(
                Icon::new(registry::PR_BATCH)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(muted),
            )
        })
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted.opacity(0.8))
                .child(SharedString::from(state)),
        );
        // The BOTTOM entry carries the whole-stack merge; the alert confirms
        // it (a popover closes on the first click, so a two-click arm here
        // would never be a confirm).
        if index == 0 && size > 1 {
            row = row.child(merge_stack_control(&issue_id, size, cx));
        }
        let nav_id = issue_id.clone();
        rows.push(
            row.on_click(move |_: &ClickEvent, window, cx| {
                let screen = crate::navigation::Screen::PrDiff {
                    issue_id: nav_id.clone(),
                };
                in_opener(window, cx, move |window, cx| {
                    crate::navigation::navigate(window, cx, screen);
                });
            })
            .into_any_element(),
        );
        // A batch member folds its issues out underneath: ONE nested row of
        // small chips (the web's `flex-wrap gap-1.5 pl-5`). The chips click,
        // so the shell neither hovers nor clicks; it only keeps the
        // connector (EXP-965) and the indent.
        if entry.is_batch() {
            let guides = guide_at(rows.len());
            rows.push(
                div()
                    .id(SharedString::from(format!("pr-graph-batch-{issue_id}")))
                    .flex()
                    .w_full()
                    .min_w_0()
                    .relative()
                    .items_center()
                    .py_1()
                    .pr_2()
                    .pl(px(ROW_PAD + crate::tree_guides::LEVEL_PITCH * guides.depth() as f32))
                    .children(crate::tree_guides::guide_layer(&guides, ROW_PAD, ROW_GAP))
                    .child(small_chip_row(&entry.issues, true, cx).flex_1())
                    .into_any_element(),
            );
        }
    }
    rows
}

/// "Merge stack" — the whole chain, bottom-up, behind the shared confirm
/// dialog. The call rides the BOTTOM member's issue id; the server resolves
/// the top itself. Dialogs never nest, so the graph dialog closes and the
/// confirm opens over the opener.
fn merge_stack_control(issue_id: &str, size: usize, cx: &App) -> AnyElement {
    let issue_id = issue_id.to_string();
    glass_pill(
        SharedString::from(format!("pr-graph-merge-{issue_id}")),
        PillSize::Sm,
        PillMode::Action,
        cx,
    )
    .child(
        Icon::new(registry::PR_MERGED)
            .with_size(px(PillSize::Sm.glyph()))
            .text_color(cx.theme().muted_foreground),
    )
    .child(div().child(pr_stack::MERGE_STACK_LABEL))
    .on_click(move |event: &ClickEvent, window, cx| {
        let _ = event;
        cx.stop_propagation();
        let issue_id = issue_id.clone();
        in_opener(window, cx, move |window, cx| crate::native_dialog::open_alert(
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
        ));
    })
    .into_any_element()
}

/// The Reviews list's batch glyph: the `pr-batch` concept with the batch's
/// issues in a popover — the SAME rows the overlay's "In batch with" section
/// draws (EXP-897 §4: one model, one row primitive, everywhere).
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
        .p_2()
        .trigger(trigger)
        .content(move |_, _window, cx| {
            v_flex()
                .w(px(OVERLAY_W))
                .min_w_0()
                .gap_0p5()
                .children(issue_rows(&issues, false, cx))
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
}
