import ExpCore
import ExpUI
import SwiftUI

/// The "Agent session" screen (EXP-32) — a chat-style view of a live coding
/// session over the relay's scrubbed activity channel. NO terminal rendering:
/// narration bubbles + compact tool rows, a pinned "Latest changes" diff chip
/// above the input bar, and message-shaped steering (steal-claim + text + \r).
/// Identical UX to the Android AgentSessionScreen (glass design system).
struct AgentSessionView: View {
    let accountId: String
    let session: CodingSessionEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var model: AgentSessionModel?
    @State private var inputText = ""
    @State private var showDiffSheet = false
    /// Whether the feed's bottom sentinel is on screen — auto-scroll only
    /// while pinned; scrolling up pauses follow.
    @State private var atBottom = true
    @FocusState private var inputFocused: Bool

    private static let bottomAnchor = "feed-bottom"

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                header
                if let model {
                    feedArea(model)
                    banners(model)
                    bottomBar(model)
                } else {
                    Spacer()
                }
            }
        }
        .onAppear {
            if model == nil {
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

    private var header: some View {
        HStack(spacing: 8) {
            HStack(spacing: 6) {
                StatusDot(phase: model?.phase ?? .connecting)
                Text(headerTitle)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            Spacer()
            if case .closed = model?.phase {
                Button {
                    model?.connect()
                } label: {
                    Label("Reconnect", systemImage: "arrow.clockwise")
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                }
                .glassButton()
                .buttonStyle(.plain)
                .foregroundStyle(.white)
            }
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(8)
            }
            .glassButton()
            .buttonStyle(.plain)
            .accessibilityLabel("Close")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.ultraThinMaterial)
    }

    private var headerTitle: String {
        let label = (model?.session ?? session).deviceLabel
        let device = (label?.isEmpty == false) ? " · \(label!)" : ""
        switch model?.phase {
        case .live: return "Live\(device)"
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
    private func feedList(_ model: AgentSessionModel) -> some View {
        GeometryReader { geo in
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(model.feed) { item in
                            feedRow(item)
                        }
                        // Follow sentinel: visible ⇔ pinned to the bottom.
                        Color.clear
                            .frame(height: 1)
                            .id(Self.bottomAnchor)
                            .onAppear { atBottom = true }
                            .onDisappear { atBottom = false }
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(minHeight: geo.size.height, alignment: .bottom)
                }
                .defaultScrollAnchor(.bottom)
                .onChange(of: model.feed.count) { _, _ in
                    if atBottom {
                        proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                    }
                }
                .overlay(alignment: .bottom) {
                    if !atBottom {
                        Button {
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
                        .glassButton(isActive: true)
                        .buttonStyle(.plain)
                        .padding(.bottom, 8)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func feedRow(_ item: AgentSessionModel.FeedItem) -> some View {
        switch item {
        case let .narration(_, text):
            NarrationBubble(text: text)
        case let .tool(_, name, detail):
            ToolRow(name: name, detail: detail)
        }
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
                    if model.isSteering {
                        steerCaption("You're steering")
                    } else if let name = model.remoteSteererName {
                        // Input stays enabled — sending steals the claim.
                        steerCaption("\(name) is steering")
                    }
                    inputRow(model)
                } else if watching {
                    steerCaption("Watching — only workspace owners or the session owner can steer.")
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

// MARK: - Status dot

/// Header status dot: green live / pulsing yellow while connecting or the
/// agent is starting / gray when ended or lost. Static under Reduce Motion.
private struct StatusDot: View {
    let phase: AgentSessionModel.Phase

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulsing = false

    private var connecting: Bool {
        phase == .connecting || phase == .starting || phase == .idle
    }

    private var color: Color {
        switch phase {
        case .live: DesignTokens.Semantic.green
        case .connecting, .starting, .idle: DesignTokens.Semantic.yellow
        default: DesignTokens.Semantic.neutral
        }
    }

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 8, height: 8)
            .opacity(connecting && pulsing ? 0.35 : 1)
            .onAppear { startPulseIfNeeded() }
            .onChange(of: connecting) { _, _ in startPulseIfNeeded() }
    }

    private func startPulseIfNeeded() {
        guard connecting, !reduceMotion else {
            pulsing = false
            return
        }
        pulsing = false
        withAnimation(.easeInOut(duration: 0.65).repeatForever(autoreverses: true)) {
            pulsing = true
        }
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
