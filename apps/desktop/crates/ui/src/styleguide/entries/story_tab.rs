//! STORIES TAB: `story-tab` (`fixtures/stories.json` `tab.specimen` =
//! tabCases[1]; web entry `story-tab`, iOS + Android twins): THE session
//! screen's Stories tab body ([`crate::story_tab::story_tab_body`]) over the
//! specimen case's ring, the IDE's `wide` frame (the web specimen's card),
//! no PR block, every story INLINE (#EXP-1462): the surfaces and plan texts
//! come from the case's own cards. The pictures are placeholders: the
//! thumbnail's muted fill alone (as web, iOS and Android draw them).

use gpui::{px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};
use serde_json::Value;

use domain::stories::{story_tab_specimen, Story, STORIES};

use crate::markdown::ImageCache;
use crate::thread_screen::ThreadText;

pub(crate) const ID: &str = "story-tab";
pub(crate) const OWNER: &str = "S-3";

/// The specimen's card width (the wide thumbnails' room).
const WIDTH: f32 = 800.;

/// A story's raw card from the specimen case's input.
fn case_card(story: &Story, _cx: &App) -> Option<Value> {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../../../../../packages/domain-contract/fixtures/stories.json")).ok()?;
    fixture["tabCases"][STORIES.tab.specimen.case]["input"]["messages"]
        .as_array()?
        .iter()
        .find(|message| message["id"].as_str() == Some(story.id.as_str()))
        .map(|message| message["card"].clone())
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let ring = story_tab_specimen();
    let images = window.use_keyed_state("sg-story-tab-images", cx, |_, _| ImageCache::new(None)).clone();
    let text = ThreadText::default();
    let tab_body = crate::story_tab::TabBody { images: &images, load_pictures: false, text: &text, card_for: &case_card };
    let body = crate::story_tab::story_tab_body(Some(&ring), None, &tab_body, Some(std::rc::Rc::new(|_, _, _, _| {})), window, cx);
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
