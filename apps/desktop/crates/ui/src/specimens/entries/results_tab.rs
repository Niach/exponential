//! #FEED-191: `results-tab` (`fixtures/results-tab.json` `tab.specimen`; web
//! entry `results-tab`, iOS + Android twins): THE session screen's Results tab
//! body over the specimen case's fold, the IDE's `wide` frame, no PR block.
//! The pictures are the committed samples (EXP-1654:
//! [`crate::specimen_pictures`] seeds the entry's cache, nothing is fetched); a
//! press opens THE lightbox over that result's pictures. Like web's
//! `ResultsTabSpecimen` (no `surfaceFor`) a surface result draws its text alone.

use gpui::{px, App, Div, InteractiveElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use domain::results_tab::{results_tab_specimen, RESULTS_TAB};

use crate::markdown::ImageCache;

pub(crate) const ID: &str = "results-tab";
pub(crate) const OWNER: &str = "FEED-191";

/// The specimen's card width (web's `ResultsTabSpecimen` default).
const WIDTH: f32 = 640.;

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let results = results_tab_specimen();
    let images = window
        .use_keyed_state("sg-results-tab-images", cx, |_, _| {
            let mut cache = ImageCache::new(None);
            crate::specimen_pictures::seed_attachments(&mut cache);
            cache
        })
        .clone();
    let body = crate::results_tab::results_tab_body(&results, None, &images, &|_, _, _| None, window, cx);
    let background = cx.theme().background;
    v_flex().items_start().child(
        v_flex()
            .id(SharedString::from(RESULTS_TAB.tab.test_ids.list.clone()))
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

    /// The specimen is the fixture's fold, newest first.
    #[test]
    fn the_specimen_is_the_fold() {
        let results = results_tab_specimen();
        assert!(!results.is_empty());
        assert!(results.windows(2).all(|pair| pair[0].created_at >= pair[1].created_at));
    }
}
