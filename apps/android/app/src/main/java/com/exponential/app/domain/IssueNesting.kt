package com.exponential.app.domain

import com.exponential.app.data.db.IssueRelationEntity

/**
 * EXP-980: sub-issues nest under their parent in every issue list. The ROOT
 * issue decides the group and the sort position; its sub-issues follow it,
 * whatever status they are in themselves.
 *
 * ONE pure rule over ids, mirrored ×4 (web `lib/issue-nesting.ts`, iOS
 * `IssueNesting.swift`, desktop `domain::issue_nesting`) and locked by the
 * contract fixture `domain-contract/fixtures/issue-nesting.json` — same cases,
 * same test names. The elbow connectors are the shared [TreeGuides].
 *
 * 1. A canonical `parent` row is `issue_id` = the PARENT, `related_issue_id` =
 *    the CHILD (EXP-736). Only rows whose BOTH ends are in the list count: a
 *    parent on another board, unsynced or filtered out leaves the child a root.
 * 2. A child with several listed parents nests under the one with the lowest
 *    identifier (plain string order).
 * 3. An issue on a parent CYCLE keeps no parent (walking up from it returns to
 *    it), so a cycle can never swallow rows; whatever hangs off it still nests.
 * 4. Siblings keep the LIST's order: group order first, then the position
 *    inside their own group — the caller's comparator already ran.
 */
object IssueNesting {

    /** One rendered row: the issue and how deep it hangs. */
    data class Row(
        val id: String,
        /** 0 = a root row, 1 = its sub-issue, … */
        val depth: Int,
    )

    /**
     * [groups] = the list's groups in order, each the ids in display order.
     * Returns the same number of groups; one a nesting emptied comes back
     * empty (the caller hides it).
     */
    fun nestIssueRows(
        groups: List<List<String>>,
        relations: List<IssueRelationEntity>,
        identifierOf: (String) -> String,
    ): List<List<Row>> {
        val position = LinkedHashMap<String, Int>()
        for (ids in groups) {
            for (id in ids) position.getOrPut(id) { position.size }
        }

        val parentOf = LinkedHashMap<String, String>()
        for (relation in relations) {
            if (relation.type != DomainContract.issueRelationTypeParent) continue
            val parent = relation.issueId
            val child = relation.relatedIssueId
            if (parent == child) continue
            if (!position.containsKey(parent) || !position.containsKey(child)) continue
            val current = parentOf[child]
            if (current == null || identifierOf(parent) < identifierOf(current)) {
                parentOf[child] = parent
            }
        }

        // Rule 3: drop the parent of every issue that is its own ancestor.
        // Judged against the untouched map first, so EVERY member of a cycle
        // becomes a root, whatever order the rows arrived in.
        val onCycle = ArrayList<String>()
        for (start in parentOf.keys) {
            val seen = HashSet<String>()
            seen.add(start)
            var cursor = parentOf[start]
            while (cursor != null && !seen.contains(cursor)) {
                seen.add(cursor)
                cursor = parentOf[cursor]
            }
            if (cursor == start) onCycle.add(start)
        }
        for (id in onCycle) parentOf.remove(id)

        val childrenOf = LinkedHashMap<String, MutableList<String>>()
        for ((child, parent) in parentOf) {
            childrenOf.getOrPut(parent) { ArrayList() }.add(child)
        }
        for (siblings in childrenOf.values) {
            siblings.sortBy { position[it] ?: 0 }
        }

        val emitted = HashSet<String>()
        fun emit(id: String, depth: Int, out: MutableList<Row>) {
            if (!emitted.add(id)) return
            out.add(Row(id, depth))
            for (child in childrenOf[id].orEmpty()) emit(child, depth + 1, out)
        }

        return groups.map { ids ->
            val out = ArrayList<Row>()
            for (id in ids) {
                if (!parentOf.containsKey(id)) emit(id, 0, out)
            }
            out
        }
    }
}
