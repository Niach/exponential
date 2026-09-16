import Foundation

/// EXP-897 — the PR STACK, derived from synced data alone and mirrored ×4
/// (web `lib/pr-stack.ts`, desktop `queries::nest_review_entries`, Android
/// `PrStack.kt`).
///
/// One edge, one rule: a pull request is stacked ON another when its
/// `prBaseBranch` equals the lower one's `branch` (both non-empty). No stack
/// table, no server round-trip — `issues.pr_base_branch` is synced and every
/// client derives the same chain from it.
///
/// Three guards the shared tests pin:
/// - a base NOBODY in the list owns ends the walk (the PR is simply based on
///   `main`, or on a branch outside this team's synced rows);
/// - a cycle breaks where it FIRST repeats (defensive: GitHub cannot make
///   one, a half-synced snapshot can);
/// - roots keep the CALLER's order, children follow their parent.
public enum PrStack {

    // MARK: - Position

    /// Where a member sits in its stack, and its immediate neighbours.
    public struct StackPosition<T> {
        /// 1-based, counted from the BOTTOM of the chain.
        public let position: Int
        /// The whole chain's length.
        public let size: Int
        /// The entry directly below (the foundation), nil at the bottom.
        public let below: T?
        /// The entry directly above, nil at the top.
        public let above: T?
    }

    /// The full chain `entry` belongs to, bottom first, including itself.
    /// A lone pull request returns just itself.
    public static func stackChain<T>(
        _ entry: T,
        in entries: [T],
        id: (T) -> String,
        branch: (T) -> String?,
        base: (T) -> String?
    ) -> [T] {
        let byBranch = branchIndex(entries, branch: branch)
        let childOf = childIndex(entries, id: id, branch: branch, base: base)

        // Down to the foundation.
        var down: [T] = []
        var seen: Set<String> = [id(entry)]
        var cursor = entry
        while let baseRef = nonEmpty(base(cursor)), let lower = byBranch[baseRef] {
            let lowerId = id(lower)
            if seen.contains(lowerId) { break }
            seen.insert(lowerId)
            down.append(lower)
            cursor = lower
        }

        // Up to the top.
        var up: [T] = []
        cursor = entry
        while let branchRef = nonEmpty(branch(cursor)), let upper = childOf[branchRef] {
            let upperId = id(upper)
            if seen.contains(upperId) { break }
            seen.insert(upperId)
            up.append(upper)
            cursor = upper
        }

        return down.reversed() + [entry] + up
    }

    /// `entry`'s place in its chain, or nil when it is not stacked at all
    /// (a chain of one is a plain pull request, not a stack).
    public static func stackPosition<T>(
        _ entry: T,
        in entries: [T],
        id: (T) -> String,
        branch: (T) -> String?,
        base: (T) -> String?
    ) -> StackPosition<T>? {
        let chain = stackChain(entry, in: entries, id: id, branch: branch, base: base)
        guard chain.count > 1 else { return nil }
        let entryId = id(entry)
        guard let index = chain.firstIndex(where: { id($0) == entryId }) else { return nil }
        return StackPosition(
            position: index + 1,
            size: chain.count,
            below: index > 0 ? chain[index - 1] : nil,
            above: index + 1 < chain.count ? chain[index + 1] : nil
        )
    }

    /// The synced-row convenience: an issue's position among the team's
    /// issues (one issue = one PR = one branch).
    public static func stackPosition(
        _ issue: IssueEntity, issues: [IssueEntity]
    ) -> StackPosition<IssueEntity>? {
        stackPosition(
            issue, in: issues, id: { $0.id }, branch: { $0.branch }, base: { $0.prBaseBranch }
        )
    }

    /// The synced-row convenience for the whole chain, bottom first.
    public static func stackChain(_ issue: IssueEntity, issues: [IssueEntity]) -> [IssueEntity] {
        stackChain(
            issue, in: issues, id: { $0.id }, branch: { $0.branch }, base: { $0.prBaseBranch }
        )
    }

    // MARK: - Nesting

    /// One row of a nested list: the entry, its depth, and whether anything
    /// is nested right below it.
    public struct Nested<T> {
        public let entry: T
        /// 0 for a root (a PR based on something nobody here owns), +1 per level.
        public let depth: Int
        public let hasChildren: Bool

        public init(entry: T, depth: Int, hasChildren: Bool) {
            self.entry = entry
            self.depth = depth
            self.hasChildren = hasChildren
        }
    }

    /// Nest `entries` into their stacks: a root keeps the caller's order, and
    /// every stacked entry follows the entry it is based on, one level deeper.
    public static func nestPrStacks<T>(
        _ entries: [T],
        id: (T) -> String,
        branch: (T) -> String?,
        base: (T) -> String?
    ) -> [Nested<T>] {
        let byBranch = branchIndex(entries, branch: branch)
        let ids = Dictionary(
            entries.enumerated().map { (id($0.element), $0.offset) }, uniquingKeysWith: { a, _ in a }
        )

        func lowerIndex(_ index: Int) -> Int? {
            let entry = entries[index]
            guard let baseRef = nonEmpty(base(entry)), let lower = byBranch[baseRef] else {
                return nil
            }
            guard let lowerIdx = ids[id(lower)], lowerIdx != index else { return nil }
            return lowerIdx
        }

        // Children in caller order, keyed by the index of the entry below.
        var childrenOf: [Int: [Int]] = [:]
        var isChild = Array(repeating: false, count: entries.count)
        for index in entries.indices {
            guard let lower = lowerIndex(index) else { continue }
            childrenOf[lower, default: []].append(index)
            isChild[index] = true
        }

        var placed = Array(repeating: false, count: entries.count)
        var out: [Nested<T>] = []
        func visit(_ index: Int, depth: Int) {
            if placed[index] { return }
            placed[index] = true
            let children = (childrenOf[index] ?? []).filter { !placed[$0] }
            out.append(Nested(entry: entries[index], depth: depth, hasChildren: !children.isEmpty))
            for child in children { visit(child, depth: depth + 1) }
        }
        for index in entries.indices where !isChild[index] { visit(index, depth: 0) }
        // A cycle left members unplaced — keep them, at depth 0 (the
        // `SessionTree` precedent: a list never silently loses a row).
        for index in entries.indices { visit(index, depth: 0) }
        return out
    }

    /// The synced-row convenience for a flat list of issues.
    public static func nestPrStacks(_ issues: [IssueEntity]) -> [Nested<IssueEntity>] {
        nestPrStacks(
            issues, id: { $0.id }, branch: { $0.branch }, base: { $0.prBaseBranch }
        )
    }

    // MARK: - Helpers

    /// branch → the entry that owns it. First writer wins, so a duplicated
    /// branch never re-parents the list under its later twin.
    private static func branchIndex<T>(_ entries: [T], branch: (T) -> String?) -> [String: T] {
        var out: [String: T] = [:]
        for entry in entries {
            guard let name = nonEmpty(branch(entry)) else { continue }
            if out[name] == nil { out[name] = entry }
        }
        return out
    }

    /// branch → the FIRST entry stacked on it (a fork keeps caller order).
    private static func childIndex<T>(
        _ entries: [T], id: (T) -> String, branch: (T) -> String?, base: (T) -> String?
    ) -> [String: T] {
        let byBranch = branchIndex(entries, branch: branch)
        var out: [String: T] = [:]
        for entry in entries {
            guard let baseRef = nonEmpty(base(entry)) else { continue }
            // A base nobody here owns is not a stack edge.
            guard let lower = byBranch[baseRef], id(lower) != id(entry) else { continue }
            if out[baseRef] == nil { out[baseRef] = entry }
        }
        return out
    }

    private static func nonEmpty(_ value: String?) -> String? {
        guard let value, !value.isEmpty else { return nil }
        return value
    }
}
