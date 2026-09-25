//! EXP-1037 — the launcher composer in a DIALOG (the desktop half of the ×4
//! rule).
//!
//! Every play button used to NAVIGATE to the Agent screen with a
//! [`ChatSeed`], where the subject showed up as a small badge inside the
//! composer card — so people did not realise a second press was still needed.
//! A seed that carries a SUBJECT now opens the SAME composer, prefilled, in
//! its own window: the headline says what is about to run and the send is the
//! only thing left to do. A subject-less start (the Chat button, the rail's
//! Agent entry) still opens the Agent screen.
//!
//! This file is only the WINDOW HOST, exactly like
//! [`crate::create_issue_dialog`] is for [`crate::issue_composer`]: the
//! composer, its pickers, its launch options and its start paths are
//! [`ChatScreenView`] in its `Dialog` presentation — one composer, two
//! presentations, no second implementation.

use gpui::{px, size, App, AppContext as _, Window};

use crate::chat_screen::{self, ChatScreenView};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::navigation::{self, ChatSeed};

/// The dialog's content width — the Agent page's own composer column, so the
/// card is the same width in both presentations.
const DIALOG_W: f32 = 640.;
/// Its opening height: the title bar, the headline, the card, the options row
/// and one line of room for a note — measured against what the launcher
/// actually draws, because a window that opens at twice its content reads as
/// a half-loaded dialog. Anything taller (an action's typed input rows, a
/// blocked-start question, an image strip) scrolls or is dragged bigger: the
/// window is resizable and its body self-scrolls.
const DIALOG_H: f32 = 258.;
/// The resize floor (the card alone still fits).
const DIALOG_MIN_H: f32 = 240.;

/// Open the composer prefilled with `seed`. Called from the ONE funnel
/// (`navigation::navigate_to_chat_inner`), never from the play buttons
/// themselves.
pub(crate) fn open(window: &mut Window, cx: &mut App, seed: ChatSeed) {
    // The dialog is its own window, so it gets its own `Navigation` — seed
    // its team from the opener's, or the composer would resolve the last
    // PERSISTED team instead of the one the ▶ was pressed in.
    let opener_id = window.window_handle().window_id();
    let max_height = (window.viewport_size().height * 0.85).min(px(DIALOG_H));
    let min_height = px(DIALOG_MIN_H).min(max_height);
    let spec = DialogSpec::new(
        domain::contract::COMPOSER_UI_DIALOG_TITLE,
        size(px(DIALOG_W), max_height),
    )
    .resizable(size(px(520.), min_height));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        navigation::seed_window_team(window, cx, opener_id);
        let view = cx.new(|cx| ChatScreenView::dialog(seed, window, cx));
        // EXP-862: a pinned action row lights up while its run is composed —
        // the seed no longer passes through the nav, so the open dialog is
        // the answer (`chat_screen::dialog_action_id`).
        chat_screen::register_open_dialog(&view, cx);
        let busy = view.clone();
        DialogContent::new(view)
            // The composer owns its own scrolling: the options row and the
            // blocker note must stay reachable while the card grows.
            .self_scrolling()
            // A start is in flight (images uploading, `coding::prepare`
            // running): closing the window would drop the view the launch
            // reports back to. `after_started` closes it on success.
            .can_close(move |cx| !busy.read(cx).starting())
    });
}
