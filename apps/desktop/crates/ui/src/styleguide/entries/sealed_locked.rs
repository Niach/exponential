//! EXP-1459: `sealed-locked` (Special components): THE locked state for
//! sealed session content this install cannot open (yet)
//! (`fixtures/sealed-content.json` `placeholder`), the twin of web
//! `sealed-locked.tsx` (`SealedLockedSpecimen`): the IDE transcript (the
//! person's bubble, the agent's prose) with the divider pill with the lock
//! concept and the reason's copy (`noKey`, `unreadable`), no time, then the
//! first three chat rows with the locked previews italic muted (#EXP-1469).
//! The thread screen paints the same pill (`surface_card::locked_pill`).

use domain::chats::{ChatPreview, ChatPreviewKind, ChatRow};
use domain::sealed::{LockedReason, PLACEHOLDER};
use domain::session_tree::SessionThreadRow;
use gpui::{px, AnyElement, App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::chat_list::{chat_lead_disc, chat_row};
use crate::markdown::ImageCache;

use super::chat_transcript::{message, paint};

pub(crate) const ID: &str = "sealed-locked";
pub(crate) const OWNER: &str = "EXP-1459";

/// The web entry's `LOCKED_ROWS` with their times.
fn transcript() -> Vec<(SessionThreadRow, &'static str)> {
    let locked = |id: &str, reason, author: &str| SessionThreadRow::Locked { id: id.into(), reason, author: author.into() };
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
    let (list, _, _) = super::chat_list::model();
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

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-sealed-locked-images", cx, |_, _| ImageCache::new(None)).clone();
    let rows: Vec<AnyElement> = transcript().iter().map(|(row, time)| paint(row, time, &images, window, cx)).collect();
    v_flex()
        .w(px(520.))
        .gap_5()
        .children(rows)
        .child(
            v_flex()
                .w_full()
                .rounded_xl()
                .bg(cx.theme().background)
                .children(list_rows().iter().map(|row| chat_row(row, chat_lead_disc(row.lead, None, false, cx), cx).into_any_element())),
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
        let pills = transcript().iter().filter(|(row, _)| matches!(row, SessionThreadRow::Locked { .. })).count();
        assert_eq!(pills, 2);
        let rows = list_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].preview.kind, ChatPreviewKind::Locked);
        assert_ne!(rows[1].preview.kind, ChatPreviewKind::Locked);
        assert_eq!(rows[2].preview.kind, ChatPreviewKind::Locked);
    }
}
