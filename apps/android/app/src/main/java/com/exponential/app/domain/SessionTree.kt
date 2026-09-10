package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

/**
 * EXP-818: the session TREE — a run started by another run through
 * `exponential_sessions_start` carries `parent_session_id`, and every session
 * list (the Agents screen's Running/Past sections here; the rail and the
 * Agent page on web/desktop) nests it under its parent instead of listing it
 * as a stranger.
 *
 * ONE pure rule, mirrored ×4 (web `lib/session-tree.ts`, desktop
 * `domain::session_tree`, iOS `SessionTree.swift`) with the same four tests:
 *
 * 1. The caller's order is the ROOT order — the tree never re-sorts roots.
 * 2. A row is a child iff its parent names ANOTHER row of the input; an
 *    unlisted parent leaves the child a root at depth 0.
 * 3. Children follow their parent directly, oldest start first (then id),
 *    recursively — depth grows by one per level.
 * 4. A cycle (defensive) breaks at the first repeat.
 */
object SessionTree {
    data class Row<T>(
        val session: T,
        /** 0 for a root, +1 per nesting level. */
        val depth: Int,
        /** Whether at least one child is nested right below. */
        val hasChildren: Boolean,
    )

    fun <T> nest(
        sessions: List<T>,
        id: (T) -> String,
        parent: (T) -> String?,
        startedAt: (T) -> String?,
    ): List<Row<T>> {
        val ids = sessions.map(id).toSet()
        fun isChild(session: T): Boolean {
            val p = parent(session) ?: return false
            return p != id(session) && p in ids
        }
        val childrenOf = HashMap<String, MutableList<Int>>()
        sessions.forEachIndexed { index, session ->
            if (isChild(session)) childrenOf.getOrPut(parent(session) ?: "") { mutableListOf() }.add(index)
        }
        for (list in childrenOf.values) {
            list.sortWith(compareBy<Int>({ startedAt(sessions[it]) ?: "" }, { id(sessions[it]) }))
        }
        val placed = BooleanArray(sessions.size)
        val order = ArrayList<Triple<Int, Int, Boolean>>(sessions.size)
        fun visit(index: Int, depth: Int) {
            if (placed[index]) return
            placed[index] = true
            val children = childrenOf[id(sessions[index])].orEmpty().filter { !placed[it] }
            order.add(Triple(index, depth, children.isNotEmpty()))
            for (child in children) visit(child, depth + 1)
        }
        sessions.forEachIndexed { index, session -> if (!isChild(session)) visit(index, 0) }
        // A child whose ancestry cycled without a root — keep it, at depth 0.
        for (index in sessions.indices) visit(index, 0)
        return order.map { (index, depth, hasChildren) -> Row(sessions[index], depth, hasChildren) }
    }

    /** The synced-row convenience: nests [CodingSessionEntity] rows. */
    fun nest(sessions: List<CodingSessionEntity>): List<Row<CodingSessionEntity>> =
        nest(sessions, { it.id }, { it.parentSessionId }, { it.startedAt })

    /** The ids of every row nested (at any depth) under [id]. */
    fun <T> descendantIds(rows: List<Row<T>>, id: String, rowId: (T) -> String): List<String> {
        val start = rows.indexOfFirst { rowId(it.session) == id }
        if (start < 0) return emptyList()
        val depth = rows[start].depth
        val out = ArrayList<String>()
        for (row in rows.subList(start + 1, rows.size)) {
            if (row.depth <= depth) break
            out.add(rowId(row.session))
        }
        return out
    }
}
