//! STORIES TAB: `story-tab` (`fixtures/stories.json` `tab.specimen` =
//! tabCases[1]; web entry `story-tab`, iOS + Android twins): THE session
//! screen's Stories tab body ([`crate::story_tab::story_tab_body`]) over the
//! specimen case's ring, the IDE's `wide` frame (640 px, the web
//! specimen's card), no PR block. The pictures are placeholders: the
//! thumbnail's muted fill alone (as web, iOS and Android draw them).

use gpui::{px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use domain::stories::story_tab_specimen;

pub(crate) const ID: &str = "story-tab";
pub(crate) const OWNER: &str = "S-3";

/// The web specimen's card width.
const WIDTH: f32 = 640.;

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let ring = story_tab_specimen();
    let body = crate::story_tab::story_tab_body(Some(&ring), None, None, Some(std::rc::Rc::new(|_, _, _, _| {})), cx);
    let background = cx.theme().background;
    v_flex().items_start().child(
        v_flex()
            .w(px(WIDTH))
            .p(px(20.))
            .rounded(px(24.))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .bg(background)
            .child(body),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tabCases[1]: the waiting question first, then newest first.
    #[test]
    fn the_specimen_is_the_tab_case() {
        let ring = story_tab_specimen();
        let ids: Vec<&str> = ring.stories.iter().map(|story| story.id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"q-wait"));
        assert!(!ids.contains(&"plan-old"));
    }
}
