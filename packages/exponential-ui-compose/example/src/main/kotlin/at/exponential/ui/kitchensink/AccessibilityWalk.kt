package at.exponential.ui.kitchensink

import android.view.View
import android.view.ViewGroup
import androidx.compose.ui.platform.ViewRootForTest
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull

/**
 * The walk TalkBack reads, computed from the MERGED semantics tree the
 * way Compose's accessibility delegate orders it
 * (`subtreeSortedByGeometryGrouping`): inside each traversal group the
 * flattened descendants sort geometrically (unclipped top, then left: off-screen nodes keep their place), then stably
 * by `traversalIndex`; a group's own sorted subtree follows it in place.
 * A node is spoken when it merges its descendants (a button, a clickable
 * row) or is a merged leaf with something to say; the label is its
 * content description, else its text, else its editable text. The
 * painter puts `traversalIndex = index` + `isTraversalGroup` on every
 * node, so this is the core's pre-order: what the instrumented order test
 * asserts and the real TalkBack walk is compared against.
 */
object AccessibilityWalk {
    /** The labels of every Compose root under [root], in reading order. */
    fun labels(root: View): List<String> = composeRoots(root).flatMap { r ->
        ordered(listOf(r.semanticsOwner.rootSemanticsNode)).mapNotNull(::label)
    }

    /** The top of the merged tree (depth ≤ 6): id, group, traversal index, bounds, label (debugging the order). */
    fun trace(root: View): List<String> {
        val out = ArrayList<String>()
        fun walk(n: SemanticsNode, depth: Int) {
            if (depth > 6) return
            val c = n.config
            out += "${" ".repeat(depth)}#${n.id} group=${c.getOrNull(SemanticsProperties.IsTraversalGroup) == true} index=${c.getOrNull(SemanticsProperties.TraversalIndex)} merging=${c.isMergingSemanticsOfDescendants} bounds=${n.boundsInRoot} label=${label(n)}"
            n.children.forEach { walk(it, depth + 1) }
        }
        composeRoots(root).forEach { walk(it.semanticsOwner.rootSemanticsNode, 0) }
        return out
    }

    private fun composeRoots(view: View): List<ViewRootForTest> {
        if (view is ViewRootForTest) return listOf(view)
        if (view !is ViewGroup) return emptyList()
        return (0 until view.childCount).flatMap { composeRoots(view.getChildAt(it)) }
    }

    private fun ordered(list: List<SemanticsNode>): List<SemanticsNode> {
        val geometry = ArrayList<SemanticsNode>()
        val groups = HashMap<Int, List<SemanticsNode>>()
        for (n in list) collect(n, geometry, groups)
        val sorted = geometry
            .sortedWith(compareBy({ it.positionInRoot.y }, { it.positionInRoot.x }))
            .sortedBy { it.config.getOrNull(SemanticsProperties.TraversalIndex) ?: 0f }
        val out = ArrayList<SemanticsNode>()
        for (n in sorted) {
            if (focusable(n)) out += n
            groups[n.id]?.let(out::addAll)
        }
        return out
    }

    private fun collect(n: SemanticsNode, geometry: MutableList<SemanticsNode>, groups: MutableMap<Int, List<SemanticsNode>>) {
        if (n.config.getOrNull(SemanticsProperties.HideFromAccessibility) != null) return
        val group = n.config.getOrNull(SemanticsProperties.IsTraversalGroup) == true
        if (group || focusable(n)) geometry += n
        if (group) groups[n.id] = ordered(n.children) else n.children.forEach { collect(it, geometry, groups) }
    }

    private fun focusable(n: SemanticsNode): Boolean =
        n.config.isMergingSemanticsOfDescendants || ((n.children.isEmpty() || n.config.getOrNull(SemanticsProperties.Role) != null) && label(n) != null)

    private fun label(n: SemanticsNode): String? {
        val c = n.config
        c.getOrNull(SemanticsProperties.ContentDescription)?.joinToString(", ")?.takeIf { it.isNotEmpty() }?.let { return it }
        c.getOrNull(SemanticsProperties.Text)?.joinToString(", ") { it.text }?.takeIf { it.isNotEmpty() }?.let { return it }
        return c.getOrNull(SemanticsProperties.EditableText)?.text?.takeIf { it.isNotEmpty() }
    }
}
