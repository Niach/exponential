//! EXP-1072 — a workflow's ONE final pull request (integration branch →
//! the repository's default branch) is the workflow's OWN linked PR on every
//! surface, never an "external / not linked" one. These are the few strings
//! every desktop/CLI surface names it with (web `lib/workflow-final-pr-identity.ts`
//! parity): the identifier a fix-conflicts run and its prompt carry, the PR
//! picker's option label, and the Reviews merge-state key.

/// The identifier a workflow's final PR goes by — it reads like the PR's
/// title (`Workflow: <name>`), the fix-conflicts run's subject.
pub fn identifier(name: &str) -> String {
    format!("Workflow: {name}")
}

/// The PR picker's option label for a workflow's final PR:
/// `#829 · Workflow: EXP-996 +5`, or just `Workflow: …` before the PR has a
/// number.
pub fn pick_label(number: Option<i64>, name: &str) -> String {
    match number {
        Some(number) => format!("#{number} · {}", identifier(name)),
        None => identifier(name),
    }
}

/// The Reviews/merge-state key of a workflow's final PR. The `workflow:`
/// prefix never collides with an issue UUID, a `close:`/`session:` key or a
/// `repo#n` pull key.
pub fn review_key(id: &str) -> String {
    format!("workflow:{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_reads_like_the_pr_title() {
        assert_eq!(identifier("EXP-996 +5"), "Workflow: EXP-996 +5");
    }

    #[test]
    fn pick_label_leads_with_the_pr_number_when_there_is_one() {
        assert_eq!(
            pick_label(Some(829), "EXP-996 +5"),
            "#829 · Workflow: EXP-996 +5"
        );
        assert_eq!(pick_label(None, "EXP-996 +5"), "Workflow: EXP-996 +5");
    }

    #[test]
    fn review_key_is_workflow_prefixed() {
        assert_eq!(review_key("wf-1"), "workflow:wf-1");
    }
}
