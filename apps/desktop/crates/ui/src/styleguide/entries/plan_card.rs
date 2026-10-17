//! `plan-card` (Special components): THE rich plan / question card
//! (EXP-1280, `fixtures/plan-card.json` ×4), the IDE half of the styleguide
//! entry the web page draws. The rows ARE the fixture's: the
//! `session-thread.json` case "EXP-1280: a rich plan card…" built through
//! the domain mirror (`domain::session_tree::ask_card_row`) and painted by
//! the thread's REAL cards (`crate::thread_screen::ask_row_card`): the
//! pending plan (pictures inline, a pending upload that renders nothing,
//! the inline Exponential UI surface, a picture-tile question and a
//! multi-choice list question, THE options + the feedback field), the
//! answered multi question closed with its picks, and a decided plan.
//! The fixture's attachments are local stand-in pictures (no backend).

use domain::rows::MessageRow;
use domain::session_tree::SessionThreadRow;
use gpui::{div, px, App, Div, Entity, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};
use serde_json::{Map, Value};

use crate::markdown::ImageCache;
use crate::thread_screen::{CardChrome, ThreadText};

pub(crate) const ID: &str = "plan-card";
pub(crate) const OWNER: &str = "EXP-1280";

const SESSION_THREAD_JSON: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/session-thread.json");

/// The fixture case the entry draws.
const CASE_PREFIX: &str = "EXP-1280: a rich plan card";

/// A camelCase fixture row → the snake_case `MessageRow`.
fn message(value: &Value) -> Option<MessageRow> {
    let mut out = Map::new();
    for (key, cell) in value.as_object()? {
        let mut snake = String::new();
        for ch in key.chars() {
            if ch.is_ascii_uppercase() {
                snake.push('_');
                snake.push(ch.to_ascii_lowercase());
            } else {
                snake.push(ch);
            }
        }
        out.insert(snake, cell.clone());
    }
    serde_json::from_value(Value::Object(out)).ok()
}

/// The case's card rows, in Conversation order.
pub(crate) fn rows() -> Vec<SessionThreadRow> {
    let fixture: Value = serde_json::from_str(SESSION_THREAD_JSON).expect("session-thread.json parses");
    let case = fixture["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|case| case["name"].as_str().is_some_and(|name| name.starts_with(CASE_PREFIX)))
        .expect("the EXP-1280 case");
    case["input"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(message)
        .filter_map(|message| domain::session_tree::ask_card_row(&message))
        .collect()
}

/// A stand-in picture: a soft diagonal gradient in `tone`, `w` × `h`.
fn picture_png(tone: [u8; 3], w: u32, h: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(w, h, |x, y| {
        let t = (x as f32 / w as f32 + y as f32 / h as f32) / 2.;
        let [r, g, b] = tone;
        let mix = |c: u8| ((c as f32) * (0.55 + 0.45 * t)) as u8;
        // A lighter "content" band, so it reads as a screen, not a swatch.
        let band = (y as f32 / h as f32 - 0.35).abs() < 0.08 && x > w / 8 && x < w * 7 / 8;
        if band {
            image::Rgba([235, 235, 240, 255])
        } else {
            image::Rgba([mix(r), mix(g), mix(b), 255])
        }
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("png encodes");
    bytes
}

fn seed(cache: &mut ImageCache) {
    for (id, tone, (w, h)) in [
        ("att-1", [64, 92, 160], (720, 450)),
        ("att-2", [46, 128, 112], (400, 250)),
        ("att-3", [150, 84, 160], (400, 250)),
    ] {
        cache.insert_bytes(format!("/api/attachments/{id}"), "image/png", picture_png(tone, w, h));
    }
}

fn caption(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

fn demo(label: &'static str, row: &SessionThreadRow, images: &Entity<ImageCache>, window: &mut Window, cx: &mut App) -> Div {
    let card = crate::thread_screen::ask_row_card(row, &ThreadText::default(), images, CardChrome::default(), window, cx);
    v_flex().w_full().gap_2().child(caption(label, cx)).children(card)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("sg-plan-card-images", cx, |_, _| {
            let mut cache = ImageCache::new(None);
            seed(&mut cache);
            cache
        })
        .clone();
    let rows = rows();
    let label = |row: &SessionThreadRow| match row {
        SessionThreadRow::Plan { decision: None, .. } => "A plan waiting on a decision: pictures, a surface, two questions",
        SessionThreadRow::Plan { .. } => "A decided plan",
        _ => "An answered question with several picks",
    };
    let mut column = v_flex().w(px(640.)).gap_10();
    for row in &rows {
        column = column.child(demo(label(row), row, &images, window, cx));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture case = a pending rich plan, an answered multi question
    /// with picture tiles, a decided plan.
    #[test]
    fn the_rows_are_the_fixture_case() {
        let rows = rows();
        assert_eq!(rows.len(), 3);
        let SessionThreadRow::Plan { questions, surface, decision: None, .. } = &rows[0] else {
            panic!("a pending plan first: {:?}", rows[0]);
        };
        assert_eq!(questions.len(), 2);
        assert!(surface.is_some());
        assert_eq!(crate::plan_card::option_rows(&questions[0].options), domain::plan_card::OptionRows::Tiles);
        assert_eq!(crate::plan_card::option_rows(&questions[1].options), domain::plan_card::OptionRows::List);
        let SessionThreadRow::Question { multi: true, answered: true, options, .. } = &rows[1] else {
            panic!("an answered multi question: {:?}", rows[1]);
        };
        assert_eq!(options[0].attachment_id.as_deref(), Some("att-3"));
        assert!(matches!(&rows[2], SessionThreadRow::Plan { decision: Some(_), .. }));
    }

    /// The entry paints twice in a window (the fields and the surface view
    /// are built between the frames) and settles.
    #[gpui::test]
    async fn the_entry_paints_and_settles(cx: &mut gpui::TestAppContext) {
        use gpui::{IntoElement, Render};

        struct Host;

        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
                // With the session-thread entry: the fixture reuses a card id
                // across cases (one pending, one decided), which must settle.
                let thread = super::super::find(super::super::session_thread::ID).map(|entry| (entry.render)(window, cx));
                gpui::div().children(thread).child(render(window, cx))
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

    #[test]
    fn stand_in_pictures_encode() {
        let png = picture_png([10, 20, 30], 16, 10);
        assert_eq!(&png[1..4], b"PNG");
    }
}
