import SwiftUI

#if canImport(UIKit)
import UIKit

/// A TextKit label: the same engine the measurer shaped the text with, so
/// a measured height is the painted height (line heights pinned by the
/// paragraph style). `lines` truncates with an ellipsis.
struct TextLabel: UIViewRepresentable {
    let attributed: NSAttributedString
    var lineHeight: CGFloat
    var lines: Int? = nil
    var wraps: Bool = true

    final class Label: UILabel {
        var halfLeading: CGFloat = 0

        override func drawText(in rect: CGRect) {
            // TextKit sits the glyphs at the BOTTOM of a pinned line (the
            // extra height goes above the ascender); CSS centres them
            // (half-leading). Shift the text up by half the leading.
            super.drawText(in: rect.offsetBy(dx: 0, dy: -halfLeading))
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
        label.attributedText = attributed
        label.numberOfLines = wraps ? (lines ?? 0) : 1
        label.lineBreakMode = wraps ? ((lines ?? 0) > 0 ? .byTruncatingTail : .byWordWrapping) : .byClipping
        label.halfLeading = attributed.length > 0 ? (attributed.attribute(.init("ExponentialUI.halfLeading"), at: 0, effectiveRange: nil) as? CGFloat ?? 0) : 0
        label.textAlignment = (attributed.length > 0 ? (attributed.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.alignment : nil) ?? .natural
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
        // TextKit sits the glyphs at the BOTTOM of a pinned line; CSS
        // centres them. A baseline offset of half the leading does it.
        let shifted = NSMutableAttributedString(attributedString: attributed)
        if attributed.length > 0, let half = attributed.attribute(.init("ExponentialUI.halfLeading"), at: 0, effectiveRange: nil) as? CGFloat, half > 0 {
            shifted.addAttribute(.baselineOffset, value: half, range: NSRange(location: 0, length: shifted.length))
        }
        label.attributedStringValue = shifted
        label.maximumNumberOfLines = wraps ? (lines ?? 0) : 1
        label.lineBreakMode = wraps ? ((lines ?? 0) > 0 ? .byTruncatingTail : .byWordWrapping) : .byClipping
        label.cell?.wraps = wraps
        label.alignment = (attributed.length > 0 ? (attributed.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.alignment : nil) ?? .natural
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: Label, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? nsView.intrinsicContentSize.width, height: proposal.height ?? lineHeight)
    }
}
#endif

extension TextLabel {
    /// Plain text in a text style.
    init(_ text: String, _ ts: TextStyle, color: Color, align: String? = nil, lines: Int? = nil) {
        let alignment: NSTextAlignment = switch align { case "center": .center; case "right", "end": .right; default: .natural }
        var attrs = TextShaper.attributes(ts, align: alignment, lineBreak: (lines ?? 0) > 0 ? .byTruncatingTail : .byWordWrapping)
        #if canImport(UIKit)
        attrs[.foregroundColor] = UIColor(color)
        #else
        attrs[.foregroundColor] = NSColor(color)
        #endif
        self.init(attributed: NSAttributedString(string: text, attributes: attrs), lineHeight: ts.lineHeight, lines: lines)
    }
}
