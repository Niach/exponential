//! EXP-1020 — `sub-shell` (2 General components): the settings row that
//! slides a child page in place of the whole card.
//!
//! EXP-1063: the demo IS `crate::sub_shell` — `SubShellNav` on a small
//! owning view (gpui retains no element state between frames, so the stack
//! lives on an entity `window.use_keyed_state` keeps), `sub_shell_row` as the
//! row, and `SubShellHost` rendering the card at rest or the open page with
//! `controls::back_glyph` on top. The page holds a second sub-shell, so the
//! demo shows one level deeper and Back returning exactly one. Live users:
//! the device settings dialog and Settings → Agents, both for "Workflow
//! settings".

use gpui::{
    div, App, Context, Div, FocusHandle, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window,
};
use gpui_component::{v_flex, ActiveTheme as _, Icon};

use crate::icons::registry;
use crate::sub_shell::{
    focus_back_on_open, sub_shell_row, SubShellHost, SubShellNav, SubShellPage, SubShellProps,
};
use crate::surface;

pub(crate) const ID: &str = "sub-shell";
pub(crate) const OWNER: &str = "EXP-1020";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-sub-shell", cx, |_, cx| SubShellDemo {
        nav: SubShellNav::new(),
        back_focus: cx.focus_handle(),
    });
    div().w(gpui::px(420.)).child(demo)
}

/// The owning view the component asks for: it holds the page stack.
struct SubShellDemo {
    nav: SubShellNav,
    back_focus: FocusHandle,
}

impl SubShellDemo {
    /// A row that pushes `title` onto this view's stack.
    fn row(
        &self,
        props: SubShellProps,
        cx: &mut Context<Self>,
    ) -> Div {
        let title = props.page_title();
        sub_shell_row(
            props,
            cx.listener(move |this: &mut Self, _, window, cx| {
                this.nav.open(title.clone());
                focus_back_on_open(&this.back_focus, window, cx);
                cx.notify();
            }),
            cx,
        )
    }

    fn page(&self, depth: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        if depth == 1 {
            v_flex()
                .w_full()
                .gap_2()
                .child(surface::glass_group_rows(vec![
                    surface::glass_toggle_row(
                        "Run reviews",
                        None,
                        crate::controls::web_switch("sg-sub-shell-reviews")
                            .checked(true)
                            .into_any_element(),
                        cx,
                    ),
                    self.row(
                        SubShellProps::new("sg-sub-shell-advanced", "Advanced")
                            .description("One level deeper; Back returns exactly one."),
                        cx,
                    ),
                ]))
                .into_any_element()
        } else {
            div()
                .text_xs()
                .text_color(muted)
                .child("The deepest page: its parent's rows and header are hidden.")
                .into_any_element()
        }
    }
}

impl Render for SubShellDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let card = surface::glass_group_rows(vec![
            surface::glass_toggle_row(
                "Plan mode",
                None,
                crate::controls::web_switch("sg-sub-shell-plan").into_any_element(),
                cx,
            ),
            self.row(
                SubShellProps::new("sg-sub-shell-workflow", "Workflow settings")
                    .icon(Icon::new(registry::NAV_WORKFLOWS))
                    .value("Opus · Fable"),
                cx,
            ),
            self.row(
                SubShellProps::new("sg-sub-shell-disabled", "Unavailable")
                    .value("disabled")
                    .disabled(true),
                cx,
            ),
        ]);
        let mut host = SubShellHost::new(card);
        if let Some(title) = self.nav.current().cloned() {
            let depth = self.nav.depth();
            let page = self.page(depth, cx);
            host = host.open(
                SubShellPage::new(SharedString::from(title), page),
                &self.back_focus,
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.nav.back();
                    cx.notify();
                }),
            );
        }
        div().w_full().child(host.render(window, cx))
    }
}
