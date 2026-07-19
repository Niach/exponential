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
use gpui_component::{v_flex, ActiveTheme as _};

use super::notifications_prefs::NotificationsPrefsPane;
use super::screen_header;

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

        v_flex()
            .size_full()
            .child(screen_header("Notifications", cx))
            .child(
                div()
                    .id("account-scroll")
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .p_4()
                            .gap_2()
                            .max_w(gpui::px(672.))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(subtitle),
                            )
                            .child(self.notifications.clone()),
                    ),
            )
    }
}
