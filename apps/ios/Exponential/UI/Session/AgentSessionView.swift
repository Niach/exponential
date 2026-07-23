import ExpCore
import ExpUI
import GRDB
import SwiftUI

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
/// narration bubbles + compact tool rows, a pinned "Latest changes" diff chip
/// above the input bar, and message-shaped steering (steal-claim + text + \r).
/// Identical UX to the Android AgentSessionScreen (glass design system).
/// Pushed onto the NavigationStack (EXP-221) — status lives in the native
/// nav bar; back is the system chevron + swipe gesture.
struct AgentSessionView: View {
    let accountId: String
    let session: CodingSessionEntity

    @Environment(AppDependencies.self) private var deps
    @State private var model: AgentSessionModel?
    @State private var inputText = ""
    @State private var showDiffSheet = false
    /// Whether the feed is scrolled to (within slack of) its bottom —
    /// auto-scroll only while pinned; scrolling up pauses follow and surfaces
    /// the "Jump to latest" pill.
    @State private var atBottom = true
    @FocusState private var inputFocused: Bool

    private static let bottomAnchor = "feed-bottom"
    private static let feedCoordSpace = "feed-scroll"
    /// Within this many points of the bottom still counts as pinned.
    private static let followSlack: CGFloat = 32

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
            if case .closed = model?.phase {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model?.connect()
                    } label: {
                        Label("Reconnect", systemImage: "arrow.clockwise")
                            .font(.caption.weight(.medium))
                    }
                    .foregroundStyle(.white)
                }
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
                    db: deps.db
                )
                model = m
                m.start()
            }
        }
        .onDisappear {
            // Auto-release the steer claim + close the socket when dismissed.
            model?.shutdown()
        }
        .sheet(isPresented: $showDiffSheet) {
            if let diff = model?.latestDiff {
                LatestChangesSheet(diff: diff)
            }
        }
    }

    // MARK: - Header

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
        case .closed: return "Disconnected"
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
                    ? "The agent is starting — waiting for the live stream…"
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
            feedList(model)
        }
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
    /// user scrolls up, then a "Jump to latest ↓" pill re-pins.
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
                        // Consecutive tool calls collapse into "N tool calls"
                        // rows (EXP-97) — a render projection; the flat feed
                        // (and the trailing-question rule) stays the state.
                        let rows = model.rows
                        ForEach(rows) { row in
                            feedRow(row, isLast: row.id == rows.last?.id)
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
                .modifier(FollowPinTracker(atBottom: $atBottom, slack: Self.followSlack))
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
                .overlay(alignment: .bottom) {
                    if !atBottom {
                        Button {
                            // The scroll re-pins: geometry flips atBottom once
                            // the animation lands at the bottom.
                            withAnimation {
                                proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                            }
                        } label: {
                            Text("Jump to latest ↓")
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

    @ViewBuilder
    private func feedRow(_ row: AgentSessionModel.FeedRow, isLast: Bool) -> some View {
        switch row {
        case let .toolRun(items):
            ToolGroupRow(items: items, liveTail: isLast && model?.phase == .live)
        case let .single(item):
            switch item {
            case let .narration(_, text):
                NarrationBubble(text: text)
            case let .tool(_, name, detail):
                ToolRow(name: name, detail: detail)
            case let .userMessage(_, text):
                UserMessageBubble(text: text)
            case let .question(id, text, options, multiSelect, planMode, _, answer):
                QuestionCard(
                    text: text,
                    options: options,
                    multiSelect: multiSelect,
                    planMode: planMode,
                    answer: answer,
                    active: model?.activeQuestionIds.contains(id) ?? false,
                    canAnswer: canAnswer,
                    onAnswer: { key, submit in model?.sendAnswer(key, submit: submit) },
                    onSubmit: { model?.sendSubmit() }
                )
            }
        }
    }

    /// Whether this client may answer questions at all — a question card is
    /// answerable when this holds AND it is still active per
    /// `activeQuestionIds` (EXP-78/EXP-174).
    private var canAnswer: Bool {
        guard let model else { return false }
        return model.canSteer && model.phase == .live && !model.sessionEnded
    }

    // MARK: - Status banners (feed retained above)

    @ViewBuilder
    private func banners(_ model: AgentSessionModel) -> some View {
        switch model.phase {
        case let .ended(detail):
            bannerRow {
                Text(detail ?? "Session ended")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case let .closed(detail):
            bannerRow {
                Text(detail ?? "Disconnected")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case .starting where !model.feed.isEmpty:
            bannerRow {
                ProgressView().controlSize(.small).tint(.white)
                Text("The agent is starting — waiting for the live stream…")
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
        let inputVisible = model.canSteer && model.phase == .live && !model.sessionEnded
        let watching = !inputVisible && model.perm == "view" && model.phase == .live
        if model.latestDiff != nil || inputVisible || watching {
            VStack(alignment: .leading, spacing: 8) {
                if let diff = model.latestDiff {
                    diffChip(diff)
                }
                if inputVisible {
                    // No own-steering notice — steering should feel seamless
                    // (EXP-197); only another person's claim is worth a
                    // caption. Input stays enabled — sending steals the claim.
                    if let name = model.remoteSteererName {
                        steerCaption("\(name) is steering")
                    }
                    inputRow(model)
                } else if watching {
                    steerCaption("Watching — only team owners or the session owner can steer.")
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
                Image(systemName: "plus.forwardslash.minus")
                    .font(.caption)
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
                Image(systemName: "chevron.up")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .glassRow()
    }

    private func steerCaption(_ text: String) -> some View {
        Text(text)
            .font(.caption2)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }

    private var trimmedInput: String {
        inputText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func inputRow(_ model: AgentSessionModel) -> some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField("Message the agent…", text: $inputText, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...4)
                .font(.subheadline)
                .foregroundStyle(.white)
                .focused($inputFocused)
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
                // Subtle active tint while we hold the steer claim.
                .background(Color.white.opacity(model.isSteering ? 0.10 : 0.06))
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .stroke(Color.white.opacity(model.isSteering ? 0.2 : 0.1), lineWidth: 0.5)
                )
            Button {
                sendMessage(model)
            } label: {
                Image(systemName: "arrow.up")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(9)
            }
            .glassButton(isActive: !trimmedInput.isEmpty)
            .buttonStyle(.plain)
            .disabled(trimmedInput.isEmpty)
            .opacity(trimmedInput.isEmpty ? 0.5 : 1)
            .accessibilityLabel("Send")
        }
    }

    private func sendMessage(_ model: AgentSessionModel) {
        guard !trimmedInput.isEmpty else { return }
        model.sendMessage(inputText)
        inputText = ""
    }
}

// MARK: - Feed rows

/// Assistant prose — a chat bubble with a small glyph, selectable text.
private struct NarrationBubble: View {
    let text: String

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "sparkles")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.top, 4)
            Text(text)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .glassSection()
        }
        .padding(.vertical, 5)
    }
}

/// A human turn (EXP-78): the initial prompt or a steered message — rendered
/// trailing-aligned like the sender's own chat bubble, long text folded.
private struct UserMessageBubble: View {
    let text: String

    @State private var expanded = false

    /// Fold threshold — the initial prompt can be 16 KiB.
    private static let clampLines = 6
    private static let clampChars = 600

    private var clampable: Bool {
        text.count > Self.clampChars
            || text.filter { $0 == "\n" }.count >= Self.clampLines
    }

    var body: some View {
        HStack {
            Spacer(minLength: 32)
            VStack(alignment: .leading, spacing: 4) {
                Text(text)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .textSelection(.enabled)
                    .lineLimit(clampable && !expanded ? Self.clampLines : nil)
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

/// An interactive question (EXP-78): AskUserQuestion / plan approval. Option
/// buttons send the option's raw TUI keystroke while the question is still
/// active (per `activeQuestionIds` — trailing run, or an unresolved plan card,
/// EXP-174); stale/view-only cards render options as plain rows.
/// `planMode` cards (EXP-97) get a dedicated "Plan ready" presentation with
/// the first option as the primary approve action — labels/keys always come
/// from the wire options, the desktop owns the TUI key mapping. Best-effort
/// by design — the desktop TUI remains the source of truth.
private struct QuestionCard: View {
    let text: String
    let options: [AgentSessionModel.QuestionOption]
    let multiSelect: Bool
    let planMode: Bool
    /// The chosen answer once the question resolved (EXP-197) — replaces the
    /// option rows.
    let answer: String?
    /// Still answerable per the feed — the session is blocked on this card.
    let active: Bool
    /// Live + steer perm — whether this client may answer at all.
    let canAnswer: Bool
    /// (key, submit) — single-select taps submit (digit + Enter); multi-select
    /// taps toggle with the digit alone and `onSubmit` advances (Tab).
    let onAnswer: (String, Bool) -> Void
    let onSubmit: () -> Void

    @State private var expanded = false
    @State private var picked: Set<String> = []

    private static let clampLines = 6
    private static let clampChars = 600

    /// Plans are always fully rendered — never folded (EXP-197).
    private var clampable: Bool {
        !planMode
            && (text.count > Self.clampChars
                || text.filter { $0 == "\n" }.count >= Self.clampLines)
    }

    private var answerable: Bool { active && canAnswer }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: planMode ? "checklist" : "questionmark.circle")
                .font(.caption)
                .foregroundStyle(planMode ? DesignTokens.Semantic.blue : DesignTokens.Semantic.yellow)
                .padding(.top, 4)
            VStack(alignment: .leading, spacing: 8) {
                if planMode {
                    Text("Plan ready")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(DesignTokens.Semantic.blue)
                }
                Text(text)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .textSelection(.enabled)
                    .lineLimit(clampable && !expanded ? Self.clampLines : nil)
                if clampable {
                    Button(expanded ? "Show less" : "Show more") {
                        expanded.toggle()
                    }
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .buttonStyle(.plain)
                }
                if let answer {
                    // Answered (EXP-197): the chosen answer replaces the options.
                    HStack(alignment: .top, spacing: 6) {
                        Image(systemName: "checkmark")
                            .font(.caption2)
                            .foregroundStyle(DesignTokens.Semantic.green)
                            .padding(.top, 2)
                        Text(answer)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.white)
                            .textSelection(.enabled)
                    }
                } else {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(Array(options.enumerated()), id: \.element.key) { index, option in
                            if answerable {
                                // The wire's first option of a plan is the primary
                                // approve action ("Approve — auto-accept edits").
                                let primary = planMode && index == 0
                                Button {
                                    pick(option)
                                } label: {
                                    optionLabel(option, showKey: !planMode)
                                        .padding(.horizontal, 10)
                                        .padding(.vertical, 6)
                                }
                                .glassButton(isActive: primary || picked.contains(option.key))
                                .buttonStyle(.plain)
                            } else {
                                optionLabel(option)
                            }
                        }
                    }
                }
                if answerable, multiSelect {
                    // Advances to the picker's next tab / review step (Tab on
                    // the wire — see AgentSessionModel.sendSubmit).
                    Button("Continue") {
                        onSubmit()
                    }
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .glassButton(isActive: true)
                    .buttonStyle(.plain)
                }
                if active, !canAnswer {
                    Text(planMode
                        ? "Waiting for approval — you're viewing read-only."
                        : "Waiting for an answer — you're viewing read-only.")
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

    private func pick(_ option: AgentSessionModel.QuestionOption) {
        onAnswer(option.key, !multiSelect)
        if multiSelect {
            if picked.contains(option.key) {
                picked.remove(option.key)
            } else {
                picked.insert(option.key)
            }
        } else {
            picked = [option.key]
        }
    }

    private func optionLabel(
        _ option: AgentSessionModel.QuestionOption,
        showKey: Bool = true
    ) -> some View {
        HStack(spacing: 6) {
            if showKey {
                Text(option.key)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Text(option.label)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .multilineTextAlignment(.leading)
        }
    }
}

/// Tool-call headline — compact single line, consecutive rows visually tight.
private struct ToolRow: View {
    let name: String
    let detail: String?

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "wrench.and.screwdriver")
                .font(.caption2)
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
    let items: [AgentSessionModel.FeedItem]
    let liveTail: Bool

    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                expanded.toggle()
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: expanded ? "chevron.down" : "chevron.right")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Image(systemName: "wrench.and.screwdriver")
                        .font(.caption2)
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
                        if case let .tool(_, name, detail) = item {
                            ToolRow(name: name, detail: detail)
                        }
                    }
                }
                .padding(.leading, 20)
            } else if liveTail, let last = items.last,
                      case let .tool(_, name, detail) = last {
                ToolRow(name: name, detail: detail)
                    .padding(.leading, 20)
            }
        }
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
        phase == .connecting || phase == .starting || phase == .idle
    }

    private var color: Color {
        switch phase {
        case .live: awaiting ? DesignTokens.Semantic.yellow : DesignTokens.Semantic.green
        case .connecting, .starting, .idle: DesignTokens.Semantic.yellow
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
/// stuck true so the "Jump to latest" pill never appeared and follow-scroll
/// couldn't be escaped (EXP-212). Pre-18 keeps the preference path.
private struct FollowPinTracker: ViewModifier {
    @Binding var atBottom: Bool
    /// Within this many points of the bottom still counts as pinned.
    let slack: CGFloat

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content.onScrollGeometryChange(for: Bool.self) { geometry in
                // Pinned ⇔ within slack of the MAXIMUM scrollable offset.
                // The max is clamped to the minimum resting offset
                // (-contentInsets.top): a feed shorter than the viewport
                // rests there, and the unclamped bottom formula reported it
                // as "not at bottom" — sticking the "Jump to latest" pill
                // on screen with nothing to scroll (EXP-242).
                let minOffset = -geometry.contentInsets.top
                let maxOffset = max(
                    geometry.contentSize.height + geometry.contentInsets.bottom
                        - geometry.containerSize.height,
                    minOffset
                )
                return geometry.contentOffset.y >= maxOffset - slack
            } action: { _, pinned in
                if atBottom != pinned {
                    atBottom = pinned
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
