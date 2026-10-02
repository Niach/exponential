//! SLOP-3: the blocked-start dialog, the ONE home of its copy and rules.
//!
//! Starting an issue that something OPEN blocks asks Cancel / Start anyway /
//! Stacked PR, byte-identical x4 (web `lib/blocked-start.ts`, iOS
//! `BlockedStart.swift`, Android `BlockedStart.kt`), locked by
//! `packages/domain-contract/fixtures/blocked-start.json`.
//!
//! The stacked start is PROMPT TEXT only ([`stacked_start_prompt`]): the same
//! base instruction a follow-up run gets from the playbook. No stack plan, no
//! relay frame, no server code. [`stack_target`] picks the one blocker to
//! stack on, or the one reason the button is disabled (first match wins, in
//! the fixture's `reasons` order: batch, many, no-pr, repo).

/// The dialog's title for ONE picked issue.
pub const TITLE: &str = "This issue is blocked";
/// The title when two or more issues were picked.
pub const BATCH_TITLE: &str = "Some of these issues are blocked";
/// The batch body (a batch never stacks, so it has no stackable variant).
pub const BATCH_BODY: &str = "Open issues outside this batch block it. Start anyway?";
/// The one-issue body: prefix, `#IDENT, #IDENT`, suffix.
pub const BODY_PREFIX: &str = "This issue is blocked by ";
/// The suffix while the stacked button is disabled.
pub const BODY_SUFFIX: &str = ". Start anyway?";
/// The suffix while the stacked button is enabled.
pub const BODY_SUFFIX_STACKABLE: &str = ". Start anyway, or start a stacked PR?";
/// The plain start button.
pub const START_ANYWAY: &str = "Start anyway";
/// The stacked start button (the dialog's primary).
pub const STACKED_PR: &str = "Stacked PR";

const PROMPT_TEMPLATE: &str = "Stacked on #{ident}. Before any edit: `git fetch origin {branch}`; if this branch has no commits of its own, `git reset --hard origin/{branch}`, else `git rebase origin/{branch}`. Open your PR with `exponential_pr_open({issueId, base: \"{branch}\"})`.";

/// One OPEN blocker as the stack rule reads it. `repository_id` = the
/// blocker's BOARD's repository.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StackBlocker {
    pub identifier: String,
    pub pr_state: Option<String>,
    pub branch: Option<String>,
    pub repository_id: Option<String>,
}

/// Why "Stacked PR" is disabled, in the fixture's `reasons` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDisabledReason {
    /// Two or more issues picked.
    Batch,
    /// Not exactly one open blocker.
    Many,
    /// The blocker has no open pull request with a recorded branch.
    NoPr,
    /// The subject and the blocker do not share one repository.
    Repo,
}

impl StackDisabledReason {
    /// The fixture's wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Batch => "batch",
            Self::Many => "many",
            Self::NoPr => "no-pr",
            Self::Repo => "repo",
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The blocker a stacked start builds on, or the reason it cannot. Exactly
/// one side of the pair is `Some`.
pub fn stack_target<'a>(
    picked_count: usize,
    subject_repository_id: Option<&str>,
    blockers: &'a [StackBlocker],
) -> (Option<&'a StackBlocker>, Option<StackDisabledReason>) {
    if picked_count > 1 {
        return (None, Some(StackDisabledReason::Batch));
    }
    let [blocker] = blockers else {
        return (None, Some(StackDisabledReason::Many));
    };
    if blocker.pr_state.as_deref() != Some("open") || non_empty(blocker.branch.as_deref()).is_none()
    {
        return (None, Some(StackDisabledReason::NoPr));
    }
    match (non_empty(subject_repository_id), non_empty(blocker.repository_id.as_deref())) {
        (Some(subject), Some(lower)) if subject == lower => (Some(blocker), None),
        _ => (None, Some(StackDisabledReason::Repo)),
    }
}

/// The caption under a disabled "Stacked PR"; `ident` names the blocker for
/// the reasons that are about it.
pub fn stack_disabled_note(reason: StackDisabledReason, ident: &str) -> String {
    match reason {
        StackDisabledReason::Batch => "A stacked PR starts one issue at a time.".to_string(),
        StackDisabledReason::Many => "A stacked PR needs exactly one open blocker.".to_string(),
        StackDisabledReason::NoPr => format!("#{ident} has no open pull request yet."),
        StackDisabledReason::Repo => format!("#{ident} lives in another repository."),
    }
}

/// The one-issue body sentence around plain `#IDENT` identifiers.
pub fn body(identifiers: &[String], stackable: bool) -> String {
    let names: Vec<String> = identifiers
        .iter()
        .map(|identifier| format!("#{identifier}"))
        .collect();
    let suffix = if stackable {
        BODY_SUFFIX_STACKABLE
    } else {
        BODY_SUFFIX
    };
    format!("{BODY_PREFIX}{}{suffix}", names.join(", "))
}

/// The stacked start's prompt: the base instruction, then the typed text
/// (trimmed) after a blank line when there is any.
pub fn stacked_start_prompt(identifier: &str, branch: &str, text: &str) -> String {
    let base = PROMPT_TEMPLATE
        .replace("{ident}", identifier)
        .replace("{branch}", branch);
    let text = text.trim();
    if text.is_empty() {
        base
    } else {
        format!("{base}\n\n{text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        copy: Copy,
        reasons: Vec<String>,
        notes: HashMap<String, String>,
        prompt_template: String,
        target_cases: Vec<TargetCase>,
        prompt_cases: Vec<PromptCase>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Copy {
        title: String,
        batch_title: String,
        batch_body: String,
        body_prefix: String,
        body_suffix: String,
        body_suffix_stackable: String,
        start_anyway: String,
        stacked_pr: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TargetCase {
        name: String,
        picked_count: usize,
        subject_repository_id: Option<String>,
        blockers: Vec<Blocker>,
        target: Option<String>,
        reason: Option<String>,
        note: Option<String>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Blocker {
        identifier: String,
        pr_state: Option<String>,
        branch: Option<String>,
        repository_id: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct PromptCase {
        name: String,
        identifier: String,
        branch: String,
        text: String,
        prompt: String,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/blocked-start.json"
        ))
        .expect("blocked-start.json parses")
    }

    const ALL: [StackDisabledReason; 4] = [
        StackDisabledReason::Batch,
        StackDisabledReason::Many,
        StackDisabledReason::NoPr,
        StackDisabledReason::Repo,
    ];

    #[test]
    fn the_copy_is_byte_locked() {
        let fixture = fixture();
        assert_eq!(TITLE, fixture.copy.title);
        assert_eq!(BATCH_TITLE, fixture.copy.batch_title);
        assert_eq!(BATCH_BODY, fixture.copy.batch_body);
        assert_eq!(BODY_PREFIX, fixture.copy.body_prefix);
        assert_eq!(BODY_SUFFIX, fixture.copy.body_suffix);
        assert_eq!(BODY_SUFFIX_STACKABLE, fixture.copy.body_suffix_stackable);
        assert_eq!(START_ANYWAY, fixture.copy.start_anyway);
        assert_eq!(STACKED_PR, fixture.copy.stacked_pr);
        assert_eq!(PROMPT_TEMPLATE, fixture.prompt_template);
        assert_eq!(
            body(&["ABC-12".to_string(), "ABC-13".to_string()], false),
            "This issue is blocked by #ABC-12, #ABC-13. Start anyway?"
        );
        assert_eq!(
            body(&["ABC-12".to_string()], true),
            "This issue is blocked by #ABC-12. Start anyway, or start a stacked PR?"
        );
    }

    #[test]
    fn the_reasons_and_notes_follow_the_fixture() {
        let fixture = fixture();
        let words: Vec<&str> = ALL.iter().map(|reason| reason.as_str()).collect();
        assert_eq!(words, fixture.reasons);
        for reason in ALL {
            let expected = fixture.notes[reason.as_str()].replace("{ident}", "APP-11");
            assert_eq!(stack_disabled_note(reason, "APP-11"), expected, "{}", reason.as_str());
        }
    }

    #[test]
    fn every_target_case_matches_the_fixture() {
        let fixture = fixture();
        assert!(!fixture.target_cases.is_empty());
        for case in fixture.target_cases {
            let blockers: Vec<StackBlocker> = case
                .blockers
                .iter()
                .map(|row| StackBlocker {
                    identifier: row.identifier.clone(),
                    pr_state: row.pr_state.clone(),
                    branch: row.branch.clone(),
                    repository_id: row.repository_id.clone(),
                })
                .collect();
            let (target, reason) = stack_target(
                case.picked_count,
                case.subject_repository_id.as_deref(),
                &blockers,
            );
            assert_eq!(
                target.map(|blocker| blocker.identifier.clone()),
                case.target,
                "{}",
                case.name
            );
            assert_eq!(
                reason.map(|reason| reason.as_str().to_string()),
                case.reason,
                "{}",
                case.name
            );
            let ident = blockers
                .first()
                .map(|blocker| blocker.identifier.as_str())
                .unwrap_or_default();
            assert_eq!(
                reason.map(|reason| stack_disabled_note(reason, ident)),
                case.note,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn every_prompt_case_matches_the_fixture() {
        let fixture = fixture();
        assert!(!fixture.prompt_cases.is_empty());
        for case in fixture.prompt_cases {
            assert_eq!(
                stacked_start_prompt(&case.identifier, &case.branch, &case.text),
                case.prompt,
                "{}",
                case.name
            );
        }
    }
}
