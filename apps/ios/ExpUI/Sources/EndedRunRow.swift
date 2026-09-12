import SwiftUI

/// EXP-637: the ONE row a runs list draws — the Actions tab's "Recent
/// automated runs" and the Devices tab's "Past".
///
/// EXP-773 made it a plain LINK. A row used to expand to the agent's close-out
/// summary and a Resume pill; both now live at the top of the fullscreen
/// session view, where the transcript they belong to is. So a row is title ·
/// state · byline and a tap opens that session — live or finished, the same
/// gesture, the same destination. The same rule holds on web, desktop and
/// Android.
///
/// EXP-818: it wears the FLAT list row (`.flatRow()`) — both its lists sit
/// under a `GlassSectionBand` now, and a bordered card inside a table was the
/// one row that still read as a card.
public struct EndedRunRow: View {
    private let title: String
    private let identifier: String?
    private let byline: String
    private let isLive: Bool
    private let onOpen: () -> Void

    /// - Parameter isLive: the run is still going — the row says so; the tap
    ///   target is the same either way.
    public init(
        title: String,
        identifier: String? = nil,
        byline: String,
        isLive: Bool = false,
        onOpen: @escaping () -> Void
    ) {
        self.title = title
        self.identifier = identifier
        self.byline = byline
        self.isLive = isLive
        self.onOpen = onOpen
    }

    public var body: some View {
        Button(action: onOpen) {
            header
        }
        .buttonStyle(.plain)
        // The styleguide capture taps this to reach the session view
        // (EXP-663); same tag as Android's.
        .accessibilityIdentifier("ended-run-row")
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
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

            AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .contentShape(Rectangle())
    }
}
