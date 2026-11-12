import ExpCore
import ExpUI
import SwiftUI
import UIKit

// #FEED-191 RESULTS TAB ×iOS (fixtures/results-tab.json; web
// `packages/ui/src/results-tab.tsx` + `components/chat/chat-results`): the
// session screen's Results tab (#EXP-1732). Top: the PR block while the chat has its
// ONE pull request (`guidePullRequest`): the Stack band + THE StackRail when it
// sits in a linear open stack, then ONE Changes row opening THE review page
// (the complete diff, back row `‹ Results`). Then the results INLINE
// (`ResultsTab.results`, off the tree's card rows `ChatRows.resultCards`,
// never the loaded window: no sync of its own). A
// press on a picture opens THE lightbox over that result's pictures at its index.

/// One picture's bytes in a thumbnail's frame, cropped from the top.
struct ResultThumbnailImage: View {
    let picture: ResultsTab.Picture

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var image: UIImage?

    var body: some View {
        // A clear ground: `.task` never fires on an empty view.
        ZStack(alignment: .top) {
            Color.clear
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            }
        }
        .task(id: picture.attachmentId) {
            if let sample = SpecimenPicture.image(picture.attachmentId) {
                image = sample
                return
            }
            let loader = AttachmentImageLoader(
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                pendingImages: [:]
            )
            image = try? await loader.load("/api/attachments/\(picture.attachmentId)")
        }
    }
}

/// THE lightbox's set over one result's pictures, at `index`.
func resultPictureSet(_ result: ResultsTab.Result, start index: Int) -> PictureViewerSet {
    PictureViewerSet(
        items: result.pictures.map { picture in
            PictureViewerItem(
                id: picture.attachmentId,
                src: "/api/attachments/\(picture.attachmentId)",
                label: ResultsTabMetrics.pictureText(picture)
            )
        },
        start: index
    )
}

/// One result's surface, inline: THE surface card, no title over it, presses
/// reply as in the chat.
private struct ResultSurface: View {
    let surface: CardSurface
    let onReply: ((String) -> Void)?

    @State private var handle = PlanCardSurfaceHandle()

    var body: some View {
        SurfaceCardFrame(title: nil, animate: false) {
            PlanCardSurfaceView(
                surface: surface, handle: handle, editable: onReply != nil, onReply: onReply, bare: true
            )
        }
    }
}

/// The chat's Results page: the PR block, the results, THE lightbox, and the
/// review page (the PR's complete diff) in place of them while open.
struct ChatResultsPage: View {
    let rootId: String
    let rows: ChatRows
    /// A member may answer a surface and merge through a rail member.
    let canEdit: Bool
    @Binding var mergeState: WorkMergeState

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster

    /// The ages follow the clock.
    @State private var now = Date()
    @State private var lightbox: PictureViewerSet?
    @State private var prChanges: ChangesViewModel?
    @State private var review = false
    @State private var focusPath: String?
    @State private var fileSheet = false
    @State private var mergeThrough: PullRequestStack.Confirm?

    /// THE fold over the tree's card rows (`ChatRows.resultInputs`, opened
    /// off main in `load`), and each result's card (its surface).
    private var fold: (results: [ResultsTab.Result], cards: [String: TreeMessageCard]) {
        Self.foldResults(rootId: rootId, rows: rows, now: now)
    }

    /// #EXP-1732: the Results tab's results (the header tab's count).
    static func results(rootId: String, rows: ChatRows, now: Date) -> [ResultsTab.Result] {
        foldResults(rootId: rootId, rows: rows, now: now).results
    }

    private static func foldResults(
        rootId: String, rows: ChatRows, now: Date
    ) -> (results: [ResultsTab.Result], cards: [String: TreeMessageCard]) {
        let results = ResultsTab.results(
            now: now.timeIntervalSince1970 * 1000,
            rootId: rootId,
            sessions: rows.sessions.map { ResultsTab.SessionInput(id: $0.id, rootId: $0.rootId, name: $0.title ?? "") },
            messages: rows.resultInputs
        )
        return (results, rows.resultCardsById)
    }

    var body: some View {
        Group {
            if review, let files = prChanges?.loadedFiles {
                reviewPage(files)
            } else {
                let fold = fold
                ScrollView {
                    ResultsTabView(
                        results: fold.results,
                        onOpenPicture: { result, index in lightbox = resultPictureSet(result, start: index) },
                        lead: { lead },
                        thumbnail: { ResultThumbnailImage(picture: $0) },
                        surface: { result in
                            if let surface = fold.cards[result.id]?.surface {
                                ResultSurface(
                                    surface: surface,
                                    onReply: canEdit ? { text in reply(text, to: result) } : nil
                                )
                            }
                        }
                    )
                    .padding(.horizontal, 16)
                    .padding(.top, 24)
                    .padding(.bottom, 40)
                }
                .scrollIndicators(.hidden)
            }
        }
        .onChange(of: rows.guidePullRequest?.id, initial: true) { _, _ in syncPrChanges() }
        .onDisappear { prChanges?.stopObserving() }
        .pictureViewer($lightbox)
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                now = Date()
            }
        }
    }

    // MARK: - The PR block

    @ViewBuilder
    private var lead: some View {
        if rows.guidePullRequest != nil {
            VStack(alignment: .leading, spacing: 12) {
                stackBlock
                if let changes = prChanges?.loadedFiles, !changes.isEmpty {
                    ResultsChangesRow(files: changes) {
                        focusPath = nil
                        review = true
                    }
                } else if prChanges?.loadedFiles == nil, case .loading = prChanges?.load {
                    SkeletonRows(rows: 1, rowHeight: 40)
                }
            }
        }
    }

    /// The `Stack` band over THE `StackRail`, top first, this chat's PR in
    /// the current wash, down to the base branch. None = no linear open stack.
    @ViewBuilder
    private var stackBlock: some View {
        if let view = rows.guideStackView {
            let members = view.rows.map {
                StackRailMember(id: $0.pullRequestId, identifier: $0.identifier, title: $0.title, current: $0.isCurrent)
            }
            let roots = Dictionary(
                view.rows.compactMap { row in row.rootSessionId.map { (row.pullRequestId, $0) } },
                uniquingKeysWith: { first, _ in first }
            )
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(ResultsTab.Tab.Copy.stack) {
                    AppIcon(AppIcons.prStack, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                } trailing: {
                    EmptyView()
                }
                StackRail(
                    members: members,
                    baseBranch: view.baseBranch ?? "default branch",
                    onOpen: { member in
                        // The chat on screen is already open.
                        guard let other = roots[member.id], other != rootId else { return }
                        pushRoute(.chat(accountId: accountId, chatId: other, child: .results))
                    },
                    onMergeThrough: canEdit && !mergeState.merging
                        ? { member in mergeThrough = rows.mergeThroughConfirm(pullRequestId: member.id) }
                        : nil
                )
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(ResultsTab.Tab.TestIds.stack)
            // THE stack confirm, on a zero-size node of its own (EXP-240).
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(item: $mergeThrough) { confirm in
                        GlassAlert(
                            title: confirm.title,
                            message: confirm.body,
                            actions: [
                                GlassAlertAction(
                                    PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel"
                                ) {},
                                GlassAlertAction(confirm.title, role: .primary, id: "merge-through") {
                                    merge(through: confirm)
                                },
                            ]
                        )
                    }
            }
        }
    }

    /// Merge through a rail member: `pullRequests.merge({stack: true})`;
    /// shares the header pill's in-flight state. EXP-1391: every refusal
    /// offers Retry.
    private func merge(through confirm: PullRequestStack.Confirm) {
        mergeState.merging = true
        let api = deps.pullRequestsApi
        let accountId = accountId
        let title = FeedbackStates.Errors.title(verb: "merge", thing: "the stack")
        Task {
            do {
                let result = try await api.merge(accountId: accountId, pullRequestId: confirm.throughId, stack: true)
                if !result.merged, result.queued != true {
                    toaster.error(title, description: result.error)
                }
            } catch {
                toaster.error(
                    title,
                    description: error.userFacingMessage,
                    action: ToastAction(label: FeedbackStates.Errors.retry) {
                        Task { @MainActor in merge(through: confirm) }
                    }
                )
            }
            mergeState.merging = false
        }
    }

    /// The review page counts the chat PR's files (open, else merged).
    private func syncPrChanges() {
        let wanted = rows.guidePullRequest?.id
        guard prChanges?.pullRequestId != wanted else {
            prChanges?.startObserving()
            return
        }
        prChanges?.stopObserving()
        prChanges = nil
        review = false
        guard let wanted else { return }
        let changes = ChangesViewModel(
            accountId: accountId, pullRequestId: wanted, db: deps.db, pullRequestsApi: deps.pullRequestsApi
        )
        prChanges = changes
        changes.startObserving()
    }

    // MARK: - The review page

    private func reviewPage(_ files: [Diff.File]) -> some View {
        DiffFileList(
            files: files,
            emptyLabel: .changedFiles,
            focusPath: focusPath,
            accessibilityId: "chat-results-review",
            headerFade: false,
            header: { EmptyView() }
        )
        .safeAreaInset(edge: .top, spacing: 0) { reviewBackRow(files) }
        .safeAreaInset(edge: .bottom) {
            if files.count > 1 {
                FloatingBarCluster {
                    DiffFilesBarCircle(count: files.count) { fileSheet = true }
                } center: {
                    EmptyView()
                } trailing: {
                    EmptyView()
                }
                .floatingBarEdge()
            }
        }
        .sheet(isPresented: $fileSheet) {
            DiffFileListSheet(files: files, selected: focusPath, onSelect: { focusPath = $0 })
        }
    }

    private func reviewBackRow(_ files: [Diff.File]) -> some View {
        HStack(spacing: 8) {
            Button { review = false } label: {
                HStack(spacing: 4) {
                    AppIcon(AppIcons.uiChevronLeft, size: 14, weight: .medium)
                    Text(ResultsTab.Tab.Copy.label)
                        .font(.subheadline.weight(.medium))
                }
                .foregroundStyle(.white)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("chat-results-review-back")
            Rectangle()
                .fill(GlassTokens.strokeCard)
                .frame(width: GlassTokens.hairline, height: 14)
            Text(ResultsTab.Tab.Copy.changes)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
            Spacer(minLength: 4)
            DiffCountsLabel(
                additions: files.reduce(0) { $0 + $1.additions },
                deletions: files.reduce(0) { $0 + $1.deletions }
            )
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .accessibilityElement(children: .contain)
    }

    /// An inline surface's reply press: THE chat's, sent to the result's
    /// session (`sessions.send`); a refusal toasts.
    private func reply(_ text: String, to result: ResultsTab.Result) {
        let api = deps.sessionsApi
        let accountId = accountId
        let toaster = toaster
        Task {
            do {
                _ = try await api.send(accountId: accountId, .message(sessionId: result.sessionId, text: text))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the message"),
                    description: error.userFacingMessage
                )
            }
        }
    }
}

/// The PR block's ONE `Changes` row — `<> Changes · n files +a −d ›` (web
/// `GuideChangesRow` with the tab's label): opens THE review page.
struct ResultsChangesRow: View {
    let files: [Diff.File]
    let onOpen: () -> Void

    var body: some View {
        let additions = files.reduce(0) { $0 + $1.additions }
        let deletions = files.reduce(0) { $0 + $1.deletions }
        VStack(spacing: 0) {
            Button(action: onOpen) {
                HStack(spacing: 10) {
                    AppIcon(AppIcons.guideChanges, size: 15, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text(ResultsTab.Tab.Copy.changes)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                    Spacer(minLength: 8)
                    Text(guideFileCountLabel(files.count))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    DiffCountsLabel(additions: additions, deletions: deletions)
                    AppIcon(AppIcons.uiChevronRight, size: 12)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.horizontal, 4)
                .frame(minHeight: 40)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            GlassDivider()
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(ResultsTab.Tab.Copy.changes), \(guideFileCountLabel(files.count))")
        .accessibilityIdentifier(ResultsTab.Tab.TestIds.changes)
    }
}
