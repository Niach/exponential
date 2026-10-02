import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the Work screen's nav-bar title — IDENTICAL across faces, so it
/// never jumps: a small session-state dot (live / needs input; none without a
/// live run) beside the issue identifier, or the session title (Chat / the
/// action name / Batch run) for an issue-less run.
///
/// EXP-1162: an issue subject's title COLLAPSES (`DetailChrome`) once the
/// Issue face's own title row has scrolled under the header band — and on
/// every face without that row: the mono identifier small and muted on top,
/// the issue title on ONE truncated line under it. A threshold, not a morph:
/// the collapsed form fades in over `collapseMs` while rising `collapseRise`.
struct WorkTitle: View {
    let text: String
    /// Nil = no dot.
    let tone: SessionDotTone?
    /// EXP-848: pulse only while the agent is inside a turn.
    let pulsing: Bool
    /// The issue's title — nil (an issue-less run) never collapses.
    var issueTitle: String? = nil
    var collapsed: Bool = false

    @Environment(\.motion) private var motion

    private var showsCollapsed: Bool {
        collapsed && issueTitle?.isEmpty == false
    }

    var body: some View {
        ZStack {
            if showsCollapsed, let issueTitle {
                collapsedTitle(issueTitle)
                    .transition(
                        .asymmetric(
                            insertion: .opacity.combined(
                                with: .offset(y: DetailChrome.collapseRise)
                            ),
                            removal: .opacity
                        )
                    )
            } else {
                expandedTitle
                    .transition(.opacity)
            }
        }
        .animation(motion.decelerate(DetailChrome.collapseMs / 1000), value: showsCollapsed)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("work-title")
    }

    private var expandedTitle: some View {
        HStack(spacing: 6) {
            dot
            Text(text)
                .font(.headline)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }

    private func collapsedTitle(_ issueTitle: String) -> some View {
        VStack(spacing: 1) {
            HStack(spacing: 5) {
                dot
                Text(text)
                    .font(.caption2.monospaced().weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            Text(issueTitle)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }

    @ViewBuilder
    private var dot: some View {
        if let tone {
            SessionStateDot(tone: tone, pulsing: pulsing, size: showsCollapsed ? 6 : 8)
        }
    }
}
