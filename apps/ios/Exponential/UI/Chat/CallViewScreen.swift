import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// #EXP-1943 (session-card.json `callView`, `callViewCases`): THE call view
/// of one call session, a call card's tap. THE chat header (the chat's agent
/// lead, `Call · 4 min` over `Today 09:08 – 09:12 · Assistant`), then the
/// summary under THE section band, then the spoken turns as the chat's
/// bubbles. Model = ExpCore `SessionTree.callView` over the call session's
/// own synced messages.
struct CallViewScreen: View {
    let sessionId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var data: CallViewData?

    var body: some View {
        Group {
            if let data {
                if SessionTree.callRunning(data.session) {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        page(data, now: context.date)
                    }
                } else {
                    page(data, now: Date())
                }
            } else {
                SkeletonRows(rows: 3, rowHeight: 28, dividers: false)
                    .transcriptColumn()
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .workHeaderBand { ChatHeader() }
            }
        }
        .background(AppBackground())
        .toolbar(.hidden, for: .navigationBar)
        .bareBackButton()
        .task(id: sessionId) { await observe() }
    }

    private func page(_ data: CallViewData, now: Date) -> some View {
        let view = SessionTree.callView(
            session: data.session,
            messages: data.messages,
            agentName: data.agentName,
            now: now,
            utcOffsetMinutes: Chats.currentUtcOffsetMinutes(at: now)
        )
        return CallViewBody(
            view: view, messages: data.messages, context: markdownContext(data),
            agentAvatar: .agent(UserAvatarAgent(icon: data.agentIcon, color: data.agentColor))
        )
            .workHeaderBand {
                ChatHeader {
                    ChatHeaderTitle(
                        title: view.title,
                        subtitle: view.subtitle,
                        agentIcon: data.agentIcon,
                        agentColor: data.agentColor,
                        assistant: data.assistant
                    )
                } trailing: {
                    EmptyView()
                }
            }
    }

    private func markdownContext(_ data: CallViewData) -> AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient
        )
    }

    private func observe() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let id = sessionId
        let observation = ValueObservation.tracking { db -> CallViewData? in
            guard let row = try SessionQueries.session(db: db, id: id) else { return nil }
            let root = try SessionQueries.session(db: db, id: row.rootId.isEmpty ? row.id : row.rootId)
            let agent = try root?.agentId.flatMap { try AgentDefinitionEntity.fetchOne(db, key: $0) }
            return CallViewData(
                session: TreeSession(sessionRow: row),
                messages: try SessionQueries.messages(db: db, sessionId: id).map(TreeMessage.init(messageRow:)),
                agentName: agent?.name ?? "",
                agentIcon: agent.map { BoardTypeDisplay.iconName(for: $0.icon) ?? SessionTree.ListCopy.agentDefaultIcon },
                agentColor: agent.map { $0.color ?? SessionTree.ListCopy.agentDefaultColor },
                assistant: agent?.isAssistant == true
            )
        }
        do {
            for try await next in observation.values(in: pool) where next != data {
                data = next
            }
        } catch {}
    }
}

struct CallViewData: Equatable {
    let session: TreeSession
    let messages: [TreeMessage]
    let agentName: String
    let agentIcon: String?
    let agentColor: String?
    let assistant: Bool
}

/// The call view's scroll: the summary band, then the spoken turns (the
/// chat's own bubbles and THE locked pill).
struct CallViewBody: View {
    let view: CallView
    let messages: [TreeMessage]
    let context: AgentMarkdownContext?
    /// The agent's avatar on its group's first bubble (the chat's).
    var agentAvatar: SenderAvatarModel? = nil
    /// A bubble's `HH:mm` by message id (nil = the phone's own clock).
    var timeLabel: ((String) -> String?)? = nil

    private func time(_ id: String) -> String? {
        if let timeLabel { return timeLabel(id) }
        return messages.first { $0.id == id }
            .flatMap { WireTimestamps.parse($0.createdAt) }
            .map(GlassTimeRow.formatted)
    }

    var body: some View {
        ScrollView {
            content
                .padding(.vertical, 8)
                .transcriptColumn()
        }
        .accessibilityIdentifier("call-view")
    }

    /// The summary band + the turns (the specimen draws it unscrolled).
    var content: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let summary = view.summary {
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand(view.summaryLabel)
                    AgentMarkdownText(text: summary, context: context)
                        .padding(.horizontal, 12)
                        .padding(.top, 4)
                        .padding(.bottom, 16)
                    GlassDivider()
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("call-view-summary")
                .padding(.bottom, DesignTokens.Transcript.gapTurn)
            }
            ForEach(Array(view.rows.enumerated()), id: \.element.id) { index, row in
                turn(row, after: index == 0 ? nil : view.rows[index - 1])
            }
        }
    }

    /// The chat's bubble grouping: a turn after the same author sits close.
    private func gap(_ row: SessionChatRow, after previous: SessionChatRow?) -> CGFloat {
        guard let previous else { return 0 }
        return author(previous) == author(row) ? DesignTokens.Transcript.gapTool : DesignTokens.Transcript.gapDefault
    }

    private func author(_ row: SessionChatRow) -> String? {
        switch row {
        case let .message(_, author, _, _, _, _, _): author
        case let .locked(_, _, author): author
        default: nil
        }
    }

    /// #EXP-2076 (chat-express.json WHICH ROWS): an agent turn is the agent's
    /// text: a synced row with Express blocks = its segments in ONE sender
    /// group (row key `m:<messages.id>`, final; the tail on the first
    /// segment, the time on the last prose bubble), the chat's own path.
    /// Read-only like web's call view (no press is forwarded).
    @ViewBuilder
    private func agentTurn(id: String, body: String, first: Bool) -> some View {
        let key = ChatExpress.syncedRowKey(id)
        if let segments = ExpressRowStore.shared.segments(key: key, text: body, final: true) {
            ExpressSegmentsView(rowKey: key, segments: segments, messageId: id, onAction: nil) { prose, firstSegment, last in
                agentBubble(prose, time: last ? time(id) : nil, tail: first && firstSegment)
            }
        } else {
            agentBubble(body, time: time(id), tail: first)
        }
    }

    private func agentBubble(_ text: String, time: String?, tail: Bool) -> some View {
        MessageBubble(side: .other, time: time, tail: tail) {
            AgentMarkdownText(text: text, context: context, hugsWidth: true)
        }
    }

    @ViewBuilder
    private func turn(_ row: SessionChatRow, after previous: SessionChatRow?) -> some View {
        let first = previous.map { author($0) != author(row) } ?? true
        switch row {
        case let .message(id, author, _, body, _, _, _):
            if author == DomainContract.messageAuthorKindUser {
                TranscriptRow(side: .own, gap: gap(row, after: previous), avatar: nil) {
                    UserMessageBubble(text: body, context: context ?? AgentMarkdownContext(baseURL: nil, accountId: "", httpClient: nil), caption: time(id), tail: first)
                }
            } else {
                TranscriptRow(side: .other, gap: gap(row, after: previous), avatar: first ? agentAvatar : nil) {
                    agentTurn(id: id, body: body, first: first)
                }
            }
        case let .locked(_, reason, _):
            TranscriptRow(side: .full, gap: gap(row, after: previous), avatar: nil) {
                TranscriptDividerPill(
                    icon: AppIcons.conceptOrName(SealedPlaceholder.icon) ?? SealedPlaceholder.icon,
                    text: SealedPlaceholder.copy(reason)
                )
            }
        default:
            EmptyView()
        }
    }
}
