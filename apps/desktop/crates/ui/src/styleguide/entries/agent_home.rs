//! EXP-1253 — `agent-home` (3 Special components): the Agent page as HOME
//! (`fixtures/composer-home.json`): the disc mark + the day's greeting, the
//! composer card with the home placeholder and its options line, then
//! RECENT (three small run rows). Drawn from the page's own pieces
//! ([`crate::chat_screen::home_headline`], [`crate::chat_screen::recent_band`],
//! the composer card) over static data; a second specimen shows the card's
//! STARTING row in place of the options line (EXP-1271).

use std::time::{Duration, Instant};

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::home_greeting;
use crate::icons::registry;
use crate::run_rows::{self, RunListFacts, RunRowSize};

pub(crate) const ID: &str = "agent-home";
pub(crate) const OWNER: &str = "EXP-1253";

/// The fixture's first case: "Evening, Danny".
fn greeting() -> String {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).expect("a date");
    home_greeting::home_greeting(date, 19, Some("Danny Straehhuber"))
}

/// The card at rest: the home placeholder, the "+" and the send.
fn card(id: &'static str, cx: &App) -> Div {
    let placeholder = div()
        .w_full()
        .min_h(px(40.))
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(home_greeting::HOME_PLACEHOLDER);
    let composer = crate::composer::GlassComposer::new(placeholder.into_any_element())
        .tool(Button::new((id, 0usize)).ghost().icon(registry::UI_ADD).into_any_element())
        .submit(
            crate::composer::composer_submit((id, 1usize), registry::UI_SUBMIT, true, cx)
                .into_any_element(),
        );
    crate::composer::glass_composer(composer)
}

fn options_line(cx: &App) -> Div {
    div()
        .px_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child("macbook · Opus 5.5 · Plan off")
}

/// Three ended top-level runs as the home's small rows.
fn recent(cx: &mut App) -> gpui::AnyElement {
    let now = chrono::Utc::now();
    let rows: Vec<gpui::AnyElement> = crate::styleguide::entries::session_row::rows(now)
        .iter()
        .filter(|session| session.parent_session_id.is_none())
        .take(home_greeting::RECENT_COUNT)
        .enumerate()
        .map(|(index, session)| {
            let spec = RunListFacts::derive(session, now.timestamp(), cx).row_spec(
                "styleguide-agent-home-recent",
                index,
                RunRowSize::Small,
                domain::tree_guides::Guides::default(),
                false,
                Box::new(|_, _, _| {}),
            );
            run_rows::run_row(spec, cx)
        })
        .collect();
    crate::chat_screen::recent_band(rows, cx)
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let home = v_flex()
        .w(px(640.))
        .child(crate::chat_screen::home_headline(greeting().into(), cx))
        .child(div().h(px(home_greeting::HEADLINE_TO_CARD_PX)))
        .child(v_flex().gap_2().child(card("sg-agent-home", cx)).child(options_line(cx)))
        .child(div().h(px(home_greeting::OPTIONS_TO_RECENT_PX)))
        .child(recent(cx));
    let starting = v_flex().w(px(640.)).gap_2().child(card("sg-agent-home-starting", cx)).child(
        div().px_1().child(crate::feedback::starting_row(
            "sg-agent-home-starting-row",
            "macbook",
            Instant::now() - Duration::from_millis(2300),
            crate::surface::page_ground(),
            cx,
        )),
    );
    v_flex().gap_8().child(home).child(starting)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_greets_like_the_fixture_first_case() {
        assert_eq!(greeting(), "Evening, Danny");
    }
}
