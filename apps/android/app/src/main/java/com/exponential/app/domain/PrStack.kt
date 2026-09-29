package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity

/**
 * EXP-897: the PR STACK, derived from synced data alone.
 *
 * The edge is `child.prBaseBranch == lower.branch` (both non-empty) inside one
 * repository: a pull request based on another issue's branch sits ON TOP of
 * it. There is no stack table — the column pair IS the model, so every client
 * derives the same chain from the same rows.
 *
 * Mirrored ×4 by name (web `lib/pr-stack.ts`, iOS `PrStack.swift`, desktop
 * `nest_review_entries`) with the same three tests.
 */
object PrStack {
    /** The stack merge's word, byte-identical on all four clients. */
    const val MERGE_STACK_LABEL = "Merge stack"

    /** Where an issue sits in its chain, 1-based from the BOTTOM. */
    data class StackPosition(
        val position: Int,
        val size: Int,
        /** The member directly below (the foundation), null at the bottom. */
        val below: IssueEntity?,
        /** The member directly above, null at the top. */
        val above: IssueEntity?,
    )

    /** One nested list row: the caller's entry, its depth and whether a child follows. */
    data class Nested<T>(
        val entry: T,
        /** 0 for a root, +1 per stack level. */
        val depth: Int,
        /** Whether at least one entry is nested right below. */
        val hasChildren: Boolean,
    )

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

    /** Null when [issue] is not part of a stack (a chain of one). */
    fun stackPosition(issue: IssueEntity, issues: List<IssueEntity>): StackPosition? {
        val chain = stackChain(issue, issues)
        if (chain.size < 2) return null
        val index = chain.indexOfFirst { it.id == issue.id }
        if (index < 0) return null
        return StackPosition(
            position = index + 1,
            size = chain.size,
            below = chain.getOrNull(index - 1),
            above = chain.getOrNull(index + 1),
        )
    }

    /**
     * Nest a list of entries (a review row, an issue, a PR) into stacks:
     * the caller's ROOT order is kept, an entry follows its foundation
     * directly, and depth grows by one per level. An entry whose base nobody
     * in the list owns is a root; a cycle breaks where it first repeats.
     *
     * [branch] is what an entry's pull request is cut FROM for the entries
     * above it; [base] is the branch it is based on.
     */
    fun <T> nestPrStacks(
        entries: List<T>,
        branch: (T) -> String?,
        base: (T) -> String?,
    ): List<Nested<T>> {
        val ownerOf = HashMap<String, Int>()
        entries.forEachIndexed { index, entry ->
            val own = branch(entry)
            if (!own.isNullOrEmpty() && !ownerOf.containsKey(own)) ownerOf[own] = index
        }
        fun parentOf(index: Int): Int? {
            val on = base(entries[index])?.takeIf { it.isNotEmpty() } ?: return null
            val owner = ownerOf[on] ?: return null
            return if (owner == index) null else owner
        }
        val childrenOf = HashMap<Int, MutableList<Int>>()
        val nested = BooleanArray(entries.size)
        entries.indices.forEach { index ->
            val parent = parentOf(index) ?: return@forEach
            childrenOf.getOrPut(parent) { mutableListOf() }.add(index)
            nested[index] = true
        }
        val placed = BooleanArray(entries.size)
        val out = ArrayList<Nested<T>>(entries.size)
        fun visit(index: Int, depth: Int) {
            if (placed[index]) return
            placed[index] = true
            val children = childrenOf[index].orEmpty().filter { !placed[it] }
            out.add(Nested(entries[index], depth, children.isNotEmpty()))
            for (child in children) visit(child, depth + 1)
        }
        entries.indices.forEach { if (!nested[it]) visit(it, 0) }
        // A cycle's members never look like roots — keep them, at depth 0.
        entries.indices.forEach { visit(it, 0) }
        return out
    }

    /** The [IssueEntity] convenience: nest issues by their own PR columns. */
    fun nestPrStacks(issues: List<IssueEntity>): List<Nested<IssueEntity>> =
        nestPrStacks(issues, { it.branch }, { it.prBaseBranch })

    // EXP-1145: a PLAIN Merge control on a stack member asks first. Mirrored
    // x4 by name (web `lib/pr-stack.ts` `stackMergeChoice`, desktop
    // `pr_stack::stack_merge_choice`, iOS `PrStack.stackMergeChoice`), locked
    // by `@exp/domain-contract/fixtures/stack-merge-choice.json`.

    /** The stack dialog's title, byte-identical on all four clients. */
    const val STACK_MERGE_CHOICE_TITLE = "This pull request is part of a stack"

    /** The stack dialog's single-PR merge button. */
    const val MERGE_THIS_PR_LABEL = "Merge this pull request"

    /** The stack dialog's cancel button. */
    const val STACK_MERGE_CANCEL_LABEL = "Cancel"

    private const val THIS_ONE = "this one"

    /** EXP-1145: everything the stack merge dialog says and targets. */
    data class StackMergeChoice(
        /** The chain's OPEN members, bottom to top, one label per pull request (`EXP-874 +2` for a batch PR). */
        val members: List<String>,
        /** 1-based, from the bottom: where the pull request being merged sits. */
        val position: Int,
        /** The bottom member's issue id. */
        val bottomIssueId: String,
        /** The top member's issue id, what `mergePr(mergeStack = true)` takes. */
        val topIssueId: String,
        /** `EXP-1105 → EXP-1144 (this one) → EXP-1150`. */
        val listing: String,
        /** What Merge stack does. */
        val stackSentence: String,
        /** What Merge this pull request does. */
        val thisSentence: String,
        /** The dialog's body: the listing, a blank line, the two sentences. */
        val body: String,
    )

    /**
     * EXP-1145: whether merging [issue]'s pull request from a plain Merge
     * control needs the stack dialog, and everything that dialog says.
     *
     * Null = a plain merge: [issue] has no open pull request, or its stack
     * has no OTHER open member. Only OPEN pull requests form the chain, read
     * in identifier order so every client picks the same representative for
     * a fork or a batch. Mirrored x4, fixture-locked.
     */
    fun stackMergeChoice(issue: IssueEntity, issues: List<IssueEntity>): StackMergeChoice? {
        if (issue.prState != "open") return null
        val open = issues.filter { it.prState == "open" }.sortedWith { a, b -> a.identifier.compareTo(b.identifier) }
        val self = if (open.any { it.id == issue.id }) open else listOf(issue) + open
        val chain = stackChain(issue, self)
        if (chain.size < 2) return null
        val index = chain.indexOfFirst { it.id == issue.id }
        if (index < 0) return null

        // A batch PR's siblings share its url: one label per pull request.
        fun label(member: IssueEntity): String {
            val url = member.prUrl
            val siblings = if (!url.isNullOrEmpty()) self.count { it.id != member.id && it.prUrl == url } else 0
            return if (siblings > 0) "${member.identifier} +$siblings" else member.identifier
        }
        val members = chain.map(::label)
        val own = members[index]
        val below = members.subList(0, index)
        val above = members.subList(index + 1, members.size)

        val listing = members.mapIndexed { at, name -> if (at == index) "$name ($THIS_ONE)" else name }
            .joinToString(" \u2192 ")
        val stackSentence = "$MERGE_STACK_LABEL lands all ${members.size} pull requests, bottom-up."
        val landsBelow = when (below.size) {
            0 -> "$own alone"
            1 -> "$own and the one below it (${below.joinToString(", ")})"
            else -> "$own and the ${below.size} below it (${below.joinToString(", ")})"
        }
        val leftOpen = when (above.size) {
            0 -> ", the whole stack."
            1 -> "; ${above.joinToString(", ")} is retargeted onto the base branch and stays open."
            else -> "; ${above.joinToString(", ")} are retargeted onto the base branch and stay open."
        }
        val thisSentence = "$MERGE_THIS_PR_LABEL lands $landsBelow$leftOpen"

        return StackMergeChoice(
            members = members,
            position = index + 1,
            bottomIssueId = chain.first().id,
            topIssueId = chain.last().id,
            listing = listing,
            stackSentence = stackSentence,
            thisSentence = thisSentence,
            body = "$listing\n\n$stackSentence\n$thisSentence",
        )
    }
}
