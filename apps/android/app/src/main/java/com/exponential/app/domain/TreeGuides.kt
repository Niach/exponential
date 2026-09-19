package com.exponential.app.domain

/**
 * EXP-965: the CONNECTOR every nested list draws — the run tree on the Agent
 * page and in its Recent sheet, the run family and the pull requests in the
 * stack overlay, the stack members in Reviews. Nesting used to be indent
 * alone, so at 14dp a level a child read as a slightly shifted stranger.
 *
 * The rule, mirrored ×4 (web `lib/tree-guides.ts`, iOS `TreeGuides.swift`,
 * desktop `domain::tree_guides`) and drawn here by `Modifier.treeGuides`:
 *
 * 1. Indent stays [INDENT_DP] per level, so a row at depth d owns the gutter
 *    band `[INDENT_DP * (d - 1), INDENT_DP * d]` of its PARENT's level.
 * 2. That band carries the elbow: a vertical from the row's top edge to its
 *    vertical centre, a rounded turn to the right, a stub to the band's right
 *    edge — just before the child's own leading glyph.
 * 3. A row with a later sibling continues the vertical to its bottom edge
 *    (the tee); the last child stops at the elbow.
 * 4. Every ANCESTOR level whose subtree continues after this row draws a
 *    straight full-height line in its own band, so a deep row still hangs off
 *    everything above it.
 * 5. A root draws nothing, and a folded parent's children are not rows at all.
 */
data class TreeGuide(
    /** The gutter level this row's elbow sits in — null for a root. */
    val elbowAt: Int?,
    /** A later sibling follows: the elbow's vertical runs the whole height. */
    val tee: Boolean,
    /** Ancestor gutter levels whose subtree continues below this row. */
    val passThrough: List<Int>,
) {
    /** Nothing to draw at all — every root row. */
    val isEmpty: Boolean get() = elbowAt == null && passThrough.isEmpty()

    companion object {
        val None = TreeGuide(elbowAt = null, tee = false, passThrough = emptyList())
    }
}

object TreeGuides {
    /** One nesting level of indent, the ×4 number (EXP-897). */
    const val INDENT_DP = 14

    /**
     * The guide for every row of a flattened tree, given the VISIBLE rows'
     * depths in order — folded-away rows are simply absent, which is what
     * makes a folded parent read as the last child of its band.
     */
    fun compute(depths: List<Int>): List<TreeGuide> =
        depths.mapIndexed { index, depth ->
            if (depth <= 0) {
                TreeGuide.None
            } else {
                TreeGuide(
                    elbowAt = depth - 1,
                    tee = continues(depths, index, depth - 1),
                    passThrough = (0 until depth - 1).filter { continues(depths, index, it) },
                )
            }
        }

    /**
     * Whether the branch in gutter [level] carries on below row [index] — is
     * there a later row at `level + 1` (a sibling of this row's ancestor
     * there) before the tree climbs back out of that subtree?
     */
    private fun continues(depths: List<Int>, index: Int, level: Int): Boolean {
        for (next in index + 1 until depths.size) {
            val depth = depths[next]
            if (depth <= level) return false
            if (depth == level + 1) return true
        }
        return false
    }
}
