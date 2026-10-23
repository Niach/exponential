//! EXP-1252 — moving a live session to ANOTHER device.
//!
//! The twin of [`crate::account_switch`]: a move is one
//! `sessions.send{sessionId, deviceId}` and the server does the rest
//! (NEW-1: the session ends here and continues on the target on the same
//! branch; no device packs or hands anything off). This module is the
//! decision surface over it:
//! which devices the session could move to, why a row is refused, what the
//! pin says while a move is pending, and the one call that asks for it.
//!
//! The pin sits on THE session composer's options line BEFORE the model
//! ([`crate::composer::SessionComposerControls::device_pin`]) and is built
//! exactly like the launch line's Device pin (`chat_screen::device_pin`):
//! THE device picker ([`crate::picker::device_picker`]) behind
//! [`crate::launch_options::inline_pin_trigger_with`], its rows off
//! [`crate::queries::launch_devices`].

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement as _, SharedString, Window};

use crate::queries::LaunchDevice;

// ---------------------------------------------------------------------------
// EXP-1252 — the copy, byte-identical ×4
// ---------------------------------------------------------------------------
//
// Hand-mirrored with web `lib/session-device-move.ts`, iOS
// `SessionDeviceMove.swift` and Android `domain/SessionDeviceMove.kt` (the web
// copy-lock test reads this file for the literals).

/// The picker's title (copy-locked ×4; N-12: the glyph pin's tooltip is
/// the device name).
#[allow(dead_code)]
pub(crate) const PICKER_TITLE: &str = "Move to device";

/// The pin while `move_to_device_id` is set; `{device}` = the target's name.
pub(crate) const MOVING_LABEL: &str = "Moving to {device}…";

/// What a move does (copy-locked ×4).
#[allow(dead_code)]
pub(crate) const NOTE: &str =
    "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device.";

pub(crate) const REASON_STEER: &str = "Only people who can steer this session can move it.";
pub(crate) const REASON_ENDED: &str = "This session has ended.";
// The server's refusals for an agent / an aside: the pin is HIDDEN for both,
// so only the other clients' copy-lock tests (and the server) say them.
#[allow(dead_code)]
pub(crate) const REASON_AGENT: &str = "Agents move from Settings → Agents.";
#[allow(dead_code)]
pub(crate) const REASON_ASIDE: &str = "An aside stays on its parent's device.";
pub(crate) const REASON_OFFLINE: &str = "This device is offline.";
pub(crate) const REASON_VISIBILITY: &str =
    "This device is shared differently, so the session can't move here.";
pub(crate) const REASON_NO_LOGIN: &str = "This device has no signed-in login for this agent.";
pub(crate) const REASON_UPDATE: &str = "This device needs an update to receive sessions.";

/// The failure toast's verb + object: `Couldn't move the session`.
pub(crate) const FAILURE_VERB: &str = "move";
pub(crate) const FAILURE_THING: &str = "the session";

// ---------------------------------------------------------------------------
// The rules (pure)
// ---------------------------------------------------------------------------

/// Whether the pin shows at all, and whether it can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PinState {
    /// Agents (persistent kind) and asides: no pin. The reasons
    /// [`REASON_AGENT`]/[`REASON_ASIDE`] are the server's refusals for them.
    Hidden,
    /// Shown, inert, the reason on its tooltip.
    Disabled(&'static str),
    Enabled,
}

/// The pin's state for a session: hidden for an agent or an aside, then
/// disabled when the viewer cannot steer it, then when it has ended.
pub(crate) fn pin_state(persistent: bool, aside: bool, may_steer: bool, ended: bool) -> PinState {
    if persistent || aside {
        return PinState::Hidden;
    }
    if !may_steer {
        return PinState::Disabled(REASON_STEER);
    }
    if ended {
        return PinState::Disabled(REASON_ENDED);
    }
    PinState::Enabled
}

/// One device the session could be on, as the rules read it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MoveCandidate {
    pub device_id: String,
    /// The picker line ([`crate::queries::launch_device_label`]).
    pub label: String,
    /// The device's plain name (the moving label's `{device}`).
    pub name: String,
    pub online: bool,
    /// The device is shared with the SESSION's team.
    pub shared_with_team: bool,
    /// The device has a signed-in, runnable login for the session's agent.
    pub has_login: bool,
    /// The build it reported (`devices.version`); `None` = an old build.
    pub version: Option<String>,
}

/// The first desktop/CLI build that receives a moved session (the server's
/// `MOVE_MIN_VERSION`, web `session-device-move.ts`); an unreported version
/// = an old build.
pub(crate) const MOVE_MIN_VERSION: (u64, u64, u64) = (0, 14, 73);

/// Web `parseVersionTuple`: the core before any `-pre` / `+build`, split on
/// `.`; the major must be digits, a missing or non-numeric minor / patch
/// is 0.
fn version_tuple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let mut parts = core.split('.').map(|part| {
        (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse::<u64>().ok())
            .flatten()
    });
    let major = parts.next().flatten()?;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

/// Web `deviceCanReceiveSession`: the device's reported version is at least
/// [`MOVE_MIN_VERSION`].
pub(crate) fn device_can_receive_session(version: Option<&str>) -> bool {
    version
        .filter(|version| !version.is_empty())
        .and_then(version_tuple)
        .is_some_and(|have| have >= MOVE_MIN_VERSION)
}

/// One picker row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MoveRow {
    pub device_id: String,
    pub label: String,
    pub current: bool,
    /// `None` = offerable; `Some(reason)` = disabled with the reason.
    pub reason: Option<&'static str>,
}

/// Why `candidate` cannot take the session, in the ×4 order (web
/// `deviceMoveTargetReason`): offline, visibility, update (a build before
/// [`MOVE_MIN_VERSION`]), no login.
pub(crate) fn refusal(candidate: &MoveCandidate, session_shared: bool) -> Option<&'static str> {
    if !candidate.online {
        return Some(REASON_OFFLINE);
    }
    if candidate.shared_with_team != session_shared {
        return Some(REASON_VISIBILITY);
    }
    if !device_can_receive_session(candidate.version.as_deref()) {
        return Some(REASON_UPDATE);
    }
    if !candidate.has_login {
        return Some(REASON_NO_LOGIN);
    }
    None
}

/// The picker rows: every candidate in the launch order, the current device
/// selected and never refused (picking it is "stay" or, mid-move, cancel).
/// A current device the candidates do not list still heads the rows (its
/// id as the label), so a pending move always has its cancel row — web
/// `sessionDeviceMoveRows`.
pub(crate) fn move_rows(
    candidates: &[MoveCandidate],
    current_device_id: Option<&str>,
    session_shared: bool,
) -> Vec<MoveRow> {
    let mut rows: Vec<MoveRow> = candidates
        .iter()
        .map(|candidate| {
            let current = Some(candidate.device_id.as_str()) == current_device_id;
            MoveRow {
                device_id: candidate.device_id.clone(),
                label: candidate.label.clone(),
                current,
                reason: if current {
                    None
                } else {
                    refusal(candidate, session_shared)
                },
            }
        })
        .collect();
    if let Some(current) = current_device_id.filter(|id| !id.is_empty()) {
        if !rows.iter().any(|row| row.current) {
            rows.insert(
                0,
                MoveRow {
                    device_id: current.to_string(),
                    label: current.to_string(),
                    current: true,
                    reason: None,
                },
            );
        }
    }
    rows
}

/// [`MOVING_LABEL`] for `device`.
pub(crate) fn moving_label(device: &str) -> String {
    MOVING_LABEL.replace("{device}", device)
}

/// The pin's word: the current device, or [`MOVING_LABEL`] while a move to
/// `moving_to` is pending.
pub(crate) fn pin_label(current: &str, moving_to: Option<&str>) -> String {
    match moving_to {
        Some(target) => moving_label(target),
        None => current.to_string(),
    }
}

/// What picking `picked` asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickOutcome {
    /// Move to `picked` (`sessions.send{sessionId, deviceId}`).
    Move(String),
    /// The current device while a move is pending: cancel it (the same call,
    /// naming the current device).
    Cancel(String),
    /// Nothing to ask (the current device with no move, or the pending
    /// target again).
    Nothing,
}

pub(crate) fn pick_outcome(
    picked: &str,
    current_device_id: Option<&str>,
    moving_to: Option<&str>,
) -> PickOutcome {
    if Some(picked) == current_device_id {
        return match moving_to {
            Some(_) => PickOutcome::Cancel(picked.to_string()),
            None => PickOutcome::Nothing,
        };
    }
    if Some(picked) == moving_to {
        return PickOutcome::Nothing;
    }
    PickOutcome::Move(picked.to_string())
}

// ---------------------------------------------------------------------------
// The candidates (synced state)
// ---------------------------------------------------------------------------

/// The synced `devices` row for `device_id` (the caller's own row first when a
/// shared machine produced several).
fn device_row(
    devices: &[domain::rows::DeviceRow],
    device_id: &str,
    me: &str,
) -> Option<domain::rows::DeviceRow> {
    let matches: Vec<&domain::rows::DeviceRow> = devices
        .iter()
        .filter(|row| row.device_id.as_deref() == Some(device_id))
        .collect();
    matches
        .iter()
        .find(|row| row.user_id.as_deref() == Some(me))
        .or_else(|| matches.first())
        .map(|row| (*row).clone())
}

fn device_name(row: Option<&domain::rows::DeviceRow>, device_id: &str) -> String {
    row.and_then(|row| row.label.clone())
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| device_id.to_string())
}

/// The candidates for `session`: the devices the launch composer offers
/// ([`crate::queries::launch_devices`]: this device, then the online remote
/// ones), plus the session's own device when the launch list dropped it
/// (offline), so the current row is always there to be selected.
pub(crate) fn candidates(session: &domain::rows::SessionRow, cx: &mut App) -> Vec<MoveCandidate> {
    let launch: Vec<LaunchDevice> = crate::queries::launch_devices(cx);
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let devices: Vec<domain::rows::DeviceRow> =
        store.collections().devices.read(cx).iter().cloned().collect();
    let me = crate::queries::active_account(cx)
        .map(|account| account.user_id)
        .unwrap_or_default();
    let team_id = session.team_id.clone().unwrap_or_default();
    // A row with no `cli` is claude's (web `session.cli ?? 'claude'`).
    let agent = session
        .cli
        .as_deref()
        .and_then(coding::CodingAgent::parse)
        .unwrap_or(coding::CodingAgent::Claude);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut out: Vec<MoveCandidate> = launch
        .iter()
        .map(|device| {
            let row = device_row(&devices, &device.device_id, &me);
            MoveCandidate {
                device_id: device.device_id.clone(),
                label: device.label.clone(),
                name: device_name(row.as_ref(), &device.device_id),
                // The launch list holds this device and ONLINE remote ones.
                online: true,
                shared_with_team: row.as_ref().is_some_and(|row| row.is_shared_with(&team_id)),
                has_login: device.agents.contains(&agent),
                // This IDE knows its own build even before its row synced.
                version: row
                    .as_ref()
                    .and_then(|row| row.version.clone())
                    .filter(|version| !version.is_empty())
                    .or_else(|| {
                        device
                            .is_own
                            .then(|| domain::client_version::current_version().to_string())
                            .filter(|version| !version.is_empty())
                    }),
            }
        })
        .collect();
    if let Some(current) = session.device_id.as_deref().filter(|id| !id.is_empty()) {
        if !out.iter().any(|candidate| candidate.device_id == current) {
            let row = device_row(&devices, current, &me);
            let owner = row
                .as_ref()
                .filter(|row| row.user_id.as_deref() != Some(me.as_str()))
                .and_then(|row| row.user_id.as_deref())
                .and_then(|user_id| {
                    store
                        .collections()
                        .users
                        .read(cx)
                        .get(user_id)
                        .and_then(|user| user.name.clone())
                });
            out.push(MoveCandidate {
                device_id: current.to_string(),
                label: crate::queries::launch_device_label(
                    row.as_ref().and_then(|row| row.label.as_deref()).unwrap_or(""),
                    current,
                    owner.as_deref(),
                    false,
                ),
                name: device_name(row.as_ref(), current),
                online: row.as_ref().is_some_and(|row| {
                    crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms)
                }),
                shared_with_team: row.as_ref().is_some_and(|row| row.is_shared_with(&team_id)),
                has_login: true,
                version: row.as_ref().and_then(|row| row.version.clone()),
            });
        }
    }
    out
}

/// The name of `device_id` for the moving label, off the candidates, else
/// the synced row, else the id.
fn target_name(candidates: &[MoveCandidate], device_id: &str, cx: &App) -> String {
    if let Some(candidate) = candidates.iter().find(|c| c.device_id == device_id) {
        return candidate.name.clone();
    }
    let row = sync::Store::try_global(cx).and_then(|store| {
        store
            .collections()
            .devices
            .read(cx)
            .iter()
            .find(|row| row.device_id.as_deref() == Some(device_id))
            .cloned()
    });
    device_name(row.as_ref(), device_id)
}

// ---------------------------------------------------------------------------
// The pin + the call
// ---------------------------------------------------------------------------

/// THE session composer's Device pin (before Model). `None` when the session
/// takes no pin ([`PinState::Hidden`]) or names no device yet.
pub(crate) fn session_device_pin(
    id: SharedString,
    session: &domain::rows::SessionRow,
    may_steer: bool,
    cx: &mut App,
) -> Option<AnyElement> {
    let state = pin_state(session.is_persistent(), session.aside, may_steer, session.is_ended());
    if state == PinState::Hidden {
        return None;
    }
    let current = session.device_id.clone().filter(|id| !id.is_empty())?;
    let moving_to = session.moving_to().map(str::to_string);
    let candidates = candidates(session, cx);
    let rows = move_rows(&candidates, Some(current.as_str()), session.shared);
    let current_label = candidates
        .iter()
        .find(|candidate| candidate.device_id == current)
        .map(|candidate| candidate.label.clone())
        .unwrap_or_else(|| current.clone());
    let moving_name = moving_to.as_deref().map(|target| target_name(&candidates, target, cx));
    let label = pin_label(&current_label, moving_name.as_deref());
    // N-12: the pin is its glyph alone (the target's while a move is
    // pending); the name, or the pending move, is the tooltip.
    let shown = moving_to.as_deref().unwrap_or(current.as_str());
    let glyph = crate::icons::device_icon(Some(crate::launch_options::device_glyph_name(shown, cx)), false);
    let tooltip = match state {
        PinState::Disabled(reason) => SharedString::from(format!("{label}\n{reason}")),
        _ => SharedString::from(label),
    };
    let trigger = crate::launch_options::inline_pin_glyph_trigger(id.clone(), glyph, cx)
        .tooltip(tooltip)
        .into_any_element();
    // The launch line's rows ([`crate::launch_options::launch_device_rows`]:
    // each machine's own glyph + its picker line), refused ones disabled
    // with their reason as the description.
    let picker_rows: Vec<crate::picker::device_picker::DevicePickerDevice> = rows
        .iter()
        .map(|row| crate::picker::device_picker::DevicePickerDevice {
            id: row.device_id.clone(),
            name: row.label.clone(),
            icon: Some(crate::launch_options::device_glyph_name(&row.device_id, cx).to_string()),
            server: false,
            description: row.reason.map(str::to_string),
            disabled: row.reason.is_some(),
        })
        .collect();
    let disabled = matches!(state, PinState::Disabled(_));
    let session_id = session.id.clone();
    let picker_id = SharedString::from(format!("{id}-picker"));
    Some(
        crate::picker::deferred(move |window, cx| {
            crate::picker::device_picker::device_picker(
                &picker_rows,
                Some(current.clone()),
                trigger,
                Rc::new(move |values: Vec<String>, _window: &mut Window, cx: &mut App| {
                    let Some(picked) = values.into_iter().next() else {
                        return;
                    };
                    match pick_outcome(&picked, Some(current.as_str()), moving_to.as_deref()) {
                        PickOutcome::Move(device_id) | PickOutcome::Cancel(device_id) => {
                            request_move(session_id.clone(), device_id, cx);
                        }
                        PickOutcome::Nothing => {}
                    }
                }),
            )
            .disabled(disabled)
            .id(picker_id.clone())
            .render(window, cx)
        })
        .into_any_element(),
    )
}

/// `sessions.send{sessionId, deviceId}`: a move request, or (naming the
/// current device while a move is pending) its cancel. The synced
/// `move_to_device_id` confirms; a refusal is `Couldn't move the session`.
pub(crate) fn request_move(session_id: String, device_id: String, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action(FAILURE_VERB, FAILURE_THING, None, cx);
        return;
    };
    let input = api::sessions::SendInput {
        session_id: Some(session_id),
        device_id: Some(device_id),
        ..Default::default()
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::sessions::send(&trpc, &input) })
            .await;
        if let Err(err) = result {
            let _ = cx.update(|cx| {
                crate::action_error::failed_action_err(FAILURE_VERB, FAILURE_THING, &err, cx);
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The copy is the ×4 contract, byte for byte.
    #[test]
    fn copy_is_the_contract() {
        assert_eq!(PICKER_TITLE, "Move to device");
        assert_eq!(MOVING_LABEL, "Moving to {device}\u{2026}");
        assert_eq!(
            NOTE,
            "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device."
        );
        assert_eq!(REASON_STEER, "Only people who can steer this session can move it.");
        assert_eq!(REASON_ENDED, "This session has ended.");
        assert_eq!(REASON_AGENT, "Agents move from Settings \u{2192} Agents.");
        assert_eq!(REASON_ASIDE, "An aside stays on its parent's device.");
        assert_eq!(REASON_OFFLINE, "This device is offline.");
        assert_eq!(
            REASON_VISIBILITY,
            "This device is shared differently, so the session can't move here."
        );
        assert_eq!(REASON_NO_LOGIN, "This device has no signed-in login for this agent.");
        assert_eq!(REASON_UPDATE, "This device needs an update to receive sessions.");
        assert_eq!(format!("Couldn't {FAILURE_VERB} {FAILURE_THING}"), "Couldn't move the session");
    }

    #[test]
    fn pin_hides_for_agents_and_asides_then_gates_steer_then_ended() {
        assert_eq!(pin_state(true, false, true, false), PinState::Hidden);
        assert_eq!(pin_state(false, true, true, false), PinState::Hidden);
        assert_eq!(pin_state(true, false, false, true), PinState::Hidden);
        assert_eq!(pin_state(false, false, false, true), PinState::Disabled(REASON_STEER));
        assert_eq!(pin_state(false, false, true, true), PinState::Disabled(REASON_ENDED));
        assert_eq!(pin_state(false, false, true, false), PinState::Enabled);
    }

    fn candidate(id: &str, online: bool, shared: bool, login: bool) -> MoveCandidate {
        MoveCandidate {
            device_id: id.into(),
            label: format!("{id} label"),
            name: id.into(),
            online,
            shared_with_team: shared,
            has_login: login,
            version: Some("0.14.73".into()),
        }
    }

    /// The refusal order: offline, then visibility, then update, then no
    /// login.
    #[test]
    fn refusals_follow_the_contract_order() {
        let mut all_wrong = candidate("a", false, true, false);
        all_wrong.version = None;
        assert_eq!(refusal(&all_wrong, false), Some(REASON_OFFLINE));
        let mut online = candidate("a", true, true, false);
        online.version = None;
        assert_eq!(refusal(&online, false), Some(REASON_VISIBILITY));
        assert_eq!(refusal(&online, true), Some(REASON_UPDATE));
        online.version = Some("0.14.73".into());
        assert_eq!(refusal(&online, true), Some(REASON_NO_LOGIN));
        assert_eq!(refusal(&candidate("a", true, false, true), false), None);
        assert_eq!(refusal(&candidate("a", true, true, true), true), None);
    }

    /// Web `deviceCanReceiveSession`: at least 0.14.73, parsed like
    /// `parseVersionTuple`; unreported or unparsable = an old build.
    #[test]
    fn a_target_needs_the_move_build() {
        for (version, ok) in [
            (Some("0.14.73"), true),
            (Some("0.14.74"), true),
            (Some("0.15.0"), true),
            (Some("1.0.0"), true),
            (Some("0.14.73-beta.1+build.7"), true),
            (Some("0.14.72"), false),
            (Some("0.13.99"), false),
            (Some("0.14"), false),
            (Some("0.14.x"), false),
            (Some("v0.14.73"), false),
            (Some(""), false),
            (None, false),
        ] {
            assert_eq!(device_can_receive_session(version), ok, "{version:?}");
        }
        let mut old = candidate("a", true, false, true);
        old.version = Some("0.14.72".into());
        assert_eq!(refusal(&old, false), Some(REASON_UPDATE));
    }

    /// A current device the candidates do not list (its row never synced,
    /// or the launch list dropped it) still heads the rows, selected and
    /// never refused: a pending move keeps its cancel row.
    #[test]
    fn an_unlisted_current_device_still_heads_the_rows() {
        let rows = move_rows(&[candidate("there", true, false, true)], Some("gone"), false);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            MoveRow { device_id: "gone".into(), label: "gone".into(), current: true, reason: None }
        );
        assert!(!rows[1].current);
        assert_eq!(pick_outcome("gone", Some("gone"), Some("there")), PickOutcome::Cancel("gone".into()));
        // No current device at all: nothing is synthesised.
        assert_eq!(move_rows(&[candidate("there", true, false, true)], None, false).len(), 1);
        assert_eq!(move_rows(&[candidate("there", true, false, true)], Some(""), false).len(), 1);
    }

    /// The current device is selected and never refused, even offline.
    #[test]
    fn current_row_is_never_refused() {
        let rows = move_rows(
            &[
                candidate("here", false, false, false),
                candidate("there", true, false, true),
                candidate("server", true, true, true),
            ],
            Some("here"),
            false,
        );
        assert!(rows[0].current);
        assert_eq!(rows[0].reason, None);
        assert_eq!(rows[1].reason, None);
        assert_eq!(rows[2].reason, Some(REASON_VISIBILITY));
    }

    #[test]
    fn pin_label_names_the_current_device_or_the_move() {
        assert_eq!(pin_label("mint — This device", None), "mint — This device");
        assert_eq!(pin_label("mint — This device", Some("Studio")), "Moving to Studio\u{2026}");
    }

    #[test]
    fn picking_moves_cancels_or_does_nothing() {
        assert_eq!(pick_outcome("b", Some("a"), None), PickOutcome::Move("b".into()));
        assert_eq!(pick_outcome("a", Some("a"), None), PickOutcome::Nothing);
        assert_eq!(pick_outcome("a", Some("a"), Some("b")), PickOutcome::Cancel("a".into()));
        assert_eq!(pick_outcome("b", Some("a"), Some("b")), PickOutcome::Nothing);
        assert_eq!(pick_outcome("c", Some("a"), Some("b")), PickOutcome::Move("c".into()));
    }
}
