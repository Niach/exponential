//! Account screen: profile identity + Notifications (masterplan v5 §8.9, L25).
//!
//! Web parity: `routes/_authenticated/account/notifications.tsx` (identity
//! header + email prefs — see [`super::notifications_prefs`]). The old
//! Integrations pane is gone (L25): GitHub App install/manage lives solely in
//! **team settings → Repositories**, and there is no calendar UI anywhere on
//! the desktop. EXP-311: the rail's account button shows only the avatar +
//! first name, so the full name + email live HERE.

use gpui::{
    div, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use super::notifications_prefs::NotificationsPrefsPane;

/// The account screen (`Screen::Account`) — identity + notifications.
pub struct AccountView {
    notifications: Entity<NotificationsPrefsPane>,
    _subscriptions: Vec<Subscription>,
}

impl AccountView {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let notifications = cx.new(NotificationsPrefsPane::new);
        let users = sync::Store::global(cx).collections().users.clone();
        let avatar_cache = crate::user_avatar::AvatarCache::global(cx);
        let subscriptions = vec![
            // The identity header rides the users shape (profile image URL)
            // plus the async avatar-byte cache (EXP-311).
            cx.observe(&users, |_, _, cx| cx.notify()),
            cx.observe(&avatar_cache, |_, _, cx| cx.notify()),
        ];
        Self {
            notifications,
            _subscriptions: subscriptions,
        }
    }

    /// Avatar + full name + email — the one place the full identity shows
    /// (the rail's chrome is first-name-only, EXP-311).
    fn render_identity(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let account = crate::queries::active_account(cx);
        let name = account
            .as_ref()
            .and_then(|account| account.name.clone())
            .filter(|name| !name.trim().is_empty());
        let email = account.as_ref().map(|account| account.email.clone());
        let full_name: SharedString = name
            .clone()
            .or_else(|| email.clone())
            .map(SharedString::from)
            .unwrap_or_else(|| "Not signed in".into());
        // The email sub-line is dropped when it already IS the title
        // (name-less Apple sign-in accounts).
        let sub_email = name
            .is_some()
            .then(|| email.clone())
            .flatten()
            .map(SharedString::from);
        let image_url = crate::queries::active_user(cx).and_then(|user| user.image);

        h_flex()
            .w_full()
            .gap_3()
            .items_center()
            .child(crate::user_avatar::user_avatar(
                &full_name,
                image_url.as_deref(),
                gpui_component::Size::Medium,
                cx,
            ))
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(full_name.clone()),
                    )
                    .children(sub_email.map(|email| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(email)
                    })),
            )
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
                    .child(self.render_identity(cx))
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
