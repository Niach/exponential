import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the phone's WORK screen — one screen per subject (an issue, or a
/// session) with up to three FACES held as screen state, never as
/// navigation: Issue (today's issue body), Run (the transcript and the steer
/// composer), Guide (EXP-1251: the run's report over its diff — the run's
/// live diff, else the issue's PR files).
/// Nav-bar Back pops the whole screen; opening a run from any list opens it
/// on the Run face. The port of the desktop's unified issue/session UI
/// (EXP-877/886); the pure rules are `WorkFaces` (web `lib/work-faces.ts`).
///
/// The nav bar is identical across faces, so it never jumps: back · the
/// identifier (or the session title; EXP-1162: no dot, the state rides the
/// face tabs) · on the Run face only,
/// Stop / Resume · for issue subjects, the `…` menu. EXP-1150: directly under
/// it the face TABS (`WorkFaceTabs`, two or more faces) sit at the same place
/// on every face — tapping the selected `Runs` tab opens the run menu — and
/// (EXP-1152) the faces are PAGES under it (`WorkFacePager`) that follow the
/// finger to the neighbouring face. The floating bottom bar is per face:
/// Issue `[Properties][+ Comment][Start]`, Run `[usage][composer][Start once
/// ended for good]`, Guide `[Merge]` (a section page `[files][Merge]`).
/// EXP-1154: the ONE Merge (`WorkMergePill`, the solid white capsule; "Merge
/// stack" on an open-stack member) is on the bar of EVERY face while there is
/// a PR to merge; EXP-1191: on the Issue and Run bars as a 52pt glass circle
/// right of the centre capsule (`style: .circle`). A Reviews row opens this
/// screen on its Guide face; Close PR is the `…` menu's.
struct WorkScreen: View {
    let subject: WorkSubject

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @Environment(\.motion) private var motion
    @Environment(\.toaster) private var toaster
    @State private var face: WorkFaceKind
    /// The run the Run and Changes faces show. Follows `WorkFaces.codingTarget`
    /// until the reader picks one from the run menu.
    @State private var shownSessionId: String?
    @State private var userPickedRun = false
    /// EXP-933: the requested `initialFace` while it is not available YET
    /// (a Results push lands before the issue's runs synced): the fallback
    /// holds off until the runs were read (and, for Changes / Results, the
    /// issue row too), then lets go either way (`WorkFaces.holdsPendingFace`).
    @State private var pendingInitialFace: WorkFaceKind?
    @State private var issueVM: IssueDetailViewModel?
    @State private var subjectModel: WorkSubjectModel?
    /// What the session view last reported (`RunChrome`). Held while the
    /// pager has the Run page unmounted (its preference resets then).
    @State private var runChrome = RunChrome()
    @State private var runRequest: RunRequest?
    /// EXP-1150: the run menu, anchored under the `Runs` tab — opened by
    /// tapping that tab while it is already selected.
    @State private var runsMenuAnchor: CGRect = .zero
    @State private var runsMenuOpen = false
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
    /// EXP-773/849/935: Resume and the account switch are COMMANDS — the
    /// screen holds while the desktop answers with the successor row and
    /// swaps it in place. Owned HERE (not by the run's own view, which is
    /// unmounted on the Issue face) so the hold survives every face.
    @State private var continuation = RunContinuation()
    /// EXP-696: whether THIS screen ever saw the run live — the issue-less
    /// auto-pop on the ended edge only fires after that, so a finished run
    /// opened from a list stays browsable.
    @State private var sawLiveSession = false
    @State private var steerEnabled = false
    /// `steerEnabled` is an answer, not the `false` default — the readiness
    /// model reads nil (loading) until then.
    @State private var steerConfigLoaded = false
    @State private var markedRead = false
    /// EXP-1121: the "Ready to code?" inputs (repositories, GitHub, live
    /// devices) — shared by the Start circles and the sheet.
    @State private var readinessModel: CodingReadinessModel?
    @State private var showReadiness = false
    /// What the sheet asked for, run once it has dismissed (a push from
    /// under a dismissing sheet is dropped).
    @State private var readinessFollowUp: ReadinessFollowUp?
    /// EXP-876: the issues a multi-issue run covers, opened from its title.
    @State private var coveredIssuesOpen = false
    /// EXP-897 Part 4/SLOP-3: the blockers / batch / stack the subject is
    /// entangled with: ONE badge in the header, ONE overlay behind it.
    @State private var prGraphModel: PrGraphModel?
    @State private var prGraphOpen = false
    /// EXP-952: the issue's PR / pushed-branch files, read only when there is
    /// no live diff to draw. The model lives HERE, not inside the Guide face
    /// (Android's `WorkScreen` owns its `ChangesViewModel` the same way), so
    /// the files load before the face was ever opened.
    @State private var prChangesModel: ChangesViewModel?
    /// EXP-1251: the Guide's section page in view (nil = the Guide itself)
    /// and the file its sheet picked.
    @State private var guideSection: GuideSectionKey?
    @State private var changesFocusPath: String?
    /// EXP-1248: a stack row's "Merge through here", waiting on its confirm.
    @State private var mergeThroughRequest: PrStack.StackMergeConfirm?
    /// EXP-1154: the Merge capsule's shared state (every face mounts one).
    @State private var mergeState = WorkMergeState()
    /// EXP-1154: the Close PR confirm (the `…` menu's) and its flight.
    @State private var showClosePrConfirm = false
    @State private var closingPr = false
    /// EXP-1154: the open PR's GitHub title + body, the Guide while the PR
    /// has no run report yet. Fetched only while the Guide shows, once per
    /// issue, and held while the screen lives (a failure retries on the next
    /// Guide visit).
    @State private var prDescription = PrDescriptionLoad.idle
    /// EXP-1154: the arrival asked for the Guide (a Reviews row): honoured
    /// for a merged / closed PR too, like Android's `changesRequested`.
    private let changesRequested: Bool
    /// EXP-1162: whether the Issue face's title row has scrolled under the
    /// header band — flipped ONLY on the edge (`titleEdges`), so a scroll
    /// never re-renders the screen.
    @State private var issueTitleScrolledAway = false
    /// The two edges the collapse compares, outside SwiftUI's diffing.
    @State private var titleEdges = TitleCollapseTracker()
    /// `initialFace` opens an issue subject on that face (a deep link's
    /// Guide); unavailable faces fall back as usual.
    init(subject: WorkSubject, initialFace: WorkFaceKind = .issue) {
        self.subject = subject
        changesRequested = initialFace == .guide
        switch subject {
        case .issue:
            _face = State(initialValue: initialFace)
            _pendingInitialFace = State(initialValue: initialFace == .issue ? nil : initialFace)
        case .session:
            // The run's faces only: `.issue` lands on Run through the same
            // rule.
            let first = WorkFaces.fallbackFace(
                shown: initialFace, available: [.run, .guide]
            ) ?? .run
            _face = State(initialValue: first)
            _pendingInitialFace = State(initialValue: first == .run ? nil : first)
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

    /// The issue's pull request on GitHub — the Guide face's header action.
    private var prURL: URL? {
        issue?.prUrl.flatMap { URL(string: $0) }
    }

    /// EXP-1154: the issue's own diff source, web parity: an OPEN PR or a
    /// pushed branch with no PR yet; a merged / closed PR only when the
    /// arrival asked for the Guide (a Reviews row's `initialFace`).
    private var issueChangesAvailable: Bool {
        guard let issue else { return false }
        let hasPr = issue.prUrl?.isEmpty == false
        let hasBranch = issue.branch?.isEmpty == false
        if hasPr && issue.prState == DomainContract.prStateOpen { return true }
        if !hasPr && hasBranch { return true }
        return changesRequested && (hasPr || hasBranch)
    }

    /// The run's live diff outranks the issue's PR files.
    private var hasDiff: Bool {
        (hasRun && runHasDiff) || issueChangesAvailable
    }

    /// EXP-932: the shown run's CURRENT worktree diff, read straight off its
    /// retained model — the socket outlives the session VIEW, so this is right
    /// on the Issue face too, where that view (and the `RunChrome` it reports)
    /// is unmounted and its last snapshot would be a stale, partial count.
    private var shownDiff: String? { shownModel?.latestDiff }

    /// The shown run's retained model (the socket owner), if the store still
    /// holds one — nil once it was reaped or before a run is shown.
    private var shownModel: AgentSessionModel? {
        guard let shownSessionId else { return nil }
        return deps.steerSessions.peek(accountId: accountId, sessionId: shownSessionId)
    }

    /// EXP-932: whether the run HAS a diff — the mounted view's report, else
    /// the retained model's own answer. Both, because either can be the only
    /// one there is: the view knows while it is up, the model while it is not
    /// (and a model the store already reaped leaves the last report standing).
    private var runHasDiff: Bool {
        runChrome.hasDiff || shownDiff != nil
    }

    /// EXP-933: the run whose Results this screen shows. An ISSUE shows its
    /// report for everyone (`WorkFaces.issueResultsRun` over every member's
    /// runs: my coding target when it has results, else the newest run with
    /// any) — unless the reader picked a run of their own that has results.
    /// An issue-less run shows its own.
    private var resultsSession: CodingSessionEntity? {
        guard issueId != nil else { return shownSession }
        if userPickedRun, let shownSession, hasSessionResults(shownSession.results) {
            return shownSession
        }
        return subjectModel?.resultsRun(boundId: boundSessionId, issuePrUrl: issue?.prUrl)
    }

    /// EXP-879/933: the results run's PUBLISHED report — pictures and each
    /// topic's text, grouped — off its synced row. EXP-1251: an issue shows
    /// only its own PR's topics plus the untagged ones (`sessionResultsForPr`).
    private var sessionResultGroups: [SessionResultGroup] {
        guard issueId != nil else { return parseSessionResultGroups(resultsSession?.results) }
        return sessionResultGroupsForPr(resultsSession?.results, prUrl: issue?.prUrl)
    }

    private var hasRunResults: Bool { !sessionResultGroups.isEmpty }

    /// EXP-1154: the issue's PR is open — the Guide then exists even without
    /// a run report (the PR body stands in).
    private var issuePrOpen: Bool {
        issueId != nil
            && issue?.prState == DomainContract.prStateOpen
            && issue?.prUrl?.isEmpty == false
    }

    /// EXP-1154: the report = the run's report, else an open PR's body.
    private var hasResults: Bool { hasRunResults || issuePrOpen }

    /// EXP-1251: the files the Guide counts — the run's live diff, else the
    /// issue's loaded PR files.
    private var changesFiles: [Diff.File]? {
        if runHasDiff, let model = shownModel, model.latestDiff != nil {
            return model.parsedDiff.files
        }
        return prChangesModel?.loadedFiles
    }

    private var availableFaces: [WorkFaceKind] {
        WorkFaces.availableFaces(
            hasIssue: hasIssue, hasRun: hasRun, hasResults: hasResults, hasDiff: hasDiff
        )
    }

    /// EXP-974: the run menu's rows. An issue-bound subject lists the issue's
    /// own runs (`PastRuns.issueRuns`, whose rows already include its
    /// resumes); an issue-LESS one (a chat, action or batch run) lists the
    /// shown run's resume chain (`RunChain.chain`, newest first) — a resumed
    /// run and its successor share ONE toggle, and this menu is where the
    /// reader picks between them.
    private var menuRuns: [WorkSubjectModel.IssueRun] {
        guard let subjectModel else { return [] }
        return issueId != nil ? subjectModel.issueRuns : subjectModel.chainRuns
    }

    private var runIds: [String] {
        menuRuns.map(\.id)
    }

    /// EXP-886: two or more runs — the Run tab reads `Runs` and reselecting
    /// it opens the run menu.
    private var multipleRuns: Bool { runIds.count >= 2 }

    /// The shown run is mine and its row still lives.
    private var ownLive: Bool {
        guard let shownSession else { return false }
        // EXP-888: a sweep end is not an end — the run keeps its Stop.
        return CodingSessionOwnership.isOwn(shownSession, userId: deps.auth.userId)
            && !PastRuns.hasEnded(shownSession)
    }

    private var shownEnded: Bool {
        guard let shownSession else { return false }
        return PastRuns.hasEnded(shownSession)
    }

    private var resumeDevice: SteerDevice? {
        guard let shownSession, let subjectModel else { return nil }
        return subjectModel.resumeDevice(for: shownSession)
    }

    private var ownEndedResumable: Bool {
        steerEnabled && shownEnded && resumeDevice != nil
    }

    /// EXP-1121: whether Start coding can run right now, and what is missing
    /// when not. nil before the issue's view model exists.
    private var readiness: CodingReadiness.Readiness? {
        guard let vm = issueVM, let readinessModel else { return nil }
        return readinessModel.readiness(
            vm: vm, remoteStartEnabled: steerConfigLoaded ? steerEnabled : nil
        )
    }

    /// Member + remote start on — Start coding renders (EXP-1121 dropped the
    /// repo/device gates: a missing step is the checklist's job now).
    private var startVisible: Bool { readiness?.visible == true }

    private var primaryAction: WorkFaces.PrimaryAction {
        WorkFaces.primaryAction(
            ownLive: ownLive, ownEndedResumable: ownEndedResumable, canStart: startVisible
        )
    }

    /// EXP-1150: the Run bar offers Start coding once the shown run — mine,
    /// on an issue subject — ended for good (nothing to resume).
    private var offerStart: Bool {
        guard issueId != nil, let shownSession else { return false }
        return shownEnded && !ownEndedResumable && startVisible
            && CodingSessionOwnership.isOwn(shownSession, userId: deps.auth.userId)
    }

    /// EXP-1162: the face tabs' state dots (`DetailChrome.faceDots`) — the
    /// state the title no longer wears. Live = the shown run's synced row is
    /// live; needs input = that row's `needsInput`; the pull request = the
    /// issue's, else an issue-less run's own.
    private var faceDots: [WorkFaceKind: SessionDotTone] {
        let runLive = shownSession.map { CodingSessionLiveness.isLive($0) } ?? false
        let prState = issue?.prState ?? (issueId == nil ? shownSession?.prState : nil)
        return DetailChrome.faceDots(
            faces: availableFaces,
            runLive: runLive,
            needsInput: shownSession?.needsInput ?? false,
            prOpen: prState == DomainContract.prStateOpen
        )
    }

    /// EXP-1184: what the shown run is doing, for the Run tab's mark — the
    /// ×4 `session-display.json` rule over the live synced row, with the
    /// VIEWER's busy signal (`AgentSessionModel.agentWorking`, while its socket
    /// is live) as `agentBusy`, else the synced `agent_busy`. Nil for no live
    /// run (the tab then carries no mark at all).
    private var runState: CodingSessionDisplayState? {
        guard let shownSession, CodingSessionLiveness.isLive(shownSession) else { return nil }
        let prState = issue?.prState ?? (issueId == nil ? shownSession.prState : nil)
        let viewerBusy: Bool? = shownModel.flatMap { model in
            model.phase == .live ? model.agentWorking : nil
        }
        return CodingSessionDisplayState.of(
            session: shownSession, prState: prState, agentBusy: viewerBusy
        )
    }

    /// EXP-952: whether the issue's PR files are the Guide's diff source —
    /// an issue with changes and NO live diff on the shown run (Android's
    /// `hasChanges && latestDiff == null && issueId != null`). The key the
    /// screen-owned `ChangesViewModel` lives by.
    private var wantsPrChangesModel: Bool {
        issueId != nil && issueChangesAvailable && shownDiff == nil
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

    /// The `…` menu belongs to issue subjects — and, EXP-934, to the ISSUE
    /// face alone (`WorkFaces.faceShowsContextMenu`): Share, Move to board and
    /// Delete issue act on a subject the Run, Changes and Results faces are
    /// not even showing, and those keep the run's own verb instead.
    private var hasIssueMenu: Bool {
        issue != nil && WorkFaces.faceShowsContextMenu(face)
    }

    private var teamId: String? {
        issueVM?.board?.teamId ?? shownSession?.teamId
    }

    // MARK: - The PR graph (EXP-897 Part 4)

    private var prGraph: PrGraph.Graph? {
        prGraphModel?.graph(
            issue: issue,
            session: shownSession,
            // EXP-876: a batch run's covered issues, so the badge and its
            // sheet name it before its pull request exists.
            batchIssues: subjectModel?.batchIssues ?? []
        )
    }

    /// SLOP-16 r2: the header's graph icon button, when there IS a stack, a
    /// batch or (EXP-1097) an open blocker to name, the same on every face.
    @ViewBuilder
    private var prGraphBadge: some View {
        if let graph = prGraph, let chip = PrGraph.badgeChip(graph) {
            let shape = PrGraph.badgeShape(graph)
            PrGraphBadge(
                shape: shape,
                count: chip.count,
                accessibilityName: PrGraphBadge.accessibilityName(shape)
            ) {
                prGraphOpen = true
            }
        }
    }

    /// The graph's rows, re-armed on every appear like every other observation
    /// here (the issue may resolve after the first pass).
    private func ensurePrGraphModel() {
        if prGraphModel == nil {
            prGraphModel = PrGraphModel(accountId: accountId, db: deps.db)
        }
        prGraphModel?.start(issueId: issueId)
    }

    /// SLOP-16 r3: a Related work PR row. The subject's own PR is this
    /// screen's Guide face; any other one opens on its own.
    private func openPullRequest(_ entry: PrGraph.Entry, subject graph: PrGraph.Graph) {
        if entry.id == graph.entry?.id, availableFaces.contains(.guide) {
            selectFace(.guide)
        } else {
            deps.deepLinkBus.navigateToIssue(
                entry.representative.id, accountId: accountId, face: .guide
            )
        }
    }

    // MARK: - Covered issues (EXP-876)

    /// The issues a multi-issue (batch) run covers; empty on every other
    /// subject, where the header draws the plain title.
    private var coveredIssues: [IssueEntity] {
        guard issueId == nil, let batch = subjectModel?.batchIssues, batch.count > 1 else {
            return []
        }
        return batch
    }

    /// The nav bar's title: the plain `WorkTitle`, or — on a multi-issue run —
    /// its `EXP-874 +2` as the stacked issue chip that opens the issues it
    /// covers.
    @ViewBuilder
    private var principalTitle: some View {
        if coveredIssues.isEmpty {
            WorkTitle(
                text: title,
                issueTitle: issue?.title,
                collapsed: titleCollapsed
            )
        } else {
            IssueChipStack {
                IssueChip(
                    identifier: title,
                    title: nil,
                    iconName: nil,
                    statusColor: nil,
                    size: .sm,
                    onTap: { coveredIssuesOpen = true }
                )
            }
            .accessibilityLabel(CoveredIssuesSheet.runTitle)
            .accessibilityIdentifier("work-covered-issues")
        }
    }

    /// EXP-1162: the nav-bar title's collapse (`DetailChrome`). Only the
    /// Issue face has a title row of its own; every other face is collapsed.
    private var titleCollapsed: Bool {
        face == .issue ? issueTitleScrolledAway : true
    }

    /// One edge moved (the title row on scroll, the band on layout): re-run
    /// the rule and write the state only when its answer flips.
    private func titleEdgesChanged() {
        let away = titleEdges.collapsed
        if away != issueTitleScrolledAway { issueTitleScrolledAway = away }
    }

    private var coveredIssuesSheet: some View {
        CoveredIssuesSheet(title: CoveredIssuesSheet.runTitle, issues: coveredIssues) { id in
            coveredIssuesOpen = false
            deps.deepLinkBus.navigateToIssue(id, accountId: accountId)
        }
    }

    // MARK: - Body

    var body: some View {
        withOverlays(withLifecycle(withChrome(screenContent)))
            // EXP-1248: a Stack card row's "Merge through here" confirm.
            .modifier(StackMergeThroughHost(request: $mergeThroughRequest, state: $mergeState))
    }

    /// EXP-1150: the tab strip OUTSIDE every face, so it never jumps, and
    /// (EXP-1152) the face pager under it. Standalone, the strip is part of
    /// the HEADER BAND (title row, then tabs, one hairline under both — see
    /// `workHeaderBand`).
    private var screenContent: some View {
        ZStack {
            AppBackground()
            facePager
                .workHeaderBand(onBottom: { bottom in
                    titleEdges.headerBottom = bottom
                    titleEdgesChanged()
                }) { faceTabs }
                .onPreferenceChange(IssueTitleRowBottomKey.self) { bottom in
                    titleEdges.titleBottom = bottom
                    titleEdgesChanged()
                }
        }
    }

    private var faceTabs: some View {
        WorkFaceTabs(
            faces: availableFaces,
            shown: face,
            multipleRuns: multipleRuns,
            dots: faceDots,
            runAgent: shownSession?.agent,
            runState: runState,
            runsAnchor: $runsMenuAnchor,
            onSelect: selectFace,
            onReselectRuns: toggleRunsMenu
        )
    }

    // MARK: - Merge (EXP-1154: the bottom bar's white capsule, every face)

    /// The white Merge capsule on `page`, or nothing while there is no PR to
    /// merge.
    @ViewBuilder
    private func mergeCapsule(
        on page: WorkFaceKind,
        style: WorkMergePill.Style = .capsule
    ) -> some View {
        if let mergeTarget {
            let mergeRow = mergeIssue(for: mergeTarget)
            WorkMergePill(
                target: mergeTarget,
                issue: mergeRow,
                prIssues: mergeRow.flatMap { prGraphModel?.stackPool(for: $0) } ?? [],
                steerEnabled: steerEnabled,
                runPrNumber: shownSession?.prNumber,
                state: $mergeState,
                identifier: page == .guide ? "work-merge-pr" : "work-merge-pr-\(page.rawValue)",
                style: style
            )
            .id(mergeTargetKey(mergeTarget))
        }
    }

    /// EXP-1191: the Issue and Run bars' Merge CIRCLE, right of their
    /// centre capsule.
    private func mergeCircle(on page: WorkFaceKind) -> AnyView? {
        mergeTarget == nil ? nil : AnyView(mergeCapsule(on: page, style: .circle))
    }

    /// What the Merge capsule merges: the shown run's own target while the
    /// run can merge (its retained model), else the issue's open PR for a
    /// member. nil = no pill.
    private var mergeTarget: MergeTarget? {
        if let model = shownModel, model.canMerge, let target = model.mergeTarget {
            return target
        }
        if let issue, issueVM?.permissions.isMember == true,
           issue.prState == DomainContract.prStateOpen,
           issue.prUrl?.isEmpty == false {
            return .issue(issueId: issue.id)
        }
        return nil
    }

    private func mergeIssue(for target: MergeTarget) -> IssueEntity? {
        guard case let .issue(id) = target else { return nil }
        if let issue, issue.id == id { return issue }
        if let row = shownModel?.mergeIssue, row.id == id { return row }
        return prGraphModel?.issue(id: id)
    }

    private func mergeTargetKey(_ target: MergeTarget) -> String {
        switch target {
        case let .issue(id): "issue:\(id)"
        case let .session(id): "session:\(id)"
        }
    }

    /// EXP-1152: every face is a PAGE; a settled drag moves `face` through
    /// `switchFace` like a tab tap does (keyboard down, overlays closed).
    private var facePager: some View {
        WorkFacePager(
            faces: availableFaces,
            selection: Binding(get: { face }, set: { switchFace($0) })
        ) { page in
            facePage(page)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// One face's page. The Run page is the ONE `AgentSessionView` (the Stop
    /// `request`, the `RunChrome` report); the Guide reads the run's live
    /// diff off the same retained model (`RunModelClaim`), else the issue's
    /// PR files.
    @ViewBuilder
    private func facePage(_ page: WorkFaceKind) -> some View {
        switch page {
        case .issue:
            issueFace
        case .run:
            if let shownSession {
                sessionView(shownSession)
            } else {
                ProgressView().tint(.white)
            }
        case .guide:
            guideFace
        }
    }

    @ViewBuilder
    private var issueFace: some View {
        if let vm = issueVM, let issue = vm.issue {
            IssueFaceView(
                vm: vm,
                issue: issue,
                barTrailing: issueBarTrailing,
                onStartCoding: startCodingTapped,
                onOpenChanges: { selectFace(.guide) },
                mergeCircle: mergeCircle(on: .issue)
            )
        } else if let vm = issueVM, vm.loadTimedOut {
            unavailableState(vm: vm)
        } else {
            ProgressView().tint(.white)
        }
    }

    /// EXP-1251: the Guide — the report over the diff (the shown run's live
    /// diff while it has one, else the issue's PR / branch files), the Stack
    /// card on top while the PR sits in an open stack.
    @ViewBuilder
    private var guideFace: some View {
        if let shownSession, runHasDiff {
            RunModelClaim(accountId: accountId, session: shownSession) { model in
                let parsed = model?.latestDiff == nil ? nil : model?.parsedDiff
                guideView(
                    files: parsed?.files,
                    status: parsed == nil ? .loading : .ready,
                    truncatedLines: parsed?.truncatedLines
                )
            }
            .id(shownSession.id)
        } else {
            guideView(files: prChangesModel?.loadedFiles, status: prDiffStatus, truncatedLines: nil)
        }
    }

    /// The PR / branch files' load state (ready when there is no such diff).
    private var prDiffStatus: GuideDiffStatus {
        guard let prChangesModel, issueHasChanges else { return .ready }
        switch prChangesModel.load {
        case .loading: return .loading
        case let .failed(message): return .failed(message)
        case .loaded: return .ready
        }
    }

    private func guideView(
        files: [Diff.File]?, status: GuideDiffStatus, truncatedLines: Int?
    ) -> some View {
        GuideFace(
            groups: sessionResultGroups,
            files: files,
            diffStatus: status,
            truncatedLines: truncatedLines,
            prFallback: hasRunResults ? nil : prFallback,
            stack: guideStack,
            onOpenStackMember: { openStackMember($0) },
            onMergeThrough: mergeThroughHandler,
            section: $guideSection,
            focusPath: $changesFocusPath,
            showsMerge: mergeTarget != nil
        ) {
            mergeCapsule(on: .guide)
        }
    }

    /// Merge through needs a member.
    private var mergeThroughHandler: ((String) -> Void)? {
        guard issueVM?.permissions.isMember == true else { return nil }
        return { requestMergeThrough($0) }
    }

    /// EXP-1154: the open PR's body standing in for a missing report.
    private var prFallback: GuidePrFallback? {
        guard issuePrOpen else { return nil }
        switch prDescription {
        case let .loaded(forIssue, description) where forIssue == issueId:
            return .loaded(description)
        case let .failed(forIssue, message) where forIssue == issueId:
            return .failed(message)
        default:
            return .loading
        }
    }

    /// EXP-1248: the rail of the issue's open stack (`PrStack.stackView` over
    /// the team's pull requests), nil off a linear open stack of 2+.
    private var guideStack: PrStack.StackView? {
        guard let issue, let pool = prGraphModel?.stackPool(for: issue) else { return nil }
        return PrStack.stackView(issue, issues: pool)
    }

    /// A stack member tap: the subject's own row stays, any other SWAPS in on
    /// its Guide (the top route is replaced, as on web + desktop).
    private func openStackMember(_ id: String) {
        guard id != issueId else { return }
        deps.deepLinkBus.navigateToIssue(id, accountId: accountId, face: .guide, replacingTop: true)
    }

    /// EXP-1248: a stack row's long-press "Merge through here" — the ONE
    /// confirm in `through` mode.
    private func requestMergeThrough(_ id: String) {
        guard let issue, let pool = prGraphModel?.stackPool(for: issue),
              let row = pool.first(where: { $0.id == id }) ?? (issue.id == id ? issue : nil)
        else { return }
        mergeThroughRequest = PrStack.stackMergeConfirm(row, issues: pool, mode: .through)
    }

    /// EXP-1154: the open PR's GitHub body, fetched only while the Guide
    /// shows and needs it (no run report), once per issue. A failure keeps
    /// its message (web / Android draw the refusal) and retries on the next
    /// Guide visit; leaving the Guide mid-flight cancels it back to idle.
    private func loadPrDescription() async {
        guard face == .guide, issuePrOpen, !hasRunResults, let issueId else { return }
        switch prDescription {
        case let .loaded(forIssue, _) where forIssue == issueId: return
        case let .loading(forIssue) where forIssue == issueId: return
        default: break
        }
        prDescription = .loading(issueId: issueId)
        do {
            let description = try await deps.issuesApi.prDescription(
                accountId: accountId, issueId: issueId
            )
            prDescription = .loaded(issueId: issueId, description)
        } catch {
            if Task.isCancelled || error is CancellationError {
                prDescription = .idle
            } else {
                prDescription = .failed(issueId: issueId, error.userFacingMessage)
            }
        }
    }

    private func sessionView(_ session: CodingSessionEntity) -> some View {
        AgentSessionView(
            accountId: accountId,
            session: session,
            request: $runRequest,
            continuation: continuation,
            runState: runState,
            startReadiness: offerStart ? readiness : nil,
            onStartCoding: startCodingTapped,
            mergeCircle: mergeCircle(on: .run)
        )
        .id(session.id)
        // EXP-933: the inline `sessions_guide` card opens THIS screen's
        // Guide face.
        .environment(\.openResultsFace, { selectFace(.guide) })
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
            // EXP-1150: standalone, the header band draws the bar's material
            // over the title row AND the tabs, with its own hairline — the
            // nav bar's background and divider stand down.
            .toolbarBackground(.hidden, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    principalTitle
                }
                // SLOP-16 r2: the ONE graph icon button sits on the action
                // edge, left of `…` / Stop, on EVERY face, in the bar's own
                // capsule like `…`. Always mounted (EXP-942: an action-edge
                // item that comes and goes sometimes failed to reappear).
                // Borderless like everywhere else: on iOS 26 the bar would
                // otherwise fuse it with `…` into one shared capsule.
                if #available(iOS 26.0, *) {
                    ToolbarItem(placement: .topBarTrailing) { prGraphBadge }
                        .sharedBackgroundVisibility(.hidden)
                } else {
                    ToolbarItem(placement: .topBarTrailing) { prGraphBadge }
                }
                // EXP-942: Stop / Resume is its OWN bar item, so the system
                // gives it its own capsule instead of merging it with the
                // `…` menu into one shared shape. Mounted for the whole Run
                // face — `runPill` draws nothing while there is no Stop or
                // Resume — because an item inserted and removed on the action
                // edge sometimes failed to reappear. No spacer: nothing
                // trails the pill on this face, and a fixed one only inset it
                // from the edge.
                if face == .run {
                    ToolbarItem(placement: .topBarTrailing) { runPill }
                }
                ToolbarItemGroup(placement: .topBarTrailing) {
                    // EXP-895: on the Guide face GitHub rides the HEADER's
                    // action slot — the work bar's leading slot belongs to the
                    // file sheet now.
                    if face == .guide, let url = prURL {
                        Button { openURL(url) } label: {
                            AppIcon(
                                AppIcons.uiGithub,
                                size: AppIcon.Size.medium,
                                weight: .medium
                            )
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                        .accessibilityLabel(DomainContract.diffUiOpenOnGithub)
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

    /// EXP-818: the ONE Stop, IDENTICAL whether this phone hosts the run or
    /// only watches it; Resume once it ended and a machine can take it
    /// (EXP-773). EXP-942: a NATIVE toolbar button, not a glass pill — the
    /// navigation bar wraps its items in the system capsule at the back
    /// button's height, and a pill inside that capsule drew a second, broken
    /// outline. Tint carries the red; the bar owns the shape.
    @ViewBuilder
    private var runPill: some View {
        switch primaryAction {
        case .stop:
            // Text, not a Label: a bar item collapses a Label to its icon,
            // and the ending verb is the word "Stop" on every client.
            Button(role: .destructive) { runRequest = .stop } label: {
                Text("Stop").fontWeight(.medium)
            }
            .tint(DesignTokens.Semantic.red)
            .accessibilityLabel("Stop the agent and end the run")
            .accessibilityIdentifier("session-stop")
        case .resume:
            Button { showResumeConfirm = true } label: {
                Text("Resume").fontWeight(.medium)
            }
            .disabled(resuming)
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
                isPresented: $runsMenuOpen, anchor: runsMenuAnchor, presentation: .inline
            ) {
                runsMenuItems
            }
            // Each presentation lives on its OWN node (EXP-240).
            .background {
                Color.clear
                    .sheet(item: $shareTarget) { target in
                        ActivityShareSheet(items: [target.text, target.url])
                    }
            }
            // EXP-1021: the shared board picker, opened by the `…` menu item
            // rather than by a trigger of its own.
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
            // zero-size nodes of their own as well: stacking presentations
            // on one node is where SwiftUI starts dropping them.
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
            // EXP-1154: Close PR, off the `…` menu — the fixture copy ×4.
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(isPresented: $showClosePrConfirm) {
                        GlassAlert(
                            title: ClosePrCopy.title,
                            message: ClosePrCopy.message(otherLinkedIssues: closePrOtherIssues),
                            actions: [
                                GlassAlertAction("Cancel", role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                                GlassAlertAction(ClosePrCopy.confirm, role: .destructive, id: "close-pr") { closePr() },
                            ]
                        )
                    }
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
            // EXP-897 Part 4: the badge's overlay, the same on every face.
            .background {
                Color.clear
                    .sheet(isPresented: $prGraphOpen) {
                        if let graph = prGraph {
                            PrGraphSheet(
                                graph: graph,
                                subjectIssueId: issue?.id,
                                users: prGraphModel?.users ?? [],
                                teamStatuses: issueVM?.teamStatuses ?? [],
                                onOpenIssue: { id in
                                    prGraphOpen = false
                                    deps.deepLinkBus.navigateToIssue(id, accountId: accountId)
                                },
                                onOpenPullRequest: { entry in
                                    prGraphOpen = false
                                    openPullRequest(entry, subject: graph)
                                }
                            )
                        }
                    }
            }
            // EXP-876: a multi-issue run's covered issues, off its title.
            .background {
                Color.clear
                    .sheet(isPresented: $coveredIssuesOpen) { coveredIssuesSheet }
            }
            // EXP-1121: "Ready to code?" — its own node (EXP-240). Its
            // fixes that leave the issue run AFTER the dismiss.
            .background {
                Color.clear
                    .sheet(isPresented: $showReadiness, onDismiss: runReadinessFollowUp) {
                        if let vm = issueVM, let readinessModel {
                            CodingReadinessSheet(
                                model: readinessModel,
                                vm: vm,
                                remoteStartEnabled: steerConfigLoaded ? steerEnabled : nil,
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
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(isPresented: $showResumeConfirm) {
                        GlassAlert(
                            prompt: Prompts.ResumeRun.copy(device: resumeDevice?.deviceLabel),
                            handlers: ["resume": { resumeRun() }]
                        )
                    }
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
                if canClosePr {
                    GlassMenuItem(ClosePrCopy.menuItem, icon: AppIcons.prClosed, destructive: true) {
                        showClosePrConfirm = true
                    }
                }
                GlassMenuItem("Delete issue", icon: AppIcons.uiDelete, destructive: true) {
                    showDeleteConfirm = true
                }
            }
        }
    }

    /// EXP-1150: the run menu under the `Runs` tab — one row per run
    /// (EXP-886's byline, a check on the shown one).
    @ViewBuilder
    private var runsMenuItems: some View {
        ForEach(runIds, id: \.self) { id in
            runMenuRow(id: id)
        }
    }

    /// One run of mine as `<device> · Live` / `<device> · 2h ago` (the ×4
    /// `PastRuns` byline); the run on show wears the check, a live one the
    /// running glyph.
    @ViewBuilder
    private func runMenuRow(id: String) -> some View {
        if let run = menuRuns.first(where: { $0.id == id }) {
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
                pickRun(id)
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
                steerConfigLoaded = true
            }
            // EXP-1121: the readiness inputs follow the issue's team, and its
            // board's repository edge (a pick, a retarget) re-reads the
            // server half — the devices ride their own live observation.
            .onChange(of: issueVM?.board?.teamId, initial: true) { _, _ in
                ensureReadinessModel()
            }
            .task(id: "\(issueVM?.board?.id ?? "")|\(issueVM?.board?.repositoryId ?? "")") {
                ensureReadinessModel()
                await readinessModel?.boardChanged(issueVM?.board)
            }
            // A session subject learns its issue off its row.
            .onChange(of: subjectModel?.issueId, initial: true) { _, _ in
                ensureIssueViewModel()
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
            .onChange(of: subjectModel?.runsResolved) { _, _ in
                facesChanged(availableFaces)
            }
            // EXP-1154: a pending Changes / Results arrival also waits for
            // the issue row (`WorkFaces.holdsPendingFace`).
            .onChange(of: issue != nil) { _, _ in
                facesChanged(availableFaces)
            }
            // EXP-952: the PR-files model follows its key — created and
            // started once the issue has changes and no live diff outranks
            // them, stopped and dropped once a live diff takes over (the
            // session view draws that one) or the changes go away.
            .onChange(of: wantsPrChangesModel, initial: true) { _, wanted in
                prChangesModelWanted(wanted)
            }
            // EXP-1154: Results without a run report reads the open PR's
            // GitHub body.
            .task(id: "\(issueId ?? "")|\(issuePrOpen)|\(hasRunResults)|\(face == .guide)") {
                await loadPrDescription()
            }
            // A new merge target starts clean (no stale spinner).
            .onChange(of: mergeTarget.map(mergeTargetKey)) { _, _ in
                mergeState = WorkMergeState()
            }
            // EXP-934: the `…` is the Issue face's, so its overlay leaves with
            // the face — whichever path moved it (a tap, a vanished face, a
            // continuation swapping in).
            .onChange(of: face) { _, next in
                menuOpen = false
                runsMenuOpen = false
            }
            // EXP-893: the session view's report, held while the pager has
            // the Run page unmounted (the preference resets then).
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
            // The hold lifting is an edge too: a run that ended WHILE a
            // continuation was pending, whose successor then never landed
            // (`sent` past its deadline, or `failed`), must still pop — Android
            // keys `continuationPending` into the same effect.
            .onChange(of: continuation.isPending) { _, pending in
                if !pending { endedChanged(shownEnded) }
            }
            // The desktop picked the resume / switch up — swap the
            // continuation in, on whichever face the reader is on.
            .onChange(of: continuation.watcher.startedSession) { _, started in
                if let started {
                    continuation.watcher.startedSession = nil
                    continuation.landed(started.sessionId)
                    swapIn(started.sessionId)
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
        ensureReadinessModel()
        // EXP-952: re-arm the PR-files observation like every other one here.
        prChangesModel?.startObserving()
    }

    /// EXP-952: keep the screen-owned PR-files model in step with its key.
    private func prChangesModelWanted(_ wanted: Bool) {
        if wanted {
            guard prChangesModel == nil, let issueId else { return }
            let model = ChangesViewModel(
                accountId: accountId,
                source: .issue(issueId),
                db: deps.db,
                issuesApi: deps.issuesApi,
                repositoriesApi: deps.repositoriesApi,
                codingSessionsApi: deps.codingSessionsApi
            )
            prChangesModel = model
            model.startObserving()
        } else if let model = prChangesModel {
            model.stopObserving()
            prChangesModel = nil
        }
    }

    private func disappear() {
        // Belt-and-braces with EditorTextView.willMove(toWindow:) — no
        // first responder may outlive this screen (EXP-246).
        UIApplication.endEditing()
        subjectModel?.stop()
        prGraphModel?.stop()
        readinessModel?.stop()
        prChangesModel?.stopObserving()
        continuation.stop()
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
        if let pending = pendingInitialFace {
            if WorkFaces.holdsPendingFace(
                pending: pending,
                shown: face,
                available: faces,
                runsResolved: subjectModel?.runsResolved == true,
                issueResolved: issueId == nil || issue != nil
            ) {
                return
            }
            pendingInitialFace = nil
        }
        guard !faces.isEmpty, !faces.contains(face) else { return }
        if let next = WorkFaces.fallbackFace(shown: face, available: faces) {
            face = next
        }
    }

    private func chromeChanged(_ chrome: RunChrome) {
        // EXP-1152: the Run page comes and goes with the pager, not with the
        // face — an unmounted emitter's reset is not news on ANY face (a new
        // run's reset is `shownSessionId`'s own).
        guard chrome != RunChrome() else { return }
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
        // for the continuation instead of leaving. EXP-935: the hold is the
        // screen's own (`RunContinuation`), so it holds on every face and
        // until the successor lands or its deadline passes — it used to ride
        // the run view's chrome preference, which the Issue face dropped.
        guard !continuation.isPending, !resuming else { return }
        guard sawLiveSession else { return }
        dismiss()
    }

    // MARK: - Actions

    /// A tab tap (or an in-face link): the pages SLIDE to the face.
    private func selectFace(_ next: WorkFaceKind) {
        withAnimation(motion.standard) { switchFace(next) }
    }

    private func switchFace(_ next: WorkFaceKind) {
        guard next != face else { return }
        UIApplication.endEditing()
        // EXP-934: the `…` overlay closes off the `face` edge (`withLifecycle`).
        face = next
    }

    /// A row of the run menu: show that run on the Run face.
    private func pickRun(_ id: String) {
        userPickedRun = true
        shownSessionId = id
        selectFace(.run)
    }

    private func toggleRunsMenu() {
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) { runsMenuOpen.toggle() }
    }

    /// A resumed / switched run's continuation: shown in place, pinned.
    private func swapIn(_ sessionId: String) {
        userPickedRun = true
        shownSessionId = sessionId
        face = .run
    }

    /// The Issue face's trailing circle: the Start circle behind its gates
    /// (EXP-240) whenever the issue can be started — EXP-1150: the faces are
    /// the tab strip, so nothing else competes for the slot.
    private var issueBarTrailing: IssueBarTrailing {
        guard let readiness, readiness.visible else { return .hidden }
        return .start(readiness)
    }

    /// EXP-1121: every Start coding tap — the Issue and Run bars' circles.
    /// Ready starts (the composer); still loading does nothing; anything
    /// missing opens "Ready to code?".
    private func startCodingTapped() {
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
            // Repositories (and the GitHub connect) + the boards' repository
            // controls both live in Team settings on iOS.
            case .connectGithub, .boardSettings:
                guard let teamId else { return }
                pushRoute(.teamSettings(accountId: accountId, teamId: teamId))
            case .openDevices:
                pushRoute(.agents)
            }
        }
    }

    /// Arm (or re-arm) the readiness inputs once the issue's team is known.
    private func ensureReadinessModel() {
        guard let teamId = issueVM?.board?.teamId else { return }
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
        continuation.sending(.resume)
        let sessionId = shownSession.id
        Task {
            do {
                try await deps.steerApi.resumeSession(
                    accountId: accountId,
                    sessionId: sessionId,
                    deviceId: device.deviceId
                )
                continuation.sent(
                    key: .resumed(fromId: sessionId),
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
            } catch {
                continuation.failed(error.userFacingMessage)
            }
            resuming = false
        }
    }

    /// EXP-1154: Close PR shows for a member while the issue's PR is open.
    private var canClosePr: Bool {
        issueVM?.permissions.isMember == true && issuePrOpen
    }

    /// How many OTHER issues share the PR (a batch) — the confirm's extra line.
    private var closePrOtherIssues: Int {
        max(0, (prGraph?.entry?.issues.count ?? 1) - 1)
    }

    /// Close the PR WITHOUT merging (EXP-100, the drop path). The `prState`
    /// flip arrives through Electric sync; a refusal toasts.
    private func closePr() {
        guard let issueId, !closingPr else { return }
        closingPr = true
        Task {
            do {
                try await deps.issuesApi.closePr(accountId: accountId, issueId: issueId)
            } catch {
                toaster.error(MergeFailure(error: error).message)
            }
            closingPr = false
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

/// EXP-1121: what "Ready to code?" asked the screen to do once it closed.
private enum ReadinessFollowUp {
    case start
    case route(CodingReadinessRoute)
}

/// EXP-1154: the open PR's GitHub body for the Results fallback, keyed by
/// the issue it was fetched for.
private enum PrDescriptionLoad: Equatable {
    case idle
    case loading(issueId: String)
    case loaded(issueId: String, PrDescription)
    case failed(issueId: String, String)
}
