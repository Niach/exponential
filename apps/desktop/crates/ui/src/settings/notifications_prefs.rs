//! Account → Notifications: email-notification prefs (masterplan-v3 §4.2).
//!
//! Web parity: `routes/_authenticated/account/notifications.tsx` — a master
//! email `Switch`, the six per-type rows (labels + hints verbatim), and the
//! delivery cadence select (`off` = Immediately / `daily` = Daily digest,
//! the server's `digestValues`). `user_notification_prefs` is server-only —
//! read + written via `notifications.emailPrefs` / `updateEmailPrefs`, never
//! synced.
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
    v_flex, ActiveTheme as _, Disableable as _, IconName, Sizable as _,
};

use api::notifications::{EmailPrefs, UpdateEmailPrefsInput};
use domain::contract::{
    NOTIFICATION_TYPE_ISSUE_ASSIGNED, NOTIFICATION_TYPE_ISSUE_COMMENT,
    NOTIFICATION_TYPE_ISSUE_MENTION, NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED,
    NOTIFICATION_TYPE_PR_MERGED, NOTIFICATION_TYPE_PR_OPENED,
};

use crate::queries;

use super::{card, error_notice, spawn_trpc};

/// Web `TYPE_ROWS` — verbatim labels + hints, contract-locked type values.
const TYPE_ROWS: [(&str, &str, &str); 6] = [
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

/// Server `digestValues` (`lib/notification-email-policy.ts`).
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

        // Card header: icon tile + title/description + the master switch.
        let mut body = card(cx).child(
            h_flex()
                .gap_3()
                .items_start()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_10()
                        .flex_shrink_0()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().muted)
                        .child(gpui_component::Icon::new(IconName::Bell)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Email notifications"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Emails deep-link straight to the issue, so nothing gets \
                                     lost while you're away.",
                                ),
                        ),
                )
                .child(
                    Switch::new("email-enabled")
                        .checked(email_enabled)
                        .disabled(!transport || !have_prefs)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.set_email_enabled(*checked, cx);
                        })),
                ),
        );

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
                            .border_color(cx.theme().border)
                            .bg(cx.theme().muted)
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Email sending is not configured on this server. Set \
                                 RESEND_API_KEY or SMTP_HOST to enable it.",
                            ),
                    );
                }

                let controls_disabled = !transport || !prefs.email_enabled;

                let mut rows = v_flex().gap_3();
                for (kind, label, hint) in TYPE_ROWS {
                    // Web: `typePrefs[type] !== false` — missing means ON.
                    let checked = prefs.type_prefs.get(kind).copied() != Some(false);
                    rows = rows.child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_0p5()
                                    .child(div().text_sm().child(label))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(hint),
                                    ),
                            )
                            .child(
                                Switch::new(SharedString::from(format!("type-{kind}")))
                                    .checked(checked)
                                    .disabled(controls_disabled)
                                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                        this.toggle_type(kind, *checked, cx);
                                    })),
                            ),
                    );
                }
                body = body.child(rows);

                let digest = prefs.digest.clone().unwrap_or_else(|| DIGEST_OFF.to_string());
                let digest_label: SharedString = if digest == DIGEST_DAILY {
                    "Daily digest".into()
                } else {
                    "Immediately".into()
                };
                body = body.child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .pt_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            v_flex()
                                .flex_1()
                                .gap_0p5()
                                .child(div().text_sm().child("Delivery"))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            "Send each email immediately, or hold them for a \
                                             daily digest.",
                                        ),
                                ),
                        )
                        .child(
                            Button::new("digest-select")
                                .outline()
                                .small()
                                .label(digest_label)
                                .icon(IconName::ChevronDown)
                                .disabled(controls_disabled)
                                .dropdown_menu({
                                    let entity = cx.entity();
                                    let current = digest.clone();
                                    move |mut menu, _, _| {
                                        for (value, label) in [
                                            (DIGEST_OFF, "Immediately"),
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
                        ),
                );
            }
        }

        v_flex().child(body)
    }
}
