import ExpCore
import SwiftUI

/// EXP-965: the tree CONNECTOR behind a nested row — the one drawing of
/// `TreeGuides` (ExpCore owns the rule, this owns the ink). A 1pt
/// `strokeStrong` line down the parent level's gutter, a rounded elbow into
/// the row's leading glyph, a tee where a sibling follows, and a straight
/// line for every ancestor level whose subtree carries on below.
///
/// Sized by whatever it backs: the gutters are measured from the row's
/// leading edge (`base` = the row's own content inset, so the vertical lands
/// on the PARENT row's leading glyph centre) and the elbow turns at the row's
/// vertical centre.
public struct TreeGuidesOverlay: View {
    private let guide: TreeGuide
    private let indent: CGFloat
    private let base: CGFloat

    /// The default `base`: every flat list row pads its content 12pt, and the
    /// fold chevron beside it is 14 wide — so a parent's glyph centre sits
    /// `12 + 7` in, exactly the first gutter's centre.
    public static let rowContentInset: CGFloat = 12

    public init(
        guide: TreeGuide,
        indent: CGFloat = TreeGuides.indentPerLevel,
        base: CGFloat = TreeGuidesOverlay.rowContentInset
    ) {
        self.guide = guide
        self.indent = indent
        self.base = base
    }

    public var body: some View {
        Canvas { context, size in
            guard !guide.isEmpty else { return }
            var path = Path()
            for level in guide.passThrough {
                let x = centre(of: level)
                path.move(to: CGPoint(x: x, y: 0))
                path.addLine(to: CGPoint(x: x, y: size.height))
            }
            if let level = guide.elbowAt {
                let x = centre(of: level)
                let middle = size.height / 2
                let radius = min(TreeGuides.elbowRadius, middle)
                path.move(to: CGPoint(x: x, y: 0))
                path.addLine(to: CGPoint(x: x, y: middle - radius))
                path.addQuadCurve(
                    to: CGPoint(x: x + radius, y: middle),
                    control: CGPoint(x: x, y: middle)
                )
                path.addLine(to: CGPoint(x: base + indent * CGFloat(level + 1), y: middle))
                // Not the last child: the vertical carries on to the next one.
                if guide.tee {
                    path.move(to: CGPoint(x: x, y: 0))
                    path.addLine(to: CGPoint(x: x, y: size.height))
                }
            }
            context.stroke(path, with: .color(GlassTokens.strokeStrong), lineWidth: 1)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    /// The gutter band for `level` runs `base + indent * level` →
    /// `+ indent`; its centre is where the parent's glyph sits.
    private func centre(of level: Int) -> CGFloat {
        base + indent * CGFloat(level) + indent / 2
    }
}

extension View {
    /// EXP-965: indent this row to its depth and draw its connector — the ONE
    /// way a nested list row is offset (every site used to type its own 14).
    public func treeGuides(
        _ guide: TreeGuide,
        base: CGFloat = TreeGuidesOverlay.rowContentInset
    ) -> some View {
        padding(.leading, CGFloat(guide.depth) * TreeGuides.indentPerLevel)
            .background(TreeGuidesOverlay(guide: guide, base: base))
    }
}
