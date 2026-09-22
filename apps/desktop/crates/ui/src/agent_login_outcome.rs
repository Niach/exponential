//! EXP-1000: the login OUTCOME — the ONE component that renders what a machine
//! handed back from an `agent_login` command, the desktop twin of the web
//! `AgentLoginOutcome` (`device-agent-account.tsx`), iOS `AgentLoginSheet`'s
//! `link` and Android `AgentLoginSheet`'s outcome block:
//!
//! * the CLI's sign-in link (open it on any device, copy it);
//! * codex's device code beside it, with its own copy button — that code goes
//!   INTO the browser;
//! * claude's way BACK (EXP-765): its link carries no code, the browser page
//!   ends by SHOWING one, and the CLI on the machine is still waiting for it
//!   at "Paste code here if prompted >". The field here hands it back as an
//!   `agent_login_code` command. Enter submits, like the pill;
//! * the caption under it, in the three ×4 shapes.
//!
//! The field went missing once already: EXP-765 put it in the device-settings
//! dialog's login note, EXP-862 replaced that note with the sign-in dialog's
//! "one status line" and rebuilt the link inline WITHOUT it — so a claude
//! re-login on another machine showed a link and nowhere to type the code.
//! Every desktop surface that shows a login link now renders THIS entity and
//! nothing of its own; the strings are byte-identical to the other three
//! clients.

use std::rc::Rc;

use gpui::{
    div, App, AppContext as _, Entity, Focusable as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};

use coding::CodingAgent;

use crate::controls::{ghost_icon_button, glass_input, WebControl as _, WebText as _};
use crate::icons::registry;
use crate::surface;

/// The code field's placeholder — web `placeholder="Code from the browser"`,
/// iOS/Android `GlassTextField("Code from the browser")`.
pub(crate) const CODE_PLACEHOLDER: &str = "Code from the browser";
/// The submit pill's label, ×4.
pub(crate) const ENTER_CODE: &str = "Enter code";

/// The caption under a published sign-in link, in the three shapes it takes
/// (web `AgentLoginOutcome`, iOS `linkCaption`): codex's code goes into the
/// browser, claude's comes back here, and a link that is neither gets the bare
/// instruction.
const CAPTION_CODE_ON_MACHINE: &str =
    "Open the link on any device and enter the code on the machine.";
const CAPTION_CODE_BACK: &str = "Open the link on any device, then paste the code it shows here.";
const CAPTION_LINK_ONLY: &str = "Open the link on any device.";

/// Whether a published link expects the browser's code BACK on the machine:
/// a link WITHOUT a device code is claude's (web `wantsCodeBack = !code`).
pub(crate) fn wants_code_back(code: Option<&str>) -> bool {
    code.is_none()
}

/// The caption for a link, keyed exactly like the web's ternary.
pub(crate) fn caption(has_code: bool, wants_code_back: bool) -> &'static str {
    if has_code {
        CAPTION_CODE_ON_MACHINE
    } else if wants_code_back {
        CAPTION_CODE_BACK
    } else {
        CAPTION_LINK_ONLY
    }
}

/// What the host does with a submitted code — queue the `agent_login_code`
/// command on the machine. Runs from INSIDE this entity's update, so a host
/// must not update this entity synchronously from it (gpui's double lease).
/// The host shows its own phase from here on ("Signing in…" → "Signed in" or
/// the error + "Try again"); a retry builds a fresh outcome.
pub(crate) type EnterCode = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// One machine's published sign-in link, rendered.
pub(crate) struct LoginOutcome {
    agent: CodingAgent,
    url: SharedString,
    /// Codex's device code; claude's flow has none.
    code: Option<SharedString>,
    /// EXP-765: where the code claude's browser page showed is pasted.
    code_input: Entity<InputState>,
    /// A code went to the host — the field and pill stay disabled so a double
    /// Enter never types the same code twice. One outcome, one code: the host
    /// shows the round trip's phases itself and rebuilds this on a retry.
    code_pending: bool,
    on_enter_code: EnterCode,
    _subscriptions: Vec<Subscription>,
}

impl LoginOutcome {
    pub(crate) fn new(
        agent: CodingAgent,
        url: impl Into<SharedString>,
        code: Option<String>,
        on_enter_code: EnterCode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let code_input = cx.new(|cx| InputState::new(window, cx).placeholder(CODE_PLACEHOLDER));
        let subscriptions = vec![cx.subscribe_in(
            &code_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )];
        let code: Option<SharedString> = code
            .map(|code| code.trim().to_string())
            .filter(|code| !code.is_empty())
            .map(SharedString::from);
        // The browser step ends with the code on the clipboard; the field
        // takes focus so the paste lands without a click first.
        if wants_code_back(code.as_deref()) {
            code_input.read(cx).focus_handle(cx).focus(window, cx);
        }
        Self {
            agent,
            url: url.into(),
            code,
            code_input,
            code_pending: false,
            on_enter_code,
            _subscriptions: subscriptions,
        }
    }

    /// Whether this link expects the browser's code back (claude's).
    pub(crate) fn wants_code_back(&self) -> bool {
        wants_code_back(self.code.as_deref())
    }

    /// Enter in the field, or the pill: a non-empty draft goes to the host
    /// and the field clears (web `submit`).
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.code_pending {
            return;
        }
        let code = self.code_input.read(cx).value().trim().to_string();
        if code.is_empty() {
            return;
        }
        self.code_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.code_pending = true;
        cx.notify();
        let enter = Rc::clone(&self.on_enter_code);
        enter(code, window, cx);
    }
}

impl Render for LoginOutcome {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let agent = self.agent.id();
        let has_code = self.code.is_some();
        let wants_code_back = self.wants_code_back();
        let typed = !self.code_input.read(cx).value().trim().is_empty();

        let open_url = self.url.clone();
        let copy_url = self.url.clone();
        let link = h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(self.url.clone()),
            )
            .child(
                ghost_icon_button(
                    SharedString::from(format!("agent-login-open-{agent}")),
                    Icon::new(registry::UI_EXTERNAL_LINK),
                    cx,
                )
                .tooltip("Open link")
                .on_click(move |_, _, cx| cx.open_url(&open_url)),
            )
            .child(
                ghost_icon_button(
                    SharedString::from(format!("agent-login-copy-{agent}")),
                    Icon::new(registry::UI_COPY),
                    cx,
                )
                .tooltip("Copy link")
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_url.to_string()));
                }),
            );

        let device_code = self.code.clone().map(|code| {
            let copy_code = code.clone();
            h_flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(code),
                )
                .child(
                    ghost_icon_button(
                        SharedString::from(format!("agent-login-copy-code-{agent}")),
                        Icon::new(registry::UI_COPY),
                        cx,
                    )
                    .tooltip("Copy code")
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            copy_code.to_string(),
                        ));
                    }),
                )
        });

        let code_entry = wants_code_back.then(|| {
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(
                    div().flex_1().min_w_0().child(
                        glass_input(&self.code_input, window, cx)
                            .web_input_sm()
                            .text_xs(),
                    ),
                )
                .child(
                    surface::glass_pill_button(
                        SharedString::from(format!("agent-login-enter-code-{agent}")),
                        surface::PillSize::Sm,
                        cx,
                    )
                    .icon(Icon::from(registry::UI_SIGN_IN))
                    .label(ENTER_CODE)
                    .disabled(!typed || self.code_pending)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                )
        });

        v_flex()
            .w_full()
            .gap_1()
            .child(link)
            .children(device_code)
            .children(code_entry)
            .child(
                div()
                    .text_2xs()
                    .text_color(muted)
                    .child(caption(has_code, wants_code_back)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A link without a device code is claude's: the browser hands one back,
    /// so the field shows. Codex's link carries its code and the field stays
    /// away — byte-for-byte the web's `wantsCodeBack = !code`.
    #[test]
    fn a_codeless_link_wants_the_code_back() {
        assert!(wants_code_back(None));
        assert!(!wants_code_back(Some("WXYZ-ABCD")));
    }

    /// The three captions the other clients print (web `AgentLoginOutcome`,
    /// iOS `linkCaption`), in the web ternary's order.
    #[test]
    fn captions_match_the_other_clients() {
        assert_eq!(
            caption(true, false),
            "Open the link on any device and enter the code on the machine."
        );
        assert_eq!(
            caption(false, true),
            "Open the link on any device, then paste the code it shows here."
        );
        assert_eq!(caption(false, false), "Open the link on any device.");
        // A code and a code-back never coincide, but the code wins as on web.
        assert_eq!(caption(true, true), caption(true, false));
    }

    #[test]
    fn field_strings_match_the_other_clients() {
        assert_eq!(CODE_PLACEHOLDER, "Code from the browser");
        assert_eq!(ENTER_CODE, "Enter code");
    }
}
