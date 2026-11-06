import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives

/// The markdown painter: resolves the `Markdown/*` recipe parts into
/// `MarkdownStyles` (shared with the measurer) and paints the blocks the
/// layout placed, each through the same TextKit label the measurer shaped.
enum MarkdownPainter {
    /// The parsed document with its image blocks resolved against the media
    /// policy: ONE rule for the measurer and the painter.
    @MainActor
    static func blocks(_ text: String, model: SurfaceModel) -> [MarkdownBlock] {
        Markdown.resolveImages(Markdown.parse(text)) { model.mediaRequest($0) != nil }
    }

    /// `blocks` with the URL policy on every link, and the text a host's
    /// `textLinks` links as links (the painter only: never a width).
    @MainActor
    static func painted(_ text: String, model: SurfaceModel) -> [MarkdownBlock] {
        Markdown.linkText(Markdown.mapLinks(blocks(text, model: model), model.linkHref)) { piece in
            model.textLinkSegments(piece)?.map { (text: $0.text, href: $0.href) }
        }
    }

    /// From the theme's `Markdown/*` recipes (the root's own text style is
    /// the body); plain metrics in geometry mode.
    static func styles(theme: ThemeHandle?, mode: Mode, body: TextStyle, props: Props) -> MarkdownStyles {
        let bodySpec = MarkdownTextSpec(size: body.fontSize, lineHeight: body.lineHeight, weight: body.fontWeight, family: body.fontFamily)
        var s = MarkdownStyles(body: bodySpec)
        guard let theme else { return s }
        func part(_ name: String) -> PartStyle { theme.part("Markdown", name, props: props, mode: mode) }
        func spec(_ p: PartStyle, _ base: MarkdownTextSpec) -> MarkdownTextSpec {
            MarkdownTextSpec(size: p.px("fontSize") ?? base.size, lineHeight: p.px("lineHeight") ?? base.lineHeight, weight: Int(p.props.num("fontWeight") ?? Double(base.weight)), family: p.fontFamily ?? base.family)
        }
        var headingBase = s.body
        headingBase.weight = 600
        s.heading = spec(part("heading"), headingBase)
        let code = part("codeBlock")
        var codeBase = s.code
        codeBase.family = theme.monoFamily
        s.code = spec(code, codeBase)
        s.codePad = code.px("padding") ?? code.px("paddingHorizontal") ?? s.codePad
        let quote = part("quote")
        s.quoteBorder = quote.px("borderWidth") ?? s.quoteBorder
        s.quotePad = quote.px("paddingHorizontal") ?? s.quotePad
        s.blockGap = theme.spacing("sm")
        s.headingTop = theme.spacing("md")
        s.headingBottom = theme.spacing("xs")
        s.cellPadH = theme.spacing("sm")
        s.cellPadV = theme.spacing("xs")
        return s
    }
}

/// #VAPP-258 (fixtures/markdown-render.json): a fenced block's code
/// tokens, the catalog's ONE tokenizer (`tokenizeCodeJson`, CodeBlock's
/// tokens), as (UTF-16 range, kind) over the block's text, when its fence
/// names a highlighted language: `plain` and `ansi` (whose escapes the
/// tokenizer drops) never are, an unknown one tokenizes as all `plain`.
/// Empty = plain text. Tokens that do not concatenate back to the text
/// paint nothing.
enum MarkdownCode {
    static func tokens(_ text: String, lang: String?) -> [(range: NSRange, kind: String)] {
        guard let lang, !lang.isEmpty, lang != "plain", lang != "ansi", !text.isEmpty else { return [] }
        var out: [(range: NSRange, kind: String)] = []
        var at = 0
        var joined = ""
        for (i, line) in (JSONValue.parse(tokenizeCodeJson(code: text, language: lang)).array ?? []).enumerated() {
            if i > 0 { joined += "\n"; at += 1 }
            for t in line.array ?? [] {
                let piece = t["text"]?.string ?? ""
                let n = (piece as NSString).length
                if n > 0 { out.append((NSRange(location: at, length: n), t["kind"]?.string ?? "plain")) }
                joined += piece
                at += n
            }
        }
        return joined == text ? out : []
    }

    /// `runs` with each token but `plain` in CodeBlock's colour for its
    /// kind (`color(kind)`, nil = the ink): colour only, so the shaped
    /// text and its measured height never change.
    static func highlighted(_ runs: NSAttributedString, lang: String?, color: (String) -> PlatformColor?) -> NSAttributedString {
        let tokens = tokens(runs.string, lang: lang)
        if tokens.isEmpty { return runs }
        let out = NSMutableAttributedString(attributedString: runs)
        var cache: [String: PlatformColor?] = [:]
        for (range, kind) in tokens where kind != "plain" {
            let c = cache[kind] ?? color(kind)
            cache[kind] = c
            if let c { out.addAttribute(.foregroundColor, value: c, range: range) }
        }
        return out
    }
}

/// The painted document: blocks at the layout's boxes inside `width`.
struct MarkdownView: View {
    let text: String
    let width: CGFloat
    let styles: MarkdownStyles
    let ink: Color
    let model: SurfaceModel
    let props: Props

    /// The blocks as painted: images the media policy denies become their
    /// alt text (as the measurer saw them), links the URL policy denies
    /// plain text, and the text a host's `textLinks` links is a link too.
    private var blocks: [MarkdownBlock] { MarkdownPainter.painted(text, model: model) }

    var body: some View {
        let blocks = self.blocks
        let shaper = MarkdownShaper(mono: model.theme?.monoFamily)
        let layout = Markdown.layout(blocks, styles, width: width, text: shaper)
        let link = model.part("Markdown", "link", props: props).color ?? model.color("primary") ?? ink
        let codePart = model.part("Markdown", "code", props: props)
        let codeBlockPart = model.part("Markdown", "codeBlock", props: props)
        let quotePart = model.part("Markdown", "quote", props: props)
        let muted = model.color("mutedForeground") ?? ink.opacity(0.6)
        let border = model.color("border") ?? ink.opacity(0.2)
        ZStack(alignment: .topLeading) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { i, block in
                let box = layout.blocks[i]
                blockView(block, box: box, link: link, codeBg: codePart.style.background, codeBlockBg: codeBlockPart.style.background ?? model.color("muted"), quoteBorder: quotePart.style.borderColor ?? border, muted: muted, border: border)
                    .frame(width: width, height: box.height, alignment: .topLeading)
                    .offset(y: box.y)
            }
        }
        .frame(width: width, height: layout.height, alignment: .topLeading)
    }

    /// A text block whose allowed links open on a tap (through the URL
    /// policy again in `openLink`); hit-tested with the measurer's
    /// typesetter at the block's wrap width.
    @ViewBuilder
    private func linked(_ runs: NSAttributedString, spec: MarkdownTextSpec, width: CGFloat) -> some View {
        let label = TextLabel(attributed: runs, lineHeight: spec.lineHeight)
        if TextShaper.hasLinks(runs) {
            label.contentShape(Rectangle()).onTapGesture(coordinateSpace: .local) { point in
                if let href = TextShaper.link(in: runs, lineHeight: spec.lineHeight, wrap: width, at: point) { model.openLink(href) }
            }
        } else {
            label
        }
    }

    private func platform(_ c: Color) -> PlatformColor {
        #if canImport(UIKit)
        UIColor(c)
        #else
        NSColor(c)
        #endif
    }

    /// Web's `list-style-position: outside` markers in the text colour: a
    /// disc 0.34em wide whose centre sits 0.46em into the indent, on the
    /// x-height's middle (Chromium's `disc`); an ordered `3.` ends one
    /// space before the item text.
    @ViewBuilder
    private func listMarker(_ marker: String, spec: MarkdownTextSpec) -> some View {
        let font = ExponentialUIFonts.font(family: spec.family, weight: spec.weight, size: spec.size)
        let baseline = (spec.lineHeight - (font.ascender - font.descender)) / 2 + font.ascender
        if marker == "•" {
            let d = spec.size * 0.34
            Circle().fill(ink)
                .frame(width: d, height: d)
                .padding(.leading, spec.size * 0.46 - d / 2)
                .padding(.top, baseline - font.xHeight / 2 - d / 2)
                .frame(width: styles.listIndent, height: spec.lineHeight, alignment: .topLeading)
        } else {
            let space = (" " as NSString).size(withAttributes: [.font: font]).width
            MeasuredLabel(text: marker, ts: TextStyle(fontSize: spec.size, fontWeight: spec.weight, lineHeight: spec.lineHeight, fontFamily: spec.family), color: ink)
                .fixedSize()
                .padding(.trailing, space)
                .frame(width: styles.listIndent, height: spec.lineHeight, alignment: .topTrailing)
        }
    }

    @ViewBuilder
    private func blockView(_ block: MarkdownBlock, box: MarkdownBlockBox, link: Color, codeBg: Color?, codeBlockBg: Color?, quoteBorder: Color, muted: Color, border: Color) -> some View {
        let spec = styles.spec(of: block.kind)
        let mono = model.theme?.monoFamily
        switch block.kind {
        case .paragraph, .heading:
            linked(TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: codeBg.map(platform)), spec: spec, width: width)
        case let .image(src, alt):
            MediaImage(request: model.mediaRequest(src), options: model.mediaPolicy) { image in
                image.resizable().scaledToFit().frame(maxWidth: width, maxHeight: box.height, alignment: .topLeading)
            } placeholder: {
                Text(alt).font(.system(size: spec.size)).foregroundStyle(muted).lineLimit(2)
            }
            .frame(width: width, height: box.height, alignment: .topLeading)
            .accessibilityLabel(alt)
        case let .listItem(marker, task):
            HStack(alignment: .top, spacing: 0) {
                Group {
                    if let task {
                        MarkdownTaskBox(checked: task, model: model, props: props, ink: ink, border: border)
                            .frame(height: spec.lineHeight)
                    } else {
                        listMarker(marker, spec: spec)
                    }
                }
                .frame(width: styles.listIndent, height: spec.lineHeight, alignment: .topLeading)
                linked(TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: codeBg.map(platform)), spec: spec, width: max(width - styles.listInset(block), 1))
            }
            .padding(.leading, styles.listIndent * CGFloat(block.depth))
        case .quote:
            HStack(alignment: .top, spacing: 0) {
                Rectangle().fill(quoteBorder).frame(width: styles.quoteBorder)
                Color.clear.frame(width: styles.quotePad)
                linked(TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(muted), link: platform(link), codeBackground: nil), spec: spec, width: max(width - styles.quoteBorder - styles.quotePad, 1))
            }
        case .codeBlock:
            TextLabel(attributed: MarkdownCode.highlighted(TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: nil), lang: block.lang) { kind in model.codeTokenLook(kind).color.map(platform) }, lineHeight: spec.lineHeight, wraps: false)
                .padding(styles.codePad)
                .frame(width: width, height: box.height, alignment: .topLeading)
                .background(codeBlockBg ?? ink.opacity(0.06), in: RoundedRectangle(cornerRadius: model.theme?.radius("md") ?? 6))
        case .rule:
            Rectangle().fill(border).frame(height: 1)
        case let .table(header, rows):
            let cols = max(header.count, rows.map(\.count).max() ?? 0, 1)
            let cellW = width / CGFloat(cols)
            VStack(spacing: 0) {
                ForEach(Array(([header] + rows).enumerated()), id: \.offset) { r, row in
                    HStack(alignment: .top, spacing: 0) {
                        ForEach(0..<cols, id: \.self) { c in
                            let cell = c < row.count ? row[c] : []
                            var cellSpec = spec
                            let _ = { if r == 0 { cellSpec.weight = max(cellSpec.weight, 600) } }()
                            TextLabel(attributed: TextShaper.runs(cell, cellSpec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: codeBg.map(platform)), lineHeight: spec.lineHeight)
                                .padding(.horizontal, styles.cellPadH)
                                .padding(.vertical, styles.cellPadV)
                                .frame(width: cellW, height: box.rows[safe: r].map { $0 - 1 } ?? spec.lineHeight, alignment: .topLeading)
                        }
                    }
                    Rectangle().fill(border).frame(height: 1)
                }
            }
            .overlay(alignment: .top) { Rectangle().fill(border).frame(height: 1) }
        }
    }
}

/// A task item's box = the Checkbox `box` + `check` recipe parts (the same
/// square the Checkbox component paints: 16×16, 1pt border, 4pt corner,
/// `ui-check` at 14 when checked), never an SF glyph.
struct MarkdownTaskBox: View {
    let checked: Bool
    let model: SurfaceModel
    let props: Props
    let ink: Color
    let border: Color

    var body: some View {
        let p: Props = { var p = props; p["checked"] = .bool(checked); return p }()
        let states = checked ? ["checked"] : []
        let box = model.part("Checkbox", "box", props: p, states: states)
        let check = model.part("Checkbox", "check", props: p, states: states)
        let side = box.width ?? 16
        let shape = RoundedRectangle(cornerRadius: box.style.radius > 0 ? box.style.radius : 4)
        ZStack {
            shape.fill(box.style.background ?? (checked ? ink : Color.clear))
            shape.strokeBorder(box.style.borderColor ?? (checked ? ink : border), lineWidth: max(box.style.borderWidth, 1))
            if checked {
                ConceptIcon(name: BuiltinIcons.name("Checkbox.check"), size: check.width ?? 14, color: check.color ?? box.style.color ?? ink, model: model, weight: .bold)
            }
        }
        .frame(width: side, height: side)
        .accessibilityHidden(true)
    }
}
