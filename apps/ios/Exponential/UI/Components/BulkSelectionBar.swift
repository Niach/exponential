import ExpCore
import ExpUI
import SwiftUI

// THE bulk selection bar's shell (EXP-239 / EXP-698 r5), shared by the issue
// list's bar and the Chats list's (#EXP-1388 chats.json SELECTION: "the
// issue list's bulk action bar, same capsule, placement and phone chrome
// swap"): close, the count, then the host's actions, on the tab pill's
// 52pt glass capsule in the tab bar's slot.

/// The bar's capsule: ✕, the count, a spacer, then `actions`.
struct BulkSelectionBar<Actions: View>: View {
    let count: String
    /// The capture / UI-test id (`bulk-selection-bar` on issues,
    /// chats.json `testIds.selectionBar` on chats).
    let testId: String
    let onClose: () -> Void
    @ViewBuilder let actions: () -> Actions

    var body: some View {
        HStack(spacing: 0) {
            Button(action: onClose) {
                AppIcon(AppIcons.uiClose, size: 15, weight: .semibold)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Clear selection")

            Text(count)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(1)
                .padding(.trailing, 2)

            Spacer(minLength: 4)

            actions()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        // The tab pill's height: the bar stands in its slot, so it must be its
        // size too (EXP-698 r5).
        .frame(height: 52)
        .glassCard(cornerRadius: 24, isOpaque: true)
        .transition(.move(edge: .bottom).combined(with: .opacity))
        // EXP-642: the styleguide capture addresses the bar directly.
        // `contain` keeps its buttons queryable.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(testId)
    }
}

/// One 32pt icon button in the bar (EXP-247).
struct BulkBarIconButton: View {
    let icon: String
    let color: Color
    let accessibility: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            AppIcon(icon, size: AppIcon.Size.medium)
                .foregroundStyle(color)
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibility)
    }
}

/// The multi-select cell a row leads with while selecting (EXP-239): THE
/// square checkbox (`ExpCheckbox`, 16pt) centred in the medium icon cell.
struct SelectionGlyph: View {
    let selected: Bool

    var body: some View {
        ExpCheckbox(checked: selected)
            .frame(width: AppIcon.Size.medium, height: AppIcon.Size.medium)
    }
}

/// #EXP-1388: the Chats list's bar = the shell with `chatSelectionActions`
/// as its icon buttons (Delete destructive and last, like the issue bar's).
struct ChatSelectionBar: View {
    let count: Int
    let items: [Chats.ChatSelectionItem]
    let onClose: () -> Void
    let onAction: (Chats.ChatSelectionItem) -> Void

    var body: some View {
        BulkSelectionBar(
            count: Chats.Copy.Selection.countLine(count),
            testId: Chats.TestIds.selectionBar,
            onClose: onClose
        ) {
            ForEach(items, id: \.action) { item in
                BulkBarIconButton(
                    icon: AppIcons.conceptOrName(item.action.icon) ?? item.action.icon,
                    color: item.action.destructive
                        ? DesignTokens.Palette.destructive
                        : .white.opacity(TextOpacity.secondary),
                    accessibility: item.action.label
                ) { onAction(item) }
                .padding(.leading, item.action.destructive ? 4 : 0)
                .accessibilityIdentifier("chat-selection-\(item.action.rawValue)")
            }
        }
    }
}

/// THE bulk-selection row mechanics (EXP-239 / EXP-698 r5), shared by the
/// issue list and the Chats list (#EXP-1388: "exactly like the issue list's
/// bulk selection"): the long-press that enters, the tap that toggles, the
/// selected row's wash + hairline.
enum BulkSelection {
    /// The row's hold that enters the selection.
    static let longPressDuration: Double = 0.35
    /// The selected-row wash and its hairline, on the row card's radius.
    static let highlightRadius: CGFloat = 10
    static let highlightFill = DesignTokens.Palette.primary.opacity(0.12)
    static let highlightStroke = DesignTokens.Palette.primary.opacity(0.45)
    /// The issue card's content inset (`issueRowContent`'s 12pt) and its
    /// glyph → first column gap (the row HStack's 10pt).
    static let rowInset: CGFloat = 12
    static let glyphGap: CGFloat = 10
}

/// One armed tap-swallow (EXP-698 r5). A long-press flips the selection on
/// while the finger is still DOWN; the release that follows arrives at the
/// row as an ordinary tap, which in selection mode means "toggle" — so the
/// row the press just selected would deselect itself. The row arms this and
/// its next tap consumes it.
///
/// The window has to outlast a comfortable hold (the press registers at
/// 0.35s, the finger may stay down well past that) while still expiring,
/// because a system-cancelled touch never sends the release that would
/// consume it.
private struct TapSuppression {
    static let window: Duration = .seconds(2)

    let armedAt = ContinuousClock.now

    var isExpired: Bool { armedAt.duration(to: ContinuousClock.now) > Self.window }
}

/// ONE Button for both modes (EXP-698 r5): the mode is decided at tap time
/// (`onTap` toggles while selecting, opens otherwise) rather than by swapping
/// two Buttons, and the long-press is simultaneous so the plain tap keeps
/// working. The haptic is the HOST's (`sensoryFeedback` on its selection).
private struct BulkSelectableRow: ViewModifier {
    let selecting: Bool
    let onTap: () -> Void
    let onLongPress: () -> Void
    @State private var suppression: TapSuppression?

    func body(content: Content) -> some View {
        Button {
            // The release that ended the long-press, not a tap: it would
            // toggle the just-selected row straight back out (and, as the
            // only selected row, leave selection mode again).
            if let armed = suppression {
                suppression = nil
                if !armed.isExpired { return }
            }
            onTap()
        } label: {
            content
        }
        .buttonStyle(.plain)
        .simultaneousGesture(
            LongPressGesture(minimumDuration: BulkSelection.longPressDuration).onEnded { _ in
                guard !selecting else { return }
                suppression = TapSuppression()
                onLongPress()
            }
        )
    }
}

extension View {
    /// The bulk-selectable row's tap + long-press (see `BulkSelectableRow`).
    func bulkSelectable(
        selecting: Bool,
        onTap: @escaping () -> Void,
        onLongPress: @escaping () -> Void
    ) -> some View {
        modifier(BulkSelectableRow(selecting: selecting, onTap: onTap, onLongPress: onLongPress))
    }

    /// The selected row's unmistakable neutral wash + hairline on top of the
    /// row (the check glyph alone is easy to miss mid-scroll). `verticalInset`
    /// pulls it in for rows that stack with no gap of their own.
    func selectedRowHighlight(_ selected: Bool, verticalInset: CGFloat = 0) -> some View {
        overlay {
            if selected {
                RoundedRectangle(cornerRadius: BulkSelection.highlightRadius)
                    .fill(BulkSelection.highlightFill)
                    .overlay(
                        RoundedRectangle(cornerRadius: BulkSelection.highlightRadius)
                            .stroke(BulkSelection.highlightStroke, lineWidth: 1)
                    )
                    .padding(.vertical, verticalInset)
                    .allowsHitTesting(false)
            }
        }
    }

    /// #EXP-1388 (owner on #1286, WhatsApp's reference): a chat row while
    /// the list selects. NOTHING shifts (no leading control column); a
    /// SELECTED row wears a full-width flat tint (glass fillActive, edge to
    /// edge of the screen past the list's `screenInset`, no radius, no
    /// hairline). The badge on the disc is `ChatRowView(selected:)`'s.
    func chatSelectedTint(_ selected: Bool, screenInset: CGFloat = 16) -> some View {
        background {
            if selected {
                Rectangle()
                    .fill(GlassTokens.fillActive)
                    .padding(.horizontal, -screenInset)
                    .allowsHitTesting(false)
            }
        }
    }
}
