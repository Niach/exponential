import SwiftUI

/// EXP-1316: THE phone reply box ×3 (Android `FloatingComposer.kt`, web
/// phone `FloatingComposer`) — used by a thread's Conversation AND the
/// session view. Collapsed, it is the floating bar's capsule reading the
/// placeholder (`Reply…`), with an optional leading circle (the session's
/// usage ring); a tap expands it into THE composer card the host hands in
/// (text, `+` images/files, send, Stop while busy). It floats over the
/// content above the bottom edge fade and never pushes the content: the
/// host mounts it as a bottom overlay / safe-area inset.
///
/// The fold rule is the host's (its draft knows when nothing would be lost):
/// `open` = expanded OR a draft/pick keeps it open; the host flips its
/// `expanded` state back to false on blur or idle.
public struct FloatingComposer<Leading: View, Card: View>: View {
    private let placeholder: String
    private let open: Bool
    private let collapsedLabel: String
    private let collapsedIdentifier: String
    private let onExpand: () -> Void
    private let leading: Leading
    private let card: Card

    /// - Parameters:
    ///   - placeholder: the capsule's words (the card's own placeholder).
    ///   - open: the card shows (expanded, or a draft keeps it open).
    ///   - collapsedLabel: the capsule's accessibility label.
    ///   - collapsedIdentifier: the capsule's accessibility identifier.
    ///   - onExpand: the capsule's tap — the host expands + focuses.
    public init(
        placeholder: String,
        open: Bool,
        collapsedLabel: String,
        collapsedIdentifier: String,
        onExpand: @escaping () -> Void,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder card: () -> Card
    ) {
        self.placeholder = placeholder
        self.open = open
        self.collapsedLabel = collapsedLabel
        self.collapsedIdentifier = collapsedIdentifier
        self.onExpand = onExpand
        self.leading = leading()
        self.card = card()
    }

    public var body: some View {
        Group {
            if open {
                card
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
            } else if Leading.self == EmptyView.self {
                // No leading circle: the capsule spans the bar's inset
                // edge to edge (FloatingBottomBar's spacing would leave a
                // phantom gap on the trailing side).
                capsule
                    .padding(.horizontal, FloatingBarTokens.inset)
                    .padding(.top, FloatingBarTokens.topPadding)
                    .padding(.bottom, FloatingBarTokens.bottomPadding)
            } else {
                FloatingBottomBar {
                    leading
                } center: {
                    capsule
                } trailing: {
                    EmptyView()
                }
            }
        }
    }
}

extension FloatingComposer {
    fileprivate var capsule: some View {
        FloatingBarCapsule(accessibilityLabel: collapsedLabel, action: onExpand) {
            Text(placeholder)
                .font(.subheadline)
                .lineLimit(1)
        }
        .accessibilityIdentifier(collapsedIdentifier)
    }
}

extension FloatingComposer where Leading == EmptyView {
    public init(
        placeholder: String,
        open: Bool,
        collapsedLabel: String,
        collapsedIdentifier: String,
        onExpand: @escaping () -> Void,
        @ViewBuilder card: () -> Card
    ) {
        self.init(
            placeholder: placeholder,
            open: open,
            collapsedLabel: collapsedLabel,
            collapsedIdentifier: collapsedIdentifier,
            onExpand: onExpand,
            leading: { EmptyView() },
            card: card
        )
    }
}
