import ExpCore
import ExpUI
import SwiftUI

/// EXP-1323/1324: a chat's live tail (web/Android `tail`, desktop's
/// transcript bottom): while the live root works a turn, ONE status line —
/// the agent's mark, the state word and the live caption
/// (`WorkingIndicatorRow`, the session view's own trailing row). N-12: THE
/// Queue left for THE Dock (`SessionDock`). Nothing once the root is over.
struct ChatLiveTail: View {
    let model: AgentSessionModel
    /// The live root's agent (its brand mark); nil = claude.
    var agent: String?

    var body: some View {
        if !model.isOver, model.agentWorking {
            WorkingIndicatorRow(
                agent: agent,
                caption: { model.workingCaption(now: $0) }
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat-live-tail")
        }
    }
}

/// EXP-1295: a root's CONVERSATION (fixture `session-chat.json` `rows`) —
/// a chat's, or a team agent's channel: its messages oldest first (EXP-1334:
/// the conversation IS the dispatcher, no root card; a child's own question
/// or plan card surfaces here by time). #EXP-1736 (transcript-grouping.json):
/// ONE transcript for every chat, read like a group chat: every card-less
/// message is THE message bubble (`MessageBubble`), the viewer's own right
/// (`UserMessageBubble`, its time or `queued` inside), anyone else's left
/// under its sender's avatar and name (`TranscriptRow`), a run of one
/// sender's rows grouped (3pt apart, 12pt between groups); a `session`
/// / `thread_started` card is THE session card (`SessionCardView`): a
/// session of this tree opens its raw view, a chat another root started
/// opens that chat; a question answers IN the session view's
/// `QuestionCard` (`sessions.answer`), #EXP-1662 a plan is ONE row opening the
/// plan tab; a subagent is one muted row; a
/// picture is the Guide's inline tile; an event is one muted line.
/// EXP-1323: a child's `report`, a turn another session posted and the
/// prompt the view's `origin` started it with are THAT session's bubbles
/// (its name ` ›`, a press pushes it); a `suggestions` card is THE suggested tasks card
/// (`ChatSuggestionsCard`; N-12: while an item is open it rides THE Dock
/// instead); the host's live tail (`tail`) closes the rows.
/// EXP-1300: an AGENT channel leaves its own root card out.
/// Presentational: the screen (and the styleguide) hand it the rows; every
/// tap goes back out through a closure.
struct ChatConversationView: View {
    let rows: ChatRows
    let view: SessionChatView
    /// #EXP-1464: a row naming a session opens it (inbox-route.json
    /// `sessionTarget`; a session of this chat opens in place).
    var onOpenSession: (String) -> Void = { _ in }
    /// #EXP-1736: a cross-session sender's bubble: PUSH that session.
    var onOpenSender: (String) -> Void = { _ in }
    /// A question or plan card's answer (`sessions.answer`).
    var onSubmit: (ChatCardAnswer) -> Void = { _ in }
    /// EXP-1317: the viewer's own queued message's X (`sessions.unqueue`).
    var onUnqueue: (String) -> Void = { _ in }
    /// The signed-in viewer (whose queued rows carry the X).
    var viewerId: String?
    /// The card answer in flight (its buttons wait).
    var answeringId: String?
    /// Whether this reader may answer the cards (a member of the team).
    var canAnswer = true
    /// #EXP-1562: the card a waiting pick lit (the focus ring).
    var highlightId: String?
    /// EXP-1323: the suggested tasks cards' actions; nil = read-only.
    var suggestions: ChatSuggestionActions?
    /// EXP-1323/1324: the live tail — THE Queue + the status line off the
    /// live root's relay state — as the Conversation's LAST row (web/Android
    /// `tail` slot, desktop parity); nil while no root is live.
    var liveTail: ChatLiveTail?
    /// Session 58cb32df (chats.json SURFACE CARD): an inline preview's reply
    /// press posts its text as the person's next message.
    var onReply: (String) -> Void = { _ in }

    /// #EXP-1380: `Load earlier` widened the window; the host keeps this row
    /// (the previous first one) where the reader saw it.
    var onLoadEarlier: (String) -> Void = { _ in }

    /// The row ids the screen FIRST painted: a row outside it ARRIVED while
    /// the chat was open (its bubble rises in, a surface card plays its
    /// ENTER); history, a reopen or a scroll never animates.
    @State private var painted: Set<String>?
    /// #EXP-1380 WINDOW (chats.json, review of #1218): the start the FIRST
    /// paint fixed (nil until painted) and the `Load earlier` presses since
    /// the open — `Chats.Transcript.windowStart` derives the start from both.
    @State private var pinnedStart: Int?
    @State private var loadedSteps = 0
    /// The rows' actions, ONE reference for the view's life: every body
    /// pass points it at the CURRENT closures, and a row calls through it at
    /// tap time — an Equatable row that skipped a re-render never holds a
    /// closure over stale state (review of #1218).
    @State private var handlers = ChatRowHandlers()
    /// #EXP-1464 (chat-row-action.json): the sessions whose expanded card
    /// the viewer answered here (client-held `answered`).
    @State private var answeredSessions: Set<String> = []

    /// A card still waiting on its answer (`AskDock.chatPendingAsks`).
    static func isPending(_ row: SessionChatRow) -> Bool {
        switch row {
        case let .plan(card): card.decision == nil
        case let .question(card): !card.answered
        default: false
        }
    }

    /// N-12: a suggestions row THE Dock holds (any item still open,
    /// `Dock.suggestionSources`).
    static func isDocked(_ row: SessionChatRow) -> Bool {
        !Dock.suggestionSources([row]).isEmpty
    }

    static let queuedWord = UserMessageCaption.queuedWord

    private func arrived(_ id: String) -> Bool {
        painted.map { !$0.contains(id) } ?? false
    }

    var body: some View {
        // #EXP-1380 WINDOW ×4: the newest `transcript.window` rows, reaching
        // up to the oldest waiting card; the start is PINNED once painted
        // (arrivals append, following or not); `Load earlier` above them
        // moves it up by `windowStep` in one press, the reader's row kept in
        // place (`onLoadEarlier`).
        let all = shownRows
        let _ = handlers.update(
            onOpenSession: onOpenSession, onOpenSender: onOpenSender,
            onSubmit: onSubmit, onUnqueue: onUnqueue, onReply: onReply, suggestions: suggestions,
            onAnswered: { [$answeredSessions] sessionId in $answeredSessions.wrappedValue.insert(sessionId) }
        )
        let start = windowStart(all, pinned: pinnedStart)
        // #EXP-1736: who sent each row and the gap above it, over EVERY row
        // (a window's first row keeps its group's gap).
        let layouts = transcriptLayouts(all, fromStart: start == 0)
        LazyVStack(alignment: .leading, spacing: 0) {
            if start > 0 {
                TranscriptLoadEarlierButton {
                    let anchor = all[start].id
                    if pinnedStart == nil { pinnedStart = start }
                    loadedSteps += 1
                    onLoadEarlier(anchor)
                }
            }
            ForEach(all[start...], id: \.id) { row in
                // EXP-1373: phones paint every chat (a chat's and an
                // agent's) as CHAT BUBBLES. #EXP-1380 PERF: each row is an
                // Equatable view over its OWN inputs — a new message, a
                // clock tick or an answer elsewhere re-renders only the
                // rows whose inputs moved.
                rowView(row, layout: layouts[row.id]).equatable()
                    .waitingHighlight(highlightId == row.id)
            }
            if let liveTail {
                liveTail
                    .padding(.top, TranscriptGrouping.Layout.gapBetween)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ChatCopy.conversationTestId)
        .onAppear {
            if painted == nil { painted = Set(all.map(\.id)) }
            if pinnedStart == nil { pinnedStart = windowStart(all, pinned: nil) }
        }
        // Another root in the same view (an agent chat switched): its own
        // fresh window, as a reopen.
        .onChange(of: rows.root.id) { _, _ in
            painted = Set(all.map(\.id))
            loadedSteps = 0
            pinnedStart = windowStart(all, pinned: nil)
        }
    }

    /// The window's first painted row (`Chats.Transcript.windowStart`): the
    /// waiting plan / question cards' indexes feed it, so none is cut off.
    private func windowStart(_ all: [SessionChatRow], pinned: Int?) -> Int {
        Chats.Transcript.windowStart(
            rowCount: all.count,
            waitingIndexes: all.indices.filter { Self.isPending(all[$0]) },
            pinnedStart: pinned,
            loadedSteps: pinned == nil ? 0 : loadedSteps
        )
    }

    /// #EXP-1736 (transcript-grouping.json `transcriptLayout`): the rows'
    /// senders, groups and gaps by row id. ONE origin rule (data first):
    /// the view's `origin` (S-3, from the synced rows) sends the
    /// conversation's first message while the window holds it (`fromStart`).
    private func transcriptLayouts(
        _ all: [SessionChatRow], fromStart: Bool
    ) -> [String: TranscriptGrouping.RowLayout] {
        let input = rows.transcriptInput(
            items: all.map(ChatRows.transcriptItem),
            viewerId: viewerId,
            // The root's own origin (a focused view's names another session).
            originSessionId: view.focusId == nil ? view.origin?.sessionId : nil,
            fromStart: fromStart
        )
        return Dictionary(
            TranscriptGrouping.transcriptLayout(input).map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        )
    }

    private static func isSurface(_ row: SessionChatRow) -> Bool {
        if case .surface = row { return true }
        return false
    }

    /// A row's `HH:mm` (the agent bubble's time).
    private func clock(_ at: String) -> String? {
        WireTimestamps.parse(at).map { GlassTimeRow.formatted($0) }
    }

    /// `bubbles` (an agent conversation): the root's own card (row 0)
    /// stays off it — the conversation IS that session.
    private var shownRows: [SessionChatRow] {
        guard rows.chatMode == .bubbles else { return view.rows }
        return view.rows.filter { row in
            if case let .session(_, card) = row { return card.sessionId != rows.root.id }
            return true
        }
    }

    /// One row with its inputs resolved here (indexed lookups, #EXP-1380).
    private func rowView(_ row: SessionChatRow, layout: TranscriptGrouping.RowLayout?) -> ChatConversationRow {
        var item = ChatConversationRow(row: row, teamId: rows.root.teamId, handlers: handlers)
        item.layout = layout
        if let layout, layout.avatar, let sender = layout.sender {
            item.avatar = rows.senderAvatar(sender)
        }
        item.arrived = arrived(row.id) && !Self.isSurface(row)
        item.animateSurface = arrived(row.id)
        item.canAnswer = canAnswer
        item.answeringId = answeringId == row.id ? answeringId : nil
        switch row {
        case let .message(id, author, authorId, _, queued, _, _):
            let at = rows.createdAt(messageId: id)
            if author != DomainContract.messageAuthorKindSystem {
                // EXP-1553 (chats.json CHAT MODE ×4): the bubble has the
                // time (+ queued) inside it, bottom-right; #EXP-1736: the
                // sender's name is the group's first bubble's author line.
                item.time = at.flatMap(clock)
                item.caption = UserMessageCaption.text(
                    name: nil,
                    at: at.flatMap { WireTimestamps.parse($0) },
                    queued: queued
                )
                item.removable = queued && viewerId != nil && authorId == viewerId
            }
        case let .session(_, card):
            item.action = action(type: "session", sessionId: card.sessionId)
            if item.action.action == .expand {
                item.waitingCard = waitingCard(card.sessionId)
                item.answeringId = item.waitingCard.flatMap { answeringId == $0.id ? answeringId : nil }
            }
        case let .report(id, _, _, _, _):
            item.time = rows.createdAt(messageId: id).flatMap(clock)
        case let .suggestions(id, _, _, _, _):
            item.docked = canAnswer && suggestions != nil && Self.isDocked(row)
            item.pending = suggestions?.pending.filter { $0.hasPrefix("\(id):") } ?? []
            item.suggestable = canAnswer && suggestions != nil
        case let .event(_, _, text, sessionId):
            item.eventIssue = rows.issues.first { issue in issue.identifier.map { text.hasSuffix(" \($0)") } ?? false }
            item.action = action(type: "event", sessionId: sessionId)
        case let .divider(_, _, at):
            item.time = clock(at)
        default:
            break
        }
        item.replies = canAnswer
        return item
    }

    /// #EXP-1464 (chat-row-action.json): what a row naming `sessionId`
    /// does when tapped.
    private func action(type: String, sessionId: String?) -> ChatRowAction.Result {
        let session = sessionId.flatMap { rows.session(id: $0) }
        let tree = session.map { TreeSession(sessionRow: $0) }
        return ChatRowAction.chatRowAction(ChatRowAction.Input(
            type: type,
            sessionId: sessionId,
            rootId: rows.root.id,
            state: tree.map { SessionTree.sessionState($0) },
            blocked: tree?.blocked != nil,
            inline: sessionId.map { ChatRowAction.waitingCardInline(view.rows, sessionId: $0) } ?? false,
            answered: sessionId.map(answeredSessions.contains) ?? false
        ))
    }

    /// The expanding row's card (`sessionWaitingCard`); nil = not synced.
    private func waitingCard(_ sessionId: String) -> SessionChatRow? {
        let messages = (rows.namedCards + rows.messages).map { TreeMessage(messageRow: $0) }
        guard let id = ChatRowAction.sessionWaitingCard(sessionId: sessionId, messages: messages),
              let message = messages.first(where: { $0.id == id })
        else { return nil }
        return SessionChatRow.askCard(message)
    }
}

/// The conversation rows' actions behind ONE stable reference
/// (`ChatConversationView.handlers`): the host's closures are swapped in
/// on every pass, the rows read them when tapped. Not observed — swapping
/// a closure never re-renders a row.
@MainActor
final class ChatRowHandlers {
    private(set) var onOpenSession: (String) -> Void = { _ in }
    private(set) var onOpenSender: (String) -> Void = { _ in }
    private(set) var onSubmit: (ChatCardAnswer) -> Void = { _ in }
    private(set) var onUnqueue: (String) -> Void = { _ in }
    private(set) var onReply: (String) -> Void = { _ in }
    private(set) var suggestions: ChatSuggestionActions?
    private(set) var onAnswered: (String) -> Void = { _ in }

    init() {}

    func update(
        onOpenSession: @escaping (String) -> Void,
        onOpenSender: @escaping (String) -> Void,
        onSubmit: @escaping (ChatCardAnswer) -> Void,
        onUnqueue: @escaping (String) -> Void,
        onReply: @escaping (String) -> Void,
        suggestions: ChatSuggestionActions?,
        onAnswered: @escaping (String) -> Void
    ) {
        self.onOpenSession = onOpenSession
        self.onAnswered = onAnswered
        self.onOpenSender = onOpenSender
        self.onSubmit = onSubmit
        self.onUnqueue = onUnqueue
        self.onReply = onReply
        self.suggestions = suggestions
    }

    /// A suggested tasks card's actions calling through this reference;
    /// `pending` = the row's own in-flight keys (compared by the row).
    func suggestionActions(pending: Set<String>) -> ChatSuggestionActions {
        ChatSuggestionActions(
            onStart: { [self] in suggestions?.onStart($0, $1) },
            onStartAll: { [self] in suggestions?.onStartAll($0) },
            onMenu: { [self] in suggestions?.onMenu($0, $1, $2) },
            devices: { [self] in await suggestions?.devices() ?? [] },
            pending: pending
        )
    }
}

/// #EXP-1380 PERF ×4: ONE conversation row, rendered once per change of its
/// OWN inputs (`==` compares the row and what the screen resolved for it).
/// Its actions are never captured per row: they call through the view's
/// ONE `ChatRowHandlers`, so a row that skipped a re-render still runs
/// the host's current closures. The markdown parse is the run view's
/// cached text path (`AgentMarkdownText`).
struct ChatConversationRow: View, Equatable {
    let row: SessionChatRow
    let teamId: String
    let handlers: ChatRowHandlers
    var arrived = false
    var animateSurface = false
    var canAnswer = true
    var answeringId: String?
    /// #EXP-1736: the row's sender, group and gap (`transcriptLayout`).
    var layout: TranscriptGrouping.RowLayout?
    /// The sender's avatar on a group's first row (nil = none).
    var avatar: SenderAvatarModel?
    /// A bubble's / divider's `HH:mm`.
    var time: String?
    /// The viewer's own bubble's caption (`UserMessageCaption`).
    var caption = ""
    var removable = false
    /// #EXP-1464: what a tap on the row does (chat-row-action.json).
    var action: ChatRowAction.Result = .none
    /// An expanding row's waiting question / plan card; nil = not synced.
    var waitingCard: SessionChatRow?
    var docked = false
    var pending: Set<String> = []
    var eventIssue: IssueEntity?
    /// The suggested tasks card takes actions (a member, actions handed in).
    var suggestable = false
    /// A surface card's reply presses post (a member).
    var replies = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.chatIssueOutlet) private var chatIssueOutlet
    /// #EXP-1464: an `expand` row turned down into its card.
    @State private var expanded = false

    init(row: SessionChatRow, teamId: String, handlers: ChatRowHandlers) {
        self.row = row
        self.teamId = teamId
        self.handlers = handlers
    }

    private func onOpenSession(_ id: String) { handlers.onOpenSession(id) }
    private func onUnqueue(_ id: String) { handlers.onUnqueue(id) }
    private func onSubmit(_ answer: ChatCardAnswer) { handlers.onSubmit(answer) }

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.row == rhs.row && lhs.teamId == rhs.teamId && lhs.arrived == rhs.arrived
            && lhs.animateSurface == rhs.animateSurface && lhs.canAnswer == rhs.canAnswer
            && lhs.answeringId == rhs.answeringId && lhs.layout == rhs.layout && lhs.avatar == rhs.avatar
            && lhs.time == rhs.time && lhs.caption == rhs.caption
            && lhs.removable == rhs.removable && lhs.action == rhs.action && lhs.waitingCard == rhs.waitingCard
            && lhs.docked == rhs.docked
            && lhs.pending == rhs.pending && lhs.eventIssue == rhs.eventIssue
            && lhs.suggestable == rhs.suggestable && lhs.replies == rhs.replies
            && lhs.handlers === rhs.handlers
    }

    /// Attachment images through the instance, `#EXP-1` refs chipped against
    /// the root's team — the session view's context.
    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            issueRefs: AgentIssueRefContext(
                teamId: teamId,
                db: deps.db,
                onOpen: { [deps, accountId, chatIssueOutlet] issueId in
                    openIssueInnermost(issueId, outlet: chatIssueOutlet, deps: deps, accountId: accountId)
                },
                mentionMembers: ChatMentionMembers.members(teamId: teamId, db: deps.db, accountId: accountId)
            )
        )
    }

    var body: some View {
        content.chatBubbleArrival(arrived)
            // #EXP-1674: a card's `~/` and `#IDENT` links resolve in the chat's team.
            .environment(\.cardTeamId, teamId)
    }

    /// #EXP-1736: the row in its frame: the gap above it, own right, full
    /// across, anyone else's under its sender's avatar column.
    private var content: some View {
        TranscriptRow(side: layout?.side ?? .full, gap: layout?.gap ?? 0, avatar: avatar) {
            bubble
        }
    }

    /// The group's first bubble's name line (a cross-session sender's ends
    /// in ` ›`); nil = none.
    private var author: String? {
        guard let layout, layout.name else { return nil }
        return layout.sender?.displayName
    }

    /// A cross-session sender's press: push that session.
    private var senderPress: (() -> Void)? {
        guard let link = layout?.sender?.link else { return nil }
        return { [handlers] in handlers.onOpenSender(link) }
    }

    /// Anyone else's bubble (a member, an agent, another session).
    private func otherBubble(_ text: String) -> some View {
        MessageBubble(side: .other, author: author, time: time, onPress: senderPress) {
            AgentMarkdownText(text: text, context: markdownContext, hugsWidth: true)
        }
    }

    @ViewBuilder
    private var bubble: some View {
        switch row {
        case let .message(id, author, _, body, queued, _, _):
            if author == DomainContract.messageAuthorKindSystem {
                ChatEventLine(text: body)
            } else if layout?.side == .own {
                UserMessageBubble(
                    text: body,
                    context: markdownContext,
                    caption: caption,
                    onRemove: removable ? { onUnqueue(id) } : nil
                )
                .opacity(queued ? 0.7 : 1)
                .accessibilityIdentifier("chat-message")
            } else {
                otherBubble(body)
                    .opacity(queued ? 0.7 : 1)
                    .accessibilityIdentifier("chat-message")
            }
        case let .session(_, card):
            sessionCard(card)
        case let .report(_, _, text, summary, _):
            // #EXP-1736: a child's report = THAT session's bubble (its name
            // ` ›`; a press pushes it), the summary its text.
            otherBubble(summary.isEmpty ? text : summary)
                .accessibilityIdentifier("chat-report")
        case let .suggestions(id, _, header, startAll, items):
            // N-12: a row with an open item rides THE Dock (`ChatScreen`)
            // for a reader who may act on it; it stays here once none is.
            if !docked {
                ChatSuggestionsCard(
                    rowId: id,
                    header: header,
                    startAll: startAll,
                    items: items,
                    actions: suggestable ? handlers.suggestionActions(pending: pending) : nil,
                    onOpenSession: onOpenSession,
                    onOpenIssue: { issueId in
                        openIssueInnermost(issueId, outlet: chatIssueOutlet, deps: deps, accountId: accountId)
                    }
                )
            }
        case let .plan(card) where card.decision == "superseded":
            // FEED-140: a plan the agent replaced paints CLOSED and
            // collapsed to `copy.superseded`, never in the dock.
            SupersededPlanCard()
        case .question, .plan:
            // FEED-140: a WAITING question card is ALWAYS visible inline at
            // its place with its actions, besides THE dock; answered ones stay
            // CLOSED. #EXP-1662: a plan = ONE row opening the plan tab.
            ChatAskCard(
                row: row,
                canAnswer: canAnswer,
                answeringId: answeringId,
                markdownContext: markdownContext,
                onSubmit: onSubmit
            )
        case let .subagent(_, title, status):
            HStack(spacing: 8) {
                AppIcon(AppIcons.codingSubagent, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text(title)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 8)
                Text(status)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize()
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("chat-subagent-row")
        case let .picture(_, attachmentId, width, height, caption):
            SessionInlinePicture(entry: SessionResultEntry(
                topic: "",
                label: caption ?? "",
                attachmentId: attachmentId,
                width: width.map { Int($0) },
                height: height.map { Int($0) },
                inline: true,
                caption: caption
            ))
        case .event(_, _, let text, _):
            ChatEventLine(
                text: text,
                issue: eventIssue,
                onOpenIssue: { issueId in
                    openIssueInnermost(issueId, outlet: chatIssueOutlet, deps: deps, accountId: accountId)
                },
                onOpen: action.sessionId.map { sessionId in { onOpenSession(sessionId) } }
            )
        case let .divider(_, kind, _):
            // chats.json: `/new` and `/compact` lines = the centred pill.
            TranscriptDividerPill(
                icon: AppIcons.conceptOrName(kind.icon) ?? kind.icon,
                text: [kind.label, time].compactMap { $0 }.joined(separator: " · ")
            )
        case let .locked(_, reason, _):
            // EXP-1459 (sealed-content.json `placeholder`): sealed content
            // this install can't open (yet) = the same pill, the lock, no time.
            TranscriptDividerPill(
                icon: AppIcons.conceptOrName(SealedPlaceholder.icon) ?? SealedPlaceholder.icon,
                text: SealedPlaceholder.copy(reason)
            )
            .accessibilityIdentifier(SealedPlaceholder.testId)
        case let .surface(_, _, title, surface):
            SurfaceCardRow(
                surface: surface,
                title: title,
                animate: animateSurface,
                onReply: replies ? { [handlers] text in handlers.onReply(text) } : nil
            )
        }
    }

    /// THE session card. #EXP-1464 (chat-row-action.json): `open` opens
    /// the session; `expand` turns the card down into THE question / plan
    /// card of that session (answered as a chat card is, `Open chat` on
    /// its label line), collapsed again once answered; `none` = inert.
    @ViewBuilder
    private func sessionCard(_ card: SessionCard) -> some View {
        switch action.action {
        case .expand:
            VStack(alignment: .leading, spacing: 8) {
                SessionCardView(card: card, expanded: expanded) {
                    withAnimation(.easeOut(duration: 0.18)) { expanded.toggle() }
                }
                if expanded {
                    if let waitingCard {
                        ChatAskCard(
                            row: waitingCard,
                            canAnswer: canAnswer,
                            answeringId: answeringId,
                            markdownContext: markdownContext,
                            headerTrailing: AnyView(ChatRowOpenLink { onOpenSession(card.sessionId) }),
                            onSubmit: { [handlers] answer in
                                handlers.onSubmit(answer)
                                handlers.onAnswered(card.sessionId)
                                expanded = false
                            }
                        )
                    } else {
                        SkeletonRows(rows: 1, rowHeight: 20, dot: false, dividers: false)
                    }
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(expanded ? ChatRowAction.TestIds.expanded : "chat-row-collapsed")
        case .open:
            SessionCardView(card: card, kindWord: action.kindWord, tone: action.tone) {
                onOpenSession(card.sessionId)
            }
        case .none:
            SessionCardView(card: card, onOpen: nil)
        }
    }

}

/// FEED-140: a settled card collapsed to ONE quiet muted line: the kind
/// icon and the outcome (`Replaced by a newer plan`).
struct ChatCardLine: View {
    let icon: String
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            AppIcon(icon, size: 13)
            Text(text)
                .font(.footnote)
                .lineLimit(1)
        }
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

/// #EXP-1561 (waiting-cards.json): a DISMISSED card = THE card frame (the
/// kind icon in its tint) at 80 % holding ONE line: `Dismissed` (xs,
/// medium, muted), then the card's title (xs, muted, truncated). No label
/// row, no body, no ×.
struct AskDismissedCard: View {
    let kind: AskDock.Kind
    let title: String

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            AppIcon(kind == .plan ? AppIcons.codingPlan : AppIcons.uiHelp, size: 14)
                .foregroundStyle(WaitingCardTint.color(kind))
            HStack(spacing: 6) {
                Text(WaitingCards.Copy.dismissed)
                    .font(.caption.weight(.medium))
                    .lineLimit(1)
                    .fixedSize()
                if !title.isEmpty {
                    Text(title)
                        .font(.caption)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
            .foregroundStyle(DesignTokens.Palette.mutedForeground)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .glassCard()
        .opacity(0.8)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(WaitingCards.TestIds.dismissed)
    }
}

/// #EXP-1561 (waiting-cards.json DISMISS): THE card's × — a ghost icon-xs
/// button (`ui-close` at 12pt, muted), the LAST item of the card's top row,
/// tucked into the top-right corner (#EXP-1662 `tucked` false: a plan row's,
/// centred on the row).
struct CardDismissButton: View {
    var tucked = true
    let onDismiss: () -> Void

    var body: some View {
        GhostIconButton(
            AppIcons.uiClose, accessibilityLabel: WaitingCards.Copy.dismiss, size: 24, glyphSize: 12,
            tint: DesignTokens.Palette.mutedForeground, action: onDismiss
        )
        .padding(.top, tucked ? -2 : 0)
        .padding(.trailing, tucked ? -4 : 0)
        .accessibilityIdentifier(WaitingCards.TestIds.dismiss)
    }
}

/// #EXP-1464 `copy.openChat`: trailing an expanded card's label line, it
/// opens that card's session.
private struct ChatRowOpenLink: View {
    let onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            Text(ChatRowAction.Copy.openChat)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .fixedSize()
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(ChatRowAction.TestIds.openChat)
    }
}

/// One muted line (an event, a system message); an event naming a linked
/// issue draws that issue as THE issue chip after its verb. #EXP-1464: an
/// event about a session (`onOpen`) is a button carrying the chevron and
/// the pressed wash; otherwise it stays plain.
private struct ChatEventLine: View {
    let text: String
    var issue: IssueEntity?
    var onOpenIssue: (String) -> Void = { _ in }
    var onOpen: (() -> Void)?

    var body: some View {
        if let onOpen {
            Button(action: onOpen) {
                HStack(spacing: 6) {
                    line
                    AppIcon(AppIcons.uiChevronRight, size: 12)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(PressableRowStyle())
            .accessibilityIdentifier(ChatRowAction.TestIds.event)
        } else {
            line
        }
    }

    private var line: some View {
        HStack(spacing: 6) {
            if let issue {
                Text(String(text.dropLast((issue.identifier?.count ?? -1) + 1)))
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize()
                IssueChip(
                    identifier: issue.identifier,
                    title: issue.title,
                    status: IssueStatus.from(issue.status),
                    bodySize: UIFont.preferredFont(forTextStyle: .footnote).pointSize,
                    size: .sm,
                    onTap: { onOpenIssue(issue.id) }
                )
            } else {
                Text(text)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("chat-event-line")
    }
}

/// EXP-1323: what a suggested tasks card can do (`sessions.startSuggestion`
/// / `startAllSuggestions` / `dismissSuggestion`; the ⋯ = THE Menu's sheet:
/// Do it here · Start on… · Start with… · File as issue · Dismiss).
struct ChatSuggestionActions {
    /// Start the item as a child on the defaults.
    var onStart: (_ rowId: String, _ itemId: String) -> Void
    var onStartAll: (_ rowId: String) -> Void
    /// A pick of the item's ⋯ menu (run once its sheet has dismissed).
    var onMenu: (_ rowId: String, _ itemId: String, _ action: SuggestionOptionsSheet.Action) -> Void
    /// Start on…'s rows (the runnable devices), loaded as the menu opens.
    var devices: () async -> [SuggestionOptionsSheet.Device] = { [] }
    /// In-flight keys (`key(rowId, itemId)`; Start all = `key(rowId)`).
    var pending: Set<String> = []

    static func key(_ rowId: String, _ itemId: String? = nil) -> String {
        itemId.map { "\(rowId):\($0)" } ?? "\(rowId):*"
    }
}

/// EXP-1323: THE suggested tasks card ×4 in the Work card's shell (web
/// `ChatSuggestionsCard`): a header (`3 suggested tasks`, `· 2 started`,
/// Start all while more than one is open), then hairline-divided items
/// (`ChatSuggestionItems`). N-12: while any item is open the row is a
/// DockCard and the items are its sheet (`ChatSuggestionsSheet`); in the
/// Conversation the card stays for the rows with no open item (and for a
/// read-only reader).
struct ChatSuggestionsCard: View {
    let rowId: String
    let header: String
    let startAll: Bool
    let items: [SuggestionRow]
    var actions: ChatSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }

    private typealias Copy = SessionTree.SuggestionsCopy

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiChecklist, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text(header)
                    .fontWeight(.medium)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .accessibilityIdentifier("chat-suggestions-header")
                Spacer(minLength: 8)
                if startAll, let actions {
                    GlassPill(
                        Copy.startAll,
                        mode: .action { actions.onStartAll(rowId) },
                        primary: true,
                        enabled: !actions.pending.contains(ChatSuggestionActions.key(rowId))
                    )
                    .accessibilityIdentifier("chat-suggestions-start-all")
                }
            }
            .padding(.horizontal, 12)
            .frame(minHeight: WorkCards.Geometry.header)
            GlassDivider()
            ChatSuggestionItems(
                rowId: rowId, items: items, actions: actions,
                onOpenSession: onOpenSession, onOpenIssue: onOpenIssue
            )
        }
        .font(.caption)
        .foregroundStyle(.white)
        // N-12: ONE gray ×4: THE card composite under the card hairline.
        .background(GlassTokens.opaqueCardFill)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-suggestions")
    }
}

/// N-12: THE suggestions sheet (the DockCard opened, web
/// `ChatSuggestionsSheet`): the items as the card lists them; the footer =
/// Start all while more than one is open.
struct ChatSuggestionsSheet: View {
    let row: Dock.Row
    let rowId: String
    let startAll: Bool
    let items: [SuggestionRow]
    var actions: ChatSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }
    let onClose: () -> Void

    private typealias Copy = SessionTree.SuggestionsCopy

    var body: some View {
        DockSheetChrome(row: row, onClose: onClose) {
            ChatSuggestionItems(
                rowId: rowId, items: items, actions: actions,
                onOpenSession: onOpenSession, onOpenIssue: onOpenIssue
            )
            .font(.caption)
            .foregroundStyle(.white)
            // The rows pad themselves (web `px-1` + the rows' `px-3`).
            .padding(.horizontal, -12)
            .accessibilityIdentifier("chat-suggestions-sheet")
        } footer: {
            if startAll, let actions {
                GlassPill(
                    Copy.startAll,
                    mode: .action { actions.onStartAll(rowId) },
                    primary: true,
                    enabled: !actions.pending.contains(ChatSuggestionActions.key(rowId))
                )
                .accessibilityIdentifier("chat-suggestions-start-all")
            }
        }
    }
}

/// The suggested tasks' items, hairline-divided (web `SuggestionItemRow`):
/// open = the title and text, the ⋯ (THE Menu's sheet) and Start on one row;
/// started = the child's session card (its caption under the name) opening
/// it; here = `Sent here`; filed = `Filed EXP-9`.
struct ChatSuggestionItems: View {
    let rowId: String
    let items: [SuggestionRow]
    var actions: ChatSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }

    /// The item whose ⋯ sheet is up, the devices it lists, its pick.
    @State private var menuItem: SuggestionRow?
    @State private var devices: [SuggestionOptionsSheet.Device]?
    @State private var picked: (itemId: String, action: SuggestionOptionsSheet.Action)?
    /// N-6 + N-14: inside a sheet host (THE Dock sheet) the ⋯ discloses
    /// THE Menu's rows INLINE under its item row — one item's at a time.
    @Environment(\.pickerPresentation) private var hostPresentation
    @State private var inlineItemId: String?

    private typealias Copy = SessionTree.SuggestionsCopy
    private var muted: Color { .white.opacity(TextOpacity.tertiary) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                if index > 0 { GlassDivider() }
                itemRow(item)
            }
        }
        .sheet(item: $menuItem, onDismiss: runPick) { item in
            SuggestionOptionsSheet(title: item.title, devices: devices) { action in
                picked = (item.id, action)
                menuItem = nil
            }
            .task {
                guard let actions else { return }
                devices = await actions.devices()
            }
        }
    }

    private func runPick() {
        guard let picked else { return }
        self.picked = nil
        actions?.onMenu(rowId, picked.itemId, picked.action)
    }

    @ViewBuilder
    private func itemRow(_ item: SuggestionRow) -> some View {
        switch item.state {
        case .started:
            if let card = item.card {
                // The card's own shell stands down inside this one.
                SessionCardView(card: card, bare: true) { onOpenSession(card.sessionId) }
                    .accessibilityIdentifier("chat-suggestion-started")
            } else {
                // Started, the child's row not synced yet.
                HStack(spacing: 8) {
                    InlineSpinner(size: 12)
                    Text(item.title).foregroundStyle(muted).lineLimit(1).truncationMode(.tail)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 12)
                .frame(minHeight: WorkCards.Geometry.row)
            }
        case .here, .filed:
            HStack(spacing: 8) {
                Text(item.title).foregroundStyle(muted).lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 8)
                if item.state == .here {
                    Text(Copy.sentHere).foregroundStyle(muted).fixedSize()
                } else {
                    let label = Copy.filed.replacingOccurrences(of: "{issue}", with: item.identifier ?? "")
                    Button {
                        if let issueId = item.issueId { onOpenIssue(issueId) }
                    } label: {
                        Text(label).foregroundStyle(muted).fixedSize()
                    }
                    .buttonStyle(.plain)
                    .disabled(item.issueId == nil)
                }
            }
            .padding(.horizontal, 12)
            .frame(minHeight: WorkCards.Geometry.row)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("chat-suggestion-\(item.state.rawValue)")
        case .open:
            openRow(item)
        }
    }

    /// One row on every width (web `flex items-start gap-3`): the words,
    /// then the ⋯ (ghost) and Start (primary).
    private func openRow(_ item: SuggestionRow) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            openRowLine(item)
            if hostPresentation == .inline, inlineItemId == item.id {
                SuggestionOptionsRows(devices: devices) { action in
                    inlineItemId = nil
                    actions?.onMenu(rowId, item.id, action)
                }
                .frame(maxHeight: GlassPickerTokens.inlineMaxHeight)
                .padding(.bottom, 6)
                .task {
                    guard let actions else { return }
                    devices = await actions.devices()
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-suggestion-options-inline")
            }
        }
    }

    private func openRowLine(_ item: SuggestionRow) -> some View {
        let busy = actions?.pending.contains(ChatSuggestionActions.key(rowId, item.id)) ?? false
        return HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .fontWeight(.medium)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if !item.text.isEmpty {
                    Text(item.text)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let actions {
                HStack(spacing: 6) {
                    GhostIconButton(
                        AppIcons.uiMore,
                        accessibilityLabel: Copy.options,
                        size: 24,
                        glyphSize: 14,
                        enabled: !busy
                    ) {
                        UIApplication.endEditing()
                        if hostPresentation == .inline {
                            inlineItemId = inlineItemId == item.id ? nil : item.id
                        } else {
                            menuItem = item
                        }
                    }
                    .accessibilityIdentifier("chat-suggestion-options")
                    GlassPill(
                        Copy.start,
                        mode: .action { actions.onStart(rowId, item.id) },
                        primary: true,
                        enabled: !busy
                    )
                    .accessibilityIdentifier("chat-suggestion-start")
                }
                .fixedSize()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-suggestion-open")
    }
}

/// The channel row's lead circle and its gap to the card (a card row
/// indents by both).
enum ChannelMessageMetrics {
    static let leadSize: CGFloat = 32
    static let gap: CGFloat = 12
}

/// EXP-1300: an agent channel's comment row (web `ChannelMessageRow`,
/// Android `ChatCommentRow`) — the lead (the agent's icon or the person's
/// avatar) in a 32pt circle, then the card: the name, the muted time, the
/// body.
struct ChannelMessageRow<Lead: View, Content: View>: View {

    let name: String
    let time: String
    @ViewBuilder let lead: () -> Lead
    @ViewBuilder let content: () -> Content

    var body: some View {
        HStack(alignment: .top, spacing: ChannelMessageMetrics.gap) {
            lead()
                .frame(width: ChannelMessageMetrics.leadSize, height: ChannelMessageMetrics.leadSize)
                .background(GlassTokens.fillRow, in: Circle())
                .clipShape(Circle())
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(name)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if !time.isEmpty {
                        Text(time)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .fixedSize()
                    }
                }
                content()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(GlassTokens.fillRow, in: RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
            .overlay(
                RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                    .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("channel-message")
    }
}

/// One answer leaving a chat card (`sessions.answer`): a question's
/// `answer` (+ EXP-1280 `answers` = the picked labels), a plan's `decision`
/// (+ `fresh`, `feedback`, `answers` by question id), and the inline
/// surface's data model. #EXP-1561: `dismissed` = the card's × (nothing
/// else rides along).
struct ChatCardAnswer {
    let messageId: String
    var dismissed = false
    var answer: String? = nil
    var decision: PlanDecision? = nil
    var fresh = false
    var feedback: String? = nil
    var answers: CardAnswers? = nil
    var surfaceData: CardJSON? = nil
}

/// FEED-140 (EXP-1553, web `PlanCard` parity ×4): a superseded plan = THE
/// plan card CLOSED — the `Plan ready` header (plan glyph, primary) on THE
/// card surface, its body collapsed to the muted `× Replaced by a newer
/// plan` line, the whole card at 80% opacity.
struct SupersededPlanCard: View {
    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.codingPlan, size: 14)
                .foregroundStyle(DesignTokens.Palette.primary)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 4) {
                Text(AskDock.Copy.planReady)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(DesignTokens.Palette.primary)
                    .lineLimit(1)
                HStack(alignment: .top, spacing: 6) {
                    AppIcon(AppIcons.uiClose, size: 12)
                        .padding(.top, 2)
                    Text(Chats.Copy.superseded)
                        .font(.caption)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .glassCard()
        .opacity(0.8)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("chat-plan-superseded")
    }
}

/// A chat's question card IN THE TRANSCRIPT — THE session view's
/// `QuestionCard` (EXP-1316; EXP-1334: a CHILD's own ask lands here too):
/// closed once answered, read-only while it waits on someone else.
/// EXP-1280 (plan-card.json): the rich card adds the inline surface and the
/// question blocks under the body (`PlanCardForm.swift`). #EXP-1662: a plan is
/// never drawn here: ONE row opening the plan tab (`PlanConversationRow`), its
/// superseded or dismissed form one closed line.
struct ChatAskCard: View {
    let row: SessionChatRow
    var canAnswer = true
    var answeringId: String?
    var markdownContext: AgentMarkdownContext?
    var caption: String?
    /// #EXP-1464: trailing the label line (`Open chat`).
    var headerTrailing: AnyView?
    var onSubmit: (ChatCardAnswer) -> Void = { _ in }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.accountId) private var accountId
    @Environment(\.chatPlanOutlet) private var planOutlet
    @State private var draft = PlanCardDraft()

    var body: some View {
        switch row {
        case let .question(card) where card.dismissed == true:
            // #EXP-1561 (waiting-cards.json): THE dismissed line.
            AskDismissedCard(kind: .question, title: AskDock.title(card.question))
        case let .plan(card) where card.decision == "dismissed":
            AskDismissedCard(kind: .plan, title: AskDock.title(card.plan))
        case let .question(card):
            questionCard(card)
                .accessibilityIdentifier("chat-question-card")
        case let .plan(card) where card.decision == "superseded":
            SupersededPlanCard()
        case let .plan(card):
            PlanConversationRow(
                card: card,
                onOpen: { openPlan(card) },
                onDismiss: dismissAction(card.id, waiting: card.decision == nil)
            )
            .accessibilityIdentifier("chat-plan-card")
        default:
            EmptyView()
        }
    }

    /// #EXP-1662: the chat's tab on this plan, from anywhere.
    private func openPlan(_ card: ChatPlanRow) {
        if planOutlet?.open(card.id) == true { return }
        pushRoute(AppRoute.openingPlan(card.sessionId, messageId: card.id, accountId: accountId, deps: deps))
    }

    /// #EXP-1561: a WAITING card's ONE × for a reader who may answer it; a
    /// press dismisses at once.
    private func dismissAction(_ messageId: String, waiting: Bool) -> (() -> Void)? {
        guard waiting, canAnswer, answeringId != messageId else { return nil }
        return { onSubmit(ChatCardAnswer(messageId: messageId, dismissed: true)) }
    }

    /// The same × in a question's header row (its top-right corner).
    private func dismissButton(_ messageId: String, waiting: Bool) -> AnyView? {
        dismissAction(messageId, waiting: waiting).map { action in AnyView(CardDismissButton(onDismiss: action)) }
    }

    /// The header's trailing slot: the host's own (`Open chat`), then the ×.
    private func trailing(_ leading: AnyView?, dismiss: AnyView?) -> AnyView? {
        guard leading != nil || dismiss != nil else { return nil }
        return AnyView(HStack(spacing: 10) {
            if let leading { leading }
            if let dismiss { dismiss }
        })
    }

    // MARK: - Question

    @ViewBuilder
    private func questionCard(_ card: ChatQuestionRow) -> some View {
        // EXP-1280 (×4): the options are THE card's option rows (text) or its
        // tiles (pictures), the `custom` field right under them and ONE
        // Answer button; nothing answers on a row tap.
        let question = AgentQuestion(
            id: 0,
            wireId: card.id,
            text: PlanCard.renderableMarkdown(card.question),
            options: [],
            multiSelect: card.multi,
            resolved: card.answered,
            // The resolution line (drawn only when the form's rows do not
            // already show the answer): a typed answer, else a legacy bare
            // `answer` that names no option (an old joined "A, B") —
            // `closedAnswerLine`.
            answers: card.answered ? [PlanCard.joinAnswers(card.closedAnswerLine)].filter { !$0.isEmpty } : []
        )
        QuestionCard(
            question: question,
            active: !card.answered,
            canAnswer: canAnswer,
            locked: answeringId == card.id,
            pending: answeringId == card.id,
            failed: false,
            onAnswer: { _, _ in },
            markdownContext: markdownContext,
            caption: caption,
            form: questionForm(card),
            hidesOptions: true,
            hidesResolution: Self.formShowsAnswer(card),
            richBody: true,
            formGap: card.surface != nil ? PlanCard.Card.blockGap : PlanCard.Card.optionsGap,
            headerTrailing: trailing(headerTrailing, dismiss: dismissButton(card.id, waiting: !card.answered))
        )
    }

    /// `copy.answer` = a small primary button this far under the options
    /// (web `mt-2`), never full width.
    private static let answerGap: CGFloat = 8

    /// A closed question card's answer reads in its own rows (the picks, a
    /// typed answer as a picked row) — no resolution line then.
    private static func formShowsAnswer(_ card: ChatQuestionRow) -> Bool {
        let picked = card.picked
        guard !picked.isEmpty else { return false }
        return card.custom || picked.allSatisfy { label in card.options.contains { $0.label == label } }
    }

    /// A question card's form: its surface, the options (rows or tiles) +
    /// the `custom` field, and the Answer button (small, primary, leading).
    private func questionForm(_ card: ChatQuestionRow) -> AnyView {
        let editable = canAnswer && !card.answered
        let picked = card.answered ? card.picked : draft.answer("")
        let busy = answeringId == card.id
        return AnyView(VStack(alignment: .leading, spacing: Self.answerGap) {
            if let surface = card.surface {
                // Closed: the person's stamped `surfaceData` over the agent's
                // initial values (web `data={row.surfaceData}`).
                PlanCardSurfaceView(
                    surface: surface, handle: draft.surface, editable: editable,
                    data: card.answered ? card.surfaceData : nil
                )
                .padding(.bottom, PlanCard.Card.blockGap - Self.answerGap)
            }
            if !card.options.isEmpty || card.custom {
                PlanCardQuestionBlock(
                    questionId: "",
                    prompt: nil,
                    caption: card.multi && !card.answered && !card.options.isEmpty ? PlanCard.Copy.chooseAny : nil,
                    multi: card.multi,
                    custom: card.custom,
                    options: card.options,
                    picked: picked,
                    editable: editable && !busy,
                    customText: Binding(get: { draft.custom[""] ?? "" }, set: { draft.custom[""] = $0 }),
                    onToggle: { draft.toggle("", label: $0, multi: card.multi, order: card.options.map(\.label)) },
                    onType: { draft.type("", text: $0, multi: card.multi) }
                )
            }
            if editable, !busy {
                let issue = PlanCard.answerIssue(
                    options: card.options.map(\.label), multi: card.multi, custom: card.custom,
                    input: PlanCard.AnswerInput(labels: draft.answer(""))
                )
                GlassPill(
                    PlanCard.Copy.answer,
                    mode: .action { submitQuestion(card, labels: draft.answer("")) },
                    primary: true,
                    enabled: issue == nil
                )
                .accessibilityIdentifier("chat-question-answer")
            }
        })
    }

    private func submitQuestion(_ card: ChatQuestionRow, labels: [String]) {
        let picked = labels.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
        guard !picked.isEmpty else { return }
        onSubmit(ChatCardAnswer(
            messageId: card.id,
            answer: PlanCard.joinAnswers(picked),
            answers: .question(picked),
            surfaceData: card.surface == nil ? nil : draft.surface.snapshot()
        ))
    }
}

/// #EXP-1662 (plan-page.json): a plan in the Conversation = ONE row, THE
/// session card's box: the plan glyph (amber while waiting, the emerald check
/// once approved, muted otherwise), `planRowLabel` one line, the status word,
/// flex, the chevron; a press opens the chat's plan tab on THIS plan. THE ×
/// (waiting-cards.json) trails it while it waits.
struct PlanConversationRow: View {
    let card: ChatPlanRow
    let onOpen: () -> Void
    var onDismiss: (() -> Void)?

    private var waiting: Bool { card.decision == nil }

    var body: some View {
        HStack(spacing: 0) {
            Button(action: onOpen) {
                HStack(spacing: 8) {
                    AppIcon(card.decision == "approved" ? AppIcons.uiCheck : AppIcons.codingPlan, size: 16)
                        .foregroundStyle(glyphTint)
                    Text(PlanPage.planRowLabel(card.plan))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Text(status)
                        .font(.caption)
                        .foregroundStyle(waiting ? DesignTokens.Semantic.yellow : DesignTokens.Palette.mutedForeground)
                        .lineLimit(1)
                        .fixedSize()
                    Spacer(minLength: 0)
                    AppIcon(AppIcons.uiChevronRight, size: 16)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                }
                .padding(.horizontal, 12)
                .frame(minHeight: CGFloat(PlanPage.Layout.rowHeight))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("plan-row")
            if let onDismiss {
                CardDismissButton(tucked: false, onDismiss: onDismiss)
                    .padding(.trailing, 8)
            }
        }
        .glassCard()
        .accessibilityElement(children: .contain)
    }

    private var glyphTint: Color {
        if waiting { return DesignTokens.Semantic.yellow }
        return card.decision == "approved" ? DesignTokens.Semantic.green : DesignTokens.Palette.mutedForeground
    }

    private var status: String {
        card.decision.map { AskDock.planDecisionLabel(decision: $0, fresh: card.fresh) } ?? PlanPage.Copy.needsYou
    }
}

/// Session 58cb32df (chats.json SURFACE CARD): an agent's inline Exponential
/// UI preview in the agent's column — THE plan card's surface painter in its
/// own rounded glass frame (`SurfaceCardFrame`, which plays the ENTER
/// animation on a live arrival); a `reply` press posts its text.
struct SurfaceCardRow: View {
    let surface: CardSurface
    let title: String?
    let animate: Bool
    var onReply: ((String) -> Void)?

    @State private var handle = PlanCardSurfaceHandle()

    var body: some View {
        SurfaceCardFrame(title: title, animate: animate) {
            PlanCardSurfaceView(
                surface: surface, handle: handle, editable: onReply != nil,
                onReply: onReply, bare: true, staggerChildren: animate
            )
        }
    }
}
