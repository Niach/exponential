package com.exponential.app.domain

/**
 * EXP-1087: what the workflow page's chip strip has selected — `All` (null)
 * or ONE node. Pure, so the page's selection rules are testable without a
 * view: tap selects, tapping the selected chip again goes back to All, and
 * stepping walks the strip in DAG order (waves, then lanes) with All at
 * position 0. Mirrors iOS `WorkflowSelection.swift`.
 */
data class WorkflowSelection(
    /** The selected node id; null = All. */
    val nodeId: String? = null,
) {
    val isAll: Boolean get() = nodeId == null

    /** Select one node, or All with null. */
    fun select(id: String?): WorkflowSelection = WorkflowSelection(id)

    /** A chip tap: the selected chip goes back to All, any other is selected. */
    fun toggle(id: String): WorkflowSelection = WorkflowSelection(if (nodeId == id) null else id)

    /**
     * The position in the strip, All = 0, the first node = 1. A selected node
     * that is not in [order] (it left the workflow) reads as All.
     */
    fun position(order: List<String>): Int {
        val id = nodeId ?: return 0
        val index = order.indexOf(id)
        return if (index < 0) 0 else index + 1
    }

    /** Step [delta] chips along the strip, clamped to All … the last node. */
    fun step(delta: Int, order: List<String>): WorkflowSelection {
        val target = (position(order) + delta).coerceIn(0, order.size)
        return WorkflowSelection(if (target == 0) null else order[target - 1])
    }

    /** A node that left the workflow (dismissed, re-planned away) falls back to All. */
    fun reconcile(order: List<String>): WorkflowSelection =
        if (nodeId != null && nodeId !in order) WorkflowSelection() else this

    companion object {
        /** The strip's chip ids in DAG order — the order `nodeStrip` draws them. */
        fun order(strip: List<StripWave>): List<String> = strip.flatMap { wave -> wave.nodes.map { it.id } }
    }
}
