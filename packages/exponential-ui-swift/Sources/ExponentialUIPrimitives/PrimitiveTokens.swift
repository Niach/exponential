import SwiftUI

/// The colours and sizes the generic primitives paint with. The painter
/// fills one from the surface's resolved theme; an embedding app fills one
/// from its own tokens (the Exponential app's `ExpUI` does, SLOP-18). Every
/// primitive reads it from the environment (`\.primitiveTokens`), so one
/// value themes a whole subtree.
public struct PrimitiveTokens: Sendable, Equatable {
    public var foreground: Color
    public var mutedForeground: Color
    public var background: Color
    public var card: Color
    public var muted: Color
    public var border: Color
    public var input: Color
    public var primary: Color
    public var primaryForeground: Color
    public var accent: Color
    public var destructive: Color
    public var success: Color
    public var warning: Color
    public var info: Color
    public var ring: Color
    /// The theme's hairline width (`control.hairline`).
    public var hairline: CGFloat
    /// The pill (`control.pill`), input (`control.input`) and row heights.
    public var pillHeight: CGFloat
    public var inputHeight: CGFloat
    public var rowHeight: CGFloat
    public var radiusSm: CGFloat
    public var radiusMd: CGFloat
    public var radiusLg: CGFloat
    /// The sans and mono families by NAME (nil = the system font).
    public var sansFamily: String?
    public var monoFamily: String?

    public init(
        foreground: Color = .primary,
        mutedForeground: Color = .secondary,
        background: Color = .clear,
        card: Color = .clear,
        muted: Color = Color.gray.opacity(0.15),
        border: Color = Color.gray.opacity(0.3),
        input: Color = Color.gray.opacity(0.25),
        primary: Color = .accentColor,
        primaryForeground: Color = .white,
        accent: Color = Color.gray.opacity(0.2),
        destructive: Color = .red,
        success: Color = .green,
        warning: Color = .orange,
        info: Color = .blue,
        ring: Color = .accentColor,
        hairline: CGFloat = 1,
        pillHeight: CGFloat = 24,
        inputHeight: CGFloat = 36,
        rowHeight: CGFloat = 32,
        radiusSm: CGFloat = 4,
        radiusMd: CGFloat = 6,
        radiusLg: CGFloat = 8,
        sansFamily: String? = nil,
        monoFamily: String? = nil
    ) {
        self.foreground = foreground
        self.mutedForeground = mutedForeground
        self.background = background
        self.card = card
        self.muted = muted
        self.border = border
        self.input = input
        self.primary = primary
        self.primaryForeground = primaryForeground
        self.accent = accent
        self.destructive = destructive
        self.success = success
        self.warning = warning
        self.info = info
        self.ring = ring
        self.hairline = hairline
        self.pillHeight = pillHeight
        self.inputHeight = inputHeight
        self.rowHeight = rowHeight
        self.radiusSm = radiusSm
        self.radiusMd = radiusMd
        self.radiusLg = radiusLg
        self.sansFamily = sansFamily
        self.monoFamily = monoFamily
    }

    /// Tokens that read as the system's own look (the default).
    public static let system = PrimitiveTokens()
}

private struct PrimitiveTokensKey: EnvironmentKey {
    static let defaultValue = PrimitiveTokens.system
}

public extension EnvironmentValues {
    var primitiveTokens: PrimitiveTokens {
        get { self[PrimitiveTokensKey.self] }
        set { self[PrimitiveTokensKey.self] = newValue }
    }
}

public extension View {
    /// Theme every primitive below with `tokens`.
    func primitiveTokens(_ tokens: PrimitiveTokens) -> some View {
        environment(\.primitiveTokens, tokens)
    }
}

/// The semantic tone a primitive may be tinted with.
public enum PrimitiveTone: String, Sendable, CaseIterable {
    case neutral, primary, success, warning, danger, info

    /// The tone's colour in `tokens`.
    public func color(_ tokens: PrimitiveTokens) -> Color {
        switch self {
        case .neutral: tokens.mutedForeground
        case .primary: tokens.primary
        case .success: tokens.success
        case .warning: tokens.warning
        case .danger: tokens.destructive
        case .info: tokens.info
        }
    }
}
