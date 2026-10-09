import SwiftUI

// The long-press arm of THE menu (web `menuProps(kind, id)` = a context menu,
// Android `combinedClickable` + `GlassMenu`): a row whose secondary actions
// sit behind a press-and-hold floats the SAME `GlassMenu` popup from the row,
// never the system `.contextMenu` (a lifted preview + a UIKit menu that takes
// no styling, the one surface on the phone that read like another app).
//
// The row's TAP stays the caller's (`onTap`), owned here with the long press
// so the two never both fire: a short tap opens, a hold opens the menu. The
// row still reads as ONE button to VoiceOver; a host names each menu item
// as an `.accessibilityAction(named:)` too, since VoiceOver has no hold.

public extension View {
    /// - Parameters:
    ///   - enabled: false = no menu (the row is only its tap).
    ///   - onTap: the row's primary action; nil = the row has none.
    ///   - menu: `GlassMenuItem`s, exactly as in a `GlassMenu`.
    func glassLongPressMenu<MenuContent: View>(
        enabled: Bool = true,
        onTap: (() -> Void)? = nil,
        @ViewBuilder menu: @escaping () -> MenuContent
    ) -> some View {
        modifier(GlassLongPressMenu(enabled: enabled, onTap: onTap, menu: menu))
    }
}

public enum GlassLongPressMenuTokens {
    /// The system context menu's own hold, so the gesture feels the same.
    public static let minimumDuration: Double = 0.45
}

private struct GlassLongPressMenu<MenuContent: View>: ViewModifier {
    let enabled: Bool
    let onTap: (() -> Void)?
    @ViewBuilder let menu: () -> MenuContent

    @State private var anchor: CGRect = .zero
    @State private var isPresented = false

    func body(content: Content) -> some View {
        content
            .contentShape(Rectangle())
            .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
                anchor = frame
            }
            // The tap is attached BEFORE the hold: SwiftUI then fires the
            // tap only for a short press and the hold alone for a long one.
            .onTapGesture { onTap?() }
            .onLongPressGesture(minimumDuration: GlassLongPressMenuTokens.minimumDuration) {
                guard enabled else { return }
                var transaction = Transaction()
                transaction.disablesAnimations = true
                withTransaction(transaction) { isPresented = true }
            }
            .sensoryFeedback(.impact(weight: .medium), trigger: isPresented) { _, presented in presented }
            .accessibilityAddTraits(onTap == nil ? [] : .isButton)
            .accessibilityAction { onTap?() }
            .glassMenuOverlay(isPresented: $isPresented, anchor: anchor, content: menu)
    }
}
