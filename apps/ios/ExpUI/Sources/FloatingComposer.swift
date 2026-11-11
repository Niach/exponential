import SwiftUI

/// EXP-1316: THE phone reply box ×3 (Android `FloatingComposer.kt`, web
/// phone `FloatingComposer`) — used by a chat's Conversation AND the
/// session view. Collapsed, it is the floating bar's capsule reading the
/// placeholder (`Reply…`), nothing beside it (EXP-1676: the usage ring lives
/// IN the card, next to send, ×4); a tap expands it into THE composer card the
/// host hands in (text, `+` images/files, send, Stop while busy). #EXP-1782:
/// the host stacks it UNDER the content (the transcript's scroll area ends at
/// its top edge, clipped there): nothing renders behind it, no edge fade.
///
/// The fold rule is the host's (its draft knows when nothing would be lost):
/// `open` = expanded OR a draft/pick keeps it open; the host flips its
/// `expanded` state back to false on blur or idle.
public struct FloatingComposer<Card: View>: View {
    private let placeholder: String
    private let open: Bool
    private let collapsedLabel: String
    private let collapsedIdentifier: String
    private let onExpand: () -> Void
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
        @ViewBuilder card: () -> Card
    ) {
        self.placeholder = placeholder
        self.open = open
        self.collapsedLabel = collapsedLabel
        self.collapsedIdentifier = collapsedIdentifier
        self.onExpand = onExpand
        self.card = card()
    }

    public var body: some View {
        Group {
            if open {
                card
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
            } else {
                // The capsule spans the bar's inset edge to edge.
                capsule
                    .padding(.horizontal, FloatingBarTokens.inset)
                    .padding(.top, FloatingBarTokens.topPadding)
                    .padding(.bottom, FloatingBarTokens.bottomPadding)
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
