import ExpUI
import UIKit

/// TextKit 1 layout manager drawing the block decorations that UITextView's
/// native per-line-fragment `.backgroundColor` painting cannot produce
/// (EXP-246, Linear parity):
///   - `.markdownCodeBlock` runs render as ONE connected rounded box spanning
///     every line of the fence (instead of a stripe per wrapped line).
///   - `.markdownBlockquote` runs get a vertical bar in the left gutter that
///     `MarkdownStyle.blockquoteParagraphStyle`'s head indent clears.
/// Installed by BlockTextEditor's explicit TextKit 1 stack — which also makes
/// the TextKit version deterministic (EditorTextView's tap handler touching
/// `layoutManager` already forced the TextKit 1 fallback, just lazily).
final class MarkdownLayoutManager: NSLayoutManager {
    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: CGPoint) {
        // super still paints `.backgroundColor` runs — inline code keeps its
        // per-run highlight; code BLOCKS no longer carry that attribute.
        super.drawBackground(forGlyphRange: glyphsToShow, at: origin)

        drawDecoration(for: .markdownCodeBlock, in: glyphsToShow, at: origin) { rect in
            let box = CGRect(
                x: rect.minX,
                y: rect.minY - 2,
                width: max(rect.width, self.blockWidth()),
                height: rect.height + 4
            )
            MarkdownStyle.codeBlockBackground.setFill()
            UIBezierPath(roundedRect: box, cornerRadius: 6).fill()
        }

        drawDecoration(for: .markdownBlockquote, in: glyphsToShow, at: origin) { rect in
            let bar = CGRect(x: rect.minX + 2, y: rect.minY, width: 3, height: rect.height)
            MarkdownStyle.blockquoteBarColor.setFill()
            UIBezierPath(roundedRect: bar, cornerRadius: 1.5).fill()
        }
    }

    /// Full usable text width, so code boxes span the container like the other
    /// clients' full-width blocks rather than hugging the longest line.
    private func blockWidth() -> CGFloat {
        guard let container = textContainers.first else { return 0 }
        return max(0, container.size.width - container.lineFragmentPadding * 2)
    }

    /// Calls `draw` once per maximal run of `key == true` intersecting the
    /// drawn glyph range, with the union of the run's line-fragment rects
    /// (offset by `origin`). Runs are extended via longestEffectiveRange so a
    /// partial redraw never truncates a box to the dirty rect's lines.
    private func drawDecoration(
        for key: NSAttributedString.Key,
        in glyphsToShow: NSRange,
        at origin: CGPoint,
        draw: (CGRect) -> Void
    ) {
        guard let storage = textStorage, storage.length > 0 else { return }
        let charRange = characterRange(forGlyphRange: glyphsToShow, actualGlyphRange: nil)
        let full = NSRange(location: 0, length: storage.length)
        var index = charRange.location
        let end = min(NSMaxRange(charRange), storage.length)
        while index < end {
            var effective = NSRange(location: 0, length: 0)
            let value = storage.attribute(key, at: index, longestEffectiveRange: &effective, in: full)
            if (value as? Bool) == true, effective.length > 0 {
                let runGlyphs = glyphRange(forCharacterRange: effective, actualCharacterRange: nil)
                var union = CGRect.null
                enumerateLineFragments(forGlyphRange: runGlyphs) { rect, _, _, _, _ in
                    union = union.union(rect)
                }
                if !union.isNull {
                    draw(union.offsetBy(dx: origin.x, dy: origin.y))
                }
            }
            index = max(NSMaxRange(effective), index + 1)
        }
    }
}
