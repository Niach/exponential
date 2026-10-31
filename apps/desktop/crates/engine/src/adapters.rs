//! EXP-746 — the ACP adapters.
//!
//! EXP-1426 part 4c: the adapters live in the agent runtime
//! (`eagent::engine::adapters`) and start from its core
//! [`LaunchSpec`]. This module re-exports them under their old paths and keeps
//! Exponential's launch shape, [`AdapterSpec`]: the same fields with the
//! launcher's `coding::LaunchOptions`, converted into the core spec (the
//! account slot becomes the profile id the live usage registry keys by).

use std::path::PathBuf;

pub use eagent::engine::adapters::{
    claude, claude_wire, codex, codex_wire, confine, is_new_conversation_command, Adapter,
    AdapterKind, LaunchSpec, RunOptions, BUILD_MODE_ID, NEW_CONVERSATION_COMMAND, PLAN_MODE_ID,
};

use crate::host::ChildExitLink;
use crate::session::ResumeHandle;

/// Everything an adapter needs to spawn and configure its CLI. Built by the
/// engine from `coding::PreparedLaunch` + its `AcpLaunch`, so an ACP launch
/// inherits the exact program/cwd/env the PTY launch would have used. Every
/// field is the core [`LaunchSpec`]'s; see it for the meaning of each.
pub struct AdapterSpec {
    pub kind: AdapterKind,
    pub agent: coding::CodingAgent,
    /// Program, cwd and env from `PreparedLaunch::spawn`; `args` is EMPTY on
    /// the Acp arm — the adapter composes the ACP argv itself.
    pub spawn: terminal::pty::SpawnSpec,
    pub options: coding::LaunchOptions,
    pub mcp: coding::AgentMcp,
    pub servers: Vec<coding::McpServerWire>,
    pub cwd: PathBuf,
    pub session_id: String,
    pub prompt: Option<String>,
    pub resume: Option<ResumeHandle>,
    pub replay: bool,
    pub personal_key: Option<String>,
    pub reaper_settings_path: Option<PathBuf>,
    pub system_append: String,
    pub context_layers: coding::ContextLayers,
    pub name_conversation: bool,
    pub aside: bool,
    pub confine_to: Option<PathBuf>,
    pub exit: ChildExitLink,
}

/// The launcher's picks as the core run options: the account slot becomes
/// its profile id ([`coding::profile_id`]), the key the live usage registry
/// and the usage cache share.
pub fn run_options(options: &coding::LaunchOptions) -> RunOptions {
    RunOptions {
        model: options.model.clone(),
        effort: options.effort.clone(),
        ultracode: options.ultracode,
        plan_mode: options.plan_mode,
        account_profile: coding::profile_id(options.account.as_deref()),
    }
}

impl From<AdapterSpec> for LaunchSpec {
    fn from(spec: AdapterSpec) -> LaunchSpec {
        LaunchSpec {
            kind: spec.kind,
            agent: spec.agent,
            spawn: spec.spawn,
            options: run_options(&spec.options),
            mcp: spec.mcp,
            servers: spec.servers,
            cwd: spec.cwd,
            session_id: spec.session_id,
            prompt: spec.prompt,
            resume: spec.resume,
            replay: spec.replay,
            personal_key: spec.personal_key,
            reaper_settings_path: spec.reaper_settings_path,
            system_append: spec.system_append,
            context_layers: spec.context_layers,
            name_conversation: spec.name_conversation,
            aside: spec.aside,
            confine_to: spec.confine_to,
            exit: spec.exit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_options_carry_the_picks_and_the_profile_id() {
        let mut options = coding::LaunchOptions {
            agent: coding::CodingAgent::Claude,
            model: "opus".into(),
            effort: "high".into(),
            ultracode: true,
            plan_mode: true,
            subagent_model: String::new(),
            mcp_server_ids: Vec::new(),
            account: Some(" work ".into()),
            computer_use: None,
        };
        let run = run_options(&options);
        assert_eq!(
            run,
            RunOptions {
                model: "opus".into(),
                effort: "high".into(),
                ultracode: true,
                plan_mode: true,
                account_profile: coding::profile_id(Some(" work ")),
            }
        );
        options.account = None;
        assert_eq!(run_options(&options).account_profile, coding::profile_id(None));
    }
}
