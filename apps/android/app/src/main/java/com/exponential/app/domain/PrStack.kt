package com.exponential.app.domain

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity

/**
 * EXP-897: the PR STACK chain, derived from synced data alone.
 *
 * The edge is `child.prBaseBranch == lower.branch` (both non-empty): a pull
 * request based on another issue's branch sits ON TOP of it. There is no
 * stack table, the column pair IS the model, so every client derives the same
 * chain from the same rows. SLOP-3: the client chain only, read by the
 * related-work badge ([PrGraph]), the stack rail ([stackView]) and the one
 * stack merge confirm ([stackMergeConfirm]).
 *
 * Mirrored x4 by name (web `lib/pr-stack.ts`, iOS `PrStack.swift`, desktop
 * `pr_stack.rs`).
 */
object PrStack {
    /** The stack merge's word, byte-identical on all four clients. */
    const val MERGE_STACK_LABEL = "Merge stack"

    /**
     * The whole chain [issue] belongs to, BOTTOM first and including itself.
     * Walking down stops at a base nobody in [issues] owns; walking up takes
     * the first owner of this branch. A cycle breaks where it first repeats.
     * A lone pull request returns a single-element list.
     */
    fun stackChain(issue: IssueEntity, issues: List<IssueEntity>): List<IssueEntity> {
        val byBranch = HashMap<String, IssueEntity>()
        for (row in issues) {
            val branch = row.branch
            if (!branch.isNullOrEmpty() && !byBranch.containsKey(branch)) byBranch[branch] = row
        }
        val seen = HashSet<String>()
        seen.add(issue.id)

        val below = ArrayList<IssueEntity>()
        var cursor = issue
        while (true) {
            val base = cursor.prBaseBranch?.takeIf { it.isNotEmpty() } ?: break
            val lower = byBranch[base] ?: break
            if (!seen.add(lower.id)) break
            below.add(lower)
            cursor = lower
        }

        val above = ArrayList<IssueEntity>()
        cursor = issue
        while (true) {
            val branch = cursor.branch?.takeIf { it.isNotEmpty() } ?: break
            val upper = issues.firstOrNull { it.prBaseBranch == branch && it.id !in seen } ?: break
            seen.add(upper.id)
            above.add(upper)
            cursor = upper
        }

        return below.reversed() + issue + above
    }

    /**
     * The pool a stack is read from: the issues of ONE team. Branch names
     * (`exp/<IDENTIFIER>`) repeat across teams, so the account's whole issue
     * list can chain one team's pull request onto another's. An issue row
     * carries no team of its own: it is reached through its board (web
     * `use-stack-merge-choice.ts` scopes the same way). No team = no pool.
     */
    fun teamIssues(
        teamId: String?,
        issues: List<IssueEntity>,
        boards: List<BoardEntity>,
    ): List<IssueEntity> {
        if (teamId == null) return emptyList()
        val boardIds = boards.filter { it.teamId == teamId }.mapTo(HashSet()) { it.id }
        return issues.filter { it.boardId in boardIds }
    }

    /**
     * [teamIssues] for the team of [issue]'s board. A board that has not
     * synced yet narrows the pool to that board alone, never widens it.
     */
    fun teamIssues(
        issue: IssueEntity,
        issues: List<IssueEntity>,
        boards: List<BoardEntity>,
    ): List<IssueEntity> {
        val teamId = boards.firstOrNull { it.id == issue.boardId }?.teamId
            ?: return issues.filter { it.boardId == issue.boardId }
        return teamIssues(teamId, issues, boards)
    }

    // ── EXP-1248: tree vs stack, the stack rail, the one confirm ─────────────
    // Mirrored x4 by name (web `lib/pr-stack.ts` prComponent / prGraphShape /
    // openPrShape / stackView / stackMergeConfirm, desktop `domain::pr_stack`,
    // iOS `PrStack.swift`), locked by `fixtures/pr-stack-view.json` and
    // `fixtures/stack-merge-choice.json` (`confirm`).

    /** The ghost action on a stack-rail member (the row's long-press on phones). */
    const val MERGE_THROUGH_LABEL = DomainContract.diffUiMergeThrough

    /** The one confirm's cancel button. */
    const val STACK_CONFIRM_CANCEL_LABEL = "Cancel"

    /**
     * A base-chained component's shape: [Tree] = a fork anywhere (follow-up
     * runs; nests with tree guides), [Stack] = linear (GitHub stacks it; the
     * stack rail), [Single] = one pull request.
     */
    enum class PrGraphShape(val wire: String) { Tree("tree"), Stack("stack"), Single("single") }

    private fun owned(branch: String?): String? = branch?.takeIf { it.isNotEmpty() }

    private fun ownerMap(nodes: List<IssueEntity>): Map<String, IssueEntity> {
        val byBranch = LinkedHashMap<String, IssueEntity>()
        for (node in nodes) {
            val branch = owned(node.branch) ?: continue
            if (!byBranch.containsKey(branch)) byBranch[branch] = node
        }
        return byBranch
    }

    /** Every node base-chained to [node] (either direction), in input order. */
    fun prComponent(node: IssueEntity, nodes: List<IssueEntity>): List<IssueEntity> {
        val byBranch = ownerMap(nodes)
        val seen = hashSetOf(node.id)
        val queue = ArrayDeque(listOf(node))
        while (queue.isNotEmpty()) {
            val current = queue.removeFirst()
            val lower = owned(current.prBaseBranch)?.let { byBranch[it] }
            val branch = owned(current.branch)
            val linked = listOfNotNull(lower) +
                (if (branch == null) emptyList() else nodes.filter { owned(it.prBaseBranch) == branch })
            for (next in linked) {
                if (seen.add(next.id)) queue.addLast(next)
            }
        }
        return nodes.filter { it.id in seen }
    }

    fun prGraphShape(component: List<IssueEntity>): PrGraphShape {
        if (component.size < 2) return PrGraphShape.Single
        val byBranch = ownerMap(component)
        val children = HashMap<String, Int>()
        for (node in component) {
            val parent = owned(node.prBaseBranch)?.let { byBranch[it] } ?: continue
            if (parent.id == node.id) continue
            val count = (children[parent.id] ?: 0) + 1
            if (count > 1) return PrGraphShape.Tree
            children[parent.id] = count
        }
        return PrGraphShape.Stack
    }

    private class OpenPicked(val open: List<IssueEntity>, val reps: List<IssueEntity>, val subject: IssueEntity)

    /**
     * The OPEN rows with ONE representative per pull request (identifier
     * order, so a batch PR is its lowest identifier); null when [issue] has
     * no open pull request.
     */
    private fun openRepresentatives(issue: IssueEntity, issues: List<IssueEntity>): OpenPicked? {
        if (issue.prState != "open") return null
        val open = issues.filter { it.prState == "open" }.toMutableList()
        if (open.none { it.id == issue.id }) open.add(issue)
        open.sortWith { a, b -> a.identifier.compareTo(b.identifier) }
        val repOf = HashMap<String, IssueEntity>()
        val reps = ArrayList<IssueEntity>()
        for (row in open) {
            val url = row.prUrl
            val rep = if (!url.isNullOrEmpty()) reps.firstOrNull { it.prUrl == url } else null
            if (rep != null) {
                repOf[row.id] = rep
            } else {
                reps.add(row)
                repOf[row.id] = row
            }
        }
        return OpenPicked(open, reps, repOf.getValue(issue.id))
    }

    /** The shape of the open component [issue]'s pull request sits in
     *  ([PrGraphShape.Single] without an open pull request). */
    fun openPrShape(issue: IssueEntity, issues: List<IssueEntity>): PrGraphShape {
        val picked = openRepresentatives(issue, issues) ?: return PrGraphShape.Single
        return prGraphShape(prComponent(picked.subject, picked.reps))
    }

    private class OpenStack(val chain: List<IssueEntity>, val subject: IssueEntity, val open: List<IssueEntity>) {
        fun siblings(row: IssueEntity): Int =
            row.prUrl?.takeIf { it.isNotEmpty() }?.let { url -> open.count { it.id != row.id && it.prUrl == url } } ?: 0
    }

    /** The linear open stack [issue] sits in, bottom → top. */
    private fun openStack(issue: IssueEntity, issues: List<IssueEntity>): OpenStack? {
        val picked = openRepresentatives(issue, issues) ?: return null
        if (prGraphShape(prComponent(picked.subject, picked.reps)) != PrGraphShape.Stack) return null
        return OpenStack(stackChain(picked.subject, picked.reps), picked.subject, picked.open)
    }

    data class StackViewRow(
        val issueId: String,
        val identifier: String,
        val title: String,
        val prNumber: Int?,
        /** The subject's pull request: the row wears the active wash. */
        val isCurrent: Boolean,
    )

    data class StackView(
        /** TOP first, one row per open pull request. */
        val rows: List<StackViewRow>,
        /** The trailing muted row: the bottom member's base; null = unknown. */
        val baseBranch: String?,
    )

    /**
     * The stack rail of [issue]'s pull request (the Guide's Stack card, a
     * Reviews stack group): null unless it sits in a linear open stack of 2+
     * pull requests. Only OPEN pull requests count; a fork = a tree = null.
     */
    fun stackView(issue: IssueEntity, issues: List<IssueEntity>): StackView? {
        val stack = openStack(issue, issues) ?: return null
        if (stack.chain.size < 2) return null
        return StackView(
            rows = stack.chain.reversed().map { row ->
                StackViewRow(
                    issueId = row.id,
                    identifier = row.identifier,
                    title = row.title,
                    prNumber = row.prNumber,
                    isCurrent = row.id == stack.subject.id,
                )
            },
            baseBranch = owned(stack.chain.first().prBaseBranch),
        )
    }

    /** [Stack] = the merge control (the whole open chain, through its top);
     *  [Through] = Merge through here on this member. */
    enum class StackConfirmMode(val wire: String) { Stack("stack"), Through("through") }

    data class StackMergeConfirm(
        /** The title AND the primary button. */
        val title: String,
        /** What lands, bottom first; a batch PR = `EXP-874 +2`. */
        val landing: List<String>,
        /** What stays open above it (GitHub retargets it). */
        val staysOpen: List<String>,
        val body: String,
        /** The issue `issues.mergePr({ mergeStack: true })` takes. */
        val issueId: String,
    )

    /**
     * EXP-1248: the ONE confirm a stack merge asks (it replaces the 3-way
     * dialog). Null = not in a linear open stack: the plain merge confirm.
     */
    fun stackMergeConfirm(issue: IssueEntity, issues: List<IssueEntity>, mode: StackConfirmMode): StackMergeConfirm? {
        val stack = openStack(issue, issues) ?: return null
        if (stack.chain.size < 2) return null
        fun label(row: IssueEntity): String {
            val extra = stack.siblings(row)
            return if (extra > 0) "${row.identifier} +$extra" else row.identifier
        }
        val index = stack.chain.indexOfFirst { it.id == stack.subject.id }
        val through = if (mode == StackConfirmMode.Stack) stack.chain.size - 1 else index
        val landing = stack.chain.subList(0, through + 1).map(::label)
        val staysOpen = stack.chain.subList(through + 1, stack.chain.size).map(::label)
        val lands = if (landing.size == 1) {
            "Lands 1 pull request: ${landing[0]}."
        } else {
            "Lands ${landing.size} pull requests, bottom-up: ${landing.joinToString(", ")}."
        }
        val open = if (staysOpen.isEmpty()) {
            ""
        } else {
            " ${staysOpen.joinToString(", ")} ${if (staysOpen.size == 1) "stays" else "stay"} open."
        }
        return StackMergeConfirm(
            title = if (mode == StackConfirmMode.Stack) MERGE_STACK_LABEL else MERGE_THROUGH_LABEL,
            landing = landing,
            staysOpen = staysOpen,
            body = "$lands$open",
            issueId = if (mode == StackConfirmMode.Stack) stack.chain.last().id else issue.id,
        )
    }
}
