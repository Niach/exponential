//! `plan-page` (Special components): #EXP-1662 THE plan tab
//! (`fixtures/plan-page.json` ×4), the IDE half of the entry the web page
//! draws (`PlanPageSpecimen`). It paints `specimen` WIDE at 1280 × 720 with the
//! REAL parts: the chat's title (`specimen.chatName`) + THE Segmented of the
//! session screen (#EXP-1732, [`crate::session_chat::screen_tabs`]; the plan
//! is an opened item now, its `‹` back over it) over the REAL body
//! ([`crate::plan_page::plan_page`]): THE Outline with `specimen.activeSection`
//! active, the document from its top (pictures = the committed samples,
//! EXP-1654: [`crate::specimen_pictures`] seeds the entry's cache) and the
//! questions RAIL on question 1 (#FEED-191).
//! `EXP_DEV_SCREEN=styleguide:plan-page` draws it full-window.

use domain::plan_page::PLAN_PAGE;
use domain::rows::MessageRow;
use domain::session_tree::SessionChatRow;
use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::markdown::ImageCache;
use crate::plan_page::PlanPageState;
use crate::session_chat::ChatText;

pub(crate) const ID: &str = "plan-page";
pub(crate) const OWNER: &str = "EXP-1493";

/// The specimen's message id and session (web `plan-page-specimen.tsx`).
const MESSAGE_ID: &str = "plan-specimen";
const SESSION_ID: &str = "plan-specimen-session";

/// The specimen's size (`specimen.$comment`: wide = 1280 × 720).
const WIDE: (f32, f32) = (1280., 720.);

/// The specimen card as its chat row.
pub(crate) fn row() -> SessionChatRow {
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
    let images = window
        .use_keyed_state("sg-plan-page-images", cx, |_, _| {
            let mut cache = ImageCache::new(None);
            crate::specimen_pictures::seed_attachments(&mut cache);
            cache
        })
        .clone();
    let state = window.use_keyed_state("sg-plan-page-state", cx, |_, _| PlanPageState::default()).read(cx).clone();
    let row = row();
    let screen = domain::session_screen::session_screen("session", true, None, Some("plan"), Default::default());
    let tabs = crate::session_chat::screen_tabs(&screen.tabs, screen.active, std::rc::Rc::new(|_, _, _| {}), cx);
    let back = crate::session_screen::back_link_row(
        "sg-plan-page-back",
        crate::session_chat::pane_back_label(screen.active),
        |_, _| {},
        cx,
    );
    let header = v_flex()
        .flex_shrink_0()
        .w_full()
        .px_10()
        .pt_6()
        .gap_4()
        .child(crate::session_chat::title_row(&PLAN_PAGE.specimen.chat_name, false, cx))
        .child(tabs)
        .child(div().w_full().h(px(1.)).bg(cx.theme().border))
        .child(back);
    let page = crate::plan_page::plan_page(
        crate::plan_page::PlanPageProps {
            row: &row,
            text: &ChatText::default(),
            images: &images,
            state: &state,
            active: Some(PLAN_PAGE.specimen.active_section.clone()),
            answer: true,
        },
        window,
        cx,
    );
    v_flex()
        // At most the specimen's width: a narrower pane (the IDE window
        // beside its sidebar) shrinks it, never clips the header and rail.
        .w_full()
        .max_w(px(WIDE.0))
        .h(px(WIDE.1))
        .overflow_hidden()
        .rounded(px(24.))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(crate::dock::card_fill())
        .child(header)
        .child(div().flex_1().min_h_0().w_full().child(page))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen = a waiting plan with two questions and four resolved
    /// pictures (one group: THE Mosaic with "+1").
    #[test]
    fn the_plan_page_specimen_is_the_fixture_card() {
        let SessionChatRow::Plan { questions, decision: None, plan, .. } = row() else {
            panic!("a waiting plan");
        };
        assert_eq!(questions.len(), 2);
        let outline = domain::plan_page::plan_outline(&plan);
        assert!(outline.iter().any(|section| section.id == PLAN_PAGE.specimen.active_section));
    }

    /// The entry paints twice in a window (the panel's fields are built
    /// between the frames) and settles.
    #[gpui::test]
    async fn the_plan_page_entry_paints_and_settles(cx: &mut gpui::TestAppContext) {
        use gpui::{IntoElement, Render};

        struct Host;

        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Host").entered();
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
