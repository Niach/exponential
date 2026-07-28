//! Account → Notifications: email-notification prefs (masterplan-v3 §4.2).
//!
//! Web parity: `routes/_authenticated/account/notifications.tsx` — a master
//! email `Switch`, the seven per-type rows (labels + hints verbatim), and
//! the delivery cadence select. Email is PUSH-FIRST (EXP-69 parity with the
//! web's digest system): nothing is emailed per-event — notifications still
//! unread ~1h after the push are bundled into one digest email, so the
//! server's `digestValues` are `off` = Hourly digest / `daily` = Daily
//! digest (`lib/notification-email-policy.ts`). `user_notification_prefs` is
//! server-only — read + written via `notifications.emailPrefs` /
//! `updateEmailPrefs`, never synced.
//!
//! Update semantics mirror the web exactly: optimistic local state + a
//! fire-and-forget mutation carrying only the changed field; when the email
//! transport is unconfigured (self-host without Resend/SMTP) every control
//! disables and the explanatory banner shows.

use std::collections::HashMap;

use gpui::{
    div, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    skeleton::Skeleton,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};

use api::notifications::{EmailPrefs, UpdateEmailPrefsInput};
use domain::contract::{
    NOTIFICATION_TYPE_ISSUE_ASSIGNED, NOTIFICATION_TYPE_ISSUE_COMMENT,
    NOTIFICATION_TYPE_ISSUE_CREATED, NOTIFICATION_TYPE_ISSUE_MENTION,
    NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED, NOTIFICATION_TYPE_PR_MERGED,
    NOTIFICATION_TYPE_PR_OPENED,
};

use crate::queries;

use super::{section, error_notice, spawn_trpc};
use crate::icons::registry;

/// Web `TYPE_ROWS` — verbatim labels + hints, contract-locked type values.
const TYPE_ROWS: [(&str, &str, &str); 7] = [
    (
        NOTIFICATION_TYPE_ISSUE_CREATED,
        "New feedback",
        "A new issue is filed in your team via the feedback widget.",
    ),
    (
        NOTIFICATION_TYPE_ISSUE_ASSIGNED,
        "Assigned to you",
        "Someone assigns an issue to you.",
    ),
    (
        NOTIFICATION_TYPE_ISSUE_COMMENT,
        "Comments",
        "New comments on issues you're subscribed to.",
    ),
    (
        NOTIFICATION_TYPE_ISSUE_MENTION,
        "Mentions",
        "Someone @mentions you in a description or comment.",
    ),
    (
        NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED,
        "Status changes",
        "An issue you're subscribed to changes status.",
    ),
    (
        NOTIFICATION_TYPE_PR_OPENED,
        "Pull request opened",
        "A PR is opened for an issue you follow.",
    ),
    (
        NOTIFICATION_TYPE_PR_MERGED,
        "Pull request merged",
        "A PR for an issue you follow is merged.",
    ),
];

/// Server `digestValues` (`lib/notification-email-policy.ts`): `off` is the
/// default HOURLY digest of still-unread notifications, `daily` batches at
/// most one digest email per day. There is no per-event email anymore.
const DIGEST_OFF: &str = "off";
const DIGEST_DAILY: &str = "daily";

enum Load {
    Idle,
    Loading,
    Ready(EmailPrefs),
    Error(String),
}

pub struct NotificationsPrefsPane {
    load: Load,
    generation: u64,
    /// The account the loaded prefs belong to — a re-login must not show
    /// (or write!) the previous account's preferences.
    account_id: Option<String>,
}

impl NotificationsPrefsPane {
    pub fn new(_cx: &mut gpui::Context<Self>) -> Self {
        Self {
            load: Load::Idle,
            generation: 0,
            account_id: None,
        }
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.load = Load::Idle;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::notifications::notifications_email_prefs(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = match result {
                    Ok(prefs) => Load::Ready(prefs),
                    Err(err) => Load::Error(err.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Optimistic local update + a fire-and-forget mutation carrying only
    /// the changed field (the web handlers' shape).
    fn apply(
        &mut self,
        mutate: impl FnOnce(&mut EmailPrefs) -> UpdateEmailPrefsInput,
        cx: &mut gpui::Context<Self>,
    ) {
        let Load::Ready(prefs) = &mut self.load else {
            return;
        };
        let input = mutate(prefs);
        cx.notify();
        spawn_trpc(cx, "notifications.updateEmailPrefs", move |trpc| {
            api::notifications::notifications_update_email_prefs(trpc, &input)
        });
    }

    fn set_email_enabled(&mut self, enabled: bool, cx: &mut gpui::Context<Self>) {
        self.apply(
            |prefs| {
                prefs.email_enabled = enabled;
                UpdateEmailPrefsInput {
                    email_enabled: Some(enabled),
                    ..Default::default()
                }
            },
            cx,
        );
    }

    fn toggle_type(&mut self, kind: &'static str, next: bool, cx: &mut gpui::Context<Self>) {
        self.apply(
            |prefs| {
                prefs.type_prefs.insert(kind.to_string(), next);
                let merged: HashMap<String, bool> = prefs.type_prefs.clone();
                UpdateEmailPrefsInput {
                    type_prefs: Some(merged),
                    ..Default::default()
                }
            },
            cx,
        );
    }

    fn set_digest(&mut self, digest: &'static str, cx: &mut gpui::Context<Self>) {
        self.apply(
            |prefs| {
                prefs.digest = Some(digest.to_string());
                UpdateEmailPrefsInput {
                    digest: Some(digest.to_string()),
                    ..Default::default()
                }
            },
            cx,
        );
    }
}

impl Render for NotificationsPrefsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);

        let (transport, email_enabled, have_prefs) = match &self.load {
            Load::Ready(prefs) => (prefs.transport_configured, prefs.email_enabled, true),
            _ => (false, false, false),
        };

        // EXP-282: section header — title/description + the master switch.
        // EXP-285: the shared `pref_row` shape — capped hint measure,
        // vertically centered control, hairline rhythm.
        let mut body = section(cx).child(super::pref_row(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Email notifications"),
            "Email is the catch-up channel: notifications still unread an hour \
             after the push are bundled into one digest email, with deep links \
             straight to each issue.",
            Switch::new("email-enabled")
                .checked(email_enabled)
                .disabled(!transport || !have_prefs)
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    this.set_email_enabled(*checked, cx);
                })),
            true,
            cx,
        ));

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_4().w_full())
                        .child(Skeleton::new().h_4().w_full())
                        .child(Skeleton::new().h_4().w_64()),
                );
            }
            Load::Error(message) => {
                body = body
                    .child(error_notice(
                        SharedString::from(format!(
                            "Couldn't load notification preferences: {message}"
                        )),
                        cx,
                    ))
                    .child(
                        h_flex().child(
                            Button::new("prefs-retry")
                                .outline()
                                .xsmall()
                                .label("Retry")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.load = Load::Idle;
                                    cx.notify();
                                })),
                        ),
                    );
            }
            Load::Ready(prefs) => {
                if !transport {
                    body = body.child(
                        div()
                            .px_3()
                            .py_2()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(super::row_stroke(cx))
                            // EXP-282: glass section fill, not the opaque
                            // `theme.muted` panel.
                            .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Email sending is not configured on this server. Set \
                                 RESEND_API_KEY or SMTP_HOST to enable it.",
                            ),
                    );
                }

                let controls_disabled = !transport || !prefs.email_enabled;

                // EXP-285: the hairlines between the `pref_row`s carry the
                // rhythm — no extra gap inside the stack.
                let mut rows = v_flex();
                for (kind, label, hint) in TYPE_ROWS {
                    // Web: `typePrefs[type] !== false` — missing means ON.
                    let checked = prefs.type_prefs.get(kind).copied() != Some(false);
                    rows = rows.child(super::pref_row(
                        div().text_sm().child(label),
                        hint,
                        Switch::new(SharedString::from(format!("type-{kind}")))
                            .checked(checked)
                            .disabled(controls_disabled)
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                this.toggle_type(kind, *checked, cx);
                            })),
                        false,
                        cx,
                    ));
                }
                body = body.child(rows);

                let digest = prefs.digest.clone().unwrap_or_else(|| DIGEST_OFF.to_string());
                let digest_label: SharedString = if digest == DIGEST_DAILY {
                    "Daily digest".into()
                } else {
                    "Hourly digest".into()
                };
                body = body.child(super::pref_row(
                    div().text_sm().child("Delivery"),
                    "How often unread notifications are bundled into one email.",
                    Button::new("digest-select")
                        .outline()
                        .small()
                        .label(digest_label)
                        .icon(registry::UI_CHEVRON_DOWN)
                        .disabled(controls_disabled)
                        .dropdown_menu({
                            let entity = cx.entity();
                            let current = digest.clone();
                            move |mut menu, _, _| {
                                for (value, label) in [
                                    (DIGEST_OFF, "Hourly digest"),
                                    (DIGEST_DAILY, "Daily digest"),
                                ] {
                                    let entity = entity.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(label)
                                            .checked(current == value)
                                            .on_click(move |_, _, cx| {
                                                entity.update(cx, |this, cx| {
                                                    this.set_digest(value, cx);
                                                });
                                            }),
                                    );
                                }
                                menu
                            }
                        }),
                    false,
                    cx,
                ));
            }
        }

        v_flex().child(body)
    }
}
