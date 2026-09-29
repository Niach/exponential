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
//! Never `codex logout` and never a credential copy: `codex logout` would
//! revoke an account every machine shares, and the credential files stay
//! where the CLI wrote them. EXP-1137's sign-out is the agent's OWN way out
//! (claude's `auth logout` in that config dir, codex's credential file
//! deleted), and it is what removing the machine's own AMBIENT login does
//! first, before hiding the row.

use gpui::{App, SharedString, Window};
use gpui_component::{button::ButtonVariant, notification::Notification, WindowExt as _};

use coding::CodingAgent;

use crate::queries;
use crate::usage_bar::{remove_account_confirm, remove_ambient_account_confirm, sign_out_confirm};

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
    // EXP-1137: the ambient login is signed out there and hidden, and the
    // sentence says so (the CLI in the person's own terminal goes with it).
    let ambient = coding::agent_profiles::is_system(Some(&profile_id));
    let confirm = if ambient {
        remove_ambient_account_confirm(&account_label, &device_label, agent.label())
    } else {
        remove_account_confirm(&account_label, &device_label)
    };
    confirmed_account_change(
        AccountChange {
            device_id,
            device_label,
            own,
            agent,
            profile_id,
            account_label,
            title: "Remove account",
            confirm,
            ok_label: "Remove",
            local_done: "removed from this device.",
            remote_done: "will remove this login.",
            remove: true,
        },
        window,
        cx,
    );
}

/// EXP-1137 — "Sign out": sign the DEVICE's copy of a login out and keep its
/// row (claude's own `auth logout` in that profile's config dir, codex's
/// credential file deleted — never `codex logout`). The ACCOUNT is untouched,
/// which is what the confirm says; for the machine's own ambient login it
/// also says that the CLI in the person's terminal signs out with it.
///
/// Destructive, so it confirms first with the pinned ×4 sentence
/// ([`sign_out_confirm`]). This device signs out directly; another of mine
/// gets the `agent_profile_sign_out` command on its heartbeat and answers by
/// re-reporting, so the row reads "signed out" on the next beat either way.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sign_out_account(
    device_id: String,
    device_label: SharedString,
    own: bool,
    agent: CodingAgent,
    profile_id: String,
    account_label: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    let ambient = coding::agent_profiles::is_system(Some(&profile_id));
    let confirm = sign_out_confirm(
        &account_label,
        &device_label,
        ambient.then(|| agent.label()),
    );
    confirmed_account_change(
        AccountChange {
            device_id,
            device_label,
            own,
            agent,
            profile_id,
            account_label,
            title: "Sign out",
            confirm,
            ok_label: "Sign out",
            local_done: "signed out on this device.",
            remote_done: "will sign this login out.",
            remove: false,
        },
        window,
        cx,
    );
}

/// One confirmed, destructive change to a login on one machine: the remove
/// (EXP-862) and the sign-out (EXP-1137) differ only in the body they run and
/// the sentences they say.
struct AccountChange {
    device_id: String,
    device_label: SharedString,
    own: bool,
    agent: CodingAgent,
    profile_id: String,
    account_label: SharedString,
    title: &'static str,
    confirm: String,
    ok_label: &'static str,
    /// After `{account_label} ` on this machine.
    local_done: &'static str,
    /// After `{device_label} ` on another of mine.
    remote_done: &'static str,
    remove: bool,
}

fn confirmed_account_change(change: AccountChange, window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    let AccountChange {
        device_id,
        device_label,
        own,
        agent,
        profile_id,
        account_label,
        title,
        confirm,
        ok_label,
        local_done,
        remote_done,
        remove,
    } = change;
    let spec = crate::native_dialog::AlertSpec::new(title, confirm, ok_label)
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |window, cx| {
            if own {
                let outcome = if remove {
                    crate::device_sync::remove_agent_profile_here(agent, &profile_id, cx)
                } else {
                    crate::device_sync::sign_out_agent_profile_here(agent, &profile_id, cx)
                };
                match outcome {
                    Ok(()) => window.push_notification(
                        Notification::success(SharedString::from(format!(
                            "{account_label} {local_done}"
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
                        let send = if remove {
                            api::devices::create_agent_profile_remove_command
                        } else {
                            api::devices::create_agent_profile_sign_out_command
                        };
                        send(&trpc, &device_id, agent.id(), &profile_id)
                    })
                    .await;
                let _ = handle.update(cx, |_, window, cx| match result {
                    Ok(_) => window.push_notification(
                        Notification::success(SharedString::from(format!(
                            "{device_label} {remote_done}"
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
