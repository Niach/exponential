import ExpUI
import ExpCore
import SwiftUI

// Issue-detail coding session section (masterplan §5b/§5c) — iOS mirror of the
// web's <SteerTerminal> root. Driven by the synced coding_sessions rows:
//   running session → "Coding now" badge (+ Watch live when the relay is on)
//   no session      → "Start on my desktop" remote start (device picker when
//                     several; hint when none online; hidden when relay is off)
// The relay/server enforce membership; the UI additionally hides interactive
// parts from non-members via WorkspacePermissions.

/// steer.config is env-derived and static per instance — fetch once per
/// account and cache for the app's lifetime (mirrors the web's fetch-once).
@MainActor
enum SteerConfigCache {
    private static var cache: [String: SteerConfig] = [:]

    static func load(accountId: String, api: SteerApi) async -> SteerConfig {
        if let cached = cache[accountId] { return cached }
        let config = (try? await api.config(accountId: accountId))
            ?? SteerConfig(enabled: false, relayUrl: nil)
        cache[accountId] = config
        return config
    }
}

struct SteerSessionSection: View {
    let issue: IssueEntity
    let runningSessions: [CodingSessionEntity]
    let permissions: WorkspacePermissions
    let users: [UserEntity]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var config: SteerConfig?
    @State private var devices: [SteerDevice]?
    @State private var starting = false
    @State private var sentToLabel: String?
    @State private var startError: String?
    @State private var watchingSession: CodingSessionEntity?
    @State private var showDevicePicker = false

    /// Multi-window desktops can run several sessions on one issue — surface
    /// the most recent (the badge counts them all).
    private var session: CodingSessionEntity? {
        runningSessions.max { $0.startedAt < $1.startedAt }
    }

    var body: some View {
        Group {
            if let session {
                liveSection(session)
            } else if permissions.isMember, config?.enabled == true {
                startSection
            }
        }
        // Keyed on session presence too: when a session ends, the start section
        // re-appears and must (re)load device presence.
        .task(id: "\(accountId)|\(issue.id)|\(session == nil)") {
            config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            await refreshDevices()
        }
        .fullScreenCover(item: $watchingSession) { session in
            AgentSessionView(accountId: accountId, session: session)
        }
    }

    // MARK: - Running session

    @ViewBuilder
    private func liveSection(_ session: CodingSessionEntity) -> some View {
        let owner = users.first { $0.id == session.userId }
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                HStack(spacing: 6) {
                    Circle()
                        .fill(DesignTokens.Semantic.green)
                        .frame(width: 8, height: 8)
                    Text(runningSessions.count > 1
                        ? "Coding now (\(runningSessions.count))"
                        : "Coding now")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(DesignTokens.Semantic.green)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .glassButton()

                Text(sessionByline(owner: owner, session: session))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)

                Spacer()
            }

            if permissions.isMember {
                if config?.enabled == true {
                    Button {
                        watchingSession = session
                    } label: {
                        Label("Watch live", systemImage: "play.display")
                            .font(.caption.weight(.medium))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                    }
                    .glassButton()
                    .buttonStyle(.plain)
                    .foregroundStyle(.white)
                } else if config != nil {
                    Text("Live steering is unavailable on this instance.")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
        .padding(12)
        .glassSection()
    }

    private func sessionByline(owner: UserEntity?, session: CodingSessionEntity) -> String {
        let name = memberDisplayName(owner, id: session.userId)
        if let device = session.deviceLabel, !device.isEmpty {
            return "\(name) · \(device)"
        }
        return name
    }

    // MARK: - Remote start

    @ViewBuilder
    private var startSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let devices {
                if devices.isEmpty {
                    HStack(spacing: 6) {
                        Image(systemName: "display.trianglebadge.exclamationmark")
                            .font(.caption)
                        Text("No desktop online — open the Exponential desktop app to run here.")
                            .font(.caption2)
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                } else {
                    HStack(spacing: 8) {
                        Button {
                            if devices.count == 1 {
                                start(on: devices[0])
                            } else {
                                showDevicePicker = true
                            }
                        } label: {
                            HStack(spacing: 6) {
                                if starting {
                                    ProgressView()
                                        .controlSize(.mini)
                                        .tint(.white)
                                } else {
                                    Image(systemName: "play.display")
                                        .font(.caption)
                                }
                                Text(devices.count == 1
                                    ? "Start coding on \(devices[0].deviceLabel)"
                                    : "Start on my desktop")
                                    .font(.caption.weight(.medium))
                                if devices.count > 1 {
                                    Image(systemName: "chevron.down")
                                        .font(.caption2)
                                }
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                        }
                        .glassButton()
                        .buttonStyle(.plain)
                        .foregroundStyle(.white)
                        .disabled(starting || sentToLabel != nil)
                        .opacity(starting || sentToLabel != nil ? 0.6 : 1)

                        if let sentToLabel {
                            HStack(spacing: 5) {
                                ProgressView()
                                    .controlSize(.mini)
                                    .tint(.white)
                                Text("Start sent to \(sentToLabel) — waiting…")
                                    .font(.caption2)
                            }
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        Spacer()
                    }
                }
            } else {
                // Presence lookup in flight — keep the section quiet.
                EmptyView()
            }

            if let startError {
                Text(startError)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        .confirmationDialog("Start on my desktop", isPresented: $showDevicePicker, titleVisibility: .visible) {
            ForEach(devices ?? []) { device in
                Button(device.deviceLabel) { start(on: device) }
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private func refreshDevices() async {
        guard config?.enabled == true, permissions.isMember, session == nil else { return }
        devices = (try? await deps.steerApi.myDevices(accountId: accountId)) ?? []
    }

    private func start(on device: SteerDevice) {
        starting = true
        startError = nil
        Task {
            defer { starting = false }
            do {
                try await deps.steerApi.startSession(
                    accountId: accountId, issueId: issue.id, deviceId: device.deviceId
                )
                sentToLabel = device.deviceLabel
                // The desktop inserts the coding_sessions row when the launcher
                // spins up, which swaps this section for the live one via the
                // sync. Re-enable after a grace window in case it never does.
                Task {
                    try? await Task.sleep(for: .seconds(30))
                    sentToLabel = nil
                }
            } catch {
                startError = error.localizedDescription
            }
        }
    }
}
