import ExpCore
import ExpUI
import SwiftUI

/// EXP-1323/1324: a thread's live tail (web/Android `tail`, desktop's
/// transcript bottom): THE Queue (`fixtures/task-queue.json` — the task list,
/// background work, open waits) and, while the live root works a turn, ONE
/// status line — the agent's mark, the state word and the live caption
/// (`WorkingIndicatorRow`, the session view's own trailing row). Both read
/// the live root's `AgentSessionModel`; nothing once it is over.
struct ThreadLiveTail: View {
    let model: AgentSessionModel
    /// The live root's agent (its brand mark); nil = claude.
    var agent: String?

    var body: some View {
        if !model.isOver {
            VStack(alignment: .leading, spacing: DesignTokens.Transcript.gapBlock) {
                TaskQueueView(
                    todoOpen: Bindable(model).queueTodoOpen,
                    completedOpen: Bindable(model).queueCompletedOpen,
                    build: { model.taskQueue(now: $0) }
                )
                if model.agentWorking {
                    WorkingIndicatorRow(
                        agent: agent,
                        caption: { model.workingCaption(now: $0) }
                    )
                }
            }
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
/// EXP-1323: a child's `report` is ONE event line `Report from <name> ›`
/// opening that session; a `suggestions` card is THE suggested tasks card
/// (`ThreadSuggestionsCard`); the host's live tail (`tail`) closes the rows.
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

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    /// EXP-1347: an issue of this thread's tabs opens its tab.
    @Environment(\.threadIssueOutlet) private var threadIssueOutlet

    /// A card still waiting on its answer (`AskDock.threadPendingAsks`).
    static func isPending(_ row: ThreadRow) -> Bool {
        switch row {
        case let .plan(card): card.decision == nil
        case let .question(card): !card.answered
        default: false
        }
    }

    static let queuedWord = UserMessageCaption.queuedWord

    /// Attachment images through the instance, `#EXP-1` refs chipped against
    /// the root's team — the session view's context.
    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            issueRefs: AgentIssueRefContext(
                teamId: rows.root.teamId,
                db: deps.db,
                onOpen: { [deps, accountId, threadIssueOutlet] issueId in
                    openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
                }
            )
        )
    }

    var body: some View {
        LazyVStack(alignment: .leading, spacing: 14) {
            ForEach(shownRows, id: \.id) { row in
                if rows.isAgent {
                    channelRowView(row)
                } else {
                    rowView(row)
                }
            }
            if let liveTail {
                liveTail
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.conversationTestId)
    }

    /// An agent channel's own card (row 0) stays off it.
    private var shownRows: [ThreadRow] {
        guard rows.isAgent else { return view.rows }
        return view.rows.filter { row in
            if case let .session(_, card) = row { return card.sessionId != rows.root.id }
            return true
        }
    }

    /// An agent channel's row: a message is a comment row, anything else
    /// the thread's own part indented under the rows' lead.
    @ViewBuilder
    private func channelRowView(_ row: ThreadRow) -> some View {
        if case let .message(id, author, authorId, body, queued, _) = row,
           author != DomainContract.messageAuthorKindSystem {
            let fromAgent = author == DomainContract.messageAuthorKindAgent
            let user = fromAgent ? nil : rows.user(id: authorId)
            ChannelMessageRow(
                name: fromAgent ? (rows.root.name ?? "") : (user?.name ?? user?.email ?? ""),
                time: channelTime(messageId: id, queued: queued),
                lead: {
                    if fromAgent {
                        AppIcon(
                            BoardTypeDisplay.iconName(for: rows.root.icon)
                                ?? SessionTree.ListCopy.agentDefaultIcon,
                            size: 16,
                            weight: .medium
                        )
                        .foregroundStyle(
                            Color(hex: rows.root.color ?? SessionTree.ListCopy.agentDefaultColor) ?? .white
                        )
                    } else {
                        UserAvatar(user: user, id: authorId, size: ChannelMessageMetrics.leadSize)
                    }
                },
                content: {
                    AgentMarkdownText(text: body, context: markdownContext, options: [.autolinkBareURLs, .hardLineBreaks])
                }
            )
            .opacity(queued ? 0.7 : 1)
            .accessibilityIdentifier("thread-message")
        } else {
            rowView(row)
                .padding(.leading, ChannelMessageMetrics.leadSize + ChannelMessageMetrics.gap)
        }
    }

    /// A channel row's time: `HH:mm`, then `queued` until the device took it.
    private func channelTime(messageId: String, queued: Bool) -> String {
        UserMessageCaption.text(name: nil, at: createdAt(of: messageId), queued: queued)
    }

    /// The wire `createdAt` of a message row, as a date.
    private func createdAt(of messageId: String) -> Date? {
        rows.messages.first { $0.id == messageId }.flatMap { WireTimestamps.parse($0.createdAt) }
    }

    @ViewBuilder
    private func rowView(_ row: ThreadRow) -> some View {
        switch row {
        case let .message(id, author, authorId, body, queued, _):
            if author == DomainContract.messageAuthorKindSystem {
                ThreadEventLine(text: body)
            } else if author == DomainContract.messageAuthorKindAgent {
                NarrationBubble(text: body, context: markdownContext)
                    .accessibilityIdentifier("thread-message")
            } else {
                UserMessageBubble(
                    text: body,
                    context: markdownContext,
                    caption: caption(messageId: id, authorId: authorId, queued: queued),
                    onRemove: queued && viewerId != nil && authorId == viewerId ? { onUnqueue(id) } : nil
                )
                .opacity(queued ? 0.7 : 1)
                .accessibilityIdentifier("thread-message")
            }
        case let .session(_, card):
            SessionCardView(card: card) {
                if rows.isOtherRoot(card.sessionId) {
                    onOpenThread(card.sessionId)
                } else {
                    onOpenSession(card.sessionId)
                }
            }
        case let .report(_, sessionId, text, _):
            ThreadReportLine(text: text) { onOpenSession(sessionId) }
        case let .suggestions(id, _, header, startAll, items):
            ThreadSuggestionsCard(
                rowId: id,
                header: header,
                startAll: startAll,
                items: items,
                actions: canAnswer ? suggestions : nil,
                onOpenSession: onOpenSession,
                onOpenIssue: { issueId in
                    openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
                }
            )
        case .question, .plan:
            // THE ask dock (fixtures/ask-dock.json): a card still waiting on
            // this reader rides the composer's slot (`ThreadScreen`); here
            // only the answered ones stay, CLOSED (and every card for a
            // read-only reader).
            if !(canAnswer && Self.isPending(row)) {
                ThreadAskCard(
                    row: row,
                    canAnswer: canAnswer,
                    answeringId: answeringId,
                    markdownContext: markdownContext,
                    onSubmit: onSubmit
                )
            }
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
            ThreadEventLine(text: text, issue: rows.issues.first { issue in issue.identifier.map { text.hasSuffix(" \($0)") } ?? false }) { issueId in
                openIssueInnermost(issueId, outlet: threadIssueOutlet, deps: deps, accountId: accountId)
            }
        }
    }

    /// A person's bubble caption: `<name> · HH:mm`, ending `· queued` until
    /// the device took the message (EXP-1317, ×4; the X rides after it) —
    /// web `userMessageCaption`, the same clock the channel rows show.
    private func caption(messageId: String, authorId: String?, queued: Bool) -> String {
        let user = rows.user(id: authorId)
        return UserMessageCaption.text(name: user?.name ?? user?.email, at: createdAt(of: messageId), queued: queued)
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

/// EXP-1323: a child's report — ONE muted event line `Report from <name> ›`
/// opening that session; the summary is never repeated (the parent's next
/// message relays it).
struct ThreadReportLine: View {
    let text: String
    let onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 6) {
                AppIcon(AppIcons.notificationReporterReply, size: 13)
                Text(text)
                    .font(.footnote)
                    .lineLimit(1)
                    .truncationMode(.tail)
                AppIcon(AppIcons.uiChevronRight, size: 12)
            }
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("thread-report")
    }
}

/// EXP-1323: what a suggested tasks card can do (`sessions.startSuggestion`
/// / `startAllSuggestions` / `dismissSuggestion`; Options = the host's THE
/// Menu sheet: Start on… · Start with… · File as issue · Dismiss).
struct ThreadSuggestionActions {
    /// Start the item as a child on the defaults.
    var onStart: (_ rowId: String, _ itemId: String) -> Void
    var onStartAll: (_ rowId: String) -> Void
    /// Do it here: the item goes to the posting session.
    var onHere: (_ rowId: String, _ itemId: String) -> Void
    var onDismiss: (_ rowId: String, _ itemId: String) -> Void
    /// Options: the host opens THE Menu's sheet for the item.
    var onOptions: (_ rowId: String, _ item: SuggestionRow) -> Void
    /// In-flight keys (`key(rowId, itemId)`; Start all = `key(rowId)`).
    var pending: Set<String> = []

    static func key(_ rowId: String, _ itemId: String? = nil) -> String {
        itemId.map { "\(rowId):\($0)" } ?? "\(rowId):*"
    }
}

/// EXP-1323: THE suggested tasks card ×4 in the Work card's shell (web
/// `ThreadSuggestionsCard`): a header (`3 suggested tasks`, `· 2 started`,
/// Start all while more than one is open), then hairline-divided items.
/// Open = title, text, × · and under the text (phones stack them) Options ·
/// Do it here · Start; started = the child's session card (its caption
/// under the name) opening it; here = `Sent here`; filed = `Filed EXP-9`.
struct ThreadSuggestionsCard: View {
    let rowId: String
    let header: String
    let startAll: Bool
    let items: [SuggestionRow]
    var actions: ThreadSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }

    private typealias Copy = SessionTree.SuggestionsCopy
    private var muted: Color { .white.opacity(TextOpacity.tertiary) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiChecklist, size: 13)
                    .foregroundStyle(muted)
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
            ForEach(items) { item in
                GlassDivider()
                itemRow(item)
            }
        }
        .font(.caption)
        .foregroundStyle(.white)
        .background(GlassTokens.fillRow)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-suggestions")
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

    private func openRow(_ item: SuggestionRow) -> some View {
        let busy = actions?.pending.contains(ThreadSuggestionActions.key(rowId, item.id)) ?? false
        return VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
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
                Spacer(minLength: 0)
                if let actions {
                    Button {
                        actions.onDismiss(rowId, item.id)
                    } label: {
                        AppIcon(AppIcons.uiClose, size: 12)
                            .foregroundStyle(muted)
                            .frame(width: 24, height: 24)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(busy)
                    .accessibilityLabel(Copy.dismiss)
                    .accessibilityIdentifier("thread-suggestion-dismiss")
                }
            }
            if let actions {
                // Phones STACK the buttons (web `flex-col`, Android
                // full-width pills): Options · Do it here · Start, each a
                // full-width pill.
                // The pill's capsule hugs its label (the SDK `PillView`), so a
                // bare `.frame(maxWidth:)` only centres a short pill in the
                // row: spacers in the leading/trailing slots make the capsule
                // itself span the row with the label centred, like web's
                // full-width buttons.
                VStack(alignment: .leading, spacing: 6) {
                    GlassPill(
                        Copy.options,
                        mode: .action { actions.onOptions(rowId, item) },
                        enabled: !busy
                    ) {
                        // Both slots nest the same way so the HStack splits
                        // the slack evenly and "Options ˅" sits centred.
                        HStack(spacing: 0) { Spacer(minLength: 0) }
                    } trailing: {
                        HStack(spacing: 4) {
                            AppIcon(AppIcons.uiChevronDown, size: 10)
                            Spacer(minLength: 0)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .accessibilityIdentifier("thread-suggestion-options")
                    GlassPill(Copy.here, mode: .action { actions.onHere(rowId, item.id) }, enabled: !busy) {
                        Spacer(minLength: 0)
                    } trailing: {
                        Spacer(minLength: 0)
                    }
                    .frame(maxWidth: .infinity)
                    .accessibilityIdentifier("thread-suggestion-here")
                    GlassPill(
                        Copy.start,
                        mode: .action { actions.onStart(rowId, item.id) },
                        primary: true,
                        enabled: !busy
                    ) {
                        Spacer(minLength: 0)
                    } trailing: {
                        Spacer(minLength: 0)
                    }
                    .frame(maxWidth: .infinity)
                    .accessibilityIdentifier("thread-suggestion-start")
                }
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

/// A thread's question or plan card — THE session view's `QuestionCard`
/// (EXP-1316; EXP-1334: a CHILD's own ask lands here too, answered by message
/// id straight back to it), drawn inline once answered (closed) and as THE
/// ask dock's big card while it waits (`caption` / `onBack` /
/// `bodyMaxHeight`). A question offers its options AND THE `custom` field
/// (`copy.custom`, the typed text IS the answer) unless the card says
/// `custom: false`, answered with ONE Answer button; a plan offers
/// `AskDock.planOptions` and a decided one
/// names `planDecisionLabel`. EXP-1280 (plan-card.json): the rich card adds
/// the inline surface and the question blocks under the body
/// (`PlanCardForm.swift`); approving waits until every question is answered
/// (`PlanCard.answerIssue`), and the answer carries `answers` +
/// `surfaceData`.
struct ThreadAskCard: View {
    let row: ThreadRow
    var canAnswer = true
    var answeringId: String?
    var markdownContext: AgentMarkdownContext?
    var caption: String?
    var onBack: (() -> Void)?
    var bodyMaxHeight: CGFloat?
    var onSubmit: (ThreadCardAnswer) -> Void = { _ in }

    @State private var draft = PlanCardDraft()

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
            onBack: onBack,
            bodyMaxHeight: bodyMaxHeight,
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
        let issue = decided ? nil : PlanCard.answerIssue(
            questions: card.questions,
            input: PlanCard.AnswerInput(planAnswers: draft.planAnswers(card.questions), decision: "approved")
        )
        QuestionCard(
            question: AgentQuestion(
                id: 0,
                wireId: card.id,
                text: PlanCard.renderableMarkdown(card.plan),
                options: AskDock.planQuestionOptions,
                planMode: true,
                resolved: decided,
                answers: card.decision.map { [AskDock.planDecisionLabel(decision: $0, fresh: card.fresh)] } ?? []
            ),
            active: !decided,
            canAnswer: canAnswer,
            locked: answeringId == card.id,
            pending: answeringId == card.id,
            failed: false,
            onAnswer: { keys, _ in
                let option = AskDock.planOption(key: keys.first)
                let approved = option?.decision == PlanDecision.approved.rawValue
                submitPlan(card, decision: approved ? .approved : .rejected, fresh: approved && option?.fresh == true)
            },
            markdownContext: markdownContext,
            caption: caption,
            onBack: onBack,
            bodyMaxHeight: bodyMaxHeight,
            form: planForm(card),
            approveIssue: issue.map(PlanCard.Copy.issue),
            resolutionNote: card.feedback,
            richBody: true,
            feedback: $draft.feedback
        )
    }

    /// A plan's form: the surface, then one block per question.
    private func planForm(_ card: ThreadPlanRow) -> AnyView? {
        guard card.surface != nil || !card.questions.isEmpty else { return nil }
        let editable = canAnswer && card.decision == nil
        return AnyView(VStack(alignment: .leading, spacing: PlanCard.Card.blockGap) {
            if let surface = card.surface {
                // Decided: the person's stamped `surfaceData` over the
                // agent's initial values (web `data={row.surfaceData}`).
                PlanCardSurfaceView(
                    surface: surface, handle: draft.surface, editable: editable,
                    data: card.decision != nil ? card.surfaceData : nil
                )
            }
            ForEach(card.questions) { question in
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
        })
    }

    private func submitPlan(_ card: ThreadPlanRow, decision: PlanDecision, fresh: Bool) {
        let answers = draft.planAnswers(card.questions)
        let typed = draft.feedback.trimmingCharacters(in: .whitespacesAndNewlines)
        let feedback: String? = typed.isEmpty ? nil : typed
        onSubmit(ThreadCardAnswer(
            messageId: card.id,
            decision: decision,
            fresh: fresh,
            feedback: feedback,
            answers: answers.isEmpty ? nil : .plan(answers),
            surfaceData: card.surface == nil ? nil : draft.surface.snapshot()
        ))
    }
}
