//! EXP-1270: `getting-started` (Special components): the ONE Getting started
//! block — the REAL `getting_started::GettingStartedList` over the pure
//! model's output, with no live data and inert rows, in two states: a fresh
//! team (Create a board expanded) and the contract's "four done" case (done
//! rows over the expanded Invite your team, later rows under it).

use gpui::{
    div, px, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    Window,
};

use crate::getting_started::{
    derive_getting_started, GettingStartedList, Model, Permissions, Platform, Signals, MAX_W,
};

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

fn model(signals: &Signals) -> Model {
    derive_getting_started(signals, Permissions::for_role(true), Platform::Pointer)
}

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    let specimen = |id: &'static str, signals: Signals| {
        div()
            .id(id)
            .w_full()
            .child(GettingStartedList::new(model(&signals), None))
            .into_any_element()
    };
    div()
        .w(px(MAX_W))
        .flex()
        .flex_col()
        .gap_8()
        .child(specimen("getting-started-fresh", Signals::default()))
        .child(specimen("getting-started-mid-path", four_done()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next_keys(model: &Model) -> Vec<&'static str> {
        model
            .entries
            .iter()
            .filter(|entry| entry.next)
            .map(|entry| entry.key.as_str())
            .collect()
    }

    #[test]
    fn the_specimens_are_fresh_and_mid_path() {
        let fresh = model(&Signals::default());
        assert_eq!(fresh.progress, "0 of 8");
        assert_eq!(next_keys(&fresh), vec!["board"]);
        let mid = model(&four_done());
        assert_eq!(mid.progress, "4 of 8");
        assert_eq!(next_keys(&mid), vec!["invite"]);
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
