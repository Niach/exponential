import ExpCore
import ExpUI
import SwiftUI

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
