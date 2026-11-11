//! `plan-card` (Special components): THE rich plan / question card
//! (EXP-1280, `fixtures/plan-card.json` ×4), the IDE half of the styleguide
//! entry the web page draws. The rows ARE the fixture's: the
//! `session-chat.json` case "EXP-1280: a rich plan card…" built through
//! the domain mirror (`domain::session_tree::ask_card_row`) and painted by
//! the chat's REAL cards (`crate::session_chat::ask_row_card`): #EXP-1662:
//! a plan = its ONE row (pending with its ×, decided, dismissed; the document
//! and the questions are the `plan-page` entry), the answered multi question
//! closed with its picks.
//! The fixture's attachments are local stand-in pictures (no backend).

use domain::rows::MessageRow;
use domain::session_tree::SessionChatRow;
use gpui::{div, px, App, Div, Entity, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};
use serde_json::Value;

use crate::markdown::ImageCache;
use crate::specimen_pictures::picture_png;
use crate::session_chat::ChatText;

pub(crate) const ID: &str = "plan-card";
pub(crate) const OWNER: &str = "EXP-1280";

const SESSION_CHAT_JSON: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/session-chat.json");

/// The fixture case the entry draws.
const CASE_PREFIX: &str = "EXP-1280: a rich plan card";

/// #EXP-1561: the dismissed question + plan case (the web specimen's).
const DISMISSED_PREFIX: &str = "#EXP-1561: a dismissed question";

/// A camelCase fixture row → the snake_case, keyed `MessageRow` (NEW-8):
/// top-level keys snake-cased, a synthesized conversation, the session set
/// after decode.
fn message(value: &Value) -> Option<MessageRow> {
    let snake = |key: &str| {
        key.chars().fold(String::with_capacity(key.len() + 4), |mut out, ch| {
            if ch.is_ascii_uppercase() {
                out.push('_');
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch);
            }
            out
        })
    };
    let session_id = value["sessionId"].as_str().unwrap_or_default().to_string();
    let mut object: serde_json::Map<String, Value> = value
        .as_object()
        .map(|object| object.iter().map(|(key, value)| (snake(key), value.clone())).collect())
        .unwrap_or_default();
    object.entry("id").or_insert_with(|| Value::from(""));
    object.entry("conversation_id").or_insert_with(|| Value::from(format!("c-{session_id}")));
    let message: MessageRow = serde_json::from_value(Value::Object(object)).ok()?;
    Some(MessageRow { session_id, ..message })
}

/// The case's card rows, in Conversation order.
pub(crate) fn rows() -> Vec<SessionChatRow> {
    case_rows(CASE_PREFIX)
}

/// #EXP-1561: the dismissed question and plan, in Conversation order.
pub(crate) fn dismissed_rows() -> Vec<SessionChatRow> {
    case_rows(DISMISSED_PREFIX)
}

fn case_rows(prefix: &str) -> Vec<SessionChatRow> {
    let fixture: Value = serde_json::from_str(SESSION_CHAT_JSON).expect("session-chat.json parses");
    let case = fixture["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|case| case["name"].as_str().is_some_and(|name| name.starts_with(prefix)))
        .expect("the fixture case");
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

fn seed(cache: &mut ImageCache) {
    let (w, h) = PICTURE_SIZE;
    for (id, from, to) in TONES {
        cache.insert_bytes(format!("/api/attachments/{id}"), "image/png", picture_png(from, to, w, h));
    }
}

fn caption(text: &str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(text.to_string()))
}

fn demo(label: &str, rows: &[SessionChatRow], images: &Entity<ImageCache>, window: &mut Window, cx: &mut App) -> Div {
    // #EXP-1561: a waiting card carries its × (the web specimen's
    // `onDismiss`); its decision stays in THE Dock.
    let text = ChatText::default().dismissing(true);
    // #EXP-1662: a plan = ONE row; a dismissed one a line.
    let cards: Vec<_> = rows.iter().filter_map(|row| crate::session_chat::ask_row_card(row, &text, images, window, cx)).collect();
    v_flex().w_full().gap_2().child(caption(label, cx)).child(v_flex().w_full().gap_3().children(cards))
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
    let label = |row: &SessionChatRow| match row {
        SessionChatRow::Plan { decision: None, .. } => specimen.pending.as_str(),
        SessionChatRow::Plan { .. } => specimen.approved.as_str(),
        _ => specimen.answered.as_str(),
    };
    let mut column = v_flex().w(px(640.)).gap_10();
    for row in &rows {
        column = column.child(demo(label(row), std::slice::from_ref(row), &images, window, cx));
    }
    // #EXP-1561 (waiting-cards.json): a dismissed question and plan, one
    // quiet line each.
    column.child(demo(specimen.dismissed.as_str(), &dismissed_rows(), &images, window, cx))
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
        let SessionChatRow::Plan { questions, surface, decision: None, .. } = &rows[0] else {
            panic!("a pending plan first: {:?}", rows[0]);
        };
        assert_eq!(questions.len(), 2);
        assert!(surface.is_some());
        assert_eq!(crate::plan_card::option_rows(&questions[0].options), domain::plan_card::OptionRows::Tiles);
        assert_eq!(crate::plan_card::option_rows(&questions[1].options), domain::plan_card::OptionRows::List);
        let SessionChatRow::Question { multi: true, answered: true, options, .. } = &rows[1] else {
            panic!("an answered multi question: {:?}", rows[1]);
        };
        assert_eq!(options[0].attachment_id.as_deref(), Some("att-3"));
        assert!(matches!(&rows[2], SessionChatRow::Plan { decision: Some(_), .. }));
        let dismissed = dismissed_rows();
        assert!(matches!(&dismissed[0], SessionChatRow::Question { dismissed: Some(true), .. }));
        assert!(matches!(&dismissed[1], SessionChatRow::Plan { decision: Some(d), .. } if d == "dismissed"));
    }

    /// The entry paints twice in a window (the fields and the surface view
    /// are built between the frames) and settles.
    #[gpui::test]
    async fn the_entry_paints_and_settles(cx: &mut gpui::TestAppContext) {
        use gpui::{IntoElement, Render};

        struct Host;

        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Host").entered();
                gpui::div().child(render(window, cx))
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
        assert!(near(image.get_pixel(48, 48).0, expected(48, 48, 0.)), "the bar's rounded corner");
        assert_eq!(TONES.map(|(id, ..)| id), ["att-1", "att-2", "att-3"]);
    }
}
