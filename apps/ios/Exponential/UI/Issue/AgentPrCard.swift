import ExpUI
import ExpCore
import SwiftUI

/// The single compact coding/PR card on issue detail (EXP-156) — replaces the
/// former SteerSessionSection + ChangesSection. One glass section holding up to
/// four independent, coexisting rows:
///   - Session: a running coding session → "Coding now" + tap-to-watch (members
///              when the relay is on; an inert note when steering is disabled).
///   - Start:   no session, a member, relay on → remote "Start coding" (device
///              label when exactly one desktop is online), or a "no desktop
///              online" hint. Dispatches 1 issue → single session, 2+ → batch.
///   - PR:      a linked PR → state pill + "PR #n", tapping opens the diff page.
///   - Branch:  a pushed branch, no PR yet → the branch name, same diff page.
/// No inline Close/Merge/GitHub-link/diff-count here — the review actions live
/// on the diff page (ChangesView); this card is a launcher + status glance.
struct AgentPrCard: View {
    let issue: IssueEntity
    let runningSessions: [CodingSessionEntity]
    let permissions: TeamPermissions
    let users: [UserEntity]
    /// Loads the eligible issues for the Start-coding sheet's picker (the
    /// current issue pre-checked). Injected so the card stays view-model-free.
    let loadStartCandidates: () async -> [StartCodingSheet.IssueOption]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var config: SteerConfig?
    @State private var devices: [SteerDevice]?
    @State private var starting = false
    @State private var sentToLabel: String?
    @State private var sentBatch = false
    @State private var startError: String?
    @State private var watchingSession: CodingSessionEntity?
    @State private var showStartSheet = false
    @State private var startCandidates: [StartCodingSheet.IssueOption] = []

    /// Multi-window desktops can run several sessions on one issue — surface the
    /// most recent (any presence at all counts as "coding now").
    private var session: CodingSessionEntity? {
        runningSessions.max { $0.startedAt < $1.startedAt }
    }

    /// The Start area shows a hint or a button (not the pre-load blank) only for
    /// a member on a relay-enabled instance with presence resolved.
    private var startAreaVisible: Bool {
        session == nil && permissions.isMember && config?.enabled == true && devices != nil
    }

    private var showsCard: Bool {
        session != nil
            || startAreaVisible
            || issue.prUrl != nil
            || (issue.branch?.isEmpty == false)
    }

    var body: some View {
        Group {
            if showsCard {
                content
            }
        }
        // Keyed on session presence AND membership: when a session ends the
        // start area re-appears and must (re)load presence, and on cold start
        // refreshDevices() guards on isMember — so the load must re-run once the
        // members shape syncs and isMember flips true (else devices stays nil
        // forever and neither the Start button nor the no-desktop hint appears).
        .task(id: "\(accountId)|\(issue.id)|\(session == nil)|\(permissions.isMember)") {
            config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            await refreshDevices()
        }
        .fullScreenCover(item: $watchingSession) { session in
            AgentSessionView(accountId: accountId, session: session)
        }
        .sheet(isPresented: $showStartSheet) {
            StartCodingSheet(
                devices: devices ?? [],
                issues: startCandidates,
                preselectedIds: [issue.id]
            ) { device, issueIds, options in
                start(on: device, issueIds: issueIds, options: options)
            }
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let session {
                sessionRow(session)
            }
            if startAreaVisible {
                startArea
            }
            if issue.prUrl != nil {
                prRow
            } else if let branch = issue.branch, !branch.isEmpty {
                branchRow(branch)
            }
        }
        .padding(12)
        .glassSection()
    }

    // MARK: - Session row

    @ViewBuilder
    private func sessionRow(_ session: CodingSessionEntity) -> some View {
        let canWatch = permissions.isMember && config?.enabled == true
        VStack(alignment: .leading, spacing: 6) {
            if canWatch {
                Button {
                    watchingSession = session
                } label: {
                    sessionRowContent(session, chevron: true)
                }
                .buttonStyle(.plain)
            } else {
                sessionRowContent(session, chevron: false)
            }
            // Relay explicitly off on this instance: the badge stays, steering
            // doesn't. (config?.enabled == false is only true once config loads.)
            if permissions.isMember, config?.enabled == false {
                Text("Live steering is unavailable on this instance.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
    }

    private func sessionRowContent(_ session: CodingSessionEntity, chevron: Bool) -> some View {
        let owner = users.first { $0.id == session.userId }
        return HStack(spacing: 8) {
            PulsingLiveDot()
            Text("Coding now")
                .font(.caption.weight(.semibold))
                .foregroundStyle(DesignTokens.Semantic.green)
            Text(sessionByline(owner: owner, session: session))
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
            Spacer(minLength: 0)
            if chevron {
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .contentShape(Rectangle())
    }

    private func sessionByline(owner: UserEntity?, session: CodingSessionEntity) -> String {
        let name = memberDisplayName(owner, id: session.userId)
        if let device = session.deviceLabel, !device.isEmpty {
            return "· \(name) · \(device)"
        }
        return "· \(name)"
    }

    // MARK: - Start row

    @ViewBuilder
    private var startArea: some View {
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
                VStack(alignment: .leading, spacing: 6) {
                    Button {
                        Task { await presentStartSheet() }
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
                            Text(devices.count == 1 && !devices[0].deviceLabel.isEmpty
                                ? "Start coding on \(devices[0].deviceLabel)"
                                : "Start coding")
                                .font(.caption.weight(.medium))
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
                            Text(sentBatch
                                ? "Batch start sent to \(sentToLabel) — follow it in the Agents tab."
                                : "Start sent to \(sentToLabel) — waiting for the desktop…")
                                .font(.caption2)
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }

                    if let startError {
                        Text(startError)
                            .font(.caption2)
                            .foregroundStyle(DesignTokens.Semantic.red)
                    }
                }
            }
        }
    }

    // MARK: - PR / branch rows

    private var prRow: some View {
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: issue.id)) {
            HStack(spacing: 8) {
                if let prState = issue.prState, !prState.isEmpty {
                    Text(prState.capitalized)
                        .font(.caption2.weight(.medium))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color.white.opacity(0.08))
                        .clipShape(Capsule())
                        .foregroundStyle(.white)
                }
                Text(prLabel)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var prLabel: String {
        if let number = issue.prNumber {
            return "PR #\(number)"
        }
        return "Pull request"
    }

    private func branchRow(_ branch: String) -> some View {
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: issue.id)) {
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.branch")
                    .font(.caption)
                    .foregroundStyle(Accent.indigo)
                Text(branch)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Actions

    private func refreshDevices() async {
        guard config?.enabled == true, permissions.isMember, session == nil else { return }
        devices = (try? await deps.steerApi.myDevices(accountId: accountId)) ?? []
    }

    private func presentStartSheet() async {
        startCandidates = await loadStartCandidates()
        showStartSheet = true
    }

    private func start(on device: SteerDevice, issueIds: [String], options: SteerStartOptions) {
        guard !issueIds.isEmpty else { return }
        starting = true
        startError = nil
        let isBatch = issueIds.count > 1
        Task {
            defer { starting = false }
            do {
                if isBatch {
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
                sentBatch = isBatch
                sentToLabel = device.deviceLabel
                // The desktop inserts the coding_sessions row when the launcher
                // spins up, which swaps the start area for the session row via
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

/// The live-session pulse: a solid green core with an expanding, fading ring —
/// the "Coding now" green, animated. Static under Reduce Motion. Shared by the
/// issue-detail card and the Agents tab (EXP-156 — one implementation).
struct PulsingLiveDot: View {
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
