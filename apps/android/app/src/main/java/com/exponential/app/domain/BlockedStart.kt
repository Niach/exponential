package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity

/**
 * SLOP-3: starting a BLOCKED issue asks Cancel / Start anyway / Stacked PR.
 *
 * Byte-identical x4 (web `lib/blocked-start.ts`, desktop `domain::blocked_start`,
 * iOS `BlockedStart.swift`), locked by the contract fixture
 * `domain-contract/fixtures/blocked-start.json`. A stacked start builds the
 * whole dependency LINE bottom-up and is PROMPT TEXT only
 * ([stackedStartPrompt]): the playbook's follow-up rule carries it from run
 * to run. No stack plan on the server, no relay frame.
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

    /** The primary: start the dependency line bottom-up. */
    const val STACKED_PR = "Stacked PR"

    /** The most issues one stacked start runs (the line above the base + the subject). */
    const val MAX_RUN = 5

    /** The note under an enabled "Stacked PR" whose run has 2+ issues. */
    const val PLAN_NOTE_TEMPLATE = "Starts #{first} first, then #{rest}."

    /** The prompt's first paragraph when the run bases on an open PR branch. */
    const val BASE_TEMPLATE = "Stacked on #{ident}. Before any edit: `git fetch origin {branch}`; " +
        "if this branch has no commits of its own, `git reset --hard origin/{branch}`, " +
        "else `git rebase origin/{branch}`. " +
        "Open your PR with `exponential_pr_open({issueId, base: \"{branch}\"})`."

    /** The paragraph that hands the rest of the line on, run by run. */
    const val LINE_TEMPLATE = "This is the bottom of a stacked line: {line}. After your PR is open " +
        "and your branch is pushed, start #{next} as a follow-up run on your branch " +
        "(playbook: Follow-ups and follow-up runs), even under `no follow-up runs`. " +
        "If more issues follow it, give it this paragraph with your own issue dropped " +
        "from the line. Do not wait for it."

    /** The typed text for the top of a line, carried along unchanged. */
    const val TEXT_TEMPLATE = "Instructions for #{subject}, pass them along unchanged:\n{text}"

    /** Why "Stacked PR" is disabled; first match wins, in this order. */
    enum class Reason(val wire: String) {
        Batch("batch"),
        Cycle("cycle"),
        Many("many"),
        Repo("repo"),
        Long("long"),
        Running("running"),
    }

    // ── Step 1: the line ────────────────────────────────────────────────────

    /** The dependency line below a subject. */
    data class Line(
        /** The issues BELOW the subject, BOTTOM first. */
        val line: List<IssueEntity>,
        /** The issue with more than one open blocker, where the walk stopped. */
        val fork: IssueEntity?,
        /** The walk met an issue twice. */
        val cycle: Boolean,
    )

    /**
     * Walk DOWN from [subjectId] over open direct blockers
     * ([IssueGraph.openBlockersOfSet]): none = stop, exactly one = it joins
     * the line and the walk continues from it, more than one = fork, an issue
     * seen twice = cycle.
     */
    fun stackLine(
        subjectId: String,
        relations: List<IssueRelationEntity>,
        issues: List<IssueEntity>,
    ): Line {
        val byId = issues.associateBy { it.id }
        val seen = hashSetOf(subjectId)
        val below = ArrayList<IssueEntity>()
        var cursor = subjectId
        var fork: IssueEntity? = null
        var cycle = false
        while (true) {
            val blockers = IssueGraph.openBlockersOfSet(listOf(cursor), relations, issues)
            if (blockers.isEmpty()) break
            if (blockers.size > 1) {
                fork = byId[cursor]
                break
            }
            val next = blockers.single()
            if (!seen.add(next.id)) {
                cycle = true
                break
            }
            below.add(next)
            cursor = next.id
        }
        return Line(below.reversed(), fork, cycle)
    }

    // ── Step 2: the plan ────────────────────────────────────────────────────

    /** The picked issue as [stackPlan] reads it. */
    data class Subject(val identifier: String, val repositoryId: String?)

    /** One line member as [stackPlan] reads it. */
    data class Member(
        val identifier: String,
        val prState: String?,
        val branch: String?,
        /** The member's board's repository. */
        val repositoryId: String?,
        /** A LIVE coding session and no open pull request yet. */
        val running: Boolean,
    )

    /** The open PR branch the run bases on. */
    data class Base(val identifier: String, val branch: String)

    /** What a stacked start runs: [run] bottom-first, [run]`[0]` starts. */
    data class Plan(val base: Base?, val run: List<String>)

    /** The plan, or the one reason "Stacked PR" is disabled (+ the issue its note names). */
    data class PlanResult(val plan: Plan?, val reason: Reason?, val ident: String?) {
        val note: String? get() = reason?.let { stackDisabledNote(it, ident.orEmpty()) }
    }

    /** The first matching reason in [Reason] order disables, else the plan. */
    fun stackPlan(
        pickedCount: Int,
        subject: Subject,
        line: List<Member>,
        fork: String?,
        cycle: Boolean,
    ): PlanResult {
        fun refuse(reason: Reason, ident: String? = null) = PlanResult(null, reason, ident)
        if (pickedCount > 1) return refuse(Reason.Batch)
        if (cycle) return refuse(Reason.Cycle)
        if (fork != null) return refuse(Reason.Many, fork)
        val foreign = if (subject.repositoryId == null) {
            line.firstOrNull()
        } else {
            line.firstOrNull { it.repositoryId == null || it.repositoryId != subject.repositoryId }
        }
        if (foreign != null) return refuse(Reason.Repo, foreign.identifier)

        val baseIndex = line.indexOfLast {
            it.prState == DomainContract.prStateOpen && !it.branch.isNullOrEmpty()
        }
        val base = line.getOrNull(baseIndex)?.let { Base(it.identifier, it.branch.orEmpty()) }
        val above = line.drop(baseIndex + 1)
        val run = above.map { it.identifier } + subject.identifier
        if (run.size > MAX_RUN) return refuse(Reason.Long)
        above.firstOrNull { it.running }?.let { return refuse(Reason.Running, it.identifier) }
        return PlanResult(Plan(base, run), null, null)
    }

    /** The note under a disabled "Stacked PR"; [ident] names the issue in the way. */
    fun stackDisabledNote(reason: Reason, ident: String): String = when (reason) {
        Reason.Batch -> "A stacked PR starts one issue at a time."
        Reason.Cycle -> "These issues block each other in a cycle. Remove one relation to stack them."
        Reason.Many -> "#$ident has more than one open blocker. A stacked PR follows one line."
        Reason.Repo -> "#$ident lives in another repository."
        Reason.Long -> "A stacked PR starts at most $MAX_RUN issues in a line."
        Reason.Running -> "#$ident is already running. Its pull request is not open yet."
    }

    /** The note under an enabled "Stacked PR"; null for a run of one. */
    fun stackPlanNote(run: List<String>): String? {
        if (run.size < 2) return null
        return PLAN_NOTE_TEMPLATE
            .replace("{first}", run.first())
            .replace("{rest}", run.drop(1).joinToString(", then #"))
    }

    /**
     * The `prompt` [Plan.run]`[0]` starts with: the base paragraph when there
     * is a base, the line paragraph for a run of 2+, then the trimmed typed
     * text (raw for a run of one, else wrapped for the top of the line),
     * joined by a blank line.
     */
    fun stackedStartPrompt(plan: Plan, text: String): String {
        val parts = ArrayList<String>()
        plan.base?.let { base ->
            parts.add(BASE_TEMPLATE.replace("{ident}", base.identifier).replace("{branch}", base.branch))
        }
        if (plan.run.size >= 2) {
            parts.add(
                LINE_TEMPLATE
                    .replace("{line}", plan.run.joinToString(", then ") { "#$it" })
                    .replace("{next}", plan.run[1]),
            )
        }
        val typed = text.trim()
        if (typed.isNotEmpty()) {
            parts.add(
                if (plan.run.size < 2) {
                    typed
                } else {
                    TEXT_TEMPLATE.replace("{subject}", plan.run.last()).replace("{text}", typed)
                },
            )
        }
        return parts.joinToString("\n\n")
    }
}
