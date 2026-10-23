//! The team's connected repositories as the composer reads them
//! (`repositories.list`).
//!
//! EXP-1294: this used to be the shared ACTION runner; NEW-8: actions are
//! gone (prompts replaced them), only the repository list it shared stays.

use serde::Deserialize;

/// Slim `repositories.list` row for the repo pickers (the composer's
/// Repository pin, the action `repo` inputs).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionRepoRow {
    pub id: String,
    pub full_name: String,
}

pub(crate) fn fetch_repositories(
    trpc: &api::TrpcClient,
    team_id: &str,
) -> Result<Vec<ActionRepoRow>, api::ApiError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { team_id })
}

/// What the composer's repository picker starts on (EXP-615, shared in
/// EXP-822; EXP-993 made it the FIRST repo whatever the count). There is no
/// "no repository" entry, so the seed has to be a real one.
pub(crate) fn preselect_repo(repos: &[ActionRepoRow]) -> Option<ActionRepoRow> {
    repos.first().cloned()
}
