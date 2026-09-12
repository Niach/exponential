import SwiftUI

/// EXP-846 — Exponential's OWN brand mark: a filled disc with three swept
/// stripes cut out of it.
///
/// Drawn from the same bezier geometry as the shipped logo
/// (`apps/desktop/assets/icons/logo.svg`, a 100×100 viewBox: a full-bleed
/// circle masked by three 3.5-wide curves), the way `GoogleLogoMark` draws
/// Google's G — a mask-and-clipPath SVG is exactly the kind Xcode's asset
/// catalog renders unpredictably, and a vector drawn here stays crisp at any
/// size and takes the caller's tint.
///
/// Where it is used: an Exponential MCP tool row in a session transcript
/// (EXP-846) — "Created issue" wears OUR mark, not the generic tool glyph, so
/// an agent touching the tracker reads apart from an agent touching the repo.
/// Size it with `size:`; the tint defaults to white like every other
/// transcript glyph.
struct ExpLogoMark: View {
    var size: CGFloat = 12
    var color: Color = .white

    /// The logo's design grid — the svg's viewBox.
    private static let grid: CGFloat = 100
    /// The svg's `stroke-width` on that grid.
    private static let stripeWidth: CGFloat = 3.5

    var body: some View {
        Canvas { context, canvasSize in
            let scale = min(canvasSize.width, canvasSize.height) / Self.grid
            let transform = CGAffineTransform(scaleX: scale, y: scale)
            context.fill(
                Path(ellipseIn: CGRect(x: 0, y: 0, width: Self.grid, height: Self.grid))
                    .applying(transform),
                with: .color(color)
            )
            // The svg's `mask`: the stripes are CUT OUT of the disc, so the
            // page shows through them. `destinationOut` is that mask — the
            // colour is irrelevant, only the coverage is.
            context.blendMode = .destinationOut
            for stripe in Self.stripes {
                context.stroke(
                    stripe.applying(transform),
                    with: .color(.black),
                    lineWidth: Self.stripeWidth * scale
                )
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    /// The three swept curves, verbatim from `logo.svg` (they start and end
    /// outside the grid — the disc is what clips them).
    private static let stripes: [Path] = [
        curve(
            from: CGPoint(x: -5.87, y: 62.01),
            control1: CGPoint(x: 39.09, y: 65.44),
            control2: CGPoint(x: 48.72, y: 28.71),
            to: CGPoint(x: 49.03, y: -6.21)
        ),
        curve(
            from: CGPoint(x: -5.07, y: 86.00),
            control1: CGPoint(x: 53.78, y: 84.42),
            control2: CGPoint(x: 71.13, y: 37.29),
            to: CGPoint(x: 73.00, y: -5.09)
        ),
        curve(
            from: CGPoint(x: -4.27, y: 109.99),
            control1: CGPoint(x: 68.46, y: 103.40),
            control2: CGPoint(x: 93.55, y: 45.86),
            to: CGPoint(x: 96.98, y: -3.98)
        ),
    ]

    private static func curve(
        from start: CGPoint, control1: CGPoint, control2: CGPoint, to end: CGPoint
    ) -> Path {
        var path = Path()
        path.move(to: start)
        path.addCurve(to: end, control1: control1, control2: control2)
        return path
    }
}

#Preview {
    HStack(spacing: 12) {
        ExpLogoMark(size: 12)
        ExpLogoMark(size: 24)
        ExpLogoMark(size: 96)
    }
    .padding()
    .background(Color.black)
}
