package com.exponential.app.domain

/**
 * EXP-1084: the workflow page's PICKER model — pure, mirrored ×4 (web
 * `lib/workflow-selection.ts`, desktop `ui::workflow_view::Selection`, iOS
 * `WorkflowSelection.swift`) and locked by the contract fixture
 * `workflow-view.json` `selection`. The strip is the picker: `All` first
 * (position 0), then every node in DAG order ([order]). An EMPTY [ids] IS All.
 *
 * A click picks exactly that node (a click on the already picked chip keeps
 * it); a toggle adds or removes one node (the last one out = All); extend
 * picks the DAG-order range from the [anchor] to the node; a step moves ONE
 * position from the [cursor] (the last clicked or stepped node, else the last
 * picked one) through All + the nodes, clamped; prune drops nodes that left.
 */
data class WorkflowSelection(
    /** The picked nodes, in DAG order. Empty = All. */
    val ids: List<String> = emptyList(),
    /** Where an extend range starts: the last plain or toggling click. */
    val anchor: String? = null,
    /** The node a step moves from: the last one clicked or stepped to. */
    val cursor: String? = null,
) {
    val isAll: Boolean get() = ids.isEmpty()

    /** The ONE picked node, or null for All and multi-picks. */
    val single: String? get() = ids.singleOrNull()

    /** A plain click on a chip; null = the All chip, an unknown node = All. */
    fun click(id: String?, order: List<String>): WorkflowSelection {
        if (id == null || id !in order) return ALL
        return WorkflowSelection(listOf(id), id, id)
    }

    /** Add or remove one node, in DAG order; the last one out = All. */
    fun toggle(id: String, order: List<String>): WorkflowSelection {
        if (id !in order) return ALL
        val next = ids.toMutableSet()
        if (!next.remove(id)) next.add(id)
        val picked = order.filter { it in next }
        if (picked.isEmpty()) return ALL
        return WorkflowSelection(picked, id, id)
    }

    /** The DAG-order range from the anchor (else the node itself) to [id]. */
    fun extend(id: String, order: List<String>): WorkflowSelection {
        if (id !in order) return ALL
        val from = order.indexOf(anchor?.takeIf { it in order } ?: id)
        val to = order.indexOf(id)
        return WorkflowSelection(order.subList(minOf(from, to), maxOf(from, to) + 1).toList(), order[from], id)
    }

    /** The node a step moves from, or null on All. */
    private fun stepOrigin(): String? = when {
        ids.isEmpty() -> null
        cursor != null && cursor in ids -> cursor
        else -> ids.last()
    }

    /** The strip position a step moves from: All = 0, the first node = 1. */
    fun position(order: List<String>): Int {
        val origin = stepOrigin() ?: return 0
        val index = order.indexOf(origin)
        return if (index < 0) 0 else index + 1
    }

    /** One position through All + the nodes, clamped; lands on ONE node or All. */
    fun step(delta: Int, order: List<String>): WorkflowSelection {
        val next = (position(order) + delta).coerceIn(0, order.size)
        if (next == 0) return ALL
        val id = order[next - 1]
        return WorkflowSelection(listOf(id), id, id)
    }

    /** Drops nodes that left the workflow; none left = All. */
    fun prune(order: List<String>): WorkflowSelection {
        val kept = order.filter { it in ids }
        if (kept.size == ids.size) return this
        if (kept.isEmpty()) return ALL
        return WorkflowSelection(
            ids = kept,
            anchor = anchor?.takeIf { it in kept } ?: kept.first(),
            cursor = cursor?.takeIf { it in kept } ?: kept.first(),
        )
    }

    companion object {
        val ALL = WorkflowSelection()

        /** The strip's chip ids in DAG order — the order `nodeStrip` draws them. */
        fun order(strip: List<StripWave>): List<String> = strip.flatMap { wave -> wave.nodes.map { it.id } }
    }
}
