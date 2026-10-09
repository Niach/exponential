import Foundation

/// EXP-818: the session TREE — a run started by another run through
/// `exponential_sessions_start` carries `parent_session_id`, and every session
/// list (the Devices screen's Running/Recent sections here; the rail and the
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
                // Parsed, never the raw string (web `startStamp`): a `.5Z`
                // fractional stamp and a Postgres `+00` text form sort by
                // instant, an unparseable one reads as 0 and the id decides.
                let (ta, tb) = (stamp(startedAt(sa)), stamp(startedAt(sb)))
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

// MARK: - The node tree (EXP-996)

/// EXP-996 — the session list as a TREE of NODES, not a flat roll of
/// strangers. `nest` above still answers "which row hangs off which"; this is
/// the selector that also says a resume succession is ONE thing.
///
/// The CONTRACT is web `lib/sessions/session-tree.ts` (its module comment =
/// the rules); same test names (`SessionTreeTests`, desktop `session_tree`,
/// Android `SessionTree.kt`):
///
///   1. Resumed runs COLLAPSE: every resume succession (`RunChain`, EXP-974)
///      is ONE node, keyed by its newest row, `chain` oldest-first.
///   2. Children nest under their `parentSessionId`, following the parent's
///      resume succession (EXP-906).
///   3. Top-level nodes sort by last activity, newest first; children keep
///      creation order, and a parent's activity counts its whole subtree (so
///      folding one never moves it).
///   4. An orphan child whose parent is gone (swept, not synced) sits at top
///      level.
///
/// CONCRETE over `CodingSessionEntity`, not generic like `nest` above: rule 1
/// is `RunChain.chain`, which is entity-typed. A view whose row wraps a
/// session (`AgentsViewModel.Row`) passes `rows.map(\.session)` and looks its
/// own row back up by `session.id` — which is what web's list does too.
extension SessionTree {

    /// One run — its whole resume succession — and whatever it started.
    public struct SessionNode: Sendable {
        /// The NEWEST row of the succession: the node's identity.
        public let session: CodingSessionEntity
        /// The succession oldest-first (`RunChain`); `[session]` when unresumed.
        public let chain: [CodingSessionEntity]
        public let children: [SessionNode]
        /// The newest `updatedAt` across the chain AND the children, seconds.
        public let lastActivityAt: TimeInterval

        public init(
            session: CodingSessionEntity,
            chain: [CodingSessionEntity],
            children: [SessionNode],
            lastActivityAt: TimeInterval
        ) {
            self.session = session
            self.chain = chain
            self.children = children
            self.lastActivityAt = lastActivityAt
        }

        /// This node's stable identity (`SessionTree.nodeKey`).
        public var key: String { SessionTree.nodeKey(self) }
    }

    /// A node's stable identity — the key a collapsed set and a list use: the
    /// newest row's id. Web `sessionTreeNodeKey`.
    public static func nodeKey(_ node: SessionNode) -> String {
        node.session.id
    }

    /// One row of a DRAWN session tree: a node, how deep it sits and whether
    /// it can fold — what the EXP-965 connector is computed over.
    public struct FlatRow: Sendable {
        public let node: SessionNode
        public let key: String
        public let depth: Int
        public let hasChildren: Bool

        public init(node: SessionNode, key: String, depth: Int, hasChildren: Bool) {
            self.node = node
            self.key = key
            self.depth = depth
            self.hasChildren = hasChildren
        }
    }

    /// A row is LIVE until the server ends it (`running` and `in_review` both
    /// are; `needs_input`/`blocked` are flags on a live row).
    public static func sessionRowIsLive(status: String) -> Bool {
        status != "ended"
    }

    // MARK: - The selector

    /// The sessions list as a tree. Pure: no clock, no IO; sort ties break on
    /// the node key, so two clients agree.
    public static func sessionTree(_ sessions: [CodingSessionEntity]) -> [SessionNode] {
        // Rule 1. Oldest row first, so the primary succession (`RunChain`'s
        // newest-successor walk) claims its members before an older fork
        // sibling does; whatever is left becomes its own node.
        let ordered = sessions.sorted { a, b in
            let (sa, sb) = (stamp(a.createdAt), stamp(b.createdAt))
            return sa != sb ? sa < sb : a.id < b.id
        }
        var canonicalOf: [String: String] = [:]
        var chainOf: [String: [CodingSessionEntity]] = [:]
        for row in ordered {
            if canonicalOf[row.id] != nil { continue }
            let chain = RunChain.chain(sessions, sessionId: row.id)
                .filter { canonicalOf[$0.id] == nil }
            let canonical = chain.last ?? row
            for member in chain { canonicalOf[member.id] = canonical.id }
            chainOf[canonical.id] = chain.isEmpty ? [row] : chain
        }

        // Rule 2: a child follows its parent's whole SUCCESSION, named by the
        // newest row of the chain that names one at all.
        var parentOf: [String: String] = [:]
        for (canonicalId, chain) in chainOf {
            var named: String?
            for member in chain.reversed() {
                if let parent = member.parentSessionId, !parent.isEmpty {
                    named = parent
                    break
                }
            }
            // Rule 4: a parent that is gone (swept, another team, not synced)
            // leaves the child at top level; so does a row naming itself.
            guard let parent = named.flatMap({ canonicalOf[$0] }), parent != canonicalId
            else { continue }
            parentOf[canonicalId] = parent
        }

        // A cycle (never written by the server, but a synced row is a synced
        // row) leaves the row it closes on at top level.
        let nodeIds = Set(chainOf.keys)
        var childrenOf: [String: [String]] = [:]
        var rootIds: [String] = []
        for canonicalId in chainOf.keys {
            if let parent = ancestor(canonicalId, parentOf: parentOf, nodeIds: nodeIds) {
                childrenOf[parent, default: []].append(canonicalId)
            } else {
                rootIds.append(canonicalId)
            }
        }

        var activity: [String: TimeInterval] = [:]
        for (canonicalId, chain) in chainOf {
            activity[canonicalId] = chain.map { stamp($0.updatedAt) }.max() ?? 0
        }

        /// One node and its subtree. Rule 3: children keep CREATION order, and
        /// the node's activity counts the subtree's.
        func build(_ id: String) -> SessionNode? {
            guard let chain = chainOf[id], let session = chain.last else { return nil }
            let children = (childrenOf[id] ?? [])
                .compactMap { build($0) }
                .sorted(by: byCreation)
            let own = activity[id] ?? 0
            return SessionNode(
                session: session,
                chain: chain,
                children: children,
                lastActivityAt: max(own, children.map(\.lastActivityAt).max() ?? 0)
            )
        }

        // Rule 3: top-level nodes sort by last activity, newest first.
        return rootIds.compactMap { build($0) }.sorted(by: byActivity)
    }

    /// The tree flattened top to bottom, every node at its depth (lists draw
    /// the whole tree: nothing folds since EXP-1248).
    public static func visibleRows(_ nodes: [SessionNode]) -> [FlatRow] {
        var out: [FlatRow] = []
        func walk(_ list: [SessionNode], _ depth: Int) {
            for node in list {
                let key = nodeKey(node)
                out.append(
                    FlatRow(
                        node: node, key: key, depth: depth, hasChildren: !node.children.isEmpty
                    )
                )
                walk(node.children, depth + 1)
            }
        }
        walk(nodes, 0)
        return out
    }

    /// Every session node of the tree, depth-first.
    public static func flatten(_ nodes: [SessionNode]) -> [SessionNode] {
        var out: [SessionNode] = []
        func walk(_ list: [SessionNode]) {
            for node in list {
                out.append(node)
                walk(node.children)
            }
        }
        walk(nodes)
        return out
    }

    // MARK: - Helpers

    /// Rule 3: children keep CREATION order, ties on the key.
    private static func byCreation(_ a: SessionNode, _ b: SessionNode) -> Bool {
        let (ca, cb) = (stamp(a.session.createdAt), stamp(b.session.createdAt))
        return ca != cb ? ca < cb : nodeKey(a) < nodeKey(b)
    }

    /// Newest activity first, ties on the key.
    private static func byActivity(_ a: SessionNode, _ b: SessionNode) -> Bool {
        a.lastActivityAt != b.lastActivityAt
            ? a.lastActivityAt > b.lastActivityAt
            : nodeKey(a) < nodeKey(b)
    }

    /// `createdAt`/`updatedAt` as a comparable instant — an unparseable wire
    /// stamp reads as 0, exactly like web's `stamp()`, so the id tie-break
    /// decides.
    private static func stamp(_ value: String?) -> TimeInterval {
        guard let value, !value.isEmpty else { return 0 }
        return WireTimestamps.parse(value)?.timeIntervalSince1970 ?? 0
    }

    /// `id`'s direct parent, or nil when it is already a root. A cycle in the
    /// walk above it returns nil, so the row stays where it is.
    private static func ancestor(
        _ id: String, parentOf: [String: String], nodeIds: Set<String>
    ) -> String? {
        guard let parent = parentOf[id], nodeIds.contains(parent), parent != id else { return nil }
        var seen: Set<String> = [id]
        var cursor: String? = parent
        while let current = cursor {
            if seen.contains(current) { return nil }
            seen.insert(current)
            cursor = parentOf[current]
        }
        return parent
    }
}
