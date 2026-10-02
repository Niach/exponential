//! EXP-481: device-side execution of the server-authoritative device state —
//! applying `launch_defaults` patches onto the local [`Settings`] (both
//! binaries, desktop + CLI daemon). EXP-1060 (compat round 26) retired the
//! remote `worktree_remove` / `worktree_prune` commands: Settings → Worktrees
//! cleans locally.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;
use crate::settings::Settings;

// ---------------------------------------------------------------------------
// Launch-defaults patches (the devices row's `launch_defaults` jsonb)
// ---------------------------------------------------------------------------

/// One agent's entry in a defaults patch. Only PRESENT fields apply.
///
/// Every `None` is OMITTED on the wire, never an explicit `null` — the
/// server's zod schema rejected nulls with a 400 that silently killed
/// `devices.register` on 0.14.10 daemons (EXP-495). Deserialization still
/// accepts both absent and null.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentDefaultsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// EXP-981: the model this agent's SUBAGENTS run on; claude-only, so it
    /// is OMITTED for every other agent (the capability mask below).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultracode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    /// EXP-1082: `Settings.auto_rotate_accounts` — claude-only, so it is
    /// OMITTED for every other agent and applied only from claude's entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_rotate_accounts: Option<bool>,
}

/// The wire form of the devices row's `launch_defaults` column — the SAME
/// camelCase shape `devices.setLaunchDefaults` accepts and the heartbeat
/// returns. `BTreeMap` keeps serialization deterministic (the fingerprint in
/// [`crate::launch_defaults_sync`] hashes the canonical JSON).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct DefaultsPatch {
    /// EXP-1158: the LAST USED agent. Only the device writes it (the
    /// launcher's stamp, pushed by [`defaults_wire`]); clients' saves omit it
    /// and the server carries the stored value forward.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    pub agents: BTreeMap<String, AgentDefaultsPatch>,
}

/// Apply `patch` onto `settings`, FIELD-wise and ignore-invalid: a value
/// outside the agent's closed vocabulary (or a toggle the agent doesn't
/// support) is dropped without touching the field — a patch must never
/// RESET what it didn't validly set (unlike `normalize_choice`'s load-time
/// fallback). Returns whether anything changed. Persisting stays the
/// caller's `Settings::save` (merge-preserving).
pub fn apply_defaults_patch(settings: &mut Settings, patch: &DefaultsPatch) -> bool {
    fn set_string(slot: &mut String, value: &str, changed: &mut bool) {
        if slot != value {
            *slot = value.to_string();
            *changed = true;
        }
    }
    fn set_bool(slot: &mut bool, value: bool, changed: &mut bool) {
        if *slot != value {
            *slot = value;
            *changed = true;
        }
    }
    let mut changed = false;
    let patch_agent = patch.default_agent.as_deref().and_then(CodingAgent::parse);
    if let Some(agent) = patch_agent {
        if settings.default_agent != agent {
            settings.default_agent = agent;
            changed = true;
        }
    }
    for (agent_id, entry) in &patch.agents {
        let Some(agent) = CodingAgent::parse(agent_id) else {
            continue;
        };
        if let Some(model) = &entry.model {
            let valid = agent.model_values().contains(&model.as_str())
                || (model.is_empty() && agent.allows_blank_model());
            if valid {
                let slot = match agent {
                    CodingAgent::Claude => &mut settings.claude_model,
                    CodingAgent::Codex => &mut settings.codex_model,
                };
                set_string(slot, model, &mut changed);
            }
        }
        if let Some(effort) = &entry.effort {
            if effort.is_empty() || agent.effort_values().contains(&effort.as_str()) {
                let slot = match agent {
                    CodingAgent::Claude => &mut settings.claude_effort,
                    CodingAgent::Codex => &mut settings.codex_effort,
                };
                set_string(slot, effort, &mut changed);
            }
        }
        if let Some(subagent_model) = &entry.subagent_model {
            // EXP-981: blank is the valid "the CLI's own default" value.
            let valid = subagent_model.is_empty()
                || agent.model_values().contains(&subagent_model.as_str());
            if valid && agent.supports_subagent_model() {
                set_string(
                    &mut settings.claude_subagent_model,
                    subagent_model,
                    &mut changed,
                );
            }
        }
        if let Some(ultracode) = entry.ultracode {
            if agent.supports_ultracode() {
                set_bool(&mut settings.claude_ultracode, ultracode, &mut changed);
            }
        }
        if let Some(plan_mode) = entry.plan_mode {
            match agent {
                CodingAgent::Claude => {
                    set_bool(&mut settings.claude_plan_mode, plan_mode, &mut changed)
                }
                CodingAgent::Codex => {}
            }
        }
        if let Some(auto_rotate) = entry.auto_rotate_accounts {
            if agent == CodingAgent::Claude {
                set_bool(&mut settings.auto_rotate_accounts, auto_rotate, &mut changed);
            }
        }
    }
    changed
}

/// EXP-1020: overlay every LAUNCH-DEFAULT field of `from` onto `onto`, and
/// nothing else. THE definition both settings surfaces lean on — the Agents
/// pane (which adds the two CLI paths it also owns) and the Device settings
/// dialog's own-device branch — so the two can never disagree about what a
/// launch default is.
///
/// It exists because the dialog hand-copied the list and a field added later
/// (`claude_subagent_model`) was then saved
/// everywhere EXCEPT on this machine's own row: the hub reseeded the selects
/// from the stale file and `launch_defaults_sync`'s PushLocal later shipped
/// the stale copy back over the server's. One function, one test
/// ([`overlay_carries_every_launch_default`]), no list to forget.
///
/// NOT copied: the CLI paths, `repos_root`, `branch_prefix`, the terminal
/// shell and the UI-state fields — those belong to other panes, and a
/// launch-defaults save must never roll one of them back. Nor (EXP-1158)
/// `default_agent`: it is the LAST USED agent, which only the launcher's
/// stamp writes, so a settings save can never put a stale one back.
pub fn overlay_launch_defaults(onto: &mut Settings, from: &Settings) {
    onto.claude_model = from.claude_model.clone();
    onto.claude_effort = from.claude_effort.clone();
    onto.claude_subagent_model = from.claude_subagent_model.clone();
    onto.codex_model = from.codex_model.clone();
    onto.codex_effort = from.codex_effort.clone();
    onto.claude_ultracode = from.claude_ultracode;
    onto.claude_plan_mode = from.claude_plan_mode;
    onto.auto_rotate_accounts = from.auto_rotate_accounts;
}

/// The PUSH direction: this machine's launch defaults as the full wire
/// object. Covers ALL agents (the server stores configuration even for a
/// not-currently-installed agent — it applies the day the CLI lands),
/// unlike the doctor advertisement's runnable-only map.
pub fn defaults_wire(settings: &Settings) -> DefaultsPatch {
    let mut agents = BTreeMap::new();
    for agent in CodingAgent::ALL {
        agents.insert(
            agent.id().to_string(),
            AgentDefaultsPatch {
                model: Some(settings.model_for(agent).to_string()),
                effort: Some(settings.effort_for(agent).to_string()),
                subagent_model: agent
                    .supports_subagent_model()
                    .then(|| settings.subagent_model_for(agent).to_string()),
                ultracode: agent
                    .supports_ultracode()
                    .then_some(settings.claude_ultracode),
                plan_mode: agent
                    .supports_plan_mode()
                    .then_some(settings.plan_mode_for(agent)),
                auto_rotate_accounts: (agent == CodingAgent::Claude)
                    .then_some(settings.auto_rotate_accounts),
            },
        );
    }
    DefaultsPatch {
        default_agent: Some(settings.default_agent.id().to_string()),
        agents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- apply_defaults_patch --------------------------------------------------

    #[test]
    fn patch_applies_valid_fields_and_ignores_invalid_field_wise() {
        let mut settings = Settings::default();
        let patch: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "defaultAgent": "codex",
            "agents": {
                "claude": { "model": "opus", "ultracode": true },
                // Invalid model must NOT reset the field; the valid toggle
                // beside it still applies. EXP-690: `skipPermissions` is a
                // retired key an old server copy may still carry — it must
                // deserialize and be ignored, never fail the whole patch.
                "codex": { "model": "not-a-model", "skipPermissions": true, "ultracode": true },
                "cursor": { "model": "opus" },
            }
        }))
        .unwrap();
        assert!(apply_defaults_patch(&mut settings, &patch));
        assert_eq!(settings.default_agent, CodingAgent::Codex);
        assert_eq!(settings.claude_model, "opus");
        assert!(settings.claude_ultracode);
        assert_eq!(settings.codex_model, "", "invalid model left untouched");
        // ultracode is claude-only — the codex entry's true was masked.
        // (claude's own entry set it; reset and re-check the mask alone.)
        let mut fresh = Settings::default();
        let codex_only: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": { "codex": { "ultracode": true, "planMode": true } }
        }))
        .unwrap();
        assert!(!apply_defaults_patch(&mut fresh, &codex_only));
        assert!(!fresh.claude_ultracode);
        assert!(fresh.claude_plan_mode, "codex planMode never lands anywhere");
    }

    #[test]
    fn auto_rotate_accounts_is_claude_only() {
        // EXP-1082: the wire carries it on claude's entry alone …
        let wire = serde_json::to_value(defaults_wire(&Settings::default())).unwrap();
        assert_eq!(wire["agents"]["claude"]["autoRotateAccounts"], true);
        assert!(wire["agents"]["codex"].get("autoRotateAccounts").is_none());
        // … a claude patch flips it, a codex one never lands.
        let mut settings = Settings::default();
        let codex_only: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": { "codex": { "autoRotateAccounts": false } }
        }))
        .unwrap();
        assert!(!apply_defaults_patch(&mut settings, &codex_only));
        assert!(settings.auto_rotate_accounts);
        let claude: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": { "claude": { "autoRotateAccounts": false } }
        }))
        .unwrap();
        assert!(apply_defaults_patch(&mut settings, &claude));
        assert!(!settings.auto_rotate_accounts);
    }

    #[test]
    fn blank_model_valid_for_codex_pi_only() {
        let mut settings = Settings::default();
        settings.codex_model = "gpt-5.6-sol".into();
        let patch: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": {
                "codex": { "model": "" },
                "claude": { "model": "" },
            }
        }))
        .unwrap();
        assert!(apply_defaults_patch(&mut settings, &patch));
        assert_eq!(settings.codex_model, "", "blank = CLI default for codex");
        assert_eq!(settings.claude_model, "fable", "claude is explicit-always");
    }

    /// EXP-1158: `defaultAccount` is gone — an old server copy still
    /// carrying it deserializes and is ignored, never fails the patch.
    #[test]
    fn the_retired_exp_872_account_key_is_ignored() {
        let mut settings = Settings::default();
        let patch: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "defaultAgent": "codex",
            "defaultAccount": "0a1b2c3d"
        }))
        .unwrap();
        assert!(apply_defaults_patch(&mut settings, &patch));
        assert_eq!(settings.default_agent, CodingAgent::Codex);
        let wire = serde_json::to_value(defaults_wire(&settings)).unwrap();
        assert_eq!(wire["defaultAgent"], "codex");
        assert!(wire.get("defaultAccount").is_none());
    }

    #[test]
    fn identical_patch_reports_unchanged() {
        let mut settings = Settings::default();
        let wire = defaults_wire(&settings);
        assert!(!apply_defaults_patch(&mut settings, &wire));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn defaults_wire_never_serializes_null() {
        // EXP-495: capability-masked toggles must be OMITTED, not null —
        // the server's zod schema 400'd explicit nulls, silently failing
        // `devices.register` on every 0.14.10 daemon.
        let wire = serde_json::to_value(defaults_wire(&Settings::default())).unwrap();
        let rendered = serde_json::to_string(&wire).unwrap();
        assert!(!rendered.contains("null"), "no nulls on the wire: {rendered}");
        let codex = &wire["agents"]["codex"];
        assert!(codex.get("ultracode").is_none(), "ultracode is claude-only");
        // EXP-981: the subagent pin is claude-only too.
        assert!(
            codex.get("subagentModel").is_none(),
            "the subagent model is claude-only"
        );
        assert!(codex.get("planMode").is_none(), "plan mode is claude-only");
        // EXP-690: the retired key is never advertised on any agent.
        assert!(codex.get("skipPermissions").is_none());
        let claude = &wire["agents"]["claude"];
        assert!(claude.get("skipPermissions").is_none());
        assert!(claude.get("planMode").is_some());
        assert!(claude.get("subagentModel").is_some());
        // EXP-849: pi is gone from the wire entirely.
        assert!(wire["agents"].get("pi").is_none());
    }

    #[test]
    fn defaults_wire_round_trips_through_apply() {
        let mut source = Settings::default();
        source.default_agent = CodingAgent::Codex;
        source.claude_model = "sonnet".into();
        source.claude_ultracode = true;
        source.codex_effort = "high".into();
        source.claude_plan_mode = false;
        let wire = defaults_wire(&source);
        let mut target = Settings::default();
        assert!(apply_defaults_patch(&mut target, &wire));
        assert_eq!(target.default_agent, CodingAgent::Codex);
        assert_eq!(target.claude_model, "sonnet");
        assert!(target.claude_ultracode);
        assert_eq!(target.codex_effort, "high");
        assert!(!target.claude_plan_mode);
    }

    /// EXP-1020: the overlay carries EVERY launch default and touches
    /// nothing else. `from` sets every field away from its default, so a
    /// launch field added later and forgotten in `overlay_launch_defaults`
    /// fails here — extend this fixture when you add one.
    #[test]
    fn overlay_carries_every_launch_default() {
        let from = Settings {
            default_agent: CodingAgent::Codex,
            claude_path: "/usr/local/bin/claude".into(),
            codex_path: "/usr/local/bin/codex".into(),
            repos_root: "/tmp/repos".into(),
            branch_prefix: "wip/".into(),
            claude_model: "sonnet".into(),
            claude_effort: "xhigh".into(),
            claude_subagent_model: "sonnet".into(),
            codex_model: "gpt-5.6-terra".into(),
            codex_effort: "high".into(),
            claude_ultracode: true,
            claude_plan_mode: false,
            auto_rotate_accounts: false,
            terminal_shell: Some("/bin/zsh".into()),
            changelog_seen_id: Some("2026-09-24".into()),
            emoji_recents: vec!["🎉".into()],
            tools_setup_seen: true,
            os_notifications: false,
        };
        assert_ne!(from, Settings::default(), "the fixture must differ everywhere");

        let mut onto = Settings::default();
        overlay_launch_defaults(&mut onto, &from);

        // What a launch-defaults save is allowed to change: everything in
        // `from` EXCEPT the fields other panes own.
        let expected = Settings {
            default_agent: Settings::default().default_agent,
            claude_path: Settings::default().claude_path,
            codex_path: Settings::default().codex_path,
            repos_root: Settings::default().repos_root,
            branch_prefix: Settings::default().branch_prefix,
            terminal_shell: Settings::default().terminal_shell,
            changelog_seen_id: Settings::default().changelog_seen_id,
            emoji_recents: Settings::default().emoji_recents,
            tools_setup_seen: Settings::default().tools_setup_seen,
            os_notifications: Settings::default().os_notifications,
            ..from.clone()
        };
        assert_eq!(onto, expected);
        // The one the Device settings dialog used to drop on its own row.
        assert_eq!(onto.claude_subagent_model, "sonnet");
    }
}
