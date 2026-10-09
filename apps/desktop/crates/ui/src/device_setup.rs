//! The device-setup block's cards (EXP-1169), one spec with the web
//! `components/device-setup.tsx` and the two natives.
//!
//! The SERVER card is the same component on all four clients: header (server
//! icon + the getting-started server title), the server description, then
//! the install command in a box with an icon-only copy control. The command
//! carries a freshly minted one-time `EXP_INSTALL_TOKEN`
//! (`devices.createInstallToken`, reminted on expiry) so the new daemon signs
//! itself in; until the token lands, or when minting fails, the box holds the
//! plain command and the CLI prints a device code instead. Copying the
//! command reveals the one field that approves such a code in place (the
//! same claim + approve calls as the web `/auth/device` page).
//!
//! Hosts: the "Add device" dialog ([`crate::machines::open_add_server_dialog`],
//! which also shows [`desktop_card`] above it) and the onboarding wizard's
//! devices step (server card only: the user is already on the desktop app).
//!
//! The strings in [`copy`] are drift-gated: the web
//! `device-setup-copy.test.ts` reads this file and matches every literal.

use std::time::Duration;

use gpui::{
    div, px, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};

use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::queries;
use crate::session::AuthContext;

/// The card's words, byte-identical with web `DEVICE_SETUP_COPY`.
pub(crate) mod copy {
    pub const COPY_COMMAND: &str = "Copy install command";
    pub const CODE_LABEL: &str = "If the CLI shows a code, enter it here";
    pub const CODE_PLACEHOLDER: &str = "XXXX-XXXX";
    pub const APPROVE: &str = "Approve";
    pub const APPROVED: &str = "Code approved. The CLI signs in within a few seconds.";
    pub const CODE_USED: &str = "That code has already been used. Run the login command again.";
    pub const CODE_EXPIRED: &str =
        "That code has expired. Run the login command again to get a new one.";
    pub const CODE_INVALID: &str =
        "That code isn't valid. Check for typos, or run the login command again.";
    pub const CODE_OTHER_ACCOUNT: &str = "This code was requested from a different account.";
    pub const FAILED: &str = "Something went wrong. Try again.";
}

/// How long the copy control shows the check after a click.
const COPIED_FLASH: Duration = Duration::from_millis(1_500);

const INSTALL_PREFIX: &str = "curl -fsSL https://exponential.at/install.sh |";

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// The install command as it goes to the clipboard: ONE line (web
/// `buildServerInstallSnippet`). The script is served by the cloud marketing
/// site for every instance, so the command always names its instance.
pub(crate) fn install_command(origin: &str, token: Option<&str>) -> String {
    let token_part = token
        .map(|token| format!(" EXP_INSTALL_TOKEN={token}"))
        .unwrap_or_default();
    format!("{INSTALL_PREFIX} EXP_INSTANCE={origin}{token_part} sh")
}

/// The same command on fixed lines for the box (never a horizontal scroll).
pub(crate) fn install_command_lines(origin: &str, token: Option<&str>) -> Vec<String> {
    match token {
        Some(token) => vec![
            INSTALL_PREFIX.to_string(),
            format!("  EXP_INSTANCE={origin} \\"),
            format!("  EXP_INSTALL_TOKEN={token} sh"),
        ],
        None => vec![
            INSTALL_PREFIX.to_string(),
            format!("  EXP_INSTANCE={origin} sh"),
        ],
    }
}

/// Web `normalizeUserCode`: uppercase, A-Z and 0-9 only, at most 8, the dash
/// after the 4th only once a 5th exists (so backspacing over it deletes).
pub(crate) fn normalize_user_code(input: &str) -> String {
    let bare: String = input
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        .take(8)
        .collect();
    if bare.len() >= 5 {
        format!("{}-{}", &bare[..4], &bare[4..])
    } else {
        bare
    }
}

/// A complete code: 8 characters once the dash is stripped.
pub(crate) fn is_complete_user_code(code: &str) -> bool {
    code.chars().filter(|c| *c != '-').count() == 8
}

/// Web `deviceErrorMessage`, keyed by the RFC 8628 `error` field.
pub(crate) fn device_error_message(error: &str) -> &'static str {
    match error {
        "expired_token" => copy::CODE_EXPIRED,
        "invalid_request" | "invalid_grant" => copy::CODE_INVALID,
        "access_denied" => copy::CODE_OTHER_ACCOUNT,
        _ => copy::FAILED,
    }
}

/// The approve flow over its two answers: the claim GET, then (only when the
/// code is still pending) the approve POST. `Err` = the sentence to show.
pub(crate) fn approval_outcome(
    claim: Result<api::login::DeviceCodeAnswer, api::ApiError>,
    approve: impl FnOnce() -> Result<api::login::DeviceCodeAnswer, api::ApiError>,
) -> Result<(), &'static str> {
    use api::login::DeviceCodeAnswer;
    match claim {
        Err(_) => return Err(copy::FAILED),
        Ok(DeviceCodeAnswer::Refused { error }) => return Err(device_error_message(&error)),
        Ok(DeviceCodeAnswer::Accepted { status }) => {
            if matches!(status.as_deref(), Some("approved") | Some("denied")) {
                return Err(copy::CODE_USED);
            }
        }
    }
    match approve() {
        Err(_) => Err(copy::FAILED),
        Ok(DeviceCodeAnswer::Refused { error }) => Err(device_error_message(&error)),
        Ok(DeviceCodeAnswer::Accepted { .. }) => Ok(()),
    }
}

/// How long until a token minted with `expires_at` (ISO-8601) lapses; `None`
/// when the stamp does not parse (the token then simply stays).
fn remint_delay(expires_at: &str, now: chrono::DateTime<chrono::Utc>) -> Option<Duration> {
    let expires = chrono::DateTime::parse_from_rfc3339(expires_at).ok()?;
    let millis = (expires.with_timezone(&chrono::Utc) - now)
        .num_milliseconds()
        .max(0);
    Some(Duration::from_millis(millis as u64))
}

// ---------------------------------------------------------------------------
// The desktop card (Add device dialog only)
// ---------------------------------------------------------------------------

/// Card header: concept icon + title (both cards).
fn card_header(icon: crate::icons::ExpIcon, title: &'static str) -> impl IntoElement {
    h_flex()
        .gap_2()
        .text_sm()
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(Icon::new(icon).size_4().flex_shrink_0())
        .child(title)
}

fn card_description(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn card_shell() -> gpui::Div {
    crate::surface::glass_row_card().p_3().flex().flex_col().gap_3()
}

/// The desktop card's secondary button (web `device-setup.tsx`).
pub(crate) const ALL_PLATFORMS: &str = "All platforms";

/// The desktop download card: header, description, the download buttons.
pub(crate) fn desktop_card(cx: &App) -> impl IntoElement {
    use crate::getting_started::copy as gs;
    card_shell()
        .child(card_header(registry::UI_DEVICE, gs::DESKTOP_TITLE))
        .child(card_description(gs::DESKTOP_DESCRIPTION, cx))
        .child(
            // The web card ×4: the primary download for THIS OS, then the
            // outline "All platforms" (the releases page).
            h_flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("device-setup-download")
                        .primary()
                        .web_sm()
                        .icon(Icon::new(registry::UI_DOWNLOAD))
                        .label(gs::DESKTOP_ACTION)
                        .on_click(|_, _, cx| {
                            crate::settings::open_url(cx, crate::machines::desktop_download_url());
                        }),
                )
                .child(
                    Button::new("device-setup-all-platforms")
                        .outline()
                        .web_sm()
                        .label(ALL_PLATFORMS)
                        .on_click(|_, _, cx| {
                            crate::settings::open_url(
                                cx,
                                crate::machines::DESKTOP_RELEASES_URL.to_string(),
                            );
                        }),
                ),
        )
}

// ---------------------------------------------------------------------------
// The server card
// ---------------------------------------------------------------------------

pub(crate) struct ServerCard {
    /// The one-time install token, once minted (`None` = plain command).
    token: Option<String>,
    /// Bumped per mint, so a stale answer never lands.
    mint_generation: u64,
    _mint: Option<Task<()>>,
    _remint: Option<Task<()>>,
    /// The copy control shows the check while true.
    copied_flash: bool,
    _flash: Option<Task<()>>,
    /// The code field exists only once the command was copied.
    copied_once: bool,
    code: Entity<InputState>,
    busy: bool,
    error: Option<SharedString>,
    approved: bool,
    _subscriptions: Vec<Subscription>,
}

impl ServerCard {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let code =
            cx.new(|cx| InputState::new(window, cx).placeholder(copy::CODE_PLACEHOLDER));
        let subscriptions = vec![cx.subscribe_in(
            &code,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = this.code.read(cx).value().to_string();
                    let normalized = normalize_user_code(&value);
                    if normalized != value {
                        this.code
                            .update(cx, |state, cx| state.set_value(normalized, window, cx));
                    }
                    this.error = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.approve(cx),
                _ => {}
            },
        )];
        let mut this = Self {
            token: None,
            mint_generation: 0,
            _mint: None,
            _remint: None,
            copied_flash: false,
            _flash: None,
            copied_once: false,
            code,
            busy: false,
            error: None,
            approved: false,
            _subscriptions: subscriptions,
        };
        this.mint(cx);
        this
    }

    /// Mint a token off the UI thread. Every failure (offline, 429) is
    /// silent: the box keeps the plain command.
    fn mint(&mut self, cx: &mut gpui::Context<Self>) {
        self.mint_generation = self.mint_generation.wrapping_add(1);
        let generation = self.mint_generation;
        self._remint = None;
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self._mint = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::create_install_token(&trpc) })
                .await;
            let _ = this.update(cx, |this: &mut Self, cx| {
                if this.mint_generation != generation {
                    return;
                }
                match result {
                    Ok(minted) => {
                        this.token = Some(minted.token);
                        this.schedule_remint(&minted.expires_at, cx);
                    }
                    Err(err) => {
                        log::debug!("[ui] devices.createInstallToken failed: {err}");
                        this.token = None;
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Remint once the token lapses, so the box never shows a dead command.
    fn schedule_remint(&mut self, expires_at: &str, cx: &mut gpui::Context<Self>) {
        let Some(delay) = remint_delay(expires_at, chrono::Utc::now()) else {
            return;
        };
        self._remint = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this: &mut Self, cx| this.mint(cx));
        }));
    }

    fn copy_command(&mut self, cx: &mut gpui::Context<Self>) {
        let origin = crate::machines::server_install_origin(cx);
        cx.write_to_clipboard(ClipboardItem::new_string(install_command(
            &origin,
            self.token.as_deref(),
        )));
        self.copied_once = true;
        self.copied_flash = true;
        self._flash = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FLASH).await;
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.copied_flash = false;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn approve(&mut self, cx: &mut gpui::Context<Self>) {
        let code = self.code.read(cx).value().to_string();
        if self.busy || self.approved || !is_complete_user_code(&code) {
            return;
        }
        let Some((instance_url, token, client)) = credentials(cx) else {
            self.error = Some(copy::FAILED.into());
            cx.notify();
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    approval_outcome(client.claim_device_code(&instance_url, &token, &code), || {
                        client.approve_device_code(&instance_url, &token, &code)
                    })
                })
                .await;
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.busy = false;
                match result {
                    Ok(()) => this.approved = true,
                    Err(message) => this.error = Some(message.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The command box (the EXP-725 box's look): mono lines, copy control
    /// in its top-right corner.
    fn command_box(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let origin = crate::machines::server_install_origin(cx);
        let lines = install_command_lines(&origin, self.token.as_deref());
        let glyph = if self.copied_flash {
            registry::UI_CHECK
        } else {
            registry::UI_COPY
        };
        div()
            .relative()
            .p_2()
            .pr_8()
            .rounded(px(theme::tokens::radius::SM))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .text_xs()
            .font_family(theme::terminal::FONT_FAMILY)
            .text_color(cx.theme().foreground)
            .child(v_flex().children(lines.into_iter().map(SharedString::from)))
            .child(
                div().absolute().top_1().right_1().child(
                    // EXP-862: a copy glyph on a row is ghost chrome, not a circle.
                    crate::controls::ghost_icon_button(
                        "device-setup-copy",
                        Icon::new(glyph),
                        cx,
                    )
                    .tooltip(copy::COPY_COMMAND)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_command(cx))),
                ),
            )
    }
}

impl Render for ServerCard {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        use crate::getting_started::copy as gs;
        let mut card = card_shell()
            .child(card_header(registry::UI_SERVER, gs::SERVER_TITLE))
            .child(card_description(gs::SERVER_DESCRIPTION, cx))
            .child(self.command_box(cx));
        if self.approved {
            card = card.child(div().text_sm().child(copy::APPROVED));
        } else if self.copied_once {
            let incomplete = !is_complete_user_code(&self.code.read(cx).value());
            let mut form = v_flex()
                .gap_2()
                .child(div().text_sm().child(copy::CODE_LABEL))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            div().flex_1().min_w_0().child(
                                glass_input(&self.code, window, cx)
                                    .web_input()
                                    .font_family(theme::terminal::FONT_FAMILY),
                            ),
                        )
                        .child(
                            Button::new("device-setup-approve")
                                .primary()
                                .cursor_pointer()
                                .web_sm()
                                .label(copy::APPROVE)
                                .loading(self.busy)
                                .disabled(self.busy || incomplete)
                                .on_click(cx.listener(|this, _, _, cx| this.approve(cx))),
                        ),
                );
            if let Some(error) = self.error.clone() {
                form = form.child(div().text_sm().text_color(cx.theme().danger).child(error));
            }
            card = card.child(form);
        }
        card
    }
}

/// The signed-in account's instance, session token and auth client.
fn credentials(cx: &App) -> Option<(String, String, std::sync::Arc<api::AuthClient>)> {
    let auth = cx.try_global::<AuthContext>()?;
    let account = queries::active_account(cx)?;
    let token = auth.auth.token(&account.id)?;
    Some((account.instance_url, token, auth.client.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::login::DeviceCodeAnswer;

    const ORIGIN: &str = "https://app.exponential.at";

    #[test]
    fn device_setup_install_command_is_one_line() {
        assert_eq!(
            install_command(ORIGIN, Some("expi_abc")),
            "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://app.exponential.at EXP_INSTALL_TOKEN=expi_abc sh"
        );
        assert_eq!(
            install_command(ORIGIN, None),
            "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://app.exponential.at sh"
        );
    }

    #[test]
    fn device_setup_install_command_lines_are_fixed() {
        assert_eq!(
            install_command_lines(ORIGIN, Some("expi_abc")),
            vec![
                "curl -fsSL https://exponential.at/install.sh |".to_string(),
                "  EXP_INSTANCE=https://app.exponential.at \\".to_string(),
                "  EXP_INSTALL_TOKEN=expi_abc sh".to_string(),
            ]
        );
        assert_eq!(
            install_command_lines(ORIGIN, None),
            vec![
                "curl -fsSL https://exponential.at/install.sh |".to_string(),
                "  EXP_INSTANCE=https://app.exponential.at sh".to_string(),
            ]
        );
    }

    #[test]
    fn device_setup_normalizes_user_codes() {
        assert_eq!(normalize_user_code("zp3hv7hk"), "ZP3H-V7HK");
        assert_eq!(normalize_user_code("zp3h"), "ZP3H");
        assert_eq!(normalize_user_code("ZP3H-"), "ZP3H");
        assert_eq!(normalize_user_code("zp3hv"), "ZP3H-V");
        assert_eq!(normalize_user_code("ZP3H-V7HK99"), "ZP3H-V7HK");
        // Idempotent: the field re-normalizes its own output on every change.
        assert_eq!(normalize_user_code("ZP3H-V7HK"), "ZP3H-V7HK");
    }

    #[test]
    fn device_setup_code_completeness() {
        assert!(is_complete_user_code("ZP3H-V7HK"));
        assert!(is_complete_user_code("ZP3HV7HK"));
        assert!(!is_complete_user_code("ZP3H-V7H"));
        assert!(!is_complete_user_code("ZP3H"));
        assert!(!is_complete_user_code(""));
    }

    #[test]
    fn device_setup_maps_device_errors() {
        assert_eq!(device_error_message("expired_token"), copy::CODE_EXPIRED);
        assert_eq!(device_error_message("invalid_request"), copy::CODE_INVALID);
        assert_eq!(device_error_message("invalid_grant"), copy::CODE_INVALID);
        assert_eq!(device_error_message("access_denied"), copy::CODE_OTHER_ACCOUNT);
        assert_eq!(device_error_message("slow_down"), copy::FAILED);
        assert_eq!(device_error_message(""), copy::FAILED);
    }

    fn accepted(status: Option<&str>) -> Result<DeviceCodeAnswer, api::ApiError> {
        Ok(DeviceCodeAnswer::Accepted { status: status.map(str::to_string) })
    }

    fn refused(error: &str) -> Result<DeviceCodeAnswer, api::ApiError> {
        Ok(DeviceCodeAnswer::Refused { error: error.to_string() })
    }

    #[test]
    fn device_setup_approval_outcomes() {
        assert_eq!(approval_outcome(accepted(Some("pending")), || accepted(None)), Ok(()));
        assert_eq!(
            approval_outcome(accepted(Some("approved")), || panic!("no approve")),
            Err(copy::CODE_USED)
        );
        assert_eq!(
            approval_outcome(accepted(Some("denied")), || panic!("no approve")),
            Err(copy::CODE_USED)
        );
        assert_eq!(
            approval_outcome(refused("invalid_request"), || panic!("no approve")),
            Err(copy::CODE_INVALID)
        );
        assert_eq!(
            approval_outcome(accepted(Some("pending")), || refused("expired_token")),
            Err(copy::CODE_EXPIRED)
        );
        assert_eq!(
            approval_outcome(accepted(Some("pending")), || refused("access_denied")),
            Err(copy::CODE_OTHER_ACCOUNT)
        );
        let offline = || {
            Err(api::ApiError::Transport {
                message: "down".to_string(),
                offline: true,
            })
        };
        assert_eq!(approval_outcome(offline(), || panic!("no approve")), Err(copy::FAILED));
        assert_eq!(approval_outcome(accepted(Some("pending")), offline), Err(copy::FAILED));
    }

    #[test]
    fn device_setup_remints_at_expiry() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-02T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(
            remint_delay("2026-10-02T12:15:00.000Z", now),
            Some(Duration::from_secs(900))
        );
        assert_eq!(remint_delay("2026-10-02T11:00:00Z", now), Some(Duration::ZERO));
        assert_eq!(remint_delay("garbage", now), None);
    }
}
