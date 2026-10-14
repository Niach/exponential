import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the iOS styleguide's SESSION specimen (v4-4) — a session's raw
/// work drawn by the REAL `AgentSessionView` (the header's title + Stop, the
/// transcript, the task list, the composer) over a model that never dials,
/// fed fake `activity` events through the live reducer
/// (`AgentSessionModel.loadStyleguide`): "Profile the launch path", a child
/// session of "Reduce cold start below 800 ms", mid-turn.
///
/// Capture only: the styleguide suite opens it with `-uiTestingStyleguide
/// session` on a signed-out relaunch (`ThreadCardsScreen`'s seam).
struct SessionStyleguideScreen: View {
    static let kinds: Set<String> = ["session"]

    private static let me = "u-demo"
    private static let accountId = "styleguide"
    private static let threadTitle = "Reduce cold start below 800 ms"

    @Environment(AppDependencies.self) private var deps
    @State private var model: AgentSessionModel?

    private static let session: SessionEntity = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let started = formatter.string(from: Date().addingTimeInterval(-6 * 60))
        return SessionEntity(
            id: "s-profile",
            teamId: "team-acme",
            parentId: "s-cold-start",
            rootId: "s-cold-start",
            depth: 1,
            userId: SessionStyleguideScreen.me,
            deviceId: "d-macbook",
            cli: "claude",
            status: DomainContract.sessionStatusRunning,
            busy: true,
            startedAt: started,
            title: "Profile the launch path",
            createdAt: started,
            updatedAt: started
        )
    }()

    var body: some View {
        // EXP-1308: pushed over its thread, as a session card opens it, so the
        // system back button names the thread like the other clients' back link.
        NavigationStack {
            AppBackground()
                .navigationTitle(Self.threadTitle)
                .navigationDestination(isPresented: .constant(true)) {
                    Group {
                        if let model {
                            AgentSessionView(
                                accountId: Self.accountId, session: Self.session, previewModel: model
                            )
                        } else {
                            AppBackground()
                        }
                    }
                }
        }
        .preferredColorScheme(.dark)
        .onAppear {
            guard model == nil else { return }
            let preview = AgentSessionModel(
                accountId: Self.accountId,
                session: Self.session,
                currentUserId: Self.me,
                steerApi: deps.steerApi,
                sessionsApi: SessionsApi(trpc: deps.trpc),
                attachmentsApi: deps.attachmentsApi,
                issuesApi: deps.issuesApi,
                db: deps.db
            )
            preview.loadStyleguide(events: Self.events())
            model = preview
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("session-styleguide-screen")
    }

    // MARK: - Fake activity

    private static func events() -> [[String: Any]] {
        let turnStart = Int(Date().addingTimeInterval(-95).timeIntervalSince1970 * 1000)
        return [
            ["kind": "user_message", "text": "Profile the launch path and find what keeps the first paint above 800 ms."],
            [
                "kind": "tool", "id": "t1", "name": "Bash",
                "detail": "./gradlew :app:startupProfile --variant=release",
            ],
            ["kind": "tool_update", "id": "t1", "status": "completed", "output": "p50 740ms, p90 812ms"],
            [
                "kind": "tool", "id": "t2", "name": "Edit", "toolKind": "edit",
                "detail": "app/src/main/java/com/exponential/app/ui/board/BoardScreen.kt",
            ],
            [
                "kind": "tool_update", "id": "t2", "status": "completed",
                "diff": diff(
                    "app/src/main/java/com/exponential/app/ui/board/BoardScreen.kt",
                    removed: ["    val issues by viewModel.issues.collectAsState(emptyList())"],
                    added: [
                        "    val issues by viewModel.issues.collectAsState(viewModel.cached)",
                        "    // Paint the cached snapshot first; the live one replaces it.",
                    ]
                ),
            ],
            [
                "kind": "narration", "messageId": "m1",
                "text": "The cached snapshot needs to survive a cold process, so the board reducer now writes its last state to the on-disk cache when the app backgrounds. Without it the first launch after a reboot still paints empty.",
            ],
            [
                "kind": "tool", "id": "t3", "name": "Read",
                "detail": "data/sync/BoardSnapshotCache.kt",
            ],
            ["kind": "tool_update", "id": "t3", "status": "completed"],
            [
                "kind": "subagent", "id": "a1", "agentType": "Explore", "status": "started",
                "title": "Find every first-emission consumer",
            ],
            [
                "kind": "subagent", "id": "a1", "agentType": "Explore", "status": "completed",
                "title": "Find every first-emission consumer", "toolCalls": 2,
            ],
            [
                "kind": "narration", "messageId": "m2",
                "text": "One caller assumed the first emission was the full list and skipped its empty state. Guarded it so the board never flashes \"No issues\" between the cached paint and the live snapshot.",
            ],
            [
                "kind": "task_list",
                "entries": [
                    ["content": "Profile the cold start", "status": "completed"],
                    ["content": "Persist the board snapshot", "status": "completed"],
                    ["content": "Guard the first emission", "status": "completed"],
                    ["content": "Measuring on the Galaxy A54", "status": "in_progress"],
                ],
            ],
            ["kind": "turn", "state": "started", "startedAt": turnStart, "tokens": 18_400],
        ]
    }

    /// A one-hunk unified diff for one file.
    private static func diff(_ path: String, removed: [String], added: [String]) -> String {
        let lines = removed.map { "-\($0)" } + added.map { "+\($0)" }
        return """
        diff --git a/\(path) b/\(path)
        --- a/\(path)
        +++ b/\(path)
        @@ -40,\(removed.count) +40,\(added.count) @@
        \(lines.joined(separator: "\n"))
        """
    }
}
