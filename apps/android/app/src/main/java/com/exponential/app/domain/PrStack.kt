package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity

/**
 * EXP-897: the PR STACK chain, derived from synced data alone.
 *
 * The edge is `child.prBaseBranch == lower.branch` (both non-empty): a pull
 * request based on another issue's branch sits ON TOP of it. There is no
 * stack table, the column pair IS the model, so every client derives the same
 * chain from the same rows. SLOP-3: the client chain only, read by the
 * related-work badge ([PrGraph]); no nesting, no stack merge.
 *
 * Mirrored x4 by name (web `lib/pr-stack.ts`, iOS `PrStack.swift`, desktop
 * `pr_stack.rs`).
 */
object PrStack {
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
}
