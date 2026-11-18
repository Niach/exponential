import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1943 (session-card.json `call`; web `CallSpecimen`): on top of the
/// `chat-transcript` specimen, a call in the agent chat = ONE session card
/// (an ended call with its summary, a running one), the rows of
/// session-chat.json's `#EXP-1943` case through `buildSessionChat`, then
/// THE call view of `callViewCases[0]` through `callView`. The inputs are
/// those cases' (the app bundle carries no fixtures; `SessionTreeTests`
/// replays the same cases).
struct CallSpecimens: View {
    @Environment(AppDependencies.self) private var deps

    private var context: AgentMarkdownContext {
        AgentMarkdownContext(baseURL: nil, accountId: "", httpClient: deps.httpClient)
    }

    private static let viewer = UserEntity(
        id: "u-dana", name: "Dana Reyes", email: "dana@example.com", image: nil, createdAt: "", updatedAt: ""
    )

    private static func session(
        _ id: String, parent: String?, status: String, busy: Bool, title: String?, started: String,
        call: TreeVoiceCall? = nil, agentId: String? = nil
    ) -> TreeSession {
        TreeSession(
            id: id, teamId: "t-team-1", parentId: parent, rootId: "r1", userId: "u-dana", status: status,
            busy: busy, needsInput: false, blocked: nil, shared: true, title: title, guide: nil,
            startedAt: started, endedAt: nil, createdAt: started, updatedAt: "2026-11-18T09:12:00.000Z",
            agentId: agentId, voiceCall: call
        )
    }

    private static func message(
        _ id: String, _ session: String, _ kind: String, _ body: String, _ at: String,
        card: [String: Any]? = nil, by: String? = nil, sealed: SealStatus = .plain
    ) -> TreeMessage {
        TreeMessage(
            id: id, sessionId: session, authorKind: kind, authorId: kind == "user" ? "u-dana" : nil, body: body,
            card: card.map(TreeMessageCard.init(object:)), answeredAt: nil, consumedAt: at, createdAt: at,
            authorSessionId: by, sealStatus: sealed
        )
    }

    private static let root = session(
        "r1", parent: nil, status: "running", busy: false, title: nil, started: "2026-11-18T09:00:00.000Z", agentId: "ag1"
    )

    /// session-chat.json `#EXP-1943…` (its input; `expected.rows` = this).
    static let chat: SessionChatView = {
        let ended = session(
            "call1", parent: "r1", status: "idle", busy: false, title: "Call", started: "2026-11-18T09:08:00.000Z",
            call: TreeVoiceCall(
                startedAt: "2026-11-18T09:08:00.000Z", segmentStartedAt: "2026-11-18T09:08:00.000Z",
                endedAt: "2026-11-18T09:12:12.000Z", durationMs: 252_000
            )
        )
        let running = session(
            "call2", parent: "r1", status: "running", busy: true, title: "Call", started: "2026-11-18T09:20:30.000Z",
            call: TreeVoiceCall(
                startedAt: "2026-11-18T09:20:30.000Z", segmentStartedAt: "2026-11-18T09:20:30.000Z",
                endedAt: nil, durationMs: 0
            )
        )
        let summary = "Ship 0.14.140 tomorrow; the Android mic fix goes first."
        let messages = [
            message("c1", "r1", "user", "Can you check what's left for tomorrow's release?", "2026-11-18T09:02:00.000Z"),
            message("c2", "r1", "agent", "Three PRs are green and queued; #EXP-1942 still waits for the Android pictures.", "2026-11-18T09:03:00.000Z"),
            message("c3", "r1", "system", "", "2026-11-18T09:08:00.000Z", card: ["kind": "session", "sessionId": "call1"]),
            message("c4", "r1", "system", "", "2026-11-18T09:08:01.000Z", card: ["kind": "event", "event": "session_claimed", "sessionId": "call1"]),
            message("c5", "r1", "agent", summary, "2026-11-18T09:12:20.000Z", card: ["kind": "report", "sessionId": "call1", "summary": summary, "final": false], by: "call1"),
            message("c6", "r1", "agent", "From the call: I filed #EXP-1950 for the mic fix and started a session on it.", "2026-11-18T09:13:00.000Z"),
            message("c7", "r1", "user", "Thanks. Calling you again about the changelog.", "2026-11-18T09:20:00.000Z"),
            message("c8", "r1", "system", "", "2026-11-18T09:20:30.000Z", card: ["kind": "session", "sessionId": "call2"]),
            message("c9", "r1", "agent", "", "2026-11-18T09:21:00.000Z", card: ["kind": "report", "sessionId": "call2", "summary": "", "final": false], by: "call2", sealed: .noKey),
        ]
        return SessionTree.buildSessionChat(
            root: root, sessions: [root, ended, running], sessionIssues: [], issues: [], messages: messages,
            pullRequests: [], now: WireTimestamps.parse("2026-11-18T09:22:44.000Z") ?? Date()
        )
    }()

    /// session-card.json `callViewCases[0]` (its input).
    static let callMessages: [TreeMessage] = [
        message("v1", "call1", "user", "The person is calling you and the phone is ringing. Answer with ONE short opening line in your own words that fits where the chat stands, in English, then wait for them to speak.", "2026-11-18T09:08:00.000Z"),
        message("v2", "call1", "agent", "Hi Dana, the release is close.", "2026-11-18T09:08:03.000Z"),
        message("v3", "call1", "user", "Hey, where are we with the release?", "2026-11-18T09:08:20.000Z"),
        message("v4", "call1", "agent", "Message from Assistant (session r1): typed in the chat", "2026-11-18T09:08:30.000Z", by: "r1"),
        message("v5", "call1", "agent", "Three PRs are queued. One open thing: the Android microphone drop.", "2026-11-18T09:08:40.000Z"),
        message("v6", "call1", "user", "Okay, that one has to go in first. Can you file it?", "2026-11-18T09:09:10.000Z"),
        message("v7", "call1", "system", "", "2026-11-18T09:09:30.000Z", card: ["kind": "event", "event": "issue_filed", "issueId": "i1"]),
        message("v8", "call1", "agent", "\u{200B}", "2026-11-18T09:09:40.000Z"),
        message("v9", "call1", "user", "The call ended after 04:12. Reply with the call's summary for the main chat, nothing else: what was said and decided, what you did, and what you handed to the chat. Plain sentences, no greeting.", "2026-11-18T09:12:12.000Z"),
        message("v10", "call1", "agent", "Ship 0.14.140 tomorrow at 09:00. The Android mic fix goes first.", "2026-11-18T09:12:20.000Z"),
    ]

    static let call: CallView = SessionTree.callView(
        session: session(
            "call1", parent: "r1", status: "idle", busy: false, title: "Call", started: "2026-11-18T09:08:00.000Z",
            call: TreeVoiceCall(
                startedAt: "2026-11-18T09:08:00.000Z", segmentStartedAt: "2026-11-18T09:08:00.000Z",
                endedAt: "2026-11-18T09:12:12.000Z", durationMs: 252_000
            )
        ),
        messages: callMessages,
        agentName: "Assistant",
        now: WireTimestamps.parse("2026-11-18T10:00:00.000Z") ?? Date(),
        utcOffsetMinutes: 0
    )

    /// The fixtures' wire `HH:mm` (UTC, like the web specimen).
    private static func hhmm(_ id: String) -> String? {
        let all = callMessages + [
            message("c1", "r1", "user", "", "2026-11-18T09:02:00.000Z"),
            message("c2", "r1", "agent", "", "2026-11-18T09:03:00.000Z"),
            message("c6", "r1", "agent", "", "2026-11-18T09:13:00.000Z"),
            message("c7", "r1", "user", "", "2026-11-18T09:20:00.000Z"),
        ]
        return all.first { $0.id == id }.map { String($0.createdAt.dropFirst(11).prefix(5)) }
    }

    private static func author(_ row: SessionChatRow) -> String? {
        if case let .message(_, author, _, _, _, _, _) = row { return author }
        return nil
    }

    @ViewBuilder
    private func chatRow(_ row: SessionChatRow, after previous: SessionChatRow?) -> some View {
        let first = previous.map { Self.author($0) != Self.author(row) } ?? true
        let gap: CGFloat = previous == nil ? 0 : first ? DesignTokens.Transcript.gapDefault : DesignTokens.Transcript.gapTool
        switch row {
        case let .message(id, author, _, body, _, _, _):
            if author == DomainContract.messageAuthorKindUser {
                TranscriptRow(side: .own, gap: gap, avatar: nil) {
                    UserMessageBubble(text: body, context: context, caption: Self.hhmm(id), tail: first)
                }
            } else {
                TranscriptRow(side: .other, gap: gap, avatar: first ? .agent(UserAvatarAgent(icon: nil, color: nil)) : nil) {
                    MessageBubble(side: .other, author: first ? "Assistant" : nil, time: Self.hhmm(id), tail: first) {
                        AgentMarkdownText(text: body, context: context, hugsWidth: true)
                    }
                }
            }
        case let .session(_, card):
            TranscriptRow(side: .full, gap: gap, avatar: nil) {
                SessionCardView(card: card) {}
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        default:
            EmptyView()
        }
    }

    var body: some View {
        let rows = Self.chat.rows
        VStack(alignment: .leading, spacing: 32) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                    chatRow(row, after: index == 0 ? nil : rows[index - 1])
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat-call-specimen")
            VStack(alignment: .leading, spacing: 16) {
                // The call view's header: THE chat header's title block.
                ChatHeaderTitle(
                    title: Self.call.title,
                    subtitle: Self.call.subtitle,
                    assistant: true
                )
                CallViewBody(
                    view: Self.call, messages: Self.callMessages, context: context,
                    agentAvatar: .agent(UserAvatarAgent(icon: nil, color: nil)),
                    timeLabel: { Self.hhmm($0) }
                )
                .content
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("call-view-specimen")
        }
        .padding(.bottom, 32)
    }
}
