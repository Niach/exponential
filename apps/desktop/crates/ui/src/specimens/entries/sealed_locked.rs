//! EXP-1459: `sealed-locked` (Special components): THE locked state for
//! sealed session content this install cannot open (yet)
//! (`fixtures/sealed-content.json` `placeholder`), the twin of web
//! `sealed-locked.tsx` (`SealedLockedSpecimen`): the IDE transcript (the
//! person's bubble, the agent's prose) with the divider pill with the lock
//! concept and the reason's copy (`noKey`, `unreadable`), no time, then the
//! first three chat rows with the locked previews italic muted (#EXP-1469).
//! The chat screen paints the same pill (`surface_card::locked_pill`).
//! #EXP-1553: like web, the phone's BUBBLES layout first (the agent's reply
//! in its left bubble with its time), then the IDE transcript.
//! #EXP-1633: laid out like web's — a wrapping row of the phone card, the
//! wide transcript and the chats list card, not one column.

use domain::chats::{ChatPreview, ChatPreviewKind, ChatRow};
use domain::sealed::{LockedReason, PLACEHOLDER};
use domain::session_tree::SessionChatRow;
use gpui::{div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::chat_list::{chat_lead_disc, chat_row};
use crate::markdown::ImageCache;

use super::chat_transcript::{message, paint};

pub(crate) const ID: &str = "sealed-locked";
pub(crate) const OWNER: &str = "EXP-1459";

/// The web entry's `LOCKED_ROWS` with their times.
fn transcript() -> Vec<(SessionChatRow, &'static str)> {
    let locked = |id: &str, reason, author: &str| SessionChatRow::Locked { id: id.into(), reason, author: author.into() };
    vec![
        (message("l-m1", "user", "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it."), "21:02"),
        (locked("l-k1", LockedReason::NoKey, "agent"), ""),
        (message("l-m2", "agent", "Deferred the shape subscribe until after the first frame."), "21:14"),
        (locked("l-k2", LockedReason::Unreadable, "user"), ""),
    ]
}

/// The web entry's `LOCKED_LIST_ROWS`: the fixture list's first three rows,
/// the first and the third with a locked preview.
fn list_rows() -> Vec<ChatRow> {
    let (list, _) = super::chat_list::model();
    let locked = |reason| ChatPreview { kind: ChatPreviewKind::Locked, text: PLACEHOLDER.text(reason).to_string() };
    list.rows
        .into_iter()
        .take(3)
        .enumerate()
        .map(|(index, row)| match index {
            0 => ChatRow { preview: locked(LockedReason::NoKey), ..row },
            2 => ChatRow { preview: locked(LockedReason::Unreadable), ..row },
            _ => row,
        })
        .collect()
}

/// A caption over a layout (the web entry's `text-xs` muted labels).
fn label(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-sealed-locked-images", cx, |_, _| ImageCache::new(None)).clone();
    // #EXP-1736: ONE transcript, ONE bubble: the phone card paints the
    // same rows as the transcript.
    let bubbles: Vec<AnyElement> = transcript()
        .iter()
        .map(|(row, time)| paint(row, time, &images, window, cx))
        .collect();
    let rows: Vec<AnyElement> = transcript().iter().map(|(row, time)| paint(row, time, &images, window, cx)).collect();
    // Web: `flex flex-wrap items-start gap-6`; the phone card `w-[22rem]
    // rounded-3xl border bg-background p-4`, the transcript `w-[32rem]`,
    // the list card `w-[390px] rounded-xl bg-background p-4 gap-4`.
    h_flex()
        .flex_wrap()
        .items_start()
        .gap_6()
        .child(
            v_flex()
                .w(px(352.))
                .rounded_3xl()
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(cx.theme().background)
                .p_4()
                .child(label("Phone (bubbles)", cx).mb_3())
                .child(v_flex().gap_5().children(bubbles)),
        )
        .child(v_flex().w(px(512.)).child(label("Wide (transcript)", cx).mb_3()).child(v_flex().gap_5().children(rows)))
        .child(
            v_flex()
                .w(px(390.))
                .gap_4()
                .p_4()
                .rounded_xl()
                .bg(cx.theme().background)
                .child(label("Chats list", cx))
                .child(v_flex().w_full().children(list_rows().iter().map(|row| chat_row(row, chat_lead_disc(row.lead, None, false, cx), cx).into_any_element()))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::registry;

    #[test]
    fn the_fixture_names_the_icon_and_both_labels() {
        assert!(registry::concept_by_name(&PLACEHOLDER.icon).is_some(), "{} is a concept", PLACEHOLDER.icon);
        assert_eq!(PLACEHOLDER.text(LockedReason::NoKey), "Waiting for the key");
        assert_eq!(PLACEHOLDER.text(LockedReason::Unreadable), "Can't be opened here");
    }

    /// #EXP-1469: the web entry's rows: both pills, three chat rows, the
    /// first and the third locked.
    #[test]
    fn the_specimen_is_the_web_entry() {
        let pills = transcript().iter().filter(|(row, _)| matches!(row, SessionChatRow::Locked { .. })).count();
        assert_eq!(pills, 2);
        let rows = list_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].preview.kind, ChatPreviewKind::Locked);
        assert_ne!(rows[1].preview.kind, ChatPreviewKind::Locked);
        assert_eq!(rows[2].preview.kind, ChatPreviewKind::Locked);
    }
}
