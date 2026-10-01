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

    /**
     * The rows a FOLDED list shows — everything nested under a
     * collapsed parent (at any depth) is dropped, the parent itself stays.
     * The ×4 rule (web `visibleTreeRows`, iOS/desktop the same name), so a
     * fold chevron means the same thing on every client.
     */
    fun <T> visibleRows(
        rows: List<Row<T>>,
        collapsed: Set<String>,
        rowId: (T) -> String,
    ): List<Row<T>> {
        if (collapsed.isEmpty()) return rows
        val out = ArrayList<Row<T>>(rows.size)
        var hiddenBelow: Int? = null
        for (row in rows) {
            val cut = hiddenBelow
            if (cut != null) {
                if (row.depth > cut) continue
                hiddenBelow = null
            }
            out.add(row)
            if (rowId(row.session) in collapsed) hiddenBelow = row.depth
        }
        return out
    }

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

    /** A row is LIVE until the server ends it. */
    fun sessionRowIsLive(status: String?): Boolean = status != DomainContract.codingSessionStatusEnded

    // ── EXP-1108: the needs-you mark, ONE rule ×4 (web `sessionNeedsYou`),
    //    locked by `session-tree-marks.json`.

    /**
     * The RED needs-you dot = a LIVE row with an open question. The amber
     * needs-input/blocked flags are a separate mark, never this one.
     */
    fun sessionNeedsYou(status: String?, hasPendingQuestion: Boolean): Boolean =
        sessionRowIsLive(status) && hasPendingQuestion
}

// ── the node tree ──────────────────────────────────────────────────────
// EXP-996/EXP-1050: the NODE-based session tree every sessions list draws.
// The flat [SessionTree.nest] above stays for the Recent sheet.
//
// THE CONTRACT is web `lib/sessions/session-tree.ts` (same rules ×4 —
// desktop `domain::session_tree`, iOS `SessionTree.swift`):
//
//   1. Resume successions ([runChain]) COLLAPSE into ONE node keyed by the
//      newest row, `chain` oldest-first; the primary succession claims first.
//   2. Children nest under their `parentSessionId`'s succession.
//   3. Top-level nodes sort by last activity, newest first; children keep
//      creation order; activity rolls up the subtree.
//   4. An orphan child (parent gone) or a cycle's closing row sits at top.

sealed interface SessionTreeNode {
    val children: List<SessionTreeNode>

    /** The newest `updated_at` across this node's whole subtree, epoch ms. */
    val lastActivityAt: Long

    data class Session(
        /** The newest row of the resume succession — the node's identity. */
        val session: CodingSessionEntity,
        /** The succession oldest-first ([runChain]); `[session]` unresumed. */
        val chain: List<CodingSessionEntity>,
        override val children: List<SessionTreeNode> = emptyList(),
        override val lastActivityAt: Long = 0L,
    ) : SessionTreeNode
}

/** A node's stable identity — the key a collapsed set and a list item use. */
fun sessionTreeNodeKey(node: SessionTreeNode): String = when (node) {
    is SessionTreeNode.Session -> node.session.id
}

/** One row of a DRAWN session tree: a node, how deep it sits and whether it
 *  can fold — what the EXP-965 connector is computed over. */
data class SessionTreeFlatRow(
    val node: SessionTreeNode,
    val key: String,
    val depth: Int,
    val hasChildren: Boolean,
)

/** `updated_at`/`created_at` as epoch ms; an unparseable stamp sorts as 0. */
private fun treeStamp(value: String?): Long {
    val text = value?.trim()?.takeIf { it.isNotEmpty() } ?: return 0L
    return WireTimestamps.parseEpochMs(text) ?: 0L
}

/** A session node under construction: its children and its rolled-up activity
 *  are only known once every row has been placed. */
private class TreeBuild(
    val session: CodingSessionEntity,
    val chain: List<CodingSessionEntity>,
    var lastActivityAt: Long,
) {
    val children = mutableListOf<TreeBuild>()

    fun freeze(): SessionTreeNode.Session = SessionTreeNode.Session(
        session = session,
        chain = chain,
        children = children.map { it.freeze() },
        lastActivityAt = lastActivityAt,
    )
}

/** Rule 3: CREATION order, ties on the id. */
private val byCreation = compareBy<TreeBuild>({ treeStamp(it.session.createdAt) }, { it.session.id })

/**
 * The sessions list as a tree. Pure: no clock, no IO; sort ties break on the
 * node key so two clients agree.
 */
fun sessionTree(sessions: List<CodingSessionEntity>): List<SessionTreeNode> {
    // 1. Resume successions collapse, oldest row first.
    val ordered = sessions.sortedWith(
        compareBy<CodingSessionEntity>({ treeStamp(it.createdAt) }, { it.id }),
    )
    val canonicalOf = HashMap<String, String>()
    val chainOf = LinkedHashMap<String, List<CodingSessionEntity>>()
    for (row in ordered) {
        if (row.id in canonicalOf) continue
        val chain = runChain(sessions, row.id).filter { it.id !in canonicalOf }
        val canonical = chain.lastOrNull() ?: row
        for (member in chain) canonicalOf[member.id] = canonical.id
        chainOf[canonical.id] = chain.ifEmpty { listOf(row) }
    }

    // 2. Children nest under their parent's SUCCESSION (EXP-906).
    val parentOf = HashMap<String, String>()
    for ((canonicalId, chain) in chainOf) {
        val named = chain.asReversed().firstNotNullOfOrNull { it.parentSessionId }
        val parent = named?.let { canonicalOf[it] }
        // Rule 4: a gone parent (or a row naming itself) leaves it at top.
        if (parent == null || parent == canonicalId) continue
        parentOf[canonicalId] = parent
    }

    val builds = LinkedHashMap<String, TreeBuild>()
    for ((canonicalId, chain) in chainOf) {
        val session = chain.last()
        builds[canonicalId] = TreeBuild(
            session = session,
            chain = chain,
            lastActivityAt = chain.maxOfOrNull { treeStamp(it.updatedAt) } ?: 0L,
        )
    }

    // A cycle leaves the row it closes on at top level.
    val roots = ArrayList<TreeBuild>()
    for ((canonicalId, build) in builds) {
        val parent = treeAncestor(canonicalId, parentOf, builds.keys)?.let { builds[it] }
        if (parent != null && parent !== build) parent.children.add(build) else roots.add(build)
    }

    // 3. Children keep CREATION order; a parent's activity counts its subtree;
    //    top-level nodes sort by last activity, newest first.
    for (build in builds.values) build.children.sortWith(byCreation)
    for (root in roots) rollUpActivity(root)
    return roots.map { it.freeze() }.sortedWith(
        compareByDescending<SessionTreeNode> { it.lastActivityAt }
            .thenBy { sessionTreeNodeKey(it) },
    )
}

/** The DIRECT parent of [id], or null when it is already a root. Breaks a
 *  cycle by returning null, so the row stays where it is. */
private fun treeAncestor(
    id: String,
    parentOf: Map<String, String>,
    known: Set<String>,
): String? {
    val parent = parentOf[id] ?: return null
    if (parent !in known) return null
    val seen = hashSetOf(id)
    var cursor: String? = parent
    while (cursor != null) {
        if (!seen.add(cursor)) return null
        cursor = parentOf[cursor]
    }
    return parent
}

/** A node's activity counts its whole subtree's. */
private fun rollUpActivity(build: TreeBuild): Long {
    for (child in build.children) {
        build.lastActivityAt = maxOf(build.lastActivityAt, rollUpActivity(child))
    }
    return build.lastActivityAt
}

/**
 * The tree flattened top to bottom, skipping everything under a COLLAPSED node
 * (keyed by [sessionTreeNodeKey]).
 */
fun visibleSessionTreeRows(
    nodes: List<SessionTreeNode>,
    collapsed: Set<String> = emptySet(),
): List<SessionTreeFlatRow> {
    val out = ArrayList<SessionTreeFlatRow>()
    fun walk(list: List<SessionTreeNode>, depth: Int) {
        for (node in list) {
            val key = sessionTreeNodeKey(node)
            out.add(SessionTreeFlatRow(node, key, depth, node.children.isNotEmpty()))
            if (key !in collapsed) walk(node.children, depth + 1)
        }
    }
    walk(nodes, 0)
    return out
}

/** Every session node of the tree, depth-first. */
fun flattenSessionTree(nodes: List<SessionTreeNode>): List<SessionTreeNode.Session> {
    val out = ArrayList<SessionTreeNode.Session>()
    fun walk(list: List<SessionTreeNode>) {
        for (node in list) {
            if (node is SessionTreeNode.Session) out.add(node)
            walk(node.children)
        }
    }
    walk(nodes)
    return out
}
