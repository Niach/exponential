//! Thread 58cb32df (#EXP-1373): `chat-list` (Special components): THE phone
//! Chats screen's pieces (`fixtures/chats.json` LIST + BUBBLES). The IDE has
//! no phone list (its Chats page keeps the bands), so this twin paints the
//! web entry's specimen as a phone-wide STATIC picture of the same look, fed
//! the fixture's own expected model (`listCases[0]`, `bubbleCases[0]`): the
//! large title with the search and inbox circles (the sky dot), the agent
//! bubbles (66px, a 3px ring around a background gap: amber = needs you,
//! white = unread, a hairline = read), the filter chips (select Pills, a
//! count after a non-zero one), then the flat rows: the 52px lead disc, the
//! title + time, the two-line muted preview, the row word and the unread
//! badge, a hairline under the text column only.

use gpui::{div, px, AnyElement, App, Div, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use serde::Deserialize;

use domain::chats::CHATS;

use crate::icons::registry;
use crate::surface::{glass_pill, PillMode, PillSize};

pub(crate) const ID: &str = "chat-list";
pub(crate) const OWNER: &str = "EXP-1373";

/// The phone column (the web island's 390px).
const PHONE_W: f32 = 390.;

const CHATS_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/chats.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    kind: String,
    text: String,
}

/// One row of `listCases[0].expected` (the parts the row paints).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    session_id: String,
    title: String,
    lead: String,
    preview: Preview,
    time: String,
    unread_count: usize,
    needs_you: bool,
    word: Option<String>,
}

#[derive(Deserialize)]
struct Counts {
    needs_you: usize,
    running: usize,
    unread: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bubble {
    agent_id: String,
    name: String,
    icon: Option<String>,
    color: Option<String>,
    ring: Option<String>,
}

fn model() -> (Vec<Row>, Counts, Vec<Bubble>) {
    let fixture: serde_json::Value = serde_json::from_str(CHATS_JSON).expect("chats.json parses");
    let list = &fixture["listCases"][0]["expected"];
    let rows = serde_json::from_value(list["rows"].clone()).expect("list rows");
    let counts = serde_json::from_value(list["counts"].clone()).expect("list counts");
    let bubbles = serde_json::from_value(fixture["bubbleCases"][0]["expected"].clone()).expect("bubbles");
    (rows, counts, bubbles)
}

/// A row word by its fixture key.
fn word_of(key: &str) -> Option<domain::session_tree::ThreadRowWord> {
    use domain::session_tree::ThreadRowWord as W;
    [W::Stack, W::ReadyToMerge, W::Merged, W::Question].into_iter().find(|word| word.key() == key)
}

/// A chip's label (`chatFilterLabel`): its copy, then a non-zero count.
fn filter_label(label: &str, count: Option<usize>) -> String {
    match count {
        Some(n) if n > 0 => format!("{label} {n}"),
        _ => label.to_string(),
    }
}

fn circle(icon: crate::icons::ExpIcon, dot: bool, cx: &App) -> AnyElement {
    div()
        .relative()
        .size(px(40.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .child(Icon::new(icon).with_size(px(18.)).text_color(cx.theme().foreground))
        .when_dot(dot)
        .into_any_element()
}

trait DotExt {
    fn when_dot(self, dot: bool) -> Self;
}

impl DotExt for Div {
    /// The inbox circle's sky dot.
    fn when_dot(self, dot: bool) -> Self {
        if !dot {
            return self;
        }
        self.child(
            div()
                .absolute()
                .top(px(6.))
                .right(px(6.))
                .size(px(8.))
                .rounded_full()
                .bg(theme::tokens::BLUE.to_hsla()),
        )
    }
}

fn header(cx: &App) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .text_3xl()
                .font_weight(FontWeight::BOLD)
                .text_color(cx.theme().foreground)
                .child(SharedString::from(CHATS.copy.title.clone())),
        )
        .child(circle(registry::NAV_SEARCH, false, cx))
        .child(circle(registry::NAV_INBOX, true, cx))
        .into_any_element()
}

fn bubble(bubble: &Bubble, cx: &App) -> AnyElement {
    let shell = &CHATS.shell;
    let ring = match bubble.ring.as_deref() {
        Some("needs_you") => theme::tokens::YELLOW.to_hsla(),
        Some("unread") => cx.theme().foreground,
        _ => theme::tokens::glass::STROKE_CARD.to_hsla(),
    };
    let inner = shell.bubble - 2. * shell.bubble_ring;
    v_flex()
        .w(px(shell.bubble + 8.))
        .items_center()
        .gap_1()
        .child(
            div()
                .size(px(shell.bubble))
                .rounded_full()
                .border(px(shell.bubble_ring))
                .border_color(ring)
                .p(px(shell.bubble_ring))
                .child(
                    div()
                        .size(px(inner - 2. * shell.bubble_ring))
                        .rounded_full()
                        .bg(cx.theme().muted)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            crate::icons::agent_icon(bubble.icon.as_deref(), bubble.color.as_deref())
                                .with_size(px(26.)),
                        ),
                ),
        )
        .child(
            div()
                .max_w_full()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(bubble.name.clone())),
        )
        .into_any_element()
}

fn lead(row: &Row) -> AnyElement {
    let glyph = |icon: crate::icons::ExpIcon, tint: theme::Srgb8| {
        Icon::new(icon).with_size(px(22.)).text_color(tint.to_hsla()).into_any_element()
    };
    match row.lead.as_str() {
        "needs_you" => crate::thread_screen::with_needs_you_dot(
            crate::coding_selects::agent_mark(coding::CodingAgent::default()).with_size(px(22.)),
            true,
            9.,
        ),
        "working" => crate::coding_selects::agent_working_mark(Some(coding::CodingAgent::default()), 22.).into_any_element(),
        "open" => glyph(registry::PR_OPEN, theme::tokens::GREEN),
        "merged" => glyph(registry::PR_MERGED, crate::pr_rows::PR_MERGED_TINT),
        _ => crate::coding_selects::agent_mark(coding::CodingAgent::default()).with_size(px(22.)).into_any_element(),
    }
}

fn row(row: &Row, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let unread = row.unread_count > 0;
    let time_color = if row.needs_you {
        theme::tokens::YELLOW.to_hsla()
    } else if unread {
        theme.foreground
    } else {
        theme.muted_foreground
    };
    let word = row
        .word
        .as_deref()
        .and_then(word_of)
        .and_then(|word| word.text());
    let badge = unread.then(|| {
        let (fill, text) = if row.needs_you {
            (theme::tokens::YELLOW.to_hsla(), gpui::black())
        } else {
            (theme.foreground, theme.background)
        };
        div()
            .min_w(px(20.))
            .h(px(20.))
            .px_1p5()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(fill)
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(text)
            .child(SharedString::from(row.unread_count.to_string()))
    });
    let preview = div()
        .flex_1()
        .min_w_0()
        .line_clamp(2)
        .text_sm()
        .text_color(theme.muted_foreground)
        .when_italic(row.preview.kind == "working")
        .child(SharedString::from(row.preview.text.clone()));
    h_flex()
        .w_full()
        .gap_3()
        .items_center()
        .child(
            div()
                .size(px(CHATS.shell.lead_disc))
                .flex_shrink_0()
                .rounded_full()
                .bg(theme.muted)
                .flex()
                .items_center()
                .justify_center()
                .child(lead(row)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .py_3()
                .gap_0p5()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_base()
                                .font_weight(if unread { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                                .text_color(theme.foreground)
                                .child(SharedString::from(row.title.clone())),
                        )
                        .child(div().text_xs().text_color(time_color).child(SharedString::from(row.time.clone()))),
                )
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_start()
                        .child(preview)
                        .children(word.map(|text| div().text_xs().text_color(theme.muted_foreground).child(text)))
                        .children(badge),
                ),
        )
        .into_any_element()
}

trait ItalicExt {
    fn when_italic(self, italic: bool) -> Self;
}

impl ItalicExt for Div {
    fn when_italic(self, italic: bool) -> Self {
        if italic { self.italic() } else { self }
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let (rows, counts, bubbles) = model();
    let copy = &CHATS.copy.filters;
    let chips = [
        (filter_label(&copy.all, None), true),
        (filter_label(&copy.needs_you, Some(counts.needs_you)), false),
        (filter_label(&copy.running, Some(counts.running)), false),
        (filter_label(&copy.unread, Some(counts.unread)), false),
    ];
    let chips = h_flex().gap_2().children(chips.into_iter().enumerate().map(|(index, (label, selected))| {
        glass_pill(("sg-chat-filter", index), PillSize::Md, PillMode::Select { selected }, cx)
            .child(div().text_sm().child(SharedString::from(label)))
    }));
    v_flex()
        .w(px(PHONE_W))
        .p_4()
        .gap_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(header(cx))
        .child(h_flex().gap_2().children(bubbles.iter().map(|b| {
            let _ = &b.agent_id;
            bubble(b, cx)
        })))
        .child(chips)
        .child(v_flex().w_full().children(rows.iter().map(|r| {
            let _ = &r.session_id;
            row(r, cx)
        })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen reads the fixture's expected model.
    #[test]
    fn the_specimen_is_the_fixture_model() {
        let (rows, counts, bubbles) = model();
        assert_eq!(rows.len(), 4);
        assert_eq!(counts.needs_you, 1);
        assert_eq!(bubbles.len(), 3);
        assert_eq!(filter_label("Needs you", Some(1)), "Needs you 1");
        assert_eq!(filter_label("Unread", Some(0)), "Unread");
        assert!(rows.iter().all(|row| row.word.as_deref().is_none_or(|word| word_of(word).is_some())));
    }
}
