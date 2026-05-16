import SwiftUI

// MARK: - Zinc Color Palette (OKLCH-mapped)

enum Zinc {
    static let _50 = Color(red: 0.98, green: 0.98, blue: 0.99)
    static let _100 = Color(red: 0.95, green: 0.95, blue: 0.96)
    static let _200 = Color(red: 0.90, green: 0.90, blue: 0.92)
    static let _300 = Color(red: 0.83, green: 0.83, blue: 0.85)
    static let _400 = Color(red: 0.63, green: 0.63, blue: 0.67)
    static let _500 = Color(red: 0.44, green: 0.44, blue: 0.48)
    static let _600 = Color(red: 0.33, green: 0.33, blue: 0.36)
    static let _700 = Color(red: 0.25, green: 0.25, blue: 0.27)
    static let _800 = Color(red: 0.16, green: 0.16, blue: 0.18)
    static let _900 = Color(red: 0.10, green: 0.10, blue: 0.11)
    static let _950 = Color(red: 0.06, green: 0.06, blue: 0.07)
}

// MARK: - Glass Modifiers

struct GlassCard: ViewModifier {
    var cornerRadius: CGFloat = 16

    func body(content: Content) -> some View {
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

struct GlassRow: ViewModifier {
    func body(content: Content) -> some View {
        content
            .background(.ultraThinMaterial.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(0.06), lineWidth: 0.5)
            )
    }
}

struct GlassButton: ViewModifier {
    var isActive: Bool = false

    func body(content: Content) -> some View {
        content
            .background(isActive ? Color.white.opacity(0.15) : Color.white.opacity(0.06))
            .clipShape(Capsule())
            .overlay(
                Capsule()
                    .stroke(Color.white.opacity(isActive ? 0.2 : 0.1), lineWidth: 0.5)
            )
    }
}

struct GlassSection: ViewModifier {
    func body(content: Content) -> some View {
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

struct AppBackground: View {
    var body: some View {
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
    func glassCard(cornerRadius: CGFloat = 16) -> some View {
        modifier(GlassCard(cornerRadius: cornerRadius))
    }

    func glassRow() -> some View {
        modifier(GlassRow())
    }

    func glassButton(isActive: Bool = false) -> some View {
        modifier(GlassButton(isActive: isActive))
    }

    func glassSection() -> some View {
        modifier(GlassSection())
    }

    func appBackground() -> some View {
        background { AppBackground() }
    }
}

// MARK: - Text Styles

enum TextOpacity {
    static let primary: Double = 1.0
    static let secondary: Double = 0.7
    static let tertiary: Double = 0.5
    static let quaternary: Double = 0.3
}

// MARK: - Status Colors

enum StatusColor {
    static let backlog = Color.gray
    static let todo = Color.gray
    static let inProgress = Color.yellow
    static let done = Color.green
    static let cancelled = Color.red
}

// MARK: - Priority Colors

enum PriorityColor {
    static let none = Color.gray
    static let low = Color.blue
    static let medium = Color.yellow
    static let high = Color.orange
    static let urgent = Color.red
}
