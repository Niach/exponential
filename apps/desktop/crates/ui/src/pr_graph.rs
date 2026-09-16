//! EXP-897 §4 — the ONE stack/batch badge and its overlay.
//!
//! Every face of a top tab (Issue · Run · Changes) shares one work header, so
//! it shares ONE badge: a small glass pill carrying the stack glyph, the batch
//! glyph or both, plus `2 of 3` when the pull request is stacked. Clicking it
//! opens a popover whose SECTION depends on the face that is up, built from
//! the same row primitives and the same copy so the three read as one thing:
//!
//! * **Issue** — "Blocked by" (the canonical `blocks` relations pointing AT
//!   this issue) and "In batch with" (the other issues on its pull request);
//! * **Run** — the run's session tree, nested, live dots, a click opens a run;
//! * **Changes / review** — the PR stack BOTTOM-UP: identifier(s), PR state, a
//!   batch glyph with the batch's issues folded underneath, and "Merge stack"
//!   on the bottom entry.
//!
//! The model is [`domain::pr_graph`] — this module is presentation only.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, ClickEvent, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::pr_graph::{self, BadgeKind, PrGraph};
use domain::pr_stack;
use domain::rows::{CodingSession, Issue};

use crate::icons::{registry, ExpIcon};
use crate::surface::{glass_pill, glass_pill_button, PillMode, PillSize};

/// Which face the badge is rendered on — it decides the overlay's sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadgeFace {
    Issue,
    Run,
    Changes,
}

/// Everything one badge draws: the graph, the face, and the blocked-by
/// relations (the only part of the Issue face that is not in the graph).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BadgeSpec {
    pub graph: PrGraph,
    pub face: BadgeFace,
    pub blocked_by: Vec<Issue>,
}

impl BadgeSpec {
    /// Whether the badge has anything at all to say on its face. A stack or a
    /// batch always shows; the Issue face also shows for blockers alone, and
    /// the Run face for a run that has a family.
    fn is_visible(&self) -> bool {
        if pr_graph::badge_kind(&self.graph).is_some() {
            return true;
        }
        match self.face {
            BadgeFace::Issue => !self.blocked_by.is_empty(),
            BadgeFace::Run => !self.graph.tree.is_empty(),
            BadgeFace::Changes => false,
        }
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
    let mut blockers: Vec<Issue> = collections
        .relations_for_issue(issue_id, cx)
        .into_iter()
        .filter(|row| {
            row.kind.as_deref() == Some(domain::contract::ISSUE_RELATION_TYPE_BLOCKS)
                && row.related_issue_id == issue_id
        })
        .filter_map(|row| issues.get(&row.issue_id).cloned())
        .filter(|blocker| !status_is_closed(blocker))
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
    }
}

// ---------------------------------------------------------------------------
// The badge
// ---------------------------------------------------------------------------

/// The header pill: the glyph(s) and, when the pull request is stacked, its
/// position. `None` when there is nothing to show on this face.
pub(crate) fn badge(id: &'static str, spec: BadgeSpec, cx: &App) -> Option<AnyElement> {
    if !spec.is_visible() {
        return None;
    }
    let kind = pr_graph::badge_kind(&spec.graph);
    let muted = cx.theme().muted_foreground;
    let glyphs: Vec<ExpIcon> = badge_glyphs(kind, &spec)
        .into_iter()
        .map(glyph_icon)
        .collect();
    let label = spec
        .graph
        .position()
        .map(|position| format!("{} of {}", position.position, position.size))
        .or_else(|| {
            spec.graph
                .batch
                .as_ref()
                .map(|batch| format!("{} issues", batch.issues.len()))
        });
    let tooltip = badge_tooltip(&spec, kind);
    let mut pill = glass_pill_button(id, PillSize::Sm, cx)
        .icon(
            Icon::new(glyphs[0].clone())
                .with_size(px(PillSize::Sm.glyph()))
                .text_color(muted),
        )
        .tooltip(tooltip);
    // A batch INSIDE a stack wears both concepts (EXP-897 §4).
    for glyph in glyphs.iter().skip(1) {
        pill = pill.child(
            Icon::new(glyph.clone())
                .with_size(px(PillSize::Sm.glyph()))
                .text_color(muted)
                .into_any_element(),
        );
    }
    if let Some(label) = label {
        pill = pill.label(SharedString::from(label));
    }
    let content_spec = spec;
    Some(
        gpui_component::popover::Popover::new(SharedString::from(format!("{id}-popover")))
            .p_2()
            .trigger(pill)
            .content(move |_, window, cx| overlay(&content_spec, window, cx))
            .into_any_element(),
    )
}

/// The glyph(s) the pill wears — never a raw lucide import, always the
/// `pr-stack` / `pr-batch` CONCEPTS (EXP-273).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadgeGlyph {
    Stack,
    Batch,
    Runs,
}

fn badge_glyphs(kind: Option<BadgeKind>, spec: &BadgeSpec) -> Vec<BadgeGlyph> {
    match kind {
        Some(BadgeKind::Stack) => vec![BadgeGlyph::Stack],
        Some(BadgeKind::Batch) => vec![BadgeGlyph::Batch],
        Some(BadgeKind::StackAndBatch) => vec![BadgeGlyph::Stack, BadgeGlyph::Batch],
        // No PR relationship, but the face still has something to say.
        None => match spec.face {
            BadgeFace::Run => vec![BadgeGlyph::Runs],
            _ => vec![BadgeGlyph::Stack],
        },
    }
}

/// The CONCEPT behind each glyph — never a raw lucide import (EXP-273).
fn glyph_icon(glyph: BadgeGlyph) -> ExpIcon {
    match glyph {
        BadgeGlyph::Stack => registry::PR_STACK,
        BadgeGlyph::Batch => registry::PR_BATCH,
        BadgeGlyph::Runs => registry::UI_AGENT_SOURCE,
    }
}

fn badge_tooltip(spec: &BadgeSpec, kind: Option<BadgeKind>) -> SharedString {
    SharedString::from(match (kind, spec.face) {
        (Some(BadgeKind::Batch), _) => "This pull request closes several issues".to_string(),
        (Some(_), _) => "This pull request is part of a stack".to_string(),
        (None, BadgeFace::Run) => "The runs around this one".to_string(),
        (None, _) => "What this issue waits on".to_string(),
    })
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
            if !spec.blocked_by.is_empty() {
                column = column.child(section("Blocked by", issue_rows(&spec.blocked_by, cx), cx));
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
        .gap_0p5()
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

/// The shared row shell every section draws — the glass list row, hovered.
fn row_shell(id: SharedString, depth: usize, cx: &App) -> gpui::Stateful<gpui::Div> {
    let hover = cx.theme().list_hover;
    crate::surface::flat_row()
        .id(id)
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .pl(px(8. + 14. * depth as f32))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
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
            row_shell(SharedString::from(format!("pr-graph-issue-{issue_id}")), 0, cx)
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
    spec.graph
        .tree
        .iter()
        .map(|row| {
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
                row.depth,
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
            member.depth,
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
                        member.depth + 1,
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
        }
    }

    /// The Changes face only exists to show a PR relationship: with none, the
    /// badge stays away. The Issue face still opens for blockers alone.
    #[test]
    fn the_badge_hides_when_a_face_has_nothing_to_say() {
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        assert!(!spec(BadgeFace::Changes, &lone).is_visible());
        assert!(!spec(BadgeFace::Issue, &lone).is_visible());
        let mut blocked = spec(BadgeFace::Issue, &lone);
        blocked.blocked_by = vec![issue("EXP-29", None, None)];
        assert!(blocked.is_visible());
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        assert!(spec(BadgeFace::Changes, &stacked).is_visible());
    }

    /// The pill wears the CONCEPT glyphs, both of them for a batch inside a
    /// stack — never a raw lucide import (EXP-273).
    #[test]
    fn the_badge_glyphs_follow_the_badge_kind() {
        assert_eq!(
            badge_glyphs(Some(BadgeKind::Stack), &spec(BadgeFace::Changes, &[issue("A", None, None)])),
            vec![BadgeGlyph::Stack]
        );
        assert_eq!(
            badge_glyphs(Some(BadgeKind::Batch), &spec(BadgeFace::Changes, &[issue("A", None, None)])),
            vec![BadgeGlyph::Batch]
        );
        assert_eq!(
            badge_glyphs(
                Some(BadgeKind::StackAndBatch),
                &spec(BadgeFace::Changes, &[issue("A", None, None)])
            ),
            vec![BadgeGlyph::Stack, BadgeGlyph::Batch]
        );
    }
}
