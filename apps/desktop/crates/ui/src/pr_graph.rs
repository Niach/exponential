//! EXP-897 §4: the ONE stack/batch badge and its "Related work" dialog.
//!
//! Every face of a top tab (Issue · Run · Changes) shares one work header, so
//! it shares ONE badge. SLOP-16 round 2: the badge is a muted ICON BUTTON
//! (the header `…` button's box) whose glyph names the SHAPE
//! ([`domain::pr_graph::badge_shape`], face-independent): `pr-stack` for a
//! stack (with or without a batch), `pr-batch`, `relation-blocked-by` for
//! open blockers alone; a mono `+N` beside it counts the others
//! ([`domain::pr_graph::badge_chip`], ×4) and the shape's name rides the
//! tooltip. A run family alone earns no badge (round 5).
//!
//! SLOP-16 round 5 ("C, in a dialog"): a click opens "Related work", the
//! standard native dialog whose body is EXACTLY the relations card's bands
//! ([`crate::issue_relations::fold_band`]: foldable, counted, capped at three
//! with "Show N more") in ONE order ×4
//! ([`domain::pr_graph::overlay_sections`]): "Blocked by" (the direct open
//! blockers), "Same pull request" (the batch partners), "Pull request stack"
//! (the OTHER pull requests, bottom-up). Rows = the relations card's issue
//! row, and for a pull request the same row shape (PR glyph · `#n` · title ·
//! state chip). Nothing else. Copy: [`domain::pr_graph::overlay_copy`].
//!
//! Every click inside the dialog closes it and acts in the opener.
//!
//! SLOP-3 kept this badge when the stack SYSTEM went: the three bands are
//! read from synced data alone (`blocks` relations, a shared `pr_url`, and
//! `pr_base_branch == lower.branch`); there is no merge-stack control.
//!
//! The model is [`domain::pr_graph`]: this module is presentation only.

use gpui::{
    div, prelude::FluentBuilder as _, px, size, AnyElement, App, AppContext as _, ClickEvent,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::pr_graph::{self, overlay_copy, BadgeChip, BadgeShape, OverlaySection, PrEntry, PrGraph};
use domain::rows::{CodingSession, Issue};

use crate::icons::{registry, ExpIcon};
use crate::issue_relations::{band_window, fold_band, FoldBand, IssueRowOpts};

/// Everything one badge draws: the graph, its `blocked_by` included
/// (EXP-1097).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BadgeSpec {
    pub graph: PrGraph,
}

impl BadgeSpec {
    /// The badge's count source: [`pr_graph::badge_chip`], the SAME on
    /// every face (EXP-1097).
    pub(crate) fn chip(&self) -> Option<BadgeChip> {
        pr_graph::badge_chip(&self.graph)
    }
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Every issue of the team `issue` belongs to: the scope the branch-matching
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

/// EXP-736: the issues BLOCKING `issue_id`: canonical `blocks` rows whose
/// `related_issue_id` is this issue (the inverse side, "blocked by"). Rows
/// whose blocker has not synced are dropped; a blocker that is done,
/// cancelled or a duplicate no longer blocks anything.
pub(crate) fn blocked_by(issue_id: &str, cx: &App) -> Vec<Issue> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let relations = collections.relations_for_issue(issue_id, cx);
    let mut blockers: Vec<Issue> = blockers_of(issue_id, &relations, |id| issues.get(id))
            .into_iter()
            .filter(|blocker| !status_is_closed(blocker))
            .cloned()
            .collect();
    // EXP-1097: plain identifier order, byte-for-byte web `openBlockers`
    // (`lib/stack-start.ts`) and the natives': the `blocked` chip's front
    // issue is the FIRST blocker, so a natural sort here led with a
    // different issue than the other three clients.
    blockers.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    blockers.dedup_by(|a, b| a.id == b.id);
    blockers
}

/// EXP-897: the blockers of an issue, read off the synced `issue_relations`
/// rows. Only the INVERSE side of a canonical `blocks` row counts
/// (`related_issue_id == me`); a row whose other issue has not synced is
/// skipped, and an issue named by two rows appears once. Status is NOT
/// filtered here.
fn blockers_of<'a>(
    issue_id: &str,
    relations: &[domain::rows::IssueRelation],
    issue_of: impl Fn(&str) -> Option<&'a Issue>,
) -> Vec<&'a Issue> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for relation in relations {
        if relation.kind.as_deref() != Some("blocks") || relation.related_issue_id != issue_id {
            continue;
        }
        if !seen.insert(relation.issue_id.clone()) {
            continue;
        }
        if let Some(issue) = issue_of(&relation.issue_id) {
            out.push(issue);
        }
    }
    out
}

/// A blocker that is done, cancelled or a duplicate blocks nothing any more
/// (the ×4 `openBlockers` rule).
fn status_is_closed(issue: &Issue) -> bool {
    matches!(
        issue.status,
        domain::IssueStatus::Done | domain::IssueStatus::Cancelled | domain::IssueStatus::Duplicate
    )
}

/// The badge spec for an issue-bound face (a run on its issue included:
/// the run's issue IS the subject).
pub(crate) fn issue_spec(issue: &Issue, cx: &App) -> BadgeSpec {
    let issues = team_issues(issue, cx);
    let mut graph = pr_graph::pr_graph(Some(issue), None, &issues, &[]);
    // EXP-1097: the open blockers ride the GRAPH (web `blockedBy`): they
    // earn the badge on every face, not just the Issue one.
    graph.blocked_by = blocked_by(&issue.id, cx);
    BadgeSpec { graph }
}

/// The badge spec for an issue-LESS run (a chat / action / batch run): its
/// own pull request (or a batch's covered set).
pub(crate) fn session_spec(session: &CodingSession, cx: &App) -> BadgeSpec {
    let issues: Vec<Issue> = sync::Store::try_global(cx)
        .map(|store| store.collections().issues.read(cx).iter().cloned().collect())
        .unwrap_or_default();
    let mut graph = pr_graph::pr_graph(None, Some(session), &issues, std::slice::from_ref(session));
    // A run on an issue that has synced reads that issue's blockers.
    if let Some(issue_id) = graph.subject_issue_id.clone() {
        graph.blocked_by = blocked_by(&issue_id, cx);
    }
    BadgeSpec { graph }
}

// ---------------------------------------------------------------------------
// The badge
// ---------------------------------------------------------------------------

/// The badge's NAME per shape (web `PrGraphBadge`'s `name`): the icon
/// button's tooltip and the graph dialog's title.
pub(crate) fn badge_name(shape: BadgeShape) -> &'static str {
    match shape {
        BadgeShape::Stack => "Pull request stack",
        BadgeShape::Batch => "Batch pull request",
        BadgeShape::StackAndBatch => "Stack and batch",
        BadgeShape::Blocked => "Blocked by",
    }
}

/// The badge's glyph per shape: a concept, never a raw glyph.
pub(crate) fn badge_icon(shape: BadgeShape) -> ExpIcon {
    match shape {
        BadgeShape::Stack | BadgeShape::StackAndBatch => registry::PR_STACK,
        BadgeShape::Batch => registry::PR_BATCH,
        BadgeShape::Blocked => registry::RELATION_BLOCKED_BY,
    }
}

/// What the icon button shows: its shape and the `+N` beside the glyph
/// ([`pr_graph::badge_chip`]'s count; 0 = no count).
pub(crate) fn badge_face(spec: &BadgeSpec) -> Option<(BadgeShape, usize)> {
    let shape = pr_graph::badge_shape(&spec.graph)?;
    Some((shape, spec.chip().map_or(0, |chip| chip.count)))
}

/// SLOP-16 round 2: the header badge: a muted ICON BUTTON (the header `…`
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
/// An issue row's height (the Blocked by and Same PR bands).
const ROW_H: f32 = 30.;
const SECTION_GAP: f32 = 12.;

/// One band's row height: the Stack band draws THE pull-request row
/// ([`crate::pr_rows::pr_list`], [`domain::list_item::PR_ROW`]), the others
/// the relations card's issue rows.
fn band_row_h(section: OverlaySection) -> f32 {
    match section {
        OverlaySection::Stack => domain::list_item::PR_ROW,
        OverlaySection::Blocked | OverlaySection::Batch => ROW_H,
    }
}

/// SLOP-16 round 3: open "Related work": the platform's standard modal,
/// 560 wide, as tall as its content (capped at 85% of the opener).
fn open_graph_dialog(spec: BadgeSpec, window: &mut Window, cx: &mut App) {
    let viewport = window.viewport_size();
    let height = px(content_height(&spec.graph).max(DIALOG_MIN_H)).min(viewport.height * 0.85);
    let dialog = crate::native_dialog::DialogSpec::new(
        overlay_copy::RELATED_WORK_TITLE,
        size(px(DIALOG_W).min(viewport.width * 0.9), height),
    )
    .resizable(size(px(420.), px(DIALOG_MIN_H)));
    crate::native_dialog::open_dialog_window(window, cx, dialog, move |_window, cx| {
        crate::native_dialog::DialogContent::new(cx.new(|_| GraphDialog {
            graph: spec.graph,
            folded: Vec::new(),
            show_all: Vec::new(),
        }))
    });
}

/// How many rows a section holds.
fn section_len(graph: &PrGraph, section: OverlaySection) -> usize {
    match section {
        OverlaySection::Blocked => graph.blocked_by.len(),
        OverlaySection::Batch => pr_graph::batch_partners(graph).len(),
        OverlaySection::Stack => pr_graph::stack_others(graph).len(),
    }
}

/// The fitted content height: the padding, then per band its header, its
/// capped rows and the "Show N more" row (every band opens unfolded).
fn content_height(graph: &PrGraph) -> f32 {
    let sections = pr_graph::overlay_sections(graph);
    if sections.is_empty() {
        return 2. * DIALOG_PAD + ROW_H;
    }
    let mut height = 2. * DIALOG_PAD + SECTION_GAP * (sections.len() - 1) as f32;
    for section in sections {
        let (shown, footer) = band_window(section_len(graph, section), true, false);
        // The "Show N more" footer is an issue-height row in every band.
        height += BAND_H + shown as f32 * band_row_h(section) + footer.map_or(0., |_| ROW_H);
    }
    height
}

/// The dialog body: the relations card's bands, with their fold state.
struct GraphDialog {
    graph: PrGraph,
    /// Bands the user folded (every band opens unfolded).
    folded: Vec<OverlaySection>,
    /// Bands whose "Show N more" was pressed.
    show_all: Vec<OverlaySection>,
}

/// Flip one section in one list (present → gone, absent → pushed).
fn flip(list: &mut Vec<OverlaySection>, section: OverlaySection) {
    match list.iter().position(|kept| *kept == section) {
        Some(index) => {
            list.remove(index);
        }
        None => list.push(section),
    }
}

impl Render for GraphDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let sections = pr_graph::overlay_sections(&self.graph);
        if sections.is_empty() {
            return div()
                .px_3()
                .py_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(overlay_copy::EMPTY)
                .into_any_element();
        }
        let mut column = v_flex().w_full().min_w_0().gap_3();
        for section in sections {
            let expanded = !self.folded.contains(&section);
            let everything = self.show_all.contains(&section);
            let count = section_len(&self.graph, section);
            let (shown, footer) = band_window(count, expanded, everything);
            let rows = section_rows(&self.graph, section, shown, cx);
            let slug = match section {
                OverlaySection::Blocked => "blocked-by",
                OverlaySection::Batch => "same-pr",
                OverlaySection::Stack => "stack",
            };
            column = column.child(fold_band(
                FoldBand {
                    id: SharedString::from(format!("related-work-{slug}")),
                    icon: section_icon(section),
                    title: SharedString::from(overlay_copy::band_label(section)),
                    count,
                    expanded,
                    rows,
                    footer,
                    on_toggle: Box::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        flip(&mut this.folded, section);
                        // Folding a band forgets its "Show N more".
                        this.show_all.retain(|kept| *kept != section);
                        cx.notify();
                    })),
                    on_footer: Box::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        flip(&mut this.show_all, section);
                        cx.notify();
                    })),
                },
                cx,
            ));
        }
        column.into_any_element()
    }
}

/// The glyph each band wears after its chevron.
fn section_icon(section: OverlaySection) -> ExpIcon {
    match section {
        OverlaySection::Blocked => {
            crate::issue_relations::band_icon(domain::relations_view::RelationBandKey::BlockedBy)
        }
        OverlaySection::Batch => registry::PR_BATCH,
        OverlaySection::Stack => registry::PR_STACK,
    }
}

/// The first `shown` rows of one band.
fn section_rows(graph: &PrGraph, section: OverlaySection, shown: usize, cx: &mut App) -> Vec<AnyElement> {
    let issue_rows = |issues: Vec<&Issue>, prefix: &str, cx: &mut App| -> Vec<AnyElement> {
        issues
            .into_iter()
            .take(shown)
            .map(|issue| {
                crate::issue_relations::issue_row(
                    &format!("{prefix}-{}", issue.id),
                    issue,
                    IssueRowOpts {
                        title: None,
                        open: !status_is_closed(issue),
                        remove: None,
                        guides: None,
                        in_dialog: true,
                    },
                    cx,
                )
            })
            .collect()
    };
    match section {
        OverlaySection::Blocked => {
            issue_rows(graph.blocked_by.iter().collect(), "related-work-blocker", cx)
        }
        OverlaySection::Batch => {
            issue_rows(pr_graph::batch_partners(graph), "related-work-partner", cx)
        }
        OverlaySection::Stack => {
            let rows = pr_graph::stack_others(graph)
                .into_iter()
                .take(shown)
                .map(|entry| stack_list_row(entry, cx))
                .collect();
            vec![crate::pr_rows::pr_list("related-work-pr", rows, cx)]
        }
    }
}

/// The mono label a stack row leads with: `#n`, the identifier while the
/// pull request has no number.
fn stack_row_label(entry: &PrEntry) -> String {
    let representative = entry.representative();
    match representative.pr_number {
        Some(number) => format!("#{number}"),
        None => representative.identifier.clone(),
    }
}

/// One OTHER pull request of the stack as THE pull-request row: ring lead
/// ([`stack_row_node`]) · mono `#n` · the representative issue's title · the
/// PR state chip. A click closes the dialog and opens that pull request's
/// review.
fn stack_list_row(entry: &PrEntry, cx: &mut App) -> crate::pr_rows::PrListRow {
    let representative = entry.representative();
    let state = representative
        .pr_state
        .clone()
        .unwrap_or_else(|| domain::contract::PR_STATE_OPEN.to_string());
    let node = stack_row_node(&state);
    let issue_id = representative.id.clone();
    let open_id = issue_id.clone();
    crate::pr_rows::PrListRow {
        key: issue_id,
        identifier: Some(SharedString::from(stack_row_label(entry))),
        title: SharedString::from(representative.title.clone()),
        depth: 0,
        node,
        word: None,
        active: false,
        on_open: Some(Box::new(move |_: &ClickEvent, window, cx| {
            // EXP-1251: the PR's review = its issue's Guide.
            let issue_id = open_id.clone();
            crate::native_dialog::close_then(window, cx, move |window, cx| {
                crate::screens::open_issue_changes(&issue_id, None, window, cx);
            });
        })),
        trailing: Some(crate::issue_detail::pr_state_chip(&state, cx)),
    }
}

/// Wave D (web `PrNode`, ×4): the ring a stack row's PR state draws. The
/// web node knows only open / current / base, and a DRAFT is an open pull
/// request, so it keeps the emerald `open` ring (the `Draft` chip says the
/// rest); only a merged or closed PR goes to the muted base ring.
pub(crate) fn stack_row_node(state: &str) -> domain::list_item::PrNodeState {
    if state == domain::contract::PR_STATE_MERGED || state == domain::contract::PR_STATE_CLOSED {
        domain::list_item::PrNodeState::Base
    } else {
        domain::list_item::PrNodeState::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_stack_row_keeps_the_open_ring() {
        use domain::list_item::PrNodeState;
        assert_eq!(stack_row_node("open"), PrNodeState::Open);
        assert_eq!(stack_row_node("draft"), PrNodeState::Open);
        assert_eq!(stack_row_node("merged"), PrNodeState::Base);
        assert_eq!(stack_row_node("closed"), PrNodeState::Base);
    }

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

    fn spec(issues: &[Issue]) -> BadgeSpec {
        BadgeSpec {
            graph: pr_graph::pr_graph(Some(&issues[0]), None, issues, &[]),
        }
    }

    /// SLOP-16: every shape names itself (the tooltip, byte-identical with
    /// web) and wears its own concept glyph.
    #[test]
    fn every_shape_has_a_name_and_a_glyph() {
        assert_eq!(badge_name(BadgeShape::Blocked), "Blocked by");
        assert_eq!(badge_name(BadgeShape::Stack), "Pull request stack");
        assert_eq!(badge_name(BadgeShape::Batch), "Batch pull request");
        assert_eq!(badge_name(BadgeShape::StackAndBatch), "Stack and batch");
        use gpui_component::IconNamed as _;
        let path = |icon: ExpIcon| icon.path().to_string();
        assert_eq!(path(badge_icon(BadgeShape::Stack)), path(registry::PR_STACK));
        assert_eq!(path(badge_icon(BadgeShape::StackAndBatch)), path(registry::PR_STACK));
        assert_eq!(path(badge_icon(BadgeShape::Batch)), path(registry::PR_BATCH));
        assert_eq!(
            path(badge_icon(BadgeShape::Blocked)),
            path(registry::RELATION_BLOCKED_BY)
        );
    }

    /// EXP-1097: with nothing around the issue the badge stays away; open
    /// blockers alone earn it.
    #[test]
    fn the_badge_needs_a_relation() {
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        assert!(badge_face(&spec(&lone)).is_none());
        let mut blocked = spec(&lone);
        blocked.graph.blocked_by = vec![issue("EXP-29", None, None)];
        assert_eq!(badge_face(&blocked), Some((BadgeShape::Blocked, 0)));
    }

    /// EXP-1058: `+N` counts every other issue on the stack; EXP-1097:
    /// blockers alone count all but the first.
    #[test]
    fn the_count_is_everything_but_the_front() {
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        assert_eq!(badge_face(&spec(&stacked)), Some((BadgeShape::Stack, 1)));
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        let mut blocked = spec(&lone);
        blocked.graph.blocked_by = vec![issue("EXP-28", None, None), issue("EXP-29", None, None)];
        assert_eq!(badge_face(&blocked), Some((BadgeShape::Blocked, 1)));
    }

    /// SLOP-16 round 5: the dialog lists the stack's OTHER pull requests
    /// (`#n`, the identifier without a number) and sizes to the capped bands.
    #[test]
    fn the_stack_band_lists_the_other_pull_requests() {
        let mut stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        stacked[1].pr_number = Some(41);
        let graph = spec(&stacked).graph;
        let labels: Vec<String> =
            pr_graph::stack_others(&graph).into_iter().map(stack_row_label).collect();
        assert_eq!(labels, vec!["#41".to_string()]);
        assert_eq!(section_len(&graph, OverlaySection::Stack), 1);
        // One band, one pull-request row (its own height, per band).
        let pr_row = domain::list_item::PR_ROW;
        assert_eq!(content_height(&graph), 2. * DIALOG_PAD + BAND_H + pr_row);
        let mut blocked = graph.clone();
        blocked.blocked_by = (1..=5).map(|n| issue(&format!("EXP-{n}"), None, None)).collect();
        // Blocked by: 3 capped issue rows + "Show 2 more", then the stack
        // band's one pull-request row.
        assert_eq!(
            content_height(&blocked),
            2. * DIALOG_PAD + SECTION_GAP + BAND_H * 2. + ROW_H * 4. + pr_row
        );
    }

    fn issue_row(id: &str, identifier: &str) -> domain::rows::Issue {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "board_id": "board-1",
            "number": 1,
            "identifier": identifier,
            "title": identifier,
            "status": "backlog",
        }))
        .expect("issue row")
    }

    fn relation(id: &str, issue_id: &str, related_issue_id: &str, kind: &str) -> domain::rows::IssueRelation {
        domain::rows::IssueRelation {
            id: id.to_string(),
            issue_id: issue_id.to_string(),
            related_issue_id: related_issue_id.to_string(),
            kind: Some(kind.to_string()),
            source: Some("user".to_string()),
            team_id: None,
            board_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    /// EXP-897: only the INVERSE side of a canonical `blocks` row is a
    /// blocker: the forward side is what THIS issue blocks, and no other
    /// relation type counts at all. Duplicates collapse.
    #[test]
    fn blockers_of_reads_only_the_inverse_side_of_blocks() {
        let rows = vec![issue_row("i-11", "EXP-11"), issue_row("i-13", "EXP-13")];
        let lookup = |id: &str| rows.iter().find(|issue| issue.id == id);
        let relations = vec![
            // EXP-11 blocks me: the one that counts.
            relation("r-1", "i-11", "i-12", "blocks"),
            // …named twice (two rows, one blocker).
            relation("r-2", "i-11", "i-12", "blocks"),
            // I block EXP-13: the other side, not a blocker of mine.
            relation("r-3", "i-12", "i-13", "blocks"),
            // Related/parent never block.
            relation("r-4", "i-13", "i-12", "related"),
            relation("r-5", "i-13", "i-12", "parent"),
        ];
        let blockers = blockers_of("i-12", &relations, lookup);
        assert_eq!(
            blockers.iter().map(|issue| issue.identifier.as_str()).collect::<Vec<_>>(),
            vec!["EXP-11"]
        );
        // Nothing blocks an issue with no inverse rows.
        assert!(blockers_of("i-13", &relations, lookup).is_empty());
    }

    /// A blocker whose issue row has not synced (its board is trashed, or it
    /// belongs to a team this device left) is skipped rather than rendered as
    /// a dangling id.
    #[test]
    fn blockers_of_skips_an_unsynced_blocker() {
        let rows = vec![issue_row("i-11", "EXP-11")];
        let lookup = |id: &str| rows.iter().find(|issue| issue.id == id);
        let relations = vec![
            relation("r-1", "i-11", "i-12", "blocks"),
            relation("r-2", "i-99", "i-12", "blocks"),
        ];
        let blockers = blockers_of("i-12", &relations, lookup);
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].id, "i-11");
    }
}
