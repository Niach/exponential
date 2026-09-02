import SwiftUI

/// EXP-637: the ONE row a runs list draws — the Actions tab's "Recent
/// automated runs" (EXP-676 dropped the Agents tab's "Recent runs").
///
/// Ended rows are EXPANDABLE, and the summary is deliberately NOT shown
/// inline: a close-out is a paragraph, and a list of paragraphs is unreadable.
/// Collapsed it is title · state · byline; tapping reveals the agent's full
/// summary — rendered by the caller's `summary` builder, so the app can hand
/// it real markdown (EXP-686) — and the Resume pill when the run is resumable.
/// LIVE rows carry no outcome, no chevron and no expansion: the whole header
/// opens the session instead (`isLive` + `onOpen`). The same rule holds on
/// web, desktop and Android.
public struct EndedRunRow<Summary: View>: View {
    private let title: String
    private let identifier: String?
    private let byline: String
    private let summaryText: String?
    private let expanded: Bool
    private let canResume: Bool
    private let resuming: Bool
    private let isLive: Bool
    private let onToggle: () -> Void
    private let onResume: () -> Void
    private let onOpen: () -> Void
    private let summary: (String) -> Summary

    /// - Parameters:
    ///   - isLive: the run is still going — the header opens the session
    ///     (`onOpen`) instead of expanding, and no chevron is drawn.
    ///   - summary: renders the agent's close-out text. The app passes a
    ///     markdown view; the fallback renders plain text.
    public init(
        title: String,
        identifier: String? = nil,
        byline: String,
        summary summaryText: String?,
        expanded: Bool,
        canResume: Bool,
        resuming: Bool = false,
        isLive: Bool = false,
        onToggle: @escaping () -> Void,
        onResume: @escaping () -> Void = {},
        onOpen: @escaping () -> Void = {},
        @ViewBuilder summary: @escaping (String) -> Summary
    ) {
        self.title = title
        self.identifier = identifier
        self.byline = byline
        self.summaryText = summaryText
        self.expanded = expanded
        self.canResume = canResume
        self.resuming = resuming
        self.isLive = isLive
        self.onToggle = onToggle
        self.onResume = onResume
        self.onOpen = onOpen
        self.summary = summary
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button(action: isLive ? onOpen : onToggle) {
                header
            }
            .buttonStyle(.plain)
            // The collapsed row is what the styleguide capture taps to reach
            // the summary + Resume state (EXP-663); same tag as Android's.
            .accessibilityIdentifier("ended-run-row")

            if expanded, !isLive {
                if let summaryText, !summaryText.isEmpty {
                    summary(summaryText)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("run-summary")
                } else {
                    Text("This run left no summary.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if canResume {
                    GlassPill(
                        "Resume",
                        icon: AppIcons.runResume,
                        mode: .action(onResume),
                        enabled: !resuming
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
                    // EXP-686: "Running" is the ONLY status word left — a
                    // finished row just carries its byline.
                    if isLive {
                        Text("Running")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(DesignTokens.Semantic.green)
                            .lineLimit(1)
                    }
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

            if !isLive {
                AppIcon(
                    expanded ? AppIcons.uiChevronUp : AppIcons.uiChevronDown,
                    size: AppIcon.Size.small
                )
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .contentShape(Rectangle())
    }
}

extension EndedRunRow where Summary == AnyView {
    /// The plain-text fallback: callers that have no markdown stack (or don't
    /// want one) get the same caption the row drew before EXP-686.
    public init(
        title: String,
        identifier: String? = nil,
        byline: String,
        summary summaryText: String?,
        expanded: Bool,
        canResume: Bool,
        resuming: Bool = false,
        isLive: Bool = false,
        onToggle: @escaping () -> Void,
        onResume: @escaping () -> Void = {},
        onOpen: @escaping () -> Void = {}
    ) {
        self.init(
            title: title,
            identifier: identifier,
            byline: byline,
            summary: summaryText,
            expanded: expanded,
            canResume: canResume,
            resuming: resuming,
            isLive: isLive,
            onToggle: onToggle,
            onResume: onResume,
            onOpen: onOpen,
            summary: { text in
                AnyView(
                    Text(text)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                )
            }
        )
    }
}
