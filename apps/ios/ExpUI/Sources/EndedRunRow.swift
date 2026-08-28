import ExpCore
import SwiftUI

/// EXP-637: the ONE row a runs list draws — the Actions tab's "Recent
/// automated runs" (EXP-676 dropped the Agents tab's "Recent runs").
///
/// Rows are EXPANDABLE, and the summary is deliberately NOT shown inline: a
/// close-out is a paragraph, and a list of paragraphs is unreadable. Collapsed
/// it is title · outcome glyph + tinted label · byline; tapping reveals the
/// agent's full summary (plain text — the close-out is GFM, but a run list is
/// not a document surface) and the Resume pill when the run is resumable. The
/// same rule holds on web, desktop and Android.
public struct EndedRunRow: View {
    private let title: String
    private let identifier: String?
    private let outcome: String?
    private let stateLabel: String
    private let byline: String
    private let summary: String?
    private let expanded: Bool
    private let canResume: Bool
    private let resuming: Bool
    private let onToggle: () -> Void
    private let onResume: () -> Void

    /// - Parameters:
    ///   - outcome: a `codingSessionOutcome` value, or nil on a row with no
    ///     agent-declared outcome (a still-running automated run, or an end
    ///     that came from the kill switch / a merge). Drives glyph and tint.
    ///   - stateLabel: the word next to the glyph — `RunOutcomePresentation`
    ///     for an ended row, the live status for one still going.
    public init(
        title: String,
        identifier: String? = nil,
        outcome: String?,
        stateLabel: String,
        byline: String,
        summary: String?,
        expanded: Bool,
        canResume: Bool,
        resuming: Bool = false,
        onToggle: @escaping () -> Void,
        onResume: @escaping () -> Void = {}
    ) {
        self.title = title
        self.identifier = identifier
        self.outcome = outcome
        self.stateLabel = stateLabel
        self.byline = byline
        self.summary = summary
        self.expanded = expanded
        self.canResume = canResume
        self.resuming = resuming
        self.onToggle = onToggle
        self.onResume = onResume
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button(action: onToggle) {
                header
            }
            .buttonStyle(.plain)
            // The collapsed row is what the styleguide capture taps to reach
            // the summary + Resume state (EXP-663); same tag as Android's.
            .accessibilityIdentifier("ended-run-row")

            if expanded {
                if let summary, !summary.isEmpty {
                    Text(summary)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("run-summary")
                } else {
                    Text("This run left no summary.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if canResume {
                    GlassPillButton(
                        "Resume", icon: AppIcons.runResume, enabled: !resuming, action: onResume
                    )
                    .accessibilityIdentifier("resume-run")
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassRow()
    }

    private var header: some View {
        HStack(spacing: 10) {
            if let glyph = Self.outcomeIcon(outcome) {
                AppIcon(glyph, size: AppIcon.Size.medium)
                    .foregroundStyle(Self.outcomeColor(outcome))
            }

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
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
                }
                HStack(spacing: 6) {
                    Text(stateLabel)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Self.outcomeColor(outcome))
                        .lineLimit(1)
                    if !byline.isEmpty {
                        Text(byline)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                }
            }

            Spacer(minLength: 0)

            if resuming {
                ProgressView().controlSize(.mini).tint(.white)
            }

            AppIcon(
                expanded ? AppIcons.uiChevronUp : AppIcons.uiChevronDown,
                size: AppIcon.Size.small
            )
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .contentShape(Rectangle())
    }

    /// The registry CONCEPT per outcome (EXP-637 icons) — nil while a run has
    /// declared none, which is also every still-running row.
    static func outcomeIcon(_ outcome: String?) -> String? {
        switch outcome {
        case DomainContract.codingSessionOutcomeDone: return AppIcons.runOutcomeDone
        case DomainContract.codingSessionOutcomeBlocked: return AppIcons.runOutcomeBlocked
        case DomainContract.codingSessionOutcomeNoChanges: return AppIcons.runOutcomeNoChanges
        default: return nil
        }
    }

    /// Done reads completed (the issue-status blue `done` already wears),
    /// blocked reads attention (amber, like a needs-input session), and "no
    /// changes"/"Ended" stay neutral: nothing happened is not a failure.
    static func outcomeColor(_ outcome: String?) -> Color {
        switch outcome {
        case DomainContract.codingSessionOutcomeDone: return DesignTokens.Semantic.blue
        case DomainContract.codingSessionOutcomeBlocked: return DesignTokens.Semantic.yellow
        default: return DesignTokens.Semantic.neutral
        }
    }
}
