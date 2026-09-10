import Foundation

/// EXP-818: the session TREE — a run started by another run through
/// `exponential_sessions_start` carries `parent_session_id`, and every session
/// list (the Devices screen's Running/Past sections here; the rail and the
/// Agent page on web/desktop) nests it under its parent instead of listing it
/// as a stranger.
///
/// ONE pure rule, mirrored ×4 (web `lib/session-tree.ts`, desktop
/// `domain::session_tree`, Android `SessionTree.kt`) with the same four tests:
///
/// 1. The caller's order is the ROOT order — the tree never re-sorts roots.
/// 2. A row is a child iff its parent names ANOTHER row of the input; an
///    unlisted parent leaves the child a root at depth 0.
/// 3. Children follow their parent directly, oldest start first (then id),
///    recursively — depth grows by one per level.
/// 4. A cycle (defensive) breaks at the first repeat.
public enum SessionTree {
    public struct Row<T> {
        public let session: T
        /// 0 for a root, +1 per nesting level.
        public let depth: Int
        /// Whether at least one child is nested right below.
        public let hasChildren: Bool
    }

    public static func nest<T>(
        _ sessions: [T],
        id: (T) -> String,
        parent: (T) -> String?,
        startedAt: (T) -> String?
    ) -> [Row<T>] {
        let ids = Set(sessions.map(id))
        func isChild(_ session: T) -> Bool {
            guard let p = parent(session) else { return false }
            return p != id(session) && ids.contains(p)
        }
        var childrenOf: [String: [Int]] = [:]
        for (index, session) in sessions.enumerated() where isChild(session) {
            childrenOf[parent(session) ?? "", default: []].append(index)
        }
        for key in childrenOf.keys {
            childrenOf[key]?.sort { a, b in
                let (sa, sb) = (sessions[a], sessions[b])
                let (ta, tb) = (startedAt(sa) ?? "", startedAt(sb) ?? "")
                return ta != tb ? ta < tb : id(sa) < id(sb)
            }
        }
        var placed = Array(repeating: false, count: sessions.count)
        var order: [(Int, Int, Bool)] = []
        func visit(_ index: Int, depth: Int) {
            if placed[index] { return }
            placed[index] = true
            let children = (childrenOf[id(sessions[index])] ?? []).filter { !placed[$0] }
            order.append((index, depth, !children.isEmpty))
            for child in children { visit(child, depth: depth + 1) }
        }
        for (index, session) in sessions.enumerated() where !isChild(session) {
            visit(index, depth: 0)
        }
        // A child whose ancestry cycled without a root — keep it, at depth 0.
        for index in sessions.indices { visit(index, depth: 0) }
        return order.map { Row(session: sessions[$0.0], depth: $0.1, hasChildren: $0.2) }
    }

    /// The synced-row convenience: nests `CodingSessionEntity` rows.
    public static func nest(_ sessions: [CodingSessionEntity]) -> [Row<CodingSessionEntity>] {
        nest(sessions, id: { $0.id }, parent: { $0.parentSessionId }, startedAt: { $0.startedAt })
    }

    /// The ids of every row nested (at any depth) under `id`.
    public static func descendantIds<T>(_ rows: [Row<T>], of id: String, rowId: (T) -> String) -> [String] {
        guard let start = rows.firstIndex(where: { rowId($0.session) == id }) else { return [] }
        let depth = rows[start].depth
        var out: [String] = []
        for row in rows[(start + 1)...] {
            if row.depth <= depth { break }
            out.append(rowId(row.session))
        }
        return out
    }
}
