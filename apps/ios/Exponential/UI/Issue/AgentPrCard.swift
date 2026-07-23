import ExpUI
import ExpCore
import SwiftUI

/// The compact coding/PR status card on issue detail (EXP-156). EXP-240 moved
/// the remote-start affordance into the bottom bar's Start-coding circle, so
/// this card is now a pure status glance with up to three coexisting rows:
///   - Session: a running coding session → "Coding now" + tap-to-watch (members
///              when the relay is on; an inert note when steering is disabled).
///   - PR:      a linked PR → GitHub-style capsule chip (pull icon tinted by
///              state + "PR #n"), tapping opens the diff page.
///   - Branch:  a pushed branch, no PR yet → branch icon + mono name chip,
///              same diff page.
/// No inline Close/Merge/GitHub-link/diff-count here — the review actions live
/// on the diff page (ChangesView).
struct AgentPrCard: View {
    let issue: IssueEntity
    let runningSessions: [CodingSessionEntity]
    let permissions: TeamPermissions
    let users: [UserEntity]
    /// Relay config, loaded by the view model's refreshSteer (EXP-240) —
    /// gates tap-to-watch on the session row.
    let config: SteerConfig?

    @Environment(\.accountId) private var accountId

    /// Multi-window desktops can run several sessions on one issue — surface the
    /// most recent (any presence at all counts as "coding now").
    private var session: CodingSessionEntity? {
        runningSessions.max { $0.startedAt < $1.startedAt }
    }

    private var showsCard: Bool {
        session != nil
            || issue.prUrl != nil
            || (issue.branch?.isEmpty == false)
    }

    var body: some View {
        Group {
            if showsCard {
                content
            }
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let session {
                sessionRow(session)
            }
            if issue.prUrl != nil {
                prChip
            } else if let branch = issue.branch, !branch.isEmpty {
                branchChip(branch)
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
                NavigationLink(value: AppRoute.agentSession(
                    accountId: accountId, sessionId: session.id
                )) {
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
        // The parked states render a static dot/label instead of the pulsing
        // green "Coding now": review green, done blue (once the PR merges),
        // needs-input amber while the agent waits on a plan-approval /
        // question picker (EXP-194/EXP-214).
        let state = CodingSessionDisplayState.of(session: session, prState: issue.prState)
        let tint: Color = switch state {
        case .needsInput: DesignTokens.Semantic.yellow
        case .review: DesignTokens.Semantic.green
        case .done: DesignTokens.Semantic.blue
        case .running: DesignTokens.Semantic.green
        }
        let label = switch state {
        case .needsInput: "Needs input"
        case .review: "Ready for review"
        case .done: "Done"
        case .running: "Coding now"
        }
        return HStack(spacing: 8) {
            if state != .running {
                Circle()
                    .fill(tint)
                    .frame(width: 9, height: 9)
            } else {
                PulsingLiveDot()
            }
            Text(label)
                .font(.caption.weight(.semibold))
                .foregroundStyle(tint)
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

    // MARK: - PR / branch chips (GitHub-style, EXP-240)

    /// Pull-request icon tint per PR state — GitHub's palette: open green,
    /// merged indigo (purple), closed red.
    private var prTint: Color {
        switch issue.prState {
        case DomainContract.prStateMerged: Accent.indigo
        case DomainContract.prStateClosed: DesignTokens.Semantic.red
        default: DesignTokens.Semantic.green
        }
    }

    private var prLabel: String {
        if let number = issue.prNumber {
            return "PR #\(number)"
        }
        return "Pull request"
    }

    private var prChip: some View {
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: issue.id)) {
            HStack(spacing: 6) {
                Image(systemName: "arrow.triangle.pull")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(prTint)
                Text(prLabel)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                if let prState = issue.prState, !prState.isEmpty {
                    Text(prState.capitalized)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .glassButton()
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
    }

    private func branchChip(_ branch: String) -> some View {
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: issue.id)) {
            HStack(spacing: 6) {
                Image(systemName: "arrow.triangle.branch")
                    .font(.caption)
                    .foregroundStyle(Accent.indigo)
                Text(branch)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .glassButton()
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
    }
}

/// The live-session pulse: a solid green core with an expanding, fading ring —
/// the "Coding now" green, animated. Static under Reduce Motion. Shared by the
/// issue-detail card, the bottom bar's start circle, and the Agents tab.
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
