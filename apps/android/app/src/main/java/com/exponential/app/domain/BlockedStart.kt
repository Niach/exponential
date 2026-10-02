package com.exponential.app.domain

/**
 * SLOP-3: starting a BLOCKED issue asks Cancel / Start anyway / Stacked PR.
 *
 * Byte-identical x4 (web `lib/blocked-start.ts`, desktop `domain::blocked_start`,
 * iOS `BlockedStart.swift`), locked by the contract fixture
 * `domain-contract/fixtures/blocked-start.json`. The stacked start is PROMPT
 * TEXT only ([stackedStartPrompt]): the same base instruction a follow-up run
 * gets from the playbook. No stack plan, no relay frame, no server code.
 */
object BlockedStart {
    // ── The dialog's words (EXP-980 + SLOP-3) ───────────────────────────────

    /** The title when ONE issue was picked. */
    const val TITLE = "This issue is blocked"

    /** The title when two or more issues were picked. */
    const val BATCH_TITLE = "Some of these issues are blocked"

    /** The batch body, above the graph. */
    const val BATCH_BODY = "Open issues outside this batch block it. Start anyway?"

    /** The single-issue body, around the blocker chips: prefix + chips + suffix. */
    const val BODY_PREFIX = "This issue is blocked by "

    /** The suffix while the stacked start is disabled. */
    const val BODY_SUFFIX = ". Start anyway?"

    /** The suffix while the stacked start is enabled. */
    const val BODY_SUFFIX_STACKABLE = ". Start anyway, or start a stacked PR?"

    /** Start the run, blockers and all. */
    const val START_ANYWAY = "Start anyway"

    /** The primary: start the run based on the one blocker's PR branch. */
    const val STACKED_PR = "Stacked PR"

    /** Why "Stacked PR" is disabled; first match wins, in this order. */
    enum class Reason(val wire: String) {
        Batch("batch"),
        Many("many"),
        NoPr("no-pr"),
        Repo("repo"),
    }

    /** One open blocker as [stackTarget] reads it. */
    data class Blocker(
        val identifier: String,
        val prState: String?,
        val branch: String?,
        /** The blocker's board's repository. */
        val repositoryId: String?,
    )

    /** The one blocker to stack on, or the one reason the button is disabled. */
    data class Target(val target: Blocker?, val reason: Reason?)

    /**
     * Which blocker a stacked start bases on: exactly one picked issue,
     * exactly one open blocker, whose pull request is open with a recorded
     * branch, in the subject's (non-null) repository.
     */
    fun stackTarget(
        pickedCount: Int,
        subjectRepositoryId: String?,
        blockers: List<Blocker>,
    ): Target {
        if (pickedCount > 1) return Target(null, Reason.Batch)
        if (blockers.size != 1) return Target(null, Reason.Many)
        val blocker = blockers.single()
        if (blocker.prState != DomainContract.prStateOpen || blocker.branch.isNullOrEmpty()) {
            return Target(null, Reason.NoPr)
        }
        if (subjectRepositoryId == null || blocker.repositoryId != subjectRepositoryId) {
            return Target(null, Reason.Repo)
        }
        return Target(blocker, null)
    }

    /** The note under a disabled "Stacked PR"; [ident] names the blocker. */
    fun stackDisabledNote(reason: Reason, ident: String): String = when (reason) {
        Reason.Batch -> "A stacked PR starts one issue at a time."
        Reason.Many -> "A stacked PR needs exactly one open blocker."
        Reason.NoPr -> "#$ident has no open pull request yet."
        Reason.Repo -> "#$ident lives in another repository."
    }

    /**
     * The run's `prompt` for a stacked start: the base instruction, then the
     * typed text (trimmed) after a blank line when there is any.
     */
    fun stackedStartPrompt(identifier: String, branch: String, text: String): String {
        val base = "Stacked on #$identifier. Before any edit: `git fetch origin $branch`; " +
            "if this branch has no commits of its own, `git reset --hard origin/$branch`, " +
            "else `git rebase origin/$branch`. " +
            "Open your PR with `exponential_pr_open({issueId, base: \"$branch\"})`."
        val typed = text.trim()
        return if (typed.isEmpty()) base else "$base\n\n$typed"
    }
}
