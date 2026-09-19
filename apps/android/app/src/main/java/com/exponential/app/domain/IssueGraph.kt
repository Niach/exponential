package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity

/**
 * EXP-980: the `blocks` graph behind the list badge, the mini-graph overlay
 * and the blocked-start dialog.
 *
 * ONE pure rule, mirrored ×4 (web `lib/issue-graph.ts`, iOS `IssueGraph.swift`,
 * desktop `domain::issue_graph`) and locked by the contract fixture
 * `domain-contract/fixtures/issue-graph.json` — same cases, same test names.
 *
 * A canonical `blocks` row is `issue_id` BLOCKS `related_issue_id` (EXP-736).
 * An edge counts only while BOTH ends are synced and OPEN (anchor status not
 * done / cancelled / duplicate): a finished blocker is in nobody's way, and a
 * finished issue is blocked by nothing. A SUBJECT is always kept, open or not.
 */
object IssueGraph {

    /** The most nodes one graph draws; the rest is cut and `truncated` says so. */
    const val MAX_NODES = 60

    /** Under a graph the node cap cut. Byte-identical ×4. */
    const val TRUNCATED_NOTE = "Showing the nearest $MAX_NODES issues."

    /** Under a graph that holds a cycle. Byte-identical ×4. */
    const val CYCLE_NOTE = "Red issues block each other in a cycle."

    /** The badge numbers of one row. */
    data class Counts(
        /** Open issues that block this one. */
        val blockedBy: Int,
        /** Open issues this one blocks. */
        val blocking: Int,
    )

    data class Node(
        val id: String,
        /**
         * The column: 0 = blocked by nothing in the graph; every edge points
         * to a higher wave (cycle edges aside).
         */
        val wave: Int,
        /** The row inside the wave, by identifier. */
        val lane: Int,
        val subject: Boolean,
    )

    data class Edge(
        /** The blocker. */
        val from: String,
        /** The blocked issue. */
        val to: String,
        /** Part of a blocking cycle: drawn red, and nothing on it can start. */
        val cycle: Boolean,
    )

    data class Graph(
        val nodes: List<Node>,
        val edges: List<Edge>,
        val hasCycle: Boolean,
        val truncated: Boolean,
    ) {
        val isEmpty: Boolean get() = nodes.isEmpty()
    }

    /**
     * The badge's accessible label: `Blocked by 2`, `Blocking 1` or
     * `Blocked by 2, blocking 1`. Byte-identical ×4.
     */
    fun blocksBadgeLabel(counts: Counts): String {
        val parts = ArrayList<String>()
        if (counts.blockedBy > 0) parts.add("Blocked by ${counts.blockedBy}")
        if (counts.blocking > 0) {
            parts.add("${if (parts.isNotEmpty()) "blocking" else "Blocking"} ${counts.blocking}")
        }
        return parts.joinToString(", ")
    }

    /**
     * The badge numbers of every issue that has any; an issue with neither
     * count is absent.
     */
    fun blockCounts(
        relations: List<IssueRelationEntity>,
        issues: List<IssueEntity>,
    ): Map<String, Counts> {
        val counts = LinkedHashMap<String, Counts>()
        for ((from, to) in openEdges(relations, issues, emptySet())) {
            val blocker = counts[from] ?: Counts(0, 0)
            counts[from] = blocker.copy(blocking = blocker.blocking + 1)
            val blocked = counts[to] ?: Counts(0, 0)
            counts[to] = blocked.copy(blockedBy = blocked.blockedBy + 1)
        }
        return counts
    }

    /**
     * The open issues that block any of [ids] from OUTSIDE the set, by
     * identifier: what a batch start has to ask about (a blocker picked into
     * the same batch is not in its way).
     */
    fun openBlockersOfSet(
        ids: List<String>,
        relations: List<IssueRelationEntity>,
        issues: List<IssueEntity>,
    ): List<IssueEntity> {
        val picked = ids.toSet()
        val issuesById = issues.associateBy { it.id }
        val out = LinkedHashMap<String, IssueEntity>()
        for ((from, to) in openEdges(relations, issues, emptySet())) {
            if (to !in picked || from in picked) continue
            issuesById[from]?.let { out[from] = it }
        }
        return out.values.sortedBy { it.identifier }
    }

    /**
     * The graph around [subjectIds]: the subjects, everything that
     * transitively blocks them and everything they transitively block, with
     * every open edge among those nodes.
     *
     * Layout: an edge is a CYCLE edge when its blocker is reachable from its
     * blocked end; `wave` is the longest path over the remaining (acyclic)
     * edges, `lane` the identifier order inside a wave. Nodes come back by
     * (wave, lane), edges by (from, to) identifier.
     */
    fun blockGraph(
        subjectIds: List<String>,
        relations: List<IssueRelationEntity>,
        issues: List<IssueEntity>,
    ): Graph {
        val issuesById = issues.associateBy { it.id }
        val identifierOf = { id: String -> issuesById[id]?.identifier ?: id }
        val byIdentifier = compareBy<String>({ identifierOf(it) }, { it })

        val subjects = subjectIds.distinct()
            .filter { issuesById.containsKey(it) }
            .sortedWith(byIdentifier)
        val subjectSet = subjects.toSet()
        val all = openEdges(relations, issues, subjectSet)
        val blockersOf = LinkedHashMap<String, MutableList<String>>()
        val blockedBy = LinkedHashMap<String, MutableList<String>>()
        for ((from, to) in all) {
            blockersOf.getOrPut(to) { ArrayList() }.add(from)
            blockedBy.getOrPut(from) { ArrayList() }.add(to)
        }

        // The closure, blockers first and then the blocked side, level by
        // level and in identifier order, so the node cap cuts the same nodes
        // everywhere.
        val picked = LinkedHashSet<String>()
        var truncated = false
        fun admit(id: String): Boolean {
            if (id in picked) return false
            if (picked.size >= MAX_NODES) {
                truncated = true
                return false
            }
            picked.add(id)
            return true
        }
        for (id in subjects) admit(id)
        for (next in listOf(blockersOf, blockedBy)) {
            var frontier = subjects
            while (frontier.isNotEmpty()) {
                val found = LinkedHashSet<String>()
                for (id in frontier) {
                    for (other in next[id].orEmpty()) {
                        if (other !in picked) found.add(other)
                    }
                }
                frontier = found.sortedWith(byIdentifier).filter { admit(it) }
            }
        }

        val inside = all.filter { (from, to) -> from in picked && to in picked }
        val out = LinkedHashMap<String, MutableList<String>>()
        for ((from, to) in inside) out.getOrPut(from) { ArrayList() }.add(to)
        fun reaches(start: String, goal: String): Boolean {
            val seen = HashSet<String>()
            seen.add(start)
            val stack = ArrayDeque<String>()
            stack.addLast(start)
            while (stack.isNotEmpty()) {
                val id = stack.removeLast()
                if (id == goal) return true
                for (other in out[id].orEmpty()) {
                    if (seen.add(other)) stack.addLast(other)
                }
            }
            return false
        }

        val edges = inside
            .map { (from, to) -> Edge(from = from, to = to, cycle = reaches(to, from)) }
            .sortedWith(
                compareBy<Edge, String>(byIdentifier) { it.from }
                    .thenBy(byIdentifier) { it.to },
            )

        // Longest path over the acyclic edges (Kahn).
        val wave = HashMap<String, Int>()
        val pending = HashMap<String, Int>()
        for (id in picked) {
            wave[id] = 0
            pending[id] = 0
        }
        val forward = LinkedHashMap<String, MutableList<String>>()
        for (edge in edges) {
            if (edge.cycle) continue
            pending[edge.to] = (pending[edge.to] ?: 0) + 1
            forward.getOrPut(edge.from) { ArrayList() }.add(edge.to)
        }
        val ready = ArrayDeque(picked.filter { pending[it] == 0 })
        while (ready.isNotEmpty()) {
            val id = ready.removeLast()
            for (other in forward[id].orEmpty()) {
                wave[other] = maxOf(wave[other] ?: 0, (wave[id] ?: 0) + 1)
                val left = (pending[other] ?: 0) - 1
                pending[other] = left
                if (left == 0) ready.addLast(other)
            }
        }

        val ordered = picked.sortedWith(compareBy<String> { wave[it] ?: 0 }.then(byIdentifier))
        val laneAt = HashMap<Int, Int>()
        val nodes = ordered.map { id ->
            val w = wave[id] ?: 0
            val lane = laneAt[w] ?: 0
            laneAt[w] = lane + 1
            Node(id = id, wave = w, lane = lane, subject = id in subjectSet)
        }

        return Graph(
            nodes = nodes,
            edges = edges,
            hasCycle = edges.any { it.cycle },
            truncated = truncated,
        )
    }

    /** The anchor statuses that mean an issue is out of everybody's way. */
    private fun isClosed(issue: IssueEntity): Boolean =
        when (IssueStatus.fromWire(issue.status)) {
            IssueStatus.Done, IssueStatus.Cancelled, IssueStatus.Duplicate -> true
            else -> false
        }

    /**
     * The `blocks` edges both of whose ends are synced and usable, deduped and
     * in relation order. [keep] is the subject set — a subject stays in the
     * graph even once it is finished.
     */
    private fun openEdges(
        relations: List<IssueRelationEntity>,
        issues: List<IssueEntity>,
        keep: Set<String>,
    ): List<Pair<String, String>> {
        val issuesById = issues.associateBy { it.id }
        fun usable(id: String): Boolean {
            val issue = issuesById[id] ?: return false
            return id in keep || !isClosed(issue)
        }
        val seen = HashSet<String>()
        val edges = ArrayList<Pair<String, String>>()
        for (relation in relations) {
            if (relation.type != DomainContract.issueRelationTypeBlocks) continue
            if (relation.issueId == relation.relatedIssueId) continue
            if (!usable(relation.issueId) || !usable(relation.relatedIssueId)) continue
            if (!seen.add("${relation.issueId}\n${relation.relatedIssueId}")) continue
            edges.add(relation.issueId to relation.relatedIssueId)
        }
        return edges
    }
}
