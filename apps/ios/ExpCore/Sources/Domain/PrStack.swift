import Foundation

/// EXP-897, the PR STACK, derived from synced data alone and mirrored ×4
/// (web `lib/pr-stack.ts`, desktop `domain::pr_stack`, Android `PrStack.kt`).
/// SLOP-3: the client CHAIN, read by the related-work badge (`PrGraph`).
/// EXP-1248: `openPrShape` tells a tree from a stack, `stackView` = the stack
/// rail (Guide card, Reviews), `stackMergeConfirm` = the ONE stack merge
/// confirm.
///
/// One edge, one rule: a pull request is stacked ON another when its
/// `prBaseBranch` equals the lower one's `branch` (both non-empty). No stack
/// table, no server round-trip, `issues.pr_base_branch` is synced and every
/// client derives the same chain from it.
///
/// Three guards the shared tests pin:
/// - a base NOBODY in the list owns ends the walk (the PR is simply based on
///   `main`, or on a branch outside this team's synced rows);
/// - a cycle breaks where it FIRST repeats (defensive: GitHub cannot make
///   one, a half-synced snapshot can);
/// - a fork follows the FIRST child in the caller's order.
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

    // MARK: - Team scope

    // A client database holds EVERY team of the account and branch names
    // (`exp/<IDENTIFIER>`) repeat across teams, so a chain read off all of
    // them can splice another team's pull request in. Every caller narrows
    // its rows to the subject's team first (web reads the team's boards,
    // `use-stack-merge-choice.ts`). An issue row carries no team id, its
    // board does.

    /// `issues` narrowed to the boards of `teamId`.
    public static func teamPool(
        _ issues: [IssueEntity], teamId: String, boards: [BoardEntity]
    ) -> [IssueEntity] {
        let boardIds = Set(boards.filter { $0.teamId == teamId }.map(\.id))
        return issues.filter { boardIds.contains($0.boardId) }
    }

    /// `issues` narrowed to the team `issue` belongs to. A board that has not
    /// synced yet keeps the issue's own board alone, never a guess across
    /// teams.
    public static func teamPool(
        of issue: IssueEntity, issues: [IssueEntity], boards: [BoardEntity]
    ) -> [IssueEntity] {
        guard let teamId = boards.first(where: { $0.id == issue.boardId })?.teamId else {
            return issues.filter { $0.boardId == issue.boardId }
        }
        return teamPool(issues, teamId: teamId, boards: boards)
    }

    // MARK: - EXP-1248: tree vs stack, the stack rail, the one confirm

    // ×4 (web `prComponent`/`prGraphShape`/`openPrShape`/`stackView`/
    // `stackMergeConfirm` in `lib/pr-stack.ts`, desktop `domain::pr_stack`,
    // Android `PrStack`), locked by `fixtures/pr-stack-view.json` and
    // `fixtures/stack-merge-choice.json` (`confirm`).

    /// A base-chained component's shape: `tree` = a fork anywhere (follow-up
    /// runs; nests with tree guides), `stack` = linear (GitHub stacks it; the
    /// stack rail), `single` = one pull request.
    public enum PrGraphShape: String, Sendable {
        case tree
        case stack
        case single
    }

    /// Every node base-chained to `node` (either direction), in input order.
    public static func prComponent(_ node: IssueEntity, in nodes: [IssueEntity]) -> [IssueEntity] {
        let byBranch = branchIndex(nodes, branch: { $0.branch })
        var seen: Set<String> = [node.id]
        var queue = [node]
        while !queue.isEmpty {
            let current = queue.removeFirst()
            var linked: [IssueEntity] = []
            if let base = nonEmpty(current.prBaseBranch), let lower = byBranch[base] {
                linked.append(lower)
            }
            if let branch = nonEmpty(current.branch) {
                linked += nodes.filter { nonEmpty($0.prBaseBranch) == branch }
            }
            for next in linked where !seen.contains(next.id) {
                seen.insert(next.id)
                queue.append(next)
            }
        }
        return nodes.filter { seen.contains($0.id) }
    }

    /// `tree` when any member carries two children, else `stack` (2+) or
    /// `single`.
    public static func prGraphShape(_ component: [IssueEntity]) -> PrGraphShape {
        guard component.count >= 2 else { return .single }
        let byBranch = branchIndex(component, branch: { $0.branch })
        var children: [String: Int] = [:]
        for node in component {
            guard let base = nonEmpty(node.prBaseBranch), let parent = byBranch[base],
                  parent.id != node.id
            else { continue }
            let count = (children[parent.id] ?? 0) + 1
            if count > 1 { return .tree }
            children[parent.id] = count
        }
        return .stack
    }

    /// The OPEN rows (identifier order) with ONE representative per pull
    /// request (a batch PR = its lowest identifier); nil without an open PR.
    private static func openRepresentatives(
        _ issue: IssueEntity, issues: [IssueEntity]
    ) -> (open: [IssueEntity], reps: [IssueEntity], subject: IssueEntity)? {
        guard issue.prState == DomainContract.prStateOpen else { return nil }
        var open = issues.filter { $0.prState == DomainContract.prStateOpen }
        if !open.contains(where: { $0.id == issue.id }) { open.append(issue) }
        let ident: (IssueEntity) -> [UInt16] = { Array(($0.identifier ?? "").utf16) }
        open = open.enumerated().sorted { a, b in
            let (ia, ib) = (ident(a.element), ident(b.element))
            if ia != ib { return ia.lexicographicallyPrecedes(ib) }
            return a.offset < b.offset
        }.map(\.element)
        var reps: [IssueEntity] = []
        var repOf: [String: IssueEntity] = [:]
        for row in open {
            if let url = nonEmpty(row.prUrl), let rep = reps.first(where: { $0.prUrl == url }) {
                repOf[row.id] = rep
            } else {
                reps.append(row)
                repOf[row.id] = row
            }
        }
        guard let subject = repOf[issue.id] else { return nil }
        return (open, reps, subject)
    }

    /// The shape of the open component `issue`'s pull request sits in
    /// (`single` without an open pull request).
    public static func openPrShape(_ issue: IssueEntity, issues: [IssueEntity]) -> PrGraphShape {
        guard let picked = openRepresentatives(issue, issues: issues) else { return .single }
        return prGraphShape(prComponent(picked.subject, in: picked.reps))
    }

    /// The linear open stack `issue` sits in: the chain bottom → top, the
    /// subject's representative and each pull request's batch partners.
    private static func openStack(
        _ issue: IssueEntity, issues: [IssueEntity]
    ) -> (chain: [IssueEntity], subject: IssueEntity, siblings: (IssueEntity) -> Int)? {
        guard let picked = openRepresentatives(issue, issues: issues) else { return nil }
        guard prGraphShape(prComponent(picked.subject, in: picked.reps)) == .stack else { return nil }
        let chain = stackChain(picked.subject, issues: picked.reps)
        let open = picked.open
        let siblings: (IssueEntity) -> Int = { row in
            guard let url = nonEmpty(row.prUrl) else { return 0 }
            return open.filter { $0.id != row.id && $0.prUrl == url }.count
        }
        return (chain, picked.subject, siblings)
    }

    /// One row of the stack rail.
    public struct StackViewRow: Equatable, Sendable {
        public let issueId: String
        public let identifier: String
        public let title: String
        public let prNumber: Int?
        /// The subject's pull request: the row wears the active wash.
        public let isCurrent: Bool

        public init(issueId: String, identifier: String, title: String, prNumber: Int?, isCurrent: Bool) {
            self.issueId = issueId
            self.identifier = identifier
            self.title = title
            self.prNumber = prNumber
            self.isCurrent = isCurrent
        }
    }

    public struct StackView: Equatable, Sendable {
        /// TOP first, one row per open pull request.
        public let rows: [StackViewRow]
        /// The trailing muted row: the bottom member's base; nil = unknown.
        public let baseBranch: String?

        public init(rows: [StackViewRow], baseBranch: String?) {
            self.rows = rows
            self.baseBranch = baseBranch
        }
    }

    /// The stack rail of `issue`'s pull request (the Guide's Stack card, a
    /// Reviews stack): nil unless it sits in a LINEAR open stack of 2+ pull
    /// requests. Only OPEN pull requests count; a fork anywhere = a tree = nil.
    public static func stackView(_ issue: IssueEntity, issues: [IssueEntity]) -> StackView? {
        guard let stack = openStack(issue, issues: issues), stack.chain.count >= 2 else { return nil }
        return StackView(
            rows: stack.chain.reversed().map { row in
                StackViewRow(
                    issueId: row.id,
                    identifier: row.identifier ?? "",
                    title: row.title,
                    prNumber: row.prNumber,
                    isCurrent: row.id == stack.subject.id
                )
            },
            baseBranch: nonEmpty(stack.chain[0].prBaseBranch)
        )
    }

    /// The ONE merge control's label on an open-stack member, and the hover /
    /// long-press action on a stack row (contract `diffUi`).
    public static let mergeThroughLabel = DomainContract.diffUiMergeThrough
    public static let stackConfirmCancelLabel = "Cancel"

    /// `stack` = the merge control (the whole open chain, through its top);
    /// `through` = Merge through here on this member.
    public enum StackConfirmMode: String, Sendable {
        case stack
        case through
    }

    /// The ONE confirm a stack merge asks (EXP-1248, it replaces the 3-way
    /// dialog). Send `issues.mergePr({issueId, mergeStack: true})`.
    public struct StackMergeConfirm: Equatable, Sendable {
        /// The title AND the primary button.
        public let title: String
        /// What lands, bottom first; a batch PR = `EXP-874 +2`.
        public let landing: [String]
        /// What stays open above it (GitHub retargets it).
        public let staysOpen: [String]
        public let body: String
        /// The `issues.mergePr` target (always with `mergeStack: true`).
        public let issueId: String

        public init(title: String, landing: [String], staysOpen: [String], body: String, issueId: String) {
            self.title = title
            self.landing = landing
            self.staysOpen = staysOpen
            self.body = body
            self.issueId = issueId
        }
    }

    /// Nil = not in a linear open stack: the plain merge confirm.
    public static func stackMergeConfirm(
        _ issue: IssueEntity, issues: [IssueEntity], mode: StackConfirmMode
    ) -> StackMergeConfirm? {
        guard let stack = openStack(issue, issues: issues), stack.chain.count >= 2,
              let index = stack.chain.firstIndex(where: { $0.id == stack.subject.id })
        else { return nil }
        func label(_ row: IssueEntity) -> String {
            let extra = stack.siblings(row)
            let ident = row.identifier ?? ""
            return extra > 0 ? "\(ident) +\(extra)" : ident
        }
        let through = mode == .stack ? stack.chain.count - 1 : index
        let landing = stack.chain[...through].map(label)
        let staysOpen = stack.chain[(through + 1)...].map(label)
        let lands = landing.count == 1
            ? "Lands 1 pull request: \(landing[0])."
            : "Lands \(landing.count) pull requests, bottom-up: \(landing.joined(separator: ", "))."
        let open = staysOpen.isEmpty
            ? ""
            : " \(staysOpen.joined(separator: ", ")) \(staysOpen.count == 1 ? "stays" : "stay") open."
        return StackMergeConfirm(
            title: mode == .stack ? DomainContract.diffUiMergeStack : DomainContract.diffUiMergeThrough,
            landing: Array(landing),
            staysOpen: Array(staysOpen),
            body: lands + open,
            issueId: mode == .stack ? stack.chain[stack.chain.count - 1].id : issue.id
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
