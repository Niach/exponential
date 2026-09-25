//! EXP-1021 — the `picker-account` styleguide entry: the account picker — the brand mark and the login email per row.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-account";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![demo(
        "account — the agent's brand MARK, never its name beside the login",
        |window, cx| {
            let accounts = vec![
                coding::AccountOption {
                    id: "system".to_string(),
                    agent: coding::CodingAgent::Claude,
                    email: "ada@example.com".to_string(),
                    is_device_default: true,
                    health: coding::Health::Ok,
                    limits: None,
                },
                coding::AccountOption {
                    id: "work".to_string(),
                    agent: coding::CodingAgent::Codex,
                    email: "grace@example.com".to_string(),
                    is_device_default: false,
                    health: coding::Health::Ok,
                    limits: None,
                },
            ];
            crate::picker::account_picker::account_picker(
                &accounts,
                Some("claude:system".to_string()),
                chip("sg-picker-account", "ada@example.com", cx),
                inert(),
            )
            .id("sg-picker-account-surface")
            .render(window, cx)
        },
    )])
}
