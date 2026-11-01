import SwiftUI

/// Wrapping chip layout shared by the property chip box, the label pickers,
/// the create sheet, and team settings (moved out of IssueDetailView, EXP-240).
/// A subview wider than the line is offered the line's width (so a chip's
/// title truncates instead of running off the edge); everything else keeps
/// its ideal size.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8
    /// The gap between wrapped lines; nil = `spacing` (web `gap-x-* gap-y-*`).
    var lineSpacing: CGFloat?
    /// Centre each subview on its line (web `items-center`), else top-align.
    var centersLines = false

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let result = arrange(proposal: proposal, subviews: subviews)
        return result.size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = arrange(proposal: proposal, subviews: subviews)
        for (index, position) in result.positions.enumerated() {
            subviews[index].place(
                at: CGPoint(x: bounds.minX + position.x, y: bounds.minY + position.y),
                proposal: result.proposals[index]
            )
        }
    }

    private func arrange(
        proposal: ProposedViewSize,
        subviews: Subviews
    ) -> (positions: [CGPoint], proposals: [ProposedViewSize], size: CGSize) {
        let maxWidth = proposal.width ?? .infinity
        let lineGap = lineSpacing ?? spacing
        var positions: [CGPoint] = []
        var proposals: [ProposedViewSize] = []
        var heights: [CGFloat] = []
        var lineStarts: [Int] = [0]
        var lineHeights: [CGFloat] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        var maxX: CGFloat = 0

        for subview in subviews {
            var offer = ProposedViewSize.unspecified
            var size = subview.sizeThatFits(offer)
            if size.width > maxWidth {
                offer = ProposedViewSize(width: maxWidth, height: nil)
                size = subview.sizeThatFits(offer)
            }
            if x + size.width > maxWidth && x > 0 {
                lineStarts.append(positions.count)
                lineHeights.append(rowHeight)
                x = 0
                y += rowHeight + lineGap
                rowHeight = 0
            }
            positions.append(CGPoint(x: x, y: y))
            proposals.append(offer)
            heights.append(size.height)
            rowHeight = max(rowHeight, size.height)
            x += size.width + spacing
            maxX = max(maxX, x)
        }
        lineHeights.append(rowHeight)

        if centersLines {
            for (line, start) in lineStarts.enumerated() {
                let end = line + 1 < lineStarts.count ? lineStarts[line + 1] : positions.count
                for index in start..<end {
                    positions[index].y += (lineHeights[line] - heights[index]) / 2
                }
            }
        }

        return (positions, proposals, CGSize(width: maxX, height: y + rowHeight))
    }
}
