//! EXP-909 — the three ACCOUNT writes a login row offers, wherever it is
//! read: make this login the device's default, remove the device's copy of
//! it, and the way into that device's settings.
//!
//! They were the Accounts section's (EXP-818/862) until the Devices page
//! absorbed it: the logins now hang under their own device row
//! ([`crate::machines`]), and the writes came along unchanged. Each one is
//! the SAME body whether it is asked for on this machine (in-process, over
//! `crate::device_sync`) or on another of mine (a `devices` command it picks
//! up off its heartbeat and answers by re-reporting), so a row's menu means
//! one thing everywhere.
//!
//! Never a LOGOUT and never a credential copy: `codex logout` would revoke an
//! account every machine shares, and the credential files stay where the CLI
//! wrote them.

use gpui::{App, SharedString, Window};
use gpui_component::{button::ButtonVariant, notification::Notification, WindowExt as _};

use coding::CodingAgent;

use crate::queries;
use crate::usage_bar::remove_account_confirm;

/// The device row's menu entry AND the login rows' way into it — one string
/// ×4 (it replaced "Edit" with EXP-862).
pub(crate) const DEVICE_SETTINGS: &str = "Device settings";

/// EXP-849 — "Set as default": make this login the device's DEFAULT for its
/// agent. Non-destructive — it moves a device-local pointer and signs nobody
/// out, which is why it is offered beside (never instead of) the sign-in.
///
/// This device writes the pointer directly; another of mine gets the
/// `agent_profile_use` command on its heartbeat and answers by re-reporting,
/// so the CHECK moves on the next beat either way.
pub(crate) fn use_account_here(
    device_id: String,
    device_label: String,
    own: bool,
    agent: CodingAgent,
    profile_id: String,
    window: &mut Window,
    cx: &mut App,
) {
    if own {
        // The SAME body the `agent_profile_use` command runs
        // (`coding::use_profile`): same signed-in check, same sentences, same
        // re-probe — so "Set as default" means one thing whether it was asked
        // for on this device or from another client.
        match crate::device_sync::use_agent_profile_here(agent, &profile_id, cx) {
            Ok(()) => window.push_notification(
                Notification::success(SharedString::from(format!(
                    "{} now runs as this account here.",
                    agent.label()
                ))),
                cx,
            ),
            Err(err) => {
                window.push_notification(Notification::error(SharedString::from(err)), cx)
            }
        }
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                api::devices::create_agent_profile_use_command(
                    &trpc,
                    &device_id,
                    agent.id(),
                    &profile_id,
                )
            })
            .await;
        let _ = handle.update(cx, |_, window, cx| match result {
            Ok(_) => window.push_notification(
                Notification::success(SharedString::from(format!(
                    "{device_label} will run {} as this account.",
                    agent.label()
                ))),
                cx,
            ),
            Err(err) => window.push_notification(
                Notification::error(SharedString::from(err.user_message())),
                cx,
            ),
        });
    })
    .detach();
}

/// EXP-862 — "Remove account": delete the DEVICE's copy of a login (the
/// profile's config dir, credentials included, and its index row). The ACCOUNT
/// is untouched — the device never runs `codex logout`, which would revoke it
/// server-wide — which is exactly what the confirm says.
///
/// Destructive, so it confirms first with the pinned ×4 sentence
/// ([`remove_account_confirm`]). This device removes it directly; another of
/// mine gets the `agent_profile_remove` command on its heartbeat and answers
/// by re-reporting, so the login row goes on the next beat either way.
#[allow(clippy::too_many_arguments)]
pub(crate) fn remove_account(
    device_id: String,
    device_label: SharedString,
    own: bool,
    agent: CodingAgent,
    profile_id: String,
    account_label: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    let handle = window.window_handle();
    let spec = crate::native_dialog::AlertSpec::new(
        "Remove account",
        remove_account_confirm(&account_label, &device_label),
        "Remove",
    )
    .ok_variant(ButtonVariant::Danger)
    .on_ok(move |window, cx| {
        if own {
            match crate::device_sync::remove_agent_profile_here(agent, &profile_id, cx) {
                Ok(()) => window.push_notification(
                    Notification::success(SharedString::from(format!(
                        "{account_label} removed from this device."
                    ))),
                    cx,
                ),
                Err(err) => {
                    window.push_notification(Notification::error(SharedString::from(err)), cx)
                }
            }
            return true;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return true;
        };
        let device_id = device_id.clone();
        let profile_id = profile_id.clone();
        let device_label = device_label.clone();
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::create_agent_profile_remove_command(
                        &trpc,
                        &device_id,
                        agent.id(),
                        &profile_id,
                    )
                })
                .await;
            let _ = handle.update(cx, |_, window, cx| match result {
                Ok(_) => window.push_notification(
                    Notification::success(SharedString::from(format!(
                        "{device_label} will remove this login."
                    ))),
                    cx,
                ),
                Err(err) => window.push_notification(
                    Notification::error(SharedString::from(err.user_message())),
                    cx,
                ),
            });
        })
        .detach();
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
}
