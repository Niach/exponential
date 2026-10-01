//! EXP-909 — the ACCOUNT writes a login row offers, wherever it is read:
//! sign it out, remove the device's copy of it, and the way into that
//! device's settings. EXP-1158: no entry picks the login a start runs on —
//! every start runs on the login last used on that device.
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
use gpui_component::{button::ButtonVariant};

use coding::CodingAgent;

use crate::queries;
use crate::usage_bar::{remove_account_confirm, remove_ambient_account_confirm, sign_out_confirm};

/// The device row's menu entry AND the login rows' way into it — one string
/// ×4 (it replaced "Edit" with EXP-862).
pub(crate) const DEVICE_SETTINGS: &str = "Device settings";

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
                    Ok(()) => {
                        crate::toast::success(format!("{account_label} {local_done}"), window, cx)
                    }
                    Err(err) => crate::toast::error(err, window, cx),
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
                    Ok(_) => {
                        crate::toast::success(format!("{device_label} {remote_done}"), window, cx)
                    }
                    Err(err) => crate::toast::error(err.user_message(), window, cx),
                });
            })
            .detach();
            true
        });
    crate::native_dialog::open_alert(window, cx, spec);
}
