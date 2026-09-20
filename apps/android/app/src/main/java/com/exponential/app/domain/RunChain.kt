package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

// EXP-988 contract / EXP-974: the resume chain of a coding session.
//
// A resume is the SAME run under a new row (`resumed_from_id` → predecessor,
// EXP-637/EXP-906), so every member of the succession wears ONE toggle and
// the run switcher's menu is where the reader picks between them — the
// "Continues an earlier run" band that used to sit atop the feed is gone
// (EXP-974). [runChain] is a pure SELECTOR over the synced `coding_sessions`
// rows: it follows `resumedFromId` BACKWARDS to the first row and FORWARDS to
// the latest, and returns the chain oldest-first, the named session included.
// An unknown id yields []. A fork (two rows resuming the same predecessor)
// follows the NEWEST successor by `createdAt` (ties by id, so the pick is
// stable); seen from an older sibling the chain is its own past plus itself.
// A predecessor the sweep deleted (a dangling `resumedFromId`) simply ends
// the backward walk. ×4 lockstep: web `runChain`, desktop
// `queries::run_chain`, iOS `RunChain.chain` — same rules, same test names.
//
// Who reads it: `AgentSessionViewModel.chainRuns` for an ISSUE-LESS run (a
// chat, action or batch run has no issue to list runs under); an issue-bound
// run's `issueRunRows` already include its resumes.

/** `createdAt` as epoch ms; an unparseable stamp sorts as 0 (web `stamp`). */
private fun chainStamp(session: CodingSessionEntity): Long =
    WireTimestamps.parseEpochMs(session.createdAt) ?: 0L

/** The newest of several rows resuming the same predecessor: `createdAt`
 *  descending, then id descending, so two clients agree on the fork. */
private fun newestOf(rows: List<CodingSessionEntity>): CodingSessionEntity? {
    var best: CodingSessionEntity? = null
    for (row in rows) {
        val current = best
        if (
            current == null ||
            chainStamp(row) > chainStamp(current) ||
            (chainStamp(row) == chainStamp(current) && row.id > current.id)
        ) {
            best = row
        }
    }
    return best
}

fun runChain(rows: List<CodingSessionEntity>, sessionId: String): List<CodingSessionEntity> {
    val byId = rows.associateBy { it.id }
    val start = byId[sessionId] ?: return emptyList()
    val seen = mutableSetOf(start.id)

    // Backwards to the first row. A missing predecessor (swept) ends the walk;
    // a cycle (never written by the server, but a synced row is a synced row)
    // ends it too.
    val before = mutableListOf<CodingSessionEntity>()
    var cursor: CodingSessionEntity? = start
    while (true) {
        val previousId = cursor?.resumedFromId ?: break
        val previous = byId[previousId] ?: break
        if (!seen.add(previous.id)) break
        before.add(previous)
        cursor = previous
    }
    before.reverse()

    // Forwards to the latest, the newest successor at every fork.
    val after = mutableListOf<CodingSessionEntity>()
    cursor = start
    while (true) {
        val current = cursor ?: break
        val next = newestOf(rows.filter { it.resumedFromId == current.id && it.id !in seen }) ?: break
        seen.add(next.id)
        after.add(next)
        cursor = next
    }

    return before + start + after
}
