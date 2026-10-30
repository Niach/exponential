import ExpCore
import ExpUI
import SwiftUI

/// EXP-1323/1324: a thread's live tail (web/Android `tail`, desktop's
/// transcript bottom): while the live root works a turn, ONE status line —
/// the agent's mark, the state word and the live caption
/// (`WorkingIndicatorRow`, the session view's own trailing row). N-12: THE
/// Queue left for THE Dock (`SessionDock`). Nothing once the root is over.
struct ThreadLiveTail: View {
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
            .accessibilityIdentifier("thread-live-tail")
        }
    }
}

/// EXP-1295: a root's CONVERSATION (fixture `session-thread.json` `rows`) —
/// a thread's, or a team agent's channel: its messages oldest first (EXP-1334:
/// the conversation IS the dispatcher, no root card; a child's own question
/// or plan card surfaces here by time), in the session view's own parts:
/// a person's message is THE message bubble (`UserMessageBubble`, captioned
/// with the name and time, or `queued` until the device took it), the
/// agent's reply bare prose under its mark (`NarrationBubble`); a `session`
/// / `thread_started` card is THE session card (`SessionCardView`): a
/// session of this tree opens its raw view, a thread another root started
/// opens that thread; a question or plan answers IN the session view's
/// `QuestionCard` (`sessions.answer`); a subagent is one muted row; a
/// picture is the Guide's inline tile; an event is one muted line.
/// EXP-1323: a child's `report` is ONE line, S-3: its link header
/// (`SessionXrefHeader`, `Report from <chip> ›`) pushing that session; a
/// turn another session posted carries the same header above its bubble,
/// and the view's `origin` heads the rows; a `suggestions` card is THE suggested tasks card
/// (`ThreadSuggestionsCard`; N-12: while an item is open it rides THE Dock
/// instead); the host's live tail (`tail`) closes the rows.
/// EXP-1300: an AGENT channel (web `AgentChannel`, Android
/// `ThreadCommentRow`) leaves its own root card out and draws EVERY message
/// as a comment row (`ChannelMessageRow`: the agent's icon or the person's
/// avatar, the name, the time, the body); its cards indent under the rows.
/// Presentational: the screen (and the styleguide) hand it the rows; every
/// tap goes back out through a closure.
struct ThreadConversationView: View {
    let rows: ThreadRows
    let view: SessionThreadView
    /// A card's session: open its raw view (`AgentSessionView`).
    var onOpenSession: (String) -> Void = { _ in }
    /// A card naming ANOTHER root: open that thread.
    var onOpenThread: (String) -> Void = { _ in }
    /// S-3: an inter-session link (`SessionXref`): PUSH that session.
    var onOpenXref: (SessionXref) -> Void = { _ in }
    /// A question or plan card's answer (`sessions.answer`).
    var onSubmit: (ThreadCardAnswer) -> Void = { _ in }
    /// EXP-1317: the viewer's own queued message's X (`sessions.unqueue`).
    var onUnqueue: (String) -> Void = { _ in }
    /// The signed-in viewer (whose queued rows carry the X).
    var viewerId: String?
    /// The card answer in flight (its buttons wait).
    var answeringId: String?
    /// Whether this reader may answer the cards (a member of the team).
    var canAnswer = true
    /// EXP-1323: the suggested tasks cards' actions; nil = read-only.
    var suggestions: ThreadSuggestionActions?
    /// EXP-1323/1324: the live tail — THE Queue + the status line off the
    /// live root's relay state — as the Conversation's LAST row (web/Android
    /// `tail` slot, desktop parity); nil while no root is live.
    var liveTail: ThreadLiveTail?
    /// Thread 58cb32df (chats.json SURFACE CARD): an inline preview's reply
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
    @State private var handlers = ThreadRowHandlers()

    /// A card still waiting on its answer (`AskDock.threadPendingAsks`).
    static func isPending(_ row: ThreadRow) -> Bool {
        switch row {
        case let .plan(card): card.decision == nil
        case let .question(card): !card.answered
        default: false
        }
    }

    /// N-12: a suggestions row THE Dock holds (any item still open,
    /// `Dock.suggestionSources`).
    static func isDocked(_ row: ThreadRow) -> Bool {
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
            onOpenSession: onOpenSession, onOpenThread: onOpenThread, onOpenXref: onOpenXref,
            onSubmit: onSubmit, onUnqueue: onUnqueue, onReply: onReply, suggestions: suggestions
        )
        let start = windowStart(all, pinned: pinnedStart)
        LazyVStack(alignment: .leading, spacing: 14) {
            // S-3: who started the viewed session, atop the Conversation.
            if let origin = view.origin {
                SessionXrefHeader(xref: origin, agent: rows.xrefMark(origin)) { onOpenXref(origin) }
            }
            if start > 0 {
                TranscriptLoadEarlierButton {
                    let anchor = all[start].id
                    if pinnedStart == nil { pinnedStart = start }
                    loadedSteps += 1
                    onLoadEarlier(anchor)
                }
            }
            ForEach(all[start...], id: \.id) { row in
                // EXP-1373: phones paint every chat (a thread's and an
                // agent's) as CHAT BUBBLES. #EXP-1380 PERF: each row is an
                // Equatable view over its OWN inputs — a new message, a
                // clock tick or an answer elsewhere re-renders only the
                // rows whose inputs moved.
                rowView(row).equatable()
                    // S-3 CHAT MODE `bubbles`: cards sit in the agent's
                    // column (the agent bubble's ~94%).
                    .padding(.trailing, bubbles && Self.isCard(row) ? ChatBubbleMetrics.agentTrailingGap : 0)
            }
            if let liveTail {
                liveTail
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.conversationTestId)
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
    private func windowStart(_ all: [ThreadRow], pinned: Int?) -> Int {
        Chats.Transcript.windowStart(
            rowCount: all.count,
            waitingIndexes: all.indices.filter { Self.isPending(all[$0]) },
            pinnedStart: pinned,
            loadedSteps: pinned == nil ? 0 : loadedSteps
        )
    }

    private var bubbles: Bool { rows.chatMode == .bubbles }

    /// A card (not a bubble, an event line or a divider pill).
    private static func isCard(_ row: ThreadRow) -> Bool {
        switch row {
        case .message, .event, .divider: false
        default: true
        }
    }

    private static func isSurface(_ row: ThreadRow) -> Bool {
        if case .surface = row { return true }
        return false
    }

    /// A row's `HH:mm` (the agent bubble's time).
    private func clock(_ at: String) -> String? {
        WireTimestamps.parse(at).map { GlassTimeRow.formatted($0) }
    }

    /// `bubbles` (an agent conversation): the root's own card (row 0)
    /// stays off it — the conversation IS that session.
    private var shownRows: [ThreadRow] {
        guard rows.chatMode == .bubbles else { return view.rows }
        return view.rows.filter { row in
            if case let .session(_, card) = row { return card.sessionId != rows.root.id }
            return true
        }
    }

    /// One row with its inputs resolved here (indexed lookups, #EXP-1380).
    private func rowView(_ row: ThreadRow) -> ThreadConversationRow {
        var item = ThreadConversationRow(row: row, teamId: rows.root.teamId, handlers: handlers)
        item.arrived = arrived(row.id) && !Self.isSurface(row)
        item.animateSurface = arrived(row.id)
        item.canAnswer = canAnswer
        item.answeringId = answeringId == row.id ? answeringId : nil
        switch row {
        case let .message(id, author, authorId, _, queued, _, xref):
            item.xrefMark = xref.flatMap(rows.xrefMark)
            let at = rows.createdAt(messageId: id)
            if author == DomainContract.messageAuthorKindAgent {
                item.time = at.flatMap(clock)
            } else if author != DomainContract.messageAuthorKindSystem {
                let user = rows.user(id: authorId)
                item.caption = UserMessageCaption.text(
                    name: user?.name ?? user?.email,
                    at: at.flatMap { WireTimestamps.parse($0) },
                    queued: queued
                )
                item.removable = queued && viewerId != nil && authorId == viewerId
            }
        case let .session(_, card):
            item.otherRoot = rows.isOtherRoot(card.sessionId)
        case let .report(_, _, _, _, xref):
            item.xrefMark = rows.xrefMark(xref)
        case let .suggestions(id, _, _, _, _):
            item.docked = canAnswer && suggestions != nil && Self.isDocked(row)
            item.pending = suggestions?.pending.filter { $0.hasPrefix("\(id):") } ?? []
            item.suggestable = canAnswer && suggestions != nil
        case let .event(_, _, text):
            item.eventIssue = rows.issues.first { issue in issue.identifier.map { text.hasSuffix(" \($0)") } ?? false }
        case let .divider(_, _, at):
            item.time = clock(at)
        default:
            break
        }
        item.replies = canAnswer
        return item
    }
}

/// The conversation rows' actions behind ONE stable reference
/// (`ThreadConversationView.handlers`): the host's closures are swapped in
/// on every pass, the rows read them when tapped. Not observed — swapping
/// a closure never re-renders a row.
@MainActor
final class ThreadRowHandlers {
    private(set) var onOpenSession: (String) -> Void = { _ in }
    private(set) var onOpenThread: (String) -> Void = { _ in }
    private(set) var onOpenXref: (SessionXref) -> Void = { _ in }
    private(set) var onSubmit: (ThreadCardAnswer) -> Void = { _ in }
    private(set) var onUnqueue: (String) -> Void = { _ in }
    private(set) var onReply: (String) -> Void = { _ in }
    private(set) var suggestions: ThreadSuggestionActions?

    init() {}

    func update(
        onOpenSession: @escaping (String) -> Void,
        onOpenThread: @escaping (String) -> Void,
        onOpenXref: @escaping (SessionXref) -> Void,
        onSubmit: @escaping (ThreadCardAnswer) -> Void,
        onUnqueue: @escaping (String) -> Void,
        onReply: @escaping (String) -> Void,
        suggestions: ThreadSuggestionActions?
    ) {
        self.onOpenSession = onOpenSession
        self.onOpenThread = onOpenThread
        self.onOpenXref = onOpenXref
        self.onSubmit = onSubmit
        self.onUnqueue = onUnqueue
        self.onReply = onReply
        self.suggestions = suggestions
    }

    /// A suggested tasks card's actions calling through this reference;
    /// `pending` = the row's own in-flight keys (compared by the row).
    func suggestionActions(pending: Set<String>) -> ThreadSuggestionActions {
        ThreadSuggestionActions(
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
/// ONE `ThreadRowHandlers`, so a row that skipped a re-render still runs
/// the host's current closures. The markdown parse is the run view's
/// cached text path (`AgentMarkdownText`).
struct ThreadConversationRow: View, Equatable {
    let row: ThreadRow
    let teamId: String
    let handlers: ThreadRowHandlers
    var arrived = false
    var animateSurface = false
    var canAnswer = true
    var answeringId: String?
    /// The agent bubble's / divider's `HH:mm`.
    var time: String?
    /// A person's bubble caption (`UserMessageCaption`).
    var caption = ""
    var removable = false
    var otherRoot = false
    var docked = false
    var pending: Set<String> = []
    var eventIssue: IssueEntity?
    /// The suggested tasks card takes actions (a member, actions handed in).
    var suggestable = false
    /// A surface card's reply presses post (a member).
    var replies = false
    /// S-3: the link header's agent mark (nil = the session glyph).
    var xrefMark: SessionXrefHeader.AgentMark?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.threadIssueOutlet) private var threadIssueOutlet

    init(row: ThreadRow, teamId: String, handlers: ThreadRowHandlers) {
        self.row = row
        self.teamId = teamId
        self.handlers = handlers
    }

    private func onOpenSession(_ id: String) { handlers.onOpenSession(id) }
    private func onOpenThread(_ id: String) { handlers.onOpenThread(id) }
    private func onOpenXref(_ xref: SessionXref) { handlers.onOpenXref(xref) }
    private func onUnqueue(_ id: String) { handlers.onUnqueue(id) }
    private func onSubmit(_ answer: ThreadCardAnswer) { handlers.onSubmit(answer) }

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.row == rhs.row && lhs.teamId == rhs.teamId && lhs.arrived == rhs.arrived
            && lhs.animateSurface == rhs.animateSurface && lhs.canAnswer == rhs.canAnswer
            && lhs.answeringId == rhs.answeringId && lhs.time == rhs.time && lhs.caption == rhs.caption
            && lhs.removable == rhs.removable && lhs.otherRoot == rhs.otherRoot && lhs.docked == rhs.docked
            && lhs.pending == rhs.pending && lhs.eventIssue == rhs.eventIssue
            && lhs.suggestable == rhs.suggestable && lhs.replies == rhs.replies && lhs.xrefMark == rhs.xrefMark
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
                onOpen: { [deps, accountId, threadIssueOutlet] issueId in
                    openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
                },
                mentionMembers: ChatMentionMembers.members(teamId: teamId, db: deps.db, accountId: accountId)
            )
        )
    }

    var body: some View {
        content.chatBubbleArrival(arrived)
    }

    /// S-3: a turn ANOTHER session posted carries its link header right
    /// above the bubble.
    @ViewBuilder
    private var content: some View {
        if case let .message(_, _, _, _, _, _, xref?) = row {
            VStack(alignment: .leading, spacing: 4) {
                SessionXrefHeader(xref: xref, agent: xrefMark) { onOpenXref(xref) }
                bubble
            }
        } else {
            bubble
        }
    }

    @ViewBuilder
    private var bubble: some View {
        switch row {
        case let .message(id, author, _, body, queued, _, _):
            if author == DomainContract.messageAuthorKindSystem {
                ThreadEventLine(text: body)
            } else if author == DomainContract.messageAuthorKindAgent {
                // EXP-1373: the agent's text on the left in a card-fill
                // bubble, its time bottom-right.
                AgentChatBubble(time: time) {
                    AgentMarkdownText(text: body, context: markdownContext, options: [.autolinkBareURLs, .hardLineBreaks])
                }
                .accessibilityIdentifier("thread-message")
            } else {
                UserMessageBubble(
                    text: body,
                    context: markdownContext,
                    caption: caption,
                    onRemove: removable ? { onUnqueue(id) } : nil,
                    leadingGap: ChatBubbleMetrics.personLeadingGap
                )
                .opacity(queued ? 0.7 : 1)
                .accessibilityIdentifier("thread-message")
            }
        case let .session(_, card):
            SessionCardView(card: card) {
                if otherRoot {
                    onOpenThread(card.sessionId)
                } else {
                    onOpenSession(card.sessionId)
                }
            }
        case let .report(_, _, _, _, xref):
            // S-3: a report renders AS its link header (`Report from <chip> ›`).
            SessionXrefHeader(xref: xref, agent: xrefMark) { onOpenXref(xref) }
        case let .suggestions(id, _, header, startAll, items):
            // N-12: a row with an open item rides THE Dock (`ThreadScreen`)
            // for a reader who may act on it; it stays here once none is.
            if !docked {
                ThreadSuggestionsCard(
                    rowId: id,
                    header: header,
                    startAll: startAll,
                    items: items,
                    actions: suggestable ? handlers.suggestionActions(pending: pending) : nil,
                    onOpenSession: onOpenSession,
                    onOpenIssue: { issueId in
                        openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
                    }
                )
            }
        case let .plan(card) where card.decision == "superseded":
            // FEED-140: a plan the agent replaced paints CLOSED and
            // collapsed to `copy.superseded`, never in the dock.
            HStack(spacing: 8) {
                AppIcon(AppIcons.codingPlan, size: 13)
                Text(Chats.Copy.superseded)
                    .font(.footnote)
                    .lineLimit(1)
            }
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("thread-plan-superseded")
        case .question, .plan:
            // FEED-140: a WAITING plan or question card is ALWAYS visible
            // inline at its place with its actions (Approve / answer),
            // besides THE dock; answered ones stay CLOSED.
            ThreadAskCard(
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
            .accessibilityIdentifier("thread-subagent-row")
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
        case let .event(_, _, text):
            ThreadEventLine(text: text, issue: eventIssue) { issueId in
                openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
            }
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

}

/// One muted line (an event, a system message); an event naming a linked
/// issue draws that issue as THE issue chip after its verb.
private struct ThreadEventLine: View {
    let text: String
    var issue: IssueEntity?
    var onOpenIssue: (String) -> Void = { _ in }

    var body: some View {
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
        .accessibilityIdentifier("thread-event-line")
    }
}

/// EXP-1323: what a suggested tasks card can do (`sessions.startSuggestion`
/// / `startAllSuggestions` / `dismissSuggestion`; the ⋯ = THE Menu's sheet:
/// Do it here · Start on… · Start with… · File as issue · Dismiss).
struct ThreadSuggestionActions {
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
/// `ThreadSuggestionsCard`): a header (`3 suggested tasks`, `· 2 started`,
/// Start all while more than one is open), then hairline-divided items
/// (`ThreadSuggestionItems`). N-12: while any item is open the row is a
/// DockCard and the items are its sheet (`ThreadSuggestionsSheet`); in the
/// Conversation the card stays for the rows with no open item (and for a
/// read-only reader).
struct ThreadSuggestionsCard: View {
    let rowId: String
    let header: String
    let startAll: Bool
    let items: [SuggestionRow]
    var actions: ThreadSuggestionActions?
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
                    .accessibilityIdentifier("thread-suggestions-header")
                Spacer(minLength: 8)
                if startAll, let actions {
                    GlassPill(
                        Copy.startAll,
                        mode: .action { actions.onStartAll(rowId) },
                        primary: true,
                        enabled: !actions.pending.contains(ThreadSuggestionActions.key(rowId))
                    )
                    .accessibilityIdentifier("thread-suggestions-start-all")
                }
            }
            .padding(.horizontal, 12)
            .frame(minHeight: WorkCards.Geometry.header)
            GlassDivider()
            ThreadSuggestionItems(
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
        .accessibilityIdentifier("thread-suggestions")
    }
}

/// N-12: THE suggestions sheet (the DockCard opened, web
/// `ThreadSuggestionsSheet`): the items as the card lists them; the footer =
/// Start all while more than one is open.
struct ThreadSuggestionsSheet: View {
    let row: Dock.Row
    let rowId: String
    let startAll: Bool
    let items: [SuggestionRow]
    var actions: ThreadSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }
    let onClose: () -> Void

    private typealias Copy = SessionTree.SuggestionsCopy

    var body: some View {
        DockSheetChrome(row: row, onClose: onClose) {
            ThreadSuggestionItems(
                rowId: rowId, items: items, actions: actions,
                onOpenSession: onOpenSession, onOpenIssue: onOpenIssue
            )
            .font(.caption)
            .foregroundStyle(.white)
            // The rows pad themselves (web `px-1` + the rows' `px-3`).
            .padding(.horizontal, -12)
            .accessibilityIdentifier("thread-suggestions-sheet")
        } footer: {
            if startAll, let actions {
                GlassPill(
                    Copy.startAll,
                    mode: .action { actions.onStartAll(rowId) },
                    primary: true,
                    enabled: !actions.pending.contains(ThreadSuggestionActions.key(rowId))
                )
                .accessibilityIdentifier("thread-suggestions-start-all")
            }
        }
    }
}

/// The suggested tasks' items, hairline-divided (web `SuggestionItemRow`):
/// open = the title and text, the ⋯ (THE Menu's sheet) and Start on one row;
/// started = the child's session card (its caption under the name) opening
/// it; here = `Sent here`; filed = `Filed EXP-9`.
struct ThreadSuggestionItems: View {
    let rowId: String
    let items: [SuggestionRow]
    var actions: ThreadSuggestionActions?
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
                    .accessibilityIdentifier("thread-suggestion-started")
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
            .accessibilityIdentifier("thread-suggestion-\(item.state.rawValue)")
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
                .accessibilityIdentifier("thread-suggestion-options-inline")
            }
        }
    }

    private func openRowLine(_ item: SuggestionRow) -> some View {
        let busy = actions?.pending.contains(ThreadSuggestionActions.key(rowId, item.id)) ?? false
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
                    .accessibilityIdentifier("thread-suggestion-options")
                    GlassPill(
                        Copy.start,
                        mode: .action { actions.onStart(rowId, item.id) },
                        primary: true,
                        enabled: !busy
                    )
                    .accessibilityIdentifier("thread-suggestion-start")
                }
                .fixedSize()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-suggestion-open")
    }
}

/// The channel row's lead circle and its gap to the card (a card row
/// indents by both).
enum ChannelMessageMetrics {
    static let leadSize: CGFloat = 32
    static let gap: CGFloat = 12
}

/// EXP-1300: an agent channel's comment row (web `ChannelMessageRow`,
/// Android `ThreadCommentRow`) — the lead (the agent's icon or the person's
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

/// One answer leaving a thread card (`sessions.answer`): a question's
/// `answer` (+ EXP-1280 `answers` = the picked labels), a plan's `decision`
/// (+ `fresh`, `feedback`, `answers` by question id), and the inline
/// surface's data model.
struct ThreadCardAnswer {
    let messageId: String
    var answer: String? = nil
    var decision: PlanDecision? = nil
    var fresh = false
    var feedback: String? = nil
    var answers: CardAnswers? = nil
    var surfaceData: CardJSON? = nil
}

/// A thread's question or plan card IN THE TRANSCRIPT — THE session view's
/// `QuestionCard` (EXP-1316; EXP-1334: a CHILD's own ask lands here too):
/// closed once answered (a decided plan names `planDecisionLabel` and lists
/// every question closed), read-only while it waits on someone else. N-12: a
/// card this reader may answer waits in THE Dock instead and opens as its
/// sheet (`ThreadPlanSheet` / `ThreadQuestionSheet`, ThreadDockSheets.swift).
/// EXP-1280 (plan-card.json): the rich card adds the inline surface and the
/// question blocks under the body (`PlanCardForm.swift`).
struct ThreadAskCard: View {
    let row: ThreadRow
    var canAnswer = true
    var answeringId: String?
    var markdownContext: AgentMarkdownContext?
    var caption: String?
    var onSubmit: (ThreadCardAnswer) -> Void = { _ in }

    @State private var draft = PlanCardDraft()
    /// FEED-140: a waiting plan's inline No-keep-planning field.
    @State private var rejecting = false

    var body: some View {
        switch row {
        case let .question(card):
            questionCard(card)
                .accessibilityIdentifier("thread-question-card")
        case let .plan(card):
            planCard(card)
                .accessibilityIdentifier("thread-plan-card")
        default:
            EmptyView()
        }
    }

    // MARK: - Question

    @ViewBuilder
    private func questionCard(_ card: ThreadQuestionRow) -> some View {
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
            formGap: card.surface != nil ? PlanCard.Card.blockGap : PlanCard.Card.optionsGap
        )
    }

    /// `copy.answer` = a small primary button this far under the options
    /// (web `mt-2`), never full width.
    private static let answerGap: CGFloat = 8

    /// A closed question card's answer reads in its own rows (the picks, a
    /// typed answer as a picked row) — no resolution line then.
    private static func formShowsAnswer(_ card: ThreadQuestionRow) -> Bool {
        let picked = card.picked
        guard !picked.isEmpty else { return false }
        return card.custom || picked.allSatisfy { label in card.options.contains { $0.label == label } }
    }

    /// A question card's form: its surface, the options (rows or tiles) +
    /// the `custom` field, and the Answer button (small, primary, leading).
    private func questionForm(_ card: ThreadQuestionRow) -> AnyView {
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
                .accessibilityIdentifier("thread-question-answer")
            }
        })
    }

    private func submitQuestion(_ card: ThreadQuestionRow, labels: [String]) {
        let picked = labels.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
        guard !picked.isEmpty else { return }
        onSubmit(ThreadCardAnswer(
            messageId: card.id,
            answer: PlanCard.joinAnswers(picked),
            answers: .question(picked),
            surfaceData: card.surface == nil ? nil : draft.surface.snapshot()
        ))
    }

    // MARK: - Plan

    @ViewBuilder
    private func planCard(_ card: ThreadPlanRow) -> some View {
        let decided = card.decision != nil
        let busy = answeringId == card.id
        QuestionCard(
            question: AgentQuestion(
                id: 0,
                wireId: card.id,
                text: PlanCard.renderableMarkdown(card.plan),
                options: [],
                planMode: true,
                resolved: decided,
                answers: card.decision.map { [AskDock.planDecisionLabel(decision: $0, fresh: card.fresh)] } ?? []
            ),
            active: !decided,
            canAnswer: canAnswer,
            locked: busy,
            pending: busy,
            failed: false,
            onAnswer: { _, _ in },
            markdownContext: markdownContext,
            caption: caption,
            form: planForm(card),
            // NEW-6 (N-1): the decision block lives in the form.
            hidesOptions: true,
            resolutionNote: card.feedback,
            richBody: true
        )
    }

    /// A plan's form: the surface, then every question (closed once
    /// decided, inert while it waits on someone else). FEED-140: a plan
    /// waiting on THIS reader is answerable inline too — its questions take
    /// picks and THE decision footer (Yes · No, keep planning) closes it.
    private func planForm(_ card: ThreadPlanRow) -> AnyView? {
        let decided = card.decision != nil
        let interactive = !decided && canAnswer && answeringId != card.id
        guard card.surface != nil || !card.questions.isEmpty || interactive else { return nil }
        let issue = PlanCard.answerIssue(
            questions: card.questions,
            input: PlanCard.AnswerInput(planAnswers: draft.planAnswers(card.questions), decision: "approved")
        )
        return AnyView(VStack(alignment: .leading, spacing: PlanCard.Card.blockGap) {
            if let surface = card.surface {
                // Decided: the person's stamped `surfaceData` over the
                // agent's initial values (web `data={row.surfaceData}`).
                PlanCardSurfaceView(
                    surface: surface, handle: draft.surface, editable: interactive,
                    data: decided ? card.surfaceData : nil
                )
            }
            ForEach(card.questions) { question in
                questionBlock(question, card: card, editable: interactive)
            }
            if interactive {
                PlanCardFooter(
                    onBack: nil,
                    onNext: nil,
                    decision: PlanCardFooter.Decision(
                        approveIssue: issue.map(PlanCard.Copy.issue),
                        rejecting: $rejecting,
                        feedback: $draft.feedback,
                        onDecide: { decide($0, card: card, blocked: issue != nil) },
                        busy: answeringId == card.id
                    )
                )
                .accessibilityIdentifier("thread-plan-inline-decision")
            }
        })
    }

    /// Rejecting never needs answers; approving waits on them.
    private func decide(_ option: AskDock.PlanOption, card: ThreadPlanRow, blocked: Bool) {
        let approved = option.decision == PlanDecision.approved.rawValue
        if approved, blocked { return }
        let answers = draft.planAnswers(card.questions)
        let typed = approved ? "" : draft.feedback.trimmingCharacters(in: .whitespacesAndNewlines)
        onSubmit(ThreadCardAnswer(
            messageId: card.id,
            decision: approved ? .approved : .rejected,
            fresh: approved && option.fresh,
            feedback: typed.isEmpty ? nil : typed,
            answers: answers.isEmpty ? nil : .plan(answers),
            surfaceData: card.surface == nil ? nil : draft.surface.snapshot()
        ))
    }

    private func questionBlock(_ question: ThreadPlanQuestion, card: ThreadPlanRow, editable: Bool) -> some View {
        PlanCardQuestionBlock(
            questionId: question.id,
            prompt: question.prompt,
            caption: question.multi ? PlanCard.Copy.chooseAny : PlanCard.Copy.chooseOne,
            multi: question.multi,
            custom: question.custom,
            options: question.options,
            picked: card.decision != nil ? (card.answers?[question.id] ?? []) : draft.answer(question.id),
            editable: editable,
            customText: Binding(
                get: { draft.custom[question.id] ?? "" },
                set: { draft.custom[question.id] = $0 }
            ),
            onToggle: {
                draft.toggle(question.id, label: $0, multi: question.multi, order: question.options.map(\.label))
            },
            onType: { draft.type(question.id, text: $0, multi: question.multi) }
        )
    }
}

/// Thread 58cb32df (chats.json SURFACE CARD): an agent's inline Exponential
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
        HStack(spacing: 0) {
            SurfaceCardFrame(title: title, animate: animate) {
                PlanCardSurfaceView(
                    surface: surface, handle: handle, editable: onReply != nil,
                    onReply: onReply, bare: true, staggerChildren: animate
                )
            }
            Spacer(minLength: ChatBubbleMetrics.agentTrailingGap)
        }
    }
}
