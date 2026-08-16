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
    @Environment(\.scenePhase) private var scenePhase
    @State private var model: AgentSessionModel?
    @State private var inputText = ""
    @State private var showDiffSheet = false
    @State private var showKillConfirm = false
    /// Whether the feed is scrolled to (within slack of) its bottom —
    /// auto-scroll only while pinned; scrolling up pauses follow and surfaces
    /// the "Jump to bottom" pill.
    @State private var atBottom = true
    /// EXP-356: the selected conversation tab — nil is the main agent; a
    /// subagent id focuses that agent's stream. Falls back to Main whenever
    /// the id vanishes from the feed (an `activity_reset` replay).
    @State private var agentTab: String?
    /// EXP-511: images picked for the next steer message, shown as a strip
    /// above the input row until they are sent or removed.
    @State private var pendingImages: [PendingSteerImage] = []
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showPhotoPicker = false
    @FocusState private var inputFocused: Bool

    private static let bottomAnchor = "feed-bottom"
    private static let feedCoordSpace = "feed-scroll"
    /// Within this many points of the bottom still counts as pinned.
    private static let followSlack: CGFloat = 32
    /// The server's `maxImageUploadBytes` (10 MB) — checked here so an oversize
    /// pick fails instantly instead of after a long upload.
    private static let maxImageBytes = 10 * 1024 * 1024

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
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 6) {
                    StatusDot(
                        phase: model?.phase ?? .connecting,
                        awaiting: model?.awaitingInput ?? false
                    )
                    Text(headerTitle)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                }
            }
            // Kill switch (EXP-268): force-end a live session — owner-only,
            // like everything about a live session (EXP-312).
            ToolbarItem(placement: .topBarTrailing) {
                if model?.canKill == true {
                    Button {
                        showKillConfirm = true
                    } label: {
                        AppIcon(AppIcons.codingStop, size: AppIcon.Size.large)
                            .foregroundStyle(DesignTokens.Semantic.red)
                    }
                    .accessibilityLabel("Kill session")
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
        .onChange(of: scenePhase) { _, newPhase in
            // Returning from the background (EXP-243): the socket rarely
            // survives suspension — reconnect automatically instead of
            // parking behind a manual button.
            if newPhase == .active {
                model?.reconnectNow()
            }
        }
        .onAppear {
            if let model {
                // Popped back to (something was pushed on top, whose
                // onDisappear shut the socket down) — revive.
                model.resume()
            } else {
                let m = AgentSessionModel(
                    accountId: accountId,
                    session: session,
                    currentUserId: deps.auth.userId,
                    steerApi: deps.steerApi,
                    issueImagesApi: deps.issueImagesApi,
                    db: deps.db
                )
                model = m
                m.start()
            }
        }
        .onDisappear {
            // Close the socket when dismissed.
            model?.shutdown()
        }
        .sheet(isPresented: $showDiffSheet) {
            if let diff = model?.latestDiff {
                LatestChangesSheet(diff: diff)
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

    private var headerTitle: String {
        let label = (model?.session ?? session).deviceLabel
        let device = (label?.isEmpty == false) ? " · \(label!)" : ""
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
        if model.feed.isEmpty,
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
            AgentFeed.subagents(model.feed), selected: agentTab
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
                            AgentFeed.subagents(model.feed).first { $0.subagentId == id }
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
                    .frame(minHeight: geo.size.height, alignment: .bottom)
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

    /// A lone question card: a plan approval, a single-question ask, or a
    /// legacy (pre-EXP-249) card with no wire id.
    @ViewBuilder
    private func questionCard(_ question: AgentQuestion) -> some View {
        QuestionCard(
            question: question,
            active: model?.activeQuestionIds.contains(question.id) ?? false,
            canAnswer: canAnswer,
            locked: model?.isAnswerLocked(question.lockKey) ?? false,
            pending: model?.isAnswerPending(question.lockKey) ?? false,
            failed: model?.isAnswerFailed(question.lockKey) ?? false,
            onAnswer: { keys, text in sendAnswer(question, keys: keys, text: text) },
            onLegacyToggle: { key in model?.sendLegacyKey(key) },
            onLegacyAnswer: { key in model?.sendLegacyKey(key, lockKey: question.lockKey) },
            onLegacySubmit: { model?.sendLegacyAdvance(lockKey: question.lockKey) },
            markdownContext: markdownContext
        )
        .id(question.id)
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
                active: stepIndex != nil && (model?.activeQuestionIds.contains(question.id) ?? false),
                canAnswer: canAnswer,
                locked: model?.isAnswerLocked(question.lockKey) ?? false,
                pending: model?.isAnswerPending(question.lockKey) ?? false,
                failed: model?.isAnswerFailed(question.lockKey) ?? false,
                onAnswer: { keys, text in sendAnswer(question, keys: keys, text: text) },
                onLegacyToggle: { _ in },
                onLegacyAnswer: { _ in },
                onLegacySubmit: {},
                markdownContext: markdownContext
            )
            // A fresh identity per step — the card's local selection state must
            // never leak from one question into the next.
            .id(question.id)
        }
    }

    private func stepLabel(for question: AgentQuestion, in group: AgentAskGroup) -> String? {
        guard let index = question.index else { return nil }
        let total = question.total ?? group.stepCount
        return total > 1 ? "Question \(index) of \(total)" : nil
    }

    private func sendAnswer(_ question: AgentQuestion, keys: [String], text: String? = nil) {
        guard let wireId = question.wireId else { return }
        model?.sendAnswer(questionId: wireId, askId: question.askId, keys: keys, text: text)
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

    private func bannerRow(@ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 8) {
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
    }

    // MARK: - Bottom bar (pinned diff chip + steering input)

    @ViewBuilder
    private func bottomBar(_ model: AgentSessionModel) -> some View {
        // EXP-312: live implies ownership — the ticket mint refuses others.
        let inputVisible = model.phase == .live && !model.sessionEnded
        if model.latestDiff != nil || inputVisible {
            VStack(alignment: .leading, spacing: 8) {
                if let diff = model.latestDiff {
                    diffChip(diff)
                }
                if inputVisible {
                    if !pendingImages.isEmpty {
                        pendingImageStrip
                    }
                    if let imageError = model.steerImageError {
                        Text(imageError)
                            .font(.caption)
                            .foregroundStyle(DesignTokens.Semantic.red)
                    }
                    // Steering is fully seamless (EXP-312) — no captions, no
                    // operator state; input just sends.
                    inputRow(model)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(.ultraThinMaterial)
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

    private var trimmedInput: String {
        inputText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// The images queued for the next steer message (EXP-511) — 64pt tiles with
    /// a remove overlay, mirroring the share composer's strip.
    private var pendingImageStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(pendingImages) { image in
                    if let uiImage = UIImage(data: image.data) {
                        ZStack(alignment: .topTrailing) {
                            Image(uiImage: uiImage)
                                .resizable()
                                .scaledToFill()
                                .frame(width: 64, height: 64)
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                            Button {
                                pendingImages.removeAll { $0.id == image.id }
                            } label: {
                                AppIcon(AppIcons.uiClose, size: 10, weight: .semibold)
                                    .foregroundStyle(.white)
                                    .frame(width: 20, height: 20)
                                    .background(Circle().fill(Color.black.opacity(0.6)))
                            }
                            .buttonStyle(.plain)
                            .padding(2)
                            .accessibilityLabel("Remove image")
                        }
                    }
                }
            }
            .padding(.vertical, 2)
        }
    }

    private func inputRow(_ model: AgentSessionModel) -> some View {
        // Images attach to the session's ISSUE, so a batch or action run has
        // nowhere to put them (EXP-511).
        let canAttach = model.session?.issueId != nil
        let attachFull = pendingImages.count >= SteerImageMessage.maxImages
        let canSend = !trimmedInput.isEmpty || !pendingImages.isEmpty
        let sendDisabled = !canSend || model.steerSending
        return HStack(alignment: .bottom, spacing: 8) {
            if canAttach {
                Button {
                    showPhotoPicker = true
                } label: {
                    AppIcon(AppIcons.uiAdd, size: 15, weight: .semibold)
                        .foregroundStyle(.white)
                        .padding(9)
                }
                .glassButton()
                .buttonStyle(.plain)
                .disabled(attachFull || model.steerSending)
                .opacity(attachFull || model.steerSending ? 0.5 : 1)
                .accessibilityLabel("Attach image")
            }
            TextField("Message the agent…", text: $inputText, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...4)
                .font(.subheadline)
                .foregroundStyle(.white)
                .focused($inputFocused)
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
                .background(Color.white.opacity(0.06))
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
                )
            Button {
                sendMessage(model)
            } label: {
                AppIcon(AppIcons.uiSend, size: 15, weight: .semibold)
                    .foregroundStyle(.white)
                    .padding(9)
            }
            .glassButton(isActive: !sendDisabled)
            .buttonStyle(.plain)
            .disabled(sendDisabled)
            .opacity(sendDisabled ? 0.5 : 1)
            .accessibilityLabel("Send")
        }
    }

    private func sendMessage(_ model: AgentSessionModel) {
        guard !pendingImages.isEmpty else {
            guard !trimmedInput.isEmpty else { return }
            model.sendMessage(inputText)
            inputText = ""
            return
        }
        guard !model.steerSending else { return }
        Task { await sendWithImages(model) }
    }

    /// Upload the pending images and send ONE composed message (EXP-511). A
    /// failure keeps the draft and the strip — the model hands back the images
    /// with their already-uploaded ids so a retry only uploads the rest.
    private func sendWithImages(_ model: AgentSessionModel) async {
        if let remaining = await model.sendSteerImages(inputText, images: pendingImages) {
            pendingImages = remaining
        } else {
            inputText = ""
            pendingImages = []
        }
    }

    /// Turn a photo pick into pending images: transcode anything the server's
    /// inline-image pipeline doesn't accept (notably HEIC) to JPEG, exactly as
    /// the share extension does, and cap the strip at four.
    private func ingestPhotos(_ items: [PhotosPickerItem]) async {
        defer { photoItems = [] }
        model?.steerImageError = nil
        for item in items {
            guard pendingImages.count < SteerImageMessage.maxImages else { break }
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let type = item.supportedContentTypes.first
            let contentType = AttachmentFiles.canonicalContentType(
                type?.preferredMIMEType ?? "image/jpeg"
            )
            let normalized: (data: Data, filename: String, contentType: String)
            if AttachmentFiles.isInlineImage(contentType: contentType) {
                let ext = type?.preferredFilenameExtension ?? "jpg"
                normalized = (data, "image-\(Int(Date().timeIntervalSince1970)).\(ext)", contentType)
            } else if let image = UIImage(data: data),
                      let jpeg = image.jpegData(compressionQuality: 0.9) {
                normalized = (jpeg, "image-\(Int(Date().timeIntervalSince1970)).jpg", "image/jpeg")
            } else {
                model?.steerImageError = "That image type isn't supported."
                continue
            }
            guard normalized.data.count <= Self.maxImageBytes else {
                model?.steerImageError = "That image is too large."
                continue
            }
            pendingImages.append(PendingSteerImage(
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
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
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
            guard !reduceMotion else { return }
            withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true)) {
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
    /// Legacy multi-select: the raw digit that TOGGLES an option.
    let onLegacyToggle: (String) -> Void
    /// Legacy single-select: the raw digit that answers (and locks).
    let onLegacyAnswer: (String) -> Void
    /// Legacy multi-select: submit the picker (Tab).
    let onLegacySubmit: () -> Void
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

    private var submitTitle: String { question.isSemantic ? "Submit" : "Continue" }

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
    /// already chosen.
    @ViewBuilder
    private var priorStepSummary: some View {
        if !priorSteps.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(priorSteps) { step in
                    HStack(alignment: .top, spacing: 6) {
                        AppIcon(AppIcons.uiCheck, size: 11)
                            .foregroundStyle(DesignTokens.Semantic.green)
                            .padding(.top, 2)
                        Text(step.answerSummary ?? "Answered")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
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
            Text(question.answerSummary ?? (question.dismissed ? "Dismissed" : "Answered"))
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
                        optionLabel(option, showKey: showsKeyBadge(option))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
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
                    optionLabel(option, showKey: showsKeyBadge(option))
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
            // Semantic multi-select submits every picked key at once; a legacy
            // picker already toggled each digit and only needs the Tab.
            let disabled = locked || (question.isSemantic && picked.isEmpty)
            Button(submitTitle) {
                submit()
            }
            .font(.caption.weight(.medium))
            .foregroundStyle(.white)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .glassButton(isActive: !disabled)
            .buttonStyle(.plain)
            .disabled(disabled)
            .opacity(disabled ? 0.5 : 1)
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
            // A legacy picker toggles on the raw digit; v2 batches the keys
            // into the submit frame.
            if !question.isSemantic { onLegacyToggle(option.key) }
            return
        }
        if option.freeText, question.isSemantic {
            // EXP-513: collect the reply first — nothing is sent until it
            // submits (the desktop types it into the TUI row).
            freeTextKey = freeTextKey == option.key ? nil : option.key
            freeTextFocused = freeTextKey != nil
            return
        }
        picked = [option.key]
        if question.isSemantic {
            onAnswer([option.key], nil)
        } else {
            onLegacyAnswer(option.key)
        }
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
        guard !locked else { return }
        if question.isSemantic {
            guard !picked.isEmpty else { return }
            onAnswer(picked, nil)
        } else {
            onLegacySubmit()
        }
    }

    private func optionLabel(
        _ option: AgentQuestionOption,
        showKey: Bool = true
    ) -> some View {
        HStack(alignment: .top, spacing: 6) {
            if showKey {
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

/// What an agent-markdown render needs to fetch the images it embeds: the
/// instance base URL the relative `/api/attachments/{id}` paths resolve
/// against, plus the credentials the attachment loader authenticates with
/// (EXP-440). Built once per screen, exactly like `CommentThreadView` does it.
private struct AgentMarkdownContext {
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
}

/// Read-only GFM render of agent-authored prose (plan bodies, question
/// prompts, narration, steered messages) through the SAME block stack as
/// comment bodies (EXP-249) — claude writes markdown, and a plan flattened
/// into one Text was unreadable.
private struct AgentMarkdownText: View {
    let text: String
    /// nil renders text-only: image blocks have nothing to fetch with.
    var context: AgentMarkdownContext? = nil
    /// Display-only parse deviations — safe here, nothing serializes back.
    var options: MarkdownParseOptions = []
    var imageMaxHeight: CGFloat? = 280
    /// Chat bubbles hug their text; full-width prose does not.
    var hugsWidth = false

    @State private var displayModel = IssueEditorModel()
    @State private var displayedText: String?

    var body: some View {
        MarkdownEditor(
            model: displayModel,
            placeholder: "",
            baseURL: context?.baseURL,
            accountId: context?.accountId ?? "",
            httpClient: context?.httpClient,
            isReadOnly: true,
            imageMaxHeight: imageMaxHeight,
            hugsContentWidth: hugsWidth
        )
        .frame(maxWidth: hugsWidth ? nil : .infinity, alignment: .leading)
        .task(id: text) {
            guard displayedText != text else { return }
            displayedText = text
            let model = IssueEditorModel()
            model.load(markdown: text, baseURL: context?.baseURL, options: options)
            displayModel = model
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
                Text("Approve it on the desktop.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Status dot

/// Header status dot: green live / pulsing yellow while connecting or the
/// agent is starting / gray when ended or lost. Static under Reduce Motion.
private struct StatusDot: View {
    let phase: AgentSessionModel.Phase
    /// Live but blocked on a trailing question/plan — waiting for a human
    /// answer, not stuck (EXP-97).
    var awaiting: Bool = false

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulsing = false

    private var connecting: Bool {
        // Auto-reconnecting after a drop reads as connecting (EXP-243).
        if case .closed(_, true) = phase { return true }
        return phase == .connecting || phase == .starting || phase == .idle
    }

    private var color: Color {
        switch phase {
        case .live: awaiting ? DesignTokens.Semantic.yellow : DesignTokens.Semantic.green
        case .connecting, .starting, .idle, .closed(_, true): DesignTokens.Semantic.yellow
        default: DesignTokens.Semantic.neutral
        }
    }

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 8, height: 8)
            .opacity(pulsing ? 0.35 : 1)
            // Value-bound so the repeat dies with the flag — an open-ended
            // withAnimation(.repeatForever) here kept driving the render loop
            // after the phase moved on (EXP-70).
            .animation(
                pulsing ? .easeInOut(duration: 0.65).repeatForever(autoreverses: true) : nil,
                value: pulsing
            )
            .onAppear { pulsing = connecting && !reduceMotion }
            .onChange(of: connecting) { _, now in pulsing = now && !reduceMotion }
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

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content
                .onScrollPhaseChange { _, newPhase in
                    userScrolling = newPhase == .tracking
                        || newPhase == .interacting
                        || newPhase == .decelerating
                }
                .onScrollGeometryChange(for: FeedPinMetrics.self) { geometry in
                    // Pinned ⇔ within slack of the MAXIMUM scrollable offset.
                    // The max is clamped to the minimum resting offset
                    // (-contentInsets.top): a feed shorter than the viewport
                    // rests there, and the unclamped bottom formula reported
                    // it as "not at bottom" — sticking the "Jump to bottom"
                    // pill on screen with nothing to scroll (EXP-242).
                    let minOffset = -geometry.contentInsets.top
                    let maxOffset = max(
                        geometry.contentSize.height + geometry.contentInsets.bottom
                            - geometry.containerSize.height,
                        minOffset
                    )
                    return FeedPinMetrics(
                        pinned: geometry.contentOffset.y >= maxOffset - slack,
                        offset: geometry.contentOffset.y
                    )
                } action: { old, new in
                    // Reaching the bottom always re-arms follow; LEAVING it is
                    // the user's alone (Android parity: only drags flip
                    // `follow`) — content growth un-pins the geometry without
                    // any gesture, and the growth observer below re-pins
                    // instead (EXP-272). "User's alone" additionally requires
                    // the offset to have moved UP: content growing under a
                    // finger resting at the bottom un-pins the geometry while
                    // a scroll phase is active, and treating that as a scroll-
                    // away stranded the pill on screen at the bottom — the
                    // growth observer skips repins during user scrolls, and
                    // once un-pinned nothing ever recovered (EXP-306).
                    if new.pinned {
                        if !atBottom { atBottom = true }
                    } else if userScrolling, atBottom, new.offset < old.offset {
                        atBottom = false
                    }
                }
                .onScrollGeometryChange(for: CGFloat.self) { geometry in
                    geometry.contentSize.height
                } action: { oldHeight, newHeight in
                    if newHeight > oldHeight, atBottom, !userScrolling {
                        repin()
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
/// plus the raw offset, so the action can tell a user scroll AWAY from the
/// bottom (offset moved up) apart from content growth un-pinning the geometry
/// under a stationary finger (EXP-306).
private struct FeedPinMetrics: Equatable {
    var pinned: Bool
    var offset: CGFloat
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
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text("Latest changes")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                Spacer()
                Text("+\(stats.additions)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.green)
                Text("−\(stats.deletions)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.red)
            }
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
                .padding(.bottom, 24)
            }
        }
        .padding(16)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .presentationBackground(.ultraThinMaterial)
    }
}
