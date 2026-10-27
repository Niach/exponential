//! A-16: `device-attention` (3 Special components): THE Devices row — the
//! REAL `machines::device_row` over a fabricated machine of mine with two
//! login chips, an outdated version and the attention card painted OPEN
//! beneath its icon (the card's own renderer, `machines::attention_card`; a
//! gallery has no pointer to hover it open). The lines come from the real
//! derivation (`machines::device_attention`), so they read exactly what
//! `fixtures/devices-list.json` locks. Inert: nothing is wired.

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::machines::{self, AttentionLine, AttentionPill, DeviceCard, DeviceRowProps};
use crate::usage_bar::AgentProfileUsageRow;

pub(crate) const ID: &str = "device-attention";
pub(crate) const OWNER: &str = "A-16";

/// The specimen's width — the Devices page column.
const WIDTH: f32 = 760.;

/// The instance's latest CLI version (the fixture's `latest`).
const LATEST: &str = "0.14.79";

fn login(
    profile_id: &str,
    email: &str,
    plan: &str,
    health: coding::agent_accounts::Health,
    percents: Option<(u8, u8)>,
) -> AgentProfileUsageRow {
    AgentProfileUsageRow {
        key: format!("sg-server:claude:{profile_id}"),
        device_id: "sg-server".to_string(),
        device_label: "build-server".to_string(),
        mine: true,
        online: true,
        agent: "claude".to_string(),
        profile_id: profile_id.to_string(),
        active: health == coding::agent_accounts::Health::Ok,
        signed_in: health == coding::agent_accounts::Health::Ok,
        email: Some(email.to_string()),
        plan: Some(plan.to_string()),
        usage: percents.map(|(session, weekly)| coding::agent_usage::AgentUsage {
            fetched_at: String::new(),
            stale: false,
            windows: vec![
                coding::agent_usage::UsageWindow {
                    key: "session".to_string(),
                    label: "Session".to_string(),
                    percent: session,
                    resets_at: None,
                },
                coding::agent_usage::UsageWindow {
                    key: "weekly".to_string(),
                    label: "Weekly".to_string(),
                    percent: weekly,
                    resets_at: None,
                },
            ],
        }),
        checked_at: None,
        health,
        unmonitored: false,
    }
}

fn device() -> DeviceCard {
    use coding::agent_accounts::Health;
    DeviceCard {
        row_id: "sg-device-attention".to_string(),
        device_id: "sg-server".to_string(),
        label: "build-server".into(),
        server: true,
        icon: None,
        online: true,
        own: false,
        mine: true,
        owner_tooltip: None,
        shared_team_ids: Vec::new(),
        shared: true,
        is_default: true,
        version: Some("0.14.71".to_string()),
        update_requested: false,
        update_blocked: false,
        last_seen_at: None,
        agents: vec!["claude".to_string()],
        unauthed_agents: Vec::new(),
        logins: crate::usage_bar::sort_device_logins(vec![
            login("a", "danny@straehhuber.com", "Max", Health::Ok, Some((34, 61))),
            login("b", "danny@yourev.at", "Pro", Health::NeedsRelogin, None),
        ]),
        accounts_reported: true,
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let device = device();
    let derived = machines::device_attention(&device.list_input(Some(LATEST)));
    // Inert pills: the label shows, nothing runs.
    let lines: Vec<AttentionLine> = derived
        .attention
        .iter()
        .map(|item| AttentionLine {
            text: SharedString::from(item.text.clone()),
            action: item.action.map(|action| AttentionPill {
                label: action.label(),
                busy: false,
                on_click: None,
            }),
        })
        .collect();
    let card = machines::attention_card(&SharedString::from("sg-device-attention-card"), &lines, cx);
    let row = machines::device_row(
        DeviceRowProps {
            index: 9_000,
            device,
            latest: Some(LATEST.to_string()),
            attention: lines,
            no_account: derived.no_account,
            inert: true,
            on_drop_share: None,
        },
        cx,
    );
    let theme = cx.theme();
    v_flex()
        .w(px(WIDTH))
        .gap_1()
        .child(row)
        // The card as the hover paints it: anchored under the icon, right
        // edge to the row's controls.
        .child(
            h_flex().w_full().justify_end().pr(px(44.)).child(
                div()
                    .p_2p5()
                    .rounded(px(theme::tokens::radius::LG))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.popover)
                    .text_color(theme.popover_foreground)
                    .shadow_md()
                    .child(card.into_any_element()),
            ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen shows exactly the two lines the brief names, each with
    /// its pill.
    #[test]
    fn the_specimen_lists_the_relogin_and_the_update() {
        let derived = machines::device_attention(&device().list_input(Some(LATEST)));
        let texts: Vec<&str> = derived.attention.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["danny@yourev.at · Needs re-login", "Update available · v0.14.79"]
        );
        assert!(derived.attention.iter().all(|item| item.action.is_some()));
    }
}
