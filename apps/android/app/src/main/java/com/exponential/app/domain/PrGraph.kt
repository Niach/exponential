package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity

/**
 * EXP-897 part 4: ONE model behind the Work screen's stack/batch badge and the
 * overlay it opens, so the Issue, Run and Changes faces read the same thing.
 *
 * A graph NODE is a pull request, which is either one issue or a BATCH of
 * issues sharing a `pr_url` — that is what lets a batch PR be a stack member.
 * The stack comes off `pr_base_branch`/`branch` ([PrStack]), the batch off the
 * shared `pr_url`, the tree off `parent_session_id` ([SessionTree]) and the
 * blockers off the `blocks` relations ([StackStart]). No new columns.
 *
 * Mirrored ×4 by name (web `lib/pr-graph.ts`, iOS `PrGraph.swift`, desktop
 * `pr_graph.rs`) with the same four tests.
 */
object PrGraph {
    /** One pull request: a single issue, or every issue sharing its `pr_url`. */
    data class Entry(val issues: List<IssueEntity>) {
        /** The issue a merge / a navigation acts on. */
        val representative: IssueEntity get() = issues.first()
        val isBatch: Boolean get() = issues.size > 1
        val identifiers: List<String> get() = issues.map { it.identifier }
    }

    /** One stack member, bottom-first; [depth] is its level above the bottom. */
    data class StackEntry(val entry: Entry, val depth: Int)

    /** The subject's own batch — the issues its ONE pull request spans. */
    data class BatchEntry(val issues: List<IssueEntity>)

    /**
     * One run of the family, NAMED where it is resolved (EXP-968). The overlay
     * used to join its rows against a second issue snapshot of its own, so a
     * row could read "Issue syncing…" about an issue the very same graph had
     * already resolved; the title now comes off the ONE pool `build` took.
     */
    data class RunRow(
        val session: CodingSessionEntity,
        /** 0 for the family's root, +1 per nesting level. */
        val depth: Int,
        /** Whether at least one child is nested right below. */
        val hasChildren: Boolean,
        /** The run's own issue, when it has one and it has synced. */
        val issue: IssueEntity?,
        /** EXP-876: the issues a BATCH run covers; empty for every other run. */
        val batchIssues: List<IssueEntity>,
        /** What the row is called — the ×4 rule ([pastRunTitle]). */
        val title: String,
    )

    data class Graph(
        /** The chain bottom-first; a lone pull request is a single entry. */
        val stack: List<StackEntry>,
        /** Null unless the subject's own PR spans more than one issue. */
        val batch: BatchEntry?,
        /** The run family, nested and named — the root run and its children. */
        val tree: List<RunRow>,
        /**
         * The issues that still block the subject (`StackStart.openBlockers`).
         * Android/iOS extra over the pinned three fields: the overlay's Issue
         * face lists them, and `build` already takes the relations.
         */
        val blockedBy: List<IssueEntity> = emptyList(),
        /** The issue the graph was built for — what `subject` matches on. */
        val subjectIssueId: String? = null,
    ) {
        /** The subject's own entry — the one the badge counts from. */
        val subject: StackEntry?
            get() = stack.firstOrNull { row -> row.entry.issues.any { it.id == subjectIssueId } }
    }

    enum class BadgeKind { STACK, BATCH, STACK_AND_BATCH }

    /**
     * Build the graph for a subject: an issue, an issue-less run (a batch or
     * chore PR), or both. [issues] and [sessions] are the synced pools the
     * caller already has; nothing here reads the network.
     */
    fun build(
        issue: IssueEntity?,
        session: CodingSessionEntity?,
        issues: List<IssueEntity>,
        sessions: List<CodingSessionEntity>,
        relations: List<IssueRelationEntity>,
    ): Graph {
        val subjectIssues = when {
            issue != null -> entryIssuesFor(issue, issues)
            else -> session?.prUrl?.takeIf { it.isNotEmpty() }
                ?.let { url -> issues.filter { it.prUrl == url } }
                // EXP-876: a BATCH run's own entry — see [batchSessionIssues].
                ?: batchSessionIssues(session, issues)
        }
        val anchor = subjectIssues.firstOrNull()
        val chain = if (anchor == null) emptyList() else PrStack.stackChain(anchor, issues)
        val stack = chain
            .map { Entry(entryIssuesFor(it, issues)) }
            // A batch PR is ONE node however many of its issues sit in the chain.
            .distinctBy { it.representative.prUrl ?: it.representative.id }
            .mapIndexed { index, entry -> StackEntry(entry, index) }
        val batch = subjectIssues.takeIf { it.size > 1 }?.let { BatchEntry(it) }
        return Graph(
            stack = stack,
            batch = batch,
            tree = runRows(familyOf(session, sessions), issues),
            blockedBy = issue?.let { StackStart.openBlockers(it.id, relations, issues) }.orEmpty(),
            subjectIssueId = anchor?.id,
        )
    }

    /**
     * What the badge wears, or null when there is nothing to say: a lone
     * single-issue pull request draws no badge.
     */
    fun badgeKind(graph: Graph): BadgeKind? {
        val stacked = graph.stack.size > 1
        val batched = graph.batch != null
        return when {
            stacked && batched -> BadgeKind.STACK_AND_BATCH
            stacked -> BadgeKind.STACK
            batched -> BadgeKind.BATCH
            else -> null
        }
    }

    /**
     * EXP-876: a BATCH run's own entry. A batch links no issue and stamps no
     * `pr_url` of its own, so before this it resolved nothing at all — the
     * pill and its sheet, the one surface built to name work that spans
     * several issues, never appeared on the very run that spans them. Its
     * covered set (`batch_issue_ids`, else its branch's issues) IS the entry.
     *
     * The PR-grouped entry wins whenever there is one: it carries the branch
     * and the base the stack chains on, so a batch PR stacked on another still
     * reads `stack+batch` and still offers Merge stack. The synthesized entry
     * is what a batch wears BEFORE its PR exists.
     */
    private fun batchSessionIssues(
        session: CodingSessionEntity?,
        issues: List<IssueEntity>,
    ): List<IssueEntity> {
        val covered = session?.let { batchRunIssues(it, issues) }.orEmpty()
        val first = covered.firstOrNull() ?: return emptyList()
        val grouped = entryIssuesFor(first, issues)
        return if (grouped.size > 1) grouped else covered
    }

    /**
     * EXP-968: the family, nested AND named in one pass. Resolving the rows
     * here is what keeps a label and its row on the same snapshot: the sheet
     * renders what `build` decided instead of re-joining a pool of its own.
     */
    private fun runRows(
        family: List<CodingSessionEntity>,
        issues: List<IssueEntity>,
    ): List<RunRow> {
        if (family.isEmpty()) return emptyList()
        val byId = issues.associateBy { it.id }
        return SessionTree.nest(family).map { row ->
            val session = row.session
            val issue = session.issueId?.let(byId::get)
            val batchIssues = batchRunIssues(session, issues)
            RunRow(
                session = session,
                depth = row.depth,
                hasChildren = row.hasChildren,
                issue = issue,
                batchIssues = batchIssues,
                title = pastRunTitle(session, issue, batchIssues),
            )
        }
    }

    /** Every issue on [issue]'s pull request — itself when it has none. */
    private fun entryIssuesFor(issue: IssueEntity, issues: List<IssueEntity>): List<IssueEntity> {
        val url = issue.prUrl?.takeIf { it.isNotEmpty() } ?: return listOf(issue)
        val shared = issues.filter { it.prUrl == url }
        // The subject always leads its own entry, so `representative` is what
        // the reader opened.
        return listOf(issue) + shared.filter { it.id != issue.id }
    }

    /** The run family: [session]'s root and everything under it. */
    private fun familyOf(
        session: CodingSessionEntity?,
        sessions: List<CodingSessionEntity>,
    ): List<CodingSessionEntity> {
        if (session == null) return emptyList()
        val byId = sessions.associateBy { it.id }
        var root = byId[session.id] ?: session
        val climbed = HashSet<String>()
        climbed.add(root.id)
        while (true) {
            val parent = root.parentSessionId?.let { byId[it] } ?: break
            // Defensive: a cycle stops the climb where it first repeats.
            if (!climbed.add(parent.id)) break
            root = parent
        }
        val family = ArrayList<CodingSessionEntity>()
        val placed = HashSet<String>()
        family.add(root)
        placed.add(root.id)
        var index = 0
        while (index < family.size) {
            val current = family[index]
            for (row in sessions) {
                if (row.parentSessionId == current.id && placed.add(row.id)) family.add(row)
            }
            index += 1
        }
        return family
    }
}
