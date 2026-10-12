import ExpCore
import ExpUI
import SwiftUI

/// EXP-1284: the iOS styleguide's THREAD specimens (`sg_thread`,
/// `sg_thread-runs`; views.json `thread` / `thread-runs`), drawn by the REAL
/// pieces of the Thread screen — the header band with the title and the
/// child strip as the pager tabs (`ThreadStripTabs`), the conversation
/// (`ThreadConversationView` over THE thread view, `ThreadViews.build`) and
/// the reply box (`ThreadReplyBox`) — over fake rows that match the
/// approved drafts:
///
/// * `thread` (drafts 1/3): one live run of mine on macbook whose agent
///   linked EXP-1276 and filed + linked EXP-1277, its reply under them.
/// * `thread-runs` (draft 2): "UI batch plan" — Run 1 ended, Run 2 its
///   resume (ended), Runs 3 and 4 the live children of Run 2 on two devices
///   as ONE card; a child's menu (Link an issue · New issue in this thread ·
///   Start a session here · Unlink EXP-1270) open over the strip.
///
/// Capture only: the styleguide suite opens one with `-uiTestingStyleguide
/// <thread|thread-runs>` on a signed-out relaunch (`ThreadCardsScreen`'s
/// seam).
struct ThreadStyleguideScreen: View {
    let kind: String

    static let kinds: Set<String> = ["thread", "thread-runs"]

    private static let me = "u-demo"
    private static let teamId = "team-acme"
    private static let boardId = "board-mobile"

    @State private var reply = ""

    private var rows: ThreadRows {
        kind == "thread-runs" ? Self.runsThread(now: Date()) : Self.liveThread(now: Date())
    }

    var body: some View {
        let rows = self.rows
        let view = rows.view
        ZStack {
            AppBackground()
            ScrollView {
                ThreadConversationView(rows: rows, view: view, me: Self.me)
                    .padding(.vertical, 12)
                    .transcriptColumn()
            }
            .safeAreaInset(edge: .bottom) {
                ThreadReplyBox(text: $reply, onSend: {})
            }
            .workHeaderBand(ground: true) {
                VStack(alignment: .leading, spacing: 4) {
                    ThreadHeaderTitle(title: view.title, caption: view.headerCaption)
                    ThreadStripTabs(
                        strip: view.strip,
                        selection: .conversation,
                        issue: { rows.issue(id: $0) },
                        onSelect: { _ in },
                        onAdd: { _ in }
                    )
                }
                .padding(.top, 4)
            }
            if kind == "thread-runs" {
                childMenu
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-styleguide-screen")
    }

    /// THE Menu as an issue tab's long press opens it (draft 2), drawn in
    /// place under the strip.
    private var childMenu: some View {
        GlassMenuSurface {
            VStack(spacing: 0) {
                GlassMenuItemLabel(ThreadViews.linkIssueLabel, icon: AppIcons.navSearch)
                GlassMenuItemLabel(ThreadViews.newIssueLabel, icon: AppIcons.uiAdd)
                GlassMenuItemLabel(ThreadViews.startRunLabel, icon: AppIcons.actionRun)
                Rectangle()
                    .fill(.white.opacity(GlassMenuTokens.dividerOpacity))
                    .frame(height: GlassTokens.hairline)
                GlassMenuItemLabel(ThreadViews.unlinkLabel("EXP-1270"), icon: AppIcons.uiClose)
            }
            .frame(minWidth: GlassMenuTokens.minWidth, maxWidth: GlassMenuTokens.maxWidth, alignment: .leading)
            .fixedSize(horizontal: true, vertical: true)
        }
        .padding(.top, 90)
        .padding(.trailing, 16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
        .accessibilityIdentifier("thread-styleguide-menu")
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

    private static func issue(
        _ id: String, _ identifier: String, _ title: String, status: String, created: Date
    ) -> IssueEntity {
        IssueEntity(
            id: id, boardId: boardId, number: nil, identifier: identifier, title: title,
            description: nil, status: status, priority: "none", assigneeId: nil, creatorId: me,
            source: nil, dueDate: nil, sortOrder: nil, completedAt: nil, duplicateOfId: nil,
            prUrl: nil, prNumber: nil, prState: nil, branch: nil, prMergedAt: nil,
            createdAt: stamp(created), updatedAt: stamp(created)
        )
    }

    private static func run(
        _ id: String,
        device: String,
        status: String,
        started: Date,
        ended: Date? = nil,
        busy: Bool = false,
        issueId: String? = nil,
        actionName: String? = nil,
        agentTitle: String? = nil,
        parent: String? = nil,
        resumedFrom: String? = nil,
        results: String? = nil
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id, issueId: issueId, teamId: teamId, userId: me, deviceLabel: device,
            status: status, agent: "claude", agentBusy: busy, agentTitle: agentTitle,
            results: results, actionName: actionName, resumedFromId: resumedFrom,
            parentSessionId: parent, startedAt: stamp(started), endedAt: ended.map(stamp),
            createdAt: stamp(started), updatedAt: stamp(ended ?? started)
        )
    }

    private static func item(
        _ id: String,
        thread: String,
        issueId: String? = nil,
        sessionId: String? = nil,
        agentRun: String? = nil,
        order: Double,
        created: Date
    ) -> ThreadItemEntity {
        ThreadItemEntity(
            id: id,
            threadId: thread,
            kind: issueId != nil ? DomainContract.threadItemKindIssue : DomainContract.threadItemKindRun,
            issueId: issueId,
            sessionId: sessionId,
            teamId: teamId,
            boardId: issueId != nil ? boardId : nil,
            addedBy: me,
            addedBySessionId: agentRun,
            source: agentRun != nil ? DomainContract.threadItemSourceAgent : DomainContract.threadItemSourceUser,
            sortOrder: order,
            createdAt: stamp(created),
            updatedAt: stamp(created)
        )
    }

    /// Drafts 1/3: "iOS keyboard and model picker".
    static func liveThread(now: Date) -> ThreadRows {
        let start = now.addingTimeInterval(-6 * 60)
        let linked = start.addingTimeInterval(90)
        let filed = start.addingTimeInterval(150)
        let summary = #"[{"topic":"Summary","text":"Both fixes are in one PR. Tests are running.","at":\#(ms(now.addingTimeInterval(-60)))}]"#
        let thread = ThreadEntity(
            id: "t-live", teamId: teamId, title: nil, createdBy: me, results: summary,
            createdAt: stamp(start), updatedAt: stamp(filed)
        )
        let runs = [
            run(
                "r1", device: "macbook", status: DomainContract.codingSessionStatusRunning,
                started: start, busy: true, actionName: PastRuns.chatRunName,
                agentTitle: "iOS keyboard and model picker", results: summary
            ),
        ]
        let issues = [
            issue("i-1276", "EXP-1276", "ios keyboard", status: "in_progress", created: now.addingTimeInterval(-86_400)),
            issue("i-1277", "EXP-1277", "ios run error", status: "backlog", created: filed.addingTimeInterval(-20)),
        ]
        let items = [
            item("it-r1", thread: "t-live", sessionId: "r1", order: 1, created: start),
            item("it-1276", thread: "t-live", issueId: "i-1276", agentRun: "r1", order: 2, created: linked),
            item("it-1277", thread: "t-live", issueId: "i-1277", agentRun: "r1", order: 3, created: filed),
        ]
        return ThreadRows(thread: thread, items: items, sessions: runs, issues: issues, devices: [])
    }

    /// Draft 2: "UI batch plan" with four runs.
    static func runsThread(now: Date) -> ThreadRows {
        let run1Start = now.addingTimeInterval(-5 * 3600)
        let run1End = run1Start.addingTimeInterval(80 * 60)
        let run2Start = now.addingTimeInterval(-2 * 3600)
        let run2End = now.addingTimeInterval(-10 * 60)
        let run3Start = now.addingTimeInterval(-22 * 60)
        let run4Start = now.addingTimeInterval(-9 * 60)
        let guide = #"[{"topic":"Summary","text":"Threads land in two PRs; the cards follow.","at":\#(ms(run2End))}]"#
        let thread = ThreadEntity(
            id: "t-runs", teamId: teamId, title: "UI batch plan", createdBy: me, results: guide,
            createdAt: stamp(run1Start), updatedAt: stamp(run4Start)
        )
        let runs = [
            run(
                "r1", device: "macbook", status: DomainContract.codingSessionStatusEnded,
                started: run1Start, ended: run1End, actionName: PastRuns.chatRunName,
                agentTitle: "Labeled 165 issues, drafted the batch, filed EXP-1284"
            ),
            run(
                "r2", device: "macbook", status: DomainContract.codingSessionStatusEnded,
                started: run2Start, ended: run2End, actionName: PastRuns.chatRunName,
                agentTitle: "Applied the decisions, rewrote the plan", resumedFrom: "r1", results: guide
            ),
            run(
                "r3", device: "mint", status: DomainContract.codingSessionStatusRunning,
                started: run3Start, busy: true, issueId: "i-1284", parent: "r2"
            ),
            run(
                "r4", device: "macbook", status: DomainContract.codingSessionStatusRunning,
                started: run4Start, busy: true, issueId: "i-1270", parent: "r2"
            ),
        ]
        let issues = [
            issue("i-1259", "EXP-1259", "UX batch 2", status: "backlog", created: run1Start.addingTimeInterval(-86_400)),
            issue("i-1284", "EXP-1284", "Threads schema + MCP", status: "backlog", created: run1Start.addingTimeInterval(600)),
            issue("i-1270", "EXP-1270", "Getting started cards", status: "backlog", created: run1Start.addingTimeInterval(-3600)),
        ]
        let items = [
            item("it-r1", thread: "t-runs", sessionId: "r1", order: 1, created: run1Start),
            item("it-1259", thread: "t-runs", issueId: "i-1259", order: 2, created: run1Start.addingTimeInterval(60)),
            item("it-1284", thread: "t-runs", issueId: "i-1284", order: 3, created: run1Start.addingTimeInterval(700)),
            item("it-1270", thread: "t-runs", issueId: "i-1270", order: 4, created: run1Start.addingTimeInterval(900)),
            item("it-r2", thread: "t-runs", sessionId: "r2", order: 5, created: run2Start),
            item("it-r3", thread: "t-runs", sessionId: "r3", order: 6, created: run3Start),
            item("it-r4", thread: "t-runs", sessionId: "r4", order: 7, created: run4Start),
        ]
        return ThreadRows(thread: thread, items: items, sessions: runs, issues: issues, devices: [])
    }
}
