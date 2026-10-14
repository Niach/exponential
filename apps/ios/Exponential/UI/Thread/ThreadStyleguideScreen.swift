import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the iOS styleguide's THREAD specimens, drawn by the REAL pieces
/// of the Thread screen — the header (`ThreadHeaderTitle` + the strip,
/// `ThreadStripTabs`), the Conversation (`ThreadConversationView` over
/// `buildSessionThread`) and THE session composer box (`ThreadComposer`) —
/// over fake rows that match the approved drafts:
///
/// * `thread` (v4-3 / v4-8 panel 3): "iOS keyboard and model picker" — one
///   live session whose agent linked EXP-1276 and filed EXP-1277, a child
///   session building both fixes, and the agent's reply.
/// * `agent-channel` (v4-5): the Triage agent's channel — its message, the
///   thread it started as a session card, a person's ask and its reply.
///
/// Capture only: the styleguide suite opens one with `-uiTestingStyleguide
/// <thread|agent-channel>` on a signed-out relaunch (`ThreadCardsScreen`'s
/// seam).
struct ThreadStyleguideScreen: View {
    let kind: String

    static let kinds: Set<String> = ["thread", "agent-channel"]

    private static let me = "u-demo"
    private static let teamId = "team-acme"
    private static let boardId = "board-mobile"

    var body: some View {
        let now = Date()
        let rows = kind == "agent-channel" ? Self.agentChannel(now: now) : Self.liveThread(now: now)
        let view = rows.view(now: now)
        ZStack {
            AppBackground()
            ScrollView {
                ThreadConversationView(rows: rows, view: view)
                    .padding(.vertical, 12)
                    .transcriptColumn()
            }
            .defaultScrollAnchor(.bottom)
            .safeAreaInset(edge: .bottom) {
                ThreadComposer(
                    sessionId: view.composer.sessionId,
                    placeholder: view.composer.placeholder
                )
            }
            .safeAreaInset(edge: .top, spacing: 0) {
                VStack(alignment: .leading, spacing: 4) {
                    if rows.isAgent {
                        ThreadHeaderTitle(
                            title: rows.root.name ?? view.title,
                            agentIcon: rows.root.icon,
                            agentColor: rows.root.color
                        )
                        .padding(.bottom, 8)
                    } else {
                        ThreadHeaderTitle(title: view.title, isPrivate: view.isPrivate)
                        ThreadStripTabs(
                            tabs: view.tabs,
                            selection: .conversation,
                            issue: { rows.issue(id: $0) },
                            onSelect: { _ in },
                            onAdd: {}
                        )
                    }
                }
                .padding(.top, 4)
                .frame(maxWidth: .infinity)
                .headerEdgeChrome(ground: true)
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-styleguide-screen")
    }

    // MARK: - Fake rows

    private static func stamp(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.string(from: date)
    }

    private static func ms(_ date: Date) -> Int {
        Int(date.timeIntervalSince1970 * 1000)
    }

    private static let users = [
        UserEntity(
            id: me, name: "Alex Carter", email: "demo@exponential.at", image: nil,
            createdAt: "2026-01-01T00:00:00.000Z", updatedAt: "2026-01-01T00:00:00.000Z"
        ),
    ]

    private static func issue(
        _ id: String, _ identifier: String, _ title: String, status: String, created: Date
    ) -> IssueEntity {
        IssueEntity(
            id: id, boardId: boardId, number: nil, identifier: identifier, title: title,
            description: nil, status: status, priority: "none", assigneeId: nil, creatorId: me,
            source: nil, dueDate: nil, sortOrder: nil, completedAt: nil, duplicateOfId: nil,
            createdAt: stamp(created), updatedAt: stamp(created)
        )
    }

    private static func link(_ id: String, root: String, issueId: String, order: Double, created: Date) -> SessionIssueEntity {
        SessionIssueEntity(
            id: id, sessionId: root, issueId: issueId, teamId: teamId,
            source: DomainContract.sessionIssueSourceAgent, addedBySessionId: root,
            sortOrder: order, createdAt: stamp(created), updatedAt: stamp(created)
        )
    }

    private static func message(
        _ id: String,
        session: String,
        root: String,
        author: String,
        authorId: String? = nil,
        body: String = "",
        card: String? = nil,
        at: Date
    ) -> MessageEntity {
        MessageEntity(
            id: id, sessionId: session, rootId: root, teamId: teamId, authorKind: author,
            authorId: authorId, body: body, card: card,
            consumedAt: stamp(at.addingTimeInterval(1)),
            createdAt: stamp(at), updatedAt: stamp(at)
        )
    }

    /// v4-3 / v4-8 panel 3: "iOS keyboard and model picker".
    static func liveThread(now: Date) -> ThreadRows {
        let start = now.addingTimeInterval(-6 * 60)
        let linked = start.addingTimeInterval(60)
        let filed = start.addingTimeInterval(90)
        let child = start.addingTimeInterval(120)
        let summary = #"[{"topic":"Summary","text":"Both fixes are in one PR. Tests are running.","at":\#(ms(now.addingTimeInterval(-60)))}]"#
        let root = SessionEntity(
            id: "r1", teamId: teamId, userId: me, deviceId: "d-macbook", cli: "claude",
            status: DomainContract.sessionStatusRunning, busy: true, startedAt: stamp(start),
            shared: true, title: "iOS keyboard and model picker", guide: summary,
            createdAt: stamp(start), updatedAt: stamp(now)
        )
        let build = SessionEntity(
            id: "c1", teamId: teamId, parentId: "r1", rootId: "r1", depth: 1, userId: me,
            deviceId: "d-macbook", cli: "claude", status: DomainContract.sessionStatusRunning, busy: true,
            startedAt: stamp(child), shared: true, title: "Build both fixes",
            createdAt: stamp(child), updatedAt: stamp(now)
        )
        let issues = [
            issue("i-1276", "EXP-1276", "ios keyboard", status: "in_progress", created: now.addingTimeInterval(-86_400)),
            issue("i-1277", "EXP-1277", "ios run error", status: "backlog", created: filed),
        ]
        let messages = [
            message(
                "m1", session: "r1", root: "r1", author: DomainContract.messageAuthorKindUser, authorId: me,
                body: "The keyboard covers the composer on iOS, and picking a model errors. Fix both.", at: start
            ),
            message(
                "m2", session: "r1", root: "r1", author: DomainContract.messageAuthorKindSystem,
                card: #"{"kind":"event","event":"issue_linked","issueId":"i-1276"}"#, at: linked
            ),
            message(
                "m3", session: "r1", root: "r1", author: DomainContract.messageAuthorKindSystem,
                card: #"{"kind":"event","event":"issue_filed","issueId":"i-1277"}"#, at: filed
            ),
            message(
                "m4", session: "r1", root: "r1", author: DomainContract.messageAuthorKindSystem,
                card: #"{"kind":"session","sessionId":"c1"}"#, at: child
            ),
            message(
                "m5", session: "r1", root: "r1", author: DomainContract.messageAuthorKindAgent,
                body: "Both fixes are in one PR. Tests are running.", at: now.addingTimeInterval(-60)
            ),
        ]
        return ThreadRows(
            root: root,
            sessions: [root, build],
            sessionIssues: [
                link("si-1", root: "r1", issueId: "i-1276", order: 1, created: linked),
                link("si-2", root: "r1", issueId: "i-1277", order: 2, created: filed),
            ],
            issues: issues,
            messages: messages,
            pullRequests: [],
            pullRequestIssueIds: [:],
            users: users
        )
    }

    /// v4-5: the Triage agent's channel.
    static func agentChannel(now: Date) -> ThreadRows {
        let start = now.addingTimeInterval(-3 * 3600)
        let report = now.addingTimeInterval(-14 * 60)
        let ask = now.addingTimeInterval(-3 * 60)
        let root = SessionEntity(
            id: "a1", teamId: teamId, kind: DomainContract.sessionKindAgent, userId: me,
            deviceId: "d-macbook", cli: "claude", status: DomainContract.sessionStatusIdle,
            startedAt: stamp(start), shared: true, name: "Triage", icon: "bug", color: "#8b5cf6",
            createdAt: stamp(start), updatedAt: stamp(now)
        )
        let spawned = SessionEntity(
            id: "t2", teamId: teamId, userId: me, deviceId: "d-macbook", cli: "claude",
            status: DomainContract.sessionStatusRunning, busy: true,
            startedAt: stamp(report.addingTimeInterval(30)), shared: true,
            title: "FB-9 Sort today's widget reports", spawnedBySessionId: "a1",
            createdAt: stamp(report.addingTimeInterval(30)), updatedAt: stamp(now)
        )
        let messages = [
            message(
                "m1", session: "a1", root: "a1", author: DomainContract.messageAuthorKindAgent,
                body: "Two new widget reports on Product Feedback. FB-8 is a duplicate of FB-3, merged it. "
                    + "FB-9 needs code, so I started on it:",
                at: report
            ),
            message(
                "m2", session: "a1", root: "a1", author: DomainContract.messageAuthorKindSystem,
                card: #"{"kind":"thread_started","sessionId":"t2"}"#, at: report.addingTimeInterval(30)
            ),
            message(
                "m3", session: "a1", root: "a1", author: DomainContract.messageAuthorKindUser, authorId: me,
                body: "Label the Android ones too, and check the board every hour.", at: ask
            ),
            message(
                "m4", session: "a1", root: "a1", author: DomainContract.messageAuthorKindAgent,
                body: "Done, 3 issues labelled android.", at: ask.addingTimeInterval(40)
            ),
        ]
        return ThreadRows(
            root: root,
            sessions: [root, spawned],
            sessionIssues: [],
            issues: [],
            messages: messages,
            pullRequests: [],
            pullRequestIssueIds: [:],
            users: users
        )
    }
}
