package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowNodeEntity

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
    /** What a STACK group row is called — a workflow group wears its own name.
     *  Byte-identical ×4 (web `STACK_GROUP_LABEL`, iOS `stackGroupLabel`). */
    const val STACK_GROUP_LABEL = "Stacked pull requests"

    /** The group fold's words ×4: a group has runs, not "child runs". */
    const val COLLAPSE_GROUP_LABEL = "Collapse these runs"
    const val EXPAND_GROUP_LABEL = "Expand these runs"

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
     * EXP-897: the rows a FOLDED list shows — everything nested under a
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
}

// ── the node tree ──────────────────────────────────────────────────────
// EXP-996/EXP-1050: the NODE-based session tree — the second, richer selector
// over the same synced rows, and the one every sessions list draws now. The
// flat [SessionTree.nest] above stays: `PrGraph` and the Recent sheet nest a
// plain parent/child list and have no groups to draw.
//
// THE CONTRACT is web `lib/sessions/session-tree.ts`; these are its rules, in
// this order (same names, same tests ×4 — desktop `domain::session_tree`, iOS
// `SessionTree.swift`):
//
//   1. Resumed runs COLLAPSE: every resume succession ([runChain], EXP-974) is
//      ONE node, keyed by its newest row, `chain` oldest-first. Rows are
//      walked oldest first, so the PRIMARY succession claims its members and
//      an older fork sibling becomes its own node.
//   2. Children nest under their `parentSessionId` (EXP-679/818), following
//      the parent's whole succession (EXP-906: a resume inherits it).
//   3. The sessions of ONE workflow group under a [SessionTreeNode.Workflow]
//      row — resolved through `workflow_nodes` by session, then issue, then a
//      batch row's covered issues, and only for a workflow the caller synced
//      (that row is where the NAME comes from). `startedReason = workflow`
//      alone never groups.
//   4. A stack ([PrStack.stackChain], `issues.pr_base_branch`) groups under
//      [SessionTreeNode.Stack] in LINEAR order, lowest first — only when TWO+
//      of its members are listed. Workflow grouping wins.
//   5. Groups and top-level nodes sort by last activity, newest first;
//      children keep creation order, and a parent's activity counts its whole
//      subtree, so folding one never moves it.
//   6. An orphan child whose parent is gone (swept, not synced) sits at top
//      level; so does a cycle's closing row.
//
// CONCRETE, not generic (the choice this file makes): the two walks it reuses
// are entity-typed here — [runChain] over `CodingSessionEntity` and
// [PrStack.stackChain] over `IssueEntity` — and a generic tree would have to
// carry its own copy of both. The web version is generic only because its
// helpers are; nobody on Android feeds this anything but the synced rows.

/** What the rows alone cannot say: which workflow an issue belongs to, and
 *  which issues stack on which. Every list is optional — a caller with no
 *  workflows synced still gets the session/parent tree. */
data class SessionTreeContext(
    val workflows: List<WorkflowEntity> = emptyList(),
    /** `workflow_nodes` rows: which issue sits in which workflow. */
    val workflowNodes: List<WorkflowNodeEntity> = emptyList(),
    /** The issues the sessions name, for the stack edges. */
    val issues: List<IssueEntity> = emptyList(),
)

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

    data class Workflow(
        val workflowId: String,
        val name: String,
        /** The node runs, newest first. */
        override val children: List<SessionTreeNode> = emptyList(),
        override val lastActivityAt: Long = 0L,
    ) : SessionTreeNode

    data class Stack(
        /** The lowest issue of the stack. */
        val rootIssueId: String,
        /** LINEAR: lowest first, one session node per stacked issue. */
        override val children: List<SessionTreeNode> = emptyList(),
        override val lastActivityAt: Long = 0L,
    ) : SessionTreeNode
}

/** A node's stable identity — the key a collapsed set and a list item use. */
fun sessionTreeNodeKey(node: SessionTreeNode): String = when (node) {
    is SessionTreeNode.Session -> node.session.id
    is SessionTreeNode.Workflow -> "workflow:${node.workflowId}"
    is SessionTreeNode.Stack -> "stack:${node.rootIssueId}"
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

/**
 * The sessions list as a tree. Pure: no clock, no IO; sort ties break on the
 * node key so two clients agree.
 */
fun sessionTree(
    sessions: List<CodingSessionEntity>,
    context: SessionTreeContext = SessionTreeContext(),
): List<SessionTreeNode> {
    // 1. Resume successions collapse. Oldest row first, so the primary
    //    succession ([runChain]'s newest-successor walk) claims its members
    //    before an older fork sibling does; whatever is left becomes its own
    //    node. Without a fork this is exactly [runChain].
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

    // 2. Children nest under their parent's SUCCESSION (EXP-906: a resume
    //    inherits `parentSessionId`, so the whole chain answers for it).
    val parentOf = HashMap<String, String>()
    for ((canonicalId, chain) in chainOf) {
        var named: String? = null
        var index = chain.size - 1
        while (index >= 0 && named == null) {
            named = chain[index].parentSessionId
            index -= 1
        }
        val parent = named?.let { canonicalOf[it] }
        // Rule 6: a parent that is gone (swept, another team, not synced)
        // leaves the child at top level; so does a row naming itself.
        if (parent != null && parent != canonicalId) parentOf[canonicalId] = parent
    }

    val builds = LinkedHashMap<String, TreeBuild>()
    for ((canonicalId, chain) in chainOf) {
        builds[canonicalId] = TreeBuild(
            session = chain.last(),
            chain = chain,
            lastActivityAt = chain.maxOfOrNull { treeStamp(it.updatedAt) } ?: 0L,
        )
    }

    // A cycle (never written by the server, but a synced row is a synced row)
    // leaves the row it closes on at top level.
    val roots = ArrayList<TreeBuild>()
    for ((canonicalId, build) in builds) {
        val parent = treeAncestor(canonicalId, parentOf, builds.keys)?.let { builds[it] }
        if (parent != null && parent !== build) parent.children.add(build) else roots.add(build)
    }

    // 3. Children keep CREATION order (rule 5); a parent's activity counts its
    //    subtree's, so folding one never moves it.
    for (build in builds.values) {
        build.children.sortWith(
            compareBy<TreeBuild>({ treeStamp(it.session.createdAt) }, { it.session.id }),
        )
    }
    for (root in roots) rollUpActivity(root)

    // 4. Workflow groups, then stack groups — over the TOP-LEVEL nodes only
    //    (a child run stays under its parent wherever the parent lands).
    val frozen = roots.map { it.freeze() }
    val grouped = groupSessionStacks(groupSessionWorkflows(frozen, context), context)

    // 5. Groups and lone nodes sort by last activity, newest first.
    return grouped.sortedWith(
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

/** Rule 3: the sessions of ONE workflow under one group row. A node belongs to
 *  the workflow that lists its issue (or the node run itself) — the name comes
 *  from [SessionTreeContext.workflows], so a workflow the caller did not sync
 *  leaves its runs ungrouped. */
private fun groupSessionWorkflows(
    roots: List<SessionTreeNode.Session>,
    context: SessionTreeContext,
): List<SessionTreeNode> {
    val workflows = context.workflows.associateBy { it.id }
    if (workflows.isEmpty() || context.workflowNodes.isEmpty()) return roots
    val byIssue = HashMap<String, String>()
    val bySession = HashMap<String, String>()
    for (entry in context.workflowNodes) {
        if (entry.workflowId !in workflows) continue
        if (entry.issueId.isNotEmpty() && entry.issueId !in byIssue) byIssue[entry.issueId] = entry.workflowId
        val sessionId = entry.sessionId
        if (sessionId != null && sessionId !in bySession) bySession[sessionId] = entry.workflowId
    }
    fun workflowOf(node: SessionTreeNode.Session): String? {
        for (row in node.chain) {
            bySession[row.id]?.let { return it }
            row.issueId?.let { issueId -> byIssue[issueId]?.let { return it } }
            // A batch node run covers several workflow issues (EXP-978).
            for (issueId in batchRunIssueIds(row.batchIssueIds)) {
                byIssue[issueId]?.let { return it }
            }
        }
        return null
    }

    val out = ArrayList<SessionTreeNode>(roots.size)
    // The group rows, in first-encounter order, each with its members.
    val members = LinkedHashMap<String, MutableList<SessionTreeNode>>()
    val slotOf = HashMap<String, Int>()
    for (node in roots) {
        val workflowId = workflowOf(node)
        val workflow = workflowId?.let { workflows[it] }
        if (workflowId == null || workflow == null) {
            out.add(node)
            continue
        }
        if (workflowId !in members) {
            members[workflowId] = mutableListOf()
            slotOf[workflowId] = out.size
            out.add(SessionTreeNode.Workflow(workflowId = workflowId, name = workflow.name))
        }
        members.getValue(workflowId).add(node)
    }
    for ((workflowId, children) in members) {
        // The node runs, newest first.
        val sorted = children.sortedWith(
            compareByDescending<SessionTreeNode> { it.lastActivityAt }
                .thenBy { sessionTreeNodeKey(it) },
        )
        val slot = slotOf.getValue(workflowId)
        out[slot] = (out[slot] as SessionTreeNode.Workflow).copy(
            children = sorted,
            lastActivityAt = sorted.maxOfOrNull { it.lastActivityAt } ?: 0L,
        )
    }
    return out
}

/** Rule 4: a stack (`issues.pr_base_branch`) under one group row in LINEAR
 *  order, lowest first. A stack with only ONE of its runs listed is no group —
 *  the lone node stays where it was. */
private fun groupSessionStacks(
    entries: List<SessionTreeNode>,
    context: SessionTreeContext,
): List<SessionTreeNode> {
    if (context.issues.isEmpty()) return entries
    val byIssueId = context.issues.associateBy { it.id }
    /** Every top-level session node that names an issue, by issue id. */
    val nodeOfIssue = LinkedHashMap<String, SessionTreeNode.Session>()
    for (entry in entries) {
        val issueId = (entry as? SessionTreeNode.Session)?.session?.issueId ?: continue
        if (issueId in byIssueId && issueId !in nodeOfIssue) nodeOfIssue[issueId] = entry
    }

    val out = ArrayList<SessionTreeNode>(entries.size)
    val claimed = HashSet<String>()
    for (entry in entries) {
        if (entry !is SessionTreeNode.Session) {
            out.add(entry)
            continue
        }
        // A node already pulled into a group below is gone from the top level;
        // one that names no issue (a chat, an action, a batch) can be in no
        // stack and simply stays where it was.
        if (entry.session.id in claimed) continue
        val issue = entry.session.issueId?.let { byIssueId[it] }
        if (issue == null) {
            out.add(entry)
            continue
        }
        val chain = PrStack.stackChain(issue, context.issues)
        val members = chain.mapNotNull { member ->
            nodeOfIssue[member.id]?.takeIf { it.session.id !in claimed }
        }
        if (chain.size < 2 || members.size < 2) {
            out.add(entry)
            continue
        }
        for (member in members) claimed.add(member.session.id)
        out.add(
            SessionTreeNode.Stack(
                rootIssueId = chain.first().id,
                children = members,
                lastActivityAt = members.maxOfOrNull { it.lastActivityAt } ?: 0L,
            ),
        )
    }
    return out
}

/**
 * The tree flattened top to bottom, skipping everything under a COLLAPSED node
 * (keyed by [sessionTreeNodeKey]). A group row with no children left is
 * dropped: a group is its children.
 */
fun visibleSessionTreeRows(
    nodes: List<SessionTreeNode>,
    collapsed: Set<String> = emptySet(),
): List<SessionTreeFlatRow> {
    val out = ArrayList<SessionTreeFlatRow>()
    fun walk(list: List<SessionTreeNode>, depth: Int) {
        for (node in list) {
            val key = sessionTreeNodeKey(node)
            if (node !is SessionTreeNode.Session && node.children.isEmpty()) continue
            out.add(SessionTreeFlatRow(node, key, depth, node.children.isNotEmpty()))
            if (key !in collapsed) walk(node.children, depth + 1)
        }
    }
    walk(nodes, 0)
    return out
}

/** Every session node of the tree, depth-first, groups flattened. */
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
