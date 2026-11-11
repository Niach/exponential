//! EXP-1252 — moving a live session to ANOTHER device.
//!
//! The twin of [`crate::account_switch`]: a move is one
//! `sessions.send{sessionId, deviceId}` and the server does the rest
//! (NEW-1: the session ends here and continues on the target on the same
//! branch; no device packs or hands anything off). This module is the
//! decision surface over it:
//! which devices the session could move to, why a row is refused, and the
//! one call that asks for it. NEW-8: a move is immediate (the server ends
//! the session here and continues it there), so there is no pending move to
//! show or cancel.
//!
//! The pin sits on THE session composer's options line BEFORE the model
//! ([`crate::composer::SessionComposerControls::device_pin`]) and is built
//! exactly like the launch line's Device pin (`chat_screen::device_pin`):
//! THE device picker ([`crate::picker::device_picker`]) behind
//! [`crate::launch_options::inline_pin_trigger_with`], its rows off
//! [`crate::queries::launch_devices`].
//!
//! S-3 (composer-menu.json): the session composer's Device pin is GONE (no
//! options line; a running session never moves device from its composer),
//! so the pin and its move flow have no caller until a new surface hosts
//! them; kept (and tested) rather than rewritten later. #EXP-1703: the
//! Dock's offline banner hosts the same picker behind its Move
//! ([`session_device_move_button`]).
#![allow(dead_code)]

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

/// What a move does (copy-locked ×4).
#[allow(dead_code)]
pub(crate) const NOTE: &str =
    "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device.";

pub(crate) const REASON_STEER: &str = "Only people who can steer this session can move it.";
pub(crate) const REASON_ENDED: &str = "This session has ended.";
// The server's refusal for an agent: the pin is HIDDEN for one, so only
// the other clients' copy-lock tests (and the server) say it.
#[allow(dead_code)]
pub(crate) const REASON_AGENT: &str = "Agents move from Settings → Agents.";
pub(crate) const REASON_OFFLINE: &str = "This device is offline.";
pub(crate) const REASON_VISIBILITY: &str =
    "This device is shared differently, so the session can't move here.";
pub(crate) const REASON_NO_LOGIN: &str = "This device has no signed-in login for this agent.";
/// #EXP-1440: the server's refusal for one of the caller's own machines
/// shared only with OTHER teams (the pickers hide its row:
/// [`domain::rows::DeviceRow::is_usable_in_team`]).
#[allow(dead_code)]
pub(crate) const DEVICE_OTHER_TEAM_MESSAGE: &str = "This device belongs to another team";

/// The failure toast's verb + object: `Couldn't move the session`.
pub(crate) const FAILURE_VERB: &str = "move";
pub(crate) const FAILURE_THING: &str = "the session";

// ---------------------------------------------------------------------------
// The rules (pure)
// ---------------------------------------------------------------------------

/// Whether the pin shows at all, and whether it can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PinState {
    /// Agents (persistent kind): no pin. [`REASON_AGENT`] is the server's
    /// refusal for one.
    Hidden,
    /// Shown, inert, the reason on its tooltip.
    Disabled(&'static str),
    Enabled,
}

/// The pin's state for a session: hidden for an agent, then disabled when
/// the viewer cannot steer it, then when it has ended.
pub(crate) fn pin_state(persistent: bool, may_steer: bool, ended: bool) -> PinState {
    if persistent {
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
    pub online: bool,
    /// The device is shared with the SESSION's team.
    pub shared_with_team: bool,
    /// The device has a signed-in, runnable login for the session's agent.
    pub has_login: bool,
    /// #EXP-1541: whose machine it is (its `devices` row's `user_id`; `None`
    /// = unknown, never the starter's).
    pub owner_id: Option<String>,
}

/// #EXP-1541: the session's side of a target's refusal. `queued_root` +
/// `starter_id` open #FEED-177's exception ([`visibility_move`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MoveSession {
    pub shared: bool,
    /// [`queued_root`]: queued, a root, no sub-sessions.
    pub queued_root: bool,
    /// The session's starter (`sessions.user_id`).
    pub starter_id: Option<String>,
    /// The viewer, who would move it.
    pub mover_id: Option<String>,
}

/// #EXP-1541: a QUEUED ROOT with no sub-sessions (nothing ran; the
/// [`visibility_move`] exception). Fixture `queuedRootCases`.
pub(crate) fn queued_root(status: Option<&str>, parent_id: Option<&str>, has_sub_sessions: bool) -> bool {
    status == Some("queued") && parent_id.is_none_or(str::is_empty) && !has_sub_sessions
}

/// #FEED-177 (#EXP-1536 B3), web `visibilityMove`: a session keeps its
/// tree's visibility on a move, except a QUEUED ROOT with no sub-sessions
/// moving to one of its STARTER's own machines, which takes that machine's
/// sharing; only the starter may make a shared session private. `Some` =
/// the visibility it has there, `None` = refused. Fixture `visibilityCases`.
pub(crate) fn visibility_move(
    session_shared: bool,
    target_shared: bool,
    queued_root: bool,
    target_is_starters: bool,
    mover_is_starter: bool,
) -> Option<bool> {
    if target_shared == session_shared {
        return Some(session_shared);
    }
    if !queued_root || !target_is_starters {
        return None;
    }
    if session_shared && !mover_is_starter {
        return None;
    }
    Some(target_shared)
}

/// #EXP-1541: a candidate's owner off its `devices` row: the row's
/// `user_id`, else the viewer (`me`; the launch list's own machine may have
/// no synced row yet). Fixture `targetCases` (`deviceOwner: null`).
pub(crate) fn candidate_owner(row_owner: Option<String>, me: &str) -> Option<String> {
    row_owner.or_else(|| (!me.is_empty()).then(|| me.to_string()))
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
/// `deviceMoveTargetReason`, fixture `session-device-move.json`): offline,
/// visibility ([`visibility_move`]), no login. Old builds are the
/// CLIENT_MIN_VERSION floor's job, never a refusal here.
pub(crate) fn refusal(candidate: &MoveCandidate, session: &MoveSession) -> Option<&'static str> {
    if !candidate.online {
        return Some(REASON_OFFLINE);
    }
    let starter = session.starter_id.as_deref();
    let visibility = visibility_move(
        session.shared,
        candidate.shared_with_team,
        session.queued_root,
        starter.is_some() && candidate.owner_id.as_deref() == starter,
        starter.is_some() && session.mover_id.as_deref() == starter,
    );
    if visibility.is_none() {
        return Some(REASON_VISIBILITY);
    }
    if !candidate.has_login {
        return Some(REASON_NO_LOGIN);
    }
    None
}

/// The picker rows: every candidate in the launch order, the current device
/// selected and never refused (picking it is "stay"). A current device the
/// candidates do not list still heads the rows (its id as the label) — web
/// `sessionDeviceMoveRows`.
pub(crate) fn move_rows(
    candidates: &[MoveCandidate],
    current_device_id: Option<&str>,
    session: &MoveSession,
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
                    refusal(candidate, session)
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

/// What picking `picked` asks for: a move to another device, nothing for
/// the current one (web `deviceMovePick`).
pub(crate) fn pick_outcome(picked: &str, current_device_id: Option<&str>) -> Option<String> {
    (Some(picked) != current_device_id).then(|| picked.to_string())
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

/// The candidates for `session`: the devices the launch composer offers
/// ([`crate::queries::launch_devices`]: this device, then the online remote
/// ones), plus the session's own device when the launch list dropped it
/// (offline), so the current row is always there to be selected.
pub(crate) fn candidates(session: &domain::rows::SessionRow, cx: &mut App) -> Vec<MoveCandidate> {
    let launch: Vec<LaunchDevice> =
        crate::queries::launch_devices(session.team_id.as_deref(), cx);
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
            let owner_id = row.as_ref().and_then(|row| row.user_id.clone());
            MoveCandidate {
                device_id: device.device_id.clone(),
                label: device.label.clone(),
                // The launch list holds this device and ONLINE remote ones.
                online: true,
                shared_with_team: row.as_ref().is_some_and(|row| row.is_shared_with(&team_id)),
                has_login: device.agents.contains(&agent),
                owner_id: candidate_owner(owner_id, &me),
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
                online: row.as_ref().is_some_and(|row| {
                    crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms)
                }),
                shared_with_team: row.as_ref().is_some_and(|row| row.is_shared_with(&team_id)),
                has_login: true,
                owner_id: row.as_ref().and_then(|row| row.user_id.clone()),
            });
        }
    }
    out
}

/// #EXP-1541: the session's side of the refusals off the synced rows.
pub(crate) fn move_session(session: &domain::rows::SessionRow, cx: &App) -> MoveSession {
    let has_sub_sessions = sync::Store::try_global(cx).is_some_and(|store| {
        store
            .collections()
            .sessions
            .read(cx)
            .iter()
            .any(|row| row.id != session.id && row.root_id.as_deref() == Some(session.id.as_str()))
    });
    MoveSession {
        shared: session.shared,
        queued_root: queued_root(
            session.status.as_deref(),
            session.parent_id.as_deref(),
            has_sub_sessions,
        ),
        starter_id: session.user_id.clone(),
        mover_id: crate::queries::active_account(cx).map(|account| account.user_id),
    }
}

// ---------------------------------------------------------------------------
// #FEED-204 — an app editor's Device row
// ---------------------------------------------------------------------------

/// What an app editor's composer shows and sends on (apps.json EDITOR
/// DEVICE, [`domain::apps::editor_device`]), built by the chat
/// ([`crate::agent_screen`]) and handed to the root's composer.
#[derive(Clone)]
pub(crate) struct EditorDevice {
    pub app_id: String,
    pub team_id: String,
    pub device_id: Option<String>,
    /// THE launch rows: (device id, picker line).
    pub rows: Vec<(String, String)>,
    /// An ended root's next send goes to `device_id`.
    pub moves: bool,
    /// #EXP-1729: `device_id` is among the online (launch) devices.
    pub online: bool,
    pub on_pick: Rc<dyn Fn(String, &mut Window, &mut App)>,
}

impl EditorDevice {
    /// The row's value: the device's picker line.
    pub(crate) fn value(&self) -> Option<String> {
        let picked = self.device_id.as_deref()?;
        self.rows.iter().find(|(id, _)| id == picked).map(|(_, label)| label.clone())
    }

    /// Everything but the pick: a repaint is due only when these differ.
    pub(crate) fn same_as(&self, other: &Self) -> bool {
        self.device_id == other.device_id
            && self.rows == other.rows
            && self.moves == other.moves
            && self.online == other.online
    }

    /// #EXP-1729 (apps.json `editorHeader`): the header's device, its name
    /// (the Device row's line, else its id) and whether it is online.
    pub(crate) fn header_device(&self) -> Option<(String, bool)> {
        let id = self.device_id.as_deref()?;
        Some((self.value().unwrap_or_else(|| id.to_string()), self.online))
    }

    /// `(team id, app id, device id)` of the `sessions.send({appId, deviceId})`
    /// that revives an ended root elsewhere; `None` = a plain message.
    pub(crate) fn move_target(&self) -> Option<(String, String, String)> {
        let device_id = self.device_id.clone().filter(|_| self.moves)?;
        Some((self.team_id.clone(), self.app_id.clone(), device_id))
    }

    /// The "+" Device row: THE launch submenu.
    pub(crate) fn menu_row(
        &self,
        menu: gpui_component::menu::PopupMenu,
        row: crate::chat_screen::ComposerMenuRow,
        window: &mut Window,
        cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
    ) -> gpui_component::menu::PopupMenu {
        let value = self.value();
        let rows = self.rows.clone();
        let picked = self.device_id.clone();
        let on_pick = self.on_pick.clone();
        menu.submenu_with_value(Some(row.menu_icon()), row.label(), value, window, cx, move |mut sub, _, _| {
            for (id, label) in &rows {
                let checked = picked.as_deref() == Some(id.as_str());
                let on_pick = on_pick.clone();
                let id = id.clone();
                sub = sub.item(crate::controls::pointer_check_item(label.clone(), checked, move |window, cx| {
                    on_pick(id.clone(), window, cx);
                }));
            }
            sub
        })
    }
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
    let state = pin_state(session.is_persistent(), may_steer, session.is_ended());
    if state == PinState::Hidden {
        return None;
    }
    let current = session.device_id.clone().filter(|id| !id.is_empty())?;
    let candidates = candidates(session, cx);
    let rows = move_rows(&candidates, Some(current.as_str()), &move_session(session, cx));
    let current_label = candidates
        .iter()
        .find(|candidate| candidate.device_id == current)
        .map(|candidate| candidate.label.clone())
        .unwrap_or_else(|| current.clone());
    // N-12: the pin is its glyph alone; the name is the tooltip.
    let glyph = crate::icons::device_icon(Some(crate::launch_options::device_glyph_name(&current, cx)), false);
    let tooltip = match state {
        PinState::Disabled(reason) => SharedString::from(format!("{current_label}\n{reason}")),
        _ => SharedString::from(current_label),
    };
    let trigger = crate::launch_options::inline_pin_glyph_trigger(id.clone(), glyph, cx)
        .tooltip(tooltip)
        .into_any_element();
    let disabled = matches!(state, PinState::Disabled(_));
    Some(move_picker(id, session, current, &rows, trigger, disabled, cx))
}

/// #EXP-1703 (web `SessionDeviceMoveButton`): the Dock's offline banner
/// action (session-card.json `bannerActions.offline`, `Move`): THE device
/// picker of [`session_device_pin`] (the same rows and refusals, a pick =
/// [`request_move`]) behind an outline button labelled `label`. `None` when
/// the viewer may not move it (an agent, not steerable, ended) or it names
/// no device.
pub(crate) fn session_device_move_button(
    id: SharedString,
    session: &domain::rows::SessionRow,
    may_steer: bool,
    label: &str,
    cx: &mut App,
) -> Option<AnyElement> {
    use gpui::Styled as _;
    use gpui_component::button::{Button, ButtonVariants as _};
    use gpui_component::{Icon, Sizable as _};
    if pin_state(session.is_persistent(), may_steer, session.is_ended()) != PinState::Enabled {
        return None;
    }
    let current = session.device_id.clone().filter(|id| !id.is_empty())?;
    let candidates = candidates(session, cx);
    let rows = move_rows(&candidates, Some(current.as_str()), &move_session(session, cx));
    let trigger = Button::new(SharedString::from(format!("{id}-trigger")))
        .outline()
        .small()
        .cursor_pointer()
        .icon(Icon::new(crate::icons::registry::UI_SWAP).xsmall())
        .label(label.to_string())
        .into_any_element();
    Some(move_picker(id, session, current, &rows, trigger, false, cx))
}

/// THE device picker over `rows` behind `trigger` (the pin's and the
/// banner's): a pick other than `current` = [`request_move`].
fn move_picker(
    id: SharedString,
    session: &domain::rows::SessionRow,
    current: String,
    rows: &[MoveRow],
    trigger: AnyElement,
    disabled: bool,
    cx: &mut App,
) -> AnyElement {
    // THE device picker's rows (
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
    let session_id = session.id.clone();
    let picker_id = SharedString::from(format!("{id}-picker"));
    crate::picker::deferred(move |window, cx| {
        crate::picker::device_picker::device_picker(
            &picker_rows,
            Some(current.clone()),
            trigger,
            Rc::new(move |values: Vec<String>, _window: &mut Window, cx: &mut App| {
                let Some(picked) = values.into_iter().next() else {
                    return;
                };
                if let Some(device_id) = pick_outcome(&picked, Some(current.as_str())) {
                    request_move(session_id.clone(), device_id, cx);
                }
            }),
        )
        .disabled(disabled)
        .id(picker_id.clone())
        .render(window, cx)
    })
    .into_any_element()
}

/// `sessions.send{sessionId, deviceId}`: a move request. The synced row
/// (ended here, continued there) confirms; a refusal is `Couldn't move the
/// session`.
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
        assert_eq!(
            NOTE,
            "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device."
        );
        assert_eq!(REASON_STEER, "Only people who can steer this session can move it.");
        assert_eq!(REASON_ENDED, "This session has ended.");
        assert_eq!(REASON_AGENT, "Agents move from Settings \u{2192} Agents.");
        assert_eq!(REASON_OFFLINE, "This device is offline.");
        assert_eq!(
            REASON_VISIBILITY,
            "This device is shared differently, so the session can't move here."
        );
        assert_eq!(REASON_NO_LOGIN, "This device has no signed-in login for this agent.");
        assert_eq!(format!("Couldn't {FAILURE_VERB} {FAILURE_THING}"), "Couldn't move the session");
    }

    #[test]
    fn pin_hides_for_agents_then_gates_steer_then_ended() {
        assert_eq!(pin_state(true, true, false), PinState::Hidden);
        assert_eq!(pin_state(true, false, true), PinState::Hidden);
        assert_eq!(pin_state(false, false, true), PinState::Disabled(REASON_STEER));
        assert_eq!(pin_state(false, true, true), PinState::Disabled(REASON_ENDED));
        assert_eq!(pin_state(false, true, false), PinState::Enabled);
    }

    fn candidate(id: &str, online: bool, shared: bool, login: bool) -> MoveCandidate {
        MoveCandidate {
            device_id: id.into(),
            label: format!("{id} label"),
            online,
            shared_with_team: shared,
            has_login: login,
            owner_id: None,
        }
    }

    /// The strict rule: no #FEED-177 exception in play.
    fn strict(shared: bool) -> MoveSession {
        MoveSession { shared, ..MoveSession::default() }
    }

    /// The refusal order: offline, then visibility, then no login.
    #[test]
    fn refusals_follow_the_contract_order() {
        assert_eq!(refusal(&candidate("a", false, true, false), &strict(false)), Some(REASON_OFFLINE));
        let online = candidate("a", true, true, false);
        assert_eq!(refusal(&online, &strict(false)), Some(REASON_VISIBILITY));
        assert_eq!(refusal(&online, &strict(true)), Some(REASON_NO_LOGIN));
        assert_eq!(refusal(&candidate("a", true, false, true), &strict(false)), None);
        assert_eq!(refusal(&candidate("a", true, true, true), &strict(true)), None);
    }

    /// A current device the candidates do not list (its row never synced,
    /// or the launch list dropped it) still heads the rows, selected and
    /// never refused.
    #[test]
    fn an_unlisted_current_device_still_heads_the_rows() {
        let rows = move_rows(&[candidate("there", true, false, true)], Some("gone"), &strict(false));
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            MoveRow { device_id: "gone".into(), label: "gone".into(), current: true, reason: None }
        );
        assert!(!rows[1].current);
        // No current device at all: nothing is synthesised.
        assert_eq!(move_rows(&[candidate("there", true, false, true)], None, &strict(false)).len(), 1);
        assert_eq!(move_rows(&[candidate("there", true, false, true)], Some(""), &strict(false)).len(), 1);
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
            &strict(false),
        );
        assert!(rows[0].current);
        assert_eq!(rows[0].reason, None);
        assert_eq!(rows[1].reason, None);
        assert_eq!(rows[2].reason, Some(REASON_VISIBILITY));
    }

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/session-device-move.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn reason_key(reason: Option<&str>) -> Option<&'static str> {
        reason.map(|reason| match reason {
            REASON_OFFLINE => "offline",
            REASON_VISIBILITY => "visibility",
            REASON_NO_LOGIN => "noLogin",
            other => panic!("unknown reason {other}"),
        })
    }

    /// #EXP-1541: the copy and the pure rule replay `session-device-move.json`.
    #[test]
    fn replays_the_visibility_and_queued_root_cases() {
        let fixture = fixture();
        assert_eq!(fixture["reasons"]["offline"], REASON_OFFLINE);
        assert_eq!(fixture["reasons"]["visibility"], REASON_VISIBILITY);
        assert_eq!(fixture["reasons"]["noLogin"], REASON_NO_LOGIN);
        for case in fixture["visibilityCases"].as_array().unwrap() {
            let input = &case["input"];
            let flag = |key: &str| input[key].as_bool().unwrap();
            assert_eq!(
                visibility_move(
                    flag("sessionShared"),
                    flag("targetShared"),
                    flag("queuedRoot"),
                    flag("targetIsStarters"),
                    flag("moverIsStarter"),
                ),
                case["expected"].as_bool(),
                "{}",
                case["name"]
            );
        }
        for case in fixture["queuedRootCases"].as_array().unwrap() {
            assert_eq!(
                queued_root(
                    case["status"].as_str(),
                    case["parentId"].as_str(),
                    case["hasSubSessions"].as_bool().unwrap(),
                ),
                case["expected"].as_bool().unwrap(),
                "{case}"
            );
        }
    }

    /// #EXP-1541: the whole per-target refusal replays `targetCases`.
    #[test]
    fn replays_the_target_cases() {
        let fixture = fixture();
        for case in fixture["targetCases"].as_array().unwrap() {
            let device = &case["device"];
            let session = &case["session"];
            let candidate = MoveCandidate {
                device_id: "target".into(),
                label: "Target".into(),
                online: device["online"].as_bool().unwrap(),
                shared_with_team: device["targetShared"].as_bool().unwrap(),
                has_login: device["signedIn"].as_bool().unwrap(),
                // `deviceOwner: null` = a row without an owner (the viewer's own).
                owner_id: candidate_owner(
                    device["deviceOwner"].as_str().map(str::to_string),
                    case["mover"].as_str().unwrap_or_default(),
                ),
            };
            let move_session = MoveSession {
                shared: session["shared"].as_bool().unwrap(),
                queued_root: session["queuedRoot"].as_bool().unwrap(),
                starter_id: session["starter"].as_str().map(str::to_string),
                mover_id: case["mover"].as_str().map(str::to_string),
            };
            assert_eq!(
                reason_key(refusal(&candidate, &move_session)),
                case["expected"].as_str(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn picking_moves_or_does_nothing() {
        assert_eq!(pick_outcome("b", Some("a")), Some("b".into()));
        assert_eq!(pick_outcome("a", Some("a")), None);
        assert_eq!(pick_outcome("b", None), Some("b".into()));
    }
}
