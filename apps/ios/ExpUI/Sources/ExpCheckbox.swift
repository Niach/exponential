import SwiftUI
import UIKit

/// THE checkbox ×4: every check / selection indicator on iOS (bulk-select
/// rows, chat selection, ask-card and plan-card multi-select, markdown task
/// items) is this one square, drawn to the web `CHECKBOX_CLASSES` look
/// (`packages/exponential-ui/src/web/primitives/checkbox.ts`) and the
/// Exponential UI Checkbox `box` geometry (`control-geometry.json`: 16×16,
/// 1pt border, 4pt corner):
/// - unchecked: the glass ROW fill under the STRONG glass stroke,
/// - checked / indeterminate: primary fill + border, the `ui-check` /
///   `ui-minus` glyph at 14pt in primary-foreground, centred,
/// - disabled: 50% opacity.
/// No round check, ring or disc anywhere.
public struct ExpCheckbox: View {
    public enum State: Equatable, Sendable {
        case unchecked, checked, indeterminate

        public init(_ checked: Bool) { self = checked ? .checked : .unchecked }
    }

    public enum Metrics {
        public static let side: CGFloat = 16
        public static let corner: CGFloat = 4
        public static let border: CGFloat = 1
        public static let glyph: CGFloat = 14
        public static let disabledOpacity: Double = 0.5
    }

    public enum Colors {
        public static let uncheckedFill: Color = GlassTokens.fillRow
        public static let uncheckedBorder: Color = GlassTokens.strokeStrong
        public static let checkedFill: Color = DesignTokens.Palette.primary
        public static let glyph: Color = DesignTokens.Palette.primaryForeground
    }

    private let state: State
    private let disabled: Bool

    public init(_ state: State, disabled: Bool = false) {
        self.state = state
        self.disabled = disabled
    }

    public init(checked: Bool, disabled: Bool = false) {
        self.init(State(checked), disabled: disabled)
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous)
        let on = state != .unchecked
        ZStack {
            shape.fill(on ? Colors.checkedFill : Colors.uncheckedFill)
            shape.strokeBorder(on ? Colors.checkedFill : Colors.uncheckedBorder, lineWidth: Metrics.border)
            if let glyph = Self.glyphName(state) {
                Self.glyphImage(glyph)
                    .foregroundStyle(Colors.glyph)
            }
        }
        // A fixed 16pt box (no Dynamic Type scaling): the same square ×4.
        .frame(width: Metrics.side, height: Metrics.side)
        .opacity(disabled ? Metrics.disabledOpacity : 1)
        .accessibilityHidden(true)
    }

    @ViewBuilder
    private static func glyphImage(_ name: String) -> some View {
        if let asset = AppIcons.assetName(name) {
            Image(asset)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: Metrics.glyph, height: Metrics.glyph)
        }
    }

    static func glyphName(_ state: State) -> String? {
        switch state {
        case .unchecked: nil
        case .checked: AppIcons.uiCheck
        case .indeterminate: AppIcons.uiMinus
        }
    }

    // MARK: - UIKit drawing (markdown task items)

    /// Draws the same box into the current graphics context — the markdown
    /// layout manager paints task items with it, so a `- [ ]` reads exactly
    /// like every other checkbox.
    public static func draw(_ state: State, in rect: CGRect) {
        let on = state != .unchecked
        let inset = rect.insetBy(dx: Metrics.border / 2, dy: Metrics.border / 2)
        let path = UIBezierPath(roundedRect: inset, cornerRadius: Metrics.corner - Metrics.border / 2)
        UIColor(on ? Colors.checkedFill : Colors.uncheckedFill).setFill()
        path.fill()
        UIColor(on ? Colors.checkedFill : Colors.uncheckedBorder).setStroke()
        path.lineWidth = Metrics.border
        path.stroke()
        guard let name = glyphName(state), let image = glyphUIImage(name) else { return }
        let side = Metrics.glyph * rect.width / Metrics.side
        image.draw(in: CGRect(x: rect.midX - side / 2, y: rect.midY - side / 2, width: side, height: side))
    }

    private nonisolated(unsafe) static let glyphCache = NSCache<NSString, UIImage>()

    private static func glyphUIImage(_ name: String) -> UIImage? {
        if let cached = glyphCache.object(forKey: name as NSString) { return cached }
        guard let base = AppIcons.uiImage(name, pointSize: Metrics.glyph) else { return nil }
        let tinted = base.withTintColor(UIColor(Colors.glyph), renderingMode: .alwaysOriginal)
        glyphCache.setObject(tinted, forKey: name as NSString)
        return tinted
    }
}
