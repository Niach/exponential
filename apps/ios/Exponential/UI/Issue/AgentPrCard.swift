import ExpUI
import ExpCore
import SwiftUI

/// The issue's pull requests (EXP-156; EXP-1295: `pull_request_issues` →
/// `pull_requests`, newest first, never the issue's own columns). One
/// full-width row per PR (EXP-246/327, Linear parity): the pull icon tinted
/// by state, `PR #n`, its title, the state word; a tap opens it on GitHub.
/// No inline Close/Merge here — merging is the thread's Guide.
struct AgentPrCard: View {
    let pullRequests: [PullRequestEntity]

    @Environment(\.openURL) private var openURL

    var body: some View {
        if !pullRequests.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(pullRequests) { pr in
                    row(pr)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// Pull-request icon tint per PR state: open green, merged blue (the
    /// done-status semantic, web/desktop parity), closed red.
    private func tint(_ state: String) -> Color {
        switch state {
        case DomainContract.prStateMerged: DesignTokens.Semantic.blue
        case DomainContract.prStateClosed: DesignTokens.Semantic.red
        default: DesignTokens.Semantic.green
        }
    }

    private func row(_ pr: PullRequestEntity) -> some View {
        Button {
            if let url = URL(string: pr.url) { openURL(url) }
        } label: {
            HStack(spacing: 8) {
                AppIcon(AppIcons.prOpen, size: AppIcon.Size.small, weight: .semibold)
                    .foregroundStyle(tint(pr.state))
                Text("PR #\(pr.number)")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .fixedSize()
                if let title = pr.title, !title.isEmpty {
                    Text(title)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                }
                Spacer(minLength: 8)
                Text(pr.state.capitalized)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                AppIcon(AppIcons.uiExternalLink, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("issue-pr-row")
    }
}

/// The live-session pulse: a solid green core with an expanding, fading ring —
/// the "Coding now" green, animated. Static under Reduce Motion. Shared by
/// `SessionStateDot`, the session rows and the Agents tab.
struct PulsingLiveDot: View {
    /// The disc's diameter. 9 on its own; the coding-now badge passes the
    /// pill's own `dotSize` so the live dot and the parked states' static dot
    /// are the same mark.
    var size: CGFloat = 9

    @Environment(\.motion) private var motion
    @State private var pulsing = false

    var body: some View {
        Circle()
            .fill(DesignTokens.Semantic.green)
            .frame(width: size, height: size)
            .overlay(
                Circle()
                    .stroke(DesignTokens.Semantic.green.opacity(0.6), lineWidth: 2)
                    .scaleEffect(pulsing ? 2.2 : 1.0)
                    .opacity(pulsing ? 0 : 0.8)
            )
            .onAppear {
                // EXP-523: the period and the outward-only curve stay this
                // indicator's own; only the Reduce Motion decision moves into
                // the shared helper. The guard stays because `pulsing` drives
                // the ring's resting scale/opacity too — flipping it with no
                // animation would snap the ring to scale 2.2 at opacity 0,
                // i.e. delete it, instead of leaving a static ring.
                guard !motion.reduceMotion else { return }
                withAnimation(
                    motion.pulse(duration: 1.4, autoreverses: false, curve: .easeOut)
                ) {
                    pulsing = true
                }
            }
    }
}
