//! RELEASE-run launch types (EXP-56) — "start coding on a whole release":
//! ONE Claude orchestrator session per (release, repo group), prepared by the
//! unified [`crate::launcher::prepare`] (`PrepareRequest::Release`) and
//! spawned through the same `spawn_prepared_with`.
//!
//! What lives here: the dialog-facing request/option types and the
//! integration-branch naming — `exp/rel-<slug>` with a LOWERCASE slug, so the
//! webhook's issue-identifier parse can never mis-link a release branch
//! (locked by the test below against the ported server regex).

use crate::launcher::LaunchOrigin;

/// The repo group this session works (a workspace-level release may span
/// repos; the dialog launches one orchestrator per group).
#[derive(Clone, Debug)]
pub struct RepoGroup {
    pub repository_id: String,
    /// `owner/name`.
    pub full_name: String,
    pub default_branch: String,
}

/// One selected issue, snapshotted by the dialog from the sync store.
#[derive(Clone, Debug)]
pub struct ReleaseIssueSpec {
    pub issue_id: String,
    /// `EXP-42`.
    pub issue_identifier: String,
    pub title: String,
    pub description: Option<String>,
}

/// The dialog's model/effort/mode choices.
#[derive(Clone, Debug)]
pub struct ReleaseLaunchOptions {
    /// Orchestrator model — any alias; ultracode is model-independent.
    pub main_model: String,
    /// Orchestrator effort; blank/None = omit. Ignored while ultracode is on
    /// (ultracode IS the effort level — `--effort ultracode`).
    pub main_effort: Option<String>,
    /// Subagent defaults (blank = inherit the session's).
    pub subagent_model: String,
    pub subagent_effort: Option<String>,
    /// Dynamic workflows (`--effort ultracode`).
    pub ultracode: bool,
    /// Native plan mode (`--permission-mode plan`) — the orchestrator
    /// presents its wave plan for approval before pushing anything.
    pub plan_mode: bool,
}

/// The dialog's launch input for ONE repo group.
#[derive(Clone, Debug)]
pub struct ReleaseLaunchRequest {
    pub release_id: String,
    pub release_name: String,
    pub repo: RepoGroup,
    /// Only issues resolving to `repo`, already filtered to launchable ones.
    pub issues: Vec<ReleaseIssueSpec>,
    pub device_label: String,
    pub origin: LaunchOrigin,
    pub options: ReleaseLaunchOptions,
}

/// Lowercase, webhook-safe slug for the integration branch: `<name-slug>-<id8>`.
/// The name part keeps `[a-z0-9]` runs joined by single dashes; the first 8
/// chars of the release UUID make the branch unique per release even for
/// same-named releases. EVERY char is lowercase alnum or `-`, so the branch's
/// last path segment can never match the webhook's `[A-Z0-9]+-\d+$` issue
/// parse (locked by a test below against the ported regex).
pub fn release_slug(release_name: &str, release_id: &str) -> String {
    let mut name_part = String::new();
    let mut last_dash = true; // suppress leading dashes
    for ch in release_name.chars() {
        if ch.is_ascii_alphanumeric() {
            name_part.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            name_part.push('-');
            last_dash = true;
        }
    }
    let name_part = name_part.trim_end_matches('-');
    let id_part: String = release_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    if name_part.is_empty() {
        format!("release-{id_part}")
    } else {
        format!("{name_part}-{id_part}")
    }
}

/// `exp/rel-<slug>` — the pushed integration branch. Deliberately NOT the
/// user's branch prefix: the `rel-` marker + lowercase slug are the webhook
/// safety guarantee, independent of prefix configuration.
pub fn release_branch_name(slug: &str) -> String {
    format!("exp/rel-{slug}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- slug / branch webhook safety ----

    /// The server's `parseIssueIdentifierFromBranch` regex, ported: the last
    /// `/`-segment must be ENTIRELY `[A-Z0-9]+-\d+` to link an issue (see
    /// apps/web/src/lib/integrations/pr-sync.ts — its test suite locks the
    /// same release-branch cases). The integration branch must NEVER satisfy
    /// it.
    fn parses_as_issue_branch(branch: &str) -> bool {
        let tail = branch.rsplit('/').next().unwrap_or(branch);
        let Some(dash) = tail.rfind('-') else { return false };
        let (head, digits) = (&tail[..dash], &tail[dash + 1..]);
        !head.is_empty()
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && head
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }

    #[test]
    fn release_branches_can_never_match_the_issue_webhook_parse() {
        for (name, id) in [
            ("0.4", "1dc5fb4a-8923-471c-a940-53094cd33b76"),
            ("July Wave!", "abcdef01-2222-3333-4444-555555555555"),
            ("V2", "12345678-9999-0000-1111-222222222222"),
            ("", "deadbeef-1111-2222-3333-444444444444"),
            ("ÜBER release 7", "00112233-4455-6677-8899-aabbccddeeff"),
        ] {
            let branch = release_branch_name(&release_slug(name, id));
            assert!(
                branch.starts_with("exp/rel-"),
                "branch {branch:?} lost its rel- marker"
            );
            let tail = branch.rsplit('/').next().unwrap();
            assert!(
                tail.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "branch tail must be lowercase-safe: {branch:?}"
            );
            assert!(
                !parses_as_issue_branch(&branch),
                "release branch {branch:?} would link as an issue!"
            );
        }
        // Sanity: the checker itself recognizes real issue branches.
        assert!(parses_as_issue_branch("exp/EXP-42"));
        assert!(!parses_as_issue_branch("exp/rel-0-4-1dc5fb4a"));
    }

    #[test]
    fn slug_is_stable_lowercase_and_unique_per_release() {
        assert_eq!(
            release_slug("0.4", "1dc5fb4a-8923-471c-a940-53094cd33b76"),
            "0-4-1dc5fb4a"
        );
        assert_eq!(
            release_slug("July  Wave!", "abcdef01-1111-2222-3333-444444444444"),
            "july-wave-abcdef01"
        );
        // Same name, different release ⇒ different branch.
        assert_ne!(
            release_slug("0.4", "1dc5fb4a-8923-471c-a940-53094cd33b76"),
            release_slug("0.4", "9dc5fb4a-8923-471c-a940-53094cd33b76")
        );
        // Nothing usable in the name ⇒ id-only fallback.
        assert_eq!(
            release_slug("!!!", "deadbeef-0000-1111-2222-333333333333"),
            "release-deadbeef"
        );
    }
}
