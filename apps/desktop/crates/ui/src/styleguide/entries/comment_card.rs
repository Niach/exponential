//! EXP-1241 (EXP-1259 D14): `comment-card` (Special components): the comment
//! an `exponential_comments_create` call posted, as the run thread draws it —
//! "Commented on" over the REAL thread comment card
//! (`crate::comments::thread_comment_card`): the agent's mark, the author,
//! the muted meta, the body clamped to three lines with Show more.

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::comments::{thread_comment_card, ThreadCommentProps};

pub(crate) const ID: &str = "comment-card";
pub(crate) const OWNER: &str = "EXP-1241";

const BODY: &str = "**Status 2026-10-08, evening**\n\n#VAPP-98 (renderer hardening) is open as PR #1007; the host API run #VAPP-91 is not based on the new contract yet, so I sent it a heads-up.\n\nNext: the conformance fixtures for the Swift and Compose renderers, then the docs pass.";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let header = h_flex()
        .gap_2()
        .items_center()
        .text_xs()
        .child(Icon::from(crate::icons::ExpIcon::Logo).xsmall().text_color(muted))
        .child(SharedString::from(crate::steer_viewer::COMMENTED_ON_LABEL))
        .child(div().text_color(muted).child("VAPP-84"));
    let card = thread_comment_card(
        ThreadCommentProps {
            id: SharedString::from("styleguide-comment-card"),
            mark: crate::coding_selects::agent_mark(coding::CodingAgent::Claude)
                .xsmall()
                .into_any_element(),
            author: Some("Danny".to_string()),
            meta: "2 minutes ago · via MCP".to_string(),
            body: BODY.to_string(),
            team_id: None,
            expanded: false,
            on_toggle: Some(std::rc::Rc::new(|_, _| {})),
            on_open: None,
        },
        cx,
    );
    div()
        .w(px(560.))
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(px(theme::tokens::radius::LG))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .child(header)
        .child(card)
}
