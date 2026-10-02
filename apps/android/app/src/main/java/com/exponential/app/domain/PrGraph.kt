package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity

/**
 * EXP-897 part 4 / SLOP-3: ONE model behind the Work screen's related-work
 * badge and the sheet it opens, so the Issue, Run and Changes faces read the
 * same thing.
 *
 * A graph NODE is a pull request, which is either one issue or a BATCH of
 * issues sharing a `pr_url`, which is what lets a batch PR be a stack member.
 * Three bands, all off synced rows: the blockers off the `blocks` relations
 * ([IssueGraph.openBlockersOfSet]), the batch off the shared `pr_url`, the
 * stack off `pr_base_branch`/`branch` ([PrStack]). No new columns.
 *
 * Mirrored x4 by name (web `lib/pr-graph.ts`, iOS `PrGraph.swift`, desktop
 * `pr_graph.rs`).
 */
object PrGraph {
    /** One pull request: a single issue, or every issue sharing its `pr_url`. */
    data class Entry(val issues: List<IssueEntity>) {
        /** The issue a navigation acts on. */
        val representative: IssueEntity get() = issues.first()
        val isBatch: Boolean get() = issues.size > 1
        val identifiers: List<String> get() = issues.map { it.identifier }
    }

    /** One stack member, bottom-first; [depth] is its level above the bottom. */
    data class StackEntry(val entry: Entry, val depth: Int)

    /** The subject's own batch: the issues its ONE pull request spans. */
    data class BatchEntry(val issues: List<IssueEntity>)

    data class Graph(
        /** The chain bottom-first; a lone pull request is a single entry. */
        val stack: List<StackEntry>,
        /** Null unless the subject's own PR spans more than one issue. */
        val batch: BatchEntry?,
        /** The open issues that still block the subject issue. */
        val blockedBy: List<IssueEntity> = emptyList(),
        /** The issue the graph was built for, what `subject` matches on. */
        val subjectIssueId: String? = null,
        /**
         * EXP-1058: the subject pull request's representative in POOL order,
         * the first synced issue on its `pr_url` (web's `entry.issue`), which
         * is not always the subject: [Entry.representative] leads with what
         * the reader opened. Null for a run with no issue at all.
         */
        val lead: IssueEntity? = null,
        /** Whether the graph was built for an ISSUE (not a bare run). */
        val subjectIsIssue: Boolean = false,
    ) {
        /** The subject's own entry, the one the badge counts from. */
        val subject: StackEntry?
            get() = stack.firstOrNull { row -> row.entry.issues.any { it.id == subjectIssueId } }
    }

    enum class BadgeKind { STACK, BATCH, STACK_AND_BATCH }

    /**
     * Build the graph for a subject: an issue, an issue-less run (a batch or
     * chore PR), or both. [issues] and [relations] are the synced pools the
     * caller already has; nothing here reads the network.
     */
    fun build(
        issue: IssueEntity?,
        session: CodingSessionEntity?,
        issues: List<IssueEntity>,
        relations: List<IssueRelationEntity>,
    ): Graph {
        val subjectIssues = when {
            issue != null -> entryIssuesFor(issue, issues)
            else -> session?.prUrl?.takeIf { it.isNotEmpty() }
                ?.let { url -> issues.filter { it.prUrl == url } }
                ?.takeIf { it.isNotEmpty() }
                // EXP-876: a BATCH run's own entry, see [batchSessionIssues].
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
            blockedBy = issue?.let { IssueGraph.openBlockersOfSet(listOf(it.id), relations, issues) }.orEmpty(),
            subjectIssueId = anchor?.id,
            lead = anchor?.let { first ->
                first.prUrl?.takeIf { it.isNotEmpty() }
                    ?.let { url -> issues.firstOrNull { it.prUrl == url } }
                    ?: first
            },
            subjectIsIssue = issue != null,
        )
    }

    /**
     * What the badge wears off the PR relations, or null: a lone
     * single-issue pull request draws no PR badge.
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
     * EXP-1079/EXP-1097: what the header badge DRAWS, the SAME on every face.
     * First match wins:
     *
     *  1. a PR relation ([badgeKind]: stack, batch, stack+batch);
     *  2. [BadgeShape.BLOCKED]: the subject issue has OPEN blockers
     *     ([Graph.blockedBy]);
     *  3. null = no badge. A run family alone earns NO badge.
     */
    enum class BadgeShape { STACK, BATCH, STACK_AND_BATCH, BLOCKED }

    fun badgeShape(graph: Graph): BadgeShape? = when (badgeKind(graph)) {
        BadgeKind.STACK_AND_BATCH -> BadgeShape.STACK_AND_BATCH
        BadgeKind.STACK -> BadgeShape.STACK
        BadgeKind.BATCH -> BadgeShape.BATCH
        null -> if (graph.blockedBy.isNotEmpty()) BadgeShape.BLOCKED else null
    }

    /** EXP-1058: the badge's front [issue] and how many ride behind it ([count], the `+N`). */
    data class BadgeChip(val issue: IssueEntity?, val count: Int)

    /**
     * EXP-1058/EXP-1097: the badge's count, or null exactly when [badgeShape]
     * is null. Face-independent.
     *  - stack / batch: `issue` = the subject pull request's representative
     *    ([Graph.lead]), `count` = every OTHER issue on the stack (all its
     *    entries' issues) or batch;
     *  - blocked: `issue` = the FIRST open blocker in [Graph.blockedBy]
     *    order, `count` = the other open blockers.
     */
    fun badgeChip(graph: Graph): BadgeChip? {
        val shape = badgeShape(graph) ?: return null
        if (shape == BadgeShape.BLOCKED) {
            return BadgeChip(graph.blockedBy.firstOrNull(), graph.blockedBy.size - 1)
        }
        val issue = graph.lead
        if (graph.stack.size >= 2) {
            return BadgeChip(issue, graph.stack.sumOf { it.entry.issues.size } - 1)
        }
        return BadgeChip(issue, (graph.batch?.issues?.size ?: 1) - 1)
    }

    /** One band of the "Related work" sheet. */
    enum class OverlaySection { BLOCKED, BATCH, STACK }

    /**
     * SLOP-16 r5: the batch partners, every issue sharing the subject's
     * `pr_url` but the subject itself. A bare batch run has no subject issue,
     * so its whole covered set is listed.
     */
    fun batchPartners(graph: Graph): List<IssueEntity> =
        graph.batch?.issues.orEmpty()
            .filter { !graph.subjectIsIssue || it.id != graph.subjectIssueId }

    /**
     * SLOP-16 r5: the OTHER pull requests of the subject's stack, BOTTOM-UP,
     * the subject's own pull request excluded. Empty off a real stack.
     */
    fun otherStackEntries(graph: Graph): List<StackEntry> {
        if (graph.stack.size < 2) return emptyList()
        val own = graph.subject
        return graph.stack.filter { it != own }
    }

    /**
     * SLOP-16 r5: the sheet's bands in ONE fixed order (Blocked by, Same pull
     * request, Pull request stack), each only when it has rows. No runs, no
     * graph, no merge.
     */
    fun overlaySections(graph: Graph): List<OverlaySection> = buildList {
        if (graph.blockedBy.isNotEmpty()) add(OverlaySection.BLOCKED)
        if (batchPartners(graph).isNotEmpty()) add(OverlaySection.BATCH)
        if (otherStackEntries(graph).isNotEmpty()) add(OverlaySection.STACK)
    }

    /** SLOP-16 r5: THE "Related work" sheet's copy, byte-identical x4. [EMPTY] is the ONLY empty note. */
    object OverlayCopy {
        const val RELATED_WORK_TITLE = "Related work"
        const val BLOCKED = IssueRelationsView.Copy.BLOCKED_BY
        const val BATCH = "Same pull request"
        const val STACK = "Pull request stack"
        const val EMPTY = "Nothing else is linked to this issue."
    }

    /**
     * EXP-876: a BATCH run's own entry. A batch links no issue, so its
     * covered set (`batch_issue_ids`, else its branch's issues) IS the entry
     * before its PR exists. The PR-grouped entry wins whenever there is one:
     * it carries the branch and the base the stack chains on.
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

    /** Every issue on [issue]'s pull request, itself when it has none. */
    private fun entryIssuesFor(issue: IssueEntity, issues: List<IssueEntity>): List<IssueEntity> {
        val url = issue.prUrl?.takeIf { it.isNotEmpty() } ?: return listOf(issue)
        val shared = issues.filter { it.prUrl == url }
        // The subject always leads its own entry, so `representative` is what
        // the reader opened.
        return listOf(issue) + shared.filter { it.id != issue.id }
    }
}
