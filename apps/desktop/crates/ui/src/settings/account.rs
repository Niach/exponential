//! Account screen: Integrations + Notifications (masterplan-v3 §4.2
//! "Account", §7.9, EXP-1 #9).
//!
//! Web parity: `routes/_authenticated/account/integrations.tsx` (the GitHub
//! status card) and `.../account/notifications.tsx` (email prefs — see
//! [`super::notifications_prefs`]). One desktop screen with a section nav,
//! reached from the sidebar footer account dropdown (EXP-1 #11).
//!
//! **EXP-1 #9 is enforced here by construction: the integrations pane shows
//! GitHub ONLY.** The stale Google Calendar entry the old native apps carried
//! does not exist in v3 — no calendar UI anywhere on the desktop.

use gpui::{
    div, prelude::FluentBuilder as _, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};

use crate::queries;

use super::notifications_prefs::NotificationsPrefsPane;
use super::repositories::{fetch_github_status, GithubStatus};
use super::{card, open_url, screen_header};

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Integrations,
    Notifications,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Self::Integrations => "Integrations",
            Self::Notifications => "Notifications",
        }
    }
}

/// The account screen (`Screen::Account`).
pub struct AccountView {
    section: Section,
    integrations: Entity<IntegrationsPane>,
    notifications: Entity<NotificationsPrefsPane>,
}

impl AccountView {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let integrations = cx.new(IntegrationsPane::new);
        let notifications = cx.new(NotificationsPrefsPane::new);
        Self {
            section: Section::Integrations,
            integrations,
            notifications,
        }
    }
}

impl Render for AccountView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let active = self.section;

        let subtitle: SharedString = match active {
            Section::Integrations => "Connect external services to your account.".into(),
            Section::Notifications => {
                "Choose which notifications also reach you by email. In-app and push \
                 notifications are always on."
                    .into()
            }
        };

        let pane: gpui::AnyElement = match active {
            Section::Integrations => self.integrations.clone().into_any_element(),
            Section::Notifications => self.notifications.clone().into_any_element(),
        };

        let mut nav_column = v_flex().w(gpui::px(160.)).flex_shrink_0().p_2().gap_0p5();
        for section in [Section::Integrations, Section::Notifications] {
            let selected = section == active;
            nav_column = nav_column.child(
                div()
                    .id(SharedString::from(format!(
                        "account-nav-{}",
                        section.label()
                    )))
                    .px_2()
                    .py_1()
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .cursor_pointer()
                    .when(selected, |item| {
                        item.bg(cx.theme().colors.list_active)
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!selected, |item| {
                        item.text_color(cx.theme().muted_foreground)
                            .hover(|style| style.bg(cx.theme().colors.list_hover))
                    })
                    .child(section.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        cx.notify();
                    })),
            );
        }

        v_flex()
            .size_full()
            .child(screen_header("Account", cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(
                        nav_column
                            .h_full()
                            .border_r_1()
                            .border_color(cx.theme().border),
                    )
                    .child(
                        div()
                            .id("account-scroll")
                            .flex_1()
                            .h_full()
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
                                    .child(pane),
                            ),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Integrations pane — GitHub ONLY (EXP-1 #9)
// ---------------------------------------------------------------------------

enum Load {
    Idle,
    Loading,
    /// `Err` = the status query failed (message shown, refresh retries).
    Ready(Result<GithubStatus, String>),
}

pub struct IntegrationsPane {
    load: Load,
    generation: u64,
    /// The account the loaded state belongs to — a re-login must not show
    /// the previous account's GitHub state.
    account_id: Option<String>,
}

impl IntegrationsPane {
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
                .spawn(async move { fetch_github_status(&trpc).map_err(|err| err.to_string()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = Load::Ready(result);
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }
}

impl Render for IntegrationsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);

        // Web card header: icon tile + title + description.
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
                        .child(Icon::new(IconName::Github)),
                )
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("GitHub"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Install the Exponential GitHub App on the repos you want \
                                     to code on. It opens pull requests, reads diffs, and lets \
                                     your desktop coding sessions clone + push — scoped to just \
                                     those repos.",
                                ),
                        ),
                ),
        );

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_4().w_48())
                        .child(Skeleton::new().h_7().w_40()),
                );
            }
            Load::Ready(Err(message)) => {
                body = body
                    .child(super::error_notice(
                        SharedString::from(format!("Couldn't load GitHub status: {message}")),
                        cx,
                    ))
                    .child(
                        h_flex().child(
                            Button::new("gh-status-retry")
                                .outline()
                                .xsmall()
                                .label("Retry")
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        ),
                    );
            }
            Load::Ready(Ok(status)) if !status.configured => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "GitHub is not configured on this server. Set GITHUB_APP_ID and \
                             GITHUB_APP_PRIVATE_KEY to enable it.",
                        ),
                );
            }
            Load::Ready(Ok(status)) if status.installed => {
                let label: SharedString = if status.accounts.is_empty() {
                    "Installed".into()
                } else {
                    format!("Installed · {}", status.accounts.join(", ")).into()
                };
                let mut section = v_flex().gap_2().child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_sm()
                        .child(
                            div()
                                .size_2()
                                .rounded_full()
                                .bg(theme::tokens::GREEN.to_hsla()),
                        )
                        .child(label),
                );
                if let Some(url) = status.install_url.clone() {
                    // Manage stays a browser hand-off (§7.9).
                    section = section.child(
                        h_flex().child(
                            Button::new("gh-manage-account")
                                .outline()
                                .small()
                                .label("Manage / add repos")
                                .icon(IconName::ExternalLink)
                                .on_click(move |_, _, cx| open_url(cx, url.clone())),
                        ),
                    );
                }
                body = body.child(section);
            }
            Load::Ready(Ok(status)) => {
                if let Some(url) = status.install_url.clone() {
                    body = body.child(
                        h_flex().child(
                            Button::new("gh-install-account")
                                .primary()
                                .small()
                                .label("Install GitHub App")
                                .icon(IconName::ExternalLink)
                                .on_click(move |_, _, cx| open_url(cx, url.clone())),
                        ),
                    );
                } else {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Not connected."),
                    );
                }
            }
        }

        body = body.child(
            h_flex().child(
                Button::new("integrations-refresh")
                    .ghost()
                    .xsmall()
                    .label("Refresh")
                    .loading(matches!(self.load, Load::Loading))
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            ),
        );

        // EXP-1 #9: GitHub is the ONLY integration — nothing else renders.
        v_flex().child(body)
    }
}
