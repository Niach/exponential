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

        drawChipCapsules(in: glyphsToShow, at: origin)
    }

    /// Bordered capsule behind every `.markdownChip` run — the `#IDENTIFIER`
    /// token with its display-only title attachment, and resolved `@email`
    /// mentions (EXP-322, web `.issue-ref-pill` parity). Chips deliberately
    /// carry no `.backgroundColor`, which `super` would paint as a square box
    /// per line fragment.
    ///
    /// The rects come per LINE FRAGMENT (the `drawDecoration` pattern), not from
    /// `enumerateEnclosingRects`: that returns SELECTION geometry, whose
    /// non-final lines run all the way to the container edge — a wrapped chip
    /// drew a full-width bar instead of a pill (EXP-423).
    ///
    /// A chip that wraps is ONE chip sliced at the line break, not two closed
    /// pills (EXP-713): the fragment ending a line keeps its right edge open
    /// and the continuation its left edge, the way CSS `box-decoration-break:
    /// slice` renders web's wrapped `.issue-ref-pill`. The per-run character
    /// range is extended to the whole run first, so a partial redraw of only
    /// the continuation line still knows it is a continuation.
    private func drawChipCapsules(in glyphsToShow: NSRange, at origin: CGPoint) {
        guard let storage = textStorage, storage.length > 0,
              let container = textContainers.first else { return }
        let charRange = characterRange(forGlyphRange: glyphsToShow, actualGlyphRange: nil)
        guard charRange.length > 0 else { return }
        let full = NSRange(location: 0, length: storage.length)
        var drawn = [NSRange]()
        storage.enumerateAttribute(.markdownChip, in: charRange, options: []) { value, partial, _ in
            guard value != nil else { return }
            var range = NSRange(location: 0, length: 0)
            _ = storage.attribute(.markdownChip, at: partial.location, longestEffectiveRange: &range, in: full)
            guard range.length > 0, !drawn.contains(where: { NSEqualRanges($0, range) }) else { return }
            drawn.append(range)
            let glyphs = self.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            guard glyphs.length > 0 else { return }
            // Issue chips are rounded rects (Linear); mention pills stay round.
            let isIssueRef = storage.attribute(
                .markdownIssueRef, at: range.location, effectiveRange: nil) != nil
            var boxes = [CGRect]()
            self.enumerateLineFragments(forGlyphRange: glyphs) { _, _, _, lineGlyphs, _ in
                let intersect = NSIntersectionRange(lineGlyphs, glyphs)
                guard intersect.length > 0 else { return }
                let rect = self.boundingRect(forGlyphRange: intersect, in: container)
                let box = rect.offsetBy(dx: origin.x, dy: origin.y).insetBy(dx: -2, dy: 1)
                guard box.width > 0, box.height > 0 else { return }
                boxes.append(box)
            }
            for (index, box) in boxes.enumerated() {
                let radius = isIssueRef ? MarkdownStyle.chipCornerRadius : box.height / 2
                Self.drawChipBox(
                    box,
                    radius: radius,
                    openLeft: index > 0,
                    openRight: index < boxes.count - 1
                )
            }
            self.drawChipStatusIcons(in: range, storage: storage, container: container, at: origin)
        }
    }

    /// Fill + hairline of one chip fragment. An open side is drawn by
    /// extending the rounded rect past that edge and clipping it back to the
    /// fragment, so the corners and the stroke simply never appear there.
    private static func drawChipBox(_ box: CGRect, radius: CGFloat, openLeft: Bool, openRight: Bool) {
        let lineWidth: CGFloat = 1
        let overshoot = radius + lineWidth
        var shape = box
        // Half the stroke straddles the box edge; keep it on the closed sides.
        var clip = box.insetBy(dx: -lineWidth, dy: -lineWidth)
        if openLeft {
            shape.origin.x -= overshoot
            shape.size.width += overshoot
            clip.origin.x += lineWidth
            clip.size.width -= lineWidth
        }
        if openRight {
            shape.size.width += overshoot
            clip.size.width -= lineWidth
        }
        let context = UIGraphicsGetCurrentContext()
        context?.saveGState()
        if openLeft || openRight { UIBezierPath(rect: clip).addClip() }
        let path = UIBezierPath(roundedRect: shape, cornerRadius: radius)
        MarkdownStyle.chipBackground.setFill()
        path.fill()
        MarkdownStyle.chipBorder.setStroke()
        path.lineWidth = lineWidth
        path.stroke()
        context?.restoreGState()
    }

    /// The status glyph of a resolved issue chip, painted over the token's `#`
    /// cell in the status color (EXP-423, Linear parity). `IssueRefs.decorate`
    /// gave that one character a CLEAR foreground, so the icon replaces it
    /// without adding, removing or moving a single character — the serializer,
    /// caret math and `chipAtomRange` never see this. Web and the desktop editor
    /// deliberately keep their `#` visible next to the icon instead (edit
    /// affordance + offset-map invariants there).
    private func drawChipStatusIcons(
        in charRange: NSRange,
        storage: NSTextStorage,
        container: NSTextContainer,
        at origin: CGPoint
    ) {
        storage.enumerateAttribute(.markdownIssueRefStatus, in: charRange, options: []) { value, range, _ in
            guard let info = value as? IssueRefStatusInfo, range.length > 0,
                  let image = Self.statusIcon(info) else { return }
            // Extend to the whole run: a partial redraw can hand this method a
            // range that starts after the `#`, and the icon belongs to that one
            // cell (same reasoning as `drawDecoration`).
            var run = NSRange(location: 0, length: 0)
            _ = storage.attribute(
                .markdownIssueRefStatus,
                at: range.location,
                longestEffectiveRange: &run,
                in: NSRange(location: 0, length: storage.length)
            )
            let hashGlyphs = self.glyphRange(
                forCharacterRange: NSRange(location: run.location, length: 1),
                actualCharacterRange: nil
            )
            guard hashGlyphs.length > 0 else { return }
            let cell = self.boundingRect(forGlyphRange: hashGlyphs, in: container)
                .offsetBy(dx: origin.x, dy: origin.y)
            // Center on the TEXT baseline, not on the bounding box: the box
            // carries the paragraph's line spacing, which would push the glyph
            // low. Cap height is the visual center of the surrounding text.
            let font = storage.attribute(.font, at: run.location, effectiveRange: nil)
                as? PlatformFont ?? MarkdownStyle.bodyFont
            let lineRect = self.lineFragmentRect(forGlyphAt: hashGlyphs.location, effectiveRange: nil)
            let baseline = lineRect.origin.y + origin.y
                + self.location(forGlyphAt: hashGlyphs.location).y
            let centerY = baseline - font.capHeight / 2
            let side = Self.statusIconSize
            // Left-anchored, not centred: the `#` cell is kerned wider than the
            // glyph (EXP-655), and that surplus to the RIGHT of the icon is the
            // gap that keeps it off the identifier. The -1 keeps the art inside
            // the capsule's 2pt inset.
            image.draw(in: CGRect(
                x: cell.minX - 1,
                y: centerY - side / 2,
                width: side,
                height: side
            ))
        }
    }

    /// Point size of the chip status glyph. Slightly wider than a `#` in the
    /// body font, which the Lucide art's own 24-grid padding absorbs.
    private static let statusIconSize: CGFloat = 13

    /// The layout manager asks for this on every draw pass, so memoize the
    /// tinted rendition per (name, size, color) — the `IssueRefTitleAttachment`
    /// cache precedent.
    private final class IconKey: NSObject {
        let name: String
        let size: CGFloat
        let color: PlatformColor
        init(name: String, size: CGFloat, color: PlatformColor) {
            self.name = name
            self.size = size
            self.color = color
        }
        override func isEqual(_ object: Any?) -> Bool {
            guard let other = object as? IconKey else { return false }
            return other.name == name && other.size == size && other.color == color
        }
        override var hash: Int {
            var hasher = Hasher()
            hasher.combine(name)
            hasher.combine(size)
            hasher.combine(color)
            return hasher.finalize()
        }
    }

    private nonisolated(unsafe) static let iconCache: NSCache<IconKey, UIImage> = {
        let cache = NSCache<IconKey, UIImage>()
        cache.countLimit = 64
        return cache
    }()

    private static func statusIcon(_ info: IssueRefStatusInfo) -> UIImage? {
        let key = IconKey(name: info.iconName, size: statusIconSize, color: info.color)
        if let cached = iconCache.object(forKey: key) { return cached }
        guard let base = AppIcons.uiImage(info.iconName, pointSize: statusIconSize) else { return nil }
        let tinted = base.withTintColor(info.color, renderingMode: .alwaysOriginal)
        iconCache.setObject(tinted, forKey: key)
        return tinted
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
