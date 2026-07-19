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
        .navigationTitle(viewModel?.thread?.title ?? "Support")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            if let vm = viewModel, vm.thread != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    toolbarMenu(vm)
                }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let vm = viewModel {
                composer(vm)
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
                Image(systemName: vm.isOpen ? "envelope.open" : "checkmark.circle")
                    .font(.caption)
                Text(vm.isOpen ? "Open" : "Resolved")
                    .font(.caption.weight(.medium))
                Text(reporterLabel(thread))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                Spacer()
            }
            .foregroundStyle(vm.isOpen ? Accent.indigo : .white.opacity(TextOpacity.secondary))

            if let issue = vm.linkedIssue {
                NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                    HStack(spacing: 6) {
                        Image(systemName: "arrow.up.forward.square")
                            .font(.caption)
                        if let identifier = issue.identifier {
                            Text(identifier)
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        Text(issue.title)
                            .font(.caption)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(Color.white.opacity(0.08), in: Capsule())
                    .overlay(
                        Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                    )
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
                        Image(systemName: "lock")
                            .font(.caption2)
                        Text("Internal")
                            .font(.caption2.weight(.semibold))
                    }
                    .foregroundStyle(.orange)
                }
                Text(message.body)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)
                Text(relativeDate(message.createdAt))
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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
        return Accent.indigo.opacity(0.35)
    }

    // MARK: - Composer

    @ViewBuilder
    private func composer(_ vm: SupportThreadViewModel) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let error = vm.error, vm.thread != nil {
                Text(error)
                    .font(.caption2)
                    .foregroundStyle(.orange)
                    .lineLimit(2)
            }

            HStack(spacing: 4) {
                composerModeButton(label: "Reply", isInternal: false)
                composerModeButton(label: "Internal note", isInternal: true)
                Spacer()
            }

            HStack(alignment: .bottom, spacing: 8) {
                TextField(
                    internalNote ? "Write an internal note…" : "Reply to the reporter…",
                    text: $composerText,
                    axis: .vertical
                )
                .lineLimit(1...5)
                .font(.subheadline)
                .foregroundStyle(.white)
                .focused($composerFocused)

                Button {
                    Task {
                        if await vm.send(body: composerText, internalNote: internalNote) {
                            composerText = ""
                        }
                    }
                } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 26))
                        .foregroundStyle(
                            sendDisabled(vm)
                                ? Color.white.opacity(0.3)
                                : (internalNote ? Color.orange : Accent.indigo)
                        )
                }
                .buttonStyle(.plain)
                .disabled(sendDisabled(vm))
                .accessibilityLabel(internalNote ? "Send internal note" : "Send reply")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Color.white.opacity(0.06))
            .clipShape(RoundedRectangle(cornerRadius: 18))
        }
        .padding(.horizontal, 16)
        .padding(.top, 8)
        .padding(.bottom, 10)
        .background(.ultraThinMaterial)
    }

    private func composerModeButton(label: String, isInternal: Bool) -> some View {
        let active = internalNote == isInternal
        return Button {
            internalNote = isInternal
        } label: {
            Text(label)
                .font(.caption.weight(active ? .semibold : .regular))
                .foregroundStyle(
                    active
                        ? (isInternal ? Color.orange : .white)
                        : .white.opacity(TextOpacity.secondary)
                )
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(
                    active ? Color.white.opacity(0.12) : .clear,
                    in: Capsule()
                )
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    private func sendDisabled(_ vm: SupportThreadViewModel) -> Bool {
        vm.sending
            || composerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    // MARK: - Toolbar

    @ViewBuilder
    private func toolbarMenu(_ vm: SupportThreadViewModel) -> some View {
        Menu {
            if vm.linkedIssue == nil, !vm.boards.isEmpty {
                Menu {
                    ForEach(vm.boards) { board in
                        Button(board.name) {
                            Task { await vm.escalate(boardId: board.id) }
                        }
                    }
                } label: {
                    Label("Escalate to issue", systemImage: "arrow.up.forward.square")
                }
            }
            if vm.isOpen {
                Button {
                    Task { await vm.close() }
                } label: {
                    Label("Close ticket", systemImage: "checkmark.circle")
                }
            } else {
                Button {
                    Task { await vm.reopen() }
                } label: {
                    Label("Reopen ticket", systemImage: "envelope.open")
                }
            }
        } label: {
            Image(systemName: "ellipsis.circle")
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
