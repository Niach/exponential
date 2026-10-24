import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the issue route — the issue page alone (`IssueFaceView`: pills,
/// description, sub-issues, the Threads row, files, activity) under the
/// detail chrome. The Issue|Session|Guide faces, the Stop pill, the merge
/// capsule and the PR graph badge are gone; the bottom bar's ONE Start circle
/// opens the composer with this issue picked, and the composer's send starts
/// the session.
///
/// The nav bar: back · the identifier (collapsing to the identifier over the
/// title once the page's own title row scrolled under the header band,
/// EXP-1162) · the `…` menu (Share, Move to board, Unmark duplicate, Delete).
struct IssueScreen: View {
    let issueId: String
    /// EXP-1241: the comment the page opens scrolled to (and flashes).
    var focusCommentId: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.dismiss) private var dismiss
    @State private var issueVM: IssueDetailViewModel?
    @State private var menuAnchor: CGRect = .zero
    @State private var menuOpen = false
    /// EXP-603: `ShareLink` cannot live inside a `GlassMenu` (its rows are
    /// plain buttons), so the menu item hands the URL to a host-level sheet.
    @State private var shareTarget: ShareTarget?
    @State private var showDeleteConfirm = false
    @State private var showMoveBoard = false
    /// The board picked in the move sheet, pending confirmation (EXP-57).
    @State private var moveTarget: BoardEntity?
    /// Courier: the picked board, promoted once the picker dismissed.
    @State private var pendingMoveTarget: BoardEntity?
    @State private var markedRead = false
    /// EXP-1121: the "Ready to code?" inputs (repositories, GitHub, live
    /// devices) — shared by the Start circle and the sheet.
    @State private var readinessModel: CodingReadinessModel?
    @State private var showReadiness = false
    /// What the sheet asked for, run once it has dismissed (a push from
    /// under a dismissing sheet is dropped).
    @State private var readinessFollowUp: ReadinessFollowUp?
    /// EXP-1162: whether the title row has scrolled under the header band —
    /// flipped ONLY on the edge (`titleEdges`), so a scroll never re-renders
    /// the screen.
    @State private var titleScrolledAway = false
    @State private var titleEdges = TitleCollapseTracker()

    private var issue: IssueEntity? { issueVM?.issue }
    private var teamId: String? { issueVM?.board?.teamId }

    /// EXP-1121: whether Start can run right now, and what is missing when
    /// not. nil before the issue's view model exists.
    private var readiness: CodingReadiness.Readiness? {
        guard let vm = issueVM, let readinessModel else { return nil }
        return readinessModel.readiness(vm: vm)
    }

    /// The bar's trailing circle: Start behind its gates (EXP-240).
    private var barTrailing: IssueBarTrailing {
        guard let readiness, readiness.visible else { return .hidden }
        return .start(readiness)
    }

    var body: some View {
        withOverlays(withLifecycle(withChrome(screenContent)))
    }

    private var screenContent: some View {
        ZStack {
            AppBackground()
            page
                // The detail chrome's header band, with no tabs.
                .workHeaderBand(onBottom: { bottom in
                    titleEdges.headerBottom = bottom
                    titleEdgesChanged()
                }) { EmptyView() }
                .onPreferenceChange(IssueTitleRowBottomKey.self) { bottom in
                    titleEdges.titleBottom = bottom
                    titleEdgesChanged()
                }
        }
    }

    @ViewBuilder
    private var page: some View {
        if let vm = issueVM, let issue = vm.issue {
            IssueFaceView(
                vm: vm,
                issue: issue,
                barTrailing: barTrailing,
                onStartCoding: startTapped,
                focusCommentId: focusCommentId
            )
        } else if let vm = issueVM, vm.loadTimedOut {
            unavailableState(vm: vm)
        } else {
            // EXP-1275 (D8): a first load is skeleton rows.
            SkeletonRows()
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }

    // Shown once the bounded load clock gives up (EXP-264): the issue is
    // either still syncing or genuinely out of reach — say so, and offer
    // another attempt instead of an endless spinner.
    private func unavailableState(vm: IssueDetailViewModel) -> some View {
        VStack(spacing: 12) {
            Text("This issue isn't available yet")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("It may still be syncing, or you may not have access to it.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
            GlassPill("Try again", size: .md, mode: .action { vm.retryLoad() })
        }
        .padding(24)
    }

    private func titleEdgesChanged() {
        let away = titleEdges.collapsed
        if away != titleScrolledAway { titleScrolledAway = away }
    }

    // MARK: - Nav bar

    private func withChrome(_ content: some View) -> some View {
        content
            .navigationBarTitleDisplayMode(.inline)
            // EXP-1150: the header band draws the bar's material, with its
            // own edge — the nav bar's background and divider stand down.
            .toolbarBackground(.hidden, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    WorkTitle(
                        text: issue?.identifier ?? "",
                        issueTitle: issue?.title,
                        collapsed: titleScrolledAway
                    )
                }
                ToolbarItem(placement: .topBarTrailing) {
                    if issue != nil {
                        GlassMenuBarButton(
                            icon: AppIcons.uiMore,
                            accessibilityLabel: "More",
                            anchor: $menuAnchor,
                            isPresented: $menuOpen
                        )
                    }
                }
            }
    }

    // MARK: - Overlays, sheets, alerts

    private func withOverlays(_ content: some View) -> some View {
        content
            // EXP-687: the popup rides an in-view overlay on THIS root, not a
            // presentation launched from inside a bar item.
            .glassMenuOverlay(isPresented: $menuOpen, anchor: menuAnchor, presentation: .inline) {
                menuItems
            }
            // Each presentation lives on its OWN node (EXP-240).
            .background {
                Color.clear
                    .sheet(item: $shareTarget) { target in
                        ActivityShareSheet(items: [target.text, target.url])
                    }
            }
            // EXP-1021: the shared board picker, opened by the `…` menu item.
            .background {
                if let vm = issueVM, let issue = vm.issue {
                    MoveBoardPicker(
                        boards: vm.moveTargetBoards,
                        selectedId: issue.boardId,
                        open: $showMoveBoard,
                        onDismiss: promoteMoveTarget,
                        onSelect: { target in pendingMoveTarget = target }
                    )
                }
            }
            // The alert cards (`.glassAlert` = a full-screen cover) hang off
            // zero-size nodes of their own as well.
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .moveBoardConfirm(
                        target: $moveTarget,
                        identifier: issue?.identifier,
                        onConfirm: { target in Task { await issueVM?.moveToBoard(target.id) } }
                    )
            }
            // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(isPresented: $showDeleteConfirm) {
                        GlassAlert(
                            prompt: Prompts.DeleteIssue.copy(identifier: issue?.identifier ?? "this issue"),
                            handlers: ["delete": { deleteIssue() }]
                        )
                    }
            }
            // EXP-1121: "Ready to code?" — its fixes that leave the issue run
            // AFTER the dismiss.
            .background {
                Color.clear
                    .sheet(isPresented: $showReadiness, onDismiss: runReadinessFollowUp) {
                        if let vm = issueVM, let readinessModel {
                            CodingReadinessSheet(
                                model: readinessModel,
                                vm: vm,
                                accountId: accountId,
                                onStart: {
                                    readinessFollowUp = .start
                                    showReadiness = false
                                },
                                onRoute: { route in
                                    readinessFollowUp = .route(route)
                                    showReadiness = false
                                }
                            )
                        }
                    }
            }
    }

    /// EXP-327: one `…` — Share, Move to board, Unmark duplicate, Delete.
    @ViewBuilder
    private var menuItems: some View {
        if let vm = issueVM, let issue = vm.issue {
            if let shareURL = vm.shareURL {
                GlassMenuItem("Share", icon: AppIcons.uiShare) {
                    shareTarget = ShareTarget(url: shareURL, text: vm.shareText)
                }
            }
            if vm.permissions.isModerator {
                // Move to another board in the same team (EXP-57) — hidden
                // when there's nowhere to go.
                if !vm.moveTargetBoards.isEmpty {
                    GlassMenuItem("Move to board", icon: AppIcons.navBoards) {
                        showMoveBoard = true
                    }
                }
                // Duplicate = status interception (L27): unmark is the only
                // duplicate action here; marking happens via the `duplicate`
                // status picker.
                if issue.duplicateOfId != nil {
                    GlassMenuItem("Unmark duplicate", icon: AppIcons.statusDuplicate) {
                        Task { await vm.unmarkDuplicate() }
                    }
                }
                GlassMenuItem("Delete issue", icon: AppIcons.uiDelete, destructive: true) {
                    showDeleteConfirm = true
                }
            }
        }
    }

    // MARK: - Lifecycle

    private func withLifecycle(_ content: some View) -> some View {
        content
            .onAppear(perform: appear)
            .onDisappear(perform: disappear)
            // EXP-1121: the readiness inputs follow the issue's team, and its
            // board's repository edge (a pick, a retarget) re-reads the
            // server half — the devices ride their own live observation.
            .onChange(of: teamId, initial: true) { _, _ in
                ensureReadinessModel()
            }
            .task(id: "\(issueVM?.board?.id ?? "")|\(issueVM?.board?.repositoryId ?? "")") {
                ensureReadinessModel()
                await readinessModel?.boardChanged(issueVM?.board)
            }
    }

    private func appear() {
        ensureIssueViewModel()
        // Re-arm on every appear: pushing a child screen stops the
        // observations (onDisappear), popping back must resume them.
        issueVM?.startObserving()
        ensureReadinessModel()
    }

    private func disappear() {
        // Belt-and-braces with EditorTextView.willMove(toWindow:) — no
        // first responder may outlive this screen (EXP-246).
        UIApplication.endEditing()
        readinessModel?.stop()
        if let vm = issueVM {
            // Stop synchronously: deferring it behind the async saves
            // could cancel the observers a quick pop-back just re-armed.
            vm.stopObserving()
            Task {
                await vm.saveTitle()
                await vm.commitDescription()
            }
        }
    }

    private func ensureIssueViewModel() {
        guard issueVM == nil else { return }
        let vm = IssueDetailViewModel(
            accountId: accountId,
            issueId: issueId,
            db: deps.db,
            issuesApi: deps.issuesApi,
            attachmentsApi: deps.attachmentsApi,
            labelsApi: deps.labelsApi,
            relationsApi: deps.relationsApi,
            widgetsApi: deps.widgetsApi,
            auth: deps.auth
        )
        issueVM = vm
        vm.startObserving()
        // Opening an issue clears its inbox notifications (EXP-92) — push
        // taps and universal links never pass through the inbox's own
        // mark-read. Fire-and-forget: a failure just leaves the
        // notifications unread.
        if !markedRead {
            markedRead = true
            let notificationsApi = deps.notificationsApi
            let accountId = accountId
            let issueId = issueId
            Task {
                try? await notificationsApi.markReadByIssue(accountId: accountId, issueId: issueId)
            }
        }
    }

    /// Arm (or re-arm) the readiness inputs once the issue's team is known.
    private func ensureReadinessModel() {
        guard let teamId else { return }
        if readinessModel == nil {
            readinessModel = CodingReadinessModel(
                accountId: accountId,
                userId: deps.auth.userId,
                db: deps.db,
                repositoriesApi: deps.repositoriesApi,
                integrationsApi: deps.integrationsApi
            )
        }
        readinessModel?.start(teamId: teamId)
    }

    // MARK: - Actions

    /// EXP-1121: every Start tap. Ready opens the composer; still loading
    /// does nothing; anything missing opens "Ready to code?".
    private func startTapped() {
        guard let readiness, readiness.visible, !readiness.loading else { return }
        if readiness.ready {
            openComposer()
        } else {
            UIApplication.endEditing()
            showReadiness = true
        }
    }

    /// The sheet's ask, run on its dismiss.
    private func runReadinessFollowUp() {
        guard let followUp = readinessFollowUp else { return }
        readinessFollowUp = nil
        switch followUp {
        case .start:
            openComposer()
        case let .route(route):
            switch route {
            // #EXP-1381: the GitHub connect lives on the Repositories page,
            // a board's repository control on the Boards page.
            case .connectGithub:
                guard let teamId else { return }
                pushRoute(.teamSettingsSection(accountId: accountId, teamId: teamId, section: .repositories))
            case .boardSettings:
                guard let teamId else { return }
                pushRoute(.teamSettingsSection(accountId: accountId, teamId: teamId, section: .boards))
            case .openDevices:
                pushRoute(.agents)
            }
        }
    }

    /// EXP-825: Start is NAVIGATION — the composer with this issue picked;
    /// its send (`sessions.send` new root, issueIds = [this]) starts the
    /// session and opens the new thread. The issue's team rides along: opened
    /// from the Inbox or Search it may not be the ACTIVE team, and the
    /// composer's pools are team-scoped.
    private func openComposer() {
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(issueIds: [issueId], teamId: teamId)
        ))
    }

    private func deleteIssue() {
        Task {
            if await issueVM?.deleteIssue() == true {
                dismiss()
            }
        }
    }

    private func promoteMoveTarget() {
        guard let target = pendingMoveTarget else { return }
        pendingMoveTarget = nil
        moveTarget = target
    }
}

/// EXP-1121: what "Ready to code?" asked the screen to do once it closed.
private enum ReadinessFollowUp {
    case start
    case route(CodingReadinessRoute)
}
