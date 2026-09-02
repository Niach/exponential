//! Account → Notifications: email-notification prefs (masterplan-v3 §4.2).
//!
//! Web parity: `routes/_authenticated/account/notifications.tsx` — a master
//! email `Switch`, the eight per-type rows (labels + hints verbatim), the
//! delivery cadence select and (EXP-369) the daily send-time hour.
//! **Desktop-only, no web counterpart:** the "Desktop notifications" row
//! above the email group (EXP-638) — the per-MACHINE switch for real OS
//! notifications, persisted in the local settings.json
//! (`Settings::os_notifications`), never synced; the web card
//! (`components/account/email-notifications-card.tsx`) has nothing to
//! mirror it. The per-type switches below gate it too. Email is
//! the catch-up channel: nothing is emailed per-event — still-unread
//! notifications are bundled into one digest, so the server's `digestValues`
//! are `off` = Hourly digest / `daily` = Daily digest at the account's own
//! `digestHour`, read in its timezone (`lib/notification-email-policy.ts`).
//! `user_notification_prefs` is server-only — read + written via
//! `notifications.emailPrefs` / `updateEmailPrefs`, never synced.
//!
//! Update semantics mirror the web exactly: optimistic local state + a
//! fire-and-forget mutation carrying only the changed field; when the email
//! transport is unconfigured (self-host without SES/SMTP) every control
//! disables and the explanatory banner shows. EXP-369: the per-type switches
//! gate PUSH as well as email, so they stay live with the master email
//! switch off — only the digest controls follow it.

use std::collections::HashMap;

use gpui::{div, px, IntoElement, ParentElement, Render, SharedString, Styled, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    skeleton::Skeleton,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _,
};

use crate::controls::WebControl as _;
use crate::surface::{
    glass_group_rows, glass_picker_row, glass_toggle_row, glass_tray, picker_value_label,
};

use api::notifications::{EmailPrefs, UpdateEmailPrefsInput};
use domain::contract::{
    NOTIFICATION_TYPE_ISSUE_ASSIGNED, NOTIFICATION_TYPE_ISSUE_COMMENT,
    NOTIFICATION_TYPE_ISSUE_CREATED, NOTIFICATION_TYPE_ISSUE_MENTION,
    NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED, NOTIFICATION_TYPE_PR_MERGED,
    NOTIFICATION_TYPE_PR_OPENED, NOTIFICATION_TYPE_SUPPORT_REPLY,
};

use crate::coding_flow::CodingHub;
use crate::queries;

use super::{error_notice, section, spawn_trpc};

/// Web `TYPE_ROWS` — verbatim labels + hints, contract-locked type values.
const TYPE_ROWS: [(&str, &str, &str); 8] = [
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
    (
        NOTIFICATION_TYPE_SUPPORT_REPLY,
        "Support tickets",
        "New helpdesk tickets and reporter replies in your teams.",
    ),
];

/// Server `digestValues` (`lib/notification-email-policy.ts`): `off` is the
/// default HOURLY digest of still-unread notifications, `daily` batches at
/// most one digest email per day. There is no per-event email anymore.
const DIGEST_OFF: &str = "off";
const DIGEST_DAILY: &str = "daily";

/// The server's `digest_hour` column default — what a row written before
/// EXP-369 (or by an older server that omits the field) sends at.
const DEFAULT_DIGEST_HOUR: i64 = 8;

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

    /// EXP-369: drop cached prefs so the next render refetches. The pane is
    /// built once per window and lives as long as it, so without this the
    /// first visit's snapshot would show forever (a change made on the web,
    /// or on another device, never landed). A load already in flight is left
    /// alone — it is about to write the fresh value anyway.
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.load, Load::Ready(_)) {
            self.load = Load::Idle;
            cx.notify();
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

    /// EXP-638: flip the per-machine OS-notification switch (local
    /// settings.json via the coding hub's ui-prefs save — no doctor rerun).
    fn set_os_notifications(&mut self, enabled: bool, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let mut settings = hub.read(cx).settings.clone();
        if settings.os_notifications == enabled {
            return;
        }
        settings.os_notifications = enabled;
        CodingHub::save_ui_prefs(&hub, settings, cx);
        cx.notify();
    }

    fn toggle_type(&mut self, kind: &'static str, next: bool, cx: &mut gpui::Context<Self>) {
        // EXP-638: the OS-notification gate caches these; refetch next time.
        crate::os_notifications::invalidate_type_prefs(cx);
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

    fn set_digest_hour(&mut self, hour: i64, cx: &mut gpui::Context<Self>) {
        self.apply(
            |prefs| {
                prefs.digest_hour = Some(hour);
                UpdateEmailPrefsInput {
                    digest_hour: Some(hour),
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

        // EXP-638: the per-machine OS-notification switch — its own group
        // above the email one (desktop-only; see the module doc).
        // EXP-698: every pref is a row of an inset-grouped stack now — the
        // old flat `pref_row` hairline ladder is gone.
        let os_notifications = CodingHub::global(cx).read(cx).settings.os_notifications;
        let desktop = glass_group_rows(vec![glass_toggle_row(
            "Desktop notifications",
            Some(
                "Show a system notification on this computer when something new lands in \
                 your inbox. This machine only; the per-type switches below apply to it too."
                    .into(),
            ),
            Switch::new("os-notifications")
                .checked(os_notifications)
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    this.set_os_notifications(*checked, cx);
                }))
                .into_any_element(),
            cx,
        )]);

        // The master email switch — its own single-row group, the way the web
        // card's header switch reads.
        let mut body = section(cx).child(glass_group_rows(vec![glass_toggle_row(
            "Email notifications",
            Some("Notifications still unread are bundled into one digest email.".into()),
            Switch::new("email-enabled")
                .checked(email_enabled)
                .disabled(!transport || !have_prefs)
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    this.set_email_enabled(*checked, cx);
                }))
                .into_any_element(),
            cx,
        )]));

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
                                .web_xs()
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
                    // EXP-698: the shared glass tray, not a bespoke bordered box.
                    body = body.child(
                        glass_tray()
                            .w_full()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(div().flex_1().min_w_0().child(
                                "Email sending is not configured on this server. No \
                                 digest emails go out.",
                            )),
                    );
                }

                // EXP-369: the per-type switches gate PUSH too, so the master
                // email switch no longer disables them — only the digest
                // controls (which are email-only) follow it.
                let types_disabled = !transport;
                let digest_disabled = !transport || !prefs.email_enabled;

                // The eight per-type switches fuse into ONE grouped stack;
                // the hairlines between them come from `glass_group_rows`.
                let mut rows = Vec::with_capacity(TYPE_ROWS.len());
                for (kind, label, hint) in TYPE_ROWS {
                    // Web: `typePrefs[type] !== false` — missing means ON.
                    let checked = prefs.type_prefs.get(kind).copied() != Some(false);
                    rows.push(glass_toggle_row(
                        label,
                        Some(hint.into()),
                        Switch::new(SharedString::from(format!("type-{kind}")))
                            .checked(checked)
                            .disabled(types_disabled)
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                this.toggle_type(kind, *checked, cx);
                            }))
                            .into_any_element(),
                        cx,
                    ));
                }
                body = body.child(glass_group_rows(rows));

                let digest = prefs.digest.clone().unwrap_or_else(|| DIGEST_OFF.to_string());
                let digest_label: SharedString = if digest == DIGEST_DAILY {
                    "Daily digest".into()
                } else {
                    "Hourly digest".into()
                };
                let value_color = cx.theme().foreground.opacity(0.7);
                let mut digest_rows = vec![glass_picker_row(
                    "Delivery",
                    Some("How often the digest goes out.".into()),
                    Button::new("digest-select")
                        .ghost()
                        .cursor_pointer()
                        .h_auto()
                        .px_0()
                        .py_0()
                        .text_color(value_color)
                        .dropdown_caret(true)
                        .disabled(digest_disabled)
                        // EXP-697: NOT `.label()` — upstream draws that in a
                        // `flex_none` box that neither shrinks nor truncates.
                        .child(picker_value_label(digest_label))
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
                        })
                        .into_any_element(),
                    cx,
                )];

                // EXP-369: the daily cadence's send time. Full hours only —
                // the server's 10-minute sweep resolves send points at hour
                // resolution, so there is nothing finer to offer.
                if digest == DIGEST_DAILY {
                    let hour = prefs.digest_hour.unwrap_or(DEFAULT_DIGEST_HOUR);
                    digest_rows.push(glass_picker_row(
                        "Send time",
                        Some("Full hours only, in your timezone.".into()),
                        Button::new("digest-hour-select")
                            .ghost()
                            .cursor_pointer()
                            .h_auto()
                            .px_0()
                            .py_0()
                            .text_color(value_color)
                            .dropdown_caret(true)
                            .disabled(digest_disabled)
                            .child(picker_value_label(SharedString::from(format!("{hour}:00"))))
                            .dropdown_menu({
                                let entity = cx.entity();
                                move |mut menu, _, _| {
                                    menu = menu.scrollable(true).max_h(px(320.));
                                    for value in 0..24i64 {
                                        let entity = entity.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(format!("{value}:00"))
                                                .checked(hour == value)
                                                .on_click(move |_, _, cx| {
                                                    entity.update(cx, |this, cx| {
                                                        this.set_digest_hour(value, cx);
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }
                            })
                            .into_any_element(),
                        cx,
                    ));
                }
                body = body.child(glass_group_rows(digest_rows));
            }
        }

        v_flex().gap_6().child(desktop).child(body)
    }
}
