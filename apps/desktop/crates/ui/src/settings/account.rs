//! Account screen: profile identity + Notifications (masterplan v5 §8.9, L25).
//!
//! Web parity: `routes/_authenticated/account/notifications.tsx` (identity
//! header + email prefs — see [`super::notifications_prefs`]). The old
//! Integrations pane is gone (L25): GitHub App install/manage lives solely in
//! **team settings → Repositories**, and there is no calendar UI anywhere on
//! the desktop. EXP-311: the rail's account button shows only the avatar +
//! first name, so the full name + email live HERE. EXP-369 adds the account's
//! timezone row — the clock the daily digest's send hour is read in.

use gpui::{
    div, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{button::Button, h_flex, v_flex, ActiveTheme as _, Sizable as _};

use super::notifications_prefs::NotificationsPrefsPane;
use super::spawn_trpc;
use crate::queries;

/// The account's stored IANA timezone (`users.timezone` — a server-only
/// column, so it comes over tRPC, not the users shape).
enum Timezone {
    Idle,
    Loading,
    /// `None` = never captured; the digest sweep reads UTC for those.
    Ready(Option<String>),
    Error,
}

/// The account screen (`Screen::Account`) — identity + notifications.
pub struct AccountView {
    notifications: Entity<NotificationsPrefsPane>,
    timezone: Timezone,
    timezone_generation: u64,
    /// The account the timezone belongs to — a re-login must not show the
    /// previous account's value (same guard as the prefs pane).
    account_id: Option<String>,
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
            timezone: Timezone::Idle,
            timezone_generation: 0,
            account_id: None,
            _subscriptions: subscriptions,
        }
    }

    /// EXP-369: this view is built once per window and outlives every visit,
    /// so both server-only reads it owns are dropped whenever the screen is
    /// (re-)opened — see [`crate::screens::ScreensPanel`].
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.timezone, Timezone::Ready(_) | Timezone::Error) {
            self.timezone = Timezone::Idle;
        }
        self.notifications
            .update(cx, |prefs, cx| prefs.mark_stale(cx));
        cx.notify();
    }

    fn ensure_timezone_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.timezone = Timezone::Idle;
        }
        if !matches!(self.timezone, Timezone::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.timezone = Timezone::Loading;
        self.timezone_generation += 1;
        let generation = self.timezone_generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::users::users_timezone(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.timezone_generation != generation {
                    return;
                }
                this.timezone = match result {
                    Ok(timezone) => Timezone::Ready(timezone),
                    Err(err) => {
                        log::warn!("[ui] users.timezone failed: {err}");
                        Timezone::Error
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// The OS zone, written explicitly (not `onlyIfUnset` — this button is the
    /// user asking for it), with the shown value updated optimistically.
    fn use_system_timezone(&mut self, cx: &mut gpui::Context<Self>) {
        let Ok(timezone) = iana_time_zone::get_timezone() else {
            log::warn!("[ui] could not read the system timezone");
            return;
        };
        self.timezone = Timezone::Ready(Some(timezone.clone()));
        cx.notify();
        spawn_trpc(cx, "users.setTimezone", move |trpc| {
            api::users::users_set_timezone(trpc, &timezone, false)
        });
    }

    /// The clock the daily digest's send hour is read in. gpui has no
    /// searchable 400-entry combobox, so the desktop offers the device's own
    /// zone instead of a picker (web parity is the full `Select`).
    fn render_timezone(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let value: SharedString = match &self.timezone {
            Timezone::Ready(Some(timezone)) => timezone.clone().into(),
            Timezone::Ready(None) => "Not set".into(),
            Timezone::Error => "Unavailable".into(),
            Timezone::Idle | Timezone::Loading => "Loading…".into(),
        };
        // Anything but a stored zone is a placeholder, not a value.
        let value_color = match &self.timezone {
            Timezone::Ready(Some(_)) => cx.theme().foreground,
            _ => cx.theme().muted_foreground,
        };
        super::pref_row(
            div().text_sm().child("Timezone"),
            "Used to schedule your daily digest email.",
            h_flex()
                .gap_2()
                .items_center()
                .child(div().text_sm().text_color(value_color).child(value))
                .child(
                    Button::new("use-system-timezone")
                        .outline()
                        .small()
                        .label("Use system timezone")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.use_system_timezone(cx);
                        })),
                ),
            true,
            cx,
        )
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
        self.ensure_timezone_loaded(cx);

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
                    .child(super::section(cx).child(self.render_timezone(cx)))
                    .child(self.notifications.clone()),
            )
    }
}
