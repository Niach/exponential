//! EXP-1560 (was A-16): `device-attention` (3 Special components), titled
//! "Device card": THE Devices page card — the REAL `machines::device_card`
//! over two fabricated machines of mine in the page's grid (`card_grid`,
//! `device_card_columns`). The first is outdated and holds a login that needs
//! re-login: its icon wears THE run-mark badge and its account preview is
//! painted OPEN beneath it (the preview's own renderer,
//! `machines::account_preview`; a gallery has no pointer to hover it open).
//! The badge and the preview's status come from the real derivation
//! (`machines::device_attention`), so they read exactly what
//! `fixtures/devices-list.json` locks. Inert: nothing is wired. Web twin:
//! `apps/styleguide/src/entries/device-attention.tsx`.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::machines::{
    self, AttentionKind, AttentionPill, DeviceCard, DeviceCardProps, LoginFlag, UpdateLine,
};
use crate::usage_bar::AgentProfileUsageRow;

pub(crate) const ID: &str = "device-attention";
pub(crate) const OWNER: &str = "EXP-1560";

/// The specimen's width — the web island's `max-w-[44rem]`.
const WIDTH: f32 = 704.;

/// The instance's latest desktop version (the fixture's `latest`).
const LATEST: &str = "0.14.79";

/// The badged login's slot in the first card (claude · claude · codex).
const BADGED: usize = 1;

fn window(key: &str, label: &str, percent: u8) -> coding::agent_usage::UsageWindow {
    coding::agent_usage::UsageWindow {
        key: key.to_string(),
        label: label.to_string(),
        percent,
        resets_at: None,
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
        signed_in: health == coding::agent_accounts::Health::Ok,
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
        token_expires_at: None,
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
                "danny@yourev.at",
                "max",
                Health::NeedsRelogin,
                vec![window("session", "Session", 42), window("weekly", "Weekly", 81), window("model:fable", "Fable", 64)],
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
            login("sg-device-mint", "claude", "b", "danny@yourev.at", "max", Health::Ok, Vec::new()),
            login("sg-device-mint", "codex", "c", "ops@yourev.at", "pro", Health::Ok, Vec::new()),
        ],
    )
}

/// The card's props through the real derivation, every action inert.
fn props(index: usize, device: DeviceCard) -> DeviceCardProps {
    let derived = machines::device_attention(&device.list_input(Some(LATEST)));
    let inert = |label| AttentionPill {
        label,
        busy: false,
        on_click: None,
    };
    let mut login_flags: Vec<Option<LoginFlag>> = vec![None; device.logins.len()];
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
                    login_flags[slot] = Some(LoginFlag {
                        status: *status,
                        sign_in: item.action.map(|action| inert(action.label())),
                    });
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
    DeviceCardProps {
        index,
        device,
        latest: Some(LATEST.to_string()),
        login_flags,
        update_line,
        no_account: derived.no_account,
        inert: true,
        accounts_expanded: false,
        on_expand_accounts: None,
        on_drop_share: None,
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let first = props(9_000, macbook());
    let badged_login = first.device.logins[BADGED].clone();
    let badged_flag = first.login_flags[BADGED].clone();
    let preview = machines::account_preview(
        SharedString::from("sg-device-attention-preview"),
        &badged_login,
        badged_flag.as_ref(),
        chrono::Utc::now().timestamp(),
        cx,
    );
    let cards = vec![machines::device_card(first, cx), machines::device_card(props(9_001, mint()), cx)];
    let grid = machines::card_grid(cards, machines::device_card_columns(WIDTH - 24.));
    let theme = cx.theme();
    // The preview as the hover paints it: anchored under the badged icon
    // (card padding + one icon and its gap in from the card's left edge).
    let icon_x = machines::CARD_PADDING + machines::CARD_ICON_SIZE + 4.;
    v_flex()
        .w(px(WIDTH))
        .p_3()
        .gap_1p5()
        .child(grid)
        .child(
            div().pl(px(icon_x)).child(
                div()
                    .p_2p5()
                    .rounded(px(theme::tokens::radius::LG))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.popover)
                    .text_color(theme.popover_foreground)
                    .shadow_md()
                    .child(preview),
            ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen badges exactly the one login the brief names, with its
    /// Sign in, and leads the first card's `⋯` with the update line (a
    /// desktop updates itself: no Update item).
    #[test]
    fn the_specimen_badges_the_relogin_and_carries_the_update_line() {
        let first = props(0, macbook());
        let badged: Vec<usize> = first
            .login_flags
            .iter()
            .enumerate()
            .filter_map(|(slot, flag)| flag.as_ref().map(|_| slot))
            .collect();
        assert_eq!(badged, vec![BADGED]);
        let flag = first.login_flags[BADGED].as_ref().expect("flag");
        assert_eq!(flag.status, machines::LoginStatus::NeedsRelogin);
        assert!(flag.sign_in.is_some());
        let line = first.update_line.as_ref().expect("update line");
        assert_eq!(line.text.as_ref(), "Update available · v0.14.79");
        assert!(line.update.is_none());
        let second = props(1, mint());
        assert!(second.login_flags.iter().all(Option::is_none));
        assert!(second.update_line.is_none());
    }
}
