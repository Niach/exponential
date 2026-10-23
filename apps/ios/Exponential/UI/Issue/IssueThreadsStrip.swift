import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1295: one thread (a root session) holding an issue — what the issue
/// page's `Threads` row draws. Title and state are the ×4 rule
/// (`SessionTree.partitionThreads` over the root's tree, its linked issues
/// and its messages), so the chip reads exactly like the Threads list row.
struct IssueThreadLink: Identifiable, Equatable, Sendable {
    /// The root session's id (the thread route's id).
    let id: String
    let title: String
    /// Some session of the tree is not ended.
    let live: Bool
    let busy: Bool
    let needsYou: Bool
    /// The root's agent CLI, for its brand mark.
    let cli: String?

    /// The threads holding `issueId`, newest activity first.
    static func load(db: Database, issueId: String) throws -> [IssueThreadLink] {
        let roots = try SessionQueries.rootsHoldingIssue(db: db, issueId: issueId)
        return try links(db: db, roots: roots)
    }

    /// One thread by its root id (a New issue draft filed into it).
    static func load(db: Database, rootId: String) throws -> IssueThreadLink? {
        guard let root = try SessionQueries.session(db: db, id: rootId) else { return nil }
        return try links(db: db, roots: [root]).first
    }

    private static func links(db: Database, roots: [SessionEntity]) throws -> [IssueThreadLink] {
        let roots = roots.filter { $0.isRoot && !$0.isAgent && $0.appId == nil }
        guard !roots.isEmpty else { return [] }
        var sessions: [TreeSession] = []
        var sessionIssues: [TreeSessionIssue] = []
        var issues: [String: TreeIssue] = [:]
        var messages: [TreeMessage] = []
        for root in roots {
            sessions += try SessionQueries.tree(db: db, rootId: root.id).map { TreeSession(sessionRow: $0) }
            sessionIssues += try SessionQueries.sessionIssues(db: db, rootId: root.id)
                .map { TreeSessionIssue(sessionIssueRow: $0) }
            for issue in try SessionQueries.issues(db: db, rootId: root.id) {
                issues[issue.id] = TreeIssue(issueRow: issue)
            }
            // Only the root's own messages can name an untitled thread
            // (`SessionTitle`'s first-prompt rung picks the inputs).
            messages += try SessionQueries.messages(db: db, sessionId: root.id)
                .map { TreeMessage(messageRow: $0) }
        }
        let rows = SessionTree.partitionThreads(
            sessions: sessions,
            sessionIssues: sessionIssues,
            issues: Array(issues.values),
            messages: messages,
            teamId: nil
        ).flatMap(\.rows)
        let byId = Dictionary(rows.map { ($0.sessionId, $0) }, uniquingKeysWith: { a, _ in a })
        let liveRoots = Set(sessions.filter { $0.status != DomainContract.sessionStatusEnded }.map(\.rootId))
        return roots.compactMap { root in
            guard let row = byId[root.id] else { return nil }
            return IssueThreadLink(
                id: root.id,
                title: row.title,
                live: liveRoots.contains(root.id),
                busy: row.busy,
                needsYou: row.needsYou,
                cli: root.cli
            )
        }
    }
}

/// EXP-1295: the issue page's `Threads` row (draft v4-7): the label, then one
/// chip per thread holding the issue, each opening its thread. The host
/// mounts it only while the issue sits in a thread.
struct IssueThreadsStrip: View {
    let threads: [IssueThreadLink]

    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            Text(SessionTree.ListCopy.title)
                .font(.footnote)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize()
            ScrollView(.horizontal) {
                HStack(spacing: 6) {
                    ForEach(threads) { thread in
                        IssueThreadChip(thread: thread) {
                            pushRoute(.thread(accountId: accountId, threadId: thread.id))
                        }
                    }
                }
            }
            .scrollIndicators(.hidden)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("issue-threads-row")
    }
}

/// One thread as a chip, in the chip box's paint: the agent's mark (working
/// while busy, an amber dot while it needs you, dimmed once every session
/// ended), then the thread's title.
struct IssueThreadChip: View {
    let thread: IssueThreadLink
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 6) {
                mark
                    .frame(width: 12, height: 12)
                Text(thread.title)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                    .lineLimit(1)
                    .frame(maxWidth: IssueChipTokens.titleMaxWidth, alignment: .leading)
            }
            .padding(.horizontal, IssueChipTokens.horizontalPadding + 2)
            .padding(.vertical, IssueChipTokens.verticalPadding + 1)
            .background(
                RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
                    .fill(Color(MarkdownStyle.chipBackground))
            )
            .overlay(
                RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
                    .strokeBorder(Color(MarkdownStyle.chipBorder), lineWidth: IssueChipTokens.borderWidth)
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(thread.title)
        .accessibilityIdentifier("issue-thread-chip")
    }

    @ViewBuilder
    private var mark: some View {
        Group {
            if thread.live, thread.busy {
                AgentWorkingMark(agent: thread.cli)
            } else {
                BeatingAgentMark(agent: thread.cli, beating: false)
            }
        }
        // EXP-1208: an ended run's mark is its brand mark, dimmed.
        .opacity(thread.live ? 1 : 0.5)
        .overlay(alignment: .topTrailing) {
            if thread.needsYou {
                Circle()
                    .fill(DesignTokens.Semantic.yellow)
                    .frame(width: 5, height: 5)
                    .offset(x: 2, y: -2)
            }
        }
    }
}
