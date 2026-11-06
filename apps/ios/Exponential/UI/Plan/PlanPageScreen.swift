import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1493 → #EXP-1662 THE PLAN TAB's body, phone form (fixtures/plan-page.json;
/// web packages/ui plan-page.tsx, Android ui/plan/PlanPage.kt): under the
/// thread's own header + strip, on its ground, no composer, no page header. ONE
/// scroll: the big title, then the blocks (`PlanPage.blocks`): prose through THE
/// chat markdown renderer, a section = its h2 (the outline's anchor), a picture
/// group = THE carousel. A press on a picture opens THE lightbox (EXP-1586 THE
/// picture viewer, zoomable, `‹ i / n ›`) over every plan picture in document
/// order. The outline toggle (`ui-menu`, a ghost button at the document's top
/// right) opens THE eUI Drawer's phone form. The questions float over the foot
/// (`PlanQuestionsPanel`); the document pads by the panel's height.
struct PlanPageBody: View {
    let card: ThreadPlanRow
    /// The card's `pictures` (their sizes give a carousel its aspect).
    var pictureSizes: [ExpCore.PlanCardPicture] = []
    /// A member of the team decides; anyone else reads.
    var canAnswer = true
    /// The answer in flight (its controls wait).
    var busy = false
    let onSubmit: (ThreadCardAnswer) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var draft = PlanCardDraft()
    @State private var outlineOpen = false
    @State private var sectionTops: [String: CGFloat] = [:]
    @State private var lightbox: PictureViewerSet?
    @State private var panelHeight: CGFloat = 0

    private static let space = "plan-document"

    var body: some View {
        let plan = card.plan
        let outline = PlanPage.outline(plan)
        let hasOutline = !outline.isEmpty || !card.questions.isEmpty
        let padding = CGFloat(PlanPage.Layout.columnPaddingPhone)
        let gap = CGFloat(PlanPage.Panel.gap)
        GeometryReader { geometry in
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: CGFloat(PlanPage.Layout.blockGap)) {
                        ForEach(Array(PlanPage.blocks(plan).enumerated()), id: \.offset) { _, block in
                            blockView(block, plan: plan, hasOutline: hasOutline)
                        }
                        if let surface = card.surface {
                            PlanCardSurfaceView(
                                surface: surface, handle: draft.surface, editable: false,
                                data: card.decision != nil ? card.surfaceData : nil
                            )
                        }
                    }
                    .frame(maxWidth: CGFloat(PlanPage.Layout.columnMaxWidth), alignment: .leading)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, padding)
                    .padding(.top, padding)
                    .padding(.bottom, panelHeight + 2 * gap)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier(PlanPage.TestIds.document)
                }
                .coordinateSpace(name: Self.space)
                .scrollIndicators(.hidden)
                .overlay(alignment: .topTrailing) {
                    if hasOutline {
                        GhostIconButton(
                            AppIcons.uiMenu, accessibilityLabel: PlanPage.Copy.outline,
                            tint: .white.opacity(TextOpacity.secondary)
                        ) { outlineOpen = true }
                        .padding(.trailing, 8)
                        .padding(.top, 8)
                        .accessibilityIdentifier(PlanPage.TestIds.outlineToggle)
                    }
                }
                .overlay(alignment: .bottom) {
                    PlanQuestionsPanel(
                        card: card, canAnswer: canAnswer, busy: busy, draft: draft,
                        maxHeight: geometry.size.height * CGFloat(PlanPage.Panel.maxHeightFraction),
                        onSubmit: onSubmit
                    )
                    .padding(.horizontal, gap)
                    .padding(.bottom, gap)
                    // EXP-1676: the page's ground under the panel, so the
                    // document never shows beneath it.
                    .floatingBarEdge(ground: true)
                    .onGeometryChange(for: CGFloat.self, of: { $0.size.height - gap }) { panelHeight = $0 }
                }
                .sheet(isPresented: $outlineOpen) {
                    GlassSheetChrome {
                        PlanOutlineList(
                            sections: outline,
                            active: activeSection(outline),
                            questions: card.questions.count,
                            onSelect: { id in
                                outlineOpen = false
                                withAnimation { proxy.scrollTo(id, anchor: .top) }
                            }
                        )
                        .padding(.horizontal, 10)
                        .padding(.bottom, 12)
                    }
                }
            }
        }
        .pictureViewer($lightbox)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanPage.TestIds.page)
    }

    // MARK: - The document

    @ViewBuilder
    private func blockView(_ block: PlanPage.Block, plan: String, hasOutline: Bool) -> some View {
        switch block {
        case let .title(text):
            Text(text)
                .font(.system(size: CGFloat(PlanPage.Layout.titleSize), weight: .semibold))
                .foregroundStyle(.white)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.trailing, hasOutline ? 32 : 0)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("plan-title")
        case let .markdown(text):
            AgentMarkdownText(
                text: text,
                context: markdownContext,
                style: .document.autolinked.planCard
            )
            // A picture in the prose (alone on its line or mid-sentence) opens
            // THE plan lightbox too, never a plain image preview.
            .environment(\.markdownImageTapHandler) { src in openLightbox(src: src, plan: plan) }
        case let .section(id, title):
            AgentMarkdownText(text: "## \(title)", context: markdownContext, style: .document)
                .id(id)
                .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.space)).minY } action: { top in
                    sectionTops[id] = top
                }
        case let .pictures(pictures):
            PlanPictureCarousel(
                count: pictures.count,
                aspect: aspect(pictures.first),
                onOpen: { index in openLightbox(at: pictures[index], plan: plan) }
            ) { index in
                PlanPictureImage(src: pictures[index].src)
            }
        }
    }

    /// Scroll-spy: the last section whose heading passed the top.
    private func activeSection(_ outline: [PlanPage.Section]) -> String? {
        outline.last { (sectionTops[$0.id] ?? .infinity) <= 8 }?.id
    }

    private func aspect(_ picture: PlanPage.Picture?) -> CGFloat {
        guard let picture else { return 16 / 10 }
        let id = Self.attachmentId(picture.src)
        if let entry = pictureSizes.first(where: { $0.attachmentId == id }),
           let width = entry.width, let height = entry.height, width > 0, height > 0 {
            return max(0.5, min(2.4, width / height))
        }
        return 16 / 10
    }

    private static func attachmentId(_ src: String) -> String {
        String(src.split(separator: "?").first?.split(separator: "/").last ?? "")
    }

    // MARK: - THE lightbox

    private func openLightbox(at picture: PlanPage.Picture, plan: String) {
        let all = PlanPage.pictures(plan)
        let items = all.map { entry in
            let id = Self.attachmentId(entry.src)
            return PictureViewerItem(id: id.isEmpty ? entry.src : id, src: entry.src, label: entry.alt)
        }
        guard !items.isEmpty else { return }
        lightbox = PictureViewerSet(items: items, start: all.firstIndex(of: picture) ?? 0)
    }

    private func openLightbox(src: String, plan: String) {
        let all = PlanPage.pictures(plan)
        let id = Self.attachmentId(src)
        guard let picture = all.first(where: { $0.src == src }) ?? all.first(where: { Self.attachmentId($0.src) == id })
        else { return }
        openLightbox(at: picture, plan: plan)
    }

    // MARK: - Data

    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient
        )
    }
}

/// #EXP-1662 THE questions, floating (plan-page.json `panel`; web `PlanRail`):
/// plan-card `stepper`: the step label top right (more than one question), one
/// question at a time, Back / Next bottom right, the decision on the last step,
/// the feedback field after the reject pick; no questions = the decision alone;
/// decided or superseded = the closed card (the picks, `planDecisionLabel`, the
/// feedback line). A decision goes through THE card-answer path.
struct PlanQuestionsPanel: View {
    let card: ThreadPlanRow
    let canAnswer: Bool
    let busy: Bool
    let draft: PlanCardDraft
    let maxHeight: CGFloat
    let onSubmit: (ThreadCardAnswer) -> Void

    @State private var step = 0
    @State private var rejecting = false

    private var decided: Bool { card.decision != nil }

    var body: some View {
        @Bindable var bound = draft
        let interactive = !decided && !busy && canAnswer
        let count = card.questions.count
        let page = min(step, max(0, count - 1))
        let question = decided || count == 0 ? nil : card.questions[page]
        let last = page >= count - 1
        let issue = PlanCard.answerIssue(
            questions: card.questions,
            input: PlanCard.AnswerInput(planAnswers: draft.planAnswers(card.questions), decision: "approved")
        )
        let showsFooter = !decided && (page > 0 || !last || interactive || busy)
        if decided || count > 0 || interactive {
            PlanFloatingPanel(maxHeight: maxHeight) {
                if question != nil, count > 1 {
                    Text(PlanCard.Copy.stepLabel(step: page + 1, count: count))
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .padding(.bottom, 12)
                        .accessibilityIdentifier(PlanCard.TestIds.step)
                }
            } content: {
                VStack(alignment: .leading, spacing: PlanCard.Card.blockGap) {
                    if let question {
                        questionBlock(question, editable: interactive)
                    } else if decided {
                        ForEach(card.questions) { question in
                            questionBlock(question, editable: false)
                        }
                        closedLines
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(PlanCard.TestIds.card)
            } footer: {
                if showsFooter {
                    VStack(alignment: .trailing, spacing: PlanCard.Stepper.buttonGap) {
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
                        if busy {
                            Text(PlanCard.Copy.answering)
                                .font(.caption)
                                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                        }
                    }
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(PlanPage.TestIds.rail)
        }
    }

    /// A decided plan's closing lines: the decision label + the feedback.
    @ViewBuilder
    private var closedLines: some View {
        if let decision = card.decision {
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .top, spacing: 6) {
                    AppIcon(decision == "approved" ? AppIcons.uiCheck : AppIcons.uiClose, size: 12)
                        .foregroundStyle(decision == "approved"
                            ? DesignTokens.Semantic.green
                            : DesignTokens.Palette.mutedForeground)
                        .padding(.top, 2)
                    Text(AskDock.planDecisionLabel(decision: decision, fresh: card.fresh))
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                }
                if let feedback = card.feedback, !feedback.isEmpty {
                    Text(PlanCard.Copy.feedbackLine(feedback))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .accessibilityIdentifier(PlanCard.TestIds.feedback)
                }
            }
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
            picked: decided ? (card.answers?[question.id] ?? []) : draft.answer(question.id),
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

/// One plan picture fitted (contain) on black through the attachment loader.
struct PlanPictureImage: View {
    let src: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var image: UIImage?

    var body: some View {
        ZStack {
            Color.black
            if let image {
                Image(uiImage: image).resizable().scaledToFit()
            } else {
                // The muted fill while it loads (and on the specimen).
                DesignTokens.Palette.muted
            }
        }
        .task(id: src) {
            if let tile = SpecimenPicture.image(src) {
                image = tile
                return
            }
            let loader = AttachmentImageLoader(
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                pendingImages: [:]
            )
            image = try? await loader.load(src)
        }
    }
}

/// Styleguide captures only: an attachment id `specimen-<n>` paints a
/// generated tile, one in `samples.json` its bundled sample
/// (`SpecimenSamples`, EXP-1654); the captures run without a server.
enum SpecimenPicture {
    private static let hues: [Double] = [0.62, 0.38, 0.08, 0.55, 0.75, 0.3]

    static func image(_ src: String) -> UIImage? {
        if let sample = SpecimenSamples.image(src) { return sample }
        guard let range = src.range(of: "specimen-") else { return nil }
        let n = Int(src[range.upperBound...].prefix { $0.isNumber }) ?? 0
        let size = CGSize(width: 480, height: 300)
        let hue = hues[n % hues.count]
        return UIGraphicsImageRenderer(size: size).image { context in
            let colors = [
                UIColor(hue: hue, saturation: 0.35, brightness: 0.32, alpha: 1).cgColor,
                UIColor(hue: hue, saturation: 0.25, brightness: 0.16, alpha: 1).cgColor,
            ] as CFArray
            let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors, locations: [0, 1])!
            context.cgContext.drawLinearGradient(gradient, start: .zero, end: CGPoint(x: size.width, y: size.height), options: [])
            let inner = CGRect(x: 60, y: 45, width: 360, height: 210)
            UIColor.white.withAlphaComponent(0.06).setFill()
            UIBezierPath(roundedRect: inner, cornerRadius: 12).fill()
            UIColor.white.withAlphaComponent(0.14).setStroke()
            UIBezierPath(roundedRect: inner, cornerRadius: 12).stroke()
        }
    }
}
