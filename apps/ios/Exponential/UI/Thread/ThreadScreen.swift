import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-1284: the phone's THREAD screen (brief §1-4, draft 3) — the unit of
/// work: n issues and m runs. The header band carries the thread's title and
/// the CHILD STRIP as the pager tabs (`ThreadStripTabs`: Conversation · one
/// tab per issue · Guide, "+" trailing), replacing the Work screen's Issue ·
/// Run · Guide; the pages follow the finger (`FacePager`).
///
/// * Conversation = `ThreadConversationView` over THE thread view
///   (`ThreadViews.build`, fixture-locked ×4) with the reply box under it
///   (`Reply in this thread`: sends into my latest live run, resumes my
///   ended latest run, else opens the composer with the thread).
/// * An issue tab = the existing Issue face (`IssueFaceView`) of that issue.
/// * Guide = the existing Guide face fed the THREAD's Guide
///   (`threads.results`) with the Merge capsule of the thread's PR
///   (`ThreadViews.mergeTarget`).
///
/// The "+" and an issue tab's long press open THE Menu as a bottom sheet
/// (`ThreadMenuSheet`, the sheet Android and web-mobile open): Link an issue (THE
/// search list → `threads.link`), New issue in this thread (the New issue
/// page carrying the thread → `issues.create({threadId})`), Start a session here
/// (the Agent page composer carrying the thread → `steer.startSession(
/// {threadId})`) and, on an issue tab, Unlink <IDENT> (`threads.unlink`).
/// Failed actions toast (`Couldn't link the issue`, feedback-states).
struct ThreadScreen: View {
    let threadId: String
    let initialPage: ThreadPage

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.motion) private var motion
    @Environment(\.toaster) private var toaster
    @Environment(TeamState.self) private var teamState
    @State private var page: ThreadPage
    @State private var model: ThreadScreenModel?
    /// One Issue face model per linked issue, created as the issues arrive.
    @State private var issueVMs: [String: IssueDetailViewModel] = [:]
    /// nil until the relay config resolves (the reply box waits for it).
    @State private var steerEnabled: Bool?
    /// THE Menu's sheet: the "+" (issueId nil) or an issue tab's long press.
    @State private var menuTarget: ThreadMenuTarget?
    /// The sheet's pick, run once the sheet has dismissed.
    @State private var pendingMenuAction: (action: ThreadMenuSheet.Action, issueId: String?)?
    @State private var linkSheetOpen = false
    // Guide
    @State private var guideSection: GuideSectionKey?
    @State private var focusPath: String?
    @State private var mergeState = WorkMergeState()
    @State private var prChangesModel: ChangesViewModel?
    // Reply
    @State private var reply = ""
    @State private var replying = false
    /// The live run the reply box sends into — its socket claimed while the
    /// screen is up, so a message goes out the moment it is typed.
    @State private var claimedRunId: String?
    /// A reply that RESUMED my ended run, waiting for the successor's socket.
    @State private var pendingReply: String?

    init(threadId: String, initialPage: ThreadPage = .conversation) {
        self.threadId = threadId
        self.initialPage = initialPage
        _page = State(initialValue: initialPage)
    }

    private var rows: ThreadRows? { model?.rows }
    private var view: ThreadViewModel? { model?.view }

    private var pages: [ThreadPage] {
        guard let view else { return [.conversation] }
        var pages: [ThreadPage] = []
        for child in view.strip {
            switch child {
            case .conversation: pages.append(.conversation)
            case let .issue(issueId, _, _): pages.append(.issue(issueId))
            case .guide: pages.append(.guide)
            case .add: break
            }
        }
        return pages
    }

    var body: some View {
        ZStack {
            AppBackground()
            if let model, let rows = model.rows, let view = model.view {
                FacePager(pages: pages, selection: Binding(get: { page }, set: { switchPage($0) })) { page in
                    pageView(page, rows: rows, view: view)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .workHeaderBand(ground: page == .conversation) { header(rows: rows, view: view) }
            } else if model?.missing == true {
                // EXP-1284: empty-state.json `thread-not-found`; its action
                // lands on the Agent tab.
                EmptyState(.threadNotFound) {
                    pushRoute(.agent(accountId: accountId, seed: .empty))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityIdentifier("thread-not-found")
            } else {
                // EXP-1275 (D8): a first load is skeleton rows.
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        // Each presentation on its OWN node (EXP-240).
        .background {
            Color.clear
                .sheet(item: $menuTarget, onDismiss: runPendingMenuAction) { target in
                    ThreadMenuSheet(
                        identifier: target.issueId.flatMap { rows?.issue(id: $0)?.identifier }
                    ) { action in
                        pendingMenuAction = (action, target.issueId)
                        menuTarget = nil
                    }
                }
        }
        .sheet(isPresented: $linkSheetOpen) {
            if let rows {
                ThreadLinkIssueSheet(
                    teamId: rows.thread.teamId,
                    excluded: Set(rows.linkedIssues.map(\.id)),
                    onPick: { issueId in
                        linkSheetOpen = false
                        link(issueId)
                    }
                )
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-screen")
        .onAppear(perform: appear)
        .onDisappear(perform: disappear)
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        // A page that vanished (an unlinked issue) lands on the Conversation.
        .onChange(of: pages) { _, pages in
            if !pages.contains(page) { page = .conversation }
            syncIssueModels()
        }
        .onChange(of: replyTarget) { _, _ in claimReplyRun() }
        .onChange(of: wantsPrChanges, initial: true) { _, _ in syncPrChangesModel() }
        .onChange(of: mergeTarget.map(Self.mergeKey)) { _, _ in mergeState = WorkMergeState() }
    }

    // MARK: - Header

    private func header(rows: ThreadRows, view: ThreadViewModel) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            ThreadHeaderTitle(title: view.title, caption: view.headerCaption)
            ThreadStripTabs(
                strip: view.strip,
                selection: page,
                issue: { rows.issue(id: $0) },
                isFresh: { issueId in rows.item(issueId: issueId).map { model?.isFresh($0) ?? false } ?? false },
                onSelect: selectPage,
                onIssueMenu: { issueId, frame in openMenu(issueId: issueId, anchor: frame) },
                onAdd: canEdit ? { (frame: CGRect) in openMenu(issueId: nil, anchor: frame) } : nil
            )
        }
        .padding(.top, 4)
    }

    /// A member may link, file and start here (`resolveTeamAccess`: every
    /// member comments and creates issues).
    private var canEdit: Bool {
        guard let rows else { return false }
        return teamState.teams.contains { $0.id == rows.thread.teamId }
    }

    // MARK: - Pages

    @ViewBuilder
    private func pageView(_ page: ThreadPage, rows: ThreadRows, view: ThreadViewModel) -> some View {
        switch page {
        case .conversation:
            conversationPage(rows: rows, view: view)
        case let .issue(issueId):
            issuePage(issueId)
        case .guide:
            guidePage(rows: rows)
        }
    }

    private func conversationPage(rows: ThreadRows, view: ThreadViewModel) -> some View {
        ScrollView {
            ThreadConversationView(
                rows: rows,
                view: view,
                me: deps.auth.userId,
                now: model?.now ?? Date(),
                onOpenRun: { runId in
                    pushRoute(.agentSession(accountId: accountId, sessionId: runId))
                },
                onOpenIssue: { issueId in selectPage(.issue(issueId)) }
            )
            .padding(.vertical, 12)
            .transcriptColumn()
        }
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom) {
            if steerEnabled == true {
                replyBox
            }
        }
    }

    @ViewBuilder
    private func issuePage(_ issueId: String) -> some View {
        if let vm = issueVMs[issueId], let issue = vm.issue {
            IssueFaceView(
                vm: vm,
                issue: issue,
                barTrailing: .hidden,
                onStartCoding: {},
                onOpenChanges: { selectPage(.guide) }
            )
        } else {
            SkeletonRows()
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }

    // MARK: - Guide (the THREAD's Guide)

    private var mergeTarget: MergeTarget? { rows?.mergeTarget }

    private static func mergeKey(_ target: MergeTarget) -> String {
        switch target {
        case let .issue(id): "issue:\(id)"
        case let .session(id): "session:\(id)"
        }
    }

    /// The PR files the Guide counts: the thread PR's issue (or run).
    private var wantsPrChanges: String? {
        mergeTarget.map(Self.mergeKey)
    }

    private func guidePage(rows: ThreadRows) -> some View {
        GuideFace(
            groups: parseSessionResultGroups(rows.thread.results),
            files: prChangesModel?.loadedFiles,
            diffStatus: prDiffStatus,
            teamId: rows.thread.teamId,
            section: $guideSection,
            focusPath: $focusPath,
            showsMerge: mergeTarget != nil
        ) {
            mergeCapsule(rows: rows)
        }
    }

    private var prDiffStatus: GuideDiffStatus {
        guard let prChangesModel else { return .ready }
        switch prChangesModel.load {
        case .loading: return .loading
        case let .failed(message): return .failed(message)
        case .loaded: return .ready
        }
    }

    @ViewBuilder
    private func mergeCapsule(rows: ThreadRows) -> some View {
        if let target = mergeTarget {
            let issue: IssueEntity? = {
                if case let .issue(id) = target { return rows.issue(id: id) }
                return nil
            }()
            let session: CodingSessionEntity? = {
                if case let .session(id) = target { return rows.session(id: id) }
                return nil
            }()
            WorkMergePill(
                target: target,
                issue: issue,
                prIssues: rows.linkedIssues,
                steerEnabled: steerEnabled == true,
                runPrNumber: session?.prNumber,
                teamId: rows.thread.teamId,
                state: $mergeState,
                identifier: "thread-merge-pr"
            )
            .id(Self.mergeKey(target))
        }
    }

    private func syncPrChangesModel() {
        prChangesModel?.stopObserving()
        prChangesModel = nil
        guard let target = mergeTarget else { return }
        let source: ChangesViewModel.Source = switch target {
        case let .issue(id): .issue(id)
        case let .session(id): .session(id)
        }
        let changes = ChangesViewModel(
            accountId: accountId,
            source: source,
            db: deps.db,
            issuesApi: deps.issuesApi,
            repositoriesApi: deps.repositoriesApi,
            codingSessionsApi: deps.codingSessionsApi
        )
        prChangesModel = changes
        changes.startObserving()
    }

    // MARK: - Reply (brief §2)

    private var replyTarget: ThreadViews.ReplyTarget? {
        guard let rows else { return nil }
        return ThreadViews.replyTarget(runs: rows.viewRuns, me: deps.auth.userId)
    }

    private var replyBox: some View {
        ThreadReplyBox(text: $reply, sending: replying, onSend: sendReply)
    }

    private func sendReply() {
        let text = reply.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !replying, let rows, let target = replyTarget else { return }
        switch target {
        case let .send(runId):
            guard let session = rows.session(id: runId) else { return }
            let steer = deps.attachSteerModel(accountId: accountId, session: session)
            defer { deps.steerSessions.detach(accountId: accountId, sessionId: runId) }
            if steer.sendMessage(text) {
                reply = ""
            } else {
                toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the message"))
            }
        case let .resume(runId):
            guard let session = rows.session(id: runId) else { return }
            let devices = rows.devices.map {
                SteerDevice(entity: $0, now: Date(), currentUserId: deps.auth.userId)
            }
            guard let device = RunResume.target(for: session, devices: devices, currentUserId: deps.auth.userId)
            else {
                // Nothing can take the run back: start a fresh one here.
                openComposer(issueId: nil, text: text)
                reply = ""
                return
            }
            replying = true
            Task {
                do {
                    try await deps.steerApi.resumeSession(
                        accountId: accountId, sessionId: runId, deviceId: device.deviceId
                    )
                    // The resume stays in the thread; the text follows it
                    // once its socket is up (`claimReplyRun`).
                    pendingReply = text
                    reply = ""
                } catch {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: "resume", thing: "the session"),
                        description: error.userFacingMessage
                    )
                }
                replying = false
            }
        case .start:
            openComposer(issueId: nil, text: text)
            reply = ""
        }
    }

    /// Keep ONE claim on the live run the reply box sends into, and deliver a
    /// reply that resumed a run once its successor is live.
    private func claimReplyRun() {
        var wanted: String?
        if case let .send(runId)? = replyTarget { wanted = runId }
        if claimedRunId != wanted {
            if let claimedRunId {
                deps.steerSessions.detach(accountId: accountId, sessionId: claimedRunId)
            }
            claimedRunId = nil
            if let wanted, let session = rows?.session(id: wanted) {
                _ = deps.attachSteerModel(accountId: accountId, session: session)
                claimedRunId = wanted
            }
        }
        guard let text = pendingReply, let runId = wanted, let session = rows?.session(id: runId) else { return }
        pendingReply = nil
        let steer = deps.attachSteerModel(accountId: accountId, session: session)
        Task {
            defer { deps.steerSessions.detach(accountId: accountId, sessionId: runId) }
            // The successor's socket dials after its row lands: retry for a
            // while rather than drop the text.
            for _ in 0..<30 {
                if steer.sendMessage(text) { return }
                try? await Task.sleep(for: .seconds(1))
            }
            reply = text
            toaster.error(FeedbackStates.Errors.title(verb: "send", thing: "the message"))
        }
    }

    // MARK: - THE Menu (brief §4)

    /// The anchor the strip reports is unused: on a phone THE Menu is a
    /// sheet, never a popover.
    private func openMenu(issueId: String?, anchor _: CGRect) {
        guard canEdit else { return }
        UIApplication.endEditing()
        menuTarget = ThreadMenuTarget(issueId: issueId)
    }

    private func runPendingMenuAction() {
        guard let pending = pendingMenuAction else { return }
        pendingMenuAction = nil
        switch pending.action {
        case .linkIssue: linkSheetOpen = true
        case .newIssue: newIssue(nextTo: pending.issueId)
        case .startSession: openComposer(issueId: pending.issueId, text: nil)
        case .unlink:
            if let issueId = pending.issueId { unlink(issueId) }
        }
    }

    private func link(_ issueId: String) {
        let api = deps.threadsApi
        let accountId = accountId
        let threadId = threadId
        Task {
            do {
                try await api.link(accountId: accountId, .issue(threadId: threadId, issueId: issueId))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "link", thing: "the issue"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    private func unlink(_ issueId: String) {
        if page == .issue(issueId) { selectPage(.conversation) }
        let api = deps.threadsApi
        let accountId = accountId
        let threadId = threadId
        Task {
            do {
                try await api.unlink(accountId: accountId, .issue(threadId: threadId, issueId: issueId))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "unlink", thing: "the issue"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    /// The New issue page carrying the thread, on the chip's board, else the
    /// thread's first issue's, else the team's first board.
    private func newIssue(nextTo issueId: String?) {
        guard let rows else { return }
        let boardId = issueId.flatMap { rows.issue(id: $0)?.boardId }
            ?? rows.linkedIssues.first?.boardId
            ?? teamState.boards
                .filter { $0.teamId == rows.thread.teamId }
                .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
                .first?.id
        guard let boardId else {
            toaster.error(FeedbackStates.Errors.title(verb: "file", thing: "the issue"), description: "The team has no board yet.")
            return
        }
        // EXP-1170: the draft id is minted HERE, at tap time.
        pushRoute(.issueDraft(
            accountId: accountId,
            draftId: UUID().uuidString.lowercased(),
            boardId: boardId,
            threadId: threadId
        ))
    }

    /// The Agent page composer prefilled with the thread (and the chip's
    /// issue): its start lands in this thread.
    private func openComposer(issueId: String?, text: String?) {
        guard let rows else { return }
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                issueIds: issueId.map { [$0] } ?? [],
                text: text,
                teamId: rows.thread.teamId,
                threadId: threadId
            )
        ))
    }

    // MARK: - Lifecycle

    private func appear() {
        if model == nil {
            model = ThreadScreenModel(threadId: threadId, accountId: accountId, db: deps.db)
        }
        model?.start(api: deps.threadsApi)
        for vm in issueVMs.values { vm.startObserving() }
        prChangesModel?.startObserving()
        syncIssueModels()
        claimReplyRun()
    }

    private func disappear() {
        UIApplication.endEditing()
        model?.stop()
        prChangesModel?.stopObserving()
        for vm in issueVMs.values {
            vm.stopObserving()
            Task {
                await vm.saveTitle()
                await vm.commitDescription()
            }
        }
        if let claimedRunId {
            deps.steerSessions.detach(accountId: accountId, sessionId: claimedRunId)
            self.claimedRunId = nil
        }
    }

    /// One Issue face model per linked issue (created once, kept while the
    /// screen lives — an unlinked issue's model just stops).
    private func syncIssueModels() {
        guard let rows else { return }
        let ids = Set(rows.linkedIssues.map(\.id))
        for id in ids where issueVMs[id] == nil {
            let vm = IssueDetailViewModel(
                accountId: accountId,
                issueId: id,
                db: deps.db,
                issuesApi: deps.issuesApi,
                attachmentsApi: deps.attachmentsApi,
                labelsApi: deps.labelsApi,
                relationsApi: deps.relationsApi,
                steerApi: deps.steerApi,
                widgetsApi: deps.widgetsApi,
                auth: deps.auth
            )
            issueVMs[id] = vm
            vm.startObserving()
        }
        for (id, vm) in issueVMs where !ids.contains(id) {
            vm.stopObserving()
        }
    }

    // MARK: - Pages

    private func selectPage(_ next: ThreadPage) {
        withAnimation(motion.standard) { switchPage(next) }
    }

    private func switchPage(_ next: ThreadPage) {
        guard next != page else { return }
        UIApplication.endEditing()
        page = next
    }
}

/// EXP-1284: what the thread's menu sheet is open for — an issue tab
/// (`issueId`) or the strip's "+" (nil).
struct ThreadMenuTarget: Identifiable {
    let issueId: String?
    var id: String { issueId ?? "add" }
}

/// EXP-1284: the Conversation's reply box — `Reply in this thread`, the send
/// mark once there is text. What a send does is the screen's
/// (`ThreadViews.replyTarget`).
struct ThreadReplyBox: View {
    @Binding var text: String
    var sending = false
    let onSend: () -> Void

    private var hasText: Bool {
        !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField(ThreadViews.replyPlaceholder, text: $text, axis: .vertical)
                .lineLimit(1...5)
                .font(.body)
                .foregroundStyle(.white)
                .padding(.vertical, 4)
                .accessibilityIdentifier("thread-reply-field")
            if hasText {
                GlassComposerSubmitButton(
                    AppIcons.uiSend,
                    accessibilityLabel: "Send",
                    enabled: !sending,
                    pending: sending,
                    action: onSend
                )
                .accessibilityIdentifier("thread-reply-send")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: GlassComposerTokens.cornerRadius, style: .continuous)
                .fill(GlassTokens.opaqueCardFill)
        )
        .overlay(
            RoundedRectangle(cornerRadius: GlassComposerTokens.cornerRadius, style: .continuous)
                .strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .transcriptColumn()
        .padding(.bottom, 8)
        .floatingBarEdge()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-reply")
    }
}

/// EXP-1284: a run's THREAD (`AppRoute.runThread`) — what a just-started run
/// and a run row open: the thread once the run's `thread_items` row synced
/// (every start opens or joins one), else — a legacy run, or one the shapes
/// have not delivered within `wait` — the run's own Work screen, kept once
/// shown so the screen never swaps under the reader.
struct RunThreadScreen: View {
    let sessionId: String
    var wait: Duration = .seconds(4)

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var threadId: String?
    @State private var gaveUp = false

    var body: some View {
        Group {
            if let threadId {
                ThreadScreen(threadId: threadId)
                    .id(threadId)
            } else if gaveUp {
                WorkScreen(subject: .session(id: sessionId))
            } else {
                ZStack {
                    AppBackground()
                    SkeletonRows()
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                }
            }
        }
        .task(id: sessionId) { await resolve() }
    }

    private func resolve() async {
        guard threadId == nil, !gaveUp, let pool = try? deps.db.pool(forAccountId: accountId) else {
            if threadId == nil { gaveUp = true }
            return
        }
        let sessionId = sessionId
        let observation = ValueObservation.tracking { db in
            try ThreadQueries.threadOfSession(db: db, sessionId: sessionId)?.id
        }
        let deadline = ContinuousClock.now.advanced(by: wait)
        let timeout = Task { @MainActor in
            try? await Task.sleep(until: deadline, clock: .continuous)
            if threadId == nil { gaveUp = true }
        }
        defer { timeout.cancel() }
        do {
            for try await id in observation.values(in: pool) {
                if gaveUp { return }
                if let id {
                    threadId = id
                    return
                }
            }
        } catch {}
    }
}
