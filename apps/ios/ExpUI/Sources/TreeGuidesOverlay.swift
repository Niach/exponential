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
///
/// `gap` is the LIST's row spacing, the ×4 rule: every vertical that starts
/// at the row's top edge starts `gap` ABOVE it instead (drawn outside the row
/// bounds), while a tee or a pass-through still ends at the BOTTOM edge — the
/// next row's extension is what covers the gap. Without it a spaced list
/// draws a dashed branch, one break per row.
public struct TreeGuidesOverlay: View {
    private let guide: TreeGuide
    private let indent: CGFloat
    private let base: CGFloat
    private let gap: CGFloat

    /// The default `base`: every flat list row pads its content 12pt, and the
    /// fold chevron beside it is 14 wide — so a parent's glyph centre sits
    /// `12 + 7` in, exactly the first gutter's centre.
    public static let rowContentInset: CGFloat = 12

    public init(
        guide: TreeGuide,
        indent: CGFloat = TreeGuides.indentPerLevel,
        base: CGFloat = TreeGuidesOverlay.rowContentInset,
        gap: CGFloat = 0
    ) {
        self.guide = guide
        self.indent = indent
        self.base = base
        self.gap = gap
    }

    public var body: some View {
        // The canvas grows UP by the list's row spacing and is drawn outside
        // the row it backs, so the branch runs through the gap above it.
        GeometryReader { proxy in
            Canvas { context, size in
                draw(in: context, size: size)
            }
            .frame(height: proxy.size.height + gap)
            .offset(y: -gap)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    /// `size` spans the gap ABOVE the row plus the row itself: the row's top
    /// edge sits at `gap`, its bottom at `size.height`.
    private func draw(in context: GraphicsContext, size: CGSize) {
        guard !guide.isEmpty else { return }
        let middle = gap + (size.height - gap) / 2
        var path = Path()
        for level in guide.passThrough {
            let x = centre(of: level)
            path.move(to: CGPoint(x: x, y: 0))
            path.addLine(to: CGPoint(x: x, y: size.height))
        }
        if let level = guide.elbowAt {
            let x = centre(of: level)
            let radius = min(TreeGuides.elbowRadius, middle - gap)
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

    /// The gutter band for `level` runs `base + indent * level` →
    /// `+ indent`; its centre is where the parent's glyph sits.
    private func centre(of level: Int) -> CGFloat {
        base + indent * CGFloat(level) + indent / 2
    }
}

extension View {
    /// EXP-965: indent this row to its depth and draw its connector — the ONE
    /// way a nested list row is offset (every site used to type its own 14).
    /// `gap` is the list's own row spacing, which the branch runs through.
    public func treeGuides(
        _ guide: TreeGuide,
        base: CGFloat = TreeGuidesOverlay.rowContentInset,
        gap: CGFloat = 0
    ) -> some View {
        padding(.leading, CGFloat(guide.depth) * TreeGuides.indentPerLevel)
            .background(TreeGuidesOverlay(guide: guide, base: base, gap: gap))
    }
}
