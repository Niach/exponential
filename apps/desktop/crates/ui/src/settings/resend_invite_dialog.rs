//! Settings → Members → "Resend invite" (EXP-630).
//!
//! Web parity: the `Dialog` half of `components/team/members-section.tsx` — a
//! placeholder member is on the roster but has not joined, so the row's
//! overflow menu offers a FRESH link, at a corrected address if the first one
//! was wrong. The form is web's `InviteMemberForm` in `layout="stack"`,
//! prefilled and bound to that member (`placeholder_user_id`), so a resend
//! keeps the row's attributions instead of minting a second person.
//!
//! EXP-1076: for a row the Linear import seated without ever mailing
//! ([`PlaceholderStatus::Unsent`]) this is a FIRST send — the title and the
//! copy say "Send invite" and "has never been invited"; the form and the call
//! underneath are the same.
//!
//! Its own WINDOW rather than an [`crate::native_dialog::AlertSpec`]: the form
//! re-renders on its own state (spinner, notices, the fallback link) and alert
//! content is built once.
//!
//! #EXP-1629: the content is a document on the native binding (projection at
//! `/pane`), the Members pane's own form parts in the dialog's stacked
//! layout; the window around it stays native.

use std::rc::Rc;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, px, size, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window};
use serde_json::{json, Value};

use domain::placeholder_status::PlaceholderStatus;

use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

use super::doc::{ide_plugin, mount_with, path, IdeToasts, Ink, SM};
use super::invite_form::{invite_outcome, InviteOutcome, ResendTarget};
use super::members::{copy_glyph, copy_icon, link_row, seats_notice, text_button, ButtonPaint, Tints};

/// Open the dialog over the Members pane. `display_name` leads the
/// description (web `resendTarget.displayName` / `.name`); `status` picks the
/// title and the copy (web `resendTarget.status`).
pub(super) fn open(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    display_name: String,
    status: PlaceholderStatus,
    target: ResendTarget,
) {
    let spec = DialogSpec::new(status.invite_verb(), size(px(420.), px(260.)))
        .resizable(size(px(360.), px(220.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| {
            ResendInviteDialogView::new(team_id, display_name, status, target, window, cx)
        });
        let busy = view.clone();
        DialogContent::new(view).can_close(move |cx| !busy.read(cx).sending)
    });
}

/// The IDE's plugin with the link row's copy / check glyphs.
struct InviteHost {
    ide: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for InviteHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        copy_glyph(name).or_else(|| self.ide.icon(name))
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.ide.on_unknown(node)
    }
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct ResendShot {
    pub status: PlaceholderStatus,
    pub name: &'static str,
    pub email: &'static str,
    pub sending: bool,
    pub error: Option<&'static str>,
    pub out_of_seats: bool,
    pub link: Option<&'static str>,
}

/// The placeholder row the pictures open the dialog on.
#[cfg(test)]
pub(crate) const SHOT_NAME: &str = "Ada Lovelace";
#[cfg(test)]
pub(crate) const SHOT_EMAIL: &str = "ada@example.com";

pub(crate) struct ResendInviteDialogView {
    team_id: String,
    /// The placeholder member this re-invites: the row keeps its attributions
    /// and the address may be corrected.
    user_id: String,
    description: String,
    /// Optional display name (EXP-630) — empty lets the server fall back to
    /// the mailbox local part.
    name: String,
    email: String,
    sending: bool,
    /// EXP-1259 D8: the Send button's pending face ("Inviting").
    pending: crate::feedback::Pending,
    /// FEED-79: the address the server refused — INVALID INPUT, so it stays
    /// under the field until the address changes; every other failure is a
    /// toast.
    email_error: Option<String>,
    /// The seat cap rejected the invite — the web's "Out of seats" notice.
    out_of_seats: bool,
    /// The minted link: the manual fallback when delivery failed.
    invite_url: Option<String>,
    /// The copy button shows its check (for two seconds after a copy).
    copied: bool,
    copy_generation: u64,
    /// Web `autoFocus="email"`: a resend opens ON the address. The field
    /// exists only after the paint that first shows it, so the focus is
    /// retried after every render until it lands.
    want_focus: bool,
    /// Renders left to retry the focus (a field that never shows must not
    /// keep the window rendering).
    focus_tries: u8,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
}

/// Renders the email's focus is retried for.
const FOCUS_TRIES: u8 = 4;

impl ResendInviteDialogView {
    fn new(
        team_id: String,
        display_name: String,
        status: PlaceholderStatus,
        target: ResendTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // Every action leaves the binding for the view (a send's completion
        // and the window's close need its window).
        let me = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/pane", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["edit", "send", "copy"] {
                let me = me.clone();
                binding.on_action(name, move |_, event, cx| {
                    let field = event.context.get("field").and_then(Value::as_str).unwrap_or_default().to_string();
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let me = me.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = me.update(cx, |view, cx| view.action(name, &field, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let view = mount_with("resend-invite", document(&Ink::of(cx), &Tints::of(cx)), &binding, Rc::new(InviteHost { ide: ide_plugin() }), true, window, cx);
        let mut this = Self {
            team_id,
            user_id: target.user_id,
            // Web copy, word for word.
            description: resend_description(display_name.as_str(), status),
            name: target.name,
            email: target.email,
            sending: false,
            pending: crate::feedback::Pending::default(),
            email_error: None,
            out_of_seats: false,
            invite_url: None,
            copied: false,
            copy_generation: 0,
            want_focus: true,
            focus_tries: FOCUS_TRIES,
            binding,
            view,
        };
        this.push(cx);
        this
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let target = ResendTarget { user_id: "u-ada".into(), name: SHOT_NAME.into(), email: SHOT_EMAIL.into() };
        Self::new("t".into(), SHOT_NAME.into(), PlaceholderStatus::Pending, target, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn surface(&self) -> Entity<SurfaceView> {
        self.view.clone()
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: ResendShot, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.description = resend_description(SHOT_NAME, shot.status);
        // The field lets the shot's text in only while unfocused (a focused
        // field keeps what the user typed): blur, write, refocus.
        self.view.update(cx, |view, cx| view.blur_field("email.field", window, cx));
        (self.want_focus, self.focus_tries) = (true, FOCUS_TRIES);
        self.name = shot.name.to_string();
        self.email = shot.email.to_string();
        if shot.sending {
            self.sending = true;
            self.pending.start(cx);
        }
        self.email_error = shot.error.map(str::to_string);
        self.out_of_seats = shot.out_of_seats;
        self.invite_url = shot.link.map(str::to_string);
        cx.notify();
    }

    /// A document action: a field's `edit` (field in the context, text in
    /// the payload), `send`, `copy`.
    fn action(&mut self, name: &str, field: &str, value: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "edit" => match field {
                "name" => self.name = value,
                "email" => {
                    if value != self.email {
                        self.email_error = None;
                    }
                    self.email = value;
                }
                _ => return,
            },
            // Web: Enter in the address field sends.
            "send" => self.send(window, cx),
            "copy" => self.copy(cx),
            _ => return,
        }
        cx.notify();
    }

    /// gpui-component's `Clipboard`: the link to the clipboard, the button's
    /// check for two seconds (no second copy meanwhile).
    fn copy(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(url) = self.invite_url.clone().filter(|_| !self.copied) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        self.copied = true;
        self.copy_generation += 1;
        let generation = self.copy_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_generation == generation {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// `teamInvites.create` with an address, bound to the placeholder: the
    /// server mails the link and reports delivery. Delivery is best-effort —
    /// with no transport configured (or on a failed send) the link is shown
    /// so the owner can share it by hand.
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
        let email = self.email.trim().to_string();
        if email.is_empty() {
            return;
        }
        let name = self.name.trim().to_string();
        let placeholder = self.user_id.clone();
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
                            placeholder_user_id: Some(placeholder.as_str()),
                        },
                    )
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.sending = false;
                this.pending.finish();
                match invite_outcome(result, &base, &email, window, cx) {
                    // A delivered link is the whole errand — the dialog
                    // closes on it (web `onInvited`); a failed delivery keeps
                    // it open around the link to copy.
                    InviteOutcome::Delivered(_) => native_dialog::close_dialog_window(window, cx),
                    InviteOutcome::Undelivered(url) => this.invite_url = Some(url),
                    InviteOutcome::OutOfSeats => this.out_of_seats = true,
                    InviteOutcome::Refused(message) => this.email_error = Some(message),
                    InviteOutcome::Failed => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The state as the document binds it.
    fn project(&self) -> Value {
        json!({
            "description": self.description,
            "fields": {"name": self.name, "email": self.email},
            // The send button's faces: disabled while the address is empty;
            // pending while it sends (the word after `PENDING_DELAY`).
            "sendDisabled": self.email.trim().is_empty(),
            "sending": self.sending,
            "sendShown": self.pending.shown(),
            "sendLabel": if self.pending.shown() { crate::feedback::PENDING_INVITE } else { "Send invite" },
            "hasError": self.email_error.is_some(),
            "error": self.email_error.clone().unwrap_or_default(),
            "outOfSeats": self.out_of_seats,
            "hasLink": self.invite_url.is_some(),
            "link": self.invite_url.clone().unwrap_or_default(),
            "copyIcon": copy_icon(self.copied),
        })
    }

    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let projection = self.project();
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
    }
}

impl Render for ResendInviteDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ResendInviteDialogView").entered();
        self.push(cx);
        if self.want_focus && self.focus_tries > 0 {
            self.focus_tries -= 1;
            let view = self.view.clone();
            cx.defer_in(window, move |this, window, cx| {
                // Focus once the field holds its bound text: a focused field
                // keeps its own text over a later write.
                let ready = view.read(cx).field_text(EMAIL_FIELD, cx).as_ref() == Some(&this.email);
                if ready && view.update(cx, |view, cx| view.focus_field(EMAIL_FIELD, window, cx)) {
                    this.want_focus = false;
                } else if this.focus_tries > 0 {
                    cx.notify();
                }
            });
        }
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The address `Input`'s text field (its `<id>.field` part node).
const EMAIL_FIELD: &str = "email.field";

/// Web `DialogDescription`, word for word: a first send for a never-invited
/// row, a fresh link else.
fn resend_description(display_name: &str, status: PlaceholderStatus) -> String {
    match status {
        PlaceholderStatus::Unsent => format!(
            "{display_name} is on the team but has never been invited. Send them a link."
        ),
        PlaceholderStatus::Pending | PlaceholderStatus::Expired => format!(
            "{display_name} is on the team but has not joined yet. Send a \
             fresh link — fix the address first if it was wrong."
        ),
    }
}

/// The dialog's tree (`gap_3`): the description, then the form (web
/// `layout="stack"`, `gap_2`: one field per line and the button trailing, the
/// refused address under them, the seat notice, the fallback link).
fn document(ink: &Ink, tints: &Tints) -> Value {
    let description = ink.text("description", path("/pane/description"), SM, &ink.muted);
    let input = |id: &str, label: &str, placeholder: &str, submit: bool| {
        let mut node = json!({"id": id, "component": "Input", "props": {"label": label, "name": id, "variant": "box", "size": "sm", "placeholder": placeholder, "value": path(&format!("/pane/fields/{id}"))},
            "style": {"width": "100%"}, "on": {"change": {"event": {"name": "edit", "context": {"field": id}}}}});
        if submit {
            node["on"]["submit"] = json!({"event": {"name": "send"}});
        }
        node
    };
    let send = text_button(ink, tints, "send", ButtonPaint::Primary, "ui-mail", path("/pane/sendLabel"), path("/pane/sendDisabled"), path("/pane/sending"), path("/pane/sendShown"), json!({"name": "send"}), Some("Send invite"));
    let actions = json!({"id": "actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "width": "100%"}, "children": [send]});
    let error = json!({"id": "form_error", "component": "Text", "visible": path("/pane/hasError"), "props": {"text": path("/pane/error")},
        "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": tints.danger}});
    let column = |id: &str, gap: f32, children: Vec<Value>| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(gap), "width": "100%", "minWidth": 0}, "children": children});
    let controls = column("controls", 0.5, vec![input("name", "Name", "Name", false), input("email", "Email", "teammate@example.com", true), actions]);
    let form = column("form", 0.5, vec![controls, error, seats_notice(ink, tints, "seats", path("/pane/outOfSeats")), link_row(ink, tints, "link", "/pane", "form", path("/pane/hasLink"))]);
    column("root", 0.75, vec![description, form])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let tree = document(&super::super::readiness::tests::test_ink(), &Tints::sample());
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("resend-invite", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn copy_branches_on_the_placeholder_state_like_the_web_dialog() {
        assert_eq!(
            resend_description("Ada", PlaceholderStatus::Unsent),
            "Ada is on the team but has never been invited. Send them a link."
        );
        assert_eq!(
            resend_description("Ada", PlaceholderStatus::Expired),
            "Ada is on the team but has not joined yet. Send a fresh link — fix the \
             address first if it was wrong."
        );
        assert_eq!(
            resend_description("Ada", PlaceholderStatus::Pending),
            resend_description("Ada", PlaceholderStatus::Expired)
        );
    }
}
