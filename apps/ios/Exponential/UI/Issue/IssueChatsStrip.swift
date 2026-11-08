import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1295: one chat (a root session) holding an issue — what the issue
/// page's `Chats` row draws. Title and state are the ×4 rule
/// (`SessionTree.partitionChats` over the root's tree, its linked issues
/// and its messages), so the chip reads exactly like the Chats list row.
struct IssueChatLink: Identifiable, Equatable, Sendable {
    /// The root session's id (the chat route's id).
    let id: String
    let title: String
    /// Some session of the tree is not ended.
    let live: Bool
    let busy: Bool
    let needsYou: Bool
    /// The root's agent CLI, for its brand mark.
    let cli: String?

    /// The chats holding `issueId`, newest activity first.
    static func load(db: Database, issueId: String) throws -> [IssueChatLink] {
        let roots = try SessionQueries.rootsHoldingIssue(db: db, issueId: issueId)
        return try links(db: db, roots: roots)
    }

    /// One chat by its root id (a New issue draft filed into it).
    static func load(db: Database, rootId: String) throws -> IssueChatLink? {
        guard let root = try SessionQueries.session(db: db, id: rootId) else { return nil }
        return try links(db: db, roots: [root]).first
    }

    private static func links(db: Database, roots: [SessionEntity]) throws -> [IssueChatLink] {
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
            // Only the root's own messages can name an untitled chat
            // (`SessionTitle`'s first-prompt rung picks the inputs).
            messages += try SessionQueries.messages(db: db, sessionId: root.id)
                .map { TreeMessage(messageRow: $0) }
        }
        let rows = SessionTree.partitionChats(
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
            return IssueChatLink(
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

/// EXP-1295: the issue page's `Chats` row (draft v4-7): the label, then one
/// chip per chat holding the issue, each opening its chat. The host
/// mounts it only while the issue sits in a chat.
struct IssueChatsStrip: View {
    let chats: [IssueChatLink]

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
                    ForEach(chats) { chat in
                        IssueChatChip(chat: chat) {
                            pushRoute(.chat(accountId: accountId, chatId: chat.id))
                        }
                    }
                }
            }
            .scrollIndicators(.hidden)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("issue-chats-row")
    }
}

/// One chat as a chip, in the chip box's paint: the agent's mark (working
/// while busy, an amber dot while it needs you, dimmed once every session
/// ended), then the chat's title.
struct IssueChatChip: View {
    let chat: IssueChatLink
    let onTap: () -> Void

    var body: some View {
        // EXP-1553 (web `Pill` + desktop `glass_pill` ×4): THE round glass
        // pill, the agent's mark leading, never a square chip box.
        GlassPill(chat.title, size: .sm, mode: .action(onTap)) {
            mark
                .frame(width: 12, height: 12)
        }
        .frame(maxWidth: IssueChipTokens.titleMaxWidth + 40, alignment: .leading)
        .accessibilityLabel(chat.title)
        .accessibilityIdentifier("issue-chat-chip")
    }

    @ViewBuilder
    private var mark: some View {
        Group {
            if chat.live, chat.busy {
                AgentWorkingMark(agent: chat.cli)
            } else {
                BeatingAgentMark(agent: chat.cli, beating: false)
            }
        }
        // EXP-1208: an ended run's mark is its brand mark, dimmed.
        .opacity(chat.live ? 1 : 0.5)
        .overlay(alignment: .topTrailing) {
            if chat.needsYou {
                Circle()
                    .fill(DesignTokens.Semantic.yellow)
                    .frame(width: 5, height: 5)
                    .offset(x: 2, y: -2)
            }
        }
    }
}
