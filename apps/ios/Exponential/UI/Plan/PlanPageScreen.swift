import ExpCore
import ExpUI
import GRDB
import QuickLook
import SwiftUI

/// #EXP-1493 THE PLAN PAGE, phone form (fixtures/plan-page.json; web
/// packages/ui plan-page.tsx, Android ui/plan/PlanPage.kt): THE plan card's
/// expanded form, a pushed page. ONE scroll under the page header (back,
/// `copy.title`, the `ui-menu` outline toggle when the outline has a row):
/// the kind badge + the h1 title + the counts line lead the column, then the
/// blocks (`PlanPage.blocks`): prose through THE chat markdown renderer, a
/// section = its h2 (the outline's anchor), a picture group = THE carousel.
/// A press on a picture opens THE lightbox (Quick Look, `‹ i / n ›`) over
/// every plan picture in document order. The outline opens in a bottom sheet
/// (THE eUI Drawer's phone form). The bottom = THE DockCard (dock.json, kind
/// plan) opening THE Dock sheet (plan-card `stepper`); no composer.
struct PlanPageScreen: View {
    let sessionId: String
    let messageId: String
    /// The styleguide capture's card (no store read); nil = the synced row.
    var specimen: TreeMessageCard?
    /// The capture's chat name (`specimen.chatName`).
    var specimenChatName: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster
    @Environment(\.dismiss) private var dismiss

    @State private var message: SessionMessage?
    /// The asking session's name (the DockCard's caption).
    @State private var sessionName: String?
    @State private var resolved = false
    @State private var draft = PlanCardDraft()
    @State private var dockOpenId: String?
    @State private var outlineOpen = false
    @State private var busy = false
    @State private var sectionTops: [String: CGFloat] = [:]
    @State private var lightbox: URL?
    @State private var lightboxItems: [URL] = []

    private static let space = "plan-document"

    private var card: TreeMessageCard? { specimen ?? TreeMessageCard.parse(message?.card) }

    var body: some View {
        ZStack {
            AppBackground()
            if let card, card.kind == "plan" {
                page(card)
            } else if resolved {
                EmptyState(title: "Not found", description: "It may still be syncing, or it was deleted.",
                           icon: AppIcons.codingPlan)
                    .frame(maxHeight: .infinity)
            } else {
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationTitle(PlanPage.Copy.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            if !PlanPage.outline(card?.plan ?? "").isEmpty {
                ToolbarItem(placement: .topBarTrailing) {
                    GhostIconButton(AppIcons.uiMenu, accessibilityLabel: PlanPage.Copy.outline) { outlineOpen = true }
                        .accessibilityIdentifier(PlanPage.TestIds.outlineToggle)
                }
            }
        }
        .quickLookPreview($lightbox, in: lightboxItems)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanPage.TestIds.page)
        .task(id: messageId) { await observe() }
    }

    // MARK: - The page

    private func page(_ card: TreeMessageCard) -> some View {
        let plan = card.plan ?? ""
        let blocks = Self.leadTrimmed(PlanPage.blocks(plan))
        let outline = PlanPage.outline(plan)
        let counts = PlanPage.countsLine(PlanPage.counts(card))
        let row = planRow(card)
        let padding = CGFloat(PlanPage.Layout.columnPaddingPhone)
        return ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: CGFloat(PlanPage.Layout.blockGap)) {
                    VStack(alignment: .leading, spacing: 8) {
                        StoryKindBadge(kind: .plan, waiting: card.decision == nil)
                        Text(AskDock.title(plan).isEmpty ? PlanPage.Copy.title : AskDock.title(plan))
                            .font(.title2.weight(.semibold))
                            .foregroundStyle(.white)
                            .fixedSize(horizontal: false, vertical: true)
                        if !counts.isEmpty {
                            Text(counts)
                                .font(.footnote)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                    ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                        blockView(block, card: card, plan: plan)
                    }
                }
                .frame(maxWidth: CGFloat(PlanPage.Layout.columnMaxWidth), alignment: .leading)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, padding)
                .padding(.top, padding)
                .padding(.bottom, 120)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(PlanPage.TestIds.document)
            }
            .coordinateSpace(name: Self.space)
            .scrollIndicators(.hidden)
            .safeAreaInset(edge: .bottom, spacing: 0) {
                DockView(
                    rows: [row],
                    openId: $dockOpenId,
                    renderSheet: { row, close in
                        AnyView(ThreadPlanSheet(
                            row: row,
                            card: threadPlanRow(card),
                            canAnswer: true,
                            busy: busy,
                            markdownContext: markdownContext,
                            sharedDraft: draft,
                            questionsOnly: true,
                            onSubmit: { answer in submit(answer, close: close) },
                            onClose: close
                        )
                        .environment(deps)
                        .environment(\.accountId, accountId))
                    }
                ) { EmptyView() }
                .padding(.bottom, 8)
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
                        },
                        onQuestions: {
                            outlineOpen = false
                            dockOpenId = row.id
                        }
                    )
                    .padding(.horizontal, 10)
                    .padding(.bottom, 12)
                }
            }
        }
    }

    @ViewBuilder
    private func blockView(_ block: PlanPage.Block, card: TreeMessageCard, plan: String) -> some View {
        switch block {
        case let .markdown(text):
            AgentMarkdownText(
                text: text,
                context: markdownContext,
                options: [.autolinkBareURLs],
                imageMaxHeight: nil,
                imageCornerRadius: PlanCard.Card.pictureRadius,
                imageStroke: PlanCard.Card.pictureStroke
            )
        case let .section(id, title):
            AgentMarkdownText(text: "## \(title)", context: markdownContext, options: [])
                .id(id)
                .onGeometryChange(for: CGFloat.self) { $0.frame(in: .named(Self.space)).minY } action: { top in
                    sectionTops[id] = top
                }
        case let .pictures(pictures):
            PlanPictureCarousel(
                count: pictures.count,
                aspect: aspect(pictures.first, card: card),
                onOpen: { index in openLightbox(at: pictures[index], plan: plan) }
            ) { index in
                PlanPictureImage(src: pictures[index].src)
            }
        }
    }

    /// The h1 the page already leads with (`AskDock.title`) is not said twice.
    private static func leadTrimmed(_ blocks: [PlanPage.Block]) -> [PlanPage.Block] {
        guard case let .markdown(text)? = blocks.first else { return blocks }
        var lines = text.components(separatedBy: "\n")
        guard let first = lines.first, first.hasPrefix("# ") else { return blocks }
        lines.removeFirst()
        let rest = lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        return (rest.isEmpty ? [] : [.markdown(rest)]) + blocks.dropFirst()
    }

    /// Scroll-spy: the last section whose heading passed the top.
    private func activeSection(_ outline: [PlanPage.Section]) -> String? {
        outline.last { (sectionTops[$0.id] ?? .infinity) <= 8 }?.id
    }

    private func aspect(_ picture: PlanPage.Picture?, card: TreeMessageCard) -> CGFloat {
        guard let picture else { return 16 / 10 }
        let id = Self.attachmentId(picture.src)
        if let entry = card.pictures.first(where: { $0.attachmentId == id }),
           let width = entry.width, let height = entry.height, width > 0, height > 0 {
            return max(0.5, min(2.4, width / height))
        }
        return 16 / 10
    }

    private static func attachmentId(_ src: String) -> String {
        String(src.split(separator: "?").first?.split(separator: "/").last ?? "")
    }

    // MARK: - THE DockCard + Dock sheet

    /// dock.json's plan DockCard UNCHANGED (`Dock.rows` of the plan:
    /// `Plan ready`, the asking session's name, `n questions`, the title).
    private func planRow(_ card: TreeMessageCard) -> Dock.Row {
        Dock.rows([.plan(
            id: messageId, text: card.plan ?? "", sessionName: specimenChatName ?? sessionName,
            questions: card.questions.count, createdAt: 0
        )]).first ?? Dock.Row(
            id: messageId, kind: .plan, tint: .plan, icon: Dock.Icons.plan, label: AskDock.label(.plan),
            caption: nil, meta: nil, title: PlanPage.Copy.title, expandable: true, action: nil
        )
    }

    private func threadPlanRow(_ card: TreeMessageCard) -> ThreadPlanRow {
        ThreadPlanRow(
            id: messageId, sessionId: sessionId, plan: card.plan ?? "", questions: card.questions,
            surface: card.surface, decision: card.decision, fresh: card.fresh, answers: card.planAnswers,
            feedback: card.feedback, surfaceData: card.surfaceData
        )
    }

    /// THE card-answer path; the page stays open (closed once it syncs).
    private func submit(_ answer: ThreadCardAnswer, close: @escaping () -> Void) {
        guard !busy else { return }
        busy = true
        Task {
            do {
                try await deps.sessionsApi.answer(
                    accountId: accountId, messageId: answer.messageId, answer: answer.answer,
                    decision: answer.decision, fresh: answer.fresh, feedback: answer.feedback,
                    answers: answer.answers, surfaceData: answer.surfaceData
                )
                close()
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "answer", thing: "the plan"),
                    description: error.userFacingMessage
                )
            }
            busy = false
        }
    }

    // MARK: - THE lightbox

    private func openLightbox(at picture: PlanPage.Picture, plan: String) {
        let all = PlanPage.pictures(plan)
        Task {
            var urls: [URL] = []
            var selected: URL?
            for entry in all {
                guard let url = await download(entry.src) else { continue }
                urls.append(url)
                if entry == picture, selected == nil { selected = url }
            }
            lightboxItems = urls
            lightbox = selected ?? urls.first
        }
    }

    private func download(_ src: String) async -> URL? {
        let id = Self.attachmentId(src)
        guard !id.isEmpty else { return nil }
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("attachments", isDirectory: true)
            .appendingPathComponent(id, isDirectory: true)
        let destination = directory.appendingPathComponent("\(id).png")
        if FileManager.default.fileExists(atPath: destination.path) { return destination }
        do {
            let data = try await deps.attachmentsApi.download(accountId: accountId, relativeUrl: src)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try data.write(to: destination, options: .atomic)
            return destination
        } catch {
            return nil
        }
    }

    // MARK: - Data

    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient
        )
    }

    private func observe() async {
        guard specimen == nil else { return }
        guard let pool = try? deps.db.pool(forAccountId: accountId) else {
            resolved = true
            return
        }
        let id = messageId
        let session = sessionId
        let observation = ValueObservation.tracking { db in
            (try SessionQueries.messages(db: db, ids: [id]).first,
             try SessionEntity.fetchOne(db, key: session)?.title)
        }
        do {
            for try await (row, title) in observation.values(in: pool) {
                if row != message { message = row }
                if title != sessionName { sessionName = title }
                resolved = true
            }
        } catch {
            resolved = true
        }
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
/// generated tile (the captures run without a server).
enum SpecimenPicture {
    private static let hues: [Double] = [0.62, 0.38, 0.08, 0.55, 0.75, 0.3]

    static func image(_ src: String) -> UIImage? {
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
