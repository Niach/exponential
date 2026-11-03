//! #EXP-1656/1658: team skills, the device's half. ONLY the running
//! device's launch read lives here ([`resolve_for_launch`]); the desktop
//! settings UI adds list/get/create/update/delete in its own PR.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One picked skill, as `skills.resolveForLaunch` hands it out: the
/// SKILL.md frontmatter (`name`, `description`) and its markdown `body`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSkill {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
}

/// `skills.resolveForLaunch` output: the session's picks in pick order,
/// plus the picked ids that no longer resolve (deleted since).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillLaunchResolution {
    #[serde(default)]
    pub skills: Vec<LaunchSkill>,
    #[serde(default)]
    pub skipped: Vec<String>,
}

/// `skills.resolveForLaunch` — mutation, the skills the session
/// `session_id` (sent as `X-Exp-Session-Id`, like
/// `mcpServers.resolveForLaunch`) picked. The server answers only for a live
/// row its caller owns or hosts, so the row must exist first.
pub fn resolve_for_launch(trpc: &TrpcClient, session_id: &str) -> Result<SkillLaunchResolution, ApiError> {
    #[derive(Serialize)]
    struct Input {}
    trpc.mutation_in_session("skills.resolveForLaunch", &Input {}, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;

    #[test]
    fn resolve_for_launch_names_the_session_and_parses_the_picks() {
        let (base, rx) = one_shot_server(
            200,
            r#"{"result":{"data":{"skills":[{"id":"k1","name":"review-checklist","description":"Review a PR","body":"Check it."}],"skipped":["k9"]}}}"#,
        );
        let trpc = TrpcClient::new(&base, Arc::new(StaticToken("tok".to_string())));
        let resolution = resolve_for_launch(&trpc, "sess-1").expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/skills.resolveForLaunch"));
        assert!(request.to_ascii_lowercase().contains("x-exp-session-id: sess-1"));
        assert_eq!(resolution.skills.len(), 1);
        assert_eq!(resolution.skills[0].name, "review-checklist");
        assert_eq!(resolution.skills[0].body, "Check it.");
        assert_eq!(resolution.skipped, vec!["k9".to_string()]);
    }
}
