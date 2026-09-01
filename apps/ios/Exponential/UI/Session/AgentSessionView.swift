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
    /// EXP-696: whether THIS screen ever saw the run live — the auto-back on
    /// the ended edge only fires after that, so a finished run's feed opened
    /// from a list stays browsable.
    @State private var sawLiveSession = false
    @FocusState private var inputFocused: Bool

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
        model?.agentUsage != nil || model?.canKill == true
    }

    @ViewBuilder
    private var toolbarMenuItems: some View {
        if model?.agentUsage != nil {
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

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                if let model {
                    feedArea(model)
                    banners(model)
                    bottomBar(model)
                } else {
                    Spacer()
                }
            }
        }
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
                    Text(headerCaption)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
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
        .alert("Kill this coding session?", isPresented: $showKillConfirm) {
            Button("Kill session", role: .destructive) {
                Task { await model?.killSession() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This force-terminates the agent's terminal on the desktop and ends the session.")
        }
        // EXP-678: merging from the steering screen — same confirm-gated flow
        // as the Agents list and Reviews.
        .alert("Merge pull request?", isPresented: $showMergeConfirm) {
            Button("Merge", role: .destructive) {
                if let model { merge(model) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Merges the pull request, completes every linked issue, and closes the coding session.")
        }
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
        .onChange(of: model?.sessionEnded) { _, ended in
            if ended == false {
                sawLiveSession = true
            } else if ended == true && sawLiveSession && fixSessionTarget == nil {
                dismiss()
            }
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
        }
        // EXP-706: the "Fix conflicts" launcher — the machines it can run on
        // resolve off the synced devices shape, once steering is known on.
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
            await refreshFixTargets()
        }
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
            if let usage = model?.agentUsage {
                AgentUsageSheet(usage: usage.usage, account: model?.agentAccount)
            }
        }
    }

    // MARK: - Header

    /// Everything an `AgentMarkdownText` needs to render embedded images —
    /// agent prose can carry `![](/api/attachments/{id})` and those fetches are
    /// authenticated (EXP-440).
    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient
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
        CodingSessionDisplayState.of(
            session: model?.session ?? session, prState: headerIssue?.prState
        )
    }

    /// Line 2: what the header used to say on its own — the phase, and the
    /// machine the run is parked on.
    private var headerCaption: String {
        let label = model?.hostDevice.label ?? session.deviceLabel
        let device = (label?.isEmpty == false) ? " · \(label!)" : ""
        if hostPaused { return "Paused\(device)" }
        switch model?.phase {
        case .live:
            // A trailing question/plan means the session is blocked on a
            // human — say so instead of looking silently stuck (EXP-97).
            return model?.awaitingInput == true
                ? "Needs your input\(device)"
                : "Live\(device)"
        case .ended: return "Session ended"
        case let .closed(_, reconnecting): return reconnecting ? "Reconnecting…" : "Disconnected"
        default: return "Connecting…"
        }
    }

    // MARK: - Feed

    @ViewBuilder
    private func feedArea(_ model: AgentSessionModel) -> some View {
        if model.feed.isEmpty, hostPaused {
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
                .padding(.horizontal, 14)
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
        Button(action: action) {
            HStack(spacing: 5) {
                Text(label)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(
                        selected ? TextOpacity.primary : TextOpacity.secondary
                    ))
                    .lineLimit(1)
                if running {
                    ProgressView()
                        .controlSize(.mini)
                        .tint(.white)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
        }
        .glassButton(isActive: selected)
        .buttonStyle(.plain)
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
                                Text("No tool calls yet.")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.vertical, 4)
                            } else {
                                ForEach(focused.items) { item in
                                    if case let .tool(_, name, detail, _) = item {
                                        ToolRow(name: name, detail: detail)
                                    }
                                }
                            }
                        } else {
                            // Consecutive tool calls collapse into "N tool
                            // calls" rows (EXP-97) — a render projection; the
                            // flat feed (and the trailing-question rule) stays
                            // the state.
                            let rows = model.rows
                            ForEach(rows) { row in
                                feedRow(row, isLast: row.id == rows.last?.id)
                            }
                            // EXP-389: the agent-is-busy footer — live and
                            // nothing waiting on the user (Android parity).
                            if isWorking(model) {
                                WorkingIndicatorRow()
                            }
                        }
                        Color.clear
                            .frame(height: 1)
                            .id(Self.bottomAnchor)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
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
                .onChange(of: agentTab) { _, _ in
                    atBottom = true
                    proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                }
                .overlay(alignment: .bottom) {
                    if !atBottom {
                        Button {
                            // Re-arm follow directly (Android parity: the pill
                            // tap sets follow=true) instead of waiting for the
                            // scroll geometry to flip atBottom — while the
                            // agent streams, the animated scrollTo targets the
                            // bottom as of the tap, the content keeps growing
                            // underneath it, and the animation lands short of
                            // the moving max offset, so the pill never
                            // vanished (EXP-306). With atBottom set here the
                            // pill hides at once and the growth observer keeps
                            // chasing the bottom.
                            atBottom = true
                            withAnimation {
                                proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                            }
                        } label: {
                            Text("Jump to bottom ↓")
                                .font(.caption.weight(.medium))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 8)
                        }
                        // Opaque: the feed scrolls beneath this pill
                        // (EXP-165 Android parity, EXP-242).
                        .glassButton(isActive: true, isOpaque: true)
                        .buttonStyle(.plain)
                        .padding(.bottom, 8)
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

    @ViewBuilder
    private func feedRow(_ row: AgentFeedRow, isLast: Bool) -> some View {
        switch row {
        case let .toolRun(items):
            ToolGroupRow(items: items, liveTail: isLast && model?.phase == .live)
        case let .subagentRun(run):
            SubagentGroupRow(run: run, liveTail: isLast && model?.phase == .live)
        case let .ask(group):
            askCard(group)
        case let .single(item):
            switch item {
            case let .narration(_, text):
                NarrationBubble(text: text, context: markdownContext)
            case let .tool(_, name, detail, _):
                ToolRow(name: name, detail: detail)
            case let .userMessage(_, text):
                UserMessageBubble(text: text, context: markdownContext)
            case let .question(question):
                questionCard(question)
            case let .subagent(_, _, agentType, status, detail):
                SubagentRow(agentType: agentType, status: status, detail: detail)
            case let .permission(_, tool, detail):
                PermissionRow(tool: tool, detail: detail)
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

    private func bannerRow(@ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 8) {
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
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
            composerCard(model)
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
        }
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
            .padding(.horizontal, 14)
            .padding(.bottom, 8)
            .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { height in
                changesBarHeight = height
            }
        }
    }

    /// Pinned collapsible "Latest changes" chip — +/− counts, opens the diff
    /// sheet. The latest worktree diff replaces the previous one.
    private func diffChip(_ diff: String) -> some View {
        let stats = DiffRendering.stats(of: diff)
        return Button {
            showDiffSheet = true
        } label: {
            HStack(spacing: 8) {
                AppIcon(AppIcons.codingDiff, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Latest changes")
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
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .glassRow()
    }

    /// The Merge pill beside the diff chip — merging always ends the run too
    /// (EXP-498), so it only shows while there IS an open PR (`model.canMerge`,
    /// which resolves a batch run's PR through EXP-535's representative issue).
    private func mergePill(_ model: AgentSessionModel) -> some View {
        Button {
            showMergeConfirm = true
        } label: {
            GlassPillLabel("Merge", enabled: !merging, verticalPadding: 10) {
                if merging {
                    ProgressView().controlSize(.mini).tint(.white)
                } else {
                    AppIcon(AppIcons.prMerged, size: 14)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .disabled(merging)
        .accessibilityLabel("Merge pull request")
    }

    /// EXP-706: the recovery run in the Merge pill's slot — same glass pill,
    /// same height, opening the shared "Fix merge conflicts" launcher seeded
    /// with THIS run's pull request.
    private func fixConflictsPill() -> some View {
        Button {
            fixSheetOpen = true
        } label: {
            GlassPillLabel("Fix conflicts", verticalPadding: 10) {
                AppIcon(AppIcons.uiBranch, size: 14)
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Fix merge conflicts")
    }

    /// Only a REAL content conflict (EXP-533) gets the run — every other
    /// refusal (stale head, branch protection, a misconfigured GitHub App) is
    /// something no rebase can fix. The run rebases the PR's branch, so one
    /// must be recorded on the issue behind the merge.
    private var canFixConflicts: Bool {
        steerEnabled
            && mergeFailure?.isConflict == true
            && !(model?.mergeIssue?.branch ?? "").isEmpty
    }

    /// Merge the session's PR. No local surgery on success: the server ends
    /// the session and flips `pr_state`, and both land here through sync.
    private func merge(_ model: AgentSessionModel) {
        guard let issueId = model.mergeIssue?.id else { return }
        mergeFailure = nil
        merging = true
        Task {
            do {
                try await deps.issuesApi.mergePr(accountId: accountId, issueId: issueId)
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

    /// EXP-554: the steer composer now wears the comment composer's chrome — ONE
    /// rounded (rc20) ultraThinMaterial card holding the pending strip, a
    /// transparent field, and the `[+]`·spacer·send row. Behavior is untouched:
    /// four images max, the same `sendSteerImages` upload, the same frozen
    /// `SteerImageMessage` wire format.
    private func composerCard(_ model: AgentSessionModel) -> some View {
        @Bindable var model = model
        // Images attach to the session's ISSUE, so a batch or action run has
        // nowhere to put them (EXP-511).
        let canAttach = model.session?.issueId != nil
        let attachFull = model.pendingImages.count >= SteerImageMessage.maxImages
        let canSend = !model.trimmedDraft.isEmpty || !model.pendingImages.isEmpty
        // EXP-621: only SENDING waits for the socket — the field, the picker
        // and the strip stay usable so the draft is ready the moment the
        // reconnect lands.
        let sendDisabled = !canSend || model.steerSending || !model.canSteer
        let attachDisabled = attachFull || model.steerSending
        return VStack(alignment: .leading, spacing: 0) {
            if !model.pendingImages.isEmpty {
                PendingAttachmentStrip(items: model.pendingImages) { id in
                    model.pendingImages.removeAll { $0.id == id }
                }
                .padding(.horizontal, 8)
                .padding(.top, 8)
            }

            if let imageError = model.steerImageError {
                Text(imageError)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.top, 8)
            }

            TextField(
                // A pending plan approval routes free text into the plan
                // feedback flow (the desktop Esc's the picker and types the
                // message) — say so instead of the generic prompt (EXP-529).
                model.awaitingPlanApproval
                    ? "Tell Claude what to change…" : "Message the agent…",
                text: $model.draftText,
                axis: .vertical
            )
                .textFieldStyle(.plain)
                .lineLimit(1...4)
                .font(.subheadline)
                .foregroundStyle(.white)
                .focused($inputFocused)
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 4)

            HStack(spacing: 2) {
                if canAttach {
                    Button {
                        showPhotoPicker = true
                    } label: {
                        AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 36, height: 36)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(attachDisabled)
                    .opacity(attachDisabled ? 0.4 : 1)
                    .accessibilityLabel("Attach image")
                }

                Spacer(minLength: 0)

                Button {
                    sendMessage(model)
                } label: {
                    AppIcon(AppIcons.uiSend, size: 28)
                        .foregroundStyle(
                            sendDisabled ? Color.white.opacity(0.3) : Color.white
                        )
                }
                .buttonStyle(.plain)
                .disabled(sendDisabled)
                .accessibilityLabel("Send")
            }
            .padding(.horizontal, 8)
            .padding(.bottom, 6)
        }
        .background(.ultraThinMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 20))
        .overlay(
            RoundedRectangle(cornerRadius: 20).stroke(Color.white.opacity(0.12), lineWidth: 0.5)
        )
        .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
    }

    private func sendMessage(_ model: AgentSessionModel) {
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
            model.pendingImages.append(PendingSteerImage(
                data: normalized.data,
                filename: normalized.filename,
                contentType: normalized.contentType,
                uploadedId: nil
            ))
        }
    }
}

// MARK: - Feed rows

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
        .padding(.vertical, 5)
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
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .padding(.vertical, 5)
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
    private static let clampHeight: CGFloat = 220

    private var clampable: Bool {
        text.count > Self.clampChars
            || text.filter { $0 == "\n" }.count >= Self.clampLines
    }

    var body: some View {
        HStack {
            Spacer(minLength: 32)
            VStack(alignment: .leading, spacing: 4) {
                AgentMarkdownText(
                    text: text,
                    context: context,
                    options: [.autolinkBareURLs, .hardLineBreaks],
                    hugsWidth: true
                )
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
        .padding(.vertical, 5)
    }
}

/// An interactive question (EXP-78): AskUserQuestion step or plan approval.
///
/// Protocol v2 (EXP-249) cards carry a wire id, so a tap sends ONE semantic
/// `answer` frame (the desktop maps keys to its own picker and confirms with
/// `answer_ack`) and the card locks the moment it goes out — a second tap used
/// to land on the NEXT question. Legacy cards keep the raw-keystroke path: a
/// digit alone for single-select (never a trailing `\r`, which cascaded), digit
/// toggles + Tab for multi-select. `planMode` cards (EXP-97) get a dedicated
/// "Plan ready" presentation with the first wire option as the primary approve
/// action; prompt and plan bodies render as markdown, which is what claude
/// writes.
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
    /// EXP-513: the freeText option whose inline input is open (its key).
    @State private var freeTextKey: String?
    @State private var freeTextValue = ""
    @FocusState private var freeTextFocused: Bool

    private static let clampChars = 600
    private static let clampLines = 6
    private static let clampHeight: CGFloat = 220
    /// The relay's answer frame rejects `text` past 4000 UTF-16 units WHOLE
    /// (steer-relay protocol.ts — dropped, not truncated), so an oversize
    /// reply would silently fail at the 8s lock and retry could never
    /// succeed. Web caps via maxLength=4000, Android via .take(4000).
    private static let freeTextMaxUtf16 = 4000

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

    private var submitTitle: String { "Submit" }

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
        .glassSection()
        .padding(.vertical, 5)
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

    @ViewBuilder
    private var optionList: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(question.options.enumerated()), id: \.element.key) { index, option in
                if answerable {
                    // The wire's first option of a plan is the primary approve
                    // action ("Approve — auto-accept edits").
                    let primary = question.planMode && index == 0
                    Button {
                        pick(option)
                    } label: {
                        optionLabel(
                            option,
                            showKey: showsKeyBadge(option),
                            checked: question.multiSelect
                                ? picked.contains(option.key) : nil
                        )
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        // A plain-style button only hit-tests its drawn
                        // content — the row's empty trailing space (most of
                        // a short "Submit answers" option) ignored taps
                        // (EXP-588). The shape makes the whole row the target.
                        .contentShape(Rectangle())
                    }
                    // glassRow, not glassButton: the capsule's height-derived
                    // radius clipped multi-line option descriptions into an
                    // ellipse (EXP-274).
                    .glassRow(
                        isActive: primary || picked.contains(option.key)
                            || freeTextKey == option.key
                    )
                    .buttonStyle(.plain)
                    .disabled(locked)
                    .opacity(locked ? 0.5 : 1)
                } else {
                    optionLabel(
                        option,
                        showKey: showsKeyBadge(option),
                        checked: question.multiSelect ? false : nil
                    )
                }
            }
        }
    }

    @ViewBuilder
    private var trailingActions: some View {
        if answerable, freeTextKey != nil {
            // EXP-513: the inline reply for the selected freeText row.
            HStack(spacing: 6) {
                TextField("Type your answer…", text: $freeTextValue)
                    .textFieldStyle(.plain)
                    .font(.caption)
                    .foregroundStyle(.white)
                    .focused($freeTextFocused)
                    .onChange(of: freeTextValue) { _, newValue in
                        freeTextValue = Self.capFreeText(newValue)
                    }
                    .onSubmit { submitFreeText() }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .glassRow(isActive: true)
                let disabled = freeTextValue
                    .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                Button {
                    submitFreeText()
                } label: {
                    AppIcon(AppIcons.uiSend, size: 13)
                        .foregroundStyle(.white)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                }
                .glassButton(isActive: !disabled)
                .buttonStyle(.plain)
                .disabled(disabled)
                .opacity(disabled ? 0.5 : 1)
            }
        }
        if answerable, needsExplicitSubmit {
            // Multi-select submits every picked key at once.
            let disabled = locked || picked.isEmpty
            GlassPillButton(submitTitle, isActive: !disabled, enabled: !disabled) {
                submit()
            }
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

    /// The wire key doubles as the TUI hint badge — but only when it reads as
    /// one: an ask's submit step carries `\r`, and a plan's approve action is
    /// the primary button, not a keystroke.
    private func showsKeyBadge(_ option: AgentQuestionOption) -> Bool {
        guard !question.planMode else { return false }
        return option.key.allSatisfy { $0.isLetter || $0.isNumber }
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
        if option.freeText {
            // EXP-513: collect the reply first — nothing is sent until it
            // submits (the desktop types it into the TUI row).
            freeTextKey = freeTextKey == option.key ? nil : option.key
            freeTextFocused = freeTextKey != nil
            return
        }
        picked = [option.key]
        onAnswer([option.key], nil)
    }

    /// Truncate to the relay's 4000-UTF-16-unit answer cap, backing off one
    /// unit rather than splitting a surrogate pair (4001 units would still
    /// be dropped whole — same convention as `sendMessage`'s chunker).
    private static func capFreeText(_ text: String) -> String {
        let units = Array(text.utf16)
        guard units.count > freeTextMaxUtf16 else { return text }
        var end = freeTextMaxUtf16
        if UTF16.isLeadSurrogate(units[end - 1]) { end -= 1 }
        return String(decoding: units[0..<end], as: UTF16.self)
    }

    private func submitFreeText() {
        guard !locked, let key = freeTextKey else { return }
        let text = Self.capFreeText(
            freeTextValue.trimmingCharacters(in: .whitespacesAndNewlines)
        )
        guard !text.isEmpty else { return }
        picked = [key]
        onAnswer([key], text)
        freeTextKey = nil
        freeTextValue = ""
    }

    private func submit() {
        guard !locked, !picked.isEmpty else { return }
        onAnswer(picked, nil)
    }

    private func optionLabel(
        _ option: AgentQuestionOption,
        showKey: Bool = true,
        checked: Bool? = nil
    ) -> some View {
        HStack(alignment: .top, spacing: 6) {
            if let checked {
                // Multi-select rows carry an explicit checkbox (EXP-529) —
                // the glassRow tint alone was too subtle to read the picked
                // set off the card. Android parity (QuestionOptionLabel).
                AppIcon(checked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: 13)
                    .foregroundStyle(
                        .white.opacity(checked ? TextOpacity.primary : TextOpacity.tertiary)
                    )
                    .padding(.top, 1)
            } else if showKey {
                Text(option.key)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(option.label)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .multilineTextAlignment(.leading)
                if let description = option.description {
                    Text(description)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .multilineTextAlignment(.leading)
                }
            }
        }
    }
}

/// Tool-call headline — compact single line, consecutive rows visually tight.
private struct ToolRow: View {
    let name: String
    let detail: String?

    var body: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.codingTool, size: 11)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(name)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
            if let detail {
                Text(Self.middleTruncate(detail))
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Spacer(minLength: 0)
            }
        }
        .padding(.vertical, 2)
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

/// A run of ≥2 consecutive tool calls collapsed into one "N tool calls" row
/// (EXP-97), expandable to the individual rows. While the run is the trailing
/// row of a live session, the latest call stays visible under the count so
/// the viewer still sees live progress.
private struct ToolGroupRow: View {
    let items: [AgentFeedItem]
    let liveTail: Bool

    @State private var expanded = false

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
                    Text("\(items.count) tool calls")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                    Spacer(minLength: 0)
                }
                .padding(.vertical, 2)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse tool calls" : "Expand tool calls")
            if expanded {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(items) { item in
                        if case let .tool(_, name, detail, _) = item {
                            ToolRow(name: name, detail: detail)
                        }
                    }
                }
                .padding(.leading, 20)
            } else if liveTail, let last = items.last,
                      case let .tool(_, name, detail, _) = last {
                ToolRow(name: name, detail: detail)
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
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
            Text(run.done ? "done" : "running…")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
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
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(run.items) { item in
                        if case let .tool(_, name, detail, _) = item {
                            ToolRow(name: name, detail: detail)
                        }
                    }
                }
                .padding(.leading, 20)
            } else if liveTail, let last = run.items.last,
                      case let .tool(_, name, detail, _) = last {
                ToolRow(name: name, detail: detail)
                    .padding(.leading, 20)
            }
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
                .font(.caption.weight(.medium))
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
        .padding(.vertical, 2)
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
                .foregroundStyle(DesignTokens.Semantic.orange)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 2) {
                Text("Permission requested · \(tool)")
                    .font(.caption.weight(.medium))
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
        .padding(.vertical, 4)
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
            title: "Latest changes",
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
                            .glassSection()
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 24)
                }
            }
        )
    }
}
