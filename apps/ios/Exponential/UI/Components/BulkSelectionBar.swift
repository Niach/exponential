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

/// The multi-select glyph cell a row leads with while selecting (EXP-239):
/// `ui-selected` in the primary colour, `ui-unselected` muted.
struct SelectionGlyph: View {
    let selected: Bool

    var body: some View {
        AppIcon(selected ? AppIcons.uiSelected : AppIcons.uiUnselected, size: AppIcon.Size.medium)
            .foregroundStyle(selected ? DesignTokens.Palette.primary : .white.opacity(TextOpacity.tertiary))
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

extension View {
    /// #EXP-1388: a chat row leads with the selection glyph cell while the
    /// list selects (`selected` nil = not selecting).
    func chatSelectionLead(_ selected: Bool?) -> some View {
        HStack(spacing: 12) {
            if let selected {
                SelectionGlyph(selected: selected)
            }
            self
        }
    }
}
