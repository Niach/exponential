import SwiftUI

/// Live view of the agent-core's runs, fed by the core's `run_started` /
/// `run_finished` / `run_cancelled` host events. The single source of truth for
/// "is a run in flight for this issue?" — drives the panel's Cancel button and
/// the dock indicator without guessing from plan states.
@MainActor
@Observable
final class MacAgentRunMonitor {
    private(set) var runningIssueIds: Set<String> = []
    private var runIdsByIssue: [String: String] = [:]

    func isRunning(issueId: String) -> Bool { runningIssueIds.contains(issueId) }

    func runStarted(issueId: String, runId: String) {
        guard !issueId.isEmpty else { return }
        runningIssueIds.insert(issueId)
        runIdsByIssue[issueId] = runId
    }

    func runEnded(issueId: String) {
        runningIssueIds.remove(issueId)
        runIdsByIssue[issueId] = nil
    }

    /// All runs vanish with their cores (used on shutdown/unregister).
    func reset() {
        runningIssueIds.removeAll()
        runIdsByIssue.removeAll()
    }
}

/// App-wide ephemeral toasts (run started/finished/cancelled, agent errors).
/// Rendered by `MacToastOverlay` at the bottom of `MacShell`.
@MainActor
@Observable
final class MacToastCenter {
    struct Toast: Identifiable, Equatable {
        enum Style { case info, success, error }
        let id = UUID()
        let message: String
        let style: Style
    }

    private(set) var toasts: [Toast] = []

    func show(_ message: String, style: Toast.Style = .info, duration: Duration = .seconds(4)) {
        let toast = Toast(message: message, style: style)
        toasts.append(toast)
        // Keep the stack shallow — old toasts age out fast anyway.
        if toasts.count > 3 { toasts.removeFirst(toasts.count - 3) }
        Task { [weak self] in
            try? await Task.sleep(for: duration)
            self?.dismiss(toast.id)
        }
    }

    func dismiss(_ id: UUID) {
        toasts.removeAll { $0.id == id }
    }
}

struct MacToastOverlay: View {
    let center: MacToastCenter

    var body: some View {
        VStack(spacing: 6) {
            ForEach(center.toasts) { toast in
                HStack(spacing: 8) {
                    Image(systemName: icon(toast.style))
                        .foregroundStyle(color(toast.style))
                    Text(toast.message)
                        .font(.callout)
                        .lineLimit(3)
                    Button { center.dismiss(toast.id) } label: {
                        Image(systemName: "xmark").font(.caption2)
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
                .overlay(
                    RoundedRectangle(cornerRadius: 10)
                        .strokeBorder(Color.white.opacity(0.08))
                )
                .shadow(color: .black.opacity(0.3), radius: 12, y: 4)
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .padding(.bottom, 16)
        .frame(maxWidth: 480)
        .animation(.spring(duration: 0.25), value: center.toasts)
    }

    private func icon(_ style: MacToastCenter.Toast.Style) -> String {
        switch style {
        case .info: "sparkles"
        case .success: "checkmark.circle.fill"
        case .error: "exclamationmark.triangle.fill"
        }
    }

    private func color(_ style: MacToastCenter.Toast.Style) -> Color {
        switch style {
        case .info: .blue
        case .success: .green
        case .error: .red
        }
    }
}
