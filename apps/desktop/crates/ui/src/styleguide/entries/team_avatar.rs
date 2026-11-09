//! #EXP-1645: `team-avatar` (fixtures/team-image.json): the web catalog entry's
//! specimen. The marks at the ladder's sides (a transparent image at 28 and 20,
//! the glyph at 28, 20 and 18), then the Settings › General row in a group, with
//! and without an image. Inert; seeded bytes, never a fetch.

use gpui::{px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::user_avatar::{seed_team_image, team_avatar, team_image_row};

pub(crate) const ID: &str = "team-avatar";
pub(crate) const OWNER: &str = "EXP-1645";

/// The web specimen's star, at the square the server stores.
const SAMPLE_URL: &str = "specimen://team-star";
const SAMPLE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 24 24"><path fill="#f97316" d="M12 2l3 7h7l-5.5 4.5L18.5 21 12 16.5 5.5 21l2-7.5L2 9h7z"/></svg>"##;

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    seed_team_image(SAMPLE_URL, "image/svg+xml", SAMPLE_SVG.as_bytes(), cx);
    let marks = [(Some(SAMPLE_URL), 28.), (Some(SAMPLE_URL), 20.), (None, 28.), (None, 20.), (None, 18.)]
        .map(|(url, size)| team_avatar(url, size, cx));
    let with_image = team_image_row(Some(SAMPLE_URL), false, |_, _, _| {}, |_, _, _| {}, cx);
    let without = team_image_row(None, false, |_, _, _| {}, |_, _, _| {}, cx);
    v_flex()
        .w(px(448.))
        .gap_3()
        .p_4()
        .text_color(cx.theme().foreground)
        .child(h_flex().items_center().gap_3().children(marks))
        .child(crate::surface::glass_group_rows(vec![with_image, without]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The twin is reachable by id, and the seeded image is what the mark sees.
    #[test]
    fn the_entry_is_found_by_id() {
        let entry = crate::styleguide::entries::find(ID).expect("registered");
        assert_eq!(entry.owner, OWNER);
        assert!(matches!(
            domain::team_image::team_mark(Some(SAMPLE_URL)),
            domain::team_image::TeamMark::Image { .. }
        ));
    }

    /// It paints, twice (the second frame reuses the seed).
    #[gpui::test]
    async fn the_entry_paints(cx: &mut gpui::TestAppContext) {
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
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}
