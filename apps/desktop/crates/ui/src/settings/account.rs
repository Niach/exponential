//! Account screen: Notifications (masterplan v5 §8.9, L25).
//!
//! Web parity: `routes/_authenticated/account/notifications.tsx` (email prefs
//! — see [`super::notifications_prefs`]). The old Integrations pane is gone
//! (L25): GitHub App install/manage lives solely in **team
//! settings → Repositories**, and there is no calendar UI anywhere on the
//! desktop. What remains is a single Notifications screen reached from the
//! sidebar footer account dropdown.

use gpui::{
    div, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::ActiveTheme as _;

use super::notifications_prefs::NotificationsPrefsPane;

/// The account screen (`Screen::Account`) — now Notifications only.
pub struct AccountView {
    notifications: Entity<NotificationsPrefsPane>,
}

impl AccountView {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let notifications = cx.new(NotificationsPrefsPane::new);
        Self { notifications }
    }
}

impl Render for AccountView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let subtitle: SharedString = "Choose which notifications also reach you by email. In-app \
             and push notifications are always on."
            .into();

        // EXP-277: no screen header — the center tab carries the title.
        // EXP-282: the content column was `max_w` WITHOUT `w_full` (the
        // EXP-179 bug — see `issue_detail.rs:66`): taffy then sized it
        // fit-content, so wrapped text overflowed its own box and painted
        // over the sections below. `detail_column()` is the shared settings
        // grid (w_full + cap + padding), so this screen lines up with every
        // settings pane beside it in the nav.
        div()
            .id("account-scroll")
            .size_full()
            .min_w_0()
            .overflow_y_scroll()
            .child(
                super::detail_column()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(subtitle),
                    )
                    .child(self.notifications.clone()),
            )
    }
}
