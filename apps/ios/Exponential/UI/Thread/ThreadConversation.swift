import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: a root's CONVERSATION (fixture `session-thread.json` `rows`) —
/// a thread's, or a team agent's channel: the root's own session card
/// first, then its messages oldest first, in the session view's own parts:
/// a person's message is THE message bubble (`UserMessageBubble`, captioned
/// with the name and time, or `queued` until the device took it), the
/// agent's reply bare prose under its mark (`NarrationBubble`); a `session`
/// / `thread_started` card is THE session card (`SessionCardView`): a
/// session of this tree opens its raw view, a thread another root started
/// opens that thread; a question or plan answers IN the session view's
/// `QuestionCard` (`sessions.answer`); a subagent is one muted row; a
/// picture is the Guide's inline tile; an event is one muted line.
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
    /// A question card's pick: (message id, the option).
    var onAnswer: (String, String) -> Void = { _, _ in }
    /// A plan card's Approve / Reject: (message id, decision, the reject
    /// row's feedback).
    var onDecide: (String, PlanDecision, String?) -> Void = { _, _, _ in }
    /// The card answer in flight (its buttons wait).
    var answeringId: String?
    /// Whether this reader may answer the cards (a member of the team).
    var canAnswer = true

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    /// The plan card's two rows: the first is the primary approve, `reject`
    /// expands into the inline feedback field (`AgentFeed.planRejectKey`).
    static let planOptions = [
        AgentQuestionOption(label: "Approve", key: approveKey),
        AgentQuestionOption(label: "Reject", key: "reject"),
    ]
    static let approveKey = "approve"
    static let queuedWord = "queued"

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
                onOpen: { [deps, accountId] issueId in
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
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
        if case let .message(id, author, authorId, body, queued) = row,
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
        let at = rows.messages.first { $0.id == messageId }
            .flatMap { WireTimestamps.parse($0.createdAt) }
            .map(GlassTimeRow.formatted)
        return [at, queued ? Self.queuedWord : nil].compactMap { $0 }.joined(separator: " · ")
    }

    @ViewBuilder
    private func rowView(_ row: ThreadRow) -> some View {
        switch row {
        case let .message(id, author, authorId, body, queued):
            if author == DomainContract.messageAuthorKindSystem {
                ThreadEventLine(text: body)
            } else if author == DomainContract.messageAuthorKindAgent {
                NarrationBubble(text: body, context: markdownContext)
                    .accessibilityIdentifier("thread-message")
            } else {
                UserMessageBubble(
                    text: body,
                    context: markdownContext,
                    caption: caption(messageId: id, authorId: authorId, queued: queued)
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
        case let .question(id, question, options, answer, answered):
            QuestionCard(
                question: AgentQuestion(
                    id: 0,
                    wireId: id,
                    text: question,
                    options: options.map { AgentQuestionOption(label: $0, key: $0) },
                    resolved: answered,
                    answers: answer.map { [$0] } ?? []
                ),
                active: !answered,
                canAnswer: canAnswer,
                locked: answeringId == id,
                pending: answeringId == id,
                failed: false,
                onAnswer: { keys, text in
                    if let pick = text ?? keys.first { onAnswer(id, pick) }
                },
                markdownContext: markdownContext
            )
            .accessibilityIdentifier("thread-question-card")
        case let .plan(id, plan, decision):
            QuestionCard(
                question: AgentQuestion(
                    id: 0,
                    wireId: id,
                    text: plan,
                    options: Self.planOptions,
                    planMode: true,
                    resolved: decision != nil,
                    answers: decision.map {
                        [$0 == PlanDecision.approved.rawValue ? "Approved" : "Rejected"]
                    } ?? []
                ),
                active: decision == nil,
                canAnswer: canAnswer,
                locked: answeringId == id,
                pending: answeringId == id,
                failed: false,
                onAnswer: { keys, _ in
                    onDecide(id, keys.first == Self.approveKey ? .approved : .rejected, nil)
                },
                onAnswerThenSend: { _, feedback in onDecide(id, .rejected, feedback) },
                markdownContext: markdownContext
            )
            .accessibilityIdentifier("thread-plan-card")
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
                deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
            }
        }
    }

    /// A person's bubble caption: `<name> · <time>`, or `queued` until the
    /// device took the message.
    private func caption(messageId: String, authorId: String?, queued: Bool) -> String {
        if queued { return Self.queuedWord }
        let user = rows.user(id: authorId)
        let name = user?.name ?? user?.email
        let time = rows.messages.first { $0.id == messageId }.map { RelativeTime.compact(wire: $0.createdAt) }
        return [name, time].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
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
