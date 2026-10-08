import SwiftUI

#if canImport(UIKit)
import UIKit

/// A TextKit label: the same engine the measurer shaped the text with, so
/// a measured height is the painted height (line heights pinned by the
/// paragraph style). `lines` truncates with an ellipsis. The inherited text
/// keys (`xuiTextPaint`: tracking, decoration, transform, italic) and the
/// surface direction (`xuiRTL`) shape the runs here.
struct TextLabel: UIViewRepresentable {
    let attributed: NSAttributedString
    var lineHeight: CGFloat
    var lines: Int? = nil
    var wraps: Bool = true

    final class Label: UILabel {
        var halfLeading: CGFloat = 0
        var rtl = false

        override func drawText(in rect: CGRect) {
            // TextKit sits the glyphs at the BOTTOM of a pinned line (the
            // extra height goes above the ascender); CSS centres them
            // (half-leading). Shift the text up by half the leading.
            let scale = window?.screen.scale ?? traitCollection.displayScale
            let box = TextLabel.paintRect(rect, alignment: textAlignment, rtl: rtl, scale: scale)
            super.drawText(in: box.offsetBy(dx: 0, dy: -halfLeading))
        }
    }

    func makeUIView(context: Context) -> Label {
        let label = Label()
        label.backgroundColor = .clear
        label.isUserInteractionEnabled = false
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        label.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return label
    }

    func updateUIView(_ label: Label, context: Context) {
        let rtl = context.environment.xuiRTL
        let shaped = TextLabel.shaped(attributed, context.environment.xuiTextPaint, rtl: rtl)
        label.attributedText = shaped
        label.semanticContentAttribute = rtl ? .forceRightToLeft : .forceLeftToRight
        label.rtl = rtl
        label.numberOfLines = wraps ? (lines ?? 0) : 1
        label.lineBreakMode = wraps ? ((lines ?? 0) > 0 ? .byTruncatingTail : .byWordWrapping) : .byClipping
        label.halfLeading = shaped.length > 0 ? (shaped.attribute(.init("ExponentialUI.halfLeading"), at: 0, effectiveRange: nil) as? CGFloat ?? 0) : 0
        label.textAlignment = (shaped.length > 0 ? (shaped.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.alignment : nil) ?? .natural
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: Label, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? uiView.intrinsicContentSize.width, height: proposal.height ?? lineHeight)
    }
}

#elseif canImport(AppKit)
import AppKit

struct TextLabel: NSViewRepresentable {
    let attributed: NSAttributedString
    var lineHeight: CGFloat
    var lines: Int? = nil
    var wraps: Bool = true

    final class Label: NSTextField {
        var halfLeading: CGFloat = 0
        override var isFlipped: Bool { true }
        override class var cellClass: AnyClass? {
            get { Cell.self }
            set {}
        }
    }

    /// Draws in `TextLabel.paintRect` (see there).
    final class Cell: NSTextFieldCell {
        override func drawingRect(forBounds rect: NSRect) -> NSRect {
            let inner = super.drawingRect(forBounds: rect)
            let scale = controlView?.window?.backingScaleFactor ?? 2
            let rtl = (controlView as? NSTextField)?.baseWritingDirection == .rightToLeft
            return TextLabel.paintRect(inner, alignment: alignment, rtl: rtl, scale: scale)
        }
    }

    func makeNSView(context: Context) -> Label {
        let label = Label(labelWithAttributedString: attributed)
        label.isEditable = false
        label.isSelectable = false
        label.isBordered = false
        label.drawsBackground = false
        label.cell?.wraps = true
        label.cell?.isScrollable = false
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        return label
    }

    func updateNSView(_ label: Label, context: Context) {
        let paint = context.environment.xuiTextPaint
        let rtl = context.environment.xuiRTL
        // TextKit sits the glyphs at the BOTTOM of a pinned line; CSS
        // centres them. A baseline offset of half the leading does it.
        let shifted = NSMutableAttributedString(attributedString: TextLabel.shaped(attributed, paint, rtl: rtl))
        if shifted.length > 0, let half = shifted.attribute(.init("ExponentialUI.halfLeading"), at: 0, effectiveRange: nil) as? CGFloat, half > 0 {
            shifted.addAttribute(.baselineOffset, value: half, range: NSRange(location: 0, length: shifted.length))
        }
        label.attributedStringValue = shifted
        label.maximumNumberOfLines = wraps ? (lines ?? 0) : 1
        label.lineBreakMode = wraps ? ((lines ?? 0) > 0 ? .byTruncatingTail : .byWordWrapping) : .byClipping
        label.cell?.wraps = wraps
        label.baseWritingDirection = rtl ? .rightToLeft : .leftToRight
        label.alignment = (shifted.length > 0 ? (shifted.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.alignment : nil) ?? .natural
        // `userSelect: text` opts a label into selection (contract §2);
        // labels are not selectable otherwise.
        label.isSelectable = paint.userSelect == "text"
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: Label, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? nsView.intrinsicContentSize.width, height: proposal.height ?? lineHeight)
    }
}
#endif

extension TextLabel {
    /// The rect a label lays its text out in: its bounds WIDENED by two
    /// device pixels on the side(s) the text grows towards. The frame is the
    /// measurer's width (1/64 px), but the view system snaps it to the
    /// pixel grid (54.781 → 54.667 at 3x) and TextKit ceils the text's own
    /// width to a pixel (54.770 → 55.000) before it decides to truncate or
    /// wrap, so a line that FITS as measured lost its tail ("Accou…").
    /// The slack only absorbs that rounding (≤ 2 px); a line genuinely wider
    /// than its box (a `lines` clamp, a shrunk native) still truncates.
    /// Labels never clip to bounds, so the slack paints like gpui, unclipped.
    static func paintRect(_ rect: CGRect, alignment: NSTextAlignment, rtl: Bool, scale: CGFloat) -> CGRect {
        let slack = 2 / max(scale, 1)
        var r = rect
        switch alignment {
        case .center:
            r.origin.x -= slack / 2
        case .right:
            r.origin.x -= slack
        case .natural where rtl, .justified where rtl:
            r.origin.x -= slack
        default:
            break
        }
        r.size.width += slack
        return r
    }

    /// Plain text in a text style. `align` is PHYSICAL (the core resolved
    /// `start`/`end`; `end` is still read as the trailing edge of the
    /// surface for older callers).
    init(_ text: String, _ ts: TextStyle, color: Color, align: String? = nil, lines: Int? = nil) {
        let alignment: NSTextAlignment = switch align {
        case "center": .center
        case "right", "end": .right
        case "left": .left
        case "justify": .justified
        default: .natural
        }
        var attrs = TextShaper.attributes(ts, align: alignment, lineBreak: (lines ?? 0) > 0 ? .byTruncatingTail : .byWordWrapping)
        #if canImport(UIKit)
        attrs[.foregroundColor] = UIColor(color)
        #else
        attrs[.foregroundColor] = NSColor(color)
        #endif
        self.init(attributed: NSAttributedString(string: text, attributes: attrs), lineHeight: ts.lineHeight, lines: lines)
    }

    /// The runs with the inherited text keys applied (contract §2):
    /// `textTransform` per run (attributes kept), `letterSpacing` as kern,
    /// underline / line-through, italic (the run's font with the italic
    /// trait), and the paragraph's writing direction under rtl (a natural
    /// alignment then sits on the right).
    static func shaped(_ source: NSAttributedString, _ paint: TextPaint, rtl: Bool) -> NSAttributedString {
        if paint.isPlain && !rtl { return source }
        let out = NSMutableAttributedString()
        source.enumerateAttributes(in: NSRange(location: 0, length: source.length)) { attrs, range, _ in
            var a = attrs
            let raw = (source.string as NSString).substring(with: range)
            if let l = paint.letterSpacing, l != 0 { a[.kern] = l }
            switch paint.decoration {
            case .underline: a[.underlineStyle] = NSUnderlineStyle.single.rawValue
            case .lineThrough: a[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            case nil: break
            }
            if paint.italic == true, let f = a[.font] as? PlatformFont { a[.font] = italicFont(f) }
            if rtl, let p = a[.paragraphStyle] as? NSParagraphStyle, let m = p.mutableCopy() as? NSMutableParagraphStyle {
                m.baseWritingDirection = .rightToLeft
                if m.alignment == .natural { m.alignment = .right }
                a[.paragraphStyle] = m
            }
            out.append(NSAttributedString(string: TextPaint.transform(raw, paint.transform), attributes: a))
        }
        return out
    }

    static func italicFont(_ f: PlatformFont) -> PlatformFont {
        #if canImport(UIKit)
        guard let d = f.fontDescriptor.withSymbolicTraits(f.fontDescriptor.symbolicTraits.union(.traitItalic)) else { return f }
        return UIFont(descriptor: d, size: f.pointSize)
        #else
        return NSFontManager.shared.convert(f, toHaveTrait: .italicFontMask)
        #endif
    }
}
