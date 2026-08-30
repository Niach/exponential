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

    var body: some View {
        HStack(spacing: 6) {
            if paused || state != .running || !live {
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
