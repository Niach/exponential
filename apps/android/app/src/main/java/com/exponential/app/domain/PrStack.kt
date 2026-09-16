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
}
