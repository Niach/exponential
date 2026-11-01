import ExpCore
import ExpUI
import SwiftUI
import UIKit

// STORIES TAB ×iOS (fixtures/stories.json STORIES TAB; web
// `packages/ui/src/story-tab.tsx` + `components/thread/thread-stories.tsx`):
// the thread screen's last pager page. Top: the PR block while the thread
// has its ONE pull request (`guidePullRequest`): the Stack band + THE
// StackRail when it sits in a linear open stack, then ONE Changes row
// opening THE review page (the complete diff, back row `‹ Stories`). Then
// the stories INLINE (`Stories.storyTab`, #EXP-1462: the Guide's way, never
// a list of title rows); a tap on a story's line opens THE viewer on the
// tab's ring at that story (a picture: at that picture).

/// The waiting tint (ask-dock: plan primary, question yellow).
private func storyTint(_ story: Stories.Story) -> Color {
    (story.kind == .question ? Dock.Tint.question : Dock.Tint.plan).color
}

/// How a story's body draws in place (absent = the line alone).
struct StoryBodyRenderers {
    /// The story's raw card (its surface), nil = none.
    var card: (String) -> TreeMessageCard? = { _ in nil }
    /// A surface press's reply text (THE chat's `surfaceReply`), sent to the
    /// story's session; nil = the surface takes no input.
    var onSurfaceReply: ((Stories.Story, String) -> Void)?
}

/// THE Stories tab body: `lead` (the PR block), then the stories INLINE in
/// fold order `tile.listGap` apart, else THE EmptyState. A pure painter: the
/// styleguide specimen draws it over `tabCases[1]`.
struct StoryTabView: View {
    let ring: Stories.StoryRing?
    var lead: AnyView?
    var renderers = StoryBodyRenderers()
    /// A tap on a story's line (picture 0) or a picture (its index).
    var onOpen: (Stories.Story, Int) -> Void = { _, _ in }
    /// #EXP-1493: a tap anywhere on a plan section = THE plan page.
    var onOpenPlan: (Stories.Story) -> Void = { _ in }

    var body: some View {
        let stories = ring?.stories ?? []
        VStack(alignment: .leading, spacing: CGFloat(Stories.Tab.Tile.leadGap)) {
            if let lead { lead }
            if stories.isEmpty {
                EmptyState(title: Stories.Tab.Copy.empty, icon: AppIcons.threadStories, size: .page)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 48)
            } else {
                VStack(alignment: .leading, spacing: CGFloat(Stories.Tab.Tile.listGap)) {
                    ForEach(stories) { story in
                        StoryTile(story: story, renderers: renderers, onOpen: onOpen, onOpenPlan: onOpenPlan)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Stories.Tab.TestIds.list)
    }
}

/// One story's SECTION (#EXP-1462): line 1 = the title (one line; none = the
/// kind word) with `meta` muted at the end (a waiting one leads with the
/// tint dot and tints its meta); the description (muted, wrapping)
/// `tile.textGap` under it; then its body `tile.gap` under that, by the
/// frame rule: a PLAN (#EXP-1493) = its counts line (after the description)
/// then its pictures row,
/// a tap anywhere opening THE plan page; else ONE sideways row of pictures
/// (a muted caption-or-label line under each), else THE surface card inline
/// (no title over it, presses reply like the chat), else nothing. THE kind
/// badge leads line 1.
struct StoryTile: View {
    let story: Stories.Story
    var renderers = StoryBodyRenderers()
    let onOpen: (Stories.Story, Int) -> Void
    var onOpenPlan: (Stories.Story) -> Void = { _ in }

    @State private var handle = PlanCardSurfaceHandle()

    var body: some View {
        let gap = CGFloat(Stories.Tab.Tile.gap)
        // The frame rule: a plan = counts + pictures; pictures win, else the surface.
        let plan = story.kind == .plan
        let surface = !plan && story.pictures.isEmpty && story.surface ? renderers.card(story.id)?.surface : nil
        let counts = story.counts.map(PlanPage.countsLine) ?? ""
        VStack(alignment: .leading, spacing: gap) {
            Button { plan ? onOpenPlan(story) : onOpen(story, 0) } label: {
                VStack(alignment: .leading, spacing: CGFloat(Stories.Tab.Tile.textGap)) {
                    HStack(spacing: 8) {
                        StoryKindBadge(kind: story.kind)
                        if story.waiting {
                            Circle()
                                .fill(storyTint(story))
                                .frame(width: 6, height: 6)
                                .accessibilityHidden(true)
                        }
                        Text(story.title ?? Stories.Copy.kind(story.kind))
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Text(story.meta)
                            .font(.system(size: 12))
                            .foregroundStyle(story.waiting ? storyTint(story) : DesignTokens.Palette.mutedForeground)
                            .lineLimit(1)
                            .fixedSize()
                    }
                    if let description = story.description, !description.isEmpty {
                        Text(description)
                            .font(.system(size: 14))
                            .foregroundStyle(DesignTokens.Palette.mutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .multilineTextAlignment(.leading)
                    }
                    if plan, !counts.isEmpty {
                        Text(counts)
                            .font(.system(size: 12))
                            .foregroundStyle(DesignTokens.Palette.mutedForeground)
                            .lineLimit(1)
                            .accessibilityIdentifier("story-tile-counts")
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if !story.pictures.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: gap) {
                        ForEach(Array(story.pictures.enumerated()), id: \.offset) { index, picture in
                            Button { plan ? onOpenPlan(story) : onOpen(story, index) } label: {
                                StoryThumbnail(picture: picture)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            } else if let surface {
                let reply = renderers.onSurfaceReply.map { send in { (text: String) in send(story, text) } }
                SurfaceCardFrame(title: nil, animate: false) {
                    PlanCardSurfaceView(
                        surface: surface, handle: handle, editable: reply != nil, onReply: reply, bare: true
                    )
                }
                .accessibilityIdentifier("story-tile-surface")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Stories.Tab.TestIds.tile)
    }
}

/// One thumbnail: `tile.thumbHeight` tall, width by aspect (unknown = 4:3)
/// capped at `tile.thumbMaxWidth`, `tile.radius`, the glass-stroke hairline
/// on the muted fill, cropped from the top; the caption-else-label line 6
/// under it.
struct StoryThumbnail: View {
    let picture: Stories.Picture

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var image: UIImage?

    var body: some View {
        let width = CGFloat(Stories.Tab.Tile.thumbWidth(width: picture.width, height: picture.height))
        let height = CGFloat(Stories.Tab.Tile.thumbHeight)
        let shape = RoundedRectangle(cornerRadius: CGFloat(Stories.Tab.Tile.radius), style: .continuous)
        VStack(alignment: .leading, spacing: 6) {
            ZStack(alignment: .top) {
                DesignTokens.Palette.muted
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(width: width, height: height, alignment: .top)
                }
            }
            .frame(width: width, height: height, alignment: .top)
            .clipShape(shape)
            .overlay(shape.strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
            Text(picture.caption ?? picture.label)
                .font(.system(size: 12))
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .lineLimit(1)
                .truncationMode(.tail)
        }
        .frame(width: width, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(picture.caption ?? picture.label)
        .accessibilityAddTraits(.isButton)
        .task(id: picture.attachmentId) {
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

/// Where THE viewer opens from a tile.
private struct StoryTabTarget: Identifiable {
    let storyId: String
    let picture: Int
    var id: String { "\(storyId)#\(picture)" }
}

/// The thread's Stories page: the PR block, the tiles, THE viewer, and the
/// review page (the PR's complete diff) in place of the tiles while open.
struct ThreadStoriesPage: View {
    let rootId: String
    let rows: ThreadRows
    let ring: Stories.StoryRing?
    let stories: StoriesModel
    /// A member may answer a waiting story and merge through a rail member.
    let canEdit: Bool
    @Binding var mergeState: WorkMergeState

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.toaster) private var toaster

    @State private var opened: StoryTabTarget?
    @State private var pendingSession: Stories.Story?
    /// #EXP-1493: the plan a cover's Open plan opens once the viewer is gone.
    @State private var pendingPlan: Stories.Story?
    @State private var prChanges: ChangesViewModel?
    @State private var review = false
    @State private var focusPath: String?
    @State private var fileSheet = false
    @State private var mergeThrough: PullRequestStack.Confirm?

    var body: some View {
        Group {
            if review, let files = prChanges?.loadedFiles {
                reviewPage(files)
            } else {
                ScrollView {
                    StoryTabView(
                        ring: ring,
                        lead: lead,
                        renderers: StoryBodyRenderers(
                            card: { stories.card($0) },
                            onSurfaceReply: canEdit ? { story, text in reply(text, to: story) } : nil
                        )
                    ) { story, picture in
                        opened = StoryTabTarget(storyId: story.id, picture: picture)
                    } onOpenPlan: { story in
                        pushRoute(.planPage(accountId: accountId, sessionId: story.sessionId, messageId: story.id))
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 24)
                    .padding(.bottom, 40)
                }
                .scrollIndicators(.hidden)
            }
        }
        .onChange(of: rows.guidePullRequest?.id, initial: true) { _, _ in syncPrChanges() }
        .onDisappear { prChanges?.stopObserving() }
        .fullScreenCover(item: $opened, onDismiss: openPendingSession) { target in
            StoryViewer(
                rings: ring.map { [$0] } ?? [],
                startKey: ring?.key ?? "",
                startStory: target.storyId,
                startPicture: target.picture,
                card: { stories.card($0) },
                avatar: stories.avatar,
                onSeen: markSeen,
                onAnswer: canEdit ? answer : { _ in false },
                onOpenSession: { story in
                    pendingSession = story
                    opened = nil
                },
                onOpenPlan: { story in
                    pendingPlan = story
                    opened = nil
                },
                onClose: { opened = nil }
            )
            .environment(deps)
            .environment(\.accountId, accountId)
        }
    }

    // MARK: - The PR block

    private var lead: AnyView? {
        guard rows.guidePullRequest != nil else { return nil }
        let changes = prChanges?.loadedFiles
        return AnyView(
            VStack(alignment: .leading, spacing: 12) {
                if let stack = stackBlock { stack }
                if let changes, !changes.isEmpty {
                    StoriesChangesRow(files: changes) {
                        focusPath = nil
                        review = true
                    }
                } else if prChanges?.loadedFiles == nil, case .loading = prChanges?.load {
                    SkeletonRows(rows: 1, rowHeight: 40)
                }
            }
        )
    }

    /// The `Stack` band over THE `StackRail`, top first, this thread's PR in
    /// the current wash, down to the base branch. nil = no linear open stack.
    private var stackBlock: AnyView? {
        guard let view = rows.guideStackView else { return nil }
        let members = view.rows.map {
            StackRailMember(id: $0.pullRequestId, identifier: $0.identifier, title: $0.title, current: $0.isCurrent)
        }
        let roots = Dictionary(
            view.rows.compactMap { row in row.rootSessionId.map { (row.pullRequestId, $0) } },
            uniquingKeysWith: { first, _ in first }
        )
        return AnyView(
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(Stories.Tab.Copy.stack) {
                    AppIcon(AppIcons.prStack, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                } trailing: {
                    EmptyView()
                }
                StackRail(
                    members: members,
                    baseBranch: view.baseBranch ?? "default branch",
                    onOpen: { member in
                        // The thread on screen is already open.
                        guard let other = roots[member.id], other != rootId else { return }
                        pushRoute(.thread(accountId: accountId, threadId: other, child: .stories))
                    },
                    onMergeThrough: canEdit && !mergeState.merging
                        ? { member in mergeThrough = rows.mergeThroughConfirm(pullRequestId: member.id) }
                        : nil
                )
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Stories.Tab.TestIds.stack)
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
        )
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

    /// The review page counts the thread PR's files (open, else merged).
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
            accessibilityId: "thread-stories-review",
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
                    Text(Stories.Tab.Copy.label)
                        .font(.subheadline.weight(.medium))
                }
                .foregroundStyle(.white)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("thread-stories-review-back")
            Rectangle()
                .fill(GlassTokens.strokeCard)
                .frame(width: GlassTokens.hairline, height: 14)
            Text(Stories.Tab.Copy.changes)
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

    // MARK: - The viewer

    private func openPendingSession() {
        if let plan = pendingPlan {
            pendingPlan = nil
            pushRoute(.planPage(accountId: accountId, sessionId: plan.sessionId, messageId: plan.id))
            return
        }
        guard let story = pendingSession else { return }
        pendingSession = nil
        pushRoute(.thread(
            accountId: accountId,
            threadId: story.rootId,
            child: story.sessionId == story.rootId ? nil : .session(story.sessionId)
        ))
    }

    /// An inline surface's reply press: THE chat's, sent to the story's
    /// session (`sessions.send`); a refusal toasts.
    private func reply(_ text: String, to story: Stories.Story) {
        let api = deps.sessionsApi
        let accountId = accountId
        let toaster = toaster
        Task {
            do {
                _ = try await api.send(accountId: accountId, .message(sessionId: story.sessionId, text: text))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the message"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    private func markSeen(_ story: Stories.Story) {
        guard !story.seen else { return }
        let api = deps.sessionsApi
        let accountId = accountId
        Task { try? await api.markStoriesSeen(accountId: accountId, sessionId: story.rootId, at: story.createdAt) }
    }

    private func answer(_ card: ThreadCardAnswer) async -> Bool {
        do {
            try await deps.sessionsApi.answer(
                accountId: accountId, messageId: card.messageId, answer: card.answer, decision: card.decision,
                fresh: card.fresh, feedback: card.feedback, answers: card.answers, surfaceData: card.surfaceData
            )
            return true
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "answer", thing: card.decision == nil ? "the question" : "the plan"),
                description: error.userFacingMessage
            )
            return false
        }
    }
}

/// The PR block's ONE `Changes` row — `<> Changes · n files +a −d ›` (web
/// `GuideChangesRow` with the tab's label): opens THE review page.
struct StoriesChangesRow: View {
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
                    Text(Stories.Tab.Copy.changes)
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
        .accessibilityLabel("\(Stories.Tab.Copy.changes), \(guideFileCountLabel(files.count))")
        .accessibilityIdentifier(Stories.Tab.TestIds.changes)
    }
}
