import ExpCore
import ExpUI
import SwiftUI

// N-12: the sheets a waiting thread card opens into (THE Dock, fixtures/
// dock.json + plan-card.json `stepper`; web `ThreadPlanSheet` /
// `ThreadQuestionSheet`). The page is local UI state, never synced.

/// THE plan sheet: page 0 = the plan (markdown, pictures, the surface) and,
/// below `minQuestions` questions, every question under it; else one page
/// per question with "Question n of m" as the header's meta. The footer =
/// Back / Next, the last page THE decision (`PlanCardFooter`).
struct ThreadPlanSheet: View {
    let row: Dock.Row
    let card: ThreadPlanRow
    var canAnswer = true
    var busy = false
    var markdownContext: AgentMarkdownContext?
    /// #EXP-1493: the plan page's draft (its DockCard counts the answers);
    /// nil = the sheet's own.
    var sharedDraft: PlanCardDraft?
    /// #EXP-1493: the header glyph opening THE plan page; nil = none (on the
    /// page itself).
    var onOpenFull: (() -> Void)?
    let onSubmit: (ThreadCardAnswer) -> Void
    let onClose: () -> Void

    @State private var ownDraft = PlanCardDraft()
    @State private var step = 0
    @State private var rejecting = false

    private var draft: PlanCardDraft { sharedDraft ?? ownDraft }

    var body: some View {
        @Bindable var bound = draft
        let decided = card.decision != nil
        let interactive = !decided && !busy && canAnswer
        let stepped = PlanCard.stepperApplies(kind: "plan", decision: card.decision, questionCount: card.questions.count)
        let count = card.questions.count
        let pages = stepped ? 1 + count : 1
        let page = min(step, pages - 1)
        let last = page == pages - 1
        let question = stepped && page > 0 ? card.questions[page - 1] : nil
        let issue = PlanCard.answerIssue(
            questions: card.questions,
            input: PlanCard.AnswerInput(planAnswers: draft.planAnswers(card.questions), decision: "approved")
        )
        DockSheetChrome(
            row: row,
            meta: question != nil ? PlanCard.Copy.stepLabel(step: page, count: count) : nil,
            onOpenFull: onOpenFull,
            onClose: onClose
        ) {
            VStack(alignment: .leading, spacing: PlanCard.Card.blockGap) {
                if let question {
                    questionBlock(question, editable: interactive)
                } else {
                    AgentMarkdownText(
                        text: PlanCard.renderableMarkdown(card.plan),
                        context: markdownContext,
                        options: [.autolinkBareURLs],
                        imageMaxHeight: nil,
                        imageCornerRadius: PlanCard.Card.pictureRadius,
                        imageStroke: PlanCard.Card.pictureStroke
                    )
                    if let surface = card.surface {
                        PlanCardSurfaceView(
                            surface: surface, handle: draft.surface, editable: interactive,
                            data: decided ? card.surfaceData : nil
                        )
                    }
                    if !stepped {
                        ForEach(card.questions) { question in
                            questionBlock(question, editable: interactive)
                        }
                    }
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(PlanCard.TestIds.card)
        } footer: {
            PlanCardFooter(
                onBack: page > 0 ? { step = page - 1 } : nil,
                onNext: last ? nil : { step = page + 1 },
                decision: last && interactive
                    ? PlanCardFooter.Decision(
                        approveIssue: issue.map(PlanCard.Copy.issue),
                        rejecting: $rejecting,
                        feedback: $bound.feedback,
                        onDecide: { decide($0, blocked: issue != nil) },
                        busy: busy
                    )
                    : nil
            )
        }
    }

    private func questionBlock(_ question: ThreadPlanQuestion, editable: Bool) -> some View {
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

    /// Rejecting never needs answers; approving waits on them.
    private func decide(_ option: AskDock.PlanOption, blocked: Bool) {
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
}

/// THE question sheet: the question, the inline surface, the options and
/// the `custom` field in the body; the footer = Answer.
struct ThreadQuestionSheet: View {
    let row: Dock.Row
    let card: ThreadQuestionRow
    var canAnswer = true
    var busy = false
    var markdownContext: AgentMarkdownContext?
    let onSubmit: (ThreadCardAnswer) -> Void
    let onClose: () -> Void

    @State private var draft = PlanCardDraft()

    var body: some View {
        let interactive = canAnswer && !card.answered && !busy
        let issue = PlanCard.answerIssue(
            options: card.options.map(\.label), multi: card.multi, custom: card.custom,
            input: PlanCard.AnswerInput(labels: draft.answer(""))
        )
        DockSheetChrome(row: row, onClose: onClose) {
            VStack(alignment: .leading, spacing: PlanCard.Card.optionsGap) {
                AgentMarkdownText(
                    text: PlanCard.renderableMarkdown(card.question),
                    context: markdownContext,
                    options: [.autolinkBareURLs],
                    imageMaxHeight: nil,
                    imageCornerRadius: PlanCard.Card.pictureRadius,
                    imageStroke: PlanCard.Card.pictureStroke
                )
                if let surface = card.surface {
                    PlanCardSurfaceView(
                        surface: surface, handle: draft.surface, editable: interactive,
                        data: card.answered ? card.surfaceData : nil
                    )
                    .padding(.top, PlanCard.Card.blockGap - PlanCard.Card.optionsGap)
                }
                if !card.options.isEmpty || card.custom {
                    PlanCardQuestionBlock(
                        questionId: "",
                        prompt: nil,
                        caption: card.multi && !card.answered && !card.options.isEmpty ? PlanCard.Copy.chooseAny : nil,
                        multi: card.multi,
                        custom: card.custom,
                        options: card.options,
                        picked: card.answered ? card.picked : draft.answer(""),
                        editable: interactive,
                        customText: Binding(get: { draft.custom[""] ?? "" }, set: { draft.custom[""] = $0 }),
                        onToggle: { draft.toggle("", label: $0, multi: card.multi, order: card.options.map(\.label)) },
                        onType: { draft.type("", text: $0, multi: card.multi) }
                    )
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(PlanCard.TestIds.card)
        } footer: {
            HStack {
                Spacer(minLength: 0)
                GlassPill(
                    PlanCard.Copy.answer,
                    mode: .action { submit() },
                    primary: true,
                    enabled: interactive && issue == nil
                )
                .accessibilityIdentifier("thread-question-answer")
            }
        }
    }

    private func submit() {
        let picked = draft.answer("").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
        guard !picked.isEmpty else { return }
        onSubmit(ThreadCardAnswer(
            messageId: card.id,
            answer: PlanCard.joinAnswers(picked),
            answers: .question(picked),
            surfaceData: card.surface == nil ? nil : draft.surface.snapshot()
        ))
    }
}

extension ThreadRow {
    /// N-12: this waiting card's sheet (a plan's pages or a question), else
    /// nil (any other row).
    @MainActor
    func dockSheet(
        _ row: Dock.Row,
        canAnswer: Bool,
        busy: Bool,
        markdownContext: AgentMarkdownContext?,
        onSubmit: @escaping (ThreadCardAnswer) -> Void,
        onClose: @escaping () -> Void
    ) -> AnyView? {
        switch self {
        case let .plan(card):
            AnyView(ThreadPlanSheet(
                row: row, card: card, canAnswer: canAnswer, busy: busy, markdownContext: markdownContext,
                onSubmit: onSubmit, onClose: onClose
            ))
        case let .question(card):
            AnyView(ThreadQuestionSheet(
                row: row, card: card, canAnswer: canAnswer, busy: busy, markdownContext: markdownContext,
                onSubmit: onSubmit, onClose: onClose
            ))
        default:
            nil
        }
    }
}
