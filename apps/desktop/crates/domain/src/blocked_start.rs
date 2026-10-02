//! SLOP-3: the blocked-start dialog, the ONE home of its copy and rules.
//!
//! Starting an issue that something OPEN blocks asks Cancel / Start anyway /
//! Stacked PR, byte-identical x4 (web `lib/blocked-start.ts`, iOS
//! `BlockedStart.swift`, Android `BlockedStart.kt`), locked by
//! `packages/domain-contract/fixtures/blocked-start.json`.
//!
//! A stacked start builds the whole dependency LINE bottom-up and is PROMPT
//! TEXT only ([`stacked_start_prompt`]): the playbook's follow-up rule
//! carries it from run to run. No stack plan on the server, no relay frame.
//! Two pure steps: [`stack_line`] walks down from the subject over its open
//! direct blockers; [`stack_plan`] picks the base and the issues to run, or
//! the one reason the button is disabled (first match wins, in the fixture's
//! `reasons` order: batch, cycle, many, repo, long, running).

use std::collections::HashSet;

use crate::issue_graph::{open_blockers_of_set, GraphIssue, GraphRelation};

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

/// The most issues one stacked start runs in a line.
pub const MAX_RUN: usize = 5;

const PLAN_NOTE_TEMPLATE: &str = "Starts #{first} first, then #{rest}.";
const BASE_TEMPLATE: &str = "Stacked on #{ident}. Before any edit: `git fetch origin {branch}`; if this branch has no commits of its own, `git reset --hard origin/{branch}`, else `git rebase origin/{branch}`. Open your PR with `exponential_pr_open({issueId, base: \"{branch}\"})`.";
const LINE_TEMPLATE: &str = "This is the bottom of a stacked line: {line}. After your PR is open and your branch is pushed, start #{next} as a follow-up run on your branch (playbook: Follow-ups and follow-up runs), even under `no follow-up runs`. If more issues follow it, give it this paragraph with your own issue dropped from the line. Do not wait for it.";
const TEXT_TEMPLATE: &str = "Instructions for #{subject}, pass them along unchanged:\n{text}";

// ---------------------------------------------------------------------------
// Step 1: the line
// ---------------------------------------------------------------------------

/// What [`stack_line`] found below the subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackLine<'a> {
    /// The issues BELOW the subject, BOTTOM first.
    pub line: Vec<GraphIssue<'a>>,
    /// The identifier of the issue with more than one open blocker.
    pub fork: Option<&'a str>,
    /// The walk met an issue twice.
    pub cycle: bool,
}

/// Walk DOWN from `subject_id` over open direct blockers (the
/// [`open_blockers_of_set`] rule): none = stop; exactly one = it joins the
/// line and the walk continues from it; more than one = fork; an issue seen
/// twice = cycle.
pub fn stack_line<'a>(
    subject_id: &str,
    relations: &[GraphRelation<'a>],
    issues: &[GraphIssue<'a>],
) -> StackLine<'a> {
    let identifier_of = |id: &str| -> Option<&'a str> {
        issues
            .iter()
            .find(|issue| issue.id == id)
            .map(|issue| issue.identifier)
    };
    let mut seen: HashSet<String> = HashSet::from([subject_id.to_string()]);
    let mut below: Vec<GraphIssue<'a>> = Vec::new();
    let mut fork = None;
    let mut cycle = false;
    let mut cursor = subject_id.to_string();
    loop {
        let blockers = open_blockers_of_set(&[cursor.as_str()], relations, issues);
        match blockers.as_slice() {
            [] => break,
            [blocker] => {
                if !seen.insert(blocker.id.to_string()) {
                    cycle = true;
                    break;
                }
                below.push(*blocker);
                cursor = blocker.id.to_string();
            }
            _ => {
                fork = identifier_of(&cursor);
                break;
            }
        }
    }
    below.reverse();
    StackLine {
        line: below,
        fork,
        cycle,
    }
}

// ---------------------------------------------------------------------------
// Step 2: the plan
// ---------------------------------------------------------------------------

/// The subject as the plan reads it. `repository_id` = its BOARD's.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StackSubject {
    pub identifier: String,
    pub repository_id: Option<String>,
}

/// One line member as the plan reads it. `running` = it has a LIVE run and
/// no open pull request yet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StackMember {
    pub identifier: String,
    pub pr_state: Option<String>,
    pub branch: Option<String>,
    pub repository_id: Option<String>,
    pub running: bool,
}

/// The open pull request a stacked start builds on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackBase {
    pub identifier: String,
    pub branch: String,
}

/// What "Stacked PR" does: start `run[0]` (on `base` when set), the rest
/// follow as follow-up runs. The last of `run` is the subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackPlan {
    pub base: Option<StackBase>,
    pub run: Vec<String>,
}

/// Why "Stacked PR" is disabled, in the fixture's `reasons` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDisabledReason {
    /// Two or more issues picked.
    Batch,
    /// The line loops.
    Cycle,
    /// An issue in the line has more than one open blocker.
    Many,
    /// A line member does not share the subject's repository.
    Repo,
    /// More than [`MAX_RUN`] issues to start.
    Long,
    /// A run member below the subject is running without an open PR.
    Running,
}

impl StackDisabledReason {
    /// Every reason, in the order the first match wins.
    pub const ALL: [StackDisabledReason; 6] = [
        Self::Batch,
        Self::Cycle,
        Self::Many,
        Self::Repo,
        Self::Long,
        Self::Running,
    ];

    /// The fixture's wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Batch => "batch",
            Self::Cycle => "cycle",
            Self::Many => "many",
            Self::Repo => "repo",
            Self::Long => "long",
            Self::Running => "running",
        }
    }
}

/// A disabled "Stacked PR": the reason and the issue its note names (empty
/// for the reasons that name none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackRefusal {
    pub reason: StackDisabledReason,
    pub ident: String,
}

impl StackRefusal {
    /// The caption under the disabled button.
    pub fn note(&self) -> String {
        stack_disabled_note(self.reason, &self.ident)
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The plan for a stacked start, or why there is none.
pub fn stack_plan(
    picked_count: usize,
    subject: &StackSubject,
    line: &[StackMember],
    fork: Option<&str>,
    cycle: bool,
) -> Result<StackPlan, StackRefusal> {
    let refuse = |reason, ident: &str| {
        Err(StackRefusal {
            reason,
            ident: ident.to_string(),
        })
    };
    if picked_count > 1 {
        return refuse(StackDisabledReason::Batch, "");
    }
    if cycle {
        return refuse(StackDisabledReason::Cycle, "");
    }
    if let Some(fork) = fork {
        return refuse(StackDisabledReason::Many, fork);
    }
    let subject_repo = non_empty(subject.repository_id.as_deref());
    let foreign = match subject_repo {
        None => line.first(),
        Some(repo) => line
            .iter()
            .find(|member| non_empty(member.repository_id.as_deref()) != Some(repo)),
    };
    if let Some(member) = foreign {
        return refuse(StackDisabledReason::Repo, &member.identifier);
    }
    let base_at = line.iter().rposition(|member| {
        member.pr_state.as_deref() == Some("open") && non_empty(member.branch.as_deref()).is_some()
    });
    let above = match base_at {
        Some(at) => &line[at + 1..],
        None => line,
    };
    if above.len() + 1 > MAX_RUN {
        return refuse(StackDisabledReason::Long, "");
    }
    if let Some(member) = above.iter().find(|member| member.running) {
        return refuse(StackDisabledReason::Running, &member.identifier);
    }
    let base = base_at.map(|at| StackBase {
        identifier: line[at].identifier.clone(),
        branch: non_empty(line[at].branch.as_deref())
            .unwrap_or_default()
            .to_string(),
    });
    let mut run: Vec<String> = above.iter().map(|member| member.identifier.clone()).collect();
    run.push(subject.identifier.clone());
    Ok(StackPlan { base, run })
}

/// The caption under a disabled "Stacked PR"; `ident` names the issue for
/// the reasons that are about one.
pub fn stack_disabled_note(reason: StackDisabledReason, ident: &str) -> String {
    match reason {
        StackDisabledReason::Batch => "A stacked PR starts one issue at a time.".to_string(),
        StackDisabledReason::Cycle => {
            "These issues block each other in a cycle. Remove one relation to stack them."
                .to_string()
        }
        StackDisabledReason::Many => {
            format!("#{ident} has more than one open blocker. A stacked PR follows one line.")
        }
        StackDisabledReason::Repo => format!("#{ident} lives in another repository."),
        StackDisabledReason::Long => {
            format!("A stacked PR starts at most {MAX_RUN} issues in a line.")
        }
        StackDisabledReason::Running => {
            format!("#{ident} is already running. Its pull request is not open yet.")
        }
    }
}

/// The caption under the graph while "Stacked PR" is enabled and starts 2+
/// issues: `Starts #A first, then #B, then #C.`; `None` for a run of one.
pub fn stack_plan_note(run: &[String]) -> Option<String> {
    let (first, rest) = run.split_first()?;
    if rest.is_empty() {
        return None;
    }
    Some(
        PLAN_NOTE_TEMPLATE
            .replace("{first}", first)
            .replace("{rest}", &rest.join(", then #")),
    )
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

/// The prompt `run[0]` starts with: the base paragraph when there is a base,
/// the line paragraph when 2+ issues run, then the trimmed typed text (raw
/// for a run of one, else wrapped for the subject), joined by blank lines.
pub fn stacked_start_prompt(plan: &StackPlan, text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(base) = &plan.base {
        parts.push(
            BASE_TEMPLATE
                .replace("{ident}", &base.identifier)
                .replace("{branch}", &base.branch),
        );
    }
    if plan.run.len() >= 2 {
        let line = plan
            .run
            .iter()
            .map(|identifier| format!("#{identifier}"))
            .collect::<Vec<_>>()
            .join(", then ");
        parts.push(
            LINE_TEMPLATE
                .replace("{line}", &line)
                .replace("{next}", &plan.run[1]),
        );
    }
    let text = text.trim();
    if !text.is_empty() {
        if plan.run.len() <= 1 {
            parts.push(text.to_string());
        } else {
            let subject = plan.run.last().map(String::as_str).unwrap_or_default();
            parts.push(
                TEXT_TEMPLATE
                    .replace("{subject}", subject)
                    .replace("{text}", text),
            );
        }
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        copy: Copy,
        max_run: usize,
        reasons: Vec<String>,
        notes: HashMap<String, String>,
        plan_note_template: String,
        base_template: String,
        line_template: String,
        text_template: String,
        plan_cases: Vec<PlanCase>,
        line_cases: Vec<LineCase>,
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
    struct PlanCase {
        name: String,
        picked_count: usize,
        subject: Subject,
        line: Vec<Member>,
        fork: Option<String>,
        cycle: bool,
        plan: Option<Plan>,
        reason: Option<String>,
        note: Option<String>,
        plan_note: Option<String>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Subject {
        identifier: String,
        repository_id: Option<String>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Member {
        identifier: String,
        pr_state: Option<String>,
        branch: Option<String>,
        repository_id: Option<String>,
        running: bool,
    }

    #[derive(serde::Deserialize)]
    struct Plan {
        base: Option<Base>,
        run: Vec<String>,
    }

    #[derive(serde::Deserialize)]
    struct Base {
        identifier: String,
        branch: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LineCase {
        name: String,
        subject: String,
        relations: Vec<Relation>,
        issues: Vec<LineIssue>,
        line: Vec<String>,
        fork: Option<String>,
        cycle: bool,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Relation {
        #[serde(rename = "type")]
        kind: String,
        issue_id: String,
        related_issue_id: String,
    }

    #[derive(serde::Deserialize)]
    struct LineIssue {
        id: String,
        identifier: String,
        status: String,
    }

    #[derive(serde::Deserialize)]
    struct PromptCase {
        name: String,
        base: Option<Base>,
        run: Vec<String>,
        text: String,
        prompt: String,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/blocked-start.json"
        ))
        .expect("blocked-start.json parses")
    }

    fn to_plan(plan: Plan) -> StackPlan {
        StackPlan {
            base: plan.base.map(|base| StackBase {
                identifier: base.identifier,
                branch: base.branch,
            }),
            run: plan.run,
        }
    }

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
        assert_eq!(MAX_RUN, fixture.max_run);
        assert_eq!(PLAN_NOTE_TEMPLATE, fixture.plan_note_template);
        assert_eq!(BASE_TEMPLATE, fixture.base_template);
        assert_eq!(LINE_TEMPLATE, fixture.line_template);
        assert_eq!(TEXT_TEMPLATE, fixture.text_template);
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
        let words: Vec<&str> = StackDisabledReason::ALL
            .iter()
            .map(|reason| reason.as_str())
            .collect();
        assert_eq!(words, fixture.reasons);
        assert_eq!(fixture.notes.len(), StackDisabledReason::ALL.len());
        for reason in StackDisabledReason::ALL {
            let expected = fixture.notes[reason.as_str()].replace("{ident}", "APP-11");
            assert_eq!(stack_disabled_note(reason, "APP-11"), expected, "{}", reason.as_str());
        }
    }

    #[test]
    fn every_line_case_matches_the_fixture() {
        let fixture = fixture();
        assert!(!fixture.line_cases.is_empty());
        for case in &fixture.line_cases {
            let relations: Vec<GraphRelation<'_>> = case
                .relations
                .iter()
                .map(|row| GraphRelation {
                    kind: &row.kind,
                    issue_id: &row.issue_id,
                    related_issue_id: &row.related_issue_id,
                })
                .collect();
            let issues: Vec<GraphIssue<'_>> = case
                .issues
                .iter()
                .map(|row| GraphIssue {
                    id: &row.id,
                    identifier: &row.identifier,
                    status: &row.status,
                })
                .collect();
            let found = stack_line(&case.subject, &relations, &issues);
            let line: Vec<&str> = found.line.iter().map(|issue| issue.identifier).collect();
            assert_eq!(line, case.line, "{}", case.name);
            assert_eq!(found.fork, case.fork.as_deref(), "{}", case.name);
            assert_eq!(found.cycle, case.cycle, "{}", case.name);
        }
    }

    #[test]
    fn every_plan_case_matches_the_fixture() {
        let fixture = fixture();
        assert!(!fixture.plan_cases.is_empty());
        for case in fixture.plan_cases {
            let subject = StackSubject {
                identifier: case.subject.identifier.clone(),
                repository_id: case.subject.repository_id.clone(),
            };
            let line: Vec<StackMember> = case
                .line
                .iter()
                .map(|row| StackMember {
                    identifier: row.identifier.clone(),
                    pr_state: row.pr_state.clone(),
                    branch: row.branch.clone(),
                    repository_id: row.repository_id.clone(),
                    running: row.running,
                })
                .collect();
            let result = stack_plan(
                case.picked_count,
                &subject,
                &line,
                case.fork.as_deref(),
                case.cycle,
            );
            let (plan, refusal) = match result {
                Ok(plan) => (Some(plan), None),
                Err(refusal) => (None, Some(refusal)),
            };
            assert_eq!(plan, case.plan.map(to_plan), "{}", case.name);
            assert_eq!(
                refusal.as_ref().map(|refusal| refusal.reason.as_str().to_string()),
                case.reason,
                "{}",
                case.name
            );
            assert_eq!(
                refusal.as_ref().map(StackRefusal::note),
                case.note,
                "{}",
                case.name
            );
            assert_eq!(
                plan.as_ref().and_then(|plan| stack_plan_note(&plan.run)),
                case.plan_note,
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
            let plan = to_plan(Plan {
                base: case.base,
                run: case.run,
            });
            assert_eq!(
                stacked_start_prompt(&plan, &case.text),
                case.prompt,
                "{}",
                case.name
            );
        }
    }
}
