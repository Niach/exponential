import Foundation

/// EXP-1291/1303: the stack merge over `pull_requests` rows (EXP-1248's ONE
/// stack confirm, `stack-merge-choice.json` `confirm`, re-keyed from issue
/// rows to the PR's own `base_pull_request_id`), mirrored by Android
/// `PullRequestStack`. Only a LINEAR open stack of 2+ PRs (no fork) has one;
/// Merge = `pullRequests.merge({pullRequestId: top, stack: true})`, landing
/// the whole open chain bottom-up. The server refuses a plain merge of any
/// open-stack member, so such a member's Merge control IS this one.
public enum PullRequestStack {
    /// The confirm: `title` IS the primary button, `throughId` the PR merged
    /// with `stack = true`.
    public struct Confirm: Equatable, Sendable {
        public let title: String
        public let body: String
        public let throughId: String
    }

    /// The confirm's dismiss (`stack-merge-choice.json` `confirm.labels.cancel`).
    public static let cancelLabel = "Cancel"

    /// The open linear chain `pr` sits in, bottom-first; nil on a fork.
    public static func openChain(_ pr: PullRequestEntity, in all: [PullRequestEntity]) -> [PullRequestEntity]? {
        guard pr.isOpen else { return nil }
        let open = all.filter(\.isOpen)
        let byId = Dictionary(open.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        func childCount(_ id: String) -> Int { open.filter { $0.basePullRequestId == id }.count }
        var below: [PullRequestEntity] = []
        var seen: Set<String> = [pr.id]
        var cursor = pr
        while let baseId = cursor.basePullRequestId, let base = byId[baseId], seen.insert(base.id).inserted {
            below.insert(base, at: 0)
            cursor = base
        }
        let chain = below + [pr]
        // Walking up: a member with 2+ open children is a fork, no stack.
        if chain.contains(where: { childCount($0.id) > 1 }) { return nil }
        var above: [PullRequestEntity] = []
        cursor = pr
        while let child = open.first(where: { $0.basePullRequestId == cursor.id }), seen.insert(child.id).inserted {
            if childCount(child.id) > 1 { return nil }
            above.append(child)
            cursor = child
        }
        return chain + above
    }

    /// Non-nil = `pr` is a member of an open stack: Merge lands it all.
    public static func stackConfirm(_ pr: PullRequestEntity, in all: [PullRequestEntity]) -> Confirm? {
        guard let chain = openChain(pr, in: all), chain.count >= 2, let top = chain.last else { return nil }
        let labels = chain.map { "#\($0.number)" }
        return Confirm(
            title: DomainContract.diffUiMergeStack,
            body: "Lands \(chain.count) pull requests, bottom-up: \(labels.joined(separator: ", ")).",
            throughId: top.id
        )
    }
}
