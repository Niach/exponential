//! EXP-1270: `getting-started` (Special components): the ONE Getting started
//! block — the REAL `getting_started::block` over the pure model's output for
//! the contract's "four done" case (Invite your team is next), with no live
//! data and inert cards.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use crate::getting_started::{block, derive_getting_started, Permissions, Platform, Signals};

pub(crate) const ID: &str = "getting-started";
pub(crate) const OWNER: &str = "EXP-1270";

/// The fixture's "four done" signals.
fn four_done() -> Signals {
    Signals {
        has_board: true,
        github_installed: true,
        has_desktop_device: true,
        has_coding_session: true,
        ..Signals::default()
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let model = derive_getting_started(
        &four_done(),
        Permissions::for_role(true),
        Platform::Pointer,
    );
    div().w(px(960.)).child(block(&model, None, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_is_mid_progress_with_invite_next() {
        let model = derive_getting_started(
            &four_done(),
            Permissions::for_role(true),
            Platform::Pointer,
        );
        assert_eq!(model.progress, "4 of 8");
        let next: Vec<&str> = model
            .entries
            .iter()
            .filter(|entry| entry.next)
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(next, vec!["invite"]);
    }

    /// The entry paints with the window and the app (the gallery test's
    /// shape, for an entry the section index does not list yet).
    #[gpui::test]
    async fn paints(cx: &mut gpui::TestAppContext) {
        use gpui::{IntoElement, Render};

        struct Demo;

        impl Render for Demo {
            fn render(
                &mut self,
                window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                render(window, cx)
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Demo);
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}
