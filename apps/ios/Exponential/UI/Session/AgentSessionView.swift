import ExpCore
import ExpUI
import GRDB
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// Resolves an AppRoute.agentSession's synced coding_sessions row and hosts
/// AgentSessionView as a pushed navigation destination (EXP-221) — pushed,
/// not a fullScreenCover, so the screen gets the native back button and
/// interactive swipe-back like every other page.
struct AgentSessionRouteView: View {
    let sessionId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var session: CodingSessionEntity?

    var body: some View {
        Group {
            if let session {
                AgentSessionView(accountId: accountId, session: session)
            } else {
                ZStack {
                    AppBackground()
                    ProgressView().tint(.white)
                }
            }
        }
        .onAppear {
            guard session == nil,
                  let pool = try? deps.db.pool(forAccountId: accountId)
            else { return }
            session = try? pool.read { db in
                try CodingSessionEntity.filter(Column("id") == sessionId).fetchOne(db)
            }
        }
    }
}

/// The "Agent session" screen (EXP-32) — a chat-style view of a live coding
/// session over the relay's scrubbed activity channel. NO terminal rendering:
/// narration bubbles, compact tool rows, collapsible subagent runs, question
/// cards, and a pinned "Latest changes" diff chip above the input bar. Steering
/// is message-shaped (text + \r, perm-gated by the relay) and questions answer through the
/// semantic `answer` frame (EXP-249).
/// Identical UX to the Android AgentSessionScreen (glass design system).
/// Pushed onto the NavigationStack (EXP-221) — status lives in the native
/// nav bar; back is the system chevron + swipe gesture.
struct AgentSessionView: View {
    let accountId: String
    let session: CodingSessionEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    /// A cache of the SteerSessionStore lookup (EXP-621) — the model itself is
    /// app-scoped, so this view neither creates nor tears it down.
    @State private var model: AgentSessionModel?
    @State private var showDiffSheet = false
    @State private var showKillConfirm = false
    /// EXP-688: the `…` menu's Usage sheet — the per-window cards that used to
    /// be a hairline strip under the nav bar.
    @State private var showUsageSheet = false
    /// The measured height of the floating Latest-changes bar (EXP-688), so a
    /// feed shorter than the viewport bottom-anchors ABOVE it instead of
    /// underneath.
    @State private var changesBarHeight: CGFloat = 0
    /// EXP-678: the Merge pill's confirm + in-flight call. No success state:
    /// the server ends the run and flips `pr_state`, and the pill disappears
    /// when that echo syncs back.
    @State private var showMergeConfirm = false
    @State private var merging = false
    /// EXP-706: the refusal AND whether the server diagnosed a REAL content
    /// conflict — the only case a retry can never fix, and the only one the
    /// recovery run can. A conflict swaps the Merge pill for "Fix conflicts".
    @State private var mergeFailure: MergeFailure?
    // "Fix conflicts" (EXP-323 rails, EXP-706 on this screen): the builtin
    // recovery run, launched on any machine the caller can reach.
    @State private var fixSheetOpen = false
    @State private var steerEnabled = false
    @State private var fixDevices: [SteerDevice]?
    @State private var startCandidates: [StartCodingSheet.IssueOption] = []
    @State private var startWatcher = StartedRunWatcher()
    @State private var fixSessionTarget: StartedRunWatcher.StartedSession?
    /// Whether the feed is scrolled to (within slack of) its bottom —
    /// auto-scroll only while pinned; scrolling up pauses follow and surfaces
    /// the "Jump to bottom" pill.
    @State private var atBottom = true
    /// EXP-356: the selected conversation tab — nil is the main agent; a
    /// subagent id focuses that agent's stream. Falls back to Main whenever
    /// the id vanishes from the feed (an `activity_reset` replay).
    @State private var agentTab: String?
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showPhotoPicker = false
    /// EXP-687: the `…` popup is an in-view overlay on this screen's root —
    /// a presentation launched from inside the UIKit bar item dropped taps and
    /// slid in from the bottom.
    @State private var menuAnchor: CGRect = .zero
    @State private var menuOpen = false
    /// EXP-773: the ended run's Resume — its confirm and the in-flight send.
    /// The watcher above owns the "waiting for the desktop" caption and
    /// pushes the resumed run's own screen.
    @State private var showResumeConfirm = false
    @State private var resuming = false
    /// EXP-696: whether THIS screen ever saw the run live — the auto-back on
    /// the ended edge only fires after that, so a finished run's feed opened
    /// from a list stays browsable.
    @State private var sawLiveSession = false
    /// EXP-724: the draft the `/` menu was dismissed at (Escape / an accepted
    /// row). Keyed on the DRAFT, not a bool, so the menu comes back on its own
    /// the moment the text changes and no `onChange` has to race the accept.
    @State private var slashDismissedFor: String?
    /// Keyboard-highlighted row of the `/` menu (hardware keyboards; ↑/↓ wrap).
    @State private var slashHighlight = 0
    /// A confirm-gated command waiting on its dialog (`/clear`).
    @State private var slashConfirm: SlashCommand?
    /// EXP-790: the composer is a folded capsule until it is tapped, and
    /// folds again on blur when nothing would be lost (IssueDetailBottomBar's
    /// rule). A non-empty draft or a pending image keeps it open regardless.
    @State private var composerExpanded = false
    @Environment(\.motion) private var motion

    private static let bottomAnchor = "feed-bottom"
    private static let feedCoordSpace = "feed-scroll"
    /// Within this many points of the bottom still counts as pinned (Android
    /// carries 96dp, EXP-529): a pixel-tight slack made the "Jump to bottom"
    /// pill hard to dismiss — a short drag had to land on the exact bottom to
    /// re-pin, so it lingered while the list visually WAS at the end (EXP-588).
    /// iOS gets a little more room than Android (EXP-591): the pill should
    /// only appear after a deliberate scroll-up and hide again well before
    /// the finger reaches the true end.
    private static let followSlack: CGFloat = 120

    /// EXP-688: one `…` (the issue-detail pattern) instead of a bare red kill
    /// glyph. Usage opens the per-window cards; Kill (EXP-268) force-ends a
    /// live session — owner-only, like everything about one (EXP-312).
    private var hasToolbarMenu: Bool {
        headerIssue != nil || hasUsage || model?.canKill == true
    }

    /// EXP-746: Usage opens on EITHER half — the machine's rate-limit report
    /// (EXP-484) or this run's own context/spend off the relay. A fresh run on
    /// a machine that reported nothing used to have no usage affordance at all.
    private var hasUsage: Bool {
        model?.agentUsage != nil || model?.sessionUsage != nil
    }

    @ViewBuilder
    private var toolbarMenuItems: some View {
        // EXP-698: the run's issue is reachable from the run. The Agents list
        // dropped its duplicate identifier pill (the title prints it), so this
        // menu — and the list row's long press — are the two ways there.
        if let issue = headerIssue {
            GlassMenuItem("Open issue", icon: AppIcons.uiIssue) {
                deps.deepLinkBus.navigateToIssue(issue.id, accountId: accountId)
            }
        }
        if hasUsage {
            GlassMenuItem("Usage", icon: AppIcons.uiUsage) {
                showUsageSheet = true
            }
        }
        if model?.canKill == true {
            GlassMenuItem("Kill session", icon: AppIcons.codingStop, destructive: true) {
                showKillConfirm = true
            }
        }
    }

    // Four small chains instead of one long one. The whole modifier chain is
    // ONE expression to the type checker, and at this view's size that budget
    // has been blown twice already (#644, #656) — each time by a condition
    // spelled out inside one of its closures, and each fix bought exactly one
    // wave of headroom. Every group below is its own inference context, so a
    // new modifier costs its group and not the whole view. Nesting reads
    // inside out; the order of application is unchanged.
    var body: some View {
        withSheets(withLifecycle(withAlerts(withChrome(sessionContent))))
    }

    private var sessionContent: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                if let model {
                    // EXP-773: an ended run's close-out and its Resume sit
                    // ABOVE its transcript, where the list rows used to hide
                    // them behind a chevron.
                    endedHeader(model)
                    feedArea(model)
                    banners(model)
                    rateLimitBanner(model)
                    compactionStrip(model)
                    bottomBar(model)
                } else {
                    Spacer()
                }
            }
        }
    }

    /// Nav bar, title block and the `…` menu.
    private func withChrome(_ content: some View) -> some View {
        content
            .glassMenuOverlay(isPresented: $menuOpen, anchor: menuAnchor, presentation: .inline) {
                toolbarMenuItems
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .toolbar {
                // EXP-688: the header names the ISSUE, like the list row it was
                // opened from — the phase/machine line it used to be is demoted to
                // a caption under it.
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 1) {
                        SessionRowTitle(
                            identifier: headerIssue?.identifier,
                            title: headerTitle,
                            state: headerState,
                            paused: hostPaused || headerLost,
                            live: model?.phase == .live
                        )
                        HStack(spacing: 6) {
                            Text(headerCaption)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                            // EXP-804: the usage wall rides BESIDE the phase
                            // caption. It never replaces it — a walled run is
                            // still `Live`, it just cannot make progress, and
                            // that is exactly the pair a viewer needs to see.
                            SessionBlockedBadge(blocked: (model?.session ?? session).blocked)
                        }
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    if hasToolbarMenu {
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

    /// The four confirms: Resume, Kill, Merge, and a `/`-command's own.
    private func withAlerts(_ content: some View) -> some View {
        content
            .alert("Resume this run?", isPresented: $showResumeConfirm) {
                Button("Resume") { if let model { resumeRun(model) } }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Reopens the run on the machine that ran it, in the same worktree, and continues where the agent stopped.")
            }
            .alert("Kill this coding session?", isPresented: $showKillConfirm) {
                Button("Kill session", role: .destructive) {
                    Task { await model?.killSession() }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This stops the agent on the desktop and ends the session.")
            }
            // EXP-678: merging from the steering screen — same confirm-gated flow
            // as the Agents list and Reviews.
            .alert("Merge pull request?", isPresented: $showMergeConfirm) {
                Button("Merge", role: .destructive) {
                    if let model { merge(model) }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                // EXP-734: this run's OWN pull request links no issue, so
                // promising completed issues would be a lie.
                if case .session = model?.mergeTarget {
                    Text("Merges this run's pull request and closes the coding session.")
                } else {
                    Text("Merges the pull request, completes every linked issue, and closes the coding session.")
                }
            }
            // EXP-724: `/clear` discards the conversation, so confirm rows
            // confirm before the frames go out. Copy is byte-identical ×4.
            .alert(
                slashConfirm.map { SlashCommands.confirmTitle($0) } ?? "",
                isPresented: Binding(
                    get: { slashConfirm != nil },
                    set: { if !$0 { slashConfirm = nil } }
                ),
                presenting: slashConfirm
            ) { command in
                Button(SlashCommands.confirmButton(command), role: .destructive) {
                    if let model { performSend(model) }
                }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text(SlashCommands.confirmBody)
            }
    }

    /// Attach/detach, the photo picker, and the state changes the screen
    /// reacts to. Their bodies live in methods below for the same reason the
    /// chain is split at all.
    private func withLifecycle(_ content: some View) -> some View {
        content
            .photosPicker(
                isPresented: $showPhotoPicker,
                selection: $photoItems,
                maxSelectionCount: SteerImageMessage.maxImages,
                matching: .images
            )
            .onChange(of: photoItems) { _, newItems in
                guard !newItems.isEmpty else { return }
                Task { await ingestPhotos(newItems) }
            }
            // EXP-790: blur collapses the composer ONLY when nothing would be
            // lost — empty draft, no pending images, no picker mid-flight
            // (presenting one resigns first responder). Copied from
            // IssueDetailBottomBar.
            // EXP-802: the composer's focus lives on its editor model now (a
            // UITextView owns first responder), not in a `@FocusState`.
            .onChange(of: model?.draftEditor.isEditing) { _, editing in
                draftEditingChanged(editing)
            }
            // EXP-696: leave the screen when the run finishes under the viewer
            // (kill, merge, the agent's own exit — the synced row edge covers every
            // path). Gated on having SEEN the run live here first: the model
            // attaches after onAppear, so a plain false→true onChange would also
            // fire when opening an ALREADY-ended run's feed, which must stay put.
            // Row status, not `isOver`: a relay `bye` alone shouldn't yank a
            // screen the row still calls live.
            // EXP-706: NOT while a recovery run is pushed on top of this screen —
            // that run's merge is what ends this one, and popping the parent would
            // yank the viewer out of the session they just started.
            // The decision lives in `sessionEndedChanged` rather than inline: this
            // body is a 200-line modifier chain, and every condition spelled out
            // inside one of its closures is type-checked as part of it. Spelling
            // this one out inline is what tipped the budget over twice already
            // (the app target is only compiled by the `ios-v*` tag build and the
            // staging archive, so it fails nowhere else).
            .onChange(of: model?.sessionEnded) { _, ended in
                sessionEndedChanged(ended)
            }
            // No scenePhase handler here: foreground revival (EXP-243) is
            // app-scoped since EXP-621 — the root handler reconnects every retained
            // session, not just the one that happens to be on screen.
            .onAppear {
                // The socket owner is app-scoped (EXP-621): popping back to this
                // screen re-attaches to the SAME model, so the feed is already
                // there, the composer still holds its draft, and there is no
                // connect phase to sit through.
                model = deps.steerSessions.attach(accountId: accountId, sessionId: session.id) {
                    AgentSessionModel(
                        accountId: accountId,
                        session: session,
                        currentUserId: deps.auth.userId,
                        steerApi: deps.steerApi,
                        attachmentsApi: deps.attachmentsApi,
                        db: deps.db
                    )
                }
            }
            .onDisappear {
                // NOT a teardown: the store keeps the socket up while the session
                // runs and retires it once it is over (or falls off the cap).
                deps.steerSessions.detach(accountId: accountId, sessionId: session.id)
                startWatcher.stop()
                // EXP-802: the DRAFT outlives this screen, its focus must not —
                // the editor model would otherwise hand first responder straight
                // back on return and pop the keyboard over a screen nobody typed
                // into. (The text is untouched; only the caret's claim goes.)
                model?.draftEditor.setFocused(nil)
            }
            // EXP-706: the "Fix conflicts" launcher — the machines it can run on
            // resolve off the synced devices shape, once steering is known on.
            .task(id: accountId) {
                let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
                steerEnabled = config.enabled
                await refreshFixTargets()
            }
    }

    /// Sheets and the one pushed destination (the recovery run, EXP-706).
    private func withSheets(_ content: some View) -> some View {
        content
            .sheet(isPresented: $fixSheetOpen) {
                StartCodingSheet(
                    devices: fixDevices ?? [],
                    issues: startCandidates,
                    preselectedIds: [],
                    teamId: session.teamId,
                    initialTab: .actions,
                    preselectedActionId: DomainContract.builtinFixConflictsId,
                    preselectedPrIssueId: model?.mergeIssue?.id,
                    onStart: { device, issueIds, options in
                        startFixIssues(on: device, issueIds: issueIds, options: options)
                    },
                    onRunAction: { device, action, options, inputs in
                        runFixAction(on: device, action: action, options: options, inputs: inputs)
                    }
                )
            }
            // The desktop picked the start up — push the recovery run's own steer
            // screen ONCE, exactly like Reviews does (EXP-536).
            .onChange(of: startWatcher.startedSession) { _, started in
                if let started {
                    startWatcher.startedSession = nil
                    fixSessionTarget = started
                }
            }
            .navigationDestination(item: $fixSessionTarget) { target in
                AgentSessionRouteView(sessionId: target.sessionId)
                    .environment(\.accountId, accountId)
            }
            .sheet(isPresented: $showDiffSheet) {
                if let diff = model?.latestDiff {
                    LatestChangesSheet(diff: diff)
                }
            }
            // EXP-688: usage lives in its own sheet now — every window the machine
            // reported, grouped, instead of one pinned hairline.
            .sheet(isPresented: $showUsageSheet) {
                if hasUsage {
                    AgentUsageSheet(
                        usage: model?.agentUsage?.usage,
                        account: model?.agentAccount,
                        sessionUsage: model?.sessionUsage
                    )
                }
            }
    }

    // MARK: - Lifecycle handlers

    // EXP-790: blur collapses the composer ONLY when nothing would be lost —
    // empty draft, no pending images, no picker mid-flight (presenting one
    // resigns first responder). Copied from IssueDetailBottomBar.
    // EXP-802: the composer's focus lives on its editor model now (a UITextView
    // owns first responder), not in a `@FocusState`.
    private func draftEditingChanged(_ editing: Bool?) {
        guard composerExpanded, editing == false, let model else { return }
        guard !showPhotoPicker, photoItems.isEmpty else { return }
        guard model.trimmedDraft.isEmpty, model.pendingImages.isEmpty else { return }
        withAnimation(motion.standard) { composerExpanded = false }
    }

    // EXP-696: leave the screen when the run finishes under the viewer (kill,
    // merge, the agent's own exit — the synced row edge covers every path).
    // Gated on having SEEN the run live here first: the model attaches after
    // onAppear, so a plain false→true change would also fire when opening an
    // ALREADY-ended run's feed, which must stay put. Row status, not `isOver`:
    // a relay `bye` alone shouldn't yank a screen the row still calls live.
    // EXP-706: NOT while a recovery run is pushed on top of this screen — that
    // run's merge is what ends this one, and popping the parent would yank the
    // viewer out of the session they just started.
    private func sessionEndedChanged(_ ended: Bool?) {
        guard let ended else { return }
        if !ended {
            sawLiveSession = true
            return
        }
        guard sawLiveSession, fixSessionTarget == nil else { return }
        dismiss()
    }

    // MARK: - Header

    /// Everything an `AgentMarkdownText` needs to render embedded images —
    /// agent prose can carry `![](/api/attachments/{id})` and those fetches are
    /// authenticated (EXP-440).
    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            // EXP-760: narration, steered messages, plans and question prompts
            // chip issue identifiers the agent names — `#EXP-1` AND the bare
            // `EXP-1` agents actually write — scoped to this run's team, since
            // a batch / action / chat run has no issue to derive one from.
            issueRefs: AgentIssueRefContext(
                teamId: session.teamId,
                db: deps.db,
                onOpen: { issueId in
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                }
            )
        )
    }

    /// EXP-550: the host machine is asleep and the run is parked on it — the
    /// screen says paused (grey), not "Connecting…" forever.
    private var hostPaused: Bool {
        guard model?.hostDeviceOffline == true else { return false }
        switch model?.phase {
        case .starting, .connecting, .idle, .none: return true
        case let .closed(_, reconnecting): return reconnecting
        default: return false
        }
    }

    /// EXP-549: the CURRENT machine label off the synced devices row — the
    /// session's own `device_label` is a start-time snapshot a rename never
    /// rewrites.
    private var hostLabel: String {
        model?.hostDevice.displayLabel
            ?? SessionDevicePresentation.resolve(session: session, devices: []).displayLabel
    }

    /// EXP-688: the issue this run is steering. The model already observes
    /// that row for the Merge pill (EXP-678), and for an issue-linked session
    /// it IS the issue — a batch or action run has none.
    private var headerIssue: IssueEntity? {
        guard session.issueId != nil else { return nil }
        return model?.mergeIssue
    }

    /// Line 1's title, by the Agents-list rule: an action run says its action
    /// name, a batch run "Batch run", an issue run its title.
    private var headerTitle: String {
        sessionRowTitle(issue: headerIssue, session: model?.session ?? session)
    }

    /// The socket is gone for good as far as this screen is concerned — a
    /// dropped connection or an ended run. The dot goes static neutral with
    /// the paused ones: none of them is "coding now", and the caption right
    /// under it already says which.
    private var headerLost: Bool {
        switch model?.phase {
        case .ended, .closed: return true
        default: return false
        }
    }

    /// Line 1's dot, by the Agents-list rule (EXP-194/EXP-214), narrowed by
    /// the live phase — the row alone cannot know the socket is down.
    private var headerState: CodingSessionDisplayState {
        let row = model?.session ?? session
        // EXP-734: a run that opened its own issue-less PR carries the state
        // on its OWN row.
        let state = CodingSessionDisplayState.of(
            session: row, prState: headerIssue?.prState ?? row.prState
        )
        // FEED-26: a live run whose feed went quiet gets the steady amber of
        // "Needs your input" — a pulsing green "coding now" dot over a caption
        // that says `No activity for 27 min` is a lie. Never over review/done:
        // those outrank every attention state (EXP-531).
        if state == .running, model?.staleActivityMinutes != nil { return .needsInput }
        return state
    }

    /// EXP-773: the journal fetch's own status line — which machine is being
    /// asked, and what it said. Nil whenever no fetch is in play. Mirrored ×4.
    private func historyStatus(_ model: AgentSessionModel) -> String? {
        switch model.history {
        case .pending: return "Fetching the transcript from \(hostLabel)…"
        case .deviceOffline:
            return "\(hostLabel) is offline. The transcript lives on that machine."
        case .unavailable: return "No transcript on \(hostLabel)."
        case nil: return nil
        }
    }

    /// Line 2: what the header used to say on its own — the phase, and the
    /// machine the run is parked on.
    private var headerCaption: String {
        let label = model?.hostDevice.label ?? session.deviceLabel
        let deviceName = (label?.isEmpty == false) ? label : nil
        let device = deviceName.map { " · \($0)" } ?? ""
        if let model, model.history != nil { return "Session ended" }
        if hostPaused { return "Paused\(device)" }
        switch model?.phase {
        case .live:
            // A trailing question/plan means the session is blocked on a
            // human — say so instead of looking silently stuck (EXP-97).
            if model?.awaitingInput == true { return "Needs your input\(device)" }
            // FEED-26: nothing is blocking it and nothing has happened for ten
            // minutes — say how long instead of a healthy-looking "Live".
            if let minutes = model?.staleActivityMinutes {
                return AgentFeed.staleActivityLabel(minutes: minutes, deviceLabel: deviceName)
            }
            return "Live\(device)"
        case .ended: return "Session ended"
        case let .closed(_, reconnecting): return reconnecting ? "Reconnecting…" : "Disconnected"
        default: return "Connecting…"
        }
    }

    // MARK: - Ended header (EXP-773)

    /// A finished run's byline, its Resume and its close-out summary, above
    /// the transcript. Every runs list dropped its expand-to-summary row for
    /// this: a close-out is a paragraph, and a paragraph belongs next to the
    /// transcript it summarizes, not in a list.
    @ViewBuilder
    private func endedHeader(_ model: AgentSessionModel) -> some View {
        if model.sessionEnded {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Text(endedByline(model))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    // Only on the machine that still holds the run's worktree
                    // (`RunResume`) — everywhere else there is nothing to
                    // pick up.
                    if steerEnabled, model.resumeDevice != nil {
                        GlassPill(
                            "Resume",
                            icon: AppIcons.runResume,
                            mode: .action { showResumeConfirm = true },
                            enabled: !resuming
                        )
                        .accessibilityIdentifier("resume-run")
                    }
                }
                if let summary = model.session?.summary, !summary.isEmpty {
                    AgentMarkdownText(text: summary, context: markdownContext)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("run-summary")
                }
                if let failure = startWatcher.failure {
                    Text(failure)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else if let caption = startWatcher.sentCaption {
                    Text(caption)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
        }
    }

    /// "macbook · Claude Code · ended by you · 5m ago" — the ×4 `PastRuns`
    /// rule the list rows print, now that the row itself only carries a link.
    private func endedByline(_ model: AgentSessionModel) -> String {
        let row = model.session ?? session
        return PastRuns.byline(
            device: model.hostDevice.displayLabel,
            agent: row.agent.map { LaunchVocabulary.agentLabel($0) },
            endedBy: row.endedBy,
            relativeTime: relativeDate(PastRuns.endedAt(row))
        )
    }

    private func relativeDate(_ stamp: String) -> String {
        guard let date = WireTimestamps.parse(stamp) else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }

    /// Pick this finished run up again on the machine that ran it — the SAME
    /// `steer.startSession({resumeSessionId})` the list rows used to send.
    /// A start is a COMMAND, so the shared watcher waits for the row the
    /// desktop inserts and pushes that session.
    private func resumeRun(_ model: AgentSessionModel) {
        guard let device = model.resumeDevice, !resuming else { return }
        resuming = true
        startWatcher.sending()
        Task {
            do {
                try await deps.steerApi.resumeSession(
                    accountId: accountId,
                    sessionId: session.id,
                    deviceId: device.deviceId
                )
                startWatcher.begin(
                    key: .resumed(fromId: session.id),
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

    // MARK: - Feed

    @ViewBuilder
    private func feedArea(_ model: AgentSessionModel) -> some View {
        if model.feed.isEmpty, let status = historyStatus(model) {
            // EXP-773: a finished run's transcript lives on the machine that
            // ran it, and the relay is asking that machine for it. Say which
            // machine, and say plainly when it can't answer — this is not a
            // connection problem the viewer can wait out.
            centeredState {
                if model.history == .pending {
                    ProgressView().tint(.white)
                }
                Text(status)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .multilineTextAlignment(.center)
            }
        } else if model.feed.isEmpty, hostPaused {
            // EXP-550: the machine hosting this run is asleep. The session is
            // NOT over — it resumes when the machine comes back — so say that
            // instead of spinning on "waiting for the live stream" forever.
            centeredState { hostOfflineState }
        } else if model.feed.isEmpty,
                  model.phase == .connecting || model.phase == .starting || model.phase == .idle {
            centeredState {
                ProgressView().tint(.white)
                Text(model.phase == .starting
                    ? "The agent is starting. Waiting for the live stream…"
                    : "Connecting…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .multilineTextAlignment(.center)
            }
        } else if model.feed.isEmpty, model.phase == .live, model.latestDiff == nil {
            centeredState {
                Text("Waiting for activity…")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Update the Exponential desktop app to see the live feed.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)
            }
        } else {
            VStack(spacing: 0) {
                agentTabStrip(model)
                feedList(model)
            }
            // Only present once real activity has arrived — the store-screenshot
            // test waits on it so it never captures a placeholder state.
            // `children: .contain` keeps the feed's own elements queryable; a
            // bare identifier on a plain container never reaches the hierarchy.
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-feed")
        }
    }

    /// EXP-356: conversation tabs — Main plus one per RUNNING subagent (ended
    /// tabs are dropped, EXP-387). Rendered only while a tab is visible; the
    /// strip scrolls horizontally on a fan-out.
    @ViewBuilder
    private func agentTabStrip(_ model: AgentSessionModel) -> some View {
        let agents = AgentFeed.visibleSubagentTabs(
            model.subagents, selected: agentTab
        )
        if !agents.isEmpty {
            let active = agents.contains { $0.subagentId == agentTab } ? agentTab : nil
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    agentTabChip(label: "Main", running: false, selected: active == nil) {
                        agentTab = nil
                    }
                    ForEach(agents) { run in
                        agentTabChip(
                            label: run.agentType,
                            running: !run.done,
                            selected: active == run.subagentId
                        ) {
                            agentTab = run.subagentId
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
            }
        }
    }

    private func agentTabChip(
        label: String,
        running: Bool,
        selected: Bool,
        action: @escaping () -> Void
    ) -> some View {
        GlassPill(label, mode: .select(isSelected: selected, action: action)) {
            if running {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
            }
        }
    }

    private func centeredState(@ViewBuilder content: () -> some View) -> some View {
        VStack(spacing: 8) {
            content()
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        // The bar is the feed's, not the composer's (EXP-688) — a run with an
        // open PR must still offer Merge while the feed is still arriving.
        .safeAreaInset(edge: .bottom, spacing: 0) { changesBar() }
    }

    /// Bottom-anchored feed (a short feed sits above the input bar, not at the
    /// top of the screen) with follow-scroll: pinned to the bottom until the
    /// user scrolls up, then a "Jump to bottom ↓" pill re-pins.
    ///
    /// Follow state is derived from scroll GEOMETRY and the pin is an explicit
    /// scrollTo — NOT from onAppear/onDisappear of a lazy sentinel and NOT
    /// from defaultScrollAnchor(.bottom). Both of those make layout depend on
    /// state the layout itself mutates (lazy realization ⇄ body invalidation,
    /// anchored re-scroll ⇄ lazy sizing), and once the feed outgrew the
    /// viewport that cycle wedged the main thread for good (EXP-70). Geometry
    /// comes from onScrollGeometryChange on iOS 18+ — the EXP-70 content-frame
    /// preference stopped updating during scrolls on current iOS, which left
    /// `atBottom` stuck true and the pill never appeared (EXP-212); the
    /// preference stays as the pre-18 fallback.
    private func feedList(_ model: AgentSessionModel) -> some View {
        GeometryReader { geo in
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        // EXP-356: a selected agent tab focuses that subagent's
                        // conversation; Main keeps the grouped feed.
                        if let focused = agentTab.flatMap({ id in
                            model.subagents.first { $0.subagentId == id }
                        }) {
                            SubagentRow(
                                agentType: focused.agentType,
                                status: focused.done ? .completed : .started,
                                detail: focused.detail
                            )
                            if focused.items.isEmpty {
                                Text("Nothing from this agent yet.")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.vertical, 4)
                            } else {
                                // EXP-773: the run's prose and the turns
                                // addressed to it interleave with its tool
                                // calls, in publish order.
                                let items = focused.items
                                ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                                    SubagentItemRow(
                                        item: item, context: markdownContext, nested: false
                                    )
                                    .padding(.top, Self.gapPoints(AgentFeed.transcriptGap(
                                        // The SubagentRow header above the
                                        // list is the first "previous row".
                                        prev: index == 0
                                            ? .tool
                                            : AgentFeedRow.single(items[index - 1]).rowClass,
                                        cur: AgentFeedRow.single(item).rowClass
                                    )))
                                }
                            }
                        } else {
                            // Consecutive tool calls collapse into "N tool
                            // calls" rows (EXP-97) — a render projection; the
                            // flat feed (and the trailing-question rule) stays
                            // the state.
                            // EXP-783: the run's transcript is kept WHOLE and
                            // rendered as a window; "Load earlier" grows it
                            // upward (and, past the feed's own first row, asks
                            // the device for the page below it).
                            if model.canLoadEarlier {
                                Button("Load earlier") { model.loadEarlier() }
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                                    .frame(maxWidth: .infinity)
                                    .padding(.bottom, 4)
                            }
                            let rows = model.rows
                            // EXP-787: every row's space above it comes from
                            // the shared gap ladder, keyed on the row BEFORE
                            // it — the rows themselves carry no outer margin
                            // any more, so the rhythm is one derivation
                            // instead of a padding per row kind.
                            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                                feedRow(row, isLast: row.id == rows.last?.id)
                                    .padding(.top, Self.gapPoints(AgentFeed.transcriptGap(
                                        prev: index == 0 ? nil : rows[index - 1].rowClass,
                                        cur: row.rowClass
                                    )))
                            }
                            // EXP-389: the agent-is-busy footer — live and
                            // nothing waiting on the user (Android parity).
                            if isWorking(model) {
                                WorkingIndicatorRow()
                                    .padding(.top, Self.gapPoints(AgentFeed.transcriptGap(
                                        prev: rows.last?.rowClass, cur: .tool
                                    )))
                            }
                        }
                        Color.clear
                            .frame(height: 1)
                            .id(Self.bottomAnchor)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    // EXP-787: a transcript is a reading measure. The 16pt
                    // screen inset stays, but the column caps at the shared
                    // `maxWidth` and centres inside whatever is wider — a
                    // landscape phone or an iPad must not stretch prose edge
                    // to edge.
                    .frame(maxWidth: DesignTokens.Transcript.maxWidth, alignment: .leading)
                    .frame(maxWidth: .infinity)
                    .frame(
                        minHeight: max(0, geo.size.height - (changesBarVisible ? changesBarHeight : 0)),
                        alignment: .bottom
                    )
                    .background(
                        GeometryReader { content in
                            Color.clear.preference(
                                key: FeedBottomOverflowKey.self,
                                value: content.frame(in: .named(Self.feedCoordSpace)).maxY
                                    - geo.size.height
                            )
                        }
                    )
                }
                // EXP-688: the floating Latest-changes/Merge bar. As a safe
                // area inset it becomes a CONTENT inset: the feed scrolls
                // under it, and `visibleRect` (what FollowPinTracker reads)
                // already accounts for it, so the last line still comes to
                // rest fully above the bar.
                .safeAreaInset(edge: .bottom, spacing: 0) { changesBar() }
                // EXP-698: the nav bar is `.ultraThinMaterial`, so a scrolled
                // narration line used to be sliced through its letterforms at
                // the header's edge. The fade lets it recede instead.
                .stickyHeaderFade()
                .coordinateSpace(name: Self.feedCoordSpace)
                .modifier(FollowPinTracker(
                    atBottom: $atBottom,
                    slack: Self.followSlack,
                    repin: { proxy.scrollTo(Self.bottomAnchor, anchor: .bottom) }
                ))
                .onAppear {
                    proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                    // Lazy rows can still be sizing on the first pass, landing
                    // the scroll short — re-assert once layout has settled.
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(50))
                        if atBottom {
                            proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                        }
                    }
                }
                .onChange(of: model.feed.count) { _, _ in
                    if atBottom {
                        proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                    }
                }
                // Switching conversation tabs re-pins to the newest event
                // (EXP-356).
                // EXP-795: back at the bottom, the window slides with the stream
                // again (web/Android/desktop parity).
                .onChange(of: atBottom) { _, now in model.noteAtBottom(now) }
                .onChange(of: agentTab) { _, _ in
                    atBottom = true
                    proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                }
                .overlay(alignment: .bottom) {
                    if !atBottom {
                        // Opaque: the feed scrolls beneath this pill
                        // (EXP-165 Android parity, EXP-242).
                        GlassPill(
                            "Jump to bottom ↓",
                            size: .md,
                            mode: .select(isSelected: true) {
                                // Re-arm follow directly (Android parity: the
                                // pill tap sets follow=true) instead of waiting
                                // for the scroll geometry to flip atBottom —
                                // while the agent streams, the animated
                                // scrollTo targets the bottom as of the tap,
                                // the content keeps growing underneath it, and
                                // the animation lands short of the moving max
                                // offset, so the pill never vanished
                                // (EXP-306). With atBottom set here the pill
                                // hides at once and the growth observer keeps
                                // chasing the bottom.
                                atBottom = true
                                withAnimation {
                                    proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                                }
                            },
                            isOpaque: true
                        )
                        // EXP-743: above the floating Latest-changes bar, not
                        // on top of it — the bar is a safe-area inset of this
                        // scroller, so the overlay's bottom edge is the bar's
                        // bottom edge (Android: `8.dp + bottomInset`).
                        .padding(.bottom, 8 + (changesBarVisible ? changesBarHeight : 0))
                    }
                }
            }
        }
    }

    /// EXP-389: the agent is actively working — live socket, session not
    /// ended, no active question card, and the synced `needs_input` flag
    /// clear (all three agents drive it: claude via hooks, codex via turn
    /// edges, pi via agent_settled).
    private func isWorking(_ model: AgentSessionModel) -> Bool {
        model.phase == .live && !model.sessionEnded && !model.awaitingInput
            && model.session?.needsInput != true
    }

    /// EXP-787: the ladder's token name resolved to points. ExpCore cannot see
    /// ExpUI, so `AgentFeed.transcriptGap` names the gap and the view measures
    /// it off the shared `DesignTokens.Transcript` group.
    private static func gapPoints(_ gap: AgentTranscriptGap) -> CGFloat {
        switch gap {
        case .none: 0
        case .turn: DesignTokens.Transcript.gapTurn
        case .block: DesignTokens.Transcript.gapBlock
        case .tool: DesignTokens.Transcript.gapTool
        case .default: DesignTokens.Transcript.gapDefault
        }
    }

    @ViewBuilder
    private func feedRow(_ row: AgentFeedRow, isLast: Bool) -> some View {
        switch row {
        case let .toolRun(items):
            ToolGroupRow(items: items, liveTail: isLast && model?.phase == .live)
        case let .subagentRun(run):
            SubagentGroupRow(
                run: run,
                liveTail: isLast && model?.phase == .live,
                context: markdownContext
            )
        case let .ask(group):
            askCard(group)
        case let .single(item):
            switch item {
            case let .narration(_, text, _, _):
                NarrationBubble(text: text, context: markdownContext)
            case let .tool(_, name, detail, _, _, _, _, failed, diff):
                ToolRow(name: name, detail: detail, failed: failed, diff: diff)
            case let .userMessage(_, text, _):
                // EXP-724: a steered slash command is a control action, not
                // prose — it renders as a compact pill instead of a bubble.
                // EXP-746: over the MERGED catalog, so a command the agent
                // itself advertised gets the pill too (the `/` menu that sent
                // it reads the same merge).
                if let command = SlashCommands.command(
                    for: text,
                    agent: model?.catalogAgent,
                    extra: model?.sessionConfig?.commands ?? []
                ) {
                    CommandPill(command: command, text: text)
                } else {
                    UserMessageBubble(text: text, context: markdownContext)
                }
            case let .question(question):
                questionCard(question)
            case let .subagent(_, _, agentType, status, detail, _):
                SubagentRow(agentType: agentType, status: status, detail: detail)
            case let .permission(_, tool, detail):
                PermissionRow(tool: tool, detail: detail)
            case .compaction:
                CompactionMarkerRow()
            }
        }
    }

    /// A lone question card: a plan approval or a single-question ask.
    @ViewBuilder
    private func questionCard(_ question: AgentQuestion) -> some View {
        QuestionCard(
            question: question,
            localAnswer: model?.localAnswerSummary(question.lockKey),
            active: model?.activeQuestionIds.contains(question.id) ?? false,
            canAnswer: canAnswer,
            locked: model?.isAnswerLocked(question.lockKey) ?? false,
            pending: model?.isAnswerPending(question.lockKey) ?? false,
            failed: model?.isAnswerFailed(question.lockKey) ?? false,
            onAnswer: { keys, text in sendAnswer(question, keys: keys, text: text) },
            markdownContext: markdownContext
        )
        .id(question.id)
        // EXP-642: the store slide's pop-out rect is measured off the question
        // card (`PopRects`). `contain` keeps its option buttons queryable.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-feed-question")
    }

    /// A multi-question ask (EXP-249) as ONE stepper card, claude-style: one
    /// step at a time, the desktop's `answer_ack` advances it, and the ask's
    /// final review step submits the whole thing.
    @ViewBuilder
    private func askCard(_ group: AgentAskGroup) -> some View {
        let stepIndex = AgentFeed.currentStepIndex(
            of: group, done: model?.answeredQuestionIds ?? []
        )
        // Every step done: keep the last one on screen with its resolution.
        let index = stepIndex ?? (group.questions.count - 1)
        if group.questions.indices.contains(index) {
            let question = group.questions[index]
            QuestionCard(
                question: question,
                stepLabel: stepLabel(for: question, in: group),
                priorSteps: Array(group.questions.prefix(index)),
                priorAnswers: group.questions.prefix(index).map { step in
                    step.answerSummary ?? model?.localAnswerSummary(step.lockKey)
                },
                localAnswer: model?.localAnswerSummary(question.lockKey),
                active: stepIndex != nil && (model?.activeQuestionIds.contains(question.id) ?? false),
                canAnswer: canAnswer,
                locked: model?.isAnswerLocked(question.lockKey) ?? false,
                pending: model?.isAnswerPending(question.lockKey) ?? false,
                failed: model?.isAnswerFailed(question.lockKey) ?? false,
                onAnswer: { keys, text in sendAnswer(question, keys: keys, text: text) },
                markdownContext: markdownContext
            )
            // A fresh identity per step — the card's local selection state must
            // never leak from one question into the next.
            .id(question.id)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-feed-question")
        }
    }

    private func stepLabel(for question: AgentQuestion, in group: AgentAskGroup) -> String? {
        guard let index = question.index else { return nil }
        let total = question.total ?? group.stepCount
        return total > 1 ? "Question \(index) of \(total)" : nil
    }

    private func sendAnswer(_ question: AgentQuestion, keys: [String], text: String? = nil) {
        // The picked labels (a typed free-text reply wins over its row's
        // "Type something" label) — what the stepper shows for this step
        // until the desktop resolves the ask (EXP-588).
        let labels: [String] = keys.compactMap { key in
            guard let option = question.options.first(where: { $0.key == key }) else { return nil }
            if option.freeText, let text, !text.isEmpty { return text }
            return option.label
        }
        model?.sendAnswer(
            questionId: question.wireId, askId: question.askId,
            keys: keys, text: text, labels: labels
        )
    }

    /// Whether this client may answer questions at all — a question card is
    /// answerable when this holds AND it is still active per
    /// `activeQuestionIds` (EXP-78/EXP-174).
    private var canAnswer: Bool {
        guard let model else { return false }
        return model.phase == .live && !model.sessionEnded
    }

    // MARK: - Status banners (feed retained above)

    @ViewBuilder
    private func banners(_ model: AgentSessionModel) -> some View {
        // Kill-switch failure (EXP-268) — inline banner; cleared on retry.
        if let killError = model.killError {
            bannerRow {
                Text("Couldn't kill the session. \(killError)")
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        // A refused merge (conflicts, branch protection) — same shape
        // (EXP-678); cleared on the next attempt. EXP-706: the reason only —
        // a conflict's recovery run took the Merge pill's slot in the bar.
        if let mergeFailure {
            bannerRow {
                Text(mergeFailure.message)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        // The recovery run's own progress (EXP-536): "sent to <machine>",
        // then the live session pushes itself once the desktop picks it up.
        if let runCaption = startWatcher.sentCaption {
            bannerRow {
                Text(runCaption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        if let runError = startWatcher.failure {
            bannerRow {
                Text(runError)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        // EXP-550: an offline host outranks every reconnect/starting banner —
        // the redial loops keep running underneath (untouched), they just
        // stop being what the viewer is told about.
        if hostPaused, !model.feed.isEmpty {
            bannerRow {
                AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("\(hostLabel) is offline. The agent is paused on that machine and continues when it comes back online.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        } else {
            phaseBanner(model)
        }
    }

    @ViewBuilder
    private func phaseBanner(_ model: AgentSessionModel) -> some View {
        switch model.phase {
        case let .ended(detail):
            bannerRow {
                Text(detail ?? "Session ended")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case let .closed(detail, reconnecting):
            bannerRow {
                if reconnecting {
                    ProgressView().controlSize(.small).tint(.white)
                }
                Text(detail ?? (reconnecting ? "Connection lost. Reconnecting…" : "Disconnected"))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case .starting where !model.feed.isEmpty:
            bannerRow {
                ProgressView().controlSize(.small).tint(.white)
                Text("The agent is starting. Waiting for the live stream…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        default:
            EmptyView()
        }
    }

    /// EXP-550: the "your machine is asleep" empty state — the same glyph the
    /// machines list draws for an offline device (`ui-device-offline`).
    @ViewBuilder
    private var hostOfflineState: some View {
        AppIcon(AppIcons.uiDeviceOffline, size: AppIcon.Size.large)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
        Text("\(hostLabel) is offline")
            .font(.subheadline)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
        Text("The agent is paused on that machine and continues when it comes back online.")
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .multilineTextAlignment(.center)
    }

    // MARK: - Compaction strip (EXP-724)

    /// The indeterminate strip a running compaction puts above the composer:
    /// the agent goes silent for 10–170s while it folds its context away, and
    /// without this the viewer reads that as a hang. Indeterminate on purpose
    /// — no publisher knows how far along a compaction is. It never outlives
    /// the session (the model clears the state on every end path).
    @ViewBuilder
    private func compactionStrip(_ model: AgentSessionModel) -> some View {
        if model.compacting != nil, !model.isOver {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    AppIcon(AppIcons.codingCompact, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text(AgentFeed.compactingLabel)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Spacer(minLength: 0)
                }
                ProgressView()
                    .progressViewStyle(.linear)
                    .tint(.white.opacity(TextOpacity.secondary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .padding(.horizontal, 14)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(AgentFeed.compactingLabel)
        }
    }

    // MARK: - Rate-limit banner (EXP-784)

    /// The agent's rate-limit window, in the compaction strip's slot: its own
    /// message and when the window resets, local time. It stands only while
    /// the slot holds a window — an `ok`/empty status, the replay swap and
    /// the end of the run all clear it.
    @ViewBuilder
    private func rateLimitBanner(_ model: AgentSessionModel) -> some View {
        if let limit = model.sessionRateLimit, !model.isOver {
            let caption = AgentFeed.rateLimitCaption(limit)
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                Text(caption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .padding(.horizontal, 14)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(caption)
            .accessibilityIdentifier("agent-rate-limit")
        }
    }

    private func bannerRow(@ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 8) {
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    // MARK: - Bottom bar (steering input)

    @ViewBuilder
    private func bottomBar(_ model: AgentSessionModel) -> some View {
        // EXP-312: live implies ownership — the ticket mint refuses others.
        // EXP-621: the composer is tied to the SESSION, not the socket — it
        // stays up through a reconnect (send disabled, draft intact) and only
        // goes away once the session is over. It used to vanish on every drop,
        // taking the half-typed message with it.
        if !model.isOver {
            // Steering is fully seamless (EXP-312) — no captions, no
            // operator state; input just sends.
            VStack(spacing: 8) {
                // EXP-724: the composer's menu rides ABOVE it, inside the
                // same bottom band, so it sits over the feed and above the
                // keyboard instead of being clipped by the field. EXP-802:
                // the `@`/`#`/`:` one is mounted here for the same reason,
                // and `composerMenu` is what keeps them to one at a time.
                switch composerMenu(model) {
                case .slash:
                    SlashCommandMenu(
                        commands: model.slashMatches,
                        highlighted: slashHighlight
                    ) { command in
                        applySlashCommand(command, model)
                    }
                case .autocomplete:
                    // EXP-802: the same `@`/`#`/`:` menu the comment composer
                    // mounts, over the same rows — picks route through the
                    // draft editor, which keeps first responder, so the
                    // keyboard never drops mid-message.
                    EditorAutocompleteMenu(
                        mentions: model.draftEditor.mentionCandidates,
                        issueRefs: model.draftEditor.issueRefCandidates,
                        emoji: model.draftEditor.emojiCandidates,
                        onPickMention: { model.draftEditor.applyMention($0) },
                        onPickIssueRef: { model.draftEditor.applyIssueRef($0) },
                        onPickEmoji: { model.draftEditor.applyEmoji($0) }
                    )
                case .none:
                    EmptyView()
                }
                if composerOpen(model) {
                    composerCard(model)
                } else {
                    collapsedComposerBar(model)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .animation(motion.standard, value: composerExpanded)
        }
    }

    /// EXP-790: open while tapped open, and whenever folding would hide a
    /// draft or a pending image (the model outlives this screen, so a draft
    /// typed before navigating away reopens the field on return).
    private func composerOpen(_ model: AgentSessionModel) -> Bool {
        composerExpanded || !model.trimmedDraft.isEmpty || !model.pendingImages.isEmpty
    }

    /// EXP-790: the folded composer — the capsule IssueDetailBottomBar folds
    /// its comment box into, wearing the placeholder the open field would
    /// (so a pending card's "pick an option above" still reads folded), plus
    /// a Stop circle while the agent works, so an interrupt never needs the
    /// keyboard first.
    private func collapsedComposerBar(_ model: AgentSessionModel) -> some View {
        HStack(spacing: 12) {
            Button {
                expandComposer()
            } label: {
                HStack(spacing: 6) {
                    Text(model.composerPlaceholder)
                        .font(.subheadline)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 14)
                .frame(height: 42)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(5)
                .background(GlassTokens.opaqueCardFill, in: Capsule())
                .overlay(
                    Capsule().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
                )
                .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Message the agent")
            .accessibilityIdentifier("agent-composer-collapsed")

            if model.agentWorking {
                Button {
                    model.sendInterrupt()
                } label: {
                    AppIcon(AppIcons.uiStop, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(
                            model.canSteer ? .white : .white.opacity(TextOpacity.quaternary)
                        )
                        .frame(width: 52, height: 52)
                        .background(GlassTokens.opaqueCardFill, in: Circle())
                        .overlay(
                            Circle().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
                        )
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .disabled(!model.canSteer)
                .accessibilityLabel("Stop")
            }
        }
    }

    private func expandComposer() {
        withAnimation(motion.standard) { composerExpanded = true }
        // Programmatic focus needs the field mounted — one runloop hop, with
        // a 150ms retry in case the first lands before layout. EXP-802: the
        // same shape as IssueDetailBottomBar's, driven off the editor model.
        DispatchQueue.main.async { focusComposer() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            if composerExpanded, model?.draftEditor.isEditing == false { focusComposer() }
        }
    }

    /// Hand first responder to the composer's one text block.
    private func focusComposer() {
        guard let editor = model?.draftEditor else { return }
        editor.setFocused(editor.blocks.first?.id)
    }

    // MARK: - Composer menus (EXP-724, EXP-802)

    /// What rides ABOVE the composer in the bottom band. ONE value, because
    /// the band has room for ONE menu.
    ///
    /// The two can never both have content — `SlashCommands.partialName`
    /// rejects whitespace, so a `/` menu needs a draft that is one bare word
    /// starting with `/`, while every `@`/`#`/`:` trigger has to follow
    /// start-of-text or whitespace and carry its own sigil. So this is DEFENCE
    /// against two menus stacking (and against the ↑/↓/Return keys being
    /// claimed twice), not arbitration logic: there is nothing to arbitrate.
    private enum ComposerMenu {
        case none
        case slash
        case autocomplete
    }

    private func composerMenu(_ model: AgentSessionModel) -> ComposerMenu {
        // A folded composer has no field to complete into. Both menus need
        // focus already, but the editor's focus outlives the text view it was
        // handed to, so say it here rather than trust that.
        guard composerOpen(model) else { return .none }
        if slashMenuVisible(model) { return .slash }
        if model.draftEditor.showsAutocompleteMenu { return .autocomplete }
        return .none
    }

    // MARK: - Slash commands (EXP-724)

    /// The menu is up while the field has focus, the draft matches something,
    /// and it hasn't been dismissed at exactly this draft.
    private func slashMenuVisible(_ model: AgentSessionModel) -> Bool {
        model.draftEditor.isEditing && !model.slashMatches.isEmpty
            && slashDismissedFor != model.draftText
    }

    /// Accepting a row REWRITES the draft and never sends — `/name ` when the
    /// command takes an argument, `/name` when it does not. The menu closes on
    /// the accepted draft; typing anything reopens it.
    private func applySlashCommand(_ command: SlashCommand, _ model: AgentSessionModel) {
        model.draftText = command.insertion
        slashDismissedFor = command.insertion
        slashHighlight = 0
        focusComposer()
    }

    /// The row a hardware Return would accept.
    private func highlightedSlashCommand(_ model: AgentSessionModel) -> SlashCommand? {
        let matches = model.slashMatches
        guard matches.indices.contains(slashHighlight) else { return matches.first }
        return matches[slashHighlight]
    }

    /// ↑/↓ wrap around the list (web/Android/desktop parity).
    private func moveSlashHighlight(_ delta: Int, _ model: AgentSessionModel) {
        let count = model.slashMatches.count
        guard count > 0 else { return }
        slashHighlight = ((slashHighlight + delta) % count + count) % count
    }

    // MARK: - Floating changes bar (EXP-688)

    /// Whether the Latest-changes / Merge bar is on screen. Unchanged gate —
    /// only WHERE it draws moved: it floats over the feed now instead of
    /// eating a band of height above the composer.
    private var changesBarVisible: Bool {
        guard let model else { return false }
        return model.latestDiff != nil || model.canMerge
    }

    /// The bar itself, hung off the feed as a bottom safe-area inset so the
    /// feed scrolls under it and its last line still comes to rest above it.
    @ViewBuilder
    private func changesBar() -> some View {
        if let model, changesBarVisible {
            HStack(spacing: 8) {
                if let diff = model.latestDiff {
                    diffChip(diff)
                } else {
                    Spacer()
                }
                // EXP-678: merge this run's PR without leaving the steering
                // screen — same height as the chip it sits beside. EXP-706: a
                // merge refused on a REAL conflict swaps the pill for the
                // recovery run, which is the only thing that can unblock it.
                if model.canMerge {
                    if canFixConflicts {
                        fixConflictsPill()
                    } else {
                        mergePill(model)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 8)
            .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { height in
                changesBarHeight = height
            }
        }
    }

    /// Pinned collapsible "Changes" chip — +/− counts, opens the diff
    /// sheet. The latest worktree diff replaces the previous one.
    private func diffChip(_ diff: String) -> some View {
        let stats = DiffRendering.stats(of: diff)
        return Button {
            showDiffSheet = true
        } label: {
            HStack(spacing: 8) {
                AppIcon(AppIcons.codingDiff, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Changes")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                Spacer()
                Text("+\(stats.additions)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.green)
                Text("−\(stats.deletions)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.red)
                AppIcon(AppIcons.uiChevronUp, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            // EXP-698: the chip shares its row with the `.md` Merge /
            // Fix-conflicts pills, so it takes their height rather than
            // whatever its own padding happened to add up to.
            .frame(height: GlassPillTokens.heightMd)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // Opaque: the feed scrolls beneath the bar (EXP-165, the
        // Jump-to-bottom pill's rule; Android + mobile web parity, EXP-743).
        .glassRow(isOpaque: true)
    }

    /// The Merge pill beside the diff chip — merging always ends the run too
    /// (EXP-498), so it only shows while there IS an open PR (`model.canMerge`,
    /// which resolves a batch run's PR through EXP-535's representative issue).
    private func mergePill(_ model: AgentSessionModel) -> some View {
        GlassPill(
            "Merge",
            size: .md,
            mode: .action { showMergeConfirm = true },
            // Floats over the feed like the chip beside it (EXP-743).
            isOpaque: true,
            enabled: !merging
        ) {
            if merging {
                ProgressView().controlSize(.mini).tint(.white)
            } else {
                AppIcon(AppIcons.prMerged, size: GlassPillTokens.glyphMd)
            }
        }
        .accessibilityLabel("Merge pull request")
    }

    /// EXP-706: the recovery run in the Merge pill's slot — same glass pill,
    /// same height, opening the shared "Fix merge conflicts" launcher seeded
    /// with THIS run's pull request.
    private func fixConflictsPill() -> some View {
        GlassPill(
            "Fix conflicts",
            icon: AppIcons.uiBranch,
            size: .md,
            mode: .action { fixSheetOpen = true },
            isOpaque: true
        )
        .accessibilityLabel("Fix merge conflicts")
    }

    /// Only a REAL content conflict (EXP-533) gets the run — every other
    /// refusal (stale head, branch protection, a misconfigured GitHub App) is
    /// something no rebase can fix. The run rebases the PR's branch, so one
    /// must be recorded on the issue behind the merge. EXP-734: the builtin
    /// takes an ISSUE-linked PR, so a run's own issue-less PR gets no recovery
    /// offer (the caption still names the refusal).
    private var canFixConflicts: Bool {
        guard case .issue = model?.mergeTarget else { return false }
        return steerEnabled
            && mergeFailure?.isConflict == true
            && !(model?.mergeIssue?.branch ?? "").isEmpty
    }

    /// Merge the session's PR. No local surgery on success: the server ends
    /// the session and flips `pr_state`, and both land here through sync.
    /// EXP-734: an action or chat run's PR links no issue, so it merges
    /// through the session row the server stamped it on.
    private func merge(_ model: AgentSessionModel) {
        guard let target = model.mergeTarget else { return }
        mergeFailure = nil
        merging = true
        Task {
            do {
                switch target {
                case let .issue(issueId):
                    try await deps.issuesApi.mergePr(accountId: accountId, issueId: issueId)
                case let .session(sessionId):
                    try await deps.codingSessionsApi.mergePr(
                        accountId: accountId, sessionId: sessionId
                    )
                }
            } catch {
                mergeFailure = MergeFailure(error: error)
            }
            merging = false
        }
    }

    // MARK: - Fix conflicts (EXP-706)

    private func refreshFixTargets() async {
        guard steerEnabled else {
            fixDevices = nil
            return
        }
        // EXP-432: team-scoped, so a teammate's shared machine can host the
        // run. EXP-481: read off the synced devices shape, not the network.
        fixDevices = await DeviceQueries.onlineStartTargets(
            db: deps.db, accountId: accountId,
            teamId: session.teamId, userId: deps.auth.userId
        )
        startCandidates = await StartCodingSheet.IssueOption.loadCandidates(
            db: deps.db,
            accountId: accountId,
            teamId: session.teamId
        )
    }

    /// Actions-mode launch from the unified sheet — the "Fix merge conflicts"
    /// builtin, which always rides its teamId.
    private func runFixAction(
        on device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: [String: String]
    ) {
        startWatcher.sending()
        Task {
            do {
                try await deps.steerApi.startSession(
                    accountId: accountId,
                    actionId: action.id,
                    deviceId: device.deviceId,
                    teamId: action.isBuiltin ? action.teamId : nil,
                    options: options,
                    inputs: inputs.isEmpty ? nil : inputs
                )
                startWatcher.begin(
                    key: .action(name: action.name),
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.userFacingMessage)
            }
        }
    }

    /// Issues-tab launch from the same sheet (flipping tabs must not dead-end).
    private func startFixIssues(on device: SteerDevice, issueIds: [String], options: SteerStartOptions) {
        guard let key = StartedRunKey.forIssues(issueIds) else { return }
        startWatcher.sending()
        Task {
            do {
                if issueIds.count > 1 {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueIds: issueIds,
                        deviceId: device.deviceId,
                        options: options
                    )
                } else {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueId: issueIds[0],
                        deviceId: device.deviceId,
                        options: options
                    )
                }
                startWatcher.begin(
                    key: key,
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
            } catch {
                startWatcher.failed(error.userFacingMessage)
            }
        }
    }

    /// EXP-554: the steer composer wears the comment composer's chrome, and
    /// since EXP-698 that IS the same object — one `GlassComposer` holding the
    /// transparent field, the pending strip and the `[+]`·spacer·send row. No
    /// material, no shadow, no radius of its own. Behavior is untouched: four
    /// images max, the same `sendSteerImages` upload, the same frozen
    /// `SteerImageMessage` wire format.
    private func composerCard(_ model: AgentSessionModel) -> some View {
        // EXP-702: images attach to the SESSION, so every run can carry them —
        // a batch or action run no longer has "nowhere to put them".
        let attachFull = model.pendingImages.count >= SteerImageMessage.maxImages
        let canSend = !model.trimmedDraft.isEmpty || !model.pendingImages.isEmpty
        // EXP-621: only SENDING waits for the socket — the field, the picker
        // and the strip stay usable so the draft is ready the moment the
        // reconnect lands.
        let sendDisabled = !canSend || model.steerSending || !model.canSteer
        let attachDisabled = attachFull || model.steerSending
        // EXP-790: an EMPTY field while the agent works offers Stop (the
        // interrupt) in the send slot; the first typed character brings Send
        // back.
        let showsStop = model.agentWorking && !canSend
        return GlassComposer(isOpaque: true) {
            // EXP-802: a caret-bearing field, so `@` members, `#` issues and
            // `:` emoji complete here exactly as they do in a comment. The
            // return key SENDS (the field is one message, never a document),
            // which is also how Return accepts an open menu below.
            MarkdownComposerField(
                model: model.draftEditor,
                // EXP-788: the composer IS the free answer of a pending card
                // — a plan's feedback or a question's typed reply — and the
                // placeholder says which.
                placeholder: model.composerPlaceholder,
                onReturn: { handleComposerReturn(model) },
                onPasteImage: { image in ingestPastedImage(model, image) },
                onIssueRefTap: { issueId in
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                },
                // One line of body text plus the field's own 12pt inset,
                // growing to about five before it scrolls inside itself.
                minHeight: 34,
                maxHeight: 120
            )
            // EXP-724: hardware-keyboard driving of the `/` menu. Every
            // handler returns `.ignored` while no menu is open, so a
            // Bluetooth keyboard behaves exactly as it did before. (Return is
            // NOT here: the text view intercepts it itself, so it works for
            // the soft keyboard too — see `handleComposerReturn`.)
            .onKeyPress(.upArrow) {
                guard composerMenu(model) == .slash else { return .ignored }
                moveSlashHighlight(-1, model)
                return .handled
            }
            .onKeyPress(.downArrow) {
                guard composerMenu(model) == .slash else { return .ignored }
                moveSlashHighlight(1, model)
                return .handled
            }
            .onKeyPress(.escape) {
                guard composerMenu(model) == .slash else { return .ignored }
                slashDismissedFor = model.draftText
                return .handled
            }
            .onChange(of: model.draftText) { _, _ in
                slashHighlight = 0
            }
            // The text view carries a 6pt inset of its own (`singleLine`), so
            // the card's 12/12/6 band is spelled 6/6/0 here.
            .padding(.horizontal, 6)
            .padding(.top, 6)
        } strip: {
            // EXP-790 retired the mode chip that used to ride this strip:
            // Plan/Build is the plan card's own business now, and a live
            // toggle beside the field only ever raced the card.
            if !model.pendingImages.isEmpty {
                PendingAttachmentStrip(items: model.pendingImages) { id in
                    removePendingImage(model, id: id)
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 4)
            }

            if let imageError = model.steerImageError {
                Text(imageError)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 4)
            }
        } tools: {
            // EXP-818: the image glyph every other composer wears (×4).
            GlassComposerToolButton(
                AppIcons.editorImage,
                accessibilityLabel: "Attach image",
                enabled: !attachDisabled
            ) {
                showPhotoPicker = true
            }
        } submit: {
            GlassComposerSubmitButton(
                showsStop ? AppIcons.uiStop : AppIcons.uiSubmit,
                accessibilityLabel: showsStop ? "Stop" : "Send",
                enabled: showsStop ? model.canSteer : !sendDisabled
            ) {
                if showsStop {
                    model.sendInterrupt()
                } else {
                    sendMessage(model)
                }
            }
        }
    }

    /// The return key. With a menu open it ACCEPTS the top row and never
    /// sends — the `/` menu's highlighted command (↑/↓ move it), or the
    /// candidate the `@`/`#`/`:` list is showing first, which is the row every
    /// other client's Enter takes.
    private func handleComposerReturn(_ model: AgentSessionModel) {
        switch composerMenu(model) {
        case .slash:
            guard let command = highlightedSlashCommand(model) else { return }
            applySlashCommand(command, model)
        case .autocomplete:
            let editor = model.draftEditor
            if let member = editor.mentionCandidates.first {
                editor.applyMention(member)
            } else if let candidate = editor.issueRefCandidates.first {
                editor.applyIssueRef(candidate)
            } else if let record = editor.emojiCandidates.first {
                editor.applyEmoji(record)
            }
        case .none:
            sendMessage(model)
        }
    }

    private func sendMessage(_ model: AgentSessionModel) {
        // EXP-724: `/clear` discards the whole conversation — ask
        // first, then send exactly what was typed.
        if let command = model.pendingSlashCommand, command.confirm {
            slashConfirm = command
            return
        }
        performSend(model)
    }

    private func performSend(_ model: AgentSessionModel) {
        guard !model.pendingImages.isEmpty else {
            guard !model.trimmedDraft.isEmpty else { return }
            // Clear ONLY once the frames are actually out (EXP-621): a send
            // into a dropped socket used to wipe the composer with nothing
            // sent.
            if model.sendMessage(model.draftText) { model.draftText = "" }
            return
        }
        guard !model.steerSending else { return }
        Task { await sendWithImages(model) }
    }

    /// Upload the pending images and send ONE composed message (EXP-511). A
    /// failure keeps the draft and the strip — the model hands back the images
    /// with their already-uploaded ids so a retry only uploads the rest.
    private func sendWithImages(_ model: AgentSessionModel) async {
        if let remaining = await model.sendSteerImages(
            model.draftText, images: model.pendingImages
        ) {
            model.pendingImages = remaining
        } else {
            model.draftText = ""
            model.pendingImages = []
        }
    }

    /// Turn a photo pick into pending images: transcode anything the server's
    /// inline-image pipeline doesn't accept (notably HEIC) to JPEG, exactly as
    /// the share extension does, and cap the strip at four.
    private func ingestPhotos(_ items: [PhotosPickerItem]) async {
        defer { photoItems = [] }
        guard let model else { return }
        model.steerImageError = nil
        for item in items {
            guard model.pendingImages.count < SteerImageMessage.maxImages else { break }
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let type = item.supportedContentTypes.first
            // EXP-554: one shared normalizer for every composer (steer + the two
            // comment ones) — transcode, canonical type, 10 MB cap.
            let outcome = AttachmentPicks.normalizedPhoto(
                data: data,
                contentTypeHint: type?.preferredMIMEType,
                filenameExtensionHint: type?.preferredFilenameExtension
            )
            guard let normalized = outcome.attachment else {
                model.steerImageError = outcome.failure
                continue
            }
            queuePendingImage(model, normalized)
        }
    }

    /// EXP-802: a PASTED image joins the strip like a picked one — the field
    /// is a text view now, so an image paste actually reaches the composer. It
    /// can never become an image BLOCK: the draft is exactly one text block.
    private func ingestPastedImage(_ model: AgentSessionModel, _ image: UIImage) {
        guard model.pendingImages.count < SteerImageMessage.maxImages else { return }
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        model.steerImageError = nil
        let outcome = AttachmentPicks.normalizedPhoto(
            data: data, contentTypeHint: "image/jpeg", filenameExtensionHint: "jpg"
        )
        guard let normalized = outcome.attachment else {
            model.steerImageError = outcome.failure
            return
        }
        queuePendingImage(model, normalized)
    }

    /// Queue one normalized image and drop its positional `[Image #k]` marker
    /// into the draft, so a sentence can name the picture it means (EXP-698).
    ///
    /// The marker lands at the END of the draft, not at the caret: the draft
    /// now HAS a caret (EXP-802), but it is an offset into the DECORATED text,
    /// where a resolved `#EXP-1` chip carries a title character that the sent
    /// text does not. Mapping it back is not worth it for a token that names
    /// the k-th of at most four images.
    private func queuePendingImage(
        _ model: AgentSessionModel, _ normalized: PendingCommentAttachment
    ) {
        model.pendingImages.append(PendingSteerImage(
            data: normalized.data,
            filename: normalized.filename,
            contentType: normalized.contentType,
            uploadedId: nil
        ))
        let inserted = SteerImageMessage.insertImageMarker(
            text: model.draftText,
            caret: (model.draftText as NSString).length,
            index: model.pendingImages.count
        )
        model.draftText = inserted.text
    }

    /// Dropping a pending image renumbers the draft's markers: the removed
    /// one goes and every higher one slides down, so `[Image #N]` keeps
    /// naming the N-th image that will actually be sent.
    private func removePendingImage(_ model: AgentSessionModel, id: UUID) {
        guard let position = model.pendingImages.firstIndex(where: { $0.id == id }) else { return }
        model.pendingImages.remove(at: position)
        model.draftText = SteerImageMessage.renumberImageMarkers(
            model.draftText, removedIndex: position + 1
        )
    }
}

// MARK: - Feed rows

/// EXP-787 — the transcript's own type scale, straight off the shared tokens:
/// prose at `bodySize` on a `bodyLineHeight` line, every tool row and caption
/// at `toolSize`/`toolLineHeight`.
///
/// Those numbers are the DEFAULT-size values, not fixed points: each one goes
/// through `UIFontMetrics` for the text style it stands in for (prose `.body`,
/// tool rows `.caption1`), so the transcript keeps Dynamic Type exactly as the
/// `.subheadline`/`.caption` semantics it replaced did. `lineSpacing` is
/// LEADING and SwiftUI has no line-height modifier, so it is the difference
/// between the SCALED line box and the SCALED font — resolved per render,
/// never precomputed. `AgentMarkdownText.chatCodePalette` carries the same two
/// tokens into the markdown renderer, which resolves them the same way.
private enum TranscriptType {
    static func bodyFont() -> Font { Font(bodyUIFont()) }

    static func bodyLineSpacing() -> CGFloat {
        leading(
            lineHeight: DesignTokens.Transcript.bodyLineHeight,
            font: bodyUIFont(),
            textStyle: .body
        )
    }

    static func toolFont(_ weight: PlatformFont.Weight = .regular) -> Font {
        Font(toolUIFont(weight))
    }

    static func toolLineSpacing() -> CGFloat {
        leading(
            lineHeight: DesignTokens.Transcript.toolLineHeight,
            font: toolUIFont(.regular),
            textStyle: .caption1
        )
    }

    private static func bodyUIFont() -> PlatformFont {
        scaled(size: DesignTokens.Transcript.bodySize, weight: .regular, textStyle: .body)
    }

    private static func toolUIFont(_ weight: PlatformFont.Weight) -> PlatformFont {
        scaled(size: DesignTokens.Transcript.toolSize, weight: weight, textStyle: .caption1)
    }

    private static func scaled(
        size: CGFloat, weight: PlatformFont.Weight, textStyle: PlatformFont.TextStyle
    ) -> PlatformFont {
        UIFontMetrics(forTextStyle: textStyle)
            .scaledFont(for: .systemFont(ofSize: size, weight: weight))
    }

    private static func leading(
        lineHeight: CGFloat, font: PlatformFont, textStyle: PlatformFont.TextStyle
    ) -> CGFloat {
        max(0, UIFontMetrics(forTextStyle: textStyle).scaledValue(for: lineHeight) - font.lineHeight)
    }
}

/// A tool row / transcript caption: 12pt on an 18pt line at the default text
/// size, scaled from there.
private struct TranscriptToolText: ViewModifier {
    let weight: PlatformFont.Weight
    /// `UIFontMetrics` resolves off the CURRENT trait collection, which SwiftUI
    /// cannot see into — reading the size category is what makes this view
    /// re-evaluate (and re-bake the font) when the reader changes it.
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    func body(content: Content) -> some View {
        let _ = dynamicTypeSize
        content
            .font(TranscriptType.toolFont(weight))
            .lineSpacing(TranscriptType.toolLineSpacing())
    }
}

extension View {
    fileprivate func transcriptToolText(_ weight: PlatformFont.Weight = .regular) -> some View {
        modifier(TranscriptToolText(weight: weight))
    }
}

/// Assistant prose — a small glyph + a full-width selectable markdown render
/// (EXP-440: claude narrates in markdown, so lists, code and images have to
/// come out as themselves). EXP-274 dropped the glass speech bubble: agent
/// output is the feed's bulk, and the bubble insets cost real width on a phone.
private struct NarrationBubble: View {
    let text: String
    let context: AgentMarkdownContext

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.codingAssistant, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.top, 4)
            // EXP-440: narration is markdown (lists, code, images), rendered
            // through the block stack. Bare URLs stay tappable — the remote
            // `/login` flow publishes the claude sign-in URL as narration
            // (EXP-430) — now via cmark's GFM autolink rather than the hand
            // tokenizer this replaced.
            AgentMarkdownText(
                text: text,
                context: context,
                options: [.autolinkBareURLs, .hardLineBreaks]
            )
        }
    }
}

/// The trailing "agent is busy" row (EXP-389): a gently pulsing "Working…"
/// under the newest event whenever the session is live and nothing waits on
/// the user — without it a feed that ends in tool rows gives no cue whether
/// the agent is still going. Static under Reduce Motion.
private struct WorkingIndicatorRow: View {
    @Environment(\.motion) private var motion
    @State private var pulsing = false

    var body: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.codingAssistant, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("Working…")
                .transcriptToolText()
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .opacity(pulsing ? 0.4 : 1)
        .onAppear {
            // EXP-523: ambient loops keep their own periods — a different
            // design axis from the shared duration tokens — but read the
            // Reduce Motion decision from `motion` rather than the raw
            // environment key. The flag must STAY false there: `pulsing`
            // drives the resting opacity too, so flipping it with a nil
            // animation would pin the row at 0.4 instead of leaving it
            // static.
            guard !motion.reduceMotion else { return }
            withAnimation(motion.pulse(duration: 0.9)) {
                pulsing = true
            }
        }
    }
}

/// A human turn (EXP-78): the initial prompt or a steered message — rendered
/// trailing-aligned like the sender's own chat bubble, long text folded.
private struct UserMessageBubble: View {
    let text: String
    let context: AgentMarkdownContext

    @State private var expanded = false

    /// Fold threshold — the initial prompt can be 16 KiB.
    private static let clampLines = 6
    private static let clampChars = 600
    /// Folded height. A block render has no `lineLimit`, so the fold is a
    /// clipped height cap instead — same approach as QuestionCard's prompt.
    private static let clampHeight: CGFloat = 160

    private func isClampable(_ body: String) -> Bool {
        body.count > Self.clampChars
            || body.filter { $0 == "\n" }.count >= Self.clampLines
    }

    /// EXP-698: a steered message with images (EXP-511) splits into its PROSE
    /// — whose `[Image #N]` markers become chips — and the embeds themselves,
    /// which stay below it. A message without embeds is the wire message and
    /// keeps the full markdown block render, markers and all: a marker with no
    /// picture behind it is prose, and chipping it would promise a preview
    /// that does not exist (web's `MarkerText` `count` gate).
    private var parsed: SteerImageMessage.Parsed { SteerImageMessage.parse(text) }

    var body: some View {
        let parsed = parsed
        let hasImages = !parsed.attachmentIds.isEmpty
        // Clamp the PROSE, never the wire message: four embed lines are four
        // more "lines" of nothing, and they used to push a two-line steer into
        // the fold — which then hid the images the fold was measuring.
        let clampable = isClampable(hasImages ? parsed.text : text)
        return HStack {
            Spacer(minLength: 32)
            VStack(alignment: .leading, spacing: 4) {
                Group {
                    if !hasImages {
                        AgentMarkdownText(
                            text: text,
                            context: context,
                            options: [.autolinkBareURLs, .hardLineBreaks],
                            hugsWidth: true
                        )
                    } else if !parsed.text.isEmpty {
                        if parsed.markers.isEmpty {
                            AgentMarkdownText(
                                text: parsed.text,
                                context: context,
                                options: [.autolinkBareURLs, .hardLineBreaks],
                                hugsWidth: true
                            )
                        } else {
                            MarkedUpUserText(
                                text: parsed.text,
                                imageCount: parsed.attachmentIds.count
                            )
                        }
                    }
                }
                .frame(maxHeight: clampable && !expanded ? Self.clampHeight : nil, alignment: .top)
                .clipped()
                if clampable {
                    Button(expanded ? "Show less" : "Show more") {
                        expanded.toggle()
                    }
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .buttonStyle(.plain)
                }
                // OUTSIDE the fold: the images are the point of the message,
                // and a long prose clamp must never be what hides them.
                if hasImages {
                    AgentMarkdownText(
                        text: SteerImageMessage.build(
                            text: "", attachmentIds: parsed.attachmentIds
                        ),
                        context: context,
                        options: [.autolinkBareURLs, .hardLineBreaks],
                        hugsWidth: true
                    )
                    .padding(.top, parsed.text.isEmpty ? 0 : 4)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            // Slightly brighter than the assistant's glass sections — the
            // sender's own bubble (matches the composer's active tint).
            .background(Color.white.opacity(0.10))
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .stroke(Color.white.opacity(0.16), lineWidth: 0.5)
            )
        }
    }
}

/// EXP-698: the PROSE of a steered message that carries `[Image #N]` markers.
/// The words flow as plain runs and a marker becomes a readonly `GlassPill`
/// beside them, so "crop [Image #2] tighter" reads as a sentence with a chip
/// in it. Markdown is deliberately NOT re-parsed here — a message that names
/// its pictures came from the composer as chat prose, and a block stack could
/// not flow around an inline chip anyway — but the two things the plain path
/// would otherwise lose are kept: line breaks (one flow row per line) and
/// bare URLs (rendered as `Link`, what cmark's autolink does on the markdown
/// path). The embeds are the caller's; this view renders text only.
private struct MarkedUpUserText: View {
    let text: String
    /// How many images the message actually carries. A marker outside
    /// `1...imageCount` — hand-typed, or left behind by an edit — is prose,
    /// not a chip: chipping it would promise a picture that is not there.
    let imageCount: Int
    /// EXP-787: this row bakes its own `UIFont` off `UIFontMetrics`, so it has
    /// to re-evaluate when the reader's text size changes.
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    /// One flowed item. Words are split out so the flow layout can wrap
    /// between them — a whole paragraph as one subview would simply overflow
    /// the bubble.
    private enum Piece: Identifiable {
        /// `text` is the word; when `url` is set only `text` is the anchor and
        /// `suffix` is the prose punctuation that followed it inside the same
        /// whitespace-delimited word (`https://x.dev).` → anchor + `).`).
        case word(id: Int, text: String, url: URL?, suffix: String)
        case marker(id: Int, index: Int)

        var id: Int {
            switch self {
            case .word(let id, _, _, _), .marker(let id, _): return id
            }
        }
    }

    /// A source line's pieces. Splitting on `\n` FIRST is what keeps a
    /// multi-line steer multi-line — one flow row per line, instead of every
    /// word of the message poured into a single wrapping paragraph.
    private struct Line: Identifiable {
        let id: Int
        let pieces: [Piece]
    }

    private var lines: [Line] {
        var lines: [Line] = []
        var current: [Piece] = []
        var nextId = 0

        func take() -> Int {
            defer { nextId += 1 }
            return nextId
        }
        func breakLine() {
            lines.append(Line(id: lines.count, pieces: current))
            current = []
        }

        for segment in SteerImageMessage.segments(of: text) {
            switch segment {
            case .text(let run):
                let runLines = run.components(separatedBy: "\n")
                for (offset, runLine) in runLines.enumerated() {
                    if offset > 0 { breakLine() }
                    for word in runLine.split(whereSeparator: \.isWhitespace) {
                        let word = String(word)
                        let anchor = Self.link(in: word)
                        current.append(.word(
                            id: take(),
                            text: anchor?.text ?? word,
                            url: anchor?.url,
                            suffix: anchor?.suffix ?? ""
                        ))
                    }
                }
            case .marker(let index):
                if index >= 1 && index <= imageCount {
                    current.append(.marker(id: take(), index: index))
                } else {
                    // Out of range: prose, exactly as the wire carried it.
                    current.append(.word(
                        id: take(),
                        text: SteerImageMessage.imageMarker(index),
                        url: nil,
                        suffix: ""
                    ))
                }
            }
        }
        breakLine()
        return lines
    }

    var body: some View {
        let _ = dynamicTypeSize
        // EXP-787: the flow rows ARE the message's lines, so their spacing is
        // the transcript's leading — a `lineSpacing` on the per-word `Text`s
        // could never reach them.
        VStack(alignment: .leading, spacing: TranscriptType.bodyLineSpacing()) {
            ForEach(lines) { line in
                if line.pieces.isEmpty {
                    // A blank line keeps its height — the paragraph break the
                    // sender typed is part of the message.
                    Text(verbatim: " ")
                        .font(TranscriptType.bodyFont())
                        .foregroundStyle(.clear)
                } else {
                    FlowLayout(spacing: 4) {
                        ForEach(line.pieces) { piece in
                            switch piece {
                            case .word(_, let word, let url, let suffix):
                                if let url {
                                    // One subview, no flow spacing inside it:
                                    // the punctuation belongs against the
                                    // anchor, not a space away from it.
                                    HStack(spacing: 0) {
                                        Link(destination: url) {
                                            Text(word)
                                                .font(TranscriptType.bodyFont())
                                                .underline()
                                                .foregroundStyle(Color(MarkdownStyle.linkColor))
                                        }
                                        if !suffix.isEmpty {
                                            Text(suffix)
                                                .font(TranscriptType.bodyFont())
                                                .foregroundStyle(.white.opacity(0.9))
                                        }
                                    }
                                } else {
                                    Text(word)
                                        .font(TranscriptType.bodyFont())
                                        .foregroundStyle(.white.opacity(0.9))
                                }
                            case .marker(_, let index):
                                GlassPill("Image \(index)", icon: AppIcons.editorImage)
                            }
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A bare `http(s)://` word, with prose punctuation trimmed off the end —
    /// `(see https://x.dev).` must not link the `).`. Balanced closing
    /// brackets are kept, so `https://x.dev/a(b)` stays whole. Hand-mirrored
    /// from web's `lib/linkify.ts`, scoped to one whitespace-delimited word
    /// because that is all this flow layout ever hands it.
    private static func link(in word: String) -> (text: String, url: URL, suffix: String)? {
        let lowered = word.lowercased()
        guard lowered.hasPrefix("http://") || lowered.hasPrefix("https://") else { return nil }
        var anchor = Substring(word)
        while let last = anchor.last {
            let opens: (Character) -> Int = { open in anchor.filter { $0 == open }.count }
            if ".,;:!?".contains(last) {
                anchor = anchor.dropLast()
            } else if last == ")", opens("(") < opens(")") {
                anchor = anchor.dropLast()
            } else if last == "]", opens("[") < opens("]") {
                anchor = anchor.dropLast()
            } else {
                break
            }
        }
        guard !anchor.isEmpty, let url = URL(string: String(anchor)) else { return nil }
        return (String(anchor), url, String(word.dropFirst(anchor.count)))
    }
}

/// An interactive question (EXP-78): AskUserQuestion step or plan approval.
///
/// Every card carries a wire id (EXP-613), so a tap sends ONE semantic
/// `answer` frame (the desktop maps keys to its own picker and confirms with
/// `answer_ack`) and the card locks the moment it goes out — a second tap used
/// to land on the NEXT question. There is no other answer path: EXP-672 retired
/// the pre-EXP-249 raw-keystroke fallback. `planMode` cards (EXP-97) get a
/// dedicated "Plan ready" presentation with the first wire option as the
/// primary approve action; prompt and plan bodies render as markdown, which is
/// what claude writes.
private struct QuestionCard: View {
    let question: AgentQuestion
    /// "Question 2 of 3" for a step of a multi-question ask; nil for a lone card.
    var stepLabel: String? = nil
    /// The ask's already-answered steps, summarized above this one.
    var priorSteps: [AgentQuestion] = []
    /// Per prior step: its answer — the desktop-resolved one, else what this
    /// client picked (EXP-588); nil = answered elsewhere, unknown here.
    var priorAnswers: [String?] = []
    /// What this client picked for THIS card — shown once it resolves when
    /// the desktop's resolution carried no answer text (EXP-588).
    var localAnswer: String? = nil
    /// Still answerable per the feed — the session is blocked on this card.
    let active: Bool
    /// Live (and not ended) — whether this client may answer at all.
    let canAnswer: Bool
    /// An answer is already out (or confirmed) for this card — every control
    /// stays dead until the desktop resolves it or the lock expires.
    let locked: Bool
    /// Locked but not yet confirmed by the desktop (`answer_ack`).
    let pending: Bool
    /// The last answer expired unconfirmed — answerable again, with a retry
    /// hint so the rollback isn't a silent mystery (EXP-334, web parity).
    let failed: Bool
    /// Protocol v2: one semantic frame carrying every chosen key; the second
    /// argument is the typed reply for a `freeText` option (EXP-513).
    let onAnswer: ([String], String?) -> Void
    /// Image fetching for the prompt's markdown render (EXP-440).
    var markdownContext: AgentMarkdownContext? = nil

    @State private var expanded = false
    /// Tap order is the submit order of the semantic answer frame.
    @State private var picked: [String] = []

    private static let clampChars = 600
    private static let clampLines = 6
    private static let clampHeight: CGFloat = 160
    /// EXP-788: how many rows get a numbered chip — the desktop's keystrokes
    /// are single digits.
    private static let numberedRows = 9

    /// Plans are always fully rendered — never folded (EXP-197).
    private var clampable: Bool {
        !question.planMode
            && (question.text.count > Self.clampChars
                || question.text.filter { $0 == "\n" }.count >= Self.clampLines)
    }

    private var answerable: Bool { active && canAnswer }

    private var headerText: String? {
        if question.planMode { return "Plan ready" }
        var parts: [String] = []
        if let stepLabel {
            parts.append(stepLabel)
        } else if question.isSubmitStep {
            parts.append("Review")
        }
        if let header = question.header { parts.append(header) }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// Multi-select answers batch into one submit; everything else answers on
    /// the option tap.
    private var needsExplicitSubmit: Bool { question.multiSelect }

    private var submitTitle: String { AgentFeed.submitLabel }

    /// EXP-788: the rows drawn — a free-text row is gone (the composer IS the
    /// free answer now, and its placeholder says so).
    private var visibleOptions: [AgentQuestionOption] {
        question.options.filter { !$0.freeText }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(question.planMode ? AppIcons.codingPlan : AppIcons.uiHelp, size: AppIcon.Size.small)
                .foregroundStyle(
                    question.planMode ? DesignTokens.Semantic.blue : DesignTokens.Semantic.yellow
                )
                .padding(.top, 4)
            VStack(alignment: .leading, spacing: 8) {
                if let headerText {
                    Text(headerText)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(
                            question.planMode
                                ? DesignTokens.Semantic.blue
                                : Color.white.opacity(TextOpacity.secondary)
                        )
                }
                priorStepSummary
                prompt
                if question.resolved {
                    resolution
                } else {
                    optionList
                    trailingActions
                }
                if active, !canAnswer {
                    Text(question.planMode
                        ? "Waiting for approval. You're viewing read-only."
                        : "Waiting for an answer. You're viewing read-only.")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        // A bordered card, not a group container: the ask is one free-content
        // block that has to stand off the transcript behind it.
        .glassCard()
    }

    /// The answered steps of this ask, so the stepper still shows what was
    /// asked and what was chosen — the question folded to one line next to
    /// its answer (web `AnsweredStepRow` parity, EXP-588).
    @ViewBuilder
    private var priorStepSummary: some View {
        if !priorSteps.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(priorSteps.enumerated()), id: \.element.id) { position, step in
                    let answer = position < priorAnswers.count ? priorAnswers[position] : nil
                    HStack(alignment: .top, spacing: 6) {
                        AppIcon(step.dismissed ? AppIcons.uiClose : AppIcons.uiCheck, size: 11)
                            .foregroundStyle(
                                step.dismissed
                                    ? Color.white.opacity(TextOpacity.tertiary)
                                    : DesignTokens.Semantic.green
                            )
                            .padding(.top, 2)
                        Text(step.header?.isEmpty == false ? step.header! : step.text)
                            .font(.caption)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Text(answer ?? (step.dismissed ? "Dismissed" : "Answered"))
                            .font(.caption.weight(.medium))
                            .lineLimit(2)
                            .multilineTextAlignment(.trailing)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(maxWidth: 180, alignment: .trailing)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var prompt: some View {
        let clamped = clampable && !expanded
        // No hard line breaks: a prompt or plan body is authored GFM prose,
        // where a wrapped source line is one paragraph.
        AgentMarkdownText(
            text: question.text,
            context: markdownContext,
            options: [.autolinkBareURLs]
        )
        .frame(maxHeight: clamped ? Self.clampHeight : nil, alignment: .top)
        .clipped()
        if clampable {
            Button(expanded ? "Show less" : "Show more") {
                expanded.toggle()
            }
            .font(.caption2.weight(.medium))
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .buttonStyle(.plain)
        }
    }

    @ViewBuilder
    private var resolution: some View {
        HStack(alignment: .top, spacing: 6) {
            AppIcon(question.dismissed ? AppIcons.uiClose : AppIcons.uiCheck, size: 11)
                .foregroundStyle(
                    question.dismissed
                        ? Color.white.opacity(TextOpacity.tertiary)
                        : DesignTokens.Semantic.green
                )
                .padding(.top, 2)
            Text(
                question.answerSummary
                    ?? (question.dismissed ? nil : localAnswer)
                    ?? (question.dismissed ? "Dismissed" : "Answered")
            )
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .textSelection(.enabled)
        }
    }

    /// EXP-788: every option is a real full-width button — a numbered chip
    /// (1..9, the desktop's keystroke) or, on a multi-select, its checkbox,
    /// then the label with the option's description under it. The wire's
    /// first option of a plan is the primary action ("Yes" — the contract
    /// puts it first) and wears the accent.
    @ViewBuilder
    private var optionList: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(visibleOptions.enumerated()), id: \.element.key) { index, option in
                let primary = question.planMode && index == 0
                if answerable {
                    Button {
                        pick(option)
                    } label: {
                        optionRow(
                            option,
                            number: index + 1,
                            primary: primary,
                            checked: question.multiSelect ? picked.contains(option.key) : nil
                        )
                    }
                    .buttonStyle(.plain)
                    .disabled(locked)
                    .opacity(locked ? 0.5 : 1)
                    .accessibilityIdentifier("agent-question-option-\(index + 1)")
                } else {
                    optionRow(
                        option,
                        number: index + 1,
                        primary: primary,
                        checked: question.multiSelect ? false : nil
                    )
                }
            }
        }
    }

    /// EXP-788: no in-card text field — the composer under the transcript is
    /// the free answer (a plan's feedback, a question's typed reply). What is
    /// left here is the multi-select Submit and the lock/retry captions.
    @ViewBuilder
    private var trailingActions: some View {
        if answerable, needsExplicitSubmit {
            // Multi-select submits every picked key at once.
            let disabled = locked || picked.isEmpty
            GlassPill(
                submitTitle,
                mode: .select(isSelected: !disabled) { submit() },
                enabled: !disabled
            )
        }
        if locked, !question.resolved {
            HStack(spacing: 6) {
                if pending {
                    ProgressView().controlSize(.small).tint(.white)
                } else {
                    // Confirmed injected — the card stays locked until the
                    // desktop retires it with `question_resolved`.
                    AppIcon(AppIcons.uiCheck, size: 11)
                        .foregroundStyle(DesignTokens.Semantic.green)
                }
                Text(pending ? "Sending…" : "Answer sent")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        } else if failed, !question.resolved, answerable {
            // The optimistic lock expired with no `answer_ack` — say WHY the
            // step re-surfaced instead of silently rolling back (EXP-334).
            Text("No confirmation from the desktop. Pick again to retry.")
                .font(.caption2)
                .foregroundStyle(DesignTokens.Semantic.yellow)
        }
    }

    private func pick(_ option: AgentQuestionOption) {
        guard !locked else { return }
        if question.multiSelect {
            if let index = picked.firstIndex(of: option.key) {
                picked.remove(at: index)
            } else {
                picked.append(option.key)
            }
            // The keys batch into the submit frame.
            return
        }
        picked = [option.key]
        onAnswer([option.key], nil)
    }

    private func submit() {
        guard !locked, !picked.isEmpty else { return }
        onAnswer(picked, nil)
    }

    /// One option row (EXP-788): the whole width is the hit target (a
    /// plain-style button only hit-tests what it draws, EXP-588), glassRow
    /// rather than the capsule button whose height-derived radius clipped a
    /// two-line description into an ellipse (EXP-274). The primary row is
    /// stroked in the design-tokens blue.
    private func optionRow(
        _ option: AgentQuestionOption,
        number: Int,
        primary: Bool,
        checked: Bool?
    ) -> some View {
        HStack(alignment: .top, spacing: 8) {
            if let checked {
                // Multi-select rows carry an explicit checkbox (EXP-529) —
                // the glassRow tint alone was too subtle to read the picked
                // set off the card. Android parity (QuestionOptionLabel).
                AppIcon(checked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: 13)
                    .foregroundStyle(
                        .white.opacity(checked ? TextOpacity.primary : TextOpacity.tertiary)
                    )
                    .padding(.top, 2)
            } else if number <= Self.numberedRows {
                numberChip(number, primary: primary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(option.label)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .multilineTextAlignment(.leading)
                if let description = option.description, !description.isEmpty {
                    Text(description)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .multilineTextAlignment(.leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .contentShape(Rectangle())
        .glassRow(isActive: primary || checked == true)
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                .stroke(
                    primary ? DesignTokens.Semantic.blue.opacity(0.6) : Color.clear,
                    lineWidth: GlassTokens.hairline
                )
        )
    }

    /// The 1..9 chip — the keystroke the desktop would take. Filled with the
    /// accent on the primary row, a quiet glass square elsewhere.
    private func numberChip(_ number: Int, primary: Bool) -> some View {
        Text("\(number)")
            .font(.caption2.weight(.semibold).monospacedDigit())
            .foregroundStyle(primary ? Color.white : .white.opacity(TextOpacity.secondary))
            .frame(width: 18, height: 18)
            .background(
                primary ? DesignTokens.Semantic.blue : GlassTokens.fillActive,
                in: RoundedRectangle(cornerRadius: 5)
            )
            .accessibilityHidden(true)
    }
}

/// Tool-call headline — compact single line, consecutive rows visually tight.
private struct ToolRow: View {
    let name: String
    let detail: String?
    /// EXP-785: the call failed — the row tints red (web parity) so a
    /// collapsed group's "1 failed" has a row to point at once expanded.
    var failed: Bool = false
    /// EXP-787: a row INSIDE an expanded group keeps the compact inner rhythm
    /// this used to give every tool row; an outermost one is spaced by the
    /// transcript's gap ladder instead.
    var nested: Bool = false
    /// EXP-786: the per-call unified diff an `edit` published, already cut to
    /// the contract's caps by the publisher. Nil for every other call.
    var diff: String? = nil

    /// EXP-806: COLLAPSED by default, unlike web's always-open `ToolDiff` —
    /// a phone transcript is one narrow column, and a dozen open patches
    /// would bury the prose between them.
    @State private var showsDiff = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if diff == nil {
                headline
            } else {
                Button { showsDiff.toggle() } label: {
                    headline.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(showsDiff ? "Hide the diff" : "Show the diff")
            }
            if showsDiff, let diff {
                ToolDiffBlock(diff: diff)
                    .padding(.top, 4)
            }
        }
        .padding(.vertical, nested ? 2 : 0)
    }

    private var headline: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.codingTool, size: 11)
                .foregroundStyle(
                    failed ? DesignTokens.Semantic.red : .white.opacity(TextOpacity.tertiary)
                )
            Text(name)
                .transcriptToolText(.medium)
                .foregroundStyle(failed ? DesignTokens.Semantic.red : .white)
            if let detail {
                Text(Self.middleTruncate(detail))
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Spacer(minLength: 0)
            }
            // The disclosure sits on the TRAILING edge on purpose: a leading
            // chevron would indent the diff-carrying rows out of line with
            // every other tool row in the same run.
            if diff != nil {
                AppIcon(showsDiff ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
    }

    /// Middle-truncate a tool detail (paths etc.) so head AND tail stay
    /// readable (Android AgentSessionScreen parity).
    private static func middleTruncate(_ s: String, max: Int = 72) -> String {
        guard s.count > max else { return s }
        let head = max * 2 / 3
        let tail = max - head - 1
        return String(s.prefix(head)) + "…" + String(s.suffix(tail))
    }
}

/// EXP-806: one call's diff, under its tool row — the same per-file renderer
/// the "Latest changes" sheet uses (`DiffPatchBlock`), in a scroll box no
/// taller than that bar, mirroring web's `ToolDiff`.
///
/// The publisher's cut note is split off FIRST and drawn as a muted footer
/// OUTSIDE the patch: `\ 120 more lines truncated` is metadata about the
/// diff, and inside the block `DiffRendering.kind` would colour it as a
/// context line the agent supposedly read. That footer is a different fact
/// from `DiffPatchBlock`'s own "Diff truncated…" line, which reports THIS
/// renderer's 600-line layout cap — both can show at once and mean different
/// things.
private struct ToolDiffBlock: View {
    let diff: String

    /// Web's `max-h-72`. A box tall enough to read a hunk in, short enough
    /// that the prose after the call stays on screen.
    private static let maxHeight: CGFloat = 288

    var body: some View {
        let split = AgentFeed.splitTruncatedDiff(diff)
        let sections = DiffRendering.splitFiles(split.diff)
        if !sections.isEmpty || split.truncated != nil {
            ScrollView(.vertical) {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(sections) { section in
                        if let filename = section.filename {
                            Text(filename)
                                .font(.caption2.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        DiffPatchBlock(patch: section.patch)
                    }
                    if let truncated = split.truncated {
                        Text(AgentFeed.diffTruncationNote(truncated))
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: Self.maxHeight)
            // Web's `overscroll-contain`: with a short patch there is nothing
            // to scroll here, so the drag belongs to the transcript.
            .scrollBounceBehavior(.basedOnSize)
            .background(Color.white.opacity(0.03))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
            )
        }
    }
}

/// A run of ≥2 consecutive tool calls collapsed into one "N tool calls" row
/// (EXP-97), expandable to the individual rows. While the run is the trailing
/// row of a live session, the latest call stays visible under the count so
/// the viewer still sees live progress.
private struct ToolGroupRow: View {
    let items: [AgentFeedItem]
    let liveTail: Bool

    @State private var expanded = false

    /// EXP-785: the collapsed caption is what the calls DID ("Ran 3 commands
    /// · edited 2 files"), the one summary every client derives from the
    /// contract fixture, not a bare count.
    private var caption: String {
        ToolGroupSummary.summarize(items.compactMap { item in
            guard case let .tool(_, _, detail, _, _, kind, _, failed, _) = item else { return nil }
            return ToolCallSummary(kind: kind ?? "other", detail: detail, failed: failed)
        })
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                expanded.toggle()
            } label: {
                HStack(spacing: 8) {
                    AppIcon(expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    AppIcon(AppIcons.codingTool, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text(caption)
                        .transcriptToolText(.medium)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse tool calls" : "Expand tool calls")
            if expanded {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(items) { item in
                        if case let .tool(_, name, detail, _, _, _, _, failed, diff) = item {
                            ToolRow(
                                name: name, detail: detail, failed: failed,
                                nested: true, diff: diff
                            )
                        }
                    }
                }
                .padding(.leading, 20)
            } else if liveTail, let last = items.last,
                      case let .tool(_, name, detail, _, _, _, _, failed, diff) = last {
                ToolRow(
                    name: name, detail: detail, failed: failed, nested: true, diff: diff
                )
                    .padding(.leading, 20)
            }
        }
    }
}

/// A subagent's run (EXP-249): its `started` marker plus every tool call the
/// desktop tagged with it, collapsed into one expandable row so a long subagent
/// detour never buries the main thread's activity. The header always shows the
/// agent type, a running/done status, and the delegation detail; expansion only
/// reveals the tool calls — so a run with none renders as a static row with no
/// chevron (EXP-350: the chevron used to expand to nothing).
private struct SubagentGroupRow: View {
    let run: AgentSubagentRun
    /// The trailing row of a live session — keep the latest call visible.
    let liveTail: Bool
    /// EXP-773: the group carries the subagent's own prose now, so it needs
    /// what an `AgentMarkdownText` needs.
    let context: AgentMarkdownContext

    @State private var expanded = false

    private var title: String {
        let count = run.toolCount
        let work = count == 1 ? "1 tool call" : "\(count) tool calls"
        return count == 0 ? run.agentType : "\(run.agentType) · \(work)"
    }

    private var header: some View {
        HStack(spacing: 8) {
            if run.expandable {
                AppIcon(expanded ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            AppIcon(AppIcons.codingSubagent, size: 11)
                .foregroundStyle(DesignTokens.Semantic.blue)
            Text(title)
                .transcriptToolText(.medium)
                .foregroundStyle(.white)
                .lineLimit(1)
            Text(run.done ? "done" : "running…")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if run.expandable {
                Button {
                    expanded.toggle()
                } label: {
                    header.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(expanded ? "Collapse subagent" : "Expand subagent")
            } else {
                header
            }
            if let detail = run.detail {
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(2)
                    .padding(.leading, 20)
            }
            if expanded {
                // EXP-773: the run's whole conversation in order — its prose
                // and the turns addressed to it, not just its tool calls.
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(run.items) { item in
                        SubagentItemRow(item: item, context: context)
                    }
                }
                .padding(.leading, 20)
            } else if liveTail, let last = run.items.last {
                SubagentItemRow(item: last, context: context)
                    .padding(.leading, 20)
            }
        }
    }
}

/// EXP-773: one row of a subagent's conversation — its prose, a turn
/// addressed to it, or one of its tool calls. Anything else a group somehow
/// collected renders nothing rather than crashing the feed.
private struct SubagentItemRow: View {
    let item: AgentFeedItem
    let context: AgentMarkdownContext
    /// EXP-787: inside an expanded group the conversation keeps its own
    /// compact rhythm; the focused-subagent list spaces its rows with the
    /// transcript's gap ladder instead.
    var nested: Bool = true

    var body: some View {
        content.padding(.vertical, nested ? 2 : 0)
    }

    @ViewBuilder
    private var content: some View {
        switch item {
        case let .tool(_, name, detail, _, _, _, _, failed, diff):
            ToolRow(name: name, detail: detail, failed: failed, diff: diff)
        case let .narration(_, text, _, _):
            NarrationBubble(text: text, context: context)
        case let .userMessage(_, text, _):
            UserMessageBubble(text: text, context: context)
        default:
            EmptyView()
        }
    }
}

/// A stray subagent marker (EXP-249) — a `completed` whose `started` fell off
/// the top of the feed, or a start with nothing published under it yet.
private struct SubagentRow: View {
    let agentType: String
    let status: AgentSubagentStatus
    let detail: String?

    var body: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.codingSubagent, size: 11)
                .foregroundStyle(DesignTokens.Semantic.blue)
            Text(status == .completed ? "\(agentType) finished" : "\(agentType) started")
                .transcriptToolText(.medium)
                .foregroundStyle(.white)
            if let detail {
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Spacer(minLength: 0)
            }
        }
    }
}

/// A permission prompt the agent hit (EXP-249) — INFORMATIONAL only: the
/// approval lives in the desktop's own TUI, there is nothing to answer here.
private struct PermissionRow: View {
    let tool: String
    let detail: String?

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.uiPermission, size: 11)
                .foregroundStyle(DesignTokens.Semantic.yellow)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 2) {
                Text("Permission requested · \(tool)")
                    .transcriptToolText(.medium)
                    .foregroundStyle(.white)
                if let detail {
                    Text(detail)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(2)
                }
                Text("Approve on the desktop, or reply below to continue.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Spacer(minLength: 0)
        }
    }
}

/// EXP-724: the quiet marker a finished compaction leaves in the feed, so the
/// gap in the conversation above it stays explained after the strip is gone.
/// Centered and muted — it is punctuation, not a turn. `compactedLabel` is
/// byte-identical on all four clients.
private struct CompactionMarkerRow: View {
    var body: some View {
        HStack(spacing: 6) {
            Spacer(minLength: 0)
            AppIcon(AppIcons.codingCompact, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(AgentFeed.compactedLabel)
                .transcriptToolText()
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
    }
}

/// EXP-724: a steered slash command in the feed. It is a control action, not
/// prose, so it gets a compact pill on the human side of the conversation
/// instead of a markdown bubble: `square-slash` · mono `/name` · muted args.
private struct CommandPill: View {
    let command: SlashCommand
    /// The message as sent — everything after the command token is its
    /// argument.
    let text: String

    /// Everything after the command token — the match ran on that first
    /// whitespace token, so the rest of the message is exactly the argument.
    private var arguments: String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let space = trimmed.firstIndex(where: { $0.isWhitespace }) else { return "" }
        return String(trimmed[space...]).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        HStack {
            Spacer(minLength: 32)
            HStack(spacing: 6) {
                AppIcon(AppIcons.codingCommand, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(command.token)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white)
                if !arguments.isEmpty {
                    Text(arguments)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .glassRow()
        }
    }
}

// MARK: - Follow-scroll geometry

/// Derives "pinned to the bottom" from the feed's scroll geometry. iOS 18+
/// reads onScrollGeometryChange — the supported scroll-observation API; the
/// EXP-70 content-frame preference (still emitted by the feed's background)
/// stopped re-evaluating during scrolls on current iOS, leaving `atBottom`
/// stuck true so the "Jump to bottom" pill never appeared and follow-scroll
/// couldn't be escaped (EXP-212). Pre-18 keeps the preference path.
private struct FollowPinTracker: ViewModifier {
    @Binding var atBottom: Bool
    /// Within this many points of the bottom still counts as pinned.
    let slack: CGFloat
    /// Re-assert the bottom pin (a scrollTo) after the CONTENT grew while
    /// pinned — in-place feed mutations (a question card filling in, a tool
    /// group's live tail) change no row count, so the feed-count follow
    /// misses them. Without this the growth pushed the bottom out from
    /// under a pinned viewer, `atBottom` flipped false with no user gesture,
    /// and the "Jump to bottom" pill sat on screen permanently (EXP-272).
    let repin: () -> Void

    /// A user scroll gesture is driving the offset (drag or its momentum).
    @State private var userScrolling = false
    /// The latest geometry sample's pin verdict — lets the phase observer
    /// re-pin once a user scroll ends with `atBottom` still true but the
    /// viewport left short of the end (content grew under the finger).
    @State private var lastPinned = true
    /// A growth repin is already queued for the next run-loop turn.
    @State private var repinQueued = false
    /// Growth repins issued since the feed last rested at its true bottom
    /// or the user last scrolled. Bounded: a repin that keeps landing short
    /// (lazy rows realising under it, an OS layout bug) must not become a
    /// relayout storm — build 94 was watchdog-killed with the main thread
    /// 100% in lazy-stack layout and +380MB of view-list copies in 84s.
    @State private var growthRepins = 0
    private static let maxGrowthRepins = 8

    /// Queue ONE repin for the next run-loop turn. The growth observer runs
    /// inside the scroll view's layout transaction; calling scrollTo there
    /// re-enters layout synchronously (seen in the build 92 watchdog
    /// stack: onScrollGeometryChange action → scrollTo → LazyStack layout),
    /// and every scrollTo that realises more lazy rows changes the content
    /// height, which fires the observer again before the run loop ever
    /// turns. Deferring breaks the re-entrancy and coalesces a burst of
    /// growth samples into a single scroll.
    private func queueGrowthRepin() {
        guard !repinQueued, growthRepins < Self.maxGrowthRepins else { return }
        repinQueued = true
        growthRepins += 1
        Task { @MainActor in
            repinQueued = false
            guard atBottom, !userScrolling else { return }
            repin()
        }
    }

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content
                .onScrollPhaseChange { _, newPhase in
                    userScrolling = newPhase == .tracking
                        || newPhase == .interacting
                        || newPhase == .decelerating
                    if userScrolling { growthRepins = 0 }
                    // The growth observer skips repins during user scrolls
                    // (EXP-306); catch up once the gesture settles so a
                    // followed feed never idles a few points shy of its end.
                    if newPhase == .idle, atBottom, !lastPinned {
                        repin()
                    }
                }
                .onScrollGeometryChange(for: FeedPinMetrics.self) { geometry in
                    // Pinned ⇔ the content's bottom edge is within slack of
                    // the visible rect's bottom edge (Android parity: last
                    // item bottom vs viewport end). `visibleRect` already
                    // accounts for the content insets, so nothing is
                    // reconstructed from contentOffset/containerSize/insets —
                    // the EXP-212 max-offset formula overstated the reachable
                    // maximum whenever an inset (nav bar, keyboard, safe area)
                    // didn't line up, and the true bottom then read as "not
                    // pinned", stranding the "Jump to bottom" pill (EXP-591).
                    // A feed shorter than the viewport and a bounce past the
                    // end both come out negative, i.e. pinned (EXP-242).
                    let below = geometry.contentSize.height - geometry.visibleRect.maxY
                    return FeedPinMetrics(
                        pinned: below <= slack,
                        offset: geometry.contentOffset.y,
                        height: geometry.contentSize.height
                    )
                } action: { old, new in
                    lastPinned = new.pinned
                    // The rule itself is pure (ExpCore's FeedFollowPolicy):
                    // reaching the bottom re-arms follow, LEAVING it is the
                    // user's alone (Android parity: only drags flip `follow`),
                    // and a pin the CONTENT produced under a stationary reader
                    // holds — EXP-656, where a staged replay committing a
                    // shorter history pulled the bottom edge into slack range
                    // and scrolled a reader out of the plan they were reading.
                    switch FeedFollowPolicy.decide(
                        pinned: new.pinned,
                        atBottom: atBottom,
                        userScrolling: userScrolling,
                        offsetDelta: Double(new.offset - old.offset),
                        heightDelta: Double(new.height - old.height)
                    ) {
                    case .rearm:
                        if !atBottom { atBottom = true }
                    case .unpin:
                        atBottom = false
                    case .hold:
                        break
                    }
                }
                .onScrollGeometryChange(for: FeedGrowthMetrics.self) { geometry in
                    FeedGrowthMetrics(
                        height: geometry.contentSize.height,
                        below: geometry.contentSize.height - geometry.visibleRect.maxY
                    )
                } action: { old, new in
                    // Resting at (or bouncing past) the true bottom re-arms
                    // the growth budget: the last repin converged.
                    if new.below <= 0 { growthRepins = 0 }
                    // Only chase growth that actually pushed the bottom out
                    // of view; a scrollTo whose target is already visible is
                    // pure layout churn.
                    if new.height > old.height, new.below > 0, atBottom, !userScrolling {
                        queueGrowthRepin()
                    }
                }
        } else {
            content.onPreferenceChange(FeedBottomOverflowKey.self) { [atBottom = $atBottom] overflow in
                // Points of content extending below the viewport; ≤ slack
                // counts as pinned. Only the flip writes state.
                let pinned = overflow <= slack
                if atBottom.wrappedValue != pinned {
                    atBottom.wrappedValue = pinned
                }
            }
        }
    }
}

/// Scroll-geometry sample the iOS 18+ pin tracker acts on: pinned-to-bottom
/// plus the raw offset and content height, so the action can tell a user
/// scroll AWAY from the bottom (offset moved up) apart from content growth
/// un-pinning the geometry under a stationary finger (EXP-306) and from
/// content SHRINKING into a pin nobody scrolled into (EXP-656).
private struct FeedPinMetrics: Equatable {
    var pinned: Bool
    var offset: CGFloat
    var height: CGFloat
}

/// Content height plus how far the content's bottom edge sits below the
/// visible rect (≤ 0 when the end is on screen) — the growth observer's
/// sample.
private struct FeedGrowthMetrics: Equatable {
    var height: CGFloat
    var below: CGFloat
}

/// Points of feed content extending below the visible viewport — 0 when the
/// feed is pinned to the bottom, negative while bouncing past it. Pre-iOS-18
/// fallback input to FollowPinTracker.
private struct FeedBottomOverflowKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

// MARK: - Latest-changes diff sheet

/// The pinned "Latest changes" diff, expanded: the latest worktree diff (raw
/// `git diff` output) split on `diff --git` into per-file glass sections with
/// the shared DiffRendering coloring — horizontal panning stays inside each
/// file's code block only.
private struct LatestChangesSheet: View {
    let diff: String

    var body: some View {
        let stats = DiffRendering.stats(of: diff)
        let sections = DiffRendering.splitFiles(diff)
        GlassSheetChrome(
            title: "Changes",
            height: .full,
            headerTrailing: {
                HStack(spacing: 8) {
                    Text("+\(stats.additions)")
                        .font(.caption.monospaced())
                        .foregroundStyle(.green)
                    Text("−\(stats.deletions)")
                        .font(.caption.monospaced())
                        .foregroundStyle(.red)
                }
            },
            content: {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 10) {
                        ForEach(sections) { section in
                            VStack(alignment: .leading, spacing: 0) {
                                if let filename = section.filename {
                                    Text(filename)
                                        .font(.caption.monospaced())
                                        .foregroundStyle(.white)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                        .padding(.horizontal, 12)
                                        .padding(.vertical, 8)
                                }
                                DiffPatchBlock(patch: section.patch)
                                    .padding(.horizontal, 8)
                                    .padding(.top, section.filename == nil ? 8 : 0)
                                    .padding(.bottom, 8)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            // One file among many in a gapped stack — a row,
                            // not a borderless group.
                            .glassRow()
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 24)
                }
            }
        )
    }
}
