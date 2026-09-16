import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the phone's WORK screen — one screen per subject (an issue, or a
/// session) with up to three FACES held as screen state, never as
/// navigation: Issue (today's issue body), Run (the transcript and the steer
/// composer), Changes (the run's live diff, else the issue's PR files).
/// Nav-bar Back pops the whole screen; opening a run from any list opens it
/// on the Run face. The port of the desktop's unified issue/session UI
/// (EXP-877/886); the pure rules are `WorkFaces` (web `lib/work-faces.ts`).
///
/// The nav bar is identical across faces, so it never jumps: back · the
/// title dot + identifier (or the session title) · on the Run face only,
/// Stop / Resume · for issue subjects, the `…` menu. The floating bottom bar
/// is `[left circle] [centre capsule] [right circle]` per face, and the right
/// circle is the face switcher on every face.
struct WorkScreen: View {
    let subject: WorkSubject

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var face: WorkFaceKind
    /// The run the Run and Changes faces show. Follows `WorkFaces.codingTarget`
    /// until the reader picks one from the switcher.
    @State private var shownSessionId: String?
    @State private var userPickedRun = false
    @State private var issueVM: IssueDetailViewModel?
    @State private var subjectModel: WorkSubjectModel?
    /// What the session view last reported (`RunChrome`). Held across a
    /// switch to the Issue face, where the session view is unmounted.
    @State private var runChrome = RunChrome()
    @State private var runRequest: RunRequest?
    @State private var switcherAnchor: CGRect = .zero
    @State private var switcherOpen = false
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
    @State private var showResumeConfirm = false
    @State private var resuming = false
    /// EXP-773: Resume is a COMMAND — the watcher waits for the row the
    /// desktop inserts and the screen swaps it in place.
    @State private var startWatcher = StartedRunWatcher()
    /// EXP-696: whether THIS screen ever saw the run live — the issue-less
    /// auto-pop on the ended edge only fires after that, so a finished run
    /// opened from a list stays browsable.
    @State private var sawLiveSession = false
    @State private var steerEnabled = false
    @State private var markedRead = false
    /// EXP-897 Part 4: the stack / batch / run-tree the subject is entangled
    /// with — ONE badge in the header, ONE overlay behind it, sections per
    /// face.
    @State private var prGraphModel: PrGraphModel?
    @State private var prGraphOpen = false
    /// EXP-917: a refused stack merge from the overlay. The sheet is gone by
    /// the time the server answers, so the refusal is an alert — it used to be
    /// swallowed (`try?`), the one merge on this screen that reported nothing.
    @State private var stackMergeFailure: MergeFailure?

    init(subject: WorkSubject) {
        self.subject = subject
        switch subject {
        case .issue: _face = State(initialValue: .issue)
        case .session: _face = State(initialValue: .run)
        }
    }

    // MARK: - Derived state

    private var issueId: String? { subjectModel?.issueId }
    private var issue: IssueEntity? { issueVM?.issue }

    private var shownSession: CodingSessionEntity? {
        guard let shownSessionId else { return nil }
        return subjectModel?.session(id: shownSessionId)
    }

    private var boundSessionId: String? {
        if case let .session(id) = subject { return id }
        return nil
    }

    /// The run `shownSessionId` follows while the reader has not picked one.
    private var targetSessionId: String? {
        subjectModel?.codingTarget(boundId: boundSessionId)?.id
    }

    /// An issue SUBJECT always has its Issue face, even before its row has
    /// landed — otherwise the fallback rule would flip a freshly opened
    /// issue onto its live run while the issue shape is still syncing. A
    /// session subject grows the face once its row names an issue.
    private var hasIssue: Bool {
        if case .issue = subject { return true }
        return issue != nil
    }
    private var hasRun: Bool { shownSession != nil }

    /// The issue's PR / pushed branch — what the PR row opens.
    private var issueHasChanges: Bool {
        guard let issue else { return false }
        return issue.prUrl?.isEmpty == false || issue.branch?.isEmpty == false
    }

    /// The issue's pull request on GitHub — the Changes face's header action.
    private var prURL: URL? {
        issue?.prUrl.flatMap { URL(string: $0) }
    }

    /// The run's live diff outranks the issue's PR files.
    private var hasChanges: Bool {
        (hasRun && runChrome.hasDiff) || issueHasChanges
    }

    /// EXP-879: the shown run's PUBLISHED results, off its synced row. Like
    /// the run's own diff this is a sub-face of Run — no run of mine, no
    /// results — so it reads `shownSession`, never the issue.
    private var sessionResults: [SessionResultEntry] {
        parseSessionResults(shownSession?.results)
    }

    private var hasResults: Bool { !sessionResults.isEmpty }

    private var availableFaces: [WorkFaceKind] {
        WorkFaces.availableFaces(
            hasIssue: hasIssue, hasRun: hasRun, hasChanges: hasChanges, hasResults: hasResults
        )
    }

    private var runIds: [String] {
        subjectModel?.issueRuns.map(\.id) ?? []
    }

    /// The shown run is mine and its row still lives.
    private var ownLive: Bool {
        guard let shownSession else { return false }
        return CodingSessionOwnership.isOwn(shownSession, userId: deps.auth.userId)
            && shownSession.status != DomainContract.codingSessionStatusEnded
    }

    private var shownEnded: Bool {
        shownSession?.status == DomainContract.codingSessionStatusEnded
    }

    private var resumeDevice: SteerDevice? {
        guard let shownSession, let subjectModel else { return nil }
        return subjectModel.resumeDevice(for: shownSession)
    }

    private var ownEndedResumable: Bool {
        steerEnabled && shownEnded && resumeDevice != nil
    }

    /// Relay on, member, repo-backed board — the Start gate (EXP-240).
    private var canStartCoding: Bool {
        guard let vm = issueVM else { return false }
        return vm.steerConfig?.enabled == true
            && vm.permissions.isMember
            && vm.board?.repositoryId != nil
    }

    private var primaryAction: WorkFaces.PrimaryAction {
        WorkFaces.primaryAction(
            ownLive: ownLive, ownEndedResumable: ownEndedResumable, canStart: canStartCoding
        )
    }

    /// The switcher offers Start coding once the shown run ended for good.
    private var offerStart: Bool {
        shownEnded && !ownEndedResumable && canStartCoding
    }

    private var switcherTargets: [WorkFaces.SwitcherTarget] {
        WorkFaces.switcherTargets(
            faces: availableFaces,
            shown: face,
            runIds: runIds,
            shownRunId: shownSessionId,
            offerStart: offerStart
        )
    }

    private var switcherMode: WorkFaces.SwitcherMode {
        WorkFaces.switcherMode(switcherTargets)
    }

    /// The title dot's tone: none without a LIVE run; on the Run face the
    /// socket phase decides (`phaseDotTone`), elsewhere the synced row.
    private var dotTone: SessionDotTone? {
        guard let shownSession, CodingSessionLiveness.isLive(shownSession) else { return nil }
        if face != .issue {
            return WorkFaces.phaseDotTone(
                live: runChrome.live,
                connecting: runChrome.connecting,
                awaitingInput: runChrome.awaitingInput,
                paused: runChrome.paused,
                stale: runChrome.stale
            ).tone
        }
        let state = CodingSessionDisplayState.of(
            session: shownSession, prState: issue?.prState ?? shownSession.prState
        )
        return SessionStateDot.tone(of: state)
    }

    private var dotPulsing: Bool {
        face != .issue ? runChrome.busy : (shownSession?.agentBusy ?? false)
    }

    private var switcherBadge: WorkFaces.SwitcherBadge? {
        WorkFaces.switcherBadge(shown: face, sessionTone: dotTone, hasChanges: hasChanges)
    }

    /// The identifier for an issue subject, the session's own title for an
    /// issue-less run.
    private var title: String {
        if let identifier = issue?.identifier, !identifier.isEmpty { return identifier }
        if issueId != nil { return "" }
        guard let shownSession else { return "" }
        // EXP-876: a batch names itself after the issues it covers.
        return sessionRowTitle(
            issue: nil,
            session: shownSession,
            batchIssues: subjectModel?.batchIssues ?? []
        )
    }

    /// The `…` menu belongs to issue subjects.
    private var hasIssueMenu: Bool {
        issue != nil
    }

    // MARK: - The PR graph (EXP-897 Part 4)

    private var teamId: String? {
        issueVM?.board?.teamId ?? shownSession?.teamId
    }

    private var prGraph: PrGraph.Graph? {
        prGraphModel?.graph(
            issue: issue,
            session: shownSession,
            // EXP-876: a batch run's covered issues — the pill and its sheet
            // name it before its pull request exists.
            batchIssues: subjectModel?.batchIssues ?? []
        )
    }

    /// The header badge, when there IS a stack or a batch to name.
    @ViewBuilder
    private var prGraphBadge: some View {
        if let graph = prGraph, let kind = PrGraph.badgeKind(graph) {
            PrGraphBadge(kind: kind, positionLabel: graph.positionLabel) {
                prGraphOpen = true
            }
        }
    }

    // MARK: - Body

    var body: some View {
        withOverlays(withLifecycle(withChrome(screenContent)))
    }

    private var screenContent: some View {
        ZStack {
            AppBackground()
            faceBody
        }
    }

    /// The Run and Changes-with-a-diff faces are ONE `AgentSessionView`
    /// identity that only changes its `face`: a remount would detach the
    /// model, and `SteerSessionStore` drops a FINISHED run's model the moment
    /// nothing is attached — its transcript with it.
    @ViewBuilder
    private var faceBody: some View {
        if face == .issue {
            issueFace
        } else if face == .results {
            // EXP-879: BEFORE the session-view branch — a run with a live diff
            // would otherwise swallow its own Results face.
            resultsFace
        } else if let shownSession, face == .run || runChrome.hasDiff {
            sessionView(shownSession, face: face)
        } else if face == .changes {
            changesFace
        } else {
            ProgressView().tint(.white)
        }
    }

    @ViewBuilder
    private var issueFace: some View {
        if let vm = issueVM, let issue = vm.issue {
            IssueFaceView(
                vm: vm,
                issue: issue,
                barTrailing: issueBarTrailing,
                onStartCoding: openComposer,
                onOpenChanges: { switchFace(.changes) },
                switcher: { switcherView }
            )
        } else if let vm = issueVM, vm.loadTimedOut {
            unavailableState(vm: vm)
        } else {
            ProgressView().tint(.white)
        }
    }

    /// The issue's PR files — the Changes face when the run has no live diff
    /// (`faceBody` routes a live diff to the session view).
    @ViewBuilder
    private var changesFace: some View {
        if let issueId, issueHasChanges {
            PrChangesFace(issueId: issueId, reviewMode: false) { switcherView }
        } else {
            ProgressView().tint(.white)
        }
    }

    /// EXP-879: the shown run's published screenshots — one scrolling page,
    /// a band per topic, and the face switcher as its whole bottom bar (no
    /// Stop / Resume, no merge: those belong to Run and Changes).
    @ViewBuilder
    private var resultsFace: some View {
        if hasResults {
            SessionResultsFace(groups: groupSessionResults(sessionResults)) { switcherView }
        } else {
            ProgressView().tint(.white)
        }
    }

    private func sessionView(_ session: CodingSessionEntity, face: WorkFaceKind) -> some View {
        AgentSessionView(
            accountId: accountId,
            session: session,
            face: face,
            request: $runRequest,
            onContinuation: { started in swapIn(started.sessionId) },
            switcher: { switcherView }
        )
        .id(session.id)
    }

    private var switcherView: some View {
        WorkFaceSwitcher(
            mode: switcherMode,
            badge: switcherBadge,
            badgePulsing: dotPulsing,
            anchor: $switcherAnchor,
            menuOpen: $switcherOpen,
            onSelect: select
        )
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

    // MARK: - Nav bar

    private func withChrome(_ content: some View) -> some View {
        content
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    HStack(spacing: 6) {
                        WorkTitle(text: title, tone: dotTone, pulsing: dotPulsing)
                        // EXP-897 Part 4: the ONE stack/batch badge, beside
                        // the title on every face.
                        prGraphBadge
                    }
                }
                ToolbarItemGroup(placement: .topBarTrailing) {
                    if face == .run {
                        runPill
                    }
                    // EXP-895: on the Changes face GitHub rides the HEADER's
                    // action slot — the work bar's leading slot belongs to the
                    // file sheet now.
                    if face == .changes, let url = prURL {
                        Button { openURL(url) } label: {
                            AppIcon(
                                AppIcons.uiExternalLink,
                                size: AppIcon.Size.medium,
                                weight: .medium
                            )
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                        .accessibilityLabel("Open PR on GitHub")
                        .accessibilityIdentifier("changes-github-action")
                    }
                    if hasIssueMenu {
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

    /// EXP-818: the ONE Stop — a small red-tinted glass pill, IDENTICAL
    /// whether this phone hosts the run or only watches it; Resume once it
    /// ended and a machine can take it (EXP-773).
    @ViewBuilder
    private var runPill: some View {
        switch primaryAction {
        case .stop:
            GlassPill(
                "Stop",
                icon: AppIcons.codingStop,
                size: .sm,
                mode: .action { runRequest = .stop },
                tint: DesignTokens.Semantic.red
            )
            .accessibilityLabel("Stop the agent and end the session")
            .accessibilityIdentifier("session-stop")
        case .resume:
            GlassPill(
                "Resume",
                icon: AppIcons.runResume,
                size: .sm,
                mode: .action { showResumeConfirm = true },
                enabled: !resuming
            )
            .accessibilityIdentifier("resume-run")
        case .start, .none:
            EmptyView()
        }
    }

    // MARK: - Overlays, sheets, alerts

    private func withOverlays(_ content: some View) -> some View {
        content
            // EXP-687: the popups ride in-view overlays on THIS root, not
            // presentations launched from inside a bar item.
            .glassMenuOverlay(isPresented: $menuOpen, anchor: menuAnchor, presentation: .inline) {
                issueMenuItems
            }
            .glassMenuOverlay(
                isPresented: $switcherOpen, anchor: switcherAnchor, presentation: .inline
            ) {
                switcherMenuItems
            }
            // Each presentation lives on its OWN node (EXP-240).
            .background {
                Color.clear
                    .sheet(item: $shareTarget) { target in
                        ActivityShareSheet(items: [target.text, target.url])
                    }
            }
            .background {
                Color.clear
                    .sheet(isPresented: $showMoveBoard, onDismiss: promoteMoveTarget) {
                        if let vm = issueVM, let issue = vm.issue {
                            MoveBoardPickerSheet(
                                boards: vm.moveTargetBoards,
                                selectedId: issue.boardId,
                                onSelect: { target in pendingMoveTarget = target }
                            )
                        }
                    }
            }
            .moveBoardConfirm(
                target: $moveTarget,
                identifier: issue?.identifier,
                onConfirm: { target in Task { await issueVM?.moveToBoard(target.id) } }
            )
            .alert("Delete Issue", isPresented: $showDeleteConfirm) {
                Button("Delete", role: .destructive) { deleteIssue() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This action cannot be undone.")
            }
            // EXP-897 Part 4: the badge's overlay — its sections follow the
            // face underneath.
            .background {
                Color.clear
                    .sheet(isPresented: $prGraphOpen) {
                        if let graph = prGraph {
                            PrGraphSheet(
                                graph: graph,
                                face: face,
                                onOpenIssue: { id in
                                    prGraphOpen = false
                                    deps.deepLinkBus.navigateToIssue(id, accountId: accountId)
                                },
                                onOpenRun: { id in
                                    prGraphOpen = false
                                    swapIn(id)
                                },
                                onMergeStack: { id in
                                    prGraphOpen = false
                                    mergeStack(issueId: id)
                                }
                            )
                        }
                    }
            }
            .alert("Resume this run?", isPresented: $showResumeConfirm) {
                Button("Resume") { resumeRun() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Reopens the run on the machine that ran it, in the same worktree, and continues where the agent stopped.")
            }
            .alert(
                "Couldn't merge the stack",
                isPresented: Binding(
                    get: { stackMergeFailure != nil },
                    set: { if !$0 { stackMergeFailure = nil } }
                )
            ) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(stackMergeFailure?.message ?? "")
            }
    }

    /// EXP-327: one `…` — Share, Move to board, Unmark duplicate, Delete.
    /// Usage and "Open issue" left it (EXP-893): usage is the composer's
    /// ring, and the issue is a face away.
    @ViewBuilder
    private var issueMenuItems: some View {
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

    /// The switcher menu: Start coding · Issue · Run (or one row per own run
    /// with two or more, EXP-886's byline, a check on the shown one) ·
    /// Changes with its `+A -D` when it is a live diff.
    @ViewBuilder
    private var switcherMenuItems: some View {
        ForEach(switcherTargets, id: \.self) { target in
            switcherMenuRow(target)
        }
    }

    @ViewBuilder
    private func switcherMenuRow(_ target: WorkFaces.SwitcherTarget) -> some View {
        switch target {
        case .startCoding:
            GlassMenuItem(WorkFaces.startCodingLabel, icon: AppIcons.actionRun) {
                select(target)
            }
        case .face(.changes):
            GlassMenuItem(WorkFaces.changesFaceLabel, icon: AppIcons.codingDiff) {
                select(target)
            }
            .overlay(alignment: .trailing) { diffCounts }
        case let .face(face):
            GlassMenuItem(WorkFaces.faceLabel(face), icon: WorkFaceSwitcher.icon(target)) {
                select(target)
            }
        case let .run(id):
            runMenuRow(id: id)
        }
    }

    /// `+A -D` beside the Changes row, only from a live diff.
    @ViewBuilder
    private var diffCounts: some View {
        if hasRun, runChrome.hasDiff {
            DiffCountsLabel(
                additions: runChrome.additions, deletions: runChrome.deletions
            )
            .padding(.trailing, GlassMenuTokens.itemHPadding)
            .allowsHitTesting(false)
        }
    }

    /// One run of mine as `<device> · Live` / `<device> · 2h ago` (the ×4
    /// `PastRuns` byline); the run on show wears the check, a live one the
    /// running glyph.
    @ViewBuilder
    private func runMenuRow(id: String) -> some View {
        if let run = subjectModel?.issueRuns.first(where: { $0.id == id }) {
            let onShow = id == shownSessionId
            let live = PastRuns.isLiveRunStatus(run.session.status)
            GlassMenuItem(
                PastRuns.byline(
                    device: run.device.displayLabel,
                    relativeTime: PastRuns.issueRunWhen(
                        run.session,
                        endedRelative: relativeWireDate(PastRuns.endedAt(run.session))
                    )
                ),
                icon: onShow ? AppIcons.uiCheck : (live ? AppIcons.codingRunning : nil)
            ) {
                select(.run(id: id))
            }
        }
    }

    // MARK: - Lifecycle

    private func withLifecycle(_ content: some View) -> some View {
        content
            .onAppear(perform: appear)
            .onDisappear(perform: disappear)
            // The relay config gates Resume.
            .task(id: accountId) {
                let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
                steerEnabled = config.enabled
            }
            // A session subject learns its issue off its row.
            .onChange(of: subjectModel?.issueId, initial: true) { _, _ in
                ensureIssueViewModel()
                ensurePrGraphModel()
            }
            .onChange(of: teamId) { _, _ in
                ensurePrGraphModel()
            }
            // `shownSessionId` follows the coding target until a pick.
            .onChange(of: targetSessionId, initial: true) { _, id in
                targetChanged(id)
            }
            // A face that vanished under the reader lands on its fallback.
            .onChange(of: availableFaces) { _, faces in
                facesChanged(faces)
            }
            // EXP-893: the session view's report, held across the Issue face
            // (where the emitter is unmounted and the preference resets).
            .onPreferenceChange(RunChrome.Key.self) { chrome in
                chromeChanged(chrome)
            }
            .onChange(of: shownSessionId, initial: true) { _, id in
                runChrome = RunChrome()
                sawLiveSession = false
                subjectModel?.observeShown(id: id)
            }
            // The ended edge: an issue-bound subject stays; an issue-less one
            // keeps the native auto-pop (EXP-696).
            .onChange(of: shownEnded) { _, ended in
                endedChanged(ended)
            }
            // The desktop picked the resume up — swap the continuation in.
            .onChange(of: startWatcher.startedSession) { _, started in
                if let started {
                    startWatcher.startedSession = nil
                    swapIn(started.sessionId)
                }
            }
    }

    /// EXP-897: merging the whole stack from the overlay — one call on the
    /// BOTTOM entry's issue; the server resolves the top and merges every
    /// unmerged member below it.
    ///
    /// EXP-917: a refusal is reported (the Reviews list captions its row; here
    /// the overlay has already closed, so an alert carries it). A stack
    /// refusal offers no Fix conflicts — the recovery run takes ONE pull
    /// request — the same rule as every other client's stack merge.
    private func mergeStack(issueId: String) {
        let accountId = accountId
        let issuesApi = deps.issuesApi
        Task {
            do {
                try await issuesApi.mergePr(
                    accountId: accountId, issueId: issueId, mergeStack: true
                )
            } catch {
                stackMergeFailure = MergeFailure(error: error)
            }
        }
    }

    private func appear() {
        if subjectModel == nil {
            subjectModel = WorkSubjectModel(
                accountId: accountId,
                subject: subject,
                currentUserId: deps.auth.userId,
                db: deps.db
            )
        }
        // Re-arm on every appear: pushing a child screen stops the
        // observations (onDisappear), popping back must resume them.
        subjectModel?.observeShown(id: shownSessionId)
        subjectModel?.start()
        ensureIssueViewModel()
        issueVM?.startObserving()
        ensurePrGraphModel()
    }

    /// The graph's rows, re-armed on every appear like every other observation
    /// here (the issue and the team may resolve after the first pass).
    private func ensurePrGraphModel() {
        if prGraphModel == nil {
            prGraphModel = PrGraphModel(accountId: accountId, db: deps.db)
        }
        prGraphModel?.start(issueId: issueId, teamId: teamId)
    }

    private func disappear() {
        // Belt-and-braces with EditorTextView.willMove(toWindow:) — no
        // first responder may outlive this screen (EXP-246).
        UIApplication.endEditing()
        subjectModel?.stop()
        prGraphModel?.stop()
        startWatcher.stop()
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

    /// The issue's own view model, once the subject names an issue.
    private func ensureIssueViewModel() {
        guard issueVM == nil, let issueId else { return }
        let vm = IssueDetailViewModel(
            accountId: accountId,
            issueId: issueId,
            db: deps.db,
            issuesApi: deps.issuesApi,
            attachmentsApi: deps.attachmentsApi,
            labelsApi: deps.labelsApi,
            relationsApi: deps.relationsApi,
            steerApi: deps.steerApi,
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
            Task {
                try? await notificationsApi.markReadByIssue(accountId: accountId, issueId: issueId)
            }
        }
    }

    private func targetChanged(_ id: String?) {
        guard !userPickedRun else { return }
        if let id { shownSessionId = id }
    }

    private func facesChanged(_ faces: [WorkFaceKind]) {
        guard !faces.isEmpty, !faces.contains(face) else { return }
        if let next = WorkFaces.fallbackFace(shown: face, available: faces) {
            face = next
        }
    }

    private func chromeChanged(_ chrome: RunChrome) {
        // The Issue face has no emitter: the reset it triggers is not news.
        guard face != .issue else { return }
        runChrome = chrome
    }

    private func endedChanged(_ ended: Bool) {
        if !ended {
            if shownSession != nil { sawLiveSession = true }
            return
        }
        // Issue-bound: stay. The pill flips to Resume and the composer retires
        // on its own; `facesChanged` lands a vanished diff back on the Run face.
        guard issueId == nil else { return }
        // EXP-849/773: a resume or switch ENDS this run on purpose — hold
        // for the continuation instead of leaving.
        guard startWatcher.sentCaption == nil, !resuming, !runChrome.continuationPending else {
            return
        }
        guard sawLiveSession else { return }
        dismiss()
    }

    // MARK: - Actions

    private func switchFace(_ next: WorkFaceKind) {
        UIApplication.endEditing()
        face = next
    }

    private func select(_ target: WorkFaces.SwitcherTarget) {
        switch target {
        case let .face(next):
            switchFace(next)
        case let .run(id):
            userPickedRun = true
            shownSessionId = id
            switchFace(.run)
        case .startCoding:
            openComposer()
        }
    }

    /// A resumed / switched run's continuation: shown in place, pinned.
    private func swapIn(_ sessionId: String) {
        userPickedRun = true
        shownSessionId = sessionId
        face = .run
    }

    /// The Issue face's trailing circle: the switcher once a run of mine
    /// exists, else the Start circle behind its gates (EXP-240).
    private var issueBarTrailing: IssueBarTrailing {
        if hasRun {
            if case .hidden = switcherMode { return .hidden }
            return .switcher
        }
        guard canStartCoding, let vm = issueVM else { return .hidden }
        guard let devices = vm.steerDevices else { return .hidden }
        return devices.isEmpty ? .noDevices : .start
    }

    /// EXP-825: Start coding is NAVIGATION — the Agent page composer with
    /// this issue pre-checked; the watcher there pushes the run once the
    /// desktop picks it up.
    private func openComposer() {
        guard let issueId else { return }
        // The issue's team rides along: opened from the Inbox, Reviews or
        // Search it may not be the ACTIVE team, and the composer's pools are
        // team-scoped (the chip would vanish and the button read "Start chat").
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(issueIds: [issueId], teamId: issueVM?.board?.teamId)
        ))
    }

    /// Pick this finished run up again on the machine that ran it — the SAME
    /// `steer.startSession({resumeSessionId})` the list rows used to send.
    /// A start is a COMMAND, so the watcher waits for the row the desktop
    /// inserts and the screen swaps that session in.
    private func resumeRun() {
        guard let shownSession, let device = resumeDevice, !resuming else { return }
        resuming = true
        startWatcher.sending()
        let sessionId = shownSession.id
        Task {
            do {
                try await deps.steerApi.resumeSession(
                    accountId: accountId,
                    sessionId: sessionId,
                    deviceId: device.deviceId
                )
                startWatcher.begin(
                    key: .resumed(fromId: sessionId),
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.userFacingMessage)
            }
            resuming = false
        }
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
