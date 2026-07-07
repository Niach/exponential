import SwiftUI

/// The official multi-color Google "G" — SF Symbols has no Google mark, so the
/// four brand-colored segments are drawn from the bezier geometry of Google's
/// sign-in-branding SVG (48×48 viewBox). Vector, so it stays crisp at any
/// size; size it with `.frame(width:height:)`. Per the current Google identity
/// guidelines the plain multi-color G sits directly on dark buttons (no white
/// tile), which matches the login screen's glass buttons.
struct GoogleLogoMark: View {
    var body: some View {
        Canvas { context, size in
            let scale = min(size.width, size.height) / 48
            let transform = CGAffineTransform(scaleX: scale, y: scale)
            context.fill(Self.blue.applying(transform), with: .color(Self.googleBlue))
            context.fill(Self.green.applying(transform), with: .color(Self.googleGreen))
            context.fill(Self.yellow.applying(transform), with: .color(Self.googleYellow))
            context.fill(Self.red.applying(transform), with: .color(Self.googleRed))
        }
        .accessibilityHidden(true)
    }

    // Google brand colors: #4285F4, #34A853, #FBBC05, #EA4335.
    private static let googleBlue = Color(red: 66 / 255, green: 133 / 255, blue: 244 / 255)
    private static let googleGreen = Color(red: 52 / 255, green: 168 / 255, blue: 83 / 255)
    private static let googleYellow = Color(red: 251 / 255, green: 188 / 255, blue: 5 / 255)
    private static let googleRed = Color(red: 234 / 255, green: 67 / 255, blue: 53 / 255)

    // Right side of the G plus the crossbar.
    private static let blue: Path = {
        var p = Path()
        p.move(to: CGPoint(x: 46.98, y: 24.55))
        p.addCurve(to: CGPoint(x: 46.60, y: 20.00),
                   control1: CGPoint(x: 46.98, y: 22.98),
                   control2: CGPoint(x: 46.83, y: 21.46))
        p.addLine(to: CGPoint(x: 24.00, y: 20.00))
        p.addLine(to: CGPoint(x: 24.00, y: 29.02))
        p.addLine(to: CGPoint(x: 36.94, y: 29.02))
        p.addCurve(to: CGPoint(x: 32.16, y: 36.20),
                   control1: CGPoint(x: 36.36, y: 31.98),
                   control2: CGPoint(x: 34.68, y: 34.50))
        p.addLine(to: CGPoint(x: 39.89, y: 42.20))
        p.addCurve(to: CGPoint(x: 46.98, y: 24.55),
                   control1: CGPoint(x: 44.40, y: 38.02),
                   control2: CGPoint(x: 46.98, y: 31.84))
        p.closeSubpath()
        return p
    }()

    // Bottom arc.
    private static let green: Path = {
        var p = Path()
        p.move(to: CGPoint(x: 24.00, y: 48.00))
        p.addCurve(to: CGPoint(x: 39.89, y: 42.19),
                   control1: CGPoint(x: 30.48, y: 48.00),
                   control2: CGPoint(x: 35.93, y: 45.87))
        p.addLine(to: CGPoint(x: 32.16, y: 36.19))
        p.addCurve(to: CGPoint(x: 24.00, y: 38.49),
                   control1: CGPoint(x: 30.01, y: 37.64),
                   control2: CGPoint(x: 27.24, y: 38.49))
        p.addCurve(to: CGPoint(x: 10.53, y: 28.58),
                   control1: CGPoint(x: 17.74, y: 38.49),
                   control2: CGPoint(x: 12.43, y: 34.27))
        p.addLine(to: CGPoint(x: 2.55, y: 34.77))
        p.addCurve(to: CGPoint(x: 24.00, y: 48.00),
                   control1: CGPoint(x: 6.51, y: 42.62),
                   control2: CGPoint(x: 14.62, y: 48.00))
        p.closeSubpath()
        return p
    }()

    // Left arc.
    private static let yellow: Path = {
        var p = Path()
        p.move(to: CGPoint(x: 10.53, y: 28.59))
        p.addCurve(to: CGPoint(x: 9.77, y: 24.00),
                   control1: CGPoint(x: 10.05, y: 27.14),
                   control2: CGPoint(x: 9.77, y: 25.60))
        p.addCurve(to: CGPoint(x: 10.53, y: 19.41),
                   control1: CGPoint(x: 9.77, y: 22.40),
                   control2: CGPoint(x: 10.04, y: 20.86))
        p.addLine(to: CGPoint(x: 2.55, y: 13.22))
        p.addCurve(to: CGPoint(x: 0.00, y: 24.00),
                   control1: CGPoint(x: 0.92, y: 16.46),
                   control2: CGPoint(x: 0.00, y: 20.12))
        p.addCurve(to: CGPoint(x: 2.56, y: 34.78),
                   control1: CGPoint(x: 0.00, y: 27.88),
                   control2: CGPoint(x: 0.92, y: 31.54))
        p.addLine(to: CGPoint(x: 10.53, y: 28.59))
        p.closeSubpath()
        return p
    }()

    // Top arc.
    private static let red: Path = {
        var p = Path()
        p.move(to: CGPoint(x: 24.00, y: 9.50))
        p.addCurve(to: CGPoint(x: 33.21, y: 13.10),
                   control1: CGPoint(x: 27.54, y: 9.50),
                   control2: CGPoint(x: 30.71, y: 10.72))
        p.addLine(to: CGPoint(x: 40.06, y: 6.25))
        p.addCurve(to: CGPoint(x: 24.00, y: 0.00),
                   control1: CGPoint(x: 35.90, y: 2.38),
                   control2: CGPoint(x: 30.47, y: 0.00))
        p.addCurve(to: CGPoint(x: 2.56, y: 13.22),
                   control1: CGPoint(x: 14.62, y: 0.00),
                   control2: CGPoint(x: 6.51, y: 5.38))
        p.addLine(to: CGPoint(x: 10.54, y: 19.41))
        p.addCurve(to: CGPoint(x: 24.00, y: 9.50),
                   control1: CGPoint(x: 12.43, y: 13.72),
                   control2: CGPoint(x: 17.74, y: 9.50))
        p.closeSubpath()
        return p
    }()
}

#Preview {
    GoogleLogoMark()
        .frame(width: 96, height: 96)
        .padding()
        .background(Color.black)
}
