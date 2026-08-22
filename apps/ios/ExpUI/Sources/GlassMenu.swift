import SwiftUI

// The one glass menu treatment (EXP-603) — the iOS twin of Android's
// ui/components/GlassMenu.kt (`GlassMenuDefaults`, pinned by
// GlassMenuDefaultsTest). Android could restyle material3's `DropdownMenu`
// through parameters; SwiftUI's `Menu` renders a UIKit `UIMenu` that takes no
// styling at all, so the anchored popup itself is hand-rolled here. Geometry
// and colors are byte-matched to Android's: 12pt radius, 0.5pt white-.10
// hairline, 48pt rows with 12pt horizontal padding, white-.9 labels, white-.7
// icons, white-.06 dividers, and the fill composited as white .06 over
// `DesignTokens.Palette.card` (#171717) — the opaque #252525 Android spells as
// `GlassTokens.OpaqueCardFill`. Never hard-code that hex: a token change has to
// move both platforms at once.
//
// No shadow and no material blur, deliberately: separation comes from the
// opaque fill plus a hairline everywhere else in this app, and a menu floating
// over arbitrary content lets a low-alpha/blurred fill bleed through.
//
// Absent twins, deliberately:
//   - `PopupProperties` / `offset`: positioning here is automatic (trailing
//     edge of the anchor, flipping above when it would not fit below), so
//     there is nothing for a call site to tune.
//   - MarkdownToolbar's `UIMenu`s stay system: they hang off a UIKit keyboard
//     accessory view, which has no SwiftUI presentation host to anchor to.
//   - ReviewsView's long-press `.contextMenu` stays system: a press-and-hold
//     preview is a different interaction, not a dropdown.

// MARK: - Tokens

public enum GlassMenuTokens {
    /// 12pt — the CARD rung of the glass ladder, Android's `SectionRadius`.
    public static let radius: CGFloat = 12
    public static let hairline: CGFloat = 0.5
    /// White .10 — the card stroke.
    public static let strokeOpacity: Double = 0.10
    /// Android/material pins menu rows at 48dp; already past the 44pt target.
    public static let itemMinHeight: CGFloat = 48
    public static let itemHPadding: CGFloat = 12
    public static let textOpacity: Double = 0.9
    /// The same white .06 hairline every other divider in the app uses.
    public static let dividerOpacity: Double = 0.06
    public static let minWidth: CGFloat = 180
    public static let maxWidth: CGFloat = 280
    /// White .06 over the opaque card fill == #252525 (EXP-357 parity).
    public static let tintOpacity: Double = 0.06
    /// Gap between the anchor and the menu, and the minimum screen margin.
    static let anchorGap: CGFloat = 8
    static let screenMargin: CGFloat = 12
}

// MARK: - Surface

/// The menu container chrome on its own — for popups that must hand-roll their
/// own positioning (the markdown editor's `@`/`#` autocomplete tracks the caret
/// rect above/below the IME). Same fill, stroke and radius, so the two menu
/// families cannot drift apart.
public struct GlassMenuSurface<Content: View>: View {
    @ViewBuilder let content: () -> Content

    public init(@ViewBuilder content: @escaping () -> Content) {
        self.content = content
    }

    public var body: some View {
        content()
            .background(Color.white.opacity(GlassMenuTokens.tintOpacity))
            .background(DesignTokens.Palette.card)
            .clipShape(RoundedRectangle(cornerRadius: GlassMenuTokens.radius))
            .overlay(
                RoundedRectangle(cornerRadius: GlassMenuTokens.radius)
                    .stroke(
                        Color.white.opacity(GlassMenuTokens.strokeOpacity),
                        lineWidth: GlassMenuTokens.hairline
                    )
            )
    }
}

// MARK: - Rows

/// The styled content of a menu row — for the rare host that is not a `Button`.
/// Everything else uses `GlassMenuItem`.
public struct GlassMenuItemLabel: View {
    let title: String
    var icon: String? = nil
    var destructive: Bool = false
    var enabled: Bool = true

    public init(_ title: String, icon: String? = nil, destructive: Bool = false, enabled: Bool = true) {
        self.title = title
        self.icon = icon
        self.destructive = destructive
        self.enabled = enabled
    }

    private var titleColor: Color {
        guard enabled else { return .white.opacity(TextOpacity.quaternary) }
        return destructive ? DesignTokens.Palette.destructive : .white.opacity(GlassMenuTokens.textOpacity)
    }

    private var iconColor: Color {
        guard enabled else { return .white.opacity(TextOpacity.quaternary) }
        return destructive ? DesignTokens.Palette.destructive : .white.opacity(TextOpacity.secondary)
    }

    public var body: some View {
        HStack(spacing: 10) {
            if let icon {
                AppIcon(icon, size: 16)
                    .foregroundStyle(iconColor)
            }
            Text(title)
                .font(.subheadline)
                .foregroundStyle(titleColor)
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, GlassMenuTokens.itemHPadding)
        .frame(maxWidth: .infinity, minHeight: GlassMenuTokens.itemMinHeight, alignment: .leading)
        .contentShape(Rectangle())
    }
}

/// One row of a `GlassMenu`. Runs `action` and then closes the enclosing menu —
/// the system `Menu` did that for free, so call sites keep reading as a list of
/// actions rather than a list of actions plus dismissals.
public struct GlassMenuItem: View {
    let title: String
    var icon: String? = nil
    var destructive: Bool = false
    var enabled: Bool = true
    let action: () -> Void

    @Environment(\.glassMenuDismiss) private var dismissMenu

    public init(
        _ title: String,
        icon: String? = nil,
        destructive: Bool = false,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.icon = icon
        self.destructive = destructive
        self.enabled = enabled
        self.action = action
    }

    public var body: some View {
        Button {
            dismissMenu()
            action()
        } label: {
            GlassMenuItemLabel(title, icon: icon, destructive: destructive, enabled: enabled)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

/// Hairline between groups of menu items.
public struct GlassMenuDivider: View {
    public init() {}

    public var body: some View {
        Rectangle()
            .fill(Color.white.opacity(GlassMenuTokens.dividerOpacity))
            .frame(height: GlassMenuTokens.hairline)
    }
}

// MARK: - Dismissal environment

/// Reference box around the dismissal closure. A bare `() -> Void` cannot be an
/// `EnvironmentKey.defaultValue` under strict concurrency (a non-Sendable
/// static), and the closure it carries captures view state that is main-actor
/// bound by construction — the same shape SwiftUI's own `DismissAction` has.
final class GlassMenuDismissAction: @unchecked Sendable {
    private let handler: () -> Void

    init(_ handler: @escaping () -> Void) {
        self.handler = handler
    }

    func callAsFunction() {
        handler()
    }
}

private struct GlassMenuDismissKey: EnvironmentKey {
    static let defaultValue = GlassMenuDismissAction {}
}

extension EnvironmentValues {
    /// Closes the enclosing `GlassMenu`. Read by `GlassMenuItem`; a call site
    /// with a hand-rolled row can read it too.
    var glassMenuDismiss: GlassMenuDismissAction {
        get { self[GlassMenuDismissKey.self] }
        set { self[GlassMenuDismissKey.self] = newValue }
    }
}

// MARK: - Menu

/// Drop-in for `Menu(content:label:)` in the glass idiom: the label is a plain
/// button, and tapping it floats `content` from the label's trailing edge.
///
/// The popup rides a `.fullScreenCover` with a clear presentation background —
/// the only SwiftUI surface that reliably paints over a navigation bar and the
/// keyboard. Both the present and the dismiss run inside a transaction with
/// `disablesAnimations`, so the system's full-screen slide never shows; the
/// menu supplies its own fade + scale from the anchor corner instead.
public struct GlassMenu<Label: View, Content: View>: View {
    @ViewBuilder let content: () -> Content
    let label: Label

    @State private var anchor: CGRect = .zero
    @State private var isPresented = false

    public init(@ViewBuilder content: @escaping () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content
        self.label = label()
    }

    public var body: some View {
        Button {
            present()
        } label: {
            label
        }
        .buttonStyle(.plain)
        .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
            anchor = frame
        }
        .glassMenuOverlay(isPresented: $isPresented, anchor: anchor, content: content)
    }

    private func present() {
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) { isPresented = true }
    }
}

public extension View {
    /// The `GlassMenu` popup as a standalone modifier, for hosts that cannot
    /// own the trigger — a toolbar item whose presentation has to hang off the
    /// screen content instead. The caller captures the trigger's global frame
    /// (`.onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) })`)
    /// and flips `isPresented` inside a `disablesAnimations` transaction.
    func glassMenuOverlay<Content: View>(
        isPresented: Binding<Bool>,
        anchor: CGRect,
        @ViewBuilder content: @escaping () -> Content
    ) -> some View {
        modifier(GlassMenuOverlay(isPresented: isPresented, anchor: anchor, menuContent: content))
    }
}

private struct GlassMenuOverlay<MenuContent: View>: ViewModifier {
    @Binding var isPresented: Bool
    let anchor: CGRect
    @ViewBuilder let menuContent: () -> MenuContent

    /// Every write to the presentation flag — ours and the system's — is
    /// animation-free, so the cover never slides in or out.
    private var coverBinding: Binding<Bool> {
        Binding(
            get: { isPresented },
            set: { newValue in
                var transaction = Transaction()
                transaction.disablesAnimations = true
                withTransaction(transaction) { isPresented = newValue }
            }
        )
    }

    func body(content: Content) -> some View {
        content
            .fullScreenCover(isPresented: coverBinding) {
                GlassMenuPopup(anchor: anchor, dismiss: { coverBinding.wrappedValue = false }) {
                    menuContent()
                }
            }
    }
}

private struct GlassMenuPopup<Content: View>: View {
    let anchor: CGRect
    let dismiss: () -> Void
    @ViewBuilder let content: () -> Content

    @Environment(\.motion) private var motion
    @State private var menuSize: CGSize = .zero
    @State private var appeared = false

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .topLeading) {
                // No scrim (Android parity) — just a tap-through-proof layer
                // that closes the menu, exactly like a dropdown's outside tap.
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture { dismiss() }

                menu(in: proxy.size)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
        .ignoresSafeArea()
        .presentationBackground(.clear)
    }

    /// Below the anchor unless the measured menu would run off the bottom.
    private func opensBelow(in screen: CGSize) -> Bool {
        let below = anchor.maxY + GlassMenuTokens.anchorGap + menuSize.height
        return below + GlassMenuTokens.screenMargin <= screen.height || anchor.minY < menuSize.height
    }

    private func origin(in screen: CGSize) -> CGPoint {
        let width = max(menuSize.width, GlassMenuTokens.minWidth)
        let maxX = max(GlassMenuTokens.screenMargin, screen.width - width - GlassMenuTokens.screenMargin)
        let x = min(max(GlassMenuTokens.screenMargin, anchor.maxX - width), maxX)
        let y: CGFloat
        if opensBelow(in: screen) {
            y = anchor.maxY + GlassMenuTokens.anchorGap
        } else {
            y = max(
                GlassMenuTokens.screenMargin,
                anchor.minY - GlassMenuTokens.anchorGap - menuSize.height
            )
        }
        return CGPoint(x: x, y: y)
    }

    @ViewBuilder
    private func menu(in screen: CGSize) -> some View {
        let point = origin(in: screen)
        GlassMenuSurface {
            VStack(spacing: 0) {
                content()
            }
            .frame(
                minWidth: GlassMenuTokens.minWidth,
                maxWidth: GlassMenuTokens.maxWidth,
                alignment: .leading
            )
            .fixedSize(horizontal: true, vertical: true)
        }
        .environment(\.glassMenuDismiss, GlassMenuDismissAction(dismiss))
        .onGeometryChange(for: CGSize.self, of: { $0.size }) { size in
            guard size.height > 0 else { return }
            if menuSize != size { menuSize = size }
            if !appeared {
                withAnimation(motion.decelerate()) { appeared = true }
            }
        }
        // Until the menu has measured itself its position is a guess, so the
        // first frame is drawn transparent rather than in the wrong place.
        .opacity(appeared ? 1 : 0)
        .scaleEffect(
            appeared ? 1 : 0.92,
            anchor: opensBelow(in: screen) ? .topTrailing : .bottomTrailing
        )
        .offset(x: point.x, y: point.y)
    }
}
