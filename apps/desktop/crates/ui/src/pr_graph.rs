//! EXP-897 §4 — the ONE stack/batch badge and its overlay.
//!
//! Every face of a top tab (Issue · Run · Changes) shares one work header, so
//! it shares ONE badge. EXP-1058: the badge IS a STACKED issue chip
//! ([`crate::issue_chip`], the workflow graph's deck of ghosts) naming the
//! subject pull request's representative issue, with `+N` for every other
//! issue riding the stack or batch ([`domain::pr_graph::badge_chip`], ×4) —
//! or, on the Run face of a run with no issue, the same chip box with the
//! `session-tree` concept and the run's own title, `+N` = the other runs. The
//! chip is INERT: hovering it (desktop = a pointer platform) or clicking it
//! opens a popover whose SECTION depends on the face that is up, built from
//! the same row primitives and the same copy so the three read as one thing:
//!
//! * **Issue** — "Blocked by" (EXP-980: the transitive `blocks` GRAPH around
//!   this issue, `crate::issue_graph`, where a flat chip list used to sit)
//!   and "In batch with" (the other issues on its pull request);
//! * **Run** — the run's session tree, nested, live dots, a click opens a run;
//! * **Changes / review** — the PR stack BOTTOM-UP: identifier(s), PR state, a
//!   batch glyph with the batch's issues folded underneath, and "Merge stack"
//!   on the bottom entry.
//!
//! The model is [`domain::pr_graph`] — this module is presentation only.

use std::time::Duration;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, ClickEvent, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::pr_graph::{self, BadgeChip, PrGraph};
use domain::pr_stack;
use domain::rows::{CodingSession, Issue};
use domain::statuses::ResolvedStatus;

use crate::icons::registry;
use crate::issue_chip::{issue_chip, ISSUE_CHIP_ICON_MAX, ISSUE_CHIP_STACK_STEP};
use crate::surface::{glass_pill, glass_pill_button, PillMode, PillSize};

/// Which face the badge is rendered on — it decides the overlay's sections
/// (the domain's `PrGraphFace`, which the chip rule reads too).
pub(crate) use domain::pr_graph::PrGraphFace as BadgeFace;

/// Everything one badge draws: the graph, the face, and the blocked-by
/// relations (the only part of the Issue face that is not in the graph).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BadgeSpec {
    pub graph: PrGraph,
    pub face: BadgeFace,
    pub blocked_by: Vec<Issue>,
    /// EXP-980: the transitive `blocks` graph around the subject issue — the
    /// Issue face draws THIS where a flat chip list used to sit. Empty on the
    /// Run and Changes faces (and for an issue-less run).
    pub blocks_graph: domain::issue_graph::IssueGraph,
    /// The subject run's own title — what the front chip names when there is
    /// no issue to name (an issue-less run's family). `None` on issue faces.
    pub run_title: Option<SharedString>,
}

impl BadgeSpec {
    /// What the chip draws on this face — [`pr_graph::badge_chip`], plus the
    /// desktop's Issue face for blockers alone (the overlay's "Blocked by"),
    /// which names the subject issue with nothing behind it.
    fn chip(&self) -> Option<BadgeChip> {
        if let Some(chip) = pr_graph::badge_chip(&self.graph, self.face) {
            return Some(chip);
        }
        (self.face == BadgeFace::Issue && !self.blocked_by.is_empty()).then(|| BadgeChip {
            issue: self
                .graph
                .entry
                .as_ref()
                .map(|entry| entry.representative().clone()),
            count: 0,
        })
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
    blockers.sort_by(|a, b| sync::cmp_identifiers(&a.identifier, &b.identifier));
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
    BadgeSpec {
        graph: pr_graph::pr_graph(Some(issue), session, &issues, &sessions),
        face,
        blocked_by: blocked_by(&issue.id, cx),
        blocks_graph: match face {
            BadgeFace::Issue => crate::issue_graph::graph_for(&[issue.id.as_str()], cx),
            _ => domain::issue_graph::IssueGraph::default(),
        },
        run_title: None,
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
        blocked_by: Vec::new(),
        blocks_graph: domain::issue_graph::IssueGraph::default(),
        run_title: Some(crate::run_rows::run_title(
            session,
            None,
            &crate::run_rows::batch_run_issues(session, cx),
        )),
    }
}

// ---------------------------------------------------------------------------
// The badge
// ---------------------------------------------------------------------------

/// The front chip's title cap — a header cluster, not a row.
const CHIP_TITLE_MAX_W: f32 = 160.;
/// The deck's ghosts step up and to the right by two steps; `+N` clears them.
const CHIP_DECK_OFFSET: f32 = 2. * ISSUE_CHIP_STACK_STEP;
/// The pointer rests this long on the chip before the overlay opens, so a
/// sweep across the header opens nothing (the hover-preview's habit).
const HOVER_OPEN_DELAY: Duration = Duration::from_millis(200);
/// The grace the pointer gets to cross from the chip into the overlay (and
/// back) before it closes.
const HOVER_CLOSE_DELAY: Duration = Duration::from_millis(150);

/// What the header chip SHOWS — plain data, so the styleguide draws the very
/// same element without a synced row.
pub(crate) struct ChipFace {
    /// `EXP-12` of the front issue; empty for a run's chip.
    pub identifier: SharedString,
    /// The front issue's title, or the run's own title.
    pub title: SharedString,
    /// The front issue's resolved status; `None` = the plain issues glyph.
    pub status: Option<ResolvedStatus>,
    /// A run family without an issue: the `session-tree` concept leads.
    pub runs: bool,
    /// How many ride behind the front chip (`+N`, a deck when > 0).
    pub count: usize,
}

/// The stacked chip itself: the front [`issue_chip`] (INERT — the caller
/// owns what a click means), ghosts behind it when anything rides along, and
/// a muted `+N` after the deck.
pub(crate) fn chip_face(id: &str, face: ChipFace, muted: Hsla) -> gpui::Div {
    let mut front = issue_chip(
        SharedString::from(format!("{id}-front")),
        face.identifier,
        face.title,
    )
    .max_title_width(px(CHIP_TITLE_MAX_W));
    if let Some(status) = face.status {
        front = front.status(status);
    }
    if face.runs {
        front = front.slot(
            Icon::new(registry::SESSION_TREE)
                .with_size(px(ISSUE_CHIP_ICON_MAX))
                .text_color(muted),
        );
    }
    if face.count > 0 {
        front = front.stacked();
    }
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(CHIP_DECK_OFFSET + 4.))
        .child(front)
        .when(face.count > 0, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format!("+{}", face.count))),
            )
        })
}

/// The header chip: the stacked issue chip for this face, opening the
/// face's overlay on hover or click. `None` when there is nothing to show.
pub(crate) fn badge(id: &'static str, spec: BadgeSpec, cx: &App) -> Option<AnyElement> {
    let chip = spec.chip()?;
    let face = ChipFace {
        status: chip
            .issue
            .as_ref()
            .map(|issue| crate::queries::resolve_issue_status(cx, issue)),
        identifier: chip
            .issue
            .as_ref()
            .map(|issue| SharedString::from(issue.identifier.clone()))
            .unwrap_or_default(),
        title: match chip.issue.as_ref() {
            Some(issue) => SharedString::from(issue.title.clone()),
            None => spec.run_title.clone().unwrap_or_else(|| "Run".into()),
        },
        runs: chip.issue.is_none(),
        count: chip.count,
    };
    let element = chip_face(id, face, cx.theme().muted_foreground).into_any_element();
    Some(
        HeaderChip {
            id,
            element: Some(element),
            spec,
        }
        .into_any_element(),
    )
}

/// The overlay's hover bookkeeping, one per chip (keyed window state): open
/// on a rest over the chip, stay open while the pointer is on the chip OR
/// the overlay, close a grace after it leaves both.
#[derive(Default)]
struct HoverState {
    open: bool,
    over_chip: bool,
    over_card: bool,
    timer: Option<gpui::Task<()>>,
}

impl HoverState {
    fn hover(&mut self, chip: bool, hovered: bool, cx: &mut gpui::Context<Self>) {
        if chip {
            self.over_chip = hovered;
        } else {
            self.over_card = hovered;
        }
        let delay = if hovered {
            if self.open {
                self.timer = None;
                return;
            }
            HOVER_OPEN_DELAY
        } else {
            HOVER_CLOSE_DELAY
        };
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                let inside = this.over_chip || this.over_card;
                if this.open != inside {
                    this.open = inside;
                    cx.notify();
                }
            });
        }));
    }
}

#[derive(IntoElement)]
struct HeaderChip {
    id: &'static str,
    element: Option<AnyElement>,
    spec: BadgeSpec,
}

impl RenderOnce for HeaderChip {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let state = window.use_keyed_state(
            SharedString::from(format!("{id}-hover")),
            cx,
            |_, _| HoverState::default(),
        );
        let open = state.read(cx).open;
        let chip_state = state.clone();
        let trigger = ChipTrigger {
            base: div()
                .id(SharedString::from(format!("{id}-chip")))
                .flex()
                .items_center()
                // The header rung (EXP-926): the chip is shorter than the
                // toggle beside it, so it centres in the toggle's height.
                .h(px(crate::work_header::header_action_size(false).height()))
                .pr(px(CHIP_DECK_OFFSET))
                .cursor_pointer()
                .on_hover(move |hovered, _window, cx| {
                    chip_state.update(cx, |state, cx| state.hover(true, *hovered, cx));
                }),
            element: self.element.take(),
            selected: false,
        };
        let change_state = state.clone();
        let spec = self.spec;
        gpui_component::popover::Popover::new(SharedString::from(format!("{id}-popover")))
            .p_2()
            .open(open)
            .on_open_change(move |open, _window, cx| {
                change_state.update(cx, |state, cx| {
                    // A click on a chip the hover already opened keeps it up.
                    if !*open && state.over_chip {
                        return;
                    }
                    state.open = *open;
                    state.timer = None;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .content(move |_, window, cx| {
                let card_state = state.clone();
                div()
                    .id("pr-graph-overlay")
                    .on_hover(move |hovered, _window, cx| {
                        card_state.update(cx, |state, cx| state.hover(false, *hovered, cx));
                    })
                    .child(overlay(&spec, window, cx))
            })
    }
}

/// The chip wrapped so `Popover::trigger` takes it (a `Selectable`): the
/// wrapper paints nothing of its own (the picker's `PickerTrigger` recipe).
#[derive(IntoElement)]
struct ChipTrigger {
    base: gpui::Stateful<gpui::Div>,
    element: Option<AnyElement>,
    selected: bool,
}

impl gpui_component::Selectable for ChipTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for ChipTrigger {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let element = self.element.take();
        self.base.children(element)
    }
}

// ---------------------------------------------------------------------------
// The overlay
// ---------------------------------------------------------------------------

/// Roughly the width of the header's right cluster — wide enough for an
/// identifier, a title and a state, narrow enough to stay a popover.
const OVERLAY_W: f32 = 320.;

fn overlay(spec: &BadgeSpec, _window: &mut Window, cx: &App) -> AnyElement {
    let mut column = v_flex().w(px(OVERLAY_W)).min_w_0().gap_2();
    match spec.face {
        BadgeFace::Issue => {
            // EXP-980: the transitive blocks GRAPH, not a flat chip list —
            // "blocked by EXP-11" never said what blocks EXP-11.
            if !spec.blocked_by.is_empty() {
                column = column.child(section(
                    "Blocked by",
                    vec![crate::issue_graph::graph_overlay(
                        &spec.blocks_graph,
                        OVERLAY_W,
                        cx,
                    )],
                    cx,
                ));
            }
            if let Some(batch) = spec.graph.batch.as_ref() {
                let others: Vec<Issue> = batch.issues.clone();
                column = column.child(section("In batch with", issue_rows(&others, cx), cx));
            }
            if !spec.graph.stack.is_empty() {
                column = column.child(section("In this stack", stack_rows(spec, cx), cx));
            }
        }
        BadgeFace::Run => {
            if !spec.graph.tree.is_empty() {
                column = column.child(section("Runs", run_rows(spec, cx), cx));
            }
            if let Some(batch) = spec.graph.batch.as_ref() {
                column = column.child(section("In batch with", issue_rows(&batch.issues, cx), cx));
            }
        }
        BadgeFace::Changes => {
            if !spec.graph.stack.is_empty() {
                column = column.child(section("Pull request stack", stack_rows(spec, cx), cx));
            }
            if let Some(batch) = spec.graph.batch.as_ref() {
                column = column.child(section("In batch with", issue_rows(&batch.issues, cx), cx));
            }
        }
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

/// Issue rows — `#IDENT title`, a click opens the issue.
fn issue_rows(issues: &[Issue], cx: &App) -> Vec<AnyElement> {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    issues
        .iter()
        .map(|issue| {
            let issue_id = issue.id.clone();
            row_shell(
                SharedString::from(format!("pr-graph-issue-{issue_id}")),
                &domain::tree_guides::Guides::default(),
                cx,
            )
                .child(mono(issue.identifier.clone(), muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .truncate()
                        .text_color(foreground)
                        .child(SharedString::from(issue.title.clone())),
                )
                .on_click(move |_: &ClickEvent, window, cx| {
                    crate::navigation::navigate(
                        window,
                        cx,
                        crate::navigation::Screen::IssueDetail {
                            issue_id: issue_id.clone(),
                        },
                    );
                })
                .into_any_element()
        })
        .collect()
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
                crate::session_screen::open_session_with_origin(&session_id, None, window, cx);
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
    let size = spec.graph.stack.len();
    // EXP-965: the connector needs the WHOLE visible sequence up front — a
    // batch member folds its issues out one level deeper, so the rows are
    // interleaved and the depths cannot be read off the stack alone.
    let mut depths: Vec<usize> = Vec::with_capacity(size * 2);
    for member in spec.graph.stack.iter() {
        depths.push(member.depth);
        if member.entry.is_batch() {
            depths.extend(std::iter::repeat_n(member.depth + 1, member.entry.issues.len()));
        }
    }
    let guides = domain::tree_guides::guides_for(&depths);
    let guide_at = |position: usize| guides.get(position).cloned().unwrap_or_default();
    let mut rows: Vec<AnyElement> = Vec::with_capacity(size * 2);
    for (index, member) in spec.graph.stack.iter().enumerate() {
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
                crate::navigation::navigate(
                    window,
                    cx,
                    crate::navigation::Screen::PrDiff {
                        issue_id: nav_id.clone(),
                    },
                );
            })
            .into_any_element(),
        );
        // A batch member folds its issues out underneath.
        if entry.is_batch() {
            for issue in &entry.issues {
                rows.push(
                    row_shell(
                        SharedString::from(format!("pr-graph-batch-{}", issue.id)),
                        &guide_at(rows.len()),
                        cx,
                    )
                    .child(mono(issue.identifier.clone(), muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(foreground)
                            .child(SharedString::from(issue.title.clone())),
                    )
                    .into_any_element(),
                );
            }
        }
    }
    rows
}

/// "Merge stack" — the whole chain, bottom-up, behind the shared confirm
/// dialog. The call rides the BOTTOM member's issue id; the server resolves
/// the top itself.
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
        );
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
                .children(issue_rows(&issues, cx))
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
            blocked_by: Vec::new(),
            blocks_graph: domain::issue_graph::IssueGraph::default(),
            run_title: None,
        }
    }

    /// The Changes face only exists to show a PR relationship: with none, the
    /// badge stays away. The Issue face still opens for blockers alone.
    #[test]
    fn the_badge_hides_when_a_face_has_nothing_to_say() {
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        assert!(!spec(BadgeFace::Changes, &lone).chip().is_some());
        assert!(!spec(BadgeFace::Issue, &lone).chip().is_some());
        let mut blocked = spec(BadgeFace::Issue, &lone);
        blocked.blocked_by = vec![issue("EXP-29", None, None)];
        assert!(blocked.chip().is_some());
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        assert!(spec(BadgeFace::Changes, &stacked).chip().is_some());
    }

    /// EXP-1058 — the chip names the subject PR's representative issue and
    /// counts the rest; blockers alone still earn the Issue face a chip.
    #[test]
    fn the_chip_names_the_front_issue_and_the_rest() {
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        let chip = spec(BadgeFace::Changes, &stacked).chip().unwrap();
        assert_eq!(chip.issue.unwrap().identifier, "EXP-12");
        assert_eq!(chip.count, 1);
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        let mut blocked = spec(BadgeFace::Issue, &lone);
        blocked.blocked_by = vec![issue("EXP-29", None, None)];
        let chip = blocked.chip().unwrap();
        assert_eq!(chip.issue.unwrap().identifier, "EXP-30");
        assert_eq!(chip.count, 0);
    }
}
