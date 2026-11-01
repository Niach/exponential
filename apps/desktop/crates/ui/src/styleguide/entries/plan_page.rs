//! `plan-page` (Special components): THE plan page (#EXP-1493,
//! `fixtures/plan-page.json` ×4), the IDE half of the entry the web page
//! draws (`PlanPageSpecimen`). It paints `specimen` WIDE at 1280 × 720 with
//! the REAL page ([`crate::plan_page::plan_page`]): the header (the chat's
//! name = `specimen.chatName`, the kind badge, the waiting dot, the counts
//! line), THE Outline with `specimen.activeSection` active, the document
//! from its top (pictures = the muted placeholders) and the rail on
//! question 1. `EXP_DEV_SCREEN=styleguide:plan-page` draws it full-window.

use domain::plan_page::PLAN_PAGE;
use domain::rows::MessageRow;
use domain::session_tree::SessionThreadRow;
use gpui::{div, px, App, Div, ParentElement as _, ScrollHandle, SharedString, Styled as _, Window};
use gpui_component::ActiveTheme as _;

use crate::markdown::ImageCache;
use crate::thread_screen::ThreadText;

pub(crate) const ID: &str = "plan-page";
pub(crate) const OWNER: &str = "EXP-1493";

/// The specimen's message id and session (web `plan-page-specimen.tsx`).
const MESSAGE_ID: &str = "plan-specimen";
const SESSION_ID: &str = "plan-specimen-session";

/// The specimen's size (`specimen.$comment`: wide = 1280 × 720).
const WIDE: (f32, f32) = (1280., 720.);

/// The specimen card as its thread row.
pub(crate) fn row() -> SessionThreadRow {
    let message = MessageRow {
        id: MESSAGE_ID.to_string(),
        conversation_id: "plan-specimen-conversation".to_string(),
        session_id: SESSION_ID.to_string(),
        author_kind: Some("agent".to_string()),
        author_session_id: Some(SESSION_ID.to_string()),
        card: Some(PLAN_PAGE.specimen.card.clone()),
        ..Default::default()
    };
    domain::session_tree::ask_card_row(&message).expect("the specimen is a plan card")
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-plan-page-images", cx, |_, _| ImageCache::new(None)).clone();
    let scroll = window.use_keyed_state("sg-plan-page-scroll", cx, |_, _| ScrollHandle::new()).read(cx).clone();
    let row = row();
    let noop = || -> Option<crate::plan_page::PageAction> { Some(std::rc::Rc::new(|_, _| {})) };
    let page = crate::plan_page::plan_page(
        crate::plan_page::PlanPageProps {
            row: &row,
            chat_name: SharedString::from(PLAN_PAGE.specimen.chat_name.clone()),
            text: &ThreadText::default(),
            images: &images,
            placeholders: true,
            scroll: &scroll,
            active: Some(PLAN_PAGE.specimen.active_section.clone()),
            on_back: noop(),
            on_close: noop(),
            on_open_chat: noop(),
        },
        window,
        cx,
    );
    div()
        // At most the specimen's width: a narrower pane (the IDE window
        // beside its sidebar) shrinks it, never clips the header and rail.
        .w_full()
        .max_w(px(WIDE.0))
        .h(px(WIDE.1))
        .overflow_hidden()
        .rounded(px(24.))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(cx.theme().background)
        .child(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen = a waiting plan with two questions and four resolved
    /// pictures (one group: THE Mosaic with "+1").
    #[test]
    fn the_plan_page_specimen_is_the_fixture_card() {
        let SessionThreadRow::Plan { questions, decision: None, plan, .. } = row() else {
            panic!("a waiting plan");
        };
        assert_eq!(questions.len(), 2);
        let outline = domain::plan_page::plan_outline(&plan);
        assert!(outline.iter().any(|section| section.id == PLAN_PAGE.specimen.active_section));
    }

    /// The entry paints twice in a window (the rail's fields are built
    /// between the frames) and settles.
    #[gpui::test]
    async fn the_plan_page_entry_paints_and_settles(cx: &mut gpui::TestAppContext) {
        use gpui::{IntoElement, Render};

        struct Host;

        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
                render(window, cx)
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Host);
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
    }
}
