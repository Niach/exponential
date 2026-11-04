//! EXP-1560 (was A-16): `device-attention` (3 Special components), titled
//! "Device card": THE Devices page card — the REAL `machines::device_card`
//! over two fabricated machines of mine in the page's grid (`card_grid`,
//! `device_card_columns`). The first is outdated and holds a login that needs
//! re-login: its icon wears THE run-mark badge and #EXP-1643 its account panel
//! is open in the card. The badge and the panel's status come from the real
//! derivation (`machines::device_attention`), so they read exactly what
//! `fixtures/devices-list.json` locks. Inert: nothing is wired. Web twin:
//! `apps/styleguide/src/entries/device-attention.tsx`.

use gpui::{px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::v_flex;

use crate::machines::{self, AttentionKind, AttentionPill, DeviceCard, DeviceCardProps, UpdateLine};
use crate::usage_bar::AgentProfileUsageRow;

pub(crate) const ID: &str = "device-attention";
pub(crate) const OWNER: &str = "EXP-1560";

/// The specimen's width — the web island's `max-w-[44rem]`.
const WIDTH: f32 = 704.;

/// The specimen's height floor — the web island's `min-h-72`.
const MIN_HEIGHT: f32 = 288.;

/// The instance's latest desktop version (the fixture's `latest`).
const LATEST: &str = "0.14.79";

/// The badged login's slot in the first card (claude · claude · codex).
const BADGED: usize = 1;

/// 2h 10m and 3d 14h, with half a minute's room for the time the frame takes
/// (the countdown floors).
const RESETS_SESSION: i64 = 2 * 3600 + 10 * 60 + 30;
const RESETS_WEEK: i64 = 3 * 86_400 + 14 * 3600 + 30;

/// `resets_in` seconds from now: the island's `resets in …` captions.
fn window(key: &str, label: &str, percent: u8, resets_in: i64) -> coding::agent_usage::UsageWindow {
    coding::agent_usage::UsageWindow {
        key: key.to_string(),
        label: label.to_string(),
        percent,
        resets_at: Some((chrono::Utc::now() + chrono::Duration::seconds(resets_in)).to_rfc3339()),
    }
}

fn login(
    device: &str,
    agent: &str,
    profile_id: &str,
    email: &str,
    plan: &str,
    health: coding::agent_accounts::Health,
    windows: Vec<coding::agent_usage::UsageWindow>,
) -> AgentProfileUsageRow {
    AgentProfileUsageRow {
        key: format!("{device}:{agent}:{profile_id}"),
        device_id: device.to_string(),
        device_label: device.to_string(),
        mine: true,
        online: true,
        agent: agent.to_string(),
        profile_id: profile_id.to_string(),
        active: health == coding::agent_accounts::Health::Ok,
        // A dead credential is still a login here: it signs out and in.
        signed_in: health != coding::agent_accounts::Health::SignedOut,
        email: Some(email.to_string()),
        plan: Some(plan.to_string()),
        usage: (!windows.is_empty()).then(|| coding::agent_usage::AgentUsage {
            // Never fresh: the specimen's numbers can never read as a
            // live limit.
            fetched_at: String::new(),
            stale: false,
            windows,
        }),
        checked_at: None,
        health,
        unmonitored: false,
        // EXP-1626: no login is persisted in the specimen (persisting is an
        // option, never a prompt), so only its dead login badges.
        token_expires_at: None,
        usage_from_token: false,
    }
}

fn card(row_id: &str, label: &str, version: &str, logins: Vec<AgentProfileUsageRow>) -> DeviceCard {
    DeviceCard {
        row_id: row_id.to_string(),
        device_id: row_id.to_string(),
        label: label.to_string().into(),
        server: false,
        icon: None,
        online: true,
        own: false,
        mine: true,
        owner_tooltip: None,
        shared_team_ids: Vec::new(),
        shared: false,
        is_default: false,
        version: Some(version.to_string()),
        update_requested: false,
        update_blocked: false,
        last_seen_at: None,
        agents: vec!["claude".to_string(), "codex".to_string()],
        unauthed_agents: Vec::new(),
        // The specimen's order IS the web island's (claude · claude · codex).
        logins,
        accounts_reported: true,
    }
}

fn macbook() -> DeviceCard {
    use coding::agent_accounts::Health;
    card(
        "sg-device-macbook",
        "macbook",
        "0.14.78",
        vec![
            login("sg-device-macbook", "claude", "a", "danny@straehhuber.com", "max", Health::Ok, Vec::new()),
            login(
                "sg-device-macbook",
                "claude",
                "b",
                "alex@acme.example",
                "Max",
                Health::NeedsRelogin,
                vec![
                    window("session", "Session", 42, RESETS_SESSION),
                    window("weekly", "Weekly", 81, RESETS_WEEK),
                    window("model:fable", "Fable", 64, RESETS_WEEK),
                ],
            ),
            login("sg-device-macbook", "codex", "c", "ops@yourev.at", "pro", Health::Ok, Vec::new()),
        ],
    )
}

fn mint() -> DeviceCard {
    use coding::agent_accounts::Health;
    card(
        "sg-device-mint",
        "mint",
        LATEST,
        vec![
            login("sg-device-mint", "claude", "a", "danny@straehhuber.com", "max", Health::Ok, Vec::new()),
            login("sg-device-mint", "claude", "b", "alex@acme.example", "max", Health::Ok, Vec::new()),
            login("sg-device-mint", "codex", "c", "ops@yourev.at", "pro", Health::Ok, Vec::new()),
        ],
    )
}

/// The card's props through the real derivation, every action inert.
/// `open`: the slot of the login whose panel the card shows.
fn props(index: usize, device: DeviceCard, open: Option<usize>) -> DeviceCardProps {
    let derived = machines::device_attention(&device.list_input(Some(LATEST)));
    let inert = |label| AttentionPill {
        label,
        busy: false,
        on_click: None,
    };
    let mut login_flags = vec![None; device.logins.len()];
    let mut update_line = None;
    for item in &derived.attention {
        match &item.kind {
            AttentionKind::Login {
                agent,
                profile_id,
                status,
            } => {
                if let Some(slot) = device
                    .logins
                    .iter()
                    .position(|login| &login.agent == agent && &login.profile_id == profile_id)
                {
                    login_flags[slot] = Some(*status);
                }
            }
            AttentionKind::Update | AttentionKind::UpdateQueued => {
                update_line = Some(UpdateLine {
                    text: SharedString::from(item.text.clone()),
                    update: item.action.map(|action| inert(action.label())),
                });
            }
        }
    }
    let open_login = open.map(|slot| device.logins[slot].key.clone());
    DeviceCardProps {
        index,
        device,
        latest: Some(LATEST.to_string()),
        login_flags,
        update_line,
        no_account: derived.no_account,
        inert: true,
        open_login,
        on_toggle_login: None,
        accounts_expanded: false,
        on_expand_accounts: None,
        on_drop_share: None,
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let cards = vec![
        machines::device_card(props(9_000, macbook(), Some(BADGED)), cx),
        machines::device_card(props(9_001, mint(), None), cx),
    ];
    let grid = machines::card_grid(cards, machines::device_card_columns(WIDTH - 24.));
    v_flex().w(px(WIDTH)).min_h(px(MIN_HEIGHT)).p_3().child(grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen badges exactly the one login the brief names, opens its
    /// panel, and leads the first card's `⋯` with the update line (a desktop
    /// updates itself: no Update item).
    #[test]
    fn the_specimen_badges_the_relogin_and_carries_the_update_line() {
        let first = props(0, macbook(), Some(BADGED));
        let badged: Vec<usize> = first
            .login_flags
            .iter()
            .enumerate()
            .filter_map(|(slot, flag)| flag.as_ref().map(|_| slot))
            .collect();
        assert_eq!(badged, vec![BADGED]);
        assert_eq!(first.login_flags[BADGED], Some(machines::LoginStatus::NeedsRelogin));
        assert_eq!(first.open_login.as_deref(), Some(first.device.logins[BADGED].key.as_str()));
        let line = first.update_line.as_ref().expect("update line");
        assert_eq!(line.text.as_ref(), "Update available · v0.14.79");
        assert!(line.update.is_none());
        let second = props(1, mint(), None);
        assert!(second.login_flags.iter().all(Option::is_none));
        assert!(second.update_line.is_none());
        assert!(second.open_login.is_none());
    }
}
