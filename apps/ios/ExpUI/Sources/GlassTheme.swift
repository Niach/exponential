import SwiftUI

// MARK: - Zinc Color Palette (OKLCH-mapped)

public enum Zinc {
    public static let _50 = Color(red: 0.98, green: 0.98, blue: 0.99)
    public static let _100 = Color(red: 0.95, green: 0.95, blue: 0.96)
    public static let _200 = Color(red: 0.90, green: 0.90, blue: 0.92)
    public static let _300 = Color(red: 0.83, green: 0.83, blue: 0.85)
    public static let _400 = Color(red: 0.63, green: 0.63, blue: 0.67)
    public static let _500 = Color(red: 0.44, green: 0.44, blue: 0.48)
    public static let _600 = Color(red: 0.33, green: 0.33, blue: 0.36)
    public static let _700 = Color(red: 0.25, green: 0.25, blue: 0.27)
    public static let _800 = Color(red: 0.16, green: 0.16, blue: 0.18)
    public static let _900 = Color(red: 0.10, green: 0.10, blue: 0.11)
    public static let _950 = Color(red: 0.06, green: 0.06, blue: 0.07)
}

// MARK: - Glass Modifiers

public struct GlassCard: ViewModifier {
    public var cornerRadius: CGFloat = 16

    public init(cornerRadius: CGFloat = 16) {
        self.cornerRadius = cornerRadius
    }

    public func body(content: Content) -> some View {
        content
            .background(.ultraThinMaterial)
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius)
                    .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.3), radius: 20, y: 8)
    }
}

public struct GlassRow: ViewModifier {
    /// Brighter fill + stroke for a selected/primary row (Android
    /// `glassRow(active:)` parity — EXP-274 moved question options onto rows,
    /// whose fixed radius survives multi-line content where the capsule
    /// button clipped it).
    public var isActive: Bool = false

    public init(isActive: Bool = false) {
        self.isActive = isActive
    }

    public func body(content: Content) -> some View {
        content
            .background(isActive ? Color.white.opacity(0.15) : Color.clear)
            .background(.ultraThinMaterial.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(isActive ? 0.2 : 0.06), lineWidth: 0.5)
            )
    }
}

public struct GlassButton: ViewModifier {
    public var isActive: Bool = false
    /// Lays a solid card fill beneath the glass tint — for pills floating
    /// over scrolling content, where the low-alpha fill alone lets the
    /// content bleed through (Android glassButton `opaque` parity, EXP-165).
    public var isOpaque: Bool = false

    public init(isActive: Bool = false, isOpaque: Bool = false) {
        self.isActive = isActive
        self.isOpaque = isOpaque
    }

    public func body(content: Content) -> some View {
        content
            .background(isActive ? Color.white.opacity(0.15) : Color.white.opacity(0.06))
            .background(isOpaque ? DesignTokens.Palette.card : Color.clear)
            .clipShape(Capsule())
            .overlay(
                Capsule()
                    .stroke(Color.white.opacity(isActive ? 0.2 : 0.1), lineWidth: 0.5)
            )
    }
}

public struct GlassSection: ViewModifier {
    public init() {}

    public func body(content: Content) -> some View {
        content
            .background(.thinMaterial)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .stroke(Color.white.opacity(0.08), lineWidth: 0.5)
            )
    }
}

// MARK: - Background Gradient

public struct AppBackground: View {
    public init() {}

    public var body: some View {
        LinearGradient(
            colors: [Zinc._950, Zinc._900],
            startPoint: .top,
            endPoint: .bottom
        )
        .ignoresSafeArea()
    }
}

// MARK: - View Extensions

extension View {
    public func glassCard(cornerRadius: CGFloat = 16) -> some View {
        modifier(GlassCard(cornerRadius: cornerRadius))
    }

    public func glassRow(isActive: Bool = false) -> some View {
        modifier(GlassRow(isActive: isActive))
    }

    public func glassButton(isActive: Bool = false, isOpaque: Bool = false) -> some View {
        modifier(GlassButton(isActive: isActive, isOpaque: isOpaque))
    }

    public func glassSection() -> some View {
        modifier(GlassSection())
    }

    public func appBackground() -> some View {
        background { AppBackground() }
    }
}

// MARK: - Text Styles

public enum TextOpacity {
    public static let primary: Double = 1.0
    public static let secondary: Double = 0.7
    public static let tertiary: Double = 0.5
    public static let quaternary: Double = 0.3
}

// EXP-594: the indigo `Accent` enum is retired — the main scheme is white/
// glass. Solid accent fills that carry text use `DesignTokens.Palette.primary`
// (+ `primaryForeground` text), selections use the glass fills/strokes, and
// icon/dot accents are plain white.

// MARK: - Status Colors

// Semantic status/priority colors come from the shared design tokens
// (packages/design-tokens) so all clients stay in lockstep. The glass
// materials are deliberately NOT tokenized.
public enum StatusColor {
    public static let backlog = DesignTokens.Semantic.neutral
    /// Web's todo is `text-foreground` (near-white), not gray.
    public static let todo = Zinc._50
    public static let inProgress = DesignTokens.Semantic.yellow
    /// Up for review (EXP-120) — green, the color `done` used to carry.
    public static let inReview = DesignTokens.Semantic.green
    /// Completed (EXP-120): moved from green to blue when in_review took green.
    public static let done = DesignTokens.Semantic.blue
    /// Cancelled is a muted terminal RESOLUTION, not an error (REV2-85): the
    /// desktop/web treatment (`text-muted-foreground`) is the shared one, and
    /// red here made the same issue read as a failure on a phone and as a
    /// closed one on the desk.
    public static let cancelled = DesignTokens.Semantic.neutral
    /// Duplicate is a muted terminal resolution (like backlog's neutral gray).
    public static let duplicate = DesignTokens.Semantic.neutral
}

// MARK: - Priority Colors

public enum PriorityColor {
    public static let none = DesignTokens.Semantic.neutral
    public static let low = DesignTokens.Semantic.blue
    public static let medium = DesignTokens.Semantic.yellow
    public static let high = DesignTokens.Semantic.orange
    public static let urgent = DesignTokens.Semantic.red
}
