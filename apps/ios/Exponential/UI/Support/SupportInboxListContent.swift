import ExpCore
import ExpUI
import SwiftUI

/// The Support inbox list (EXP-180): the Support tab's content. No chrome of
/// its own (the InboxListContent precedent) — SupportView owns the screen
/// chrome; this view owns its poll lifecycle (re-armed on appear, cancelled
/// on disappear, restarted on a team switch).
struct SupportInboxListContent: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    let teamId: String
    @State private var viewModel: SupportInboxViewModel?

    var body: some View {
        VStack(spacing: 0) {
            filterPills
                .padding(.horizontal, 16)
                .padding(.bottom, 8)

            if let vm = viewModel {
                content(vm)
            } else {
                Color.clear
            }
        }
        .onAppear {
            if viewModel == nil {
                viewModel = SupportInboxViewModel(helpdeskApi: deps.helpdeskApi)
            }
            // Re-arm on every appear: pushing a thread stops the poll
            // (onDisappear), popping back must resume it.
            viewModel?.startPolling(accountId: accountId, teamId: teamId)
        }
        .onDisappear { viewModel?.stopPolling() }
        .onChange(of: teamId) { _, newTeamId in
            viewModel?.startPolling(accountId: accountId, teamId: newTeamId)
        }
    }

    // Open/Resolved pills — the same glass-pill language as the MyWork
    // segment control, sized to content.
    private var filterPills: some View {
        HStack(spacing: 4) {
            ForEach(SupportThreadFilter.allCases, id: \.rawValue) { f in
                filterButton(f)
            }
            Spacer()
        }
    }

    private func filterButton(_ f: SupportThreadFilter) -> some View {
        let active = (viewModel?.filter ?? .open) == f
        return Button {
            viewModel?.setFilter(f)
        } label: {
            Text(label(for: f))
                .font(.caption.weight(active ? .semibold : .regular))
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(
                    active ? Color.white.opacity(0.12) : Color.white.opacity(0.04),
                    in: Capsule()
                )
                .overlay(
                    Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                )
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label(for: f))
    }

    private func label(for f: SupportThreadFilter) -> String {
        switch f {
        case .open: return "Open"
        case .resolved: return "Resolved"
        }
    }

    @ViewBuilder
    private func content(_ vm: SupportInboxViewModel) -> some View {
        if vm.threads.isEmpty {
            if vm.isLoading {
                VStack {
                    Spacer()
                    ProgressView()
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else if let error = vm.error {
                emptyState(error)
            } else {
                emptyState(vm.filter == .open ? "No open tickets" : "No resolved tickets")
            }
        } else {
            ScrollView {
                LazyVStack(spacing: 8) {
                    ForEach(vm.threads) { thread in
                        NavigationLink(
                            value: AppRoute.supportThread(accountId: accountId, threadId: thread.id)
                        ) {
                            threadRow(thread)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding()
            }
            // Clearance for the floating tab bar (EXP-36).
            .tabBarBottomInset()
        }
    }

    private func threadRow(_ thread: SupportThreadRow) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 28, height: 28)
                .background(Color.white.opacity(0.08), in: Circle())

            VStack(alignment: .leading, spacing: 3) {
                Text(thread.title)
                    .font(.subheadline.weight(thread.unread ? .semibold : .regular))
                    .foregroundStyle(.white)
                    .lineLimit(1)

                HStack(spacing: 6) {
                    Text(reporterLabel(thread))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                    Text(relativeDate(thread.lastMessage?.createdAt ?? thread.updatedAt))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }

                if let last = thread.lastMessage {
                    Text(last.body)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 8)

            if thread.unread {
                Circle()
                    .fill(Accent.indigo)
                    .frame(width: 8, height: 8)
                    .padding(.top, 6)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
        .opacity(thread.unread || thread.status == "open" ? 1 : 0.6)
    }

    private func reporterLabel(_ thread: SupportThreadRow) -> String {
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

    private func emptyState(_ label: String) -> some View {
        VStack {
            Spacer()
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}
