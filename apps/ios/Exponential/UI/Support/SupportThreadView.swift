import ExpCore
import ExpUI
import SwiftUI

/// One support ticket (EXP-180): the member-side conversation with an external
/// reporter. Message bubbles (inbound leading/neutral, outbound
/// trailing/accent, internal notes amber + "Internal" tag), a bottom composer
/// with a Reply / Internal-note toggle, and toolbar actions for
/// close/reopen/escalate. Polled at 15s while on screen.
struct SupportThreadView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    let threadId: String

    @State private var viewModel: SupportThreadViewModel?
    @State private var composerText = ""
    @State private var internalNote = false
    /// EXP-603: the board choice used to be a nested `Menu`; a glass menu has
    /// no submenus, so escalation picks its board in a sheet.
    @State private var escalateOpen = false
    /// EXP-687: the `…` popup is an in-view overlay on this screen's root —
    /// a presentation launched from inside the UIKit bar item dropped taps.
    @State private var menuAnchor: CGRect = .zero
    @State private var menuOpen = false
    @FocusState private var composerFocused: Bool

    var body: some View {
        ZStack {
            AppBackground()
            Group {
                if let vm = viewModel {
                    content(vm)
                } else {
                    Color.clear
                }
            }
        }
        .glassMenuOverlay(isPresented: $menuOpen, anchor: menuAnchor, presentation: .inline) {
            if let vm = viewModel, vm.thread != nil {
                toolbarMenuItems(vm)
            }
        }
        .navigationTitle(viewModel?.thread?.title ?? "Support")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            if let vm = viewModel, vm.thread != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    GlassMenuBarButton(
                        icon: AppIcons.uiMore,
                        accessibilityLabel: "More",
                        anchor: $menuAnchor,
                        isPresented: $menuOpen
                    )
                }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let vm = viewModel {
                composer(vm)
            }
        }
        .sheet(isPresented: $escalateOpen) {
            if let vm = viewModel {
                GlassPickerSheet(
                    title: "Escalate to issue",
                    items: vm.boards,
                    selectedID: nil as String?,
                    idFor: { $0.id },
                    onSelect: { board in
                        Task { await vm.escalate(boardId: board.id) }
                    }
                ) { board in
                    Label {
                        Text(board.name)
                    } icon: {
                        // Board glyph tinted with the board color — the move-to-board
                        // picker's idiom (EXP-449).
                        AppIcon(BoardTypeDisplay.iconName(for: board), size: 16)
                            .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
                    }
                }
            }
        }
        .onAppear {
            if viewModel == nil {
                viewModel = SupportThreadViewModel(
                    accountId: accountId,
                    threadId: threadId,
                    helpdeskApi: deps.helpdeskApi,
                    db: deps.db
                )
            }
            // Re-arm on every appear (pushing the linked issue stops the poll).
            viewModel?.startPolling()
        }
        .onDisappear { viewModel?.stopPolling() }
    }

    // MARK: - Content

    @ViewBuilder
    private func content(_ vm: SupportThreadViewModel) -> some View {
        if vm.thread == nil, vm.isLoading {
            ProgressView()
        } else if vm.thread == nil, let error = vm.error {
            VStack(spacing: 8) {
                Text("Couldn't load this ticket")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)
            }
            .padding(.horizontal, 24)
        } else if let thread = vm.thread {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 10) {
                        header(vm, thread: thread)
                        ForEach(vm.messages) { message in
                            messageBubble(message)
                                .id(message.id)
                        }
                    }
                    .padding()
                }
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: vm.messages.count) { _, _ in
                    if let last = vm.messages.last {
                        withAnimation { proxy.scrollTo(last.id, anchor: .bottom) }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func header(_ vm: SupportThreadViewModel, thread: SupportThreadInfo) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                AppIcon(vm.isOpen ? AppIcons.supportOpen : AppIcons.supportResolved, size: AppIcon.Size.small)
                Text(vm.isOpen ? "Open" : "Resolved")
                    .font(.caption.weight(.medium))
                Text(reporterLabel(thread))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                Spacer()
            }
            .foregroundStyle(vm.isOpen ? Color.white : .white.opacity(TextOpacity.secondary))

            if let issue = vm.linkedIssue {
                NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                    GlassPill(issue.title) {
                        AppIcon(AppIcons.uiExternalLink, size: GlassPillTokens.glyphSm)
                        if let identifier = issue.identifier {
                            Text(identifier)
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.bottom, 2)
    }

    // MARK: - Bubbles

    @ViewBuilder
    private func messageBubble(_ message: SupportMessage) -> some View {
        HStack {
            if !message.isInbound { Spacer(minLength: 40) }
            VStack(alignment: .leading, spacing: 4) {
                if message.isInternal {
                    HStack(spacing: 4) {
                        AppIcon(AppIcons.uiPrivate, size: 11)
                        Text("Internal")
                            .font(.caption2.weight(.semibold))
                    }
                    .foregroundStyle(.orange)
                }
                Text(message.body)
                    .font(.subheadline)
                    .foregroundStyle(bubbleForeground(message))
                    .fixedSize(horizontal: false, vertical: true)
                Text(relativeDate(message.createdAt))
                    .font(.caption2)
                    .foregroundStyle(bubbleForeground(message).opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(bubbleBackground(message), in: RoundedRectangle(cornerRadius: 14))
            if message.isInbound { Spacer(minLength: 40) }
        }
        .frame(maxWidth: .infinity, alignment: message.isInbound ? .leading : .trailing)
    }

    private func bubbleBackground(_ message: SupportMessage) -> Color {
        if message.isInternal { return Color.orange.opacity(0.18) }
        if message.isInbound { return Color.white.opacity(0.08) }
        return DesignTokens.Palette.primary
    }

    /// Body/meta text color per bubble — the solid near-white outbound fill
    /// (web `bg-primary text-primary-foreground`, EXP-594) needs dark text.
    private func bubbleForeground(_ message: SupportMessage) -> Color {
        if message.isInternal || message.isInbound { return .white }
        return DesignTokens.Palette.primaryForeground
    }

    // MARK: - Composer

    /// EXP-698: the ONE composer card, with the Reply / Internal note choice
    /// as two `.select` pills in its leading slot. No material, no shadow, no
    /// second radius — `isOpaque` because it floats over the message list.
    @ViewBuilder
    private func composer(_ vm: SupportThreadViewModel) -> some View {
        GlassComposer(isOpaque: true) {
            VStack(alignment: .leading, spacing: 6) {
                if let error = vm.error, vm.thread != nil {
                    Text(error)
                        .font(.caption2)
                        .foregroundStyle(.orange)
                        .lineLimit(2)
                }

                HStack(spacing: 4) {
                    composerModePill(label: "Reply", isInternal: false)
                    composerModePill(label: "Internal note", isInternal: true)
                    Spacer(minLength: 0)
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 8)
        } field: {
            GlassTextField(
                internalNote ? "Write an internal note…" : "Reply to the reporter…",
                text: $composerText,
                lines: 1...5,
                bordered: false
            )
            .font(.subheadline)
            .focused($composerFocused)
            .padding(.horizontal, 12)
            .padding(.bottom, 4)
        } submit: {
            GlassComposerSubmitButton(
                AppIcons.uiSend,
                accessibilityLabel: internalNote ? "Send internal note" : "Send reply",
                enabled: !sendDisabled(vm),
                tint: internalNote ? Color.orange : Color.white
            ) {
                Task {
                    if await vm.send(body: composerText, internalNote: internalNote) {
                        composerText = ""
                    }
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 8)
        .padding(.bottom, 10)
    }

    private func composerModePill(label: String, isInternal: Bool) -> some View {
        GlassPill(
            label,
            mode: .select(isSelected: internalNote == isInternal) { internalNote = isInternal }
        )
        .accessibilityLabel(label)
    }

    private func sendDisabled(_ vm: SupportThreadViewModel) -> Bool {
        vm.sending
            || composerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    // MARK: - Toolbar

    @ViewBuilder
    private func toolbarMenuItems(_ vm: SupportThreadViewModel) -> some View {
        if vm.linkedIssue == nil, !vm.boards.isEmpty {
            GlassMenuItem("Escalate to issue", icon: AppIcons.uiExternalLink) {
                escalateOpen = true
            }
        }
        if vm.isOpen {
            GlassMenuItem("Close ticket", icon: AppIcons.supportResolved) {
                Task { await vm.close() }
            }
        } else {
            GlassMenuItem("Reopen ticket", icon: AppIcons.supportOpen) {
                Task { await vm.reopen() }
            }
        }
    }

    // MARK: - Helpers

    private func reporterLabel(_ thread: SupportThreadInfo) -> String {
        let name = thread.reporterName?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let name, !name.isEmpty { return name }
        return thread.reporterEmail
    }

    private func relativeDate(_ s: String) -> String {
        guard let date = WireTimestamps.parse(s) else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}
