//! #EXP-1493 THE PLAN PAGE (`fixtures/plan-page.json`; web
//! `packages/ui/src/plan-page.tsx`): the expanded form of THE plan card.

use gpui::{App, Window};

/// Opens THE plan page for `message_id` (a plan card `session_id` posted).
/// Every way in (the story viewer's `Open plan`, the plan card header, the
/// Dock sheet glyph, an inbox row, the Needs-you row) calls this.
pub(crate) fn open(session_id: &str, message_id: &str, window: &mut Window, cx: &mut App) {
    let _ = message_id;
    crate::thread_screen::open_session_card(session_id, window, cx);
}
