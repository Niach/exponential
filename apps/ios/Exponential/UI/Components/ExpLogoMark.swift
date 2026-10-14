import ExpCore
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

    /// The three swept curves of `logo.svg` (they start and end outside the
    /// grid; the disc is what clips them). EXP-1259 D8: the ONE copy of that
    /// geometry is `FeedbackStates.Mark.stripes` (ExpCore), which the
    /// feedback mark (`FeedbackMark`) sweeps; the brand mark draws it whole.
    private static let stripes: [Path] = FeedbackStates.Mark.stripes.map { stripe in
        let p = stripe.cubic
        var path = Path()
        path.move(to: CGPoint(x: p[0].x, y: p[0].y))
        path.addCurve(
            to: CGPoint(x: p[3].x, y: p[3].y),
            control1: CGPoint(x: p[1].x, y: p[1].y),
            control2: CGPoint(x: p[2].x, y: p[2].y)
        )
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

/// FEED-100: Home's DAWN — the brand mark drawn BIG and faint, its upper
/// HALF rising over the composer like a sun over the horizon, `gap` off the
/// card (web's phone `HomeDawn`: 160px, opacity 0.05, 24px). Replaces the
/// small mark + greeting headline; decoration only.
struct HomeDawn: View {
    /// The mark's diameter; the dawn shows its upper half.
    static let size: CGFloat = 160
    static let opacity: Double = 0.05
    /// The space between the dawn and the composer card.
    static let gap: CGFloat = 24

    var body: some View {
        ExpLogoMark(size: Self.size, color: .white.opacity(Self.opacity))
            .frame(height: Self.size / 2, alignment: .top)
            .clipped()
            .frame(maxWidth: .infinity)
            .accessibilityHidden(true)
            .accessibilityIdentifier(HomeGreeting.TestId.mark)
    }
}
