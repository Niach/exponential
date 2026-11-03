//! #EXP-1660: the typed `skills.*` client (web `apps/web/src/lib/trpc/skills.ts`).
//!
//! A skill = a name, the description that makes the agent load it, a
//! SKILL.md body and `enabledByDefault` (fixtures/skills.json). Listing
//! rides the synced `skills` shape (body EXCLUDED, [`from_row`]); [`get`] is
//! the only body read. Reads are member-gated, writes owner-only.
//! [`resolve_for_launch`] is the running device's launch read (#EXP-1658).
//!
//! Plain blocking calls: run them on a background executor only.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One skill as the wire carries it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The SKILL.md body: EMPTY on shape-synced rows ([`from_row`]) and off
    /// [`list`]; [`get`] returns the real one.
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub enabled_by_default: bool,
    #[serde(default)]
    pub created_by_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
struct ListResponse {
    skills: Vec<Skill>,
}

#[derive(Deserialize)]
struct SkillResponse {
    skill: Skill,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamIdInput<'a> {
    team_id: &'a str,
}

#[derive(Serialize)]
struct IdInput<'a> {
    id: &'a str,
}

/// `skills.list`: by name server-side, body-less.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<Skill>, ApiError> {
    let response: ListResponse = trpc.query_with_input("skills.list", &TeamIdInput { team_id })?;
    Ok(response.skills)
}

/// `skills.get`: the row with its body (member read).
pub fn get(trpc: &TrpcClient, id: &str) -> Result<Skill, ApiError> {
    let response: SkillResponse = trpc.query_with_input("skills.get", &IdInput { id })?;
    Ok(response.skill)
}

/// `skills.create` input (owner only).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCreate {
    pub team_id: String,
    pub name: String,
    pub description: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_by_default: Option<bool>,
}

/// `skills.create` (owner only). A taken name is a CONFLICT
/// (`errors.nameTaken` inline).
pub fn create(trpc: &TrpcClient, input: &SkillCreate) -> Result<Skill, ApiError> {
    let response: SkillResponse = trpc.mutation("skills.create", input)?;
    Ok(response.skill)
}

/// `skills.update` input (strict server-side: only these keys). Omitted
/// fields stay unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdate {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_by_default: Option<bool>,
}

impl SkillUpdate {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), ..Self::default() }
    }
}

/// `skills.update` (owner only).
pub fn update(trpc: &TrpcClient, input: &SkillUpdate) -> Result<Skill, ApiError> {
    let response: SkillResponse = trpc.mutation("skills.update", input)?;
    Ok(response.skill)
}

/// `skills.delete` (owner only). Picks naming it simply stop resolving.
pub fn delete(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Deserialize)]
    struct Ok_ {
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok_ = trpc.mutation("skills.delete", &IdInput { id })?;
    Ok(())
}

/// Hydrate the list projection from a synced `skills` shape row. `body` is
/// deliberately empty: the shape excludes it; [`get`] is the body path.
pub fn from_row(row: &domain::rows::SkillRow) -> Skill {
    Skill {
        id: row.id.clone(),
        team_id: row.team_id.clone().unwrap_or_default(),
        name: row.name.clone(),
        description: row.description.clone(),
        body: String::new(),
        enabled_by_default: row.enabled_by_default,
        created_by_id: row.created_by_id.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

// #EXP-1658: the running device's launch read.

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
    /// #VAPP-213: a built-in's files, written beside SKILL.md
    /// (`<name>/<path>`); empty for a team skill and from older servers.
    #[serde(default)]
    pub files: Vec<LaunchSkillFile>,
}

/// One file of a built-in skill (fixtures/skills.json `builtin`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSkillFile {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub content: String,
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
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    fn request(captured: &flume::Receiver<String>) -> String {
        captured.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn list_and_get_unwrap_their_envelopes() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"skills":[{"id":"s-1","teamId":"t-1","name":"triage","description":"d","enabledByDefault":true}]}}}"#,
        );
        let skills = list(&client(&base), "t-1").unwrap();
        assert_eq!(skills[0].name, "triage");
        assert!(skills[0].enabled_by_default);
        assert!(skills[0].body.is_empty());
        assert!(request(&captured).starts_with("GET /api/trpc/skills.list?input="));

        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"skill":{"id":"s-1","teamId":"t-1","name":"triage","description":"d","body":"# Triage","enabledByDefault":false}}}}"##,
        );
        assert_eq!(get(&client(&base), "s-1").unwrap().body, "# Triage");
        assert!(request(&captured).starts_with("GET /api/trpc/skills.get?input="));
    }

    #[test]
    fn writes_post_only_the_skill_keys() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"skill":{"id":"s-1","teamId":"t-1","name":"triage","description":"d","body":"b"},"txId":1}}}"#,
        );
        let input = SkillCreate {
            team_id: "t-1".into(),
            name: "triage".into(),
            description: "d".into(),
            body: "b".into(),
            enabled_by_default: Some(true),
        };
        create(&client(&base), &input).unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/skills.create HTTP/1.1"), "{text}");
        assert!(
            text.ends_with(r#"{"teamId":"t-1","name":"triage","description":"d","body":"b","enabledByDefault":true}"#),
            "{text}"
        );

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"skill":{"id":"s-1","teamId":"t-1","name":"renamed","description":"d","body":"b"},"txId":2}}}"#,
        );
        let input = SkillUpdate { name: Some("renamed".into()), enabled_by_default: Some(false), ..SkillUpdate::new("s-1") };
        assert_eq!(update(&client(&base), &input).unwrap().name, "renamed");
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/skills.update HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"id":"s-1","name":"renamed","enabledByDefault":false}"#), "{text}");

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true,"id":"s-1","txId":3}}}"#);
        delete(&client(&base), "s-1").unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/skills.delete HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"id":"s-1"}"#), "{text}");
    }

    #[test]
    fn from_row_hydrates_the_synced_projection() {
        let row: domain::rows::SkillRow = serde_json::from_value(serde_json::json!({
            "id": "s-1",
            "team_id": "t-1",
            "name": "triage",
            "description": "Read the issue",
            "enabled_by_default": "true"
        }))
        .unwrap();
        let skill = from_row(&row);
        assert_eq!(skill.team_id, "t-1");
        assert!(skill.enabled_by_default);
        assert!(skill.body.is_empty());
    }

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
