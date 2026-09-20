import Foundation

/// EXP-988 contract / EXP-974: the RESUME CHAIN of a coding session.
///
/// A resume is the SAME run under a new row (`resumed_from_id` → predecessor,
/// EXP-637/EXP-906), so every member of the succession wears ONE toggle and
/// the run menu behind the `Runs` segment is where the reader picks between
/// them — the "Continues an earlier run" band that used to open the run view
/// is gone (EXP-974). `chain` is a pure SELECTOR over the synced
/// `coding_sessions` rows: it follows `resumedFromId` BACKWARDS to the first
/// row and FORWARDS to the latest, and returns the chain oldest-first, the
/// named session included. An unknown id yields []. A fork (two rows
/// resuming the same predecessor) follows the NEWEST successor by `createdAt`
/// (ties by id, so the pick is stable); seen from an older sibling the chain
/// is its own past plus itself. A predecessor the sweep deleted (a dangling
/// `resumedFromId`) simply ends the backward walk; a cyclic link never loops.
///
/// ×4 lockstep: web `lib/sessions/run-chain.ts`, desktop `queries::run_chain`,
/// Android `runChain` — same rules, same six test names (`RunChainTests`).
///
/// Who reads it: the Work screen's run menu / phone switcher rows for an
/// issue-LESS run (a chat, action or batch run has no issue to list runs
/// under); an issue-bound run keeps `PastRuns.issueRuns`, whose rows already
/// include its resumes.
public enum RunChain {
    /// `createdAt` as a comparable instant — an unparseable wire stamp reads
    /// as 0, exactly like the web's `stamp()`, so the id tie-break decides.
    private static func stamp(_ session: CodingSessionEntity) -> TimeInterval {
        WireTimestamps.parse(session.createdAt)?.timeIntervalSince1970 ?? 0
    }

    /// The newest of several rows resuming the same predecessor: `createdAt`
    /// descending, then id descending, so two clients agree on the fork.
    private static func newest(_ rows: [CodingSessionEntity]) -> CodingSessionEntity? {
        var best: CodingSessionEntity?
        for row in rows {
            guard let current = best else {
                best = row
                continue
            }
            let (a, b) = (stamp(row), stamp(current))
            if a > b || (a == b && row.id > current.id) { best = row }
        }
        return best
    }

    /// The resume chain `sessionId` belongs to, oldest first, itself included.
    public static func chain(
        _ sessions: [CodingSessionEntity],
        sessionId: String
    ) -> [CodingSessionEntity] {
        var byId: [String: CodingSessionEntity] = [:]
        for row in sessions { byId[row.id] = row }
        guard let start = byId[sessionId] else { return [] }
        var seen: Set<String> = [start.id]

        // Backwards to the first row. A missing predecessor (swept) ends the
        // walk; a cycle (never written by the server, but a synced row is a
        // synced row) ends it too.
        var before: [CodingSessionEntity] = []
        var cursor: CodingSessionEntity? = start
        while let current = cursor, let previousId = current.resumedFromId, !previousId.isEmpty {
            guard let previous = byId[previousId], !seen.contains(previous.id) else { break }
            seen.insert(previous.id)
            before.append(previous)
            cursor = previous
        }
        before.reverse()

        // Forwards to the latest, the newest successor at every fork.
        var after: [CodingSessionEntity] = []
        cursor = start
        while let current = cursor {
            let next = newest(
                sessions.filter { $0.resumedFromId == current.id && !seen.contains($0.id) }
            )
            guard let next else { break }
            seen.insert(next.id)
            after.append(next)
            cursor = next
        }

        return before + [start] + after
    }
}
