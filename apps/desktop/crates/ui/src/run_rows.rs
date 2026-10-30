//! EXP-746 — the session rows; EXP-1248 — ONE row; EXP-1294 — no captions;
//! EXP-1323 — nested children (tree guides) and a bold unread title.
//!
//! The rail's Running section draws a live thread through [`run_row`] (web
//! `@exp/ui` SessionRow, fixture `list-item.json` small): the agent's mark,
//! mono identifier · title, nothing else — no caption, no device glyph, no
//! buttons. The mark carries the session state: it animates while the agent
//! works and wears the state's tone as a corner badge while it waits on a
//! person or a usage limit (`fixtures/session-card.json` `tones`).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, ClickEvent, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::ActiveTheme as _;

use domain::session_tree::SessionState;

/// A row callback — boxed so a spec stays one type across callers that close
/// over entities of different views.
pub(crate) type RunRowAction = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A session's agent off its synced `cli`: an absent id is claude (the
/// launcher's default), an id this build does not know `None` (the generic
/// glyph).
pub(crate) fn run_agent(cli: Option<&str>) -> Option<coding::CodingAgent> {
    match cli.map(str::trim).filter(|cli| !cli.is_empty()) {
        None => Some(coding::CodingAgent::default()),
        Some(id) => coding::CodingAgent::parse(id),
    }
}

/// EXP-1294 — the corner badge's tone for a state (`session-card.json`
/// `tones`): only the states that wait on something outside the agent wear
/// one — working moves the mark itself, the quiet states stay bare. Pure.
pub(crate) fn mark_badge_tone(state: SessionState) -> Option<&'static str> {
    match state {
        SessionState::NeedsYou => Some(state.tone()),
        _ => None,
    }
}

/// The run mark's square (= one indent level) and its badge, ×4.
const RUN_MARK_PX: f32 = domain::list_item::MARK;
const RUN_BADGE_PX: f32 = 6.;

/// EXP-1294 — a session's lead: the agent's working mark while it works, else
/// its plain mark with the state's corner badge ([`mark_badge_tone`]).
pub(crate) fn run_mark(
    agent: Option<coding::CodingAgent>,
    state: SessionState,
    cx: &App,
) -> gpui::Div {
    // EXP-1374: background tasks still spin (`running`).
    if state.running() {
        return div()
            .relative()
            .flex_shrink_0()
            .child(crate::coding_selects::agent_working_mark(agent, RUN_MARK_PX));
    }
    crate::coding_selects::run_lead(agent, RUN_MARK_PX, RUN_BADGE_PX, None).when_some(
        mark_badge_tone(state),
        |this, tone| {
            this.child(
                div()
                    .absolute()
                    .top(gpui::px(-2.))
                    .right(gpui::px(-2.))
                    .size(gpui::px(RUN_BADGE_PX))
                    .rounded_full()
                    .bg(crate::work_card::session_tone_color(tone, cx))
                    // The hairline keeps the dot readable over the glyph it
                    // overlaps.
                    .border_1()
                    .border_color(theme::tokens::BACKGROUND.to_hsla()),
            )
        },
    )
}

/// EXP-1294 — everything ONE session row draws ([`run_row`]).
pub(crate) struct RunRowSpec {
    /// Element-id namespace: two lists can share one parent.
    pub(crate) id_prefix: &'static str,
    pub(crate) index: usize,
    pub(crate) agent: Option<coding::CodingAgent>,
    pub(crate) state: SessionState,
    /// The first linked issue's identifier, else none.
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: SharedString,
    pub(crate) active: bool,
    pub(crate) on_open: RunRowAction,
    /// EXP-1318: after the title (a thread row's device icon).
    pub(crate) trailing: Option<gpui::AnyElement>,
    /// EXP-1323: a nested child session's indent + connector (TreeGuides,
    /// 14px/level, the 3px corner tee); default = a root.
    pub(crate) guides: domain::tree_guides::Guides,
    /// EXP-1323: news the viewer has not read — a semibold title.
    pub(crate) unread: bool,
}

/// EXP-1248/EXP-1294 — THE session row, small (web `@exp/ui` SessionRow):
/// [mark][mono identifier · title], on the rail's glass ground. The whole row
/// opens its subject.
pub(crate) fn run_row(spec: RunRowSpec, cx: &App) -> gpui::AnyElement {
    let RunRowSpec {
        id_prefix,
        index,
        agent,
        state,
        identifier,
        title,
        active,
        on_open,
        trailing,
        guides,
        unread,
    } = spec;
    let depth = guides.depth();
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let row_hover = theme::tokens::glass::FILL_ROW.to_hsla();
    let row_active = theme::tokens::glass::FILL_ACTIVE.to_hsla();
    let title_line = div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(gpui::px(6.))
        .text_sm()
        .children(identifier.map(|identifier| {
            div()
                .flex_shrink_0()
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .text_color(muted)
                .child(identifier)
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme.foreground)
                .when(unread, |this| this.font_weight(gpui::FontWeight::SEMIBOLD))
                .child(title),
        );
    crate::surface::flat_row()
        .id((SharedString::from(format!("{id_prefix}-row")), index))
        .relative()
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .items_center()
        .gap(gpui::px(domain::list_item::GAP))
        .h(gpui::px(domain::list_item::SMALL))
        // EXP-1477: the rail rows' 8px side, so a trailing slot lands in
        // the rail's one trailing column.
        .pr_2()
        .pl(gpui::px(domain::list_item::lead_x(depth)))
        .children(crate::tree_guides::guide_layer(&guides, domain::list_item::BASE, 0.))
        .when(active, |this| this.bg(row_active))
        .cursor_pointer()
        .hover(move |style| style.bg(if active { row_active } else { row_hover }))
        .child(run_mark(agent, state, cx))
        .child(title_line)
        .children(trailing)
        .on_click(move |event, window, cx| on_open(event, window, cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mark is one indent level square (the `list-item.json` geometry).
    #[test]
    fn the_run_mark_fills_one_indent_level() {
        assert_eq!(RUN_MARK_PX, crate::tree_guides::LEVEL_PITCH);
    }

    /// An absent `cli` is the launcher's default agent; an unknown one has
    /// no mark of its own.
    #[test]
    fn a_session_names_its_agent_off_its_cli() {
        assert_eq!(run_agent(None), Some(coding::CodingAgent::default()));
        assert_eq!(run_agent(Some(" ")), Some(coding::CodingAgent::default()));
        assert_eq!(run_agent(Some("codex")), Some(coding::CodingAgent::Codex));
        assert_eq!(run_agent(Some("claude")), Some(coding::CodingAgent::Claude));
        assert_eq!(run_agent(Some("something-new")), None);
    }

    /// EXP-1294: only waiting-on-someone states badge the mark, in the
    /// fixture's tone; working animates instead.
    #[test]
    fn only_states_that_wait_badge_the_mark() {
        assert_eq!(mark_badge_tone(SessionState::NeedsYou), Some("amber"));
        for state in [SessionState::Working, SessionState::Idle, SessionState::Done] {
            assert_eq!(mark_badge_tone(state), None, "{state:?}");
        }
    }
}
