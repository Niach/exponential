//! "Join a workspace" — the §4.2 accept-invite surface (masterplan-v3 §4.2;
//! web parity target: `apps/web/src/routes/invite/$token.tsx`).
//!
//! A native desktop app cannot receive the browser's
//! `https://…/invite/<token>` click, so §4.2 mandates two paths:
//!
//! 1. the **`exp://invite/<token>` deep link** — routed here from
//!    [`crate::oauth::handle_open_urls`] (paired with the OAuth `exp://`
//!    scheme registration);
//! 2. a fallback **"Join a workspace" dialog** (the sidebar footer account
//!    menu's "Join workspace…" item) where the user pastes an invite link or
//!    raw token.
//!
//! Both call `workspaceInvites.getByToken` to **preview** (workspace name +
//! role + expired / already-used states — the web card), then
//! `workspaceInvites.accept`, gate on the joined workspace appearing in the
//! synced collection (§4.1), and switch the window to it.

use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::workspaces::WorkspaceInviteOut;

use crate::actions::JoinWorkspace;
use crate::navigation::switch_workspace;
use crate::queries;

/// Register the App-global [`JoinWorkspace`] handler (call once from
/// `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &JoinWorkspace, cx| {
        crate::navigation::on_active_window(cx, |window, cx| open(window, cx, None));
    });
}

/// Open the dialog, optionally pre-filled (the `exp://invite/<token>` deep
/// link passes the token and previews immediately).
pub fn open(window: &mut Window, cx: &mut App, token: Option<String>) {
    if window.has_active_dialog(cx) {
        return; // never stack over an open modal (deep link mid-dialog)
    }
    let view = cx.new(|cx| JoinWorkspaceView::new(token, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).accepting;
        dialog
            .w(px(416.))
            .title("Join a workspace")
            .overlay_closable(!busy)
            .keyboard(!busy)
            .on_ok({
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |view, cx| view.primary_action(window, cx));
                    false
                }
            })
            .child(view.clone())
    });
}

/// `exp://invite/<token>` → `Some(token)` (the §4.2 deep-link form).
pub(crate) fn parse_invite_deep_link(url: &str) -> Option<String> {
    let rest = url.strip_prefix("exp://invite/")?;
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
    Ready(WorkspaceInviteOut),
    /// Invalid/expired token (server NOT_FOUND) or a transport error.
    Failed(SharedString),
}

pub struct JoinWorkspaceView {
    token_input: Entity<InputState>,
    /// The token of the current `preview` (deep link or extracted).
    token: Option<String>,
    preview: Preview,
    accepting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl JoinWorkspaceView {
    fn new(token: Option<String>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
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
            accepting: false,
            error: None,
            focused_once: false,
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

    /// `workspaceInvites.getByToken` — the web card's preview query.
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
                    api::workspaces::workspace_invites_get_by_token(&trpc, &token)
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

    /// `workspaceInvites.accept` → gate on the workspaces echo → switch.
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
                .spawn(async move { api::workspaces::workspace_invites_accept(&trpc, &token) })
                .await;

            match result {
                Ok(output) => {
                    let workspace_id = output.workspace.as_ref().map(|w| w.id.clone());
                    if let Some(workspace_id) = workspace_id {
                        // §4.1 gated flow: the joined workspace must be
                        // visible in the synced collection before we switch.
                        let workspaces = window
                            .update(|_, cx| Store::global(cx).collections().workspaces.clone())
                            .ok();
                        if let Some(workspaces) = workspaces {
                            queries::await_row_visible(&workspaces, &workspace_id, window).await;
                        }
                        let _ = this.update_in(window, |_, window, cx| {
                            window.close_dialog(cx);
                            switch_workspace(window, cx, workspace_id);
                        });
                    } else {
                        let _ = this.update_in(window, |_, window, cx| {
                            window.close_dialog(cx);
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
        invite: &WorkspaceInviteOut,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let name = invite
            .workspace_name
            .clone()
            .unwrap_or_else(|| "a workspace".to_string());
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

impl Render for JoinWorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
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
                        .child(Icon::new(IconName::TriangleAlert).xsmall())
                        .child(message.clone()),
                );
                ("Preview invite", false)
            }
            Preview::Ready(invite) => {
                let used = invite.accepted_at.is_some();
                let card = self.render_preview_card(&invite.clone(), cx);
                form = form.child(card);
                if used {
                    ("Join workspace", true)
                } else if self.accepting {
                    ("Joining…", true)
                } else {
                    ("Join workspace", false)
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

        form.child(
            h_flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("join-workspace-cancel")
                        .outline()
                        .small()
                        .label("Cancel")
                        .disabled(self.accepting)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.accepting {
                                return;
                            }
                            window.close_dialog(cx);
                        })),
                )
                .child(
                    Button::new("join-workspace-primary")
                        .primary()
                        .small()
                        .label(primary_label)
                        .disabled(primary_disabled)
                        .loading(self.accepting)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.primary_action(window, cx)),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_link_parses_token() {
        assert_eq!(
            parse_invite_deep_link("exp://invite/abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            parse_invite_deep_link("exp://invite/abc123?utm=x"),
            Some("abc123".to_string())
        );
        assert_eq!(parse_invite_deep_link("exp://invite/"), None);
        assert_eq!(parse_invite_deep_link("exp://oauth-return#token=t"), None);
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
}
