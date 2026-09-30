//! EXP-736/EXP-1097 — the issue-detail RELATIONS block, direction A
//! ("grouped bands"; web twin `apps/web/src/components/issue-relations-*`).
//!
//! What it draws is [`domain::relations_view`] — the fixture-locked ×4 model:
//!
//! * the "Sub-issue of [chip]" line ABOVE the title ([`render_parent_line`]);
//! * the Sub-issues band: a completion ring filled `done/total` in the team's
//!   done-status colour, "Sub-issues", the `2/5` progress and ONE trailing
//!   `+` that opens the detail's inline sub-issue composer, over flat rows
//!   (status glyph · identifier · title · assignee) — or an "Add sub-issues"
//!   row when there are none;
//! * ONE foldable band per remaining side (Blocked by / Blocking / Duplicate
//!   of / Duplicated by / Related): chevron, glyph, title, count; open
//!   blockers start unfolded, the rest folded; "Show N more" / "Show less"
//!   past the cap. Fold state is per issue, in memory ([`RelationFolds`]).
//!
//! Reads are pure derivations over the synced `issue_relations` + `issues`
//! collections — [`sync::Collections::relations_for_issue`] returns every
//! row touching this issue from EITHER side; the model drops a row whose
//! other issue has not synced.
//!
//! Adding a relation lives in the header's `…` menu and the list row's
//! context menu ([`add_relation_submenu`]); every row (and the parent line)
//! keeps its hover-revealed remove. Writes go through `relations.create` /
//! `relations.delete`, except the "Duplicate of" pick: duplicates are
//! DUAL-WRITTEN with `issues.duplicate_of_id` server-side, so that pick opens
//! the existing duplicate picker ([`crate::issue_detail::open_duplicate_picker`]).

use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::ButtonVariants as _, h_flex, menu::PopupMenuItem, progress::ProgressCircle, v_flex,
    ActiveTheme as _, ElementExt as _, Icon, Sizable as _,
};
use sync::Store;

use domain::relations::{RelationPick, RELATION_PICKS};
use domain::relations_view::{
    copy, issue_relations_view, RelationBandKey, RelationsView, RelationsViewBand,
    RelationsViewInput, RelationsViewIssue, RelationsViewRelation, RelationsViewRow,
};
use domain::rows::Issue;

use crate::icons::{registry, ExpIcon};
use crate::issue_detail::{open_duplicate_picker, open_issue_picker, DETAIL_GUTTER};
use crate::navigation::{navigate, Screen};
use crate::queries;

/// The row's hover group (web `group/relation-row`) — reveals the remove
/// button. Reused per row: gpui resolves `group_hover` against the innermost
/// enclosing group with the name.
const ROW_GROUP: &str = "relation-row";

/// The completion ring's rendered side (web `ProgressRing size={14}`).
const RING_PX: f32 = 14.;

/// The parent chip's title cap on the "Sub-issue of" line.
const PARENT_TITLE_MAX_W: f32 = 320.;

/// The click that opens the detail's inline sub-issue composer.
pub(crate) type AddSubIssue = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// What the issue detail hands the block for its Sub-issues band: the open
/// inline composer (drawn under the rows) and the click that opens it.
/// `on_add = None` = the team has not synced (the composer needs it), so
/// the band offers no `+`.
pub(crate) struct SubIssueComposer {
    pub open: Option<AnyElement>,
    pub on_add: Option<AddSubIssue>,
}

// ---------------------------------------------------------------------------
// Fold state — per issue, in memory
// ---------------------------------------------------------------------------

/// The bands a user folded/unfolded and the ones whose "Show N more" was
/// pressed, per issue — the model's `toggled` / `showAll` inputs. App-global
/// and in-memory: a band stays the way it was left while the app runs.
#[derive(Default)]
struct RelationFolds(HashMap<String, FoldState>);

impl gpui::Global for RelationFolds {}

#[derive(Default, Clone, Debug, PartialEq)]
struct FoldState {
    toggled: Vec<RelationBandKey>,
    show_all: Vec<RelationBandKey>,
}

impl FoldState {
    /// Flip one key in one list (present → gone, absent → pushed).
    fn flip(list: &mut Vec<RelationBandKey>, key: RelationBandKey) {
        match list.iter().position(|kept| *kept == key) {
            Some(index) => {
                list.remove(index);
            }
            None => list.push(key),
        }
    }
}

fn fold_state(issue_id: &str, cx: &App) -> FoldState {
    cx.try_global::<RelationFolds>()
        .and_then(|folds| folds.0.get(issue_id).cloned())
        .unwrap_or_default()
}

/// Fold/unfold (`show_all = false`) or show-all/show-less (`true`) one band,
/// then repaint — the state is a global, not an entity a view observes.
fn flip_fold(issue_id: &str, key: RelationBandKey, show_all: bool, window: &mut Window, cx: &mut App) {
    let state = cx
        .default_global::<RelationFolds>()
        .0
        .entry(issue_id.to_string())
        .or_default();
    if show_all {
        FoldState::flip(&mut state.show_all, key);
    } else {
        FoldState::flip(&mut state.toggled, key);
        // Folding a band forgets its "Show N more": it reopens capped.
        state.show_all.retain(|kept| *kept != key);
    }
    window.refresh();
}

// ---------------------------------------------------------------------------
// The model read
// ---------------------------------------------------------------------------

/// Which relation row a drawn row removes: keyed by the side the model files
/// it under and the OTHER issue's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
    Parent,
    Child,
    Band(RelationBandKey),
}

/// The side a canonical row lands on, read from `issue_id`'s end — the
/// model's own switch (`issueRelationsView`), mirrored for the delete ids.
fn side_of(kind: &str, forward: bool) -> Side {
    match (kind, forward) {
        ("parent", true) => Side::Child,
        ("parent", false) => Side::Parent,
        ("blocks", true) => Side::Band(RelationBandKey::Blocking),
        ("blocks", false) => Side::Band(RelationBandKey::BlockedBy),
        ("duplicate", true) => Side::Band(RelationBandKey::DuplicateOf),
        ("duplicate", false) => Side::Band(RelationBandKey::DuplicatedBy),
        _ => Side::Band(RelationBandKey::Related),
    }
}

/// The view plus the relation ids behind its rows.
struct Read {
    view: RelationsView,
    relation_ids: HashMap<(Side, String), String>,
}

impl Read {
    fn relation_id(&self, side: Side, other_id: &str) -> Option<String> {
        self.relation_ids.get(&(side, other_id.to_string())).cloned()
    }
}

fn status_wire(issue: &Issue) -> String {
    issue.status.as_wire().unwrap_or_default().to_string()
}

fn view_issue(issue: &Issue) -> RelationsViewIssue {
    RelationsViewIssue {
        id: issue.id.clone(),
        identifier: issue.identifier.clone(),
        title: issue.title.clone(),
        status: status_wire(issue),
    }
}

fn read(issue: &Issue, cx: &App) -> Read {
    let Some(store) = Store::try_global(cx) else {
        return Read {
            view: issue_relations_view(&RelationsViewInput {
                subject_id: issue.id.clone(),
                ..Default::default()
            }),
            relation_ids: HashMap::new(),
        };
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let mut relations = Vec::new();
    let mut named: Vec<RelationsViewIssue> = vec![view_issue(issue)];
    let mut relation_ids: HashMap<(Side, String), String> = HashMap::new();
    for row in collections.relations_for_issue(&issue.id, cx) {
        let Some(kind) = row.kind.clone() else {
            continue;
        };
        let forward = row.issue_id == issue.id;
        let other_id = if forward {
            row.related_issue_id.clone()
        } else {
            row.issue_id.clone()
        };
        if let Some(other) = issues.get(&other_id) {
            if !named.iter().any(|kept| kept.id == other.id) {
                named.push(view_issue(other));
            }
        }
        relation_ids
            .entry((side_of(&kind, forward), other_id))
            .or_insert_with(|| row.id.clone());
        relations.push(RelationsViewRelation {
            kind,
            issue_id: row.issue_id.clone(),
            related_issue_id: row.related_issue_id.clone(),
        });
    }
    let folds = fold_state(&issue.id, cx);
    Read {
        view: issue_relations_view(&RelationsViewInput {
            subject_id: issue.id.clone(),
            relations,
            issues: named,
            toggled: folds.toggled,
            show_all: folds.show_all,
        }),
        relation_ids,
    }
}

// ---------------------------------------------------------------------------
// The "Sub-issue of" line
// ---------------------------------------------------------------------------

/// EXP-1097: "↳ Sub-issue of [chip]" ABOVE the title — `None` when the issue
/// has no (synced) parent. The chip opens the parent; the hover ✕ removes
/// the relation.
pub(crate) fn render_parent_line(issue: &Issue, cx: &mut App) -> Option<AnyElement> {
    let read = read(issue, cx);
    let parent = read.view.parent.clone()?;
    let parent_issue = Store::try_global(cx)?
        .collections()
        .issues
        .read(cx)
        .get(&parent.id)
        .cloned()?;
    let status = queries::resolve_issue_status(cx, &parent_issue);
    let muted = cx.theme().muted_foreground;
    let parent_id = parent.id.clone();
    let chip = crate::issue_chip::issue_chip(
        SharedString::from(format!("relation-parent-{}", parent.id)),
        parent.identifier.clone(),
        parent.title.clone(),
    )
    .status(status)
    .max_title_width(px(PARENT_TITLE_MAX_W))
    .on_click(move |_, window, cx| {
        navigate(
            window,
            cx,
            Screen::IssueDetail {
                issue_id: parent_id.clone(),
            },
        );
    });
    Some(
        h_flex()
            .group(ROW_GROUP)
            .w_full()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .px(px(DETAIL_GUTTER))
            .pt(px(crate::work_header::TITLE_PT))
            .child(
                Icon::new(registry::RELATION_SUB_ISSUE)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(muted),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(muted)
                    .child(copy::SUB_ISSUE_OF),
            )
            .child(chip)
            .children(
                read.relation_id(Side::Parent, &parent.id)
                    .map(|relation_id| remove_button(&format!("parent-{relation_id}"), relation_id)),
            )
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// The block under the description
// ---------------------------------------------------------------------------

/// The relations block under the description: the Sub-issues band (or the
/// "Add sub-issues" row) and one foldable band per relation side.
pub(crate) fn render_relations_section(
    issue: &Issue,
    composer: SubIssueComposer,
    cx: &mut App,
) -> AnyElement {
    let read = read(issue, cx);
    let mut column = v_flex().w_full().min_w_0().gap_2().px(px(DETAIL_GUTTER)).pb_2();
    column = column.child(render_sub_issues(issue, &read, composer, cx));
    for band in &read.view.bands {
        column = column.child(render_band(issue, &read, band, cx));
    }
    column.into_any_element()
}

/// The team's DONE status colour — what the completion ring fills with (a
/// renamed/recoloured builtin is still the `done` row).
fn done_color(issue: &Issue, cx: &App) -> gpui::Hsla {
    let team_id = Store::try_global(cx).and_then(|store| {
        store
            .collections()
            .boards
            .read(cx)
            .get(&issue.board_id)
            .map(|board| board.team_id.clone())
    });
    let status = team_id
        .and_then(|team_id| {
            queries::team_status_options(cx, &team_id)
                .into_iter()
                .find(|status| status.builtin_key.as_deref() == Some("done"))
        })
        .unwrap_or_else(|| domain::statuses::constructed_default(domain::IssueStatus::Done));
    crate::icons::status_tint_color(&status.tint, cx)
}

/// The ring's filled share, 0..100 (web `progressRingPercent`).
fn ring_percent(done: usize, total: usize) -> f32 {
    if total == 0 {
        return 0.;
    }
    (done.min(total) as f32 / total as f32) * 100.
}

fn render_sub_issues(
    issue: &Issue,
    read: &Read,
    composer: SubIssueComposer,
    cx: &mut App,
) -> AnyElement {
    let sub = &read.view.sub_issues;
    let muted = cx.theme().muted_foreground;
    let SubIssueComposer { open, on_add } = composer;

    if sub.total == 0 {
        // No band yet: the composer itself, or the one "Add sub-issues" row.
        if let Some(open) = open {
            return open;
        }
        let Some(on_add) = on_add else {
            return div().into_any_element();
        };
        // A row, not a block: a block child stretches the ghost button to
        // the full column width and its label ends up centred.
        return h_flex()
            .w_full()
            .justify_start()
            .child(
                crate::issue_composer::add_sub_issues_button(cx)
                    .label(copy::ADD_SUB_ISSUES)
                    .on_click(move |event, window, cx| on_add(event, window, cx)),
            )
            .into_any_element();
    }

    let ring = gpui_component::Sizable::with_size(
        ProgressCircle::new("relations-sub-issue-ring")
            .value(ring_percent(sub.done, sub.total))
            .color(done_color(issue, cx)),
        // `Size::Size(s)` renders at `s * 0.75` (the context ring's note).
        px(RING_PX / 0.75),
    )
    .into_any_element();
    let mut trailing = h_flex().flex_shrink_0().items_center().gap_1().child(
        div()
            .text_xs()
            .text_color(muted)
            .font_family(theme::terminal::FONT_FAMILY)
            .child(SharedString::from(sub.progress.clone().unwrap_or_default())),
    );
    if let Some(on_add) = on_add {
        trailing = trailing.child(
            gpui_component::button::Button::new("relations-sub-issue-add")
                .ghost()
                .cursor_pointer()
                .xsmall()
                .icon(Icon::new(registry::UI_ADD))
                .tooltip(copy::ADD_SUB_ISSUES)
                .on_click(move |event, window, cx| on_add(event, window, cx)),
        );
    }
    let band = crate::surface::glass_section_band(
        Some(ring),
        copy::SUB_ISSUES,
        Some(trailing.into_any_element()),
        cx,
    );
    let mut column = v_flex().w_full().min_w_0().child(band);
    for row in &sub.rows {
        let relation_id = read.relation_id(Side::Child, &row.id);
        column = column.children(render_row("sub", row, relation_id, cx));
    }
    v_flex()
        .w_full()
        .min_w_0()
        .child(column)
        .children(open.map(|open| div().pt_1().child(open)))
        .into_any_element()
}

/// The glyph of one band — the relation pick's concept for that side.
fn band_icon(key: RelationBandKey) -> ExpIcon {
    let (kind, inverse) = match key {
        RelationBandKey::BlockedBy => ("blocks", true),
        RelationBandKey::Blocking => ("blocks", false),
        RelationBandKey::DuplicateOf => ("duplicate", false),
        RelationBandKey::DuplicatedBy => ("duplicate", true),
        RelationBandKey::Related => ("related", false),
    };
    icon_for_concept(domain::relations::icon_name(kind, inverse))
}

/// A stable id fragment per band key.
fn band_slug(key: RelationBandKey) -> &'static str {
    match key {
        RelationBandKey::BlockedBy => "blocked-by",
        RelationBandKey::Blocking => "blocking",
        RelationBandKey::DuplicateOf => "duplicate-of",
        RelationBandKey::DuplicatedBy => "duplicated-by",
        RelationBandKey::Related => "related",
    }
}

fn render_band(issue: &Issue, read: &Read, band: &RelationsViewBand, cx: &mut App) -> AnyElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let key = band.key;
    let slug = band_slug(key);
    let leading = h_flex()
        .flex_shrink_0()
        .items_center()
        .gap_1p5()
        .child(
            Icon::new(if band.expanded {
                registry::UI_CHEVRON_DOWN
            } else {
                registry::UI_CHEVRON_RIGHT
            })
            .xsmall()
            .text_color(foreground.opacity(0.7)),
        )
        .child(Icon::new(band_icon(key)).xsmall().text_color(muted))
        .into_any_element();
    let count = div()
        .flex_shrink_0()
        .text_xs()
        .text_color(foreground.opacity(0.5))
        .child(SharedString::from(band.count.to_string()))
        .into_any_element();
    let fold_issue = issue.id.clone();
    let header = crate::surface::glass_section_band(Some(leading), band.title.clone(), Some(count), cx)
        .id(SharedString::from(format!("relations-band-{slug}")))
        .cursor_pointer()
        .on_click(move |_, window, cx| flip_fold(&fold_issue, key, false, window, cx));
    let mut column = v_flex().w_full().min_w_0().child(header);
    let side = Side::Band(key);
    for row in &band.rows {
        let relation_id = read.relation_id(side, &row.id);
        column = column.children(render_row(slug, row, relation_id, cx));
    }
    if let Some(label) = band.more.clone().or_else(|| band.less.clone()) {
        let more_issue = issue.id.clone();
        column = column.child(
            crate::surface::flat_row()
                .id(SharedString::from(format!("relations-band-{slug}-more")))
                .flex()
                .w_full()
                .items_center()
                .px_3()
                .py_1()
                .cursor_pointer()
                .hover(|style| style.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(label))
                .on_click(move |_, window, cx| flip_fold(&more_issue, key, true, window, cx)),
        );
    }
    column.into_any_element()
}

/// One flat row: status glyph · identifier · title · (hover ✕) · assignee.
/// Opens the issue; hovering shows the shared issue preview card.
fn render_row(
    slug: &str,
    row: &RelationsViewRow,
    relation_id: Option<String>,
    cx: &mut App,
) -> Option<AnyElement> {
    let other = Store::try_global(cx)?
        .collections()
        .issues
        .read(cx)
        .get(&row.id)
        .cloned()?;
    let remove = relation_id
        .map(|relation_id| remove_button(&format!("{slug}-{}", other.id), relation_id));
    Some(issue_row(
        &format!("relation-row-{slug}-{}", other.id),
        &other,
        IssueRowOpts {
            title: Some(SharedString::from(row.title.clone())),
            open: row.open,
            remove,
            guides: None,
            in_dialog: false,
        },
        cx,
    ))
}

/// SLOP-16 round 3 — how one [`issue_row`] sits in its list.
pub(crate) struct IssueRowOpts {
    /// The title text; `None` = the issue's own title.
    pub(crate) title: Option<SharedString>,
    /// A closed issue's title is dimmed.
    pub(crate) open: bool,
    /// The relations card's hover-revealed ✕.
    pub(crate) remove: Option<AnyElement>,
    /// EXP-965: a NESTED row's place in its tree (a batch PR's folded
    /// issues); `None` = a flat top-level row.
    pub(crate) guides: Option<domain::tree_guides::Guides>,
    /// Drawn in a dialog window: a click closes it and opens the issue in the
    /// opener.
    pub(crate) in_dialog: bool,
}

/// The base left padding of an [`issue_row`] (`px_3`); a nested row's
/// connector gutter is measured off it.
const ISSUE_ROW_PAD: f32 = 12.;

/// SLOP-16 round 3 — THE issue row: status glyph · mono identifier · title
/// · (hover ✕) · assignee, a flat `px_3 py_1` row. Opens the issue; hovering
/// shows the shared issue preview card. The relations card draws it, and so
/// does the work header's "Related work" dialog (`pr_graph`).
pub(crate) fn issue_row(key: &str, other: &Issue, opts: IssueRowOpts, cx: &mut App) -> AnyElement {
    let IssueRowOpts {
        title: title_text,
        open,
        remove,
        guides,
        in_dialog,
    } = opts;
    let status = queries::resolve_issue_status(cx, other);
    let assignee = other.assignee_id.as_deref().map(|user_id| {
        let user = Store::try_global(cx)
            .and_then(|store| store.collections().users.read(cx).get(user_id).cloned());
        crate::user_avatar::user_avatar(
            user_id,
            &crate::comments::user_label(user_id, user.as_ref()),
            user.as_ref().and_then(|user| user.image.as_deref()),
            gpui_component::Size::XSmall,
            cx,
        )
    });
    let issue_id = other.id.clone();
    let mut title = div()
        .flex_1()
        .min_w_0()
        .text_sm()
        .truncate()
        .child(title_text.unwrap_or_else(|| SharedString::from(other.title.clone())));
    // A closed issue's title is dimmed (web `text-foreground/60`).
    if !open {
        title = title.text_color(cx.theme().foreground.opacity(0.6));
    }
    // EXP-760: the row opens the shared issue hover preview — the same card
    // the `#IDENT` pills in prose show. The row's painted rectangle is the
    // anchor, captured at prepaint (the `Popup` recipe) because a hover
    // listener is handed the pointer, not the element.
    let row_key = key.to_string();
    let preview_key = row_key.clone();
    let preview_issue = other.id.clone();
    let anchor: Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>> =
        Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
    let anchor_write = anchor.clone();
    let depth = guides.as_ref().map_or(0, |guides| guides.depth());
    crate::surface::flat_row()
        .id(ElementId::from(SharedString::from(row_key)))
        .on_prepaint(move |bounds, _window, _cx| anchor_write.set(bounds))
        .on_hover(move |hovered, window, cx| {
            let host = crate::issue_preview::host_for_window(window, cx);
            if *hovered {
                let issue_id = preview_issue.clone();
                let bounds = anchor.get();
                host.update(cx, |host, cx| {
                    host.request(preview_key.clone(), issue_id, bounds, cx)
                });
            } else {
                host.update(cx, |host, cx| host.release(preview_key.clone(), cx));
            }
        })
        .group(ROW_GROUP)
        .flex()
        .w_full()
        .min_w_0()
        .relative()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .when(depth > 0, |row| {
            row.pl(px(ISSUE_ROW_PAD + crate::tree_guides::LEVEL_PITCH * depth as f32))
        })
        .children(
            guides
                .as_ref()
                .and_then(|guides| crate::tree_guides::guide_layer(guides, ISSUE_ROW_PAD, 0.)),
        )
        .cursor_pointer()
        .hover(|style| style.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .on_click(move |_, window, cx| {
            let screen = Screen::IssueDetail {
                issue_id: issue_id.clone(),
            };
            if in_dialog {
                crate::native_dialog::close_then(window, cx, move |window, cx| {
                    navigate(window, cx, screen);
                });
            } else {
                navigate(window, cx, screen);
            }
        })
        .child(
            crate::icons::resolved_status_icon(&status, cx)
                .small()
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(other.identifier.clone())),
        )
        .child(title)
        .children(remove)
        .children(assignee)
        .into_any_element()
}

/// The hover-revealed "Remove relation" ✕ of a row (or the parent line).
fn remove_button(key: &str, relation_id: String) -> AnyElement {
    div()
        .invisible()
        .group_hover(ROW_GROUP, |style| style.visible())
        .flex_shrink_0()
        .child(
            gpui_component::button::Button::new(ElementId::from(SharedString::from(format!(
                "relation-remove-{key}"
            ))))
            .ghost()
            .cursor_pointer()
            .xsmall()
            .icon(Icon::new(registry::UI_CLOSE))
            .tooltip("Remove relation")
            .on_click(move |_, _window, cx| {
                cx.stop_propagation();
                spawn_relation_delete(cx, relation_id.clone());
            }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Adding + removing
// ---------------------------------------------------------------------------

/// EXP-760: the six picks as MENU ITEMS, for whichever menu hosts them — the
/// issue header's `…` and the list row's context menu both open the same
/// second-stage picker.
pub(crate) fn add_relation_submenu(
    mut menu: gpui_component::menu::PopupMenu,
    issue_id: &str,
) -> gpui_component::menu::PopupMenu {
    for pick in RELATION_PICKS {
        let issue_id = issue_id.to_string();
        menu = menu.item(
            PopupMenuItem::new(pick.label)
                .icon(Icon::new(pick_icon(&pick)))
                .on_click(move |_, window, cx| {
                    open_relation_target_picker(issue_id.clone(), pick, window, cx);
                }),
        );
    }
    menu
}

/// Stage two of a pick: choose the issue on the other side. "Duplicate of"
/// takes the duplicate picker instead — the duplicate relation is the mirror
/// of `issues.duplicate_of_id` and only `issues.update` writes both halves.
pub(crate) fn open_relation_target_picker(
    issue_id: String,
    pick: RelationPick,
    window: &mut Window,
    cx: &mut App,
) {
    if pick.kind == domain::contract::ISSUE_RELATION_TYPE_DUPLICATE && !pick.inverse {
        open_duplicate_picker(issue_id, window, cx);
        return;
    }
    let source_id = issue_id.clone();
    open_issue_picker(
        issue_id,
        format!("{} …", pick.label),
        "Search issues…",
        Rc::new(move |related_issue_id: String, _window: &mut Window, cx: &mut App| {
            spawn_relation_create(cx, source_id.clone(), related_issue_id, pick);
        }),
        window,
        cx,
    );
}

fn spawn_relation_create(
    cx: &mut App,
    issue_id: String,
    related_issue_id: String,
    pick: RelationPick,
) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] relations.create skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::relations::relations_create(
                &trpc,
                &issue_id,
                &related_issue_id,
                pick.kind,
                pick.inverse,
            ) {
                log::warn!("[ui] relations.create({}) failed: {err}", pick.kind);
            }
        })
        .detach();
}

fn spawn_relation_delete(cx: &mut App, relation_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] relations.delete skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::relations::relations_delete(&trpc, &relation_id) {
                log::warn!("[ui] relations.delete({relation_id}) failed: {err}");
            }
        })
        .detach();
}

/// A pick's icon concept → its glyph.
fn pick_icon(pick: &RelationPick) -> ExpIcon {
    icon_for_concept(pick.icon)
}

/// The `packages/icons` CONCEPT name → the generated registry glyph (§Shared
/// Contracts: multi-client surfaces name a concept, never a raw glyph).
fn icon_for_concept(concept: &str) -> ExpIcon {
    match concept {
        "relation-parent" => registry::RELATION_PARENT,
        "relation-sub-issue" => registry::RELATION_SUB_ISSUE,
        "relation-blocks" => registry::RELATION_BLOCKS,
        "relation-blocked-by" => registry::RELATION_BLOCKED_BY,
        "relation-duplicate" => registry::RELATION_DUPLICATE,
        "relation-related" => registry::RELATION_RELATED,
        _ => registry::RELATION_SECTION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::IconNamed as _;

    #[test]
    fn every_pick_and_band_resolves_a_real_glyph() {
        // `ExpIcon` carries no PartialEq (the macro derives Clone +
        // IntoElement only), so glyphs compare by their SVG path like the
        // issue-files table does.
        for pick in RELATION_PICKS {
            assert_ne!(
                pick_icon(&pick).path(),
                registry::RELATION_SECTION.path(),
                "pick {} fell back to the section glyph",
                pick.label
            );
        }
        for key in [
            RelationBandKey::BlockedBy,
            RelationBandKey::Blocking,
            RelationBandKey::DuplicateOf,
            RelationBandKey::DuplicatedBy,
            RelationBandKey::Related,
        ] {
            assert_ne!(
                band_icon(key).path(),
                registry::RELATION_SECTION.path(),
                "band {key:?} fell back to the section glyph"
            );
        }
        // The pick-less inverse side still gets its forward glyph.
        assert_eq!(
            band_icon(RelationBandKey::DuplicatedBy).path(),
            registry::RELATION_DUPLICATE.path()
        );
        assert_eq!(
            band_icon(RelationBandKey::BlockedBy).path(),
            registry::RELATION_BLOCKED_BY.path()
        );
    }

    /// The delete ids key on the SAME side the model files a row under —
    /// otherwise a row's ✕ would remove nothing (or the wrong relation).
    #[test]
    fn delete_sides_mirror_the_model() {
        let subject = "s";
        let issues: Vec<RelationsViewIssue> = ["s", "p", "c", "b", "k", "d", "e", "r"]
            .iter()
            .enumerate()
            .map(|(n, id)| RelationsViewIssue {
                id: id.to_string(),
                identifier: format!("EXP-{n}"),
                title: id.to_string(),
                status: "backlog".into(),
            })
            .collect();
        let rel = |kind: &str, from: &str, to: &str| RelationsViewRelation {
            kind: kind.into(),
            issue_id: from.into(),
            related_issue_id: to.into(),
        };
        let relations = vec![
            rel("parent", "p", "s"),
            rel("parent", "s", "c"),
            rel("blocks", "b", "s"),
            rel("blocks", "s", "k"),
            rel("duplicate", "s", "d"),
            rel("duplicate", "e", "s"),
            rel("related", "r", "s"),
        ];
        let expected: Vec<(Side, String)> = relations
            .iter()
            .map(|row| {
                let forward = row.issue_id == subject;
                let other = if forward { &row.related_issue_id } else { &row.issue_id };
                (side_of(&row.kind, forward), other.clone())
            })
            .collect();
        let view = issue_relations_view(&RelationsViewInput {
            subject_id: subject.into(),
            relations,
            issues,
            toggled: vec![RelationBandKey::DuplicateOf, RelationBandKey::DuplicatedBy, RelationBandKey::Related],
            show_all: Vec::new(),
        });
        let mut drawn: Vec<(Side, String)> = Vec::new();
        if let Some(parent) = &view.parent {
            drawn.push((Side::Parent, parent.id.clone()));
        }
        for row in &view.sub_issues.rows {
            drawn.push((Side::Child, row.id.clone()));
        }
        for band in &view.bands {
            for row in &band.rows {
                drawn.push((Side::Band(band.key), row.id.clone()));
            }
        }
        assert_eq!(drawn, expected);
    }

    /// Folding a band forgets its "Show N more"; a second flip undoes the
    /// first.
    #[test]
    fn fold_flips_are_their_own_inverse() {
        let mut state = FoldState::default();
        FoldState::flip(&mut state.toggled, RelationBandKey::Related);
        assert_eq!(state.toggled, vec![RelationBandKey::Related]);
        FoldState::flip(&mut state.toggled, RelationBandKey::Related);
        assert!(state.toggled.is_empty());
    }

    #[test]
    fn the_ring_fills_done_over_total() {
        assert_eq!(ring_percent(0, 0), 0.);
        assert_eq!(ring_percent(2, 4), 50.);
        assert_eq!(ring_percent(5, 4), 100.);
    }
}
