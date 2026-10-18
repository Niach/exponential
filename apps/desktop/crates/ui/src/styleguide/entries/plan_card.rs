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
use gpui::{div, px, App, Div, Entity, ParentElement as _, SharedString, Styled as _, Window};
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

/// THE stand-in palette ×4 (web `plan-card-specimen.tsx` `TONES`; the
/// iOS / Android specimens draw the same): attachment id → gradient stops.
const TONES: [(&str, [u8; 3], [u8; 3]); 3] = [
    ("att-1", [0x3f, 0x3f, 0x46], [0x52, 0x52, 0x5b]),
    ("att-2", [0x1e, 0x3a, 0x5f], [0x3b, 0x82, 0xf6]),
    ("att-3", [0x14, 0x53, 0x2d], [0x22, 0xc5, 0x5e]),
];

/// The stand-in's size (the web SVG's).
const PICTURE_SIZE: (u32, u32) = (800, 500);

/// `(x, y, w, h, radius, white alpha)`: the 35% white bar and the 12% white
/// panel the web SVG draws over the gradient.
const PICTURE_SHAPES: [(f32, f32, f32, f32, f32, f32); 2] =
    [(48., 48., 320., 28., 6., 0.35), (48., 100., 704., 352., 12., 0.12)];

/// Whether pixel centre `(px, py)` falls inside a rounded rect.
fn in_rounded_rect(px: f32, py: f32, (x, y, w, h, r, _): (f32, f32, f32, f32, f32, f32)) -> bool {
    if px < x || py < y || px > x + w || py > y + h {
        return false;
    }
    let dx = (x + r - px).max(px - (x + w - r)).max(0.);
    let dy = (y + r - py).max(py - (y + h - r)).max(0.);
    dx * dx + dy * dy <= r * r
}

/// A stand-in picture = the web's SVG: a diagonal gradient `from` → `to`
/// (bounding-box diagonal, top left → bottom right) under the bar + panel.
fn picture_png(from: [u8; 3], to: [u8; 3], w: u32, h: u32) -> Vec<u8> {
    let (sx, sy) = (w as f32 / PICTURE_SIZE.0 as f32, h as f32 / PICTURE_SIZE.1 as f32);
    let image = image::RgbaImage::from_fn(w, h, |x, y| {
        let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
        let t = ((cx / w as f32) + (cy / h as f32)) / 2.;
        let mut rgb: [f32; 3] = std::array::from_fn(|i| from[i] as f32 + (to[i] as f32 - from[i] as f32) * t);
        for shape in PICTURE_SHAPES {
            let (bx, by, bw, bh, br, alpha) = shape;
            if in_rounded_rect(cx / sx, cy / sy, (bx, by, bw, bh, br, alpha)) {
                rgb = rgb.map(|c| c + (255. - c) * alpha);
            }
        }
        let [r, g, b] = rgb.map(|c| c.round().clamp(0., 255.) as u8);
        image::Rgba([r, g, b, 255])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("png encodes");
    bytes
}

fn seed(cache: &mut ImageCache) {
    let (w, h) = PICTURE_SIZE;
    for (id, from, to) in TONES {
        cache.insert_bytes(format!("/api/attachments/{id}"), "image/png", picture_png(from, to, w, h));
    }
}

fn caption(text: &str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(text.to_string()))
}

fn demo(label: &str, row: &SessionThreadRow, images: &Entity<ImageCache>, window: &mut Window, cx: &mut App) -> Div {
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
    let specimen = &domain::plan_card::copy().specimen;
    let label = |row: &SessionThreadRow| match row {
        SessionThreadRow::Plan { decision: None, .. } => specimen.pending.as_str(),
        SessionThreadRow::Plan { .. } => specimen.approved.as_str(),
        _ => specimen.answered.as_str(),
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

    /// The stand-ins = the web SVG: gradient stops at the corners, the 35%
    /// bar and the 12% panel over it.
    #[test]
    fn stand_in_pictures_are_the_web_palette() {
        let (w, h) = PICTURE_SIZE;
        let (_, from, to) = TONES[1];
        let png = picture_png(from, to, w, h);
        assert_eq!(&png[1..4], b"PNG");
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        let near = |a: [u8; 4], b: [u8; 3]| a.iter().zip(b).all(|(a, b)| (*a as i16 - b as i16).abs() <= 2);
        assert!(near(image.get_pixel(0, 0).0, from), "top left = from");
        assert!(near(image.get_pixel(w - 1, h - 1).0, to), "bottom right = to");
        let expected = |x: u32, y: u32, alpha: f32| -> [u8; 3] {
            let t = ((x as f32 + 0.5) / w as f32 + (y as f32 + 0.5) / h as f32) / 2.;
            std::array::from_fn(|i| {
                let c = from[i] as f32 + (to[i] as f32 - from[i] as f32) * t;
                (c + (255. - c) * alpha).round() as u8
            })
        };
        assert!(near(image.get_pixel(60, 60).0, expected(60, 60, 0.35)), "the 35% bar");
        assert!(near(image.get_pixel(400, 300).0, expected(400, 300, 0.12)), "the 12% panel");
        assert!(near(image.get_pixel(400, 480).0, expected(400, 480, 0.)), "under the panel");
        // The rounded corners stay gradient.
        assert!(!in_rounded_rect(48.5, 48.5, PICTURE_SHAPES[0]));
        assert!(in_rounded_rect(60.5, 60.5, PICTURE_SHAPES[0]));
        assert_eq!(TONES.map(|(id, ..)| id), ["att-1", "att-2", "att-3"]);
    }
}
