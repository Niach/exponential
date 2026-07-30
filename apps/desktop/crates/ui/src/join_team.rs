//! "Join a team" — the §4.2 accept-invite surface (masterplan-v3 §4.2;
//! web parity target: `apps/web/src/routes/invite/$token.tsx`).
//!
//! A native desktop app cannot receive the browser's
//! `https://…/invite/<token>` click, so §4.2 mandates two paths:
//!
//! 1. the **`exponential://invite/<token>` deep link** — routed here from
//!    [`crate::oauth::handle_open_urls`] (paired with the OAuth `exponential://`
//!    scheme registration);
//! 2. a fallback **"Join a team" dialog** (the sidebar footer account
//!    menu's "Join team…" item) where the user pastes an invite link or
//!    raw token.
//!
//! Both call `teamInvites.getByToken` to **preview** (team name +
//! role + expired / already-used states — the web card), then
//! `teamInvites.accept`, gate on the joined team appearing in the
//! synced collection (§4.1), and switch the window to it.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use crate::native_dialog::{self, DialogContent, DialogSpec};

use api::teams::TeamInviteOut;

use crate::actions::JoinTeam;
use crate::navigation::switch_team;
use crate::queries;
use crate::icons::registry;

/// Register the App-global [`JoinTeam`] handler (call once from
/// `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &JoinTeam, cx| {
        crate::navigation::on_active_window(cx, |window, cx| open(window, cx, None));
    });
}

/// Open the dialog, optionally pre-filled (the `exponential://invite/<token>` deep
/// link passes the token and previews immediately).
pub fn open(window: &mut Window, cx: &mut App, token: Option<String>) {
    // Never stack over an open modal (deep link mid-dialog); EXP-287 raises
    // the dialog this window already has instead of dropping the request.
    if native_dialog::raise_existing_dialog(window, cx) {
        return;
    }
    // EXP-369: 300 → 340 so the preview card fits under the paste field; the
    // pinned footer keeps "Join team" reachable when it doesn't.
    let spec = DialogSpec::new("Join a team", size(px(416.), px(340.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| JoinTeamView::new(token, false, window, cx));
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            // EXP-369: the view pins its own footer and scrolls only the form.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).accepting)
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.primary_action(window, cx));
            })
    });
}

/// Emitted (embedded host only, EXP-367) once the invite is accepted and the
/// joined team — when the server named one — is VISIBLE in the synced
/// collection. The onboarding wizard advances on it (the server stamps
/// `onboardingCompletedAt` on accept).
pub(crate) struct InviteAccepted;
impl gpui::EventEmitter<InviteAccepted> for JoinTeamView {}

/// `exponential://invite/<token>` → `Some(token)` (the §4.2 deep-link form;
/// scheme from `api::login::OAUTH_CALLBACK_SCHEME`).
pub(crate) fn parse_invite_deep_link(url: &str) -> Option<String> {
    let prefix = format!("{}://invite/", api::login::OAUTH_CALLBACK_SCHEME);
    let rest = url.strip_prefix(prefix.as_str())?;
    let token = rest
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    (!token.is_empty()).then(|| token.to_string())
}

/// Paste tolerance: accept a full `https://…/invite/<token>` link OR a raw
/// token (mirror of what the web route path carries).
fn extract_token(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(pos) = trimmed.find("/invite/") {
        let rest = &trimmed[pos + "/invite/".len()..];
        let token = rest
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .trim_end_matches('/');
        return (!token.is_empty()).then(|| token.to_string());
    }
    (!trimmed.contains(char::is_whitespace)).then(|| trimmed.to_string())
}

enum Preview {
    /// Nothing previewed yet (the paste form shows).
    Idle,
    Loading,
    /// The web card: name + role (+ expired/used gating below).
    Ready(TeamInviteOut),
    /// Invalid/expired token (server NOT_FOUND) or a transport error.
    Failed(SharedString),
}

pub struct JoinTeamView {
    token_input: Entity<InputState>,
    /// The token of the current `preview` (deep link or extracted).
    token: Option<String>,
    preview: Preview,
    /// EXP-367: hosted inside the onboarding wizard instead of a native
    /// dialog window — no Cancel button, no auto-focus (the create-team form
    /// above it owns first focus), success EMITS [`InviteAccepted`] instead
    /// of closing a dialog.
    embedded: bool,
    accepting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    /// EXP-369: the scrolling form pane, so the footer can stay pinned.
    /// Unused in `embedded` mode (the wizard column scrolls instead).
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl JoinTeamView {
    pub(crate) fn new(
        token: Option<String>,
        embedded: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let token_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Paste an invite link or token…")
        });
        let subscriptions = vec![cx.subscribe_in(
            &token_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.primary_action(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )];

        let mut this = Self {
            token_input,
            token: None,
            preview: Preview::Idle,
            embedded,
            accepting: false,
            error: None,
            focused_once: false,
            body_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        if let Some(token) = token {
            this.start_preview(token, cx);
        }
        this
    }

    /// Enter / the footer button: preview when we have no card yet, accept
    /// when we do.
    fn primary_action(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match &self.preview {
            Preview::Ready(_) => self.accept(window, cx),
            Preview::Loading => {}
            _ => {
                let Some(token) = extract_token(&self.token_input.read(cx).value()) else {
                    self.error = Some("Paste an invite link or token first.".into());
                    cx.notify();
                    return;
                };
                self.error = None;
                self.start_preview(token, cx);
            }
        }
    }

    /// `teamInvites.getByToken` — the web card's preview query.
    fn start_preview(&mut self, token: String, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.preview = Preview::Failed("Sign in to accept an invite.".into());
            cx.notify();
            return;
        };
        self.token = Some(token.clone());
        self.preview = Preview::Loading;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::teams::team_invites_get_by_token(&trpc, &token)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.preview = match result {
                    Ok(invite) => Preview::Ready(invite),
                    Err(err) => {
                        log::warn!("[ui] invite preview failed: {err}");
                        // Web: "Invalid or expired invite link".
                        Preview::Failed("Invalid or expired invite link".into())
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// `teamInvites.accept` → gate on the teams echo → switch.
    fn accept(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.accepting {
            return;
        }
        let Some(token) = self.token.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.accepting = true;
        self.error = None;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::teams::team_invites_accept(&trpc, &token) })
                .await;

            match result {
                Ok(output) => {
                    let team_id = output.team.as_ref().map(|w| w.id.clone());
                    if let Some(team_id) = team_id {
                        // §4.1 gated flow: the joined team must be
                        // visible in the synced collection before we switch.
                        let teams = window
                            .update(|_, cx| Store::global(cx).collections().teams.clone())
                            .ok();
                        if let Some(teams) = teams {
                            queries::await_row_visible(&teams, &team_id, window).await;
                        }
                        let _ = this.update_in(window, |view, window, cx| {
                            // The server stamped onboardingCompletedAt on
                            // accept — mirror it locally (EXP-367; warm
                            // starts never re-fetch the session).
                            crate::onboarding::stamp_local_onboarding(cx);
                            if view.embedded {
                                view.accepting = false;
                                switch_team(window, cx, team_id.clone());
                                cx.emit(InviteAccepted);
                                cx.notify();
                            } else {
                                native_dialog::close_then(window, cx, move |window, cx| {
                                    switch_team(window, cx, team_id);
                                });
                            }
                        });
                    } else {
                        let _ = this.update_in(window, |view, window, cx| {
                            crate::onboarding::stamp_local_onboarding(cx);
                            if view.embedded {
                                view.accepting = false;
                                cx.emit(InviteAccepted);
                                cx.notify();
                            } else {
                                native_dialog::close_dialog_window(window, cx);
                            }
                        });
                    }
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        // Web surfaces the server message (expired/used).
                        this.accepting = false;
                        this.error = Some(format!("{err}").into());
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn render_preview_card(
        &self,
        invite: &TeamInviteOut,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let name = invite
            .team_name
            .clone()
            .unwrap_or_else(|| "a team".to_string());
        let role = invite.role.clone().unwrap_or_else(|| "member".to_string());
        let used = invite.accepted_at.is_some();

        let mut card = v_flex()
            .gap_1()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(format!("You're invited to {name}"))),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format!("Role: {role}"))),
            );
        if used {
            // Web: "This invite has already been used".
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child("This invite has already been used"),
            );
        }
        card.into_any_element()
    }
}

impl Render for JoinTeamView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Embedded (wizard) hosting: the create-team form above owns first
        // focus — stealing it here would fight it every render pass.
        if !self.focused_once && !self.embedded {
            self.focused_once = true;
            self.token_input
                .update(cx, |state, cx| state.focus(window, cx));
        }

        let mut form = v_flex().gap_3().child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Invite link"),
                )
                .child(Input::new(&self.token_input).small()),
        );

        let (primary_label, primary_disabled): (&'static str, bool) = match &self.preview {
            Preview::Idle => ("Preview invite", false),
            Preview::Loading => {
                form = form.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Looking up the invite…"),
                );
                ("Preview invite", true)
            }
            Preview::Failed(message) => {
                form = form.child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(Icon::new(registry::UI_WARNING).xsmall())
                        .child(message.clone()),
                );
                ("Preview invite", false)
            }
            Preview::Ready(invite) => {
                let used = invite.accepted_at.is_some();
                let card = self.render_preview_card(&invite.clone(), cx);
                form = form.child(card);
                if used {
                    ("Join team", true)
                } else if self.accepting {
                    ("Joining…", true)
                } else {
                    ("Join team", false)
                }
            }
        };

        if let Some(error) = &self.error {
            form = form.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        // Cancel exists only in the dialog host — the embedded wizard step has
        // nothing to close.
        let footer = h_flex()
            .flex_shrink_0()
            .justify_end()
            .gap_2()
            .when(!self.embedded, |row| {
                row.child(
                    Button::new("join-team-cancel")
                        .outline()
                        .small()
                        .label("Cancel")
                        .disabled(self.accepting)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.accepting {
                                return;
                            }
                            native_dialog::close_dialog_window(window, cx);
                        })),
                )
            })
            .child(
                Button::new("join-team-primary")
                    .primary()
                    .small()
                    .label(primary_label)
                    .disabled(primary_disabled)
                    .loading(self.accepting)
                    .on_click(cx.listener(|this, _, window, cx| this.primary_action(window, cx))),
            );

        // Embedded (onboarding wizard): the host column owns the scrolling and
        // has no definite height for `size_full` to resolve against — keep the
        // buttons in flow there.
        if self.embedded {
            return form.child(footer).into_any_element();
        }

        // EXP-369: the form scrolls, the buttons stay pinned at the bottom
        // edge (see [`DialogContent::self_scrolling`] in [`open`]).
        let body_scroll = self.body_scroll.clone();
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .id("join-team-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(form),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(Scrollbar::new(&body_scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_link_parses_token() {
        assert_eq!(
            parse_invite_deep_link("exponential://invite/abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            parse_invite_deep_link("exponential://invite/abc123?utm=x"),
            Some("abc123".to_string())
        );
        assert_eq!(parse_invite_deep_link("exponential://invite/"), None);
        assert_eq!(parse_invite_deep_link("exponential://oauth-return#token=t"), None);
        assert_eq!(parse_invite_deep_link("https://x/invite/abc"), None);
    }

    #[test]
    fn extract_token_accepts_links_and_raw_tokens() {
        assert_eq!(
            extract_token("https://app.exponential.at/invite/tok123"),
            Some("tok123".to_string())
        );
        assert_eq!(
            extract_token("https://app.exponential.at/invite/tok123?x=1"),
            Some("tok123".to_string())
        );
        assert_eq!(extract_token(" tok123 "), Some("tok123".to_string()));
        assert_eq!(extract_token("not a token"), None);
        assert_eq!(extract_token(""), None);
        assert_eq!(extract_token("https://x/invite/"), None);
    }

    #[test]
    fn extract_token_is_agnostic_to_the_teams_path_rename() {
        // The invite link lives at the web root (`/invite/<token>`), so the
        // `/w/` → `/t/` team-slug rename never touches it — extraction keys on
        // the `/invite/` segment wherever it appears. Both the legacy `/w/`
        // form and the new `/t/` form (should either ever wrap an invite) still
        // resolve; `/w/` acceptance is permanent.
        assert_eq!(
            extract_token("https://app.exponential.at/w/acme/invite/tok123"),
            Some("tok123".to_string())
        );
        assert_eq!(
            extract_token("https://app.exponential.at/t/acme/invite/tok123"),
            Some("tok123".to_string())
        );
    }
}
