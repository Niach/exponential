import ExpUI
import ExpCore
import SwiftUI

/// The live-session slot on issue detail (EXP-698 r4, restyled EXP-818/845).
///
/// It used to be a CARD — a tinted state pill, a byline and a Watch pill in
/// their own glass panel under the property chips. EXP-818 collapsed it on
/// every client (`issue-coding-rows.tsx` `variant === "start"`, the IDE's
/// `coding_now_slot`): a run the caller OWNS is just the primary **Watch**
/// pill, straight into its screen, and anyone else's is a MUTED caption
/// (`● Coding now · name`) — a card said the same thing twice and made the
/// most perishable state on the page look heavier than the issue.
///
/// The dot still pulses only while the agent is inside a turn (EXP-848), and
/// the parked states keep their own word and tone (Needs input / Ready for
/// review / Done). Renders nothing without a live session.
struct CodingNowCard: View {
    let issue: IssueEntity
    let runningSessions: [CodingSessionEntity]
    let users: [UserEntity]
    /// Relay config, loaded by the view model's refreshSteer (EXP-240) —
    /// gates Watch.
    let config: SteerConfig?
    let currentUserId: String?

    @Environment(\.accountId) private var accountId

    /// Multi-window desktops can run several sessions on one issue — surface the
    /// most recent (any presence at all counts as "coding now").
    private var session: CodingSessionEntity? {
        runningSessions.max { $0.startedAt < $1.startedAt }
    }

    var body: some View {
        if let session {
            slot(session)
        }
    }

    private func slot(_ session: CodingSessionEntity) -> some View {
        let ownSession = currentUserId != nil && session.userId == currentUserId
        let canWatch = ownSession && config?.enabled == true
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
        return VStack(alignment: .leading, spacing: 6) {
            if canWatch {
                // EXP-818: the caller's OWN run is the ONE loud thing here —
                // tap into the run, where the header's Stop lives. The app's
                // link-around-a-pill pattern (the duplicate banner, Support's
                // linked issue): the pill stays a resting label and the
                // NavigationLink owns the tap.
                NavigationLink(value: AppRoute.agentSession(
                    accountId: accountId, sessionId: session.id
                )) {
                    GlassPill("Watch", icon: AppIcons.navDevices, size: .sm, primary: true)
                        .contentShape(Capsule())
                }
                // Not `.plain`: the link owns the press, so it has to be the
                // one that dims the pill's solid fill.
                .buttonStyle(.glassPillPrimary)
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                // EXP-818: anyone else's run (and the caller's own on an
                // instance with the relay off) is a READ-ONLY caption — the
                // dot, the state's word, and whose run it is. EXP-848: the dot
                // pulses only while a turn is open, so an idle live run reads
                // as the static green it is.
                HStack(spacing: 6) {
                    if CodingSessionDisplayState.pulses(
                        state: state, agentBusy: session.agentBusy
                    ) {
                        PulsingLiveDot(size: GlassPillTokens.dotSize)
                    } else {
                        Circle()
                            .fill(tint)
                            .frame(
                                width: GlassPillTokens.dotSize,
                                height: GlassPillTokens.dotSize
                            )
                    }
                    Text(caption(label: label, owner: owner, session: session))
                        .font(.caption)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            // Relay explicitly off on this instance: the caption stays,
            // steering doesn't. (config?.enabled == false is only true once
            // config loads.)
            if ownSession, config?.enabled == false {
                Text("Live steering is unavailable on this instance.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // EXP-642: the store slide's pop-out rect is measured off this card
        // (`PopRects`). `contain` keeps the Watch link inside it queryable.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("coding-now-row")
    }

    /// EXP-818: the read-only caption — the state's word, and for a TEAMMATE's
    /// run the person it belongs to ("Coding now · Ada"). The caller's own run
    /// names nobody: it is the Watch pill's caption only when steering is off,
    /// where "· you" would be noise (web `issue-coding-rows.tsx` parity).
    private func caption(
        label: String, owner: UserEntity?, session: CodingSessionEntity
    ) -> String {
        let ownSession = currentUserId != nil && session.userId == currentUserId
        guard !ownSession else { return label }
        return "\(label) · \(memberDisplayName(owner, id: session.userId))"
    }
}

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
/// on the diff page (ChangesView).
struct AgentPrCard: View {
    let issue: IssueEntity

    @Environment(\.accountId) private var accountId

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
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: issue.id)) {
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
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: issue.id)) {
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
/// the "Coding now" green, animated. Static under Reduce Motion. Shared by the
/// issue-detail card, the bottom bar's start circle, and the Agents tab.
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
