import ExpCore
import ExpUI
import SwiftUI

/// EXP-688: the first line of a coding-session row — state dot, mono issue
/// identifier, issue title.
///
/// ONE view, two call sites: the Agents list row and the steering screen's
/// nav-bar title, which used to say `Live · macbook` and never named the issue
/// it was steering. Extracted so the two cannot drift (Android's
/// `SessionRowTitle.kt` is the twin).
struct SessionRowTitle: View {
    /// Nil for a batch or action run — those have no issue to name.
    let identifier: String?
    let title: String
    let state: CodingSessionDisplayState
    /// EXP-550: the host machine stopped heartbeating — the run is parked, so
    /// the dot goes static neutral instead of pulsing "coding now". The
    /// steering header widens this to every terminal socket state (closed /
    /// ended), which is just as much "not coding now".
    let paused: Bool
    /// Whether the run is CONNECTED and coding right now. The list reads the
    /// synced row alone, so its rows are live by definition; the steering
    /// header knows better — it holds the live phase, and a connecting or
    /// disconnected screen must not pulse a green "coding now" dot over a
    /// caption that says "Connecting…" or "Session ended".
    var live: Bool = true
    /// EXP-848: the agent is inside a TURN right now — the synced
    /// `coding_sessions.agent_busy` in a list, the screen's own working
    /// predicate while steering. Without it every live row pulsed "coding now"
    /// whether the agent was doing anything or not.
    let busy: Bool

    var body: some View {
        HStack(spacing: 6) {
            if !CodingSessionDisplayState.pulses(
                state: state, agentBusy: busy, paused: paused, live: live
            ) {
                Circle()
                    .fill(paused ? DesignTokens.Semantic.neutral : sessionStateColor(state))
                    .frame(width: 9, height: 9)
            } else {
                PulsingLiveDot()
            }
            if let identifier, !identifier.isEmpty {
                Text(identifier)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }
}

/// EXP-804: the run's usage wall — `Rate limited · resets in 2h`.
///
/// A quiet amber pill that renders BESIDE the state badge, never instead of
/// it: the wall is ORTHOGONAL to the session state, so a walled run still
/// reads `running`. Without this the two are indistinguishable — the run just
/// goes silent (the 2026-09-09 incident).
///
/// The label itself is `AgentUsagePresentation.blockedBadgeLabel`, locked ×4;
/// nothing here decides wording. Nil label = not blocked = no pill.
struct SessionBlockedBadge: View {
    /// The raw `coding_sessions.blocked` jsonb text off the wire.
    let blocked: String?
    /// Pinned by the tests; production passes the wall clock so the countdown
    /// re-reads whenever the row redraws.
    var now: Date = Date()

    private var label: String? {
        AgentUsagePresentation.blockedBadgeLabel(
            AgentUsagePresentation.parseBlocked(blocked),
            now: now
        )
    }

    var body: some View {
        if let label {
            HStack(spacing: 4) {
                AppIcon(AppIcons.uiClock, size: 10)
                Text(label)
                    .font(.caption2.weight(.semibold))
                    .lineLimit(1)
            }
            .foregroundStyle(DesignTokens.Semantic.yellow)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(DesignTokens.Semantic.yellow.opacity(0.12), in: Capsule())
            .accessibilityElement(children: .combine)
            .accessibilityLabel(label)
        }
    }
}

/// Static-dot/label tint per parked display state (EXP-194/EXP-214):
/// review green, done blue (the issue-status palette), needs-input amber.
func sessionStateColor(_ state: CodingSessionDisplayState) -> Color {
    switch state {
    case .needsInput: DesignTokens.Semantic.yellow
    case .review: DesignTokens.Semantic.green
    case .done: DesignTokens.Semantic.blue
    case .running: DesignTokens.Semantic.green
    }
}

/// The word beside the dot on a parked row; nil while the run is simply going.
func sessionStateLabel(_ state: CodingSessionDisplayState) -> String? {
    switch state {
    case .needsInput: "Needs input"
    case .review: "Ready for review"
    case .done: "Done"
    case .running: nil
    }
}

/// The title beside the identifier, one rule for the list and the steering
/// header: an issueless run is an action run when it carries its `action_name`
/// snapshot (EXP-253), else a batch run — never "Untitled issue". A
/// single-issue session whose issue row simply hasn't synced yet (or arrived
/// blank) reads "Untitled issue".
func sessionRowTitle(issue: IssueEntity?, session: CodingSessionEntity) -> String {
    if issue == nil, session.issueId == nil {
        return session.actionName ?? "Batch run"
    }
    let title = issue?.title ?? ""
    return title.isEmpty ? "Untitled issue" : title
}
