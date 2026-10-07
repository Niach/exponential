//! EXP-1233 — the Fix merge conflicts builtin as the composer draws it: the
//! picked pull request resolved off the synced issue rows, and the card's
//! `branch → base` line. Pure, so both rules are tests; mirrors web
//! `resolveFixConflictsPr` (`hooks/use-launch-composer.ts`) and `branchLine`
//! (`@exp/ui` `fix-conflicts-card.tsx`), iOS `FixConflictsPr`, Android
//! `FixConflictsPr`.

use crate::rows::Issue;

/// `exp/APP-14 → master`, or the branch alone while the base is unknown;
/// `""` without a branch.
pub fn branch_line(branch: Option<&str>, base_branch: Option<&str>) -> String {
    let Some(branch) = branch.filter(|branch| !branch.is_empty()) else {
        return String::new();
    };
    match base_branch.filter(|base| !base.is_empty()) {
        Some(base) => format!("{branch} → {base}"),
        None => branch.to_string(),
    }
}

/// The pull request picked into the builtin's `pr` input.
#[derive(Clone, Debug, PartialEq)]
pub struct FixConflictsPr {
    /// The representative issue id — the `pr` input's value.
    pub issue_id: String,
    pub pr_number: Option<i64>,
    pub branch: Option<String>,
    pub base_branch: Option<String>,
    /// Every synced issue the pull request links (a batch PR links several),
    /// sorted by identifier — the headline's chips.
    pub issues: Vec<Issue>,
}

impl FixConflictsPr {
    /// The card row's `branch → base` ([`branch_line`]).
    pub fn branch_line(&self) -> String {
        branch_line(self.branch.as_deref(), self.base_branch.as_deref())
    }
}

/// The PR `pr_issue_id` names, off the synced rows: the representative plus
/// every OPEN-PR row sharing its `pr_url` (a batch PR). `None` while nothing
/// is picked or the row is not synced.
pub fn resolve_fix_conflicts_pr<'a>(
    pr_issue_id: Option<&str>,
    issues: impl IntoIterator<Item = &'a Issue>,
) -> Option<FixConflictsPr> {
    let pr_issue_id = pr_issue_id.filter(|id| !id.is_empty())?;
    let issues: Vec<&Issue> = issues.into_iter().collect();
    let representative = *issues.iter().find(|issue| issue.id == pr_issue_id)?;
    let mut linked: Vec<Issue> = match representative.pr_url.as_deref() {
        Some(url) => issues
            .iter()
            .filter(|issue| {
                issue.pr_url.as_deref() == Some(url) && issue.pr_state.as_deref() == Some("open")
            })
            .map(|issue| (*issue).clone())
            .collect(),
        None => Vec::new(),
    };
    if linked.is_empty() {
        linked.push(representative.clone());
    }
    linked.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    Some(FixConflictsPr {
        issue_id: representative.id.clone(),
        pr_number: representative.pr_number,
        branch: representative.branch.clone(),
        base_branch: representative.pr_base_branch.clone(),
        issues: linked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_stack::tests::issue;

    #[test]
    fn branch_line_names_the_base_only_when_known() {
        assert_eq!(branch_line(Some("exp/APP-14"), Some("master")), "exp/APP-14 → master");
        assert_eq!(branch_line(Some("exp/APP-14"), None), "exp/APP-14");
        assert_eq!(branch_line(Some("exp/APP-14"), Some("")), "exp/APP-14");
        assert_eq!(branch_line(None, Some("master")), "");
        assert_eq!(branch_line(Some(""), Some("master")), "");
    }

    #[test]
    fn a_picked_pr_resolves_with_every_issue_it_links() {
        let mut a = issue("APP-21", Some("exp/batch-1"), Some("master"));
        let mut b = issue("APP-20", Some("exp/batch-1"), Some("master"));
        let url = Some("https://github.com/o/r/pull/7".to_string());
        a.pr_url = url.clone();
        b.pr_url = url.clone();
        a.pr_number = Some(7);
        // Same URL but no longer open: not one of the PR's live issues.
        let mut gone = issue("APP-19", Some("exp/batch-1"), Some("master"));
        gone.pr_url = url;
        gone.pr_state = Some("merged".into());
        let other = issue("APP-30", Some("exp/APP-30"), None);
        let rows = vec![a.clone(), other, gone, b];
        let pr = resolve_fix_conflicts_pr(Some(&a.id), &rows).expect("picked");
        assert_eq!(pr.issue_id, a.id);
        assert_eq!(pr.pr_number, Some(7));
        assert_eq!(pr.branch.as_deref(), Some("exp/batch-1"));
        assert_eq!(pr.base_branch.as_deref(), Some("master"));
        let idents: Vec<&str> = pr.issues.iter().map(|issue| issue.identifier.as_str()).collect();
        assert_eq!(idents, ["APP-20", "APP-21"]);
        assert_eq!(pr.branch_line(), "exp/batch-1 → master");
    }

    #[test]
    fn a_lone_pr_is_its_own_issue_and_nothing_picked_is_none() {
        let mut lone = issue("APP-14", Some("exp/APP-14"), None);
        lone.pr_url = None;
        let rows = vec![lone.clone()];
        let pr = resolve_fix_conflicts_pr(Some(&lone.id), &rows).unwrap();
        assert_eq!(pr.issues.len(), 1);
        assert_eq!(pr.branch_line(), "exp/APP-14");
        assert!(resolve_fix_conflicts_pr(None, &rows).is_none());
        assert!(resolve_fix_conflicts_pr(Some(""), &rows).is_none());
        assert!(resolve_fix_conflicts_pr(Some("missing"), &rows).is_none());
    }
}
