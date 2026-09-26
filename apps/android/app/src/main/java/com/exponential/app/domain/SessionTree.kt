package com.exponential.app.domain

import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DeviceEntity
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

    // ── EXP-1068: the strings every client draws off the tree, byte-identical
    //    ×4 (web `sessionRowIsLive`, `reviewBranchRound`, `reviewRoundVerdict`,
    //    `reviewRowCaption`, `workflowGroupCaption`).

    /** A row is LIVE until the server ends it. */
    fun sessionRowIsLive(status: String?): Boolean = status != DomainContract.codingSessionStatusEnded

    private val REVIEW_ROUND = Regex("-r(\\d+)$")

    /** The round a review branch carries (`…-review-<IDENT>-r<n>` → n); null
     *  for any other branch. Only the SUFFIX is read. */
    fun reviewBranchRound(branch: String?): Int? {
        if (branch.isNullOrEmpty()) return null
        val round = REVIEW_ROUND.find(branch)?.groupValues?.get(1)?.toIntOrNull() ?: return null
        return round.takeIf { it > 0 }
    }

    /** What a review row says about its verdict. `Submitted` = an older round
     *  whose verdict the node no longer carries (only the latest is stored). */
    enum class ReviewRowVerdict { Approved, ChangesRequested, Submitted, None }

    /** The verdict of the review of [round], from the node's `review_round`
     *  ([nodeReviewRound]; null = no node) and its latest `review` cell. */
    fun reviewRoundVerdict(
        round: Int?,
        nodeReviewRound: Int?,
        latestRound: Int?,
        latestVerdict: String?,
    ): ReviewRowVerdict {
        if (round == null || nodeReviewRound == null) return ReviewRowVerdict.None
        if (latestRound != null && latestVerdict != null && latestRound == round) {
            return if (latestVerdict == "approve") ReviewRowVerdict.Approved else ReviewRowVerdict.ChangesRequested
        }
        return if (round <= nodeReviewRound) ReviewRowVerdict.Submitted else ReviewRowVerdict.None
    }

    /** A review row's title: `Review r2 · approved`, `… · changes requested`,
     *  `… · submitted`, `… · no verdict` (ended), `Review r2` (still live),
     *  `Review` without a round. */
    fun reviewRowCaption(round: Int?, verdict: ReviewRowVerdict, live: Boolean): String {
        val title = if (round == null) "Review" else "Review r$round"
        return when (verdict) {
            ReviewRowVerdict.Approved -> "$title · approved"
            ReviewRowVerdict.ChangesRequested -> "$title · changes requested"
            ReviewRowVerdict.Submitted -> "$title · submitted"
            ReviewRowVerdict.None -> if (live) title else "$title · no verdict"
        }
    }

    /** The workflow group row's trailing caption: `3 running · 5 of 8 done`. */
    fun workflowGroupCaption(liveRuns: Int, nodesDone: Int, nodesTotal: Int): String {
        val parts = ArrayList<String>(2)
        if (liveRuns > 0) parts.add("$liveRuns running")
        if (nodesTotal > 0) parts.add("$nodesDone of $nodesTotal done")
        return parts.joinToString(" · ")
    }

    // ── EXP-1108: a session row's marks, ONE rule ×4 (web
    //    `deviceDefaultAccount` / `workflowRunAccountCaption` /
    //    `sessionNeedsYou`), locked by `session-tree-marks.json`.

    /**
     * The device's DEFAULT account for [agent] (EXP-872): `defaultAccount`
     * when [agent] is the default agent, else that agent's ACTIVE profile,
     * else `system`. Null when the device is unknown or the run has no agent.
     */
    fun deviceDefaultAccount(device: SessionMarkDevice?, agent: String?): String? {
        if (device == null || agent == null) return null
        if (device.defaultAgent == agent && !device.defaultAccount.isNullOrEmpty()) {
            return device.defaultAccount
        }
        return device.profiles[agent].orEmpty().firstOrNull { it.active }?.id ?: SYSTEM_PROFILE_ID
    }

    /**
     * The label a WORKFLOW run's account caption names, when the run does not
     * spend its device's default account for its agent: the profile's label,
     * else `Default` for `system`, else the raw id. Null outside a workflow,
     * with no account, on an unsynced device or on the default.
     */
    fun workflowRunAccountLabel(session: SessionMarkRow, devices: List<SessionMarkDevice>): String? {
        if (session.workflowId.isNullOrEmpty()) return null
        val account = session.agentAccount?.takeIf { it.isNotEmpty() } ?: return null
        val matches = devices.filter { it.deviceId == session.deviceId }
        val device = matches.firstOrNull { it.userId == session.userId } ?: matches.firstOrNull()
        val fallback = deviceDefaultAccount(device, session.agent)
        if (fallback == null || fallback == account) return null
        val profile = session.agent?.let { agent -> device?.profiles?.get(agent)?.firstOrNull { it.id == account } }
        return profile?.label?.takeIf { it.isNotEmpty() }
            ?: if (account == SYSTEM_PROFILE_ID) "Default" else account
    }

    /** `account <label>` ([workflowRunAccountLabel]), or null. */
    fun workflowRunAccountCaption(session: SessionMarkRow, devices: List<SessionMarkDevice>): String? =
        workflowRunAccountLabel(session, devices)?.let { "account $it" }

    /**
     * The RED needs-you dot = a LIVE row with an open question. The amber
     * needs-input/blocked flags are a separate mark, never this one.
     */
    fun sessionNeedsYou(status: String?, hasPendingQuestion: Boolean): Boolean =
        sessionRowIsLive(status) && hasPendingQuestion
}

/** EXP-1108: the session fields the account caption reads. */
data class SessionMarkRow(
    val agent: String?,
    val agentAccount: String?,
    val deviceId: String?,
    val userId: String?,
    val workflowId: String?,
) {
    companion object {
        fun of(session: CodingSessionEntity) = SessionMarkRow(
            agent = session.agent,
            agentAccount = session.agentAccount,
            deviceId = session.deviceId,
            userId = session.userId,
            workflowId = session.workflowId,
        )
    }
}

/** EXP-1108: one login profile as the account caption reads it. */
data class SessionMarkProfile(val id: String, val label: String?, val active: Boolean)

/** EXP-1108: the synced device fields the account caption reads. */
data class SessionMarkDevice(
    val deviceId: String,
    val userId: String?,
    val defaultAgent: String?,
    val defaultAccount: String?,
    /** agent → its login profiles; an agent with none is absent. */
    val profiles: Map<String, List<SessionMarkProfile>>,
) {
    companion object {
        fun of(device: DeviceEntity): SessionMarkDevice {
            val defaults = parseLaunchDefaults(device.launchDefaults)
            return SessionMarkDevice(
                deviceId = device.deviceId,
                userId = device.userId,
                defaultAgent = defaults?.defaultAgent,
                defaultAccount = defaults?.defaultAccount,
                profiles = parseAgentAccounts(device.agentAccounts).orEmpty().mapValues { (_, account) ->
                    account.profiles.orEmpty().map { SessionMarkProfile(it.id, it.label, it.active) }
                },
            )
        }
    }
}

// ── the node tree ──────────────────────────────────────────────────────
// EXP-996/EXP-1050/EXP-1068: the NODE-based session tree every sessions list
// draws. The flat [SessionTree.nest] above stays for `PrGraph` and the Recent
// sheet, which have no groups to draw.
//
// THE CONTRACT is web `lib/sessions/session-tree.ts` (same rules, same test
// names ×4 — desktop `domain::session_tree`, iOS `SessionTree.swift`):
//
//   1. Resume successions ([runChain]) COLLAPSE into ONE node keyed by the
//      newest row, `chain` oldest-first; the primary succession claims first.
//   2. Children nest under their `parentSessionId`'s succession — UNLESS the
//      child has a workflow membership the parent does not share (a chat that
//      resumed a node run stays in the group). No membership = always nests.
//   3. A chain's membership = its NEWEST row with a `workflowId` (EXP-1082,
//      server-stamped). Runs group under [SessionTreeNode.Workflow] ONLY by
//      that stamp and only for a workflow listed in the context (the NAME
//      source). Inside: one row per `author` chain; `review` chains nest under
//      their node's head author (live first, then newest), else sit as plain
//      children; everything else is a plain child. Two live authors or two
//      live reviewers on a node flag every author row `duplicateLive`.
//   4. A stack ([PrStack.stackChain]) groups under [SessionTreeNode.Stack] in
//      LINEAR order, lowest first, from the leftover top level (2+ members).
//   5. Groups and top-level nodes sort by last activity, newest first;
//      children keep creation order; activity rolls up the subtree.
//   6. An orphan child (parent gone) or a cycle's closing row sits at top.
//
// CONCRETE, not generic: [runChain] and [PrStack.stackChain] are entity-typed
// here, and nobody on Android feeds this anything but the synced rows.

/** What the rows alone cannot say: the workflows' names/status, their nodes'
 *  states (the group caption) and the stack edges. Every list is optional. */
data class SessionTreeContext(
    val workflows: List<WorkflowEntity> = emptyList(),
    /** `workflow_nodes` rows: group NOTHING since EXP-1068; `state` feeds the
     *  group's `5 of 8 done`. */
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
        /** A review chain's round, off its branch; null on every other row. */
        val reviewRound: Int? = null,
        /** An author row whose node has 2+ live authors or reviewers. */
        val duplicateLive: Boolean = false,
    ) : SessionTreeNode

    data class Workflow(
        val workflowId: String,
        val name: String,
        /** The node runs, newest first. */
        override val children: List<SessionTreeNode> = emptyList(),
        override val lastActivityAt: Long = 0L,
        /** contract `wfStatus` — the group row's dot. */
        val status: String = DomainContract.wfStatusRunning,
        /** Live session nodes in the whole subtree. */
        val liveRuns: Int = 0,
        /** The workflow's nodes in state `landed`, of [nodesTotal]. */
        val nodesDone: Int = 0,
        val nodesTotal: Int = 0,
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

/** A chain's EXP-1082 membership (its newest stamped row's). */
private data class TreeMembership(val workflowId: String, val nodeId: String?, val role: String?)

private fun membershipOf(chain: List<CodingSessionEntity>): TreeMembership? {
    val row = chain.lastOrNull { it.workflowId != null } ?: return null
    return TreeMembership(row.workflowId!!, row.workflowNodeId, row.workflowRole)
}

/** A session node under construction: its children and its rolled-up activity
 *  are only known once every row has been placed. */
private class TreeBuild(
    val session: CodingSessionEntity,
    val chain: List<CodingSessionEntity>,
    var lastActivityAt: Long,
    val reviewRound: Int?,
) {
    val children = mutableListOf<TreeBuild>()
    var duplicateLive = false

    val live: Boolean get() = SessionTree.sessionRowIsLive(session.status)

    fun freeze(): SessionTreeNode.Session = SessionTreeNode.Session(
        session = session,
        chain = chain,
        children = children.map { it.freeze() },
        lastActivityAt = lastActivityAt,
        reviewRound = reviewRound,
        duplicateLive = duplicateLive,
    )
}

/** Rule 5: CREATION order, ties on the id. */
private val byCreation = compareBy<TreeBuild>({ treeStamp(it.session.createdAt) }, { it.session.id })

/** Newest activity first, ties on the id. */
private val byActivity = compareByDescending<TreeBuild> { it.lastActivityAt }.thenBy { it.session.id }

/**
 * The sessions list as a tree. Pure: no clock, no IO; sort ties break on the
 * node key so two clients agree.
 */
fun sessionTree(
    sessions: List<CodingSessionEntity>,
    context: SessionTreeContext = SessionTreeContext(),
): List<SessionTreeNode> {
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
    val memberships = chainOf.mapValues { (_, chain) -> membershipOf(chain) }

    // 2. Children nest under their parent's SUCCESSION (EXP-906) — unless the
    //    child is a workflow's and the parent is not that workflow's.
    val parentOf = HashMap<String, String>()
    for ((canonicalId, chain) in chainOf) {
        val named = chain.asReversed().firstNotNullOfOrNull { it.parentSessionId }
        val parent = named?.let { canonicalOf[it] }
        // Rule 6: a gone parent (or a row naming itself) leaves it at top.
        if (parent == null || parent == canonicalId) continue
        val own = memberships[canonicalId]
        if (own != null && own.workflowId != memberships[parent]?.workflowId) continue
        parentOf[canonicalId] = parent
    }

    val builds = LinkedHashMap<String, TreeBuild>()
    for ((canonicalId, chain) in chainOf) {
        val session = chain.last()
        builds[canonicalId] = TreeBuild(
            session = session,
            chain = chain,
            lastActivityAt = chain.maxOfOrNull { treeStamp(it.updatedAt) } ?: 0L,
            reviewRound = if (memberships[canonicalId]?.role == DomainContract.wfSessionRoleReview) {
                SessionTree.reviewBranchRound(session.branch)
            } else {
                null
            },
        )
    }

    // A cycle leaves the row it closes on at top level.
    val roots = ArrayList<TreeBuild>()
    for ((canonicalId, build) in builds) {
        val parent = treeAncestor(canonicalId, parentOf, builds.keys)?.let { builds[it] }
        if (parent != null && parent !== build) parent.children.add(build) else roots.add(build)
    }

    // 3. Children keep CREATION order; a parent's activity counts its subtree.
    for (build in builds.values) build.children.sortWith(byCreation)
    for (root in roots) rollUpActivity(root)

    // 4. Workflow groups, then stack groups — over the TOP-LEVEL nodes only.
    val grouped = groupSessionStacks(groupSessionWorkflows(roots, memberships, context), context)

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

/** How many session nodes of a subtree are live. */
private fun liveCount(builds: List<TreeBuild>): Int =
    builds.sumOf { (if (it.live) 1 else 0) + liveCount(it.children) }

/** Rule 3: the sessions of ONE workflow under one group row, by the rows' own
 *  stamped `workflowId`; a workflow the caller did not list stays ungrouped. */
private fun groupSessionWorkflows(
    roots: List<TreeBuild>,
    memberships: Map<String, TreeMembership?>,
    context: SessionTreeContext,
): List<SessionTreeNode> {
    val workflows = context.workflows.associateBy { it.id }
    if (workflows.isEmpty()) return roots.map { it.freeze() }

    class Part {
        val authors = LinkedHashMap<String, MutableList<TreeBuild>>()
        val reviews = LinkedHashMap<String, MutableList<TreeBuild>>()
        val plain = mutableListOf<TreeBuild>()
    }
    // Ungrouped roots stay as builds; a group is a placeholder until filled.
    val out = ArrayList<Any>(roots.size)
    val parts = LinkedHashMap<String, Part>()
    for (build in roots) {
        val membership = memberships[build.session.id]
        val workflow = membership?.let { workflows[it.workflowId] }
        if (membership == null || workflow == null) {
            out.add(build)
            continue
        }
        val part = parts.getOrPut(workflow.id) {
            out.add(workflow)
            Part()
        }
        val nodeId = membership.nodeId
        when {
            nodeId != null && membership.role == DomainContract.wfSessionRoleAuthor ->
                part.authors.getOrPut(nodeId) { mutableListOf() }.add(build)
            nodeId != null && membership.role == DomainContract.wfSessionRoleReview ->
                part.reviews.getOrPut(nodeId) { mutableListOf() }.add(build)
            else -> part.plain.add(build)
        }
    }

    return out.map { entry ->
        if (entry is TreeBuild) return@map entry.freeze()
        val workflow = entry as WorkflowEntity
        val part = parts.getValue(workflow.id)
        val children = mutableListOf<TreeBuild>()
        for ((nodeId, authors) in part.authors) {
            // The node's HEAD author: live first, then newest activity.
            authors.sortWith(compareByDescending<TreeBuild> { it.live }.then(byActivity))
            val reviews = part.reviews.remove(nodeId).orEmpty()
            val duplicate = authors.count { it.live } > 1 || reviews.count { it.live } > 1
            for (author in authors) author.duplicateLive = duplicate
            if (reviews.isNotEmpty()) {
                val head = authors.first()
                head.children.addAll(reviews)
                head.children.sortWith(byCreation)
                rollUpActivity(head)
            }
            children.addAll(authors)
        }
        // A review whose node has no author listed, then everything else.
        for (reviews in part.reviews.values) children.addAll(reviews)
        children.addAll(part.plain)
        children.sortWith(byActivity)
        val nodes = context.workflowNodes.filter { it.workflowId == workflow.id }
        SessionTreeNode.Workflow(
            workflowId = workflow.id,
            name = workflow.name,
            children = children.map { it.freeze() },
            lastActivityAt = children.maxOfOrNull { it.lastActivityAt } ?: 0L,
            status = workflow.status,
            liveRuns = liveCount(children),
            nodesDone = nodes.count { it.state == DomainContract.wfNodeStateLanded },
            nodesTotal = nodes.size,
        )
    }
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
