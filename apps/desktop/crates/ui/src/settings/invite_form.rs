//! THE invite-by-email form (EXP-630).
//!
//! Web parity: `components/team/invite-member-form.tsx` — the Members pane and
//! the "Resend invite" dialog render this ONE view, so an invite looks and
//! behaves the same wherever it starts: an optional Name beside the address, a
//! PRIMARY "Send invite", and — when the mail cannot go out — the generated
//! link for sharing by hand.
//!
//! Sending puts the person on the roster AT ONCE as a PLACEHOLDER member (name
//! + email, assignable and attributable before they ever sign in), so the form
//! is a member-creating control, not just a mailer. A
//! [`ResendTarget`] turns it into a RE-INVITE of an existing placeholder whose
//! address stays editable — a resend is usually there to fix a typo — and the
//! fields then KEEP their values (the dialog closes on
//! [`InviteDelivered`] instead).
//!
//! #EXP-1629: the Members pane draws its own form as document nodes (with
//! FEED-108's OUTLINE "Add without inviting"); this gpui view is the
//! "Resend invite" dialog's (web `layout="stack"`). Both read a send's
//! answer through [`invite_outcome`].

use gpui::{
    div, App, AppContext as _, ElementId, Entity, FontWeight, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _,
};

use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::queries;

/// EXP-771 (web copy wins): the server accepted the invite but could not mail
/// it — the generated link renders right under this, so the wording points at
/// it. Kept byte-identical to `invite-member-form.tsx`'s toast — a toast
/// here too (EXP-1031).
/// FEED-79: the invite exists but its mail did not go out — the toast's
/// description under `Couldn't email the invite`.
const EMAIL_FALLBACK_MESSAGE: &str = "Copy the link below and share it instead.";

/// EXP-771: the seat cap is not "some plan limit" — it is the web's
/// `UpgradeDialog` on the invite controls, rendered inline here (the desktop
/// never shows pricing, §4.9). Title + body byte-identical to the web.
/// feedback-states.json's invalid-email input case: the field's line when
/// the server refused the address without a sentence of its own.
pub(super) const INVALID_EMAIL: &str = "Enter a full email address";

pub(super) const OUT_OF_SEATS_TITLE: &str = "Out of seats";
pub(super) const OUT_OF_SEATS_MESSAGE: &str =
    "Everyone on your plan's seats is already in this team. Add seats to \
     invite more teammates.";
/// The desktop's one billing hand-off under the notice (§4.9).
pub(super) const OUT_OF_SEATS_NOTE: &str = "Add seats on the web.";

/// The mail went out — the "Resend invite" dialog closes on it (web
/// `onInvited` + `emailDelivered`).
pub(super) struct InviteDelivered;

/// The placeholder member a "Resend invite" is bound to (web `resendTarget`).
#[derive(Clone, Debug)]
pub(super) struct ResendTarget {
    pub user_id: String,
    /// Prefill for the Name field (the placeholder's current display name).
    pub name: String,
    /// Prefill for the address — editable, since the reason for a resend is
    /// often that it was wrong.
    pub email: String,
}

pub struct InviteForm {
    team_id: String,
    /// `Some` = re-invite THIS placeholder member: the row keeps its
    /// attributions and the address may be corrected.
    placeholder_user_id: Option<String>,
    /// Optional display name (EXP-630) — empty lets the server fall back to
    /// the mailbox local part.
    name_input: Entity<InputState>,
    email_input: Entity<InputState>,
    sending: bool,
    /// EXP-1259 D8: the Send button's pending face ("Inviting").
    pending: crate::feedback::Pending,
    /// FEED-79: the address the server refused — INVALID INPUT, so it stays
    /// under the field (13px rose) until the address changes; every other
    /// failure is a toast.
    email_error: Option<SharedString>,
    /// The seat cap rejected the invite — the web's "Out of seats" notice.
    out_of_seats: bool,
    /// The minted link: the manual fallback when delivery failed.
    invite_url: Option<SharedString>,
    /// Web `autoFocus="email"` — a resend opens ON the address. Consumed by
    /// the first render.
    focus_email: bool,
    _subscriptions: Vec<Subscription>,
}

impl gpui::EventEmitter<InviteDelivered> for InviteForm {}

impl InviteForm {
    pub(super) fn new(
        team_id: String,
        resend: Option<ResendTarget>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| {
            let state = InputState::new(window, cx).placeholder("Name");
            match &resend {
                Some(target) => state.default_value(target.name.clone()),
                None => state,
            }
        });
        let email_input = cx.new(|cx| {
            let state = InputState::new(window, cx).placeholder("teammate@example.com");
            match &resend {
                Some(target) => state.default_value(target.email.clone()),
                None => state,
            }
        });
        let subscriptions = vec![
            // "Send invite" is disabled while the address is empty, so the
            // button has to re-render on every keystroke.
            cx.subscribe_in(
                &email_input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        this.email_error = None;
                        cx.notify();
                    }
                    // Web: Enter in the address field sends.
                    InputEvent::PressEnter { .. } => this.send(window, cx),
                    _ => {}
                },
            ),
        ];
        Self {
            team_id,
            placeholder_user_id: resend.as_ref().map(|target| target.user_id.clone()),
            name_input,
            email_input,
            sending: false,
            pending: crate::feedback::Pending::default(),
            email_error: None,
            out_of_seats: false,
            invite_url: None,
            focus_email: resend.is_some(),
            _subscriptions: subscriptions,
        }
    }

    /// A send is in flight — the dialog refuses to close mid-request.
    pub(super) fn sending(&self) -> bool {
        self.sending
    }

    /// `teamInvites.create` with an address: the server mails the link, puts
    /// the invitee on the roster as a placeholder member and reports delivery.
    /// Delivery is best-effort — with no transport configured (or on a failed
    /// send) the link is shown so the owner can share it by hand.
    fn send(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.sending {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let base = account.instance_url;
        let email = self.email_input.read(cx).value().trim().to_string();
        if email.is_empty() {
            return;
        }
        let name = self.name_input.read(cx).value().trim().to_string();
        let placeholder = self.placeholder_user_id.clone();
        let team_id = self.team_id.clone();

        self.sending = true;
        self.pending.start(cx);
        self.email_error = None;
        self.out_of_seats = false;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let request = (email.clone(), name, placeholder);
            let result = window
                .background_executor()
                .spawn(async move {
                    let (email, name, placeholder) = request;
                    api::teams::team_invites_create(
                        &trpc,
                        &team_id,
                        api::teams::TeamRole::Member,
                        Some(&email),
                        api::teams::InviteExtras {
                            name: Some(name.as_str()),
                            placeholder_user_id: placeholder.as_deref(),
                        },
                    )
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.sending = false;
                this.pending.finish();
                match invite_outcome(result, &base, &email, window, cx) {
                    InviteOutcome::Delivered(url) => {
                        this.invite_url = Some(url.into());
                        // A resend keeps its prefilled row (the dialog it
                        // sits in closes on the event below).
                        if this.placeholder_user_id.is_none() {
                            this.name_input.update(cx, |state, cx| state.set_value("", window, cx));
                            this.email_input.update(cx, |state, cx| state.set_value("", window, cx));
                        }
                        cx.emit(InviteDelivered);
                    }
                    InviteOutcome::Undelivered(url) => this.invite_url = Some(url.into()),
                    InviteOutcome::OutOfSeats => this.out_of_seats = true,
                    InviteOutcome::Refused(message) => this.email_error = Some(message.into()),
                    InviteOutcome::Failed => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// What `teamInvites.create` with an address came to — the form and the
/// Members pane read the same answer, each into its own fields.
pub(super) enum InviteOutcome {
    /// The mail went out; the link (`{base}/invite/{token}`) stays as a
    /// manual fallback.
    Delivered(String),
    /// The invite exists but its mail did not go out: the link to share.
    Undelivered(String),
    /// The seat cap refused it — the web's "Out of seats" notice.
    OutOfSeats,
    /// The address the server refused: the line under the field
    /// (feedback-states.json's invalid-email case).
    Refused(String),
    /// Anything else (already toasted).
    Failed,
}

/// Reads a send's `result` and raises its toasts: "Invite sent to …"
/// (EXP-1031: landing in the window that outlives a resend dialog closing on
/// it), the failed-delivery toast pointing at the link (FEED-79), the
/// failed-action toast for anything but a seat cap or a refused address.
pub(super) fn invite_outcome(result: Result<api::teams::InviteCreateOutput, api::ApiError>, base: &str, email: &str, window: &mut Window, cx: &mut App) -> InviteOutcome {
    match result {
        Ok(out) => {
            let url = format!("{base}/invite/{}", out.token);
            if out.email_delivered == Some(true) {
                let sent = format!("Invite sent to {email}");
                crate::navigation::defer_in_result_window(window, cx, move |w, cx| {
                    crate::toast::success(sent, w, cx);
                });
                InviteOutcome::Delivered(url)
            } else {
                // Requested but not delivered (transport down or
                // unconfigured) — fall back to the link.
                crate::toast::show(
                    crate::action_error::failed_toast("email", "the invite", Some(EMAIL_FALLBACK_MESSAGE.into())),
                    window,
                    cx,
                );
                InviteOutcome::Undelivered(url)
            }
        }
        Err(err) if super::is_plan_limit(&err) => InviteOutcome::OutOfSeats,
        Err(api::ApiError::Http { status: 400, message }) => {
            InviteOutcome::Refused(crate::action_error::description_of(&message).unwrap_or_else(|| INVALID_EMAIL.to_string()))
        }
        Err(err) => {
            crate::action_error::failed_action_err("send", "the invite", &err, cx);
            InviteOutcome::Failed
        }
    }
}

impl Render for InviteForm {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "InviteForm").entered();
        if self.focus_email {
            self.focus_email = false;
            self.email_input
                .update(cx, |state, cx| state.focus(window, cx));
        }
        let email_empty = self.email_input.read(cx).value().trim().is_empty();
        let name_field = glass_input(&self.name_input, window, cx).web_input_sm();
        let email_field = glass_input(&self.email_input, window, cx).web_input_sm();
        let send_button = self.pending.apply(
            Button::new("invite-send")
                .primary()
                .web_sm()
                .label("Send invite")
                .icon(registry::UI_MAIL)
                .disabled(email_empty)
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
            crate::feedback::PENDING_INVITE,
            cx,
        );
        // A dialog column is too narrow for the fields and the button
        // abreast: one field per line, the button trailing (web
        // `layout="stack"`).
        let controls = v_flex()
            .gap_2()
            .child(name_field)
            .child(email_field)
            .child(h_flex().justify_end().child(send_button));

        let mut form = v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(controls)
            .children(
                self.email_error
                    .clone()
                    .map(|message| crate::action_error::field_error(message, cx)),
            );

        // The notices sit BETWEEN the row and the link (the email fallback's
        // toast says "copy the link below", so the link stays below the row).
        if self.out_of_seats {
            form = form.child(out_of_seats_notice(cx));
        }
        if let Some(url) = &self.invite_url {
            form = form.child(invite_link_row("invite-copy", url.clone(), cx));
        }
        form
    }
}

/// Web `InviteLinkRow`: the minted link, read-only, with a copy button (the
/// send-failure fallback here and the Members pane's own "Generate invite
/// link" path).
pub(super) fn invite_link_row(
    id: impl Into<ElementId>,
    url: SharedString,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px_2()
                .py_1()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(super::row_stroke(cx))
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(url.clone()),
        )
        .child(Clipboard::new(id).value(url).tooltip("Copy invite URL"))
}

/// The web's seat-cap `UpgradeDialog`, inline: its title as the lead line, its
/// description under it, and the desktop's one billing hand-off (§4.9 — seats
/// are bought on the web).
pub(super) fn out_of_seats_notice(cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().primary.opacity(0.4))
        .bg(cx.theme().primary.opacity(0.05))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(OUT_OF_SEATS_TITLE),
        )
        .child(div().text_sm().child(OUT_OF_SEATS_MESSAGE))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(OUT_OF_SEATS_NOTE),
        )
}
