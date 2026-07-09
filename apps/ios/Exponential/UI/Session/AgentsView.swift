import ExpUI
import ExpCore
import SwiftUI

/// The Agents tab: currently running coding sessions for the active account.
/// Rows open the live agent session view directly when the relay is configured
/// (the same viewer `SteerSessionSection` presents from an issue), else fall
/// back to the issue detail; the trailing info affordance always goes to the
/// issue detail.
struct AgentsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: AgentsViewModel?
    @State private var steerEnabled = false
    @State private var watchingSession: CodingSessionEntity?

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                if vm.rows.isEmpty {
                    emptyState
                } else {
                    sessionList(vm)
                }
            }
        }
        .navigationTitle("Agents")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .onAppear {
            if viewModel == nil {
                viewModel = AgentsViewModel(accountId: accountId, db: deps.db)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        .fullScreenCover(item: $watchingSession) { session in
            AgentSessionView(accountId: accountId, session: session)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Image("tab-robot")
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: 28, height: 28)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No agents running")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Start coding on an issue from the desktop IDE — live sessions show up here.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
    }

    @ViewBuilder
    private func sessionList(_ vm: AgentsViewModel) -> some View {
        ScrollView {
            LazyVStack(spacing: 8) {
                ForEach(vm.rows) { row in
                    sessionRow(row)
                }
            }
            .padding()
        }
        .safeAreaInset(edge: .bottom) {
            Color.clear.frame(height: 16)
        }
    }

    // The primary tap target and the trailing info affordance are siblings
    // (not nested controls) so both hit areas stay reliable.
    @ViewBuilder
    private func sessionRow(_ row: AgentsViewModel.Row) -> some View {
        HStack(spacing: 12) {
            // With the relay configured, the row jumps straight into the live
            // agent session; otherwise it opens the issue detail, where the
            // session section shows whatever is available.
            Group {
                if steerEnabled {
                    Button {
                        watchingSession = row.session
                    } label: {
                        sessionRowContent(row)
                    }
                    .buttonStyle(.plain)
                } else if let issue = row.issue {
                    NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                        sessionRowContent(row)
                    }
                    .buttonStyle(.plain)
                } else {
                    sessionRowContent(row)
                }
            }

            if let issue = row.issue {
                NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                    Image(systemName: "info.circle")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Open issue")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
    }

    @ViewBuilder
    private func sessionRowContent(_ row: AgentsViewModel.Row) -> some View {
        HStack(spacing: 12) {
            PulsingLiveDot()

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    if let identifier = row.issue?.identifier {
                        Text(identifier)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    Text(row.issue?.title ?? "Untitled issue")
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                }
                Text(byline(row.session))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }

            Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
    }

    private func byline(_ session: CodingSessionEntity) -> String {
        let device: String
        if let label = session.deviceLabel, !label.isEmpty {
            device = label
        } else {
            device = "Desktop"
        }
        let started = relativeDate(session.startedAt)
        return started.isEmpty ? device : "\(device) · started \(started)"
    }

    private func relativeDate(_ s: String) -> String {
        let isoFormatter = ISO8601DateFormatter()
        isoFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let date = isoFormatter.date(from: s) ?? ISO8601DateFormatter().date(from: s)
        guard let date else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

/// The live-session pulse: a solid green core with an expanding, fading ring —
/// the "Coding now" green, animated. Static under Reduce Motion.
private struct PulsingLiveDot: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulsing = false

    var body: some View {
        Circle()
            .fill(DesignTokens.Semantic.green)
            .frame(width: 9, height: 9)
            .overlay(
                Circle()
                    .stroke(DesignTokens.Semantic.green.opacity(0.6), lineWidth: 2)
                    .scaleEffect(pulsing ? 2.2 : 1.0)
                    .opacity(pulsing ? 0 : 0.8)
            )
            .onAppear {
                guard !reduceMotion else { return }
                withAnimation(.easeOut(duration: 1.4).repeatForever(autoreverses: false)) {
                    pulsing = true
                }
            }
    }
}
