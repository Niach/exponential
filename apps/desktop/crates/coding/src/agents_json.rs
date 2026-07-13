//! The `--agents <json>` payload for release runs (EXP-56): one session-scoped
//! subagent definition per issue, carrying the dialog's subagent model/effort
//! defaults. Definitions are SHORT on purpose — the full issue context and
//! contract live in `PROMPT.md` (single source of truth), and the def's
//! prompt just points the subagent at its section and worktree. That keeps
//! the argv small no matter how many issues a release holds.
//!
//! Serialization is byte-stable (BTreeMap → keys sorted by agent name) like
//! [`crate::mcp_json`] — tests can assert exact payloads and reruns diff
//! cleanly.

use serde::Serialize;
use std::collections::BTreeMap;

use crate::release_prompt::ReleasePromptIssue;

/// One `--agents` definition. Field set verified against the Claude Code
/// `--agents` JSON surface (same fields as `.claude/agents/*.md` frontmatter):
/// `description` + `prompt` required; `model`/`effort` optional (inherit the
/// session's when omitted); `background: true` = run concurrently. NO
/// `isolation` key — Claude's worktree isolation branches from the DEFAULT
/// branch, which is the wrong base here; the orchestrator creates each
/// worktree itself from the integration branch (see release_prompt.rs).
#[derive(Serialize)]
struct AgentDef {
    description: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    background: bool,
}

/// Subagent model/effort resolution inputs.
pub struct SubagentDefaults<'a> {
    /// The dialog's subagent model; blank = inherit the session's.
    pub model: &'a str,
    /// The dialog's subagent effort; `None`/blank = inherit the session's.
    pub effort: Option<&'a str>,
}

/// Build the `--agents` JSON for the release's issues — every subagent
/// carries the same `defaults` (blank = inherit the session's).
pub fn build_agents_json(
    issues: &[ReleasePromptIssue],
    defaults: &SubagentDefaults<'_>,
) -> String {
    let mut defs: BTreeMap<String, AgentDef> = BTreeMap::new();
    for issue in issues {
        let model = non_blank(defaults.model).map(str::to_string);
        let effort = defaults.effort.and_then(non_blank).map(str::to_string);
        defs.insert(
            issue.agent_name.clone(),
            AgentDef {
                description: format!("Implement {}: {}", issue.identifier, issue.title),
                prompt: format!(
                    "Please act as the subagent for {id}. Work ONLY inside the worktree at \
{worktree} (branch {branch}) — never the main working directory, never another issue's \
worktree or branch. Read the 'Per-subagent contract' and the '### {id}:' section of \
PROMPT.md at the root of the MAIN working directory for the full issue context, then \
follow that contract exactly.",
                    id = issue.identifier,
                    worktree = issue.worktree,
                    branch = issue.branch,
                ),
                model,
                effort,
                background: true,
            },
        );
    }
    serde_json::to_string(&defs).expect("agents json serialize cannot fail")
}

fn non_blank(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(id: &str) -> ReleasePromptIssue {
        ReleasePromptIssue {
            identifier: id.to_string(),
            title: format!("Title {id}"),
            description: None,
            branch: format!("exp/{id}"),
            worktree: format!("/repos/acme/web.worktrees/exp-{id}"),
            agent_name: id.to_lowercase(),
        }
    }

    #[test]
    fn defs_are_sorted_short_and_carry_model_effort() {
        let issues = [issue("EXP-2"), issue("EXP-1")];
        let json = build_agents_json(
            &issues,
            &SubagentDefaults { model: "opus", effort: Some("high") },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
        // BTreeMap ⇒ sorted by agent name regardless of input order.
        assert_eq!(keys, ["exp-1", "exp-2"]);
        let def = &value["exp-1"];
        assert_eq!(def["description"], "Implement EXP-1: Title EXP-1");
        assert_eq!(def["model"], "opus");
        assert_eq!(def["effort"], "high");
        assert_eq!(def["background"], true);
        // No isolation key — worktrees are orchestrator-created (wrong base
        // otherwise).
        assert!(def.get("isolation").is_none());
        // The prompt points at the worktree + PROMPT.md section, not the
        // full issue body (argv stays small).
        let prompt = def["prompt"].as_str().unwrap();
        assert!(prompt.contains("/repos/acme/web.worktrees/exp-EXP-1"));
        assert!(prompt.contains("### EXP-1:"));
        assert!(prompt.len() < 600);
        // Byte-stable: same input ⇒ same output.
        let again = build_agents_json(
            &issues,
            &SubagentDefaults { model: "opus", effort: Some("high") },
        );
        assert_eq!(json, again);
    }

    #[test]
    fn blank_defaults_are_omitted_rather_than_serialized_empty() {
        let issues = [issue("EXP-1")];
        let json = build_agents_json(
            &issues,
            &SubagentDefaults { model: "  ", effort: Some("") },
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Blank = inherit the session's model/effort — no keys at all.
        assert!(value["exp-1"].get("model").is_none());
        assert!(value["exp-1"].get("effort").is_none());
    }
}
