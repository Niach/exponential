//! Debug board — the Phase-2 "live sync of the 14 shapes renders a board"
//! gate surface (masterplan-v3 §11.4 Phase 2; §5.11 gate 2). Phase 3 replaces
//! it with the real virtualized project board.
//!
//! Renders, live off the sync collections:
//! * a per-shape **status line** — name + sync phase
//!   (waiting/snapshot/live/refetching) + row count for all 14 shapes — the
//!   runtime-verification instrument for the §5.11 gates (cursor resume,
//!   atomic refetch, long-poll pacing);
//! * the issues grouped by status in the domain display order, identifier +
//!   title rows, honoring the §4.1 skeleton-vs-empty distinction (`is_ready`).

use gpui::{
    div, App, FocusHandle, Focusable, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    dock::{Panel, PanelControl, PanelEvent},
    h_flex, v_flex, ActiveTheme as _,
};
use sync::{ShapeSyncPhase, Store};

/// Stable serialization name (§3.3: never change once shipped in a layout).
pub const PANEL_NAME: &str = "DebugBoard";

pub struct DebugBoardPanel {
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl DebugBoardPanel {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Coarse observe-all is fine for the debug surface; real screens
        // observe only the collections they read (§5.8).
        let store = Store::global(cx).clone();
        let mut subscriptions = store.observe_collections(cx);
        subscriptions.push(cx.observe(&store.state(), |_, _, cx| cx.notify()));

        Self {
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The 14-shape status line — snapshot/live/refetching + row counts.
    fn render_status_line(&self, cx: &App) -> impl IntoElement {
        let statuses = Store::global(cx).shape_statuses(cx);
        h_flex()
            .flex_wrap()
            .gap_1()
            .children(statuses.into_iter().map(|status| {
                let phase_color = match status.phase {
                    ShapeSyncPhase::Live => cx.theme().success,
                    ShapeSyncPhase::Refetching => cx.theme().warning,
                    ShapeSyncPhase::Snapshot => cx.theme().info,
                    ShapeSyncPhase::Waiting => cx.theme().muted_foreground,
                };
                let text: SharedString =
                    format!("{} {} · {}", status.name, status.phase.label(), status.rows).into();
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().muted)
                    .text_xs()
                    .text_color(phase_color)
                    .child(text)
            }))
    }

    /// Issues grouped by status in the domain display order (§4.1's board
    /// query as a plain in-memory filter/sort — no SQL at render time).
    fn render_groups(&self, cx: &App) -> impl IntoElement {
        let collections = Store::global(cx).collections();
        let issues = collections.issues.read(cx);

        let mut groups = v_flex().gap_4();

        if !issues.is_ready() && issues.is_empty() {
            // §4.1 load-bearing distinction: an empty collection before the
            // first up-to-date is "still syncing", NEVER "no issues" (the
            // EXP-1 #13 empty-snapshot trap).
            return groups.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Syncing…"),
            );
        }
        if issues.is_empty() {
            return groups.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No issues."),
            );
        }

        for status in domain::IssueStatus::DISPLAY_ORDER {
            let mut rows: Vec<_> = issues.iter().filter(|i| i.status == status).collect();
            if rows.is_empty() {
                continue; // empty status groups are hidden (web parity)
            }
            rows.sort_by(|a, b| {
                let ord = a
                    .sort_order
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.sort_order.unwrap_or(f64::MAX));
                ord.then_with(|| a.identifier.cmp(&b.identifier))
            });

            let header: SharedString = format!("{} ({})", status.label(), rows.len()).into();
            let mut group = v_flex().gap_1().child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(header),
            );
            for issue in rows {
                group = group.child(
                    h_flex()
                        .gap_2()
                        .px_2()
                        .py_0p5()
                        .rounded(cx.theme().radius)
                        .hover(|s| s.bg(cx.theme().list_hover))
                        .child(
                            div()
                                .w_16()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(issue.identifier.clone())),
                        )
                        .child(
                            div()
                                .text_sm()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(SharedString::from(issue.title.clone())),
                        ),
                );
            }
            groups = groups.child(group);
        }
        groups
    }
}

impl Panel for DebugBoardPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Board (debug)"
    }

    /// The only center content until Phase 3 — closing it would leave an
    /// empty center that persists into the saved layout.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

impl gpui::EventEmitter<PanelEvent> for DebugBoardPanel {}

impl Focusable for DebugBoardPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DebugBoardPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_3()
            .gap_3()
            // NB: `theme().list` is the ListSettings struct — the list
            // SURFACE color lives on the ThemeColor field (`colors.list`).
            .bg(cx.theme().colors.list)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Debug board — Phase 3 replaces this with the real project board."),
            )
            .child(self.render_status_line(cx))
            .child(
                div()
                    .id("debug-board-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(self.render_groups(cx)),
            )
    }
}
