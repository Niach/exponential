import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the iOS styleguide's THREAD specimens, drawn by the REAL pieces
/// of the Thread screen — the header (`ThreadHeaderTitle` + the strip,
/// `ThreadStripTabs`), the Conversation (`ThreadConversationView` over
/// `buildSessionThread`) and THE session composer box (`ThreadComposer`) —
/// over fake rows that match the approved drafts:
///
/// * `thread` (EXP-1303): the seed's cold-start thread web/desktop
///   photograph — APP-5 with APP-10 and FB-3 linked, the child profiling the
///   launch path, the agent's reply and its pending question.
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
        // The channel's rows print a time of day: photographed at 09:41
        // like the status bar (web pins DEMO_AGENT_CHANNEL_TIMES the same way).
        let now = kind == "agent-channel"
            ? Calendar.current.date(bySettingHour: 9, minute: 41, second: 0, of: Date()) ?? Date()
            : Date()
        let rows = kind == "agent-channel" ? Self.agentChannel(now: now) : Self.coldStartThread(now: now)
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
                    placeholder: rows.composerPlaceholder(view)
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

    private static func link(
        _ id: String, root: String, issueId: String, order: Double,
        source: String = DomainContract.sessionIssueSourceAgent, created: Date
    ) -> SessionIssueEntity {
        SessionIssueEntity(
            id: id, sessionId: root, issueId: issueId, teamId: teamId,
            source: source, addedBySessionId: root,
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

    /// EXP-1303: the seed's cold-start thread, the one web/desktop `thread`
    /// photograph (Android `ThreadCaptures`): APP-5 with APP-10 and FB-3
    /// linked by its agent, the child profiling the launch path waiting on
    /// the question at the end.
    static func coldStartThread(now: Date) -> ThreadRows {
        let start = now.addingTimeInterval(-8 * 60)
        let child = now.addingTimeInterval(-7 * 60)
        let reply = now.addingTimeInterval(-3 * 60)
        let filed = now.addingTimeInterval(-150)
        let question = now.addingTimeInterval(-2 * 60)
        let guide = #"[{"topic":"Summary","text":"Deferred the shape subscribe until after the first frame and added a board snapshot cache that paints the last board before sync finishes: 740 ms on the CI device, under the 800 ms target. Filed #FB-3 for the editor's highlighting bundle.","at":\#(ms(reply))},{"topic":"Board snapshot cache","text":"The last rendered board is written to disk on every sync and painted on launch, then replaced in place once the shape catches up.","at":\#(ms(reply))}]"#
        let root = SessionEntity(
            id: "s-root", teamId: teamId, userId: me, deviceId: "d-macbook", cli: "claude",
            status: DomainContract.sessionStatusRunning, startedAt: stamp(start),
            shared: true, guide: guide,
            createdAt: stamp(start), updatedAt: stamp(now)
        )
        let profile = SessionEntity(
            id: "s-profile", teamId: teamId, parentId: "s-root", rootId: "s-root", depth: 1, userId: me,
            deviceId: "d-macbook", cli: "claude", status: DomainContract.sessionStatusRunning, needsInput: true,
            startedAt: stamp(child), shared: true, title: "Profile the launch path",
            createdAt: stamp(child), updatedAt: stamp(now)
        )
        let issues = [
            issue("i-app5", "APP-5", "Reduce cold start below 800 ms", status: "in_progress", created: now.addingTimeInterval(-86_400)),
            issue("i-app10", "APP-10", "Migrate image cache to on-disk LRU", status: "backlog", created: now.addingTimeInterval(-86_400)),
            issue("i-fb3", "FB-3", "Syntax highlighting ships every language", status: "backlog", created: filed),
        ]
        let messages = [
            message(
                "m-ask", session: "s-root", root: "s-root", author: DomainContract.messageAuthorKindUser, authorId: me,
                body: "Cold start is at 1.4s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of the critical path.",
                at: start.addingTimeInterval(10)
            ),
            message(
                "m-card", session: "s-root", root: "s-root", author: DomainContract.messageAuthorKindAgent,
                card: #"{"kind":"session","sessionId":"s-profile"}"#, at: child
            ),
            message(
                "m-reply", session: "s-root", root: "s-root", author: DomainContract.messageAuthorKindAgent,
                body: "### Board snapshot cache\n\nThe last rendered board is written to disk on every sync and painted on launch, "
                    + "then replaced in place once the shape catches up.\n\nDeferred the shape subscribe until after the first frame "
                    + "and added a board snapshot cache that paints the last board before sync finishes: 740 ms on the CI device, "
                    + "under the 800 ms target. Filed #FB-3 for the editor's highlighting bundle.",
                at: reply
            ),
            message(
                "m-filed", session: "s-root", root: "s-root", author: DomainContract.messageAuthorKindSystem,
                card: #"{"kind":"event","event":"issue_filed","issueId":"i-fb3"}"#, at: filed
            ),
            message(
                "m-question", session: "s-root", root: "s-root", author: DomainContract.messageAuthorKindAgent,
                card: #"{"kind":"question","question":"Cold start is at 740ms (target <800ms). Lazy-load the markdown editor too, or open the PR with what we have?","options":["Also lazy-load the markdown editor","Open the PR with what we have"]}"#,
                at: question
            ),
        ]
        return ThreadRows(
            root: root,
            sessions: [root, profile],
            sessionIssues: [
                link("si-1", root: "s-root", issueId: "i-app5", order: 1, source: DomainContract.sessionIssueSourceUser, created: start),
                link("si-2", root: "s-root", issueId: "i-app10", order: 2, created: child),
                link("si-3", root: "s-root", issueId: "i-fb3", order: 3, created: filed),
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
            startedAt: stamp(start), shared: true, name: "Triage", icon: "bug", color: "#a855f7",
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
