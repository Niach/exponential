import Foundation

/// EXP-980 — sub-issues nest under their parent in every issue list. The ROOT
/// issue decides the group and the sort position; its sub-issues follow it,
/// whatever status they are in themselves.
///
/// ONE pure rule over ids, mirrored ×4 (web `lib/issue-nesting.ts`, Android
/// `domain/IssueNesting.kt`, desktop `domain::issue_nesting`) and locked by the
/// contract fixture `domain-contract/fixtures/issue-nesting.json` — same cases,
/// same test names. The elbow connectors are the shared `TreeGuides`.
///
/// 1. A canonical `parent` row is `issueId` = the PARENT, `relatedIssueId` =
///    the CHILD (EXP-736). Only rows whose BOTH ends are in the list count: a
///    parent on another board, unsynced or filtered out leaves the child a
///    root.
/// 2. A child with several listed parents nests under the one with the lowest
///    identifier (plain string order).
/// 3. An issue on a parent CYCLE keeps no parent (walking up from it returns to
///    it), so a cycle can never swallow rows; whatever hangs off it still
///    nests.
/// 4. Siblings keep the LIST's order: group order first, then the position
///    inside their own group — the caller's comparator already ran.
public enum IssueNesting {

    /// One rendered row: the issue and how deep it hangs.
    public struct Row: Equatable, Sendable {
        public let id: String
        /// 0 = a root row, 1 = its sub-issue, …
        public let depth: Int

        public init(id: String, depth: Int) {
            self.id = id
            self.depth = depth
        }
    }

    /// `groups` = the list's groups in order, each the ids in display order.
    /// Returns the same number of groups; one a nesting emptied comes back
    /// empty (the caller hides it).
    public static func nestIssueRows(
        groups: [[String]],
        relations: [IssueRelationEntity],
        identifierOf: (String) -> String
    ) -> [[Row]] {
        var position: [String: Int] = [:]
        for ids in groups {
            for id in ids where position[id] == nil {
                position[id] = position.count
            }
        }

        var parentOf: [String: String] = [:]
        for relation in relations
        where relation.type == IssueRelationType.parent.rawValue {
            let parent = relation.issueId
            let child = relation.relatedIssueId
            if parent == child { continue }
            guard position[parent] != nil, position[child] != nil else { continue }
            if let current = parentOf[child] {
                if identifierOf(parent) < identifierOf(current) { parentOf[child] = parent }
            } else {
                parentOf[child] = parent
            }
        }

        // Rule 3: drop the parent of every issue that is its own ancestor.
        // Judged against the untouched map first, so EVERY member of a cycle
        // becomes a root, whatever order the rows arrived in.
        var onCycle: [String] = []
        for start in parentOf.keys {
            var seen: Set<String> = [start]
            var cursor = parentOf[start]
            while let current = cursor, !seen.contains(current) {
                seen.insert(current)
                cursor = parentOf[current]
            }
            if cursor == start { onCycle.append(start) }
        }
        for id in onCycle { parentOf.removeValue(forKey: id) }

        var childrenOf: [String: [String]] = [:]
        for (child, parent) in parentOf {
            childrenOf[parent, default: []].append(child)
        }
        for parent in childrenOf.keys {
            childrenOf[parent]?.sort { (position[$0] ?? 0) < (position[$1] ?? 0) }
        }

        var emitted = Set<String>()
        func emit(_ id: String, _ depth: Int, _ out: inout [Row]) {
            guard !emitted.contains(id) else { return }
            emitted.insert(id)
            out.append(Row(id: id, depth: depth))
            for child in childrenOf[id] ?? [] { emit(child, depth + 1, &out) }
        }

        return groups.map { ids in
            var out: [Row] = []
            for id in ids where parentOf[id] == nil { emit(id, 0, &out) }
            return out
        }
    }
}
