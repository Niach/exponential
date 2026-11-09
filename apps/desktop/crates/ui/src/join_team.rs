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

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, ElementExt as _,
};
use sync::Store;

use crate::controls::{glass_input, WebControl as _};
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
    // EXP-1651: a dialog that opens on a token (a deep link, the dev drive)
    // resolves to the web landing card, which has no title row and no ✕
    // (Escape and a scrim press still close it). The chrome is fixed at open
    // time, so a token picks it; the paste-link dialog keeps its header.
    let chromeless = token.is_some();
    let mut spec = DialogSpec::new(TITLE, size(px(416.), px(340.)));
    if chromeless {
        spec = spec.chromeless();
    }
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = JoinTeamView::new(token, false, window, cx);
            view.chromeless = chromeless;
            view
        });
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
    /// EXP-1651, dialog host only: opened without the header row (a token
    /// was handed in). Until the landing resolves, the view draws the
    /// title itself.
    chromeless: bool,
    /// INVALID INPUT under the field (FEED-79): nothing pasted yet. A failed
    /// accept is a toast.
    error: Option<SharedString>,
    focused_once: bool,
    /// EXP-369: the scrolling form pane, so the footer can stay pinned.
    /// Unused in `embedded` mode (the wizard column scrolls instead).
    body_scroll: ScrollHandle,
    /// EXP-698, embedded only: the wizard's Back-to-the-choice-page hook.
    /// Present ⇒ the footer becomes the web wizard's `justify-between` row
    /// (ghost Back leading, the primary trailing); absent ⇒ the standalone
    /// dialog's Cancel/Join row is unchanged.
    on_back: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    /// P9, dialog host only: the form's natural height and the scroll body's
    /// box last frame, and the content height the window was last fitted to
    /// — the dialog follows its content (no dead pane under a preview).
    content_h: Rc<std::cell::Cell<gpui::Pixels>>,
    body_h: Rc<std::cell::Cell<gpui::Pixels>>,
    fitted: Option<gpui::Pixels>,
    _subscriptions: Vec<Subscription>,
}

/// The paste-link dialog's title.
const TITLE: &str = "Join a team";

/// ×2 (web wizard): the join field's placeholder and the preview button.
pub(crate) const PASTE_PLACEHOLDER: &str = "Paste an invite link";
pub(crate) const CONTINUE: &str = "Continue";

/// EXP-1651: the web `/invite/$token` landing's copy, which the dialog shows
/// once the link resolves to a team.
const LANDING_TITLE: &str = "Team invite";
const LANDING_SUBTITLE: &str = "You've been invited to join";
const ACCEPT_INVITE: &str = "Accept invite";
const INVITE_USED: &str = "This invite has already been used";
const INVITE_EXPIRED: &str = "This invite has expired";

/// The invite's `expiresAt` lies in the past (web `invite.expiresAt < now`).
fn invite_expired(invite: &TeamInviteOut, now: chrono::DateTime<chrono::Utc>) -> bool {
    invite
        .expires_at
        .as_deref()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .is_some_and(|at| at < now)
}

impl JoinTeamView {
    pub(crate) fn new(
        token: Option<String>,
        embedded: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let token_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(PASTE_PLACEHOLDER)
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
            chromeless: false,
            error: None,
            focused_once: false,
            body_scroll: ScrollHandle::new(),
            on_back: None,
            content_h: Rc::default(),
            body_h: Rc::default(),
            fitted: None,
            _subscriptions: subscriptions,
        };
        if let Some(token) = token {
            // P9: the deep link's token SHOWS in the field it previews.
            let value = token.clone();
            this.token_input
                .update(cx, |state, cx| state.set_value(value, window, cx));
            this.start_preview(token, cx);
        }
        this
    }

    /// EXP-698 (embedded host only): give the footer a Back button that
    /// returns the onboarding wizard to its choice page.
    pub(crate) fn with_back(mut self, on_back: Rc<dyn Fn(&mut Window, &mut App)>) -> Self {
        self.on_back = Some(on_back);
        self
    }

    /// Enter / the footer button: preview when we have no card yet, accept
    /// when we do.
    fn primary_action(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match &self.preview {
            // A used or expired invite offers no accept (the landing says so).
            Preview::Ready(invite)
                if invite.accepted_at.is_some() || invite_expired(invite, chrono::Utc::now()) => {}
            Preview::Ready(_) => self.accept(window, cx),
            Preview::Loading => {}
            _ => {
                let Some(token) = extract_token(&self.token_input.read(cx).value()) else {
                    self.error = Some("Paste an invite link first.".into());
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
            crate::action_error::failed_action("accept", "the invite", Some("Not signed in.".into()), cx);
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
                        // Seed the row from the accept response so the
                        // switch is instant (EXP-470); the pipeline restart
                        // makes the rotated shapes deliver the joined team's
                        // boards/issues within seconds instead of waiting
                        // out their parked long-polls.
                        let seeded = output.team.as_ref().map(|team| {
                            domain::rows::Team::seeded(
                                team.id.clone(),
                                team.name.clone().unwrap_or_else(|| "New team".to_string()),
                                team.slug.clone(),
                            )
                        });
                        let teams = window
                            .update(|_, cx| {
                                let store = Store::global(cx).clone();
                                if let Some(seeded) = seeded {
                                    store.collections().seed_team(seeded, cx);
                                    store.resync_active(cx);
                                }
                                store.collections().teams.clone()
                            })
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
                        // FEED-79: the error toast with the server's message
                        // (expired/used).
                        this.accepting = false;
                        crate::action_error::failed_action_err("accept", "the invite", &err, cx);
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

impl JoinTeamView {
    /// EXP-1651 — the dialog host's RESOLVED state, the web `/invite/$token`
    /// landing line for line: the users disc, "Team invite", the muted
    /// subtitle, the team card (name large, role muted) and the full-width
    /// primary "Accept invite" (or the used / expired line in its place).
    fn render_landing(&self, invite: &TeamInviteOut, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let name = invite
            .team_name
            .clone()
            .unwrap_or_else(|| "a team".to_string());
        let role = invite.role.clone().unwrap_or_else(|| "member".to_string());
        let used = invite.accepted_at.is_some();
        let expired = invite_expired(invite, chrono::Utc::now());

        let head = v_flex()
            .w_full()
            .items_center()
            .gap_2()
            .text_center()
            .child(
                div()
                    .mb_2()
                    .size(px(48.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(theme.primary.opacity(0.1))
                    .child(
                        gpui_component::Icon::new(registry::SETTINGS_MEMBERS)
                            .size(px(24.))
                            .text_color(theme.primary),
                    ),
            )
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child(LANDING_TITLE),
            )
            .child(div().text_sm().text_color(muted).child(LANDING_SUBTITLE));

        let card = v_flex()
            .w_full()
            .items_center()
            .gap_1()
            .p_4()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .text_center()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child(SharedString::from(name)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(SharedString::from(format!("Role: {role}"))),
            );

        let notice = |text: &'static str, color: gpui::Hsla| {
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(color)
                .child(gpui_component::Icon::new(registry::UI_WARNING).size(px(16.)))
                .child(text)
                .into_any_element()
        };
        let action = if expired {
            notice(INVITE_EXPIRED, theme.danger)
        } else if used {
            notice(INVITE_USED, muted)
        } else {
            Button::new("join-team-primary")
                .primary()
                .web_md()
                .w_full()
                .label(ACCEPT_INVITE)
                .disabled(self.accepting)
                .loading(self.accepting)
                .on_click(cx.listener(|this, _, window, cx| this.primary_action(window, cx)))
                .into_any_element()
        };

        v_flex()
            .w_full()
            .pt_2()
            .gap_6()
            .child(head)
            .child(v_flex().w_full().gap_4().child(card).child(action))
    }

    /// P9 — size the dialog WINDOW to the form (the trigger dialog's fit):
    /// content plus the chrome around the scroll body, between the dialog's
    /// opening height and 85% of the screen it sits on. Refits only when the
    /// content moved; never touches a maximized or fullscreen window.
    fn fit_window(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let content = self.content_h.get();
        let body = self.body_h.get();
        if content <= px(0.) || body <= px(0.) || self.fitted == Some(content) {
            return;
        }
        self.fitted = Some(content);
        // EXP-1557: the in-window dialog's content box, not the window.
        let Some(current) = crate::native_dialog::dialog_size(window, cx) else {
            return;
        };
        let chrome = (current.height - body).max(px(0.));
        let cap = window
            .display(cx)
            .map(|display| display.bounds().size.height * 0.85)
            .unwrap_or(px(640.));
        let target = (content + chrome).min(cap).max(px(DIALOG_MIN_H));
        if (target - current.height).abs() <= px(1.) {
            return;
        }
        native_dialog::resize_dialog_keeping_top(window, cx, size(current.width, target));
    }
}

impl JoinTeamView {
    /// EXP-1651: the resolved landing card's height (disc, title, team card,
    /// Accept invite), set once.
    fn pin_landing_height(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let target = px(LANDING_H);
        if self.fitted == Some(target) {
            return;
        }
        let Some(current) = crate::native_dialog::dialog_size(window, cx) else {
            return;
        };
        self.fitted = Some(target);
        if (target - current.height).abs() > px(1.) {
            native_dialog::resize_dialog_keeping_top(window, cx, size(current.width, target));
        }
    }
}

/// The resolved landing card's content height (EXP-1651).
const LANDING_H: f32 = 300.;

/// The dialog's floor: the paste field and the footer.
const DIALOG_MIN_H: f32 = 200.;

impl Render for JoinTeamView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "JoinTeamView").entered();
        // First render focuses the token field. (Embedded mode used to skip
        // this while the create-team form rendered above on the same page;
        // since EXP-470 the wizard shows Join on its own page, so it may
        // focus too.)
        if !self.focused_once {
            self.focused_once = true;
            self.token_input
                .update(cx, |state, cx| state.focus(window, cx));
        }

        // EXP-1651: the dialog host shows the web landing once the link
        // resolves to a team; the paste form stays for everything before.
        let landing = match &self.preview {
            Preview::Ready(invite) if !self.embedded => {
                Some(self.render_landing(&invite.clone(), cx))
            }
            _ => None,
        };

        // EXP-1651: a chromeless dialog still loading (or refused) names
        // itself, as the header row would.
        let own_title = (self.chromeless && landing.is_none()).then(|| {
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(TITLE)
        });
        let mut form = v_flex().gap_3().children(own_title).child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .child("Invite link"),
                )
                .child(glass_input(&self.token_input, window, cx).web_input()),
        );

        let (primary_label, primary_disabled): (&'static str, bool) = match &self.preview {
            // EXP-1557: an empty field has nothing to continue with — web
            // disables Continue until a link is pasted.
            Preview::Idle => (
                CONTINUE,
                self.token_input.read(cx).value().trim().is_empty(),
            ),
            Preview::Loading => {
                form = form.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Looking up the invite…"),
                );
                (CONTINUE, true)
            }
            // The pasted link the server refused is about the INPUT: the
            // field's own line (FEED-79).
            Preview::Failed(message) => {
                form = form.child(crate::action_error::field_error(message.clone(), cx));
                (CONTINUE, false)
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
            form = form.child(crate::action_error::field_error(error.clone(), cx));
        }

        // Cancel exists only in the dialog host — the embedded wizard step has
        // nothing to close; it gets a Back to the wizard's choice page instead
        // (leading, with the primary pushed to the trailing edge).
        let footer = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .map(|row| {
                if self.on_back.is_some() {
                    row.justify_between()
                } else {
                    row.justify_end()
                }
            })
            .when_some(self.on_back.clone(), |row, on_back| {
                row.child(
                    Button::new("onboarding-join-back")
                        .ghost()
                        .web_sm()
                        .icon(registry::UI_BACK)
                        .label("Back")
                        .disabled(self.accepting)
                        .on_click(move |_, window, cx| on_back(window, cx)),
                )
            })
            .when(!self.embedded, |row| {
                row.child(
                    Button::new("join-team-cancel")
                        .outline().web_sm()
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
                {
                    let primary = Button::new("join-team-primary")
                        .primary()
                        .web_md()
                        .accessibility_label(primary_label);
                    if self.embedded { primary.rounded_full() } else { primary }
                }
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
        // edge (see [`DialogContent::self_scrolling`] in [`open`]). P9: the
        // window follows the form's height (both boxes report at prepaint).
        // EXP-1651: the landing carries its own full-width action, no footer,
        // and a fixed height (the fit measured it short and cut Accept off).
        let resolved = landing.is_some();
        if resolved {
            self.pin_landing_height(window, cx);
        } else {
            self.fit_window(window, cx);
        }
        let form = landing.unwrap_or(form);
        let view_id = cx.entity_id();
        let (content_slot, body_slot) = (self.content_h.clone(), self.body_h.clone());
        let body_scroll = self.body_scroll.clone();
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .on_prepaint(move |bounds, _, cx| {
                        if body_slot.get() != bounds.size.height {
                            body_slot.set(bounds.size.height);
                            cx.notify(view_id);
                        }
                    })
                    .child(
                        v_flex()
                            .id("join-team-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(
                                v_flex()
                                    .w_full()
                                    .flex_shrink_0()
                                    .on_prepaint(move |bounds, _, cx| {
                                        if content_slot.get() != bounds.size.height {
                                            content_slot.set(bounds.size.height);
                                            cx.notify(view_id);
                                        }
                                    })
                                    .child(form),
                            ),
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
            .when(!resolved, |this| {
                this.child(footer.pt_3().border_t_1().border_color(cx.theme().border))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1651: the landing's expired line follows `expiresAt`.
    #[test]
    fn an_invite_past_its_expiry_reads_expired() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-03T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let invite = |expires_at: Option<&str>| TeamInviteOut {
            expires_at: expires_at.map(str::to_string),
            ..serde_json::from_value(serde_json::json!({ "id": "i" })).unwrap()
        };
        assert!(invite_expired(&invite(Some("2026-11-01T00:00:00Z")), now));
        assert!(!invite_expired(&invite(Some("2026-11-10T00:00:00.000Z")), now));
        assert!(!invite_expired(&invite(None), now));
    }

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
