//! The Workflows center screen (EXP-981): this team's workflows in three
//! bands — Running, Draft, Done — newest first inside each.
//!
//! A LIST page like Automations: it lends its rows to the workflow a click
//! opens, so a workflow's graph sits beside this list and its Back comes
//! here. What each band is called, what a row's second line says and which
//! band a status lands in all come from `domain::workflow_view`, the rule
//! every client mirrors; nothing is decided here.
//!
//! Rows are FLAT under a filled band and carry no buttons (the lists rule
//! ×4): a workflow's actions live on its detail.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, ClickEvent, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::workflow_view::{
    workflow_band, workflow_shape_line, WorkflowBand, WORKFLOWS_EMPTY_BODY, WORKFLOWS_EMPTY_TITLE,
    WORKFLOWS_TITLE, WORKFLOW_BANDS,
};

use crate::actions_view::page_scaffold_with;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation, Screen};
use crate::queries;

/// The page column's cap — the Reviews/Drafts width: a row is a name and a
/// one-line shape, not a table.
const WORKFLOWS_COLUMN_W: f32 = 768.;

pub struct WorkflowsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl WorkflowsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.workflows, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// ONE flat row: the workflows glyph, the name, the shape line under it,
    /// and a warning glyph while the plan holds a cycle.
    fn workflow_row(
        &self,
        id: &'static str,
        index: usize,
        row: &domain::rows::WorkflowRow,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let row_hover = theme.list_hover;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let shape = row.shape();
        let name = SharedString::from(
            row.name
                .clone()
                .unwrap_or_else(|| WORKFLOWS_TITLE.to_string()),
        );
        let line = SharedString::from(workflow_shape_line(&shape));
        let cycles = !shape.cycles.is_empty();
        let open_id = row.id.clone();
        crate::surface::flat_row()
            .id((id, index))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2p5()
            .px_3()
            .py_2p5()
            .cursor_pointer()
            .hover(move |style| style.bg(row_hover))
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                crate::navigation::navigate(
                    window,
                    cx,
                    Screen::Workflow {
                        workflow_id: open_id.clone(),
                    },
                );
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .child(Icon::from(registry::NAV_WORKFLOWS).xsmall().text_color(muted)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().w_full().min_w_0().truncate().text_sm().child(name))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(line),
                    ),
            )
            .when(cycles, |this| {
                this.child(
                    div().flex_shrink_0().child(
                        Icon::from(registry::UI_WARNING)
                            .xsmall()
                            .text_color(danger),
                    ),
                )
            })
            .into_any_element()
    }
}

impl Render for WorkflowsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let team_id = active_team_id(&self.nav, cx);
        let (workflows, is_ready) = team_id
            .as_deref()
            .map(|team_id| queries::team_workflows(cx, team_id))
            .unwrap_or_else(|| (Vec::new(), false));

        // §4.1 `is_ready`: a skeleton while the shape has not caught up — an
        // empty collection before that is "still syncing", never "no
        // workflows".
        let column = if !is_ready {
            v_flex()
                .min_w_0()
                .gap_2()
                .child(crate::controls::skeleton().h_3p5().w_40())
                .child(crate::controls::skeleton().h_3p5().w_48())
        } else if workflows.is_empty() {
            v_flex().min_w_0().child(crate::controls::empty_state(
                Icon::from(registry::NAV_WORKFLOWS),
                WORKFLOWS_EMPTY_TITLE,
                WORKFLOWS_EMPTY_BODY,
                cx,
            ))
        } else {
            let mut column = v_flex().min_w_0().gap_6();
            for band in WORKFLOW_BANDS {
                let rows: Vec<&domain::rows::WorkflowRow> = workflows
                    .iter()
                    .filter(|row| workflow_band(row.status_wire()) == band)
                    .collect();
                // An empty band is hidden, never an empty heading.
                if rows.is_empty() {
                    continue;
                }
                let id = band_row_id(band);
                let rendered: Vec<gpui::AnyElement> = rows
                    .iter()
                    .enumerate()
                    .map(|(index, row)| self.workflow_row(id, index, row, cx))
                    .collect();
                column = column.child(
                    v_flex()
                        .min_w_0()
                        .child(crate::surface::glass_section_band(
                            None,
                            band.title(),
                            None,
                            cx,
                        ))
                        .children(rendered),
                );
            }
            column
        };

        page_scaffold_with(
            "workflows-screen-scroll",
            &self.scroll,
            v_flex()
                .gap_6()
                .child(h_flex().min_w_0().child(crate::surface::glass_section_header(
                    WORKFLOWS_TITLE,
                    None,
                    cx,
                )))
                .child(column),
            WORKFLOWS_COLUMN_W,
        )
    }
}

/// A stable per-band element id prefix (gpui ids are `&'static str` + index).
pub(crate) fn band_row_id(band: WorkflowBand) -> &'static str {
    match band {
        WorkflowBand::Running => "workflow-row-running",
        WorkflowBand::Draft => "workflow-row-draft",
        WorkflowBand::Done => "workflow-row-done",
    }
}
