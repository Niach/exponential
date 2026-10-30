//! S-3 inter-session links ×4 (`session-thread.json` "S-3 INTER-SESSION
//! LINKS", styleguide entry `session-xref`): THE compact header naming
//! ANOTHER session = the muted lead `copy.xref[kind]`, the agent's mark (its
//! 16px avatar, else the session glyph), the session's name and a muted `›`,
//! one xs line. A click opens that session through the window's navigation,
//! which pushes the current screen on the back stack (the shell's Back pops
//! it), so links are bidirectional.

use gpui::{div, prelude::FluentBuilder as _, px, AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, SharedString, StatefulInteractiveElement as _, Styled, Window};
use gpui_component::{h_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::session_tree::{SessionXref, THREAD};

use crate::icons::registry;

/// The mark's side (the agent avatar's 16px rung).
const XREF_MARK_PX: f32 = 16.;
/// The trailing chevron's side.
const XREF_CHEVRON_PX: f32 = 14.;
/// The ONE gap between the line's parts.
const XREF_GAP_PX: f32 = 4.;

/// The element id of a row's link (`testIds.xref` + the row / view id).
pub(crate) fn xref_id(key: &str) -> SharedString {
    SharedString::from(format!("{}-{key}", THREAD.test_ids.xref))
}

/// Open `session_id` as its screen (a thread root its thread, an agent's
/// conversation its agent page, any other session its view), pushed on the
/// back stack.
pub(crate) fn open_xref(session_id: &str, window: &mut Window, cx: &mut App) {
    let screen = crate::navigation::session_screen_for(session_id, cx);
    crate::navigation::navigate(window, cx, screen);
}

/// The agent's mark: its avatar when `agent_id` names a synced definition
/// (its glyph + colour), else the session glyph (web `SessionXref`).
pub(crate) fn xref_mark(agent_id: Option<&str>, cx: &App) -> AnyElement {
    match agent_id.and_then(|agent_id| crate::session_data::agent_def(agent_id, cx)) {
        Some(def) => agent_mark(def.icon.as_deref(), def.color.as_deref(), cx),
        None => Icon::new(registry::CODING_ASSISTANT).with_size(px(XREF_MARK_PX)).flex_shrink_0().into_any_element(),
    }
}

/// The agent avatar at the mark's side (the styleguide's demo agents).
pub(crate) fn agent_mark(icon: Option<&str>, color: Option<&str>, cx: &App) -> AnyElement {
    crate::user_avatar::agent_avatar(icon, color, gpui_component::Size::Size(px(XREF_MARK_PX)), cx)
}

/// THE link line with its `mark` ([`xref_mark`]), `on_click` overriding the
/// default open (the styleguide passes a no-op).
pub(crate) fn session_xref_with(
    id: impl Into<ElementId>,
    xref: &SessionXref,
    mark: AnyElement,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    xref_line(id, xref, mark, Some(Box::new(on_click)), cx)
}

/// #EXP-1464 (thread-row-action.json): the line with no action (a report
/// about the conversation's own root): no chevron, no hover, no press.
pub(crate) fn session_xref_plain(key: &str, xref: &SessionXref, cx: &App) -> AnyElement {
    let mark = xref_mark(xref.agent_id.as_deref(), cx);
    xref_line(xref_id(key), xref, mark, None, cx)
}

/// The line itself: with `on_click` = a button (pointer, hover =
/// foreground, press = the pressed wash, the trailing `›`).
fn xref_line(
    id: impl Into<ElementId>,
    xref: &SessionXref,
    mark: AnyElement,
    on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    let interactive = on_click.is_some();
    h_flex()
        .id(id)
        .max_w_full()
        .min_w_0()
        .gap(px(XREF_GAP_PX))
        .items_center()
        .text_xs()
        .text_color(muted)
        .child(div().flex_shrink_0().child(SharedString::from(THREAD.copy.xref.lead(&xref.kind).to_string())))
        .child(mark)
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(foreground)
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(SharedString::from(xref.name.clone())),
        )
        .when(interactive, |this| {
            this.child(Icon::new(registry::UI_CHEVRON_RIGHT).with_size(px(XREF_CHEVRON_PX)).flex_shrink_0())
        })
        .when_some(on_click, |this, on_click| {
            this.cursor_pointer()
                .rounded(px(theme::tokens::radius::SM))
                .hover(move |style| style.text_color(foreground))
                .active(|style| style.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
                .on_click(move |_, window, cx| on_click(window, cx))
        })
        .into_any_element()
}

/// THE link line keyed by `key` (`testIds.xref-<key>`), opening its session.
pub(crate) fn session_xref(key: &str, xref: &SessionXref, cx: &App) -> AnyElement {
    let session_id = xref.session_id.clone();
    let mark = xref_mark(xref.agent_id.as_deref(), cx);
    session_xref_with(xref_id(key), xref, mark, move |window, cx| open_xref(&session_id, window, cx), cx)
}
