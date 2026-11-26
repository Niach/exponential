import SwiftUI
import CoreText
import ExponentialUIPrimitives

/// Round 4 group B (`fixtures/text-runs.json`): a Text's `runs` and its
/// `dropCap`, painted as ONE styled string (the Markdown inline machinery:
/// weight, tone colour, the `highlight` fill, inline code's `Markdown/code`
/// box, strike, underline, policed links) on the lines `TextRunsLayout`
/// broke, the measurer's: the first `dropCap` rows beside the cap's box,
/// then the full width. `lines` / `truncate` clamp the paragraph (the last
/// line ellipsized); search marks run over the joined text.
struct TextRunsLeaf: View {
    let cx: LeafContext

    /// The look a leaf paints its runs with.
    @MainActor
    static func look(_ cx: LeafContext) -> TextRunsLook {
        let model = cx.model
        var look = TextRunsLook(style: cx.textStyle)
        look.align = alignment(cx.style.textAlign ?? cx.props["align"]?.string)
        look.mono = model.theme?.monoFamily
        look.chip = MarkdownPainter.styles(theme: model.theme, mode: model.mode, body: cx.textStyle, props: [:], fontScale: model.textScale).chip
        look.codeBackground = model.part("Markdown", "code", props: [:]).style.background.map(HighlightPainter.platform)
        look.ink = HighlightPainter.platform(cx.ink)
        look.weight = { [theme = model.theme] in theme?.fontWeight($0) }
        let ownerProps = cx.ownerProps
        look.tone = { tone in
            var props = ownerProps
            props["tone"] = .string(tone)
            return (model.part("Text", "root", props: props).color ?? cx.tone(tone)).map(HighlightPainter.platform)
        }
        look.href = { model.linkHref($0) }
        look.match = HighlightPainter.platform(HighlightPainter.colors(model).match)
        look.gap = model.spacing("xs")
        return look
    }

    static func alignment(_ align: String?) -> NSTextAlignment {
        switch align {
        case "center": .center
        case "right", "end": .right
        case "left": .left
        case "justify": .justified
        default: .natural
        }
    }

    var body: some View {
        let look = Self.look(cx)
        let parts = TextRuns.parts(cx.props, look)
        let lh = look.style.lineHeight
        let width = cx.inner.width
        let rtl = cx.rtl
        let text = marked(parts.text, look: look)
        let layout = TextRunsLayout(text: text, cap: parts.cap, capLines: parts.capLines, gap: look.gap, lineHeight: lh, width: width)
        let shown = Self.clamped(layout, clamp: parts.cap == nil ? cx.node.lines : nil)
        let flush = Self.flush(look.align, rtl: rtl)
        ZStack(alignment: .topLeading) {
            codeFills(layout, shown: shown.count, flush: flush, rtl: rtl)
            if let cap = parts.cap {
                let advance = layout.capBox.width - look.gap
                TextLabel(attributed: cap, lineHeight: layout.capBox.height, lines: 1, wraps: false)
                    .frame(width: advance, height: layout.capBox.height)
                    .offset(x: rtl ? width - advance : 0)
            }
            ForEach(Array(shown.enumerated()), id: \.offset) { _, line in
                TextLabel(attributed: line.text, lineHeight: lh, lines: 1, wraps: line.last)
                    .frame(width: line.line.width, height: lh)
                    .offset(x: rtl ? width - line.line.indent - line.line.width : line.line.indent, y: CGFloat(line.line.row) * lh)
            }
        }
        .frame(width: width, height: cx.inner.height, alignment: .topLeading)
        .modifier(TextRunsLinks(cx: cx, layout: layout, rtl: rtl))
        .clipped()
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    /// The lines painted: all of them, or the first `clamp` with the last
    /// carrying the rest (its line breaks as spaces) to ellipsize.
    static func clamped(_ layout: TextRunsLayout, clamp: Int?) -> [(line: TextRunsLayout.Line, text: NSAttributedString, last: Bool)] {
        let lines = layout.lines
        guard let n = clamp, n > 0, lines.count > n else {
            return lines.map { ($0, layout.text.attributedSubstring(from: $0.range), false) }
        }
        var out = lines[..<(n - 1)].map { ($0, layout.text.attributedSubstring(from: $0.range), false) }
        let from = lines[n - 1].range.location
        let rest = NSMutableAttributedString(attributedString: layout.text.attributedSubstring(from: NSRange(location: from, length: layout.text.length - from)))
        rest.mutableString.replaceOccurrences(of: "\n", with: " ", options: [], range: NSRange(location: 0, length: rest.length))
        out.append((lines[n - 1], rest, true))
        return out
    }

    /// CoreText's flush factor for a line in its box (0 = left, 1 = right).
    static func flush(_ align: NSTextAlignment, rtl: Bool) -> CGFloat {
        switch align {
        case .center: 0.5
        case .right: 1
        case .left: 0
        default: rtl ? 1 : 0
        }
    }

    /// #EDE-95: the search marks over the runs after the cap, numbered over
    /// the joined text (`props.text`): the cap itself never paints a fill.
    private func marked(_ runs: NSAttributedString, look: TextRunsLook) -> NSAttributedString {
        guard let spec = TextHighlightSpec(cx.props["highlight"]) else { return runs }
        let full = cx.props.text("text")
        let fullLength = (full as NSString).length
        guard (look.style.shown(full) as NSString).length == fullLength else { return runs }
        let (searched, map) = HighlightPainter.searchText(of: runs)
        let prefix = fullLength - (searched as NSString).length
        guard prefix >= 0 else { return runs }
        let found = TextHighlight.ranges(full, query: spec.query)
        guard !found.isEmpty else { return runs }
        let fills = HighlightPainter.colors(cx.model)
        let match = HighlightPainter.platform(fills.match), current = HighlightPainter.platform(fills.current)
        let local = spec.localCurrent
        let out = NSMutableAttributedString(attributedString: runs)
        for (k, r) in found.enumerated() {
            let s = max(r.start - prefix, 0), e = min(r.end - prefix, map.count)
            guard e > s else { continue }
            let start = map[s], end = map[e - 1] + 1
            out.addAttribute(.backgroundColor, value: k == local ? current : match, range: NSRange(location: start, length: end - start))
        }
        return out
    }

    /// Inline code's `Markdown/code` boxes under the text, per line.
    @ViewBuilder
    private func codeFills(_ layout: TextRunsLayout, shown: Int, flush: CGFloat, rtl: Bool) -> some View {
        let placed = Self.codeRects(layout, shown: shown, flush: flush, rtl: rtl, baseline: TextShaper.baseline(cx.textStyle))
        if !placed.isEmpty {
            ForEach(Array(placed.enumerated()), id: \.offset) { _, p in
                RoundedRectangle(cornerRadius: p.radius)
                    .fill(Color(p.fill))
                    .frame(width: p.rect.width, height: p.rect.height)
                    .offset(x: p.rect.minX, y: p.rect.minY)
            }
        }
    }

    static func codeRects(_ layout: TextRunsLayout, shown: Int, flush: CGFloat, rtl: Bool, baseline: CGFloat) -> [(fill: PlatformColor, radius: CGFloat, rect: CGRect)] {
        let a = layout.text
        var spans: [(range: NSRange, run: MarkdownCodeRun, fill: PlatformColor)] = []
        a.enumerateAttribute(TextShaper.codeKey, in: NSRange(location: 0, length: a.length)) { v, range, _ in
            if let run = v as? MarkdownCodeRun, let fill = run.fill { spans.append((range, run, fill)) }
        }
        if spans.isEmpty { return [] }
        var out: [(fill: PlatformColor, radius: CGFloat, rect: CGRect)] = []
        for line in layout.lines.prefix(shown) where line.range.length > 0 {
            let ct = CTTypesetterCreateLine(layout.typesetter, CFRange(location: line.range.location, length: line.range.length))
            let pen = CGFloat(CTLineGetPenOffsetForFlush(ct, flush, Double(line.width)))
            let x0 = rtl ? layout.width - line.indent - line.width : line.indent
            for span in spans {
                let part = NSIntersectionRange(span.range, line.range)
                guard part.length > 0 else { continue }
                let a0 = CTLineGetOffsetForStringIndex(ct, part.location, nil) + pen + x0
                let a1 = CTLineGetOffsetForStringIndex(ct, NSMaxRange(part), nil) + pen + x0
                let y = CGFloat(line.row) * layout.lineHeight + baseline - span.run.ascent
                out.append((span.fill, span.run.radius, CGRect(x: min(a0, a1), y: y, width: abs(a1 - a0), height: span.run.ascent + span.run.descent)))
            }
        }
        return out
    }
}

/// A tap on an allowed link opens it through the URL policy and never
/// presses the pressable parent; a tap elsewhere presses the nearest
/// pressable ancestor (the tap gesture shadows its button). No links = no
/// gesture.
private struct TextRunsLinks: ViewModifier {
    let cx: LeafContext
    let layout: TextRunsLayout
    let rtl: Bool

    func body(content: Content) -> some View {
        if TextShaper.hasLinks(layout.text) {
            content
                .contentShape(Rectangle())
                .onTapGesture(coordinateSpace: .local) { point in
                    if let href = layout.link(at: point, rtl: rtl, flush: TextRunsLeaf.flush(TextRunsLeaf.alignment(cx.style.textAlign ?? cx.props["align"]?.string), rtl: rtl)) {
                        _ = cx.model.openLink(href)
                        return
                    }
                    var ancestor = cx.node.parent
                    while let index = ancestor {
                        let node = cx.model.nodes[index]
                        if node.pressable {
                            cx.model.press(index)
                            return
                        }
                        ancestor = node.parent
                    }
                }
        } else {
            content
        }
    }
}
