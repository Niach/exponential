import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the iOS styleguide's HOME and THREADS specimens (v4-8-phones
/// panels 1 and 2), drawn by the REAL views over fake rows:
///
/// * `home`: the dawn (`HomeDawn`), THE composer card (`AgentComposerCard`
///   over an account-less model: no machine, so its options line names
///   none), the options line and the Running band (`HomeRunningBand`), under
///   the floating bar with the chat arm selected.
/// * `threads`: the Threads tab (`ThreadsListContent`) — the Agents band, then
///   Needs you · Running · Recent — under the floating bar, Threads lit.
///
/// Capture only: the styleguide suite opens one with `-uiTestingStyleguide
/// <home|threads>` on a signed-out relaunch (`ThreadCardsScreen`'s seam).
struct ThreadsStyleguideScreen: View {
    let kind: String

    static let kinds: Set<String> = ["home", "threads"]

    @Environment(AppDependencies.self) private var deps
    @State private var composer: AgentComposerModel?

    var body: some View {
        ZStack {
            AppBackground()
            VStack(spacing: 0) {
                Text(kind == "home" ? "Agent" : SessionTree.ListCopy.title)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .frame(height: 44)
                if kind == "home" {
                    home
                } else {
                    ThreadsListContent(
                        groups: Self.groups,
                        agents: Self.agents,
                        identifiers: Self.identifiers,
                        teams: [],
                        onOpen: { _ in }
                    )
                }
            }
        }
        .overlay(alignment: .bottom) {
            MobileTabBar(
                agentActive: kind == "home",
                issuesActive: false,
                devicesActive: false,
                actionsActive: false,
                myWorkActive: false,
                threadsActive: kind == "threads",
                unreadCount: 1,
                agentsRunning: true,
                agentsNeedInput: true,
                threadsRunning: true,
                threadsNeedYou: false,
                composeEnabled: true,
                onIssues: {},
                onDevices: {},
                onActions: {},
                onMyWork: {},
                onThreads: {},
                onCompose: {},
                onChat: {}
            )
        }
        .environment(\.accountId, "")
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("threads-styleguide-screen")
        .onAppear {
            if composer == nil {
                composer = AgentComposerModel(
                    teamId: nil,
                    accountId: "",
                    deps: deps,
                    devicesModel: AgentsViewModel(accountId: "", userId: nil, db: deps.db),
                    seed: .empty
                )
            }
        }
    }

    @ViewBuilder
    private var home: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                HomeDawn()
                    .padding(.bottom, HomeDawn.gap - 12)
                if let composer {
                    AgentComposerCard(model: composer) { _ in }
                    AgentOptionsRow(model: composer)
                }
                HomeRunningBand(
                    rows: Self.groups.filter { $0.key != .recent }.flatMap(\.rows),
                    identifiers: Self.identifiers,
                    teams: [],
                    onOpen: { _ in }
                )
                .padding(.top, 8)
            }
            .padding()
        }
        .tabBarBottomInset()
    }

    // MARK: - Fake rows

    private static let teamId = "team-acme"
    private static let stamp = "2026-10-13T09:41:00.000Z"

    private static let agents: [AgentRow] = [
        AgentRow(
            sessionId: "agent-triage", teamId: teamId, name: "Triage", icon: "bug",
            color: "#a855f7", busy: false, needsYou: false
        ),
        AgentRow(
            sessionId: "agent-designer", teamId: teamId, name: "Designer", icon: "palette",
            color: "#ec4899", busy: false, needsYou: false
        ),
        // EXP-1300: the seed's idle, never-run agent lists too (`listAgents`
        // keeps every agent root).
        AgentRow(
            sessionId: "agent-release", teamId: teamId, name: "Release", icon: "package",
            color: "#22c55e", busy: false, needsYou: false
        ),
    ]

    private static func row(
        _ id: String, _ title: String, isPrivate: Bool = false, busy: Bool = false, needsYou: Bool = false
    ) -> ThreadsListRow {
        ThreadsListRow(
            sessionId: id, teamId: teamId, title: title, isPrivate: isPrivate,
            busy: busy, needsYou: needsYou, activityAt: stamp
        )
    }

    /// EXP-1302: the seed's thread roots (seedSessionTree in
    /// apps/web/scripts/seed-screenshots.ts), so this list reads like the web
    /// and desktop `threads` shots.
    private static let groups: [ThreadsListGroup] = [
        ThreadsListGroup(key: .needsYou, title: ThreadsListGroup.Key.needsYou.title, rows: [
            row("t-cold", "Reduce cold start below 800 ms", needsYou: true),
        ]),
        ThreadsListGroup(key: .running, title: ThreadsListGroup.Key.running.title, rows: [
            row("t-offline", "Spike: offline queue", isPrivate: true),
            row("t-reports", "Sort today's widget reports", busy: true),
            row("t-group", "Group board issues by assignee", busy: true),
        ]),
        ThreadsListGroup(key: .recent, title: ThreadsListGroup.Key.recent.title, rows: [
            row("t-labels", "Batch-edit labels from the board"),
            row("t-scroll", "Fix flaky scroll restore on the issue list"),
        ]),
    ]

    private static let identifiers: [String: String] = [
        "t-cold": "APP-5",
        "t-reports": "FB-9",
        "t-group": "APP-14",
        "t-labels": "APP-15",
        "t-scroll": "APP-16",
    ]
}
