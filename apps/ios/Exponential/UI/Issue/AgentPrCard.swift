import ExpUI
import ExpCore
import SwiftUI

/// The compact PR/branch status section on issue detail (EXP-156). EXP-240
/// moved the remote-start affordance into the bottom bar's Start-coding
/// circle; EXP-246 dropped the glass card wrapper (full-width rows, Linear
/// parity); EXP-698 r4 moved the session row out into `CodingNowCard`, which
/// sits above the description with the property chips. What is left are the two
/// rows that belong beside the code:
///   - PR:      a linked PR → GitHub-style capsule chip (pull icon tinted by
///              state + "PR #n"), tapping opens the diff page.
///   - Branch:  a pushed branch, no PR yet → branch icon + mono name chip,
///              same diff page.
/// No inline Close/Merge/GitHub-link/diff-count here — the review actions live
/// on the Changes face. EXP-893: the rows are BUTTONS that switch the Work
/// screen to its Changes face (`onOpenChanges`), never a push.
struct AgentPrCard: View {
    let issue: IssueEntity
    let onOpenChanges: () -> Void

    private var showsCard: Bool {
        issue.prUrl != nil || (issue.branch?.isEmpty == false)
    }

    var body: some View {
        Group {
            if showsCard {
                content
            }
        }
    }

    // Full-width rows, no card wrapper (EXP-246) — the PR/branch chips keep
    // their own glassButton capsules.
    private var content: some View {
        VStack(alignment: .leading, spacing: 10) {
            if issue.prUrl != nil {
                prChip
            } else if let branch = issue.branch, !branch.isEmpty {
                branchChip(branch)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - PR / branch chips (GitHub-style, EXP-240)

    /// Pull-request icon tint per PR state: open green, merged blue (the
    /// done-status semantic, web/desktop parity), closed red.
    private var prTint: Color {
        switch issue.prState {
        case DomainContract.prStateMerged: DesignTokens.Semantic.blue
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

    // EXP-327: a full-width row rather than a hug-width capsule — the linked PR
    // is the way into the code, not a stray chip (Linear parity). The branch
    // variant takes the same shape: they occupy the same slot.
    private var prChip: some View {
        Button(action: onOpenChanges) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.prOpen, size: AppIcon.Size.small, weight: .semibold)
                    .foregroundStyle(prTint)
                Text(prLabel)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 8)
                if let prState = issue.prState, !prState.isEmpty {
                    Text(prState.capitalized)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func branchChip(_ branch: String) -> some View {
        Button(action: onOpenChanges) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.actionRepository, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(branch)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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
