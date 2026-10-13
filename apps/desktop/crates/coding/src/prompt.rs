//! The device-side words a launch adds to a session's first turn
//! (masterplan-v3 §7.1 step 5).
//!
//! EXP-1292: the seed prompt itself is the SERVER's — `sessions.brief`
//! composes it (the issue context, the request, the action body) and the
//! launcher sends it verbatim. What stays here is only what the device alone
//! knows: a worktree it had to re-create, and an account switch it made.

/// The first thing a RESUMED run hears when its worktree had to be
/// re-created ([`crate::run_registry::RunRecord::workspace_reclaimed`]):
/// the agent's memory of "my commits are on this branch, my PR is open" may
/// be stale. Prepended to whatever else the resume sends; `alone` (a native
/// resume with no composer text) adds the "wait" so the turn it opens does
/// not put the agent back to work unasked.
///
/// `ahead` = `git rev-list --count origin/<base>..<branch>` in the RE-CREATED
/// worktree, and it decides which of two true stories the agent hears. The
/// automatic reclaimers only delete a branch they found merged, so the branch
/// is cut fresh and `ahead` is 0: the work landed, the PR is closed, new work
/// needs a new PR. A worktree a PERSON removed (the Devices page, or by hand)
/// leaves the branch and its commits alone, so `ahead` is not 0 and the old
/// story would be a lie twice over — it would send the agent to open a second
/// pull request for a head that already has one, which GitHub refuses.
pub fn reclaimed_workspace_note(
    branch: &str,
    default_branch: &str,
    ahead: usize,
    alone: bool,
) -> String {
    let mut note = if ahead == 0 {
        format!(
            "Your worktree was reclaimed after your earlier work landed on `{default_branch}` and has been re-created for this resume: branch `{branch}` now starts fresh from `origin/{default_branch}`, which already contains everything you committed before, so `git log origin/{default_branch}..HEAD` is empty and your previous pull request is closed. Any new change goes on this branch and needs a NEW pull request; if origin still has the old `{branch}`, push with `--force-with-lease`."
        )
    } else {
        let commits = if ahead == 1 { "commit" } else { "commits" };
        format!(
            "Your worktree was removed and has been re-created for this resume: branch `{branch}` still carries its {ahead} {commits} on top of `origin/{default_branch}`, so `git log origin/{default_branch}..HEAD` shows the earlier work and nothing of it was lost. Inspect it before you continue. If you already opened a pull request for `{branch}`, that one is still the PR to update — push to it rather than opening a second one; use `--force-with-lease` if origin disagrees."
        )
    };
    if alone {
        note.push_str(" Nothing has been asked yet: acknowledge this in one line and wait.");
    }
    note
}

/// EXP-935: what a resume that SWITCHES the account closes its first turn
/// with. A switch carries no prompt of its own, so a native reload used to
/// sit idle until a person typed — from the outside, switching the account
/// did nothing at all. The run says why it was restarted and carries on by
/// itself.
pub const ACCOUNT_SWITCH_CONTINUE_PROMPT: &str = "Your account was switched; the previous session \
ended on purpose. Continue exactly where you left off: pick up the task in progress, do not \
restart or re-plan finished work, and report when done.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclaimed_workspace_note_names_the_branch_and_base() {
        let alone = reclaimed_workspace_note("exp/chat-1a2b3c4d", "main", 0, true);
        assert!(alone.contains("branch `exp/chat-1a2b3c4d` now starts fresh from `origin/main`"), "{alone}");
        assert!(alone.contains("needs a NEW pull request"), "{alone}");
        assert!(alone.contains("`--force-with-lease`"), "{alone}");
        assert!(alone.ends_with("acknowledge this in one line and wait."), "{alone}");
        let with_text = reclaimed_workspace_note("exp/chat-1a2b3c4d", "main", 0, false);
        assert!(!with_text.contains("acknowledge"), "{with_text}");
    }

    /// A worktree a person removed keeps the branch: the agent must hear that
    /// its commits survived and that an existing PR is still the one to push
    /// to, never "open a NEW pull request" (GitHub refuses a second PR for
    /// the same head).
    #[test]
    fn reclaimed_workspace_note_says_the_work_survived_when_the_branch_is_ahead() {
        let one = reclaimed_workspace_note("exp/EXP-42", "main", 1, false);
        assert!(one.contains("still carries its 1 commit on"), "{one}");
        assert!(!one.contains("NEW pull request"), "{one}");
        assert!(one.contains("still the PR to update"), "{one}");
        let many = reclaimed_workspace_note("exp/EXP-42", "main", 3, true);
        assert!(many.contains("still carries its 3 commits on"), "{many}");
        assert!(many.contains("nothing of it was lost"), "{many}");
        assert!(many.ends_with("acknowledge this in one line and wait."), "{many}");
    }
}
