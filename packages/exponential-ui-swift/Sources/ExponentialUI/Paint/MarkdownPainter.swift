import SwiftUI
import ExponentialUIPrimitives

/// The markdown painter: resolves the `Markdown/*` recipe parts into
/// `MarkdownStyles` (shared with the measurer) and paints the blocks the
/// layout placed, each through the same TextKit label the measurer shaped.
enum MarkdownPainter {
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

/// The painted document: blocks at the layout's boxes inside `width`.
struct MarkdownView: View {
    let text: String
    let width: CGFloat
    let styles: MarkdownStyles
    let ink: Color
    let model: SurfaceModel
    let props: Props

    private var blocks: [MarkdownBlock] { Markdown.parse(text) }

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

    private func platform(_ c: Color) -> PlatformColor {
        #if canImport(UIKit)
        UIColor(c)
        #else
        NSColor(c)
        #endif
    }

    @ViewBuilder
    private func blockView(_ block: MarkdownBlock, box: MarkdownBlockBox, link: Color, codeBg: Color?, codeBlockBg: Color?, quoteBorder: Color, muted: Color, border: Color) -> some View {
        let spec = styles.spec(of: block.kind)
        let mono = model.theme?.monoFamily
        switch block.kind {
        case .paragraph, .heading:
            TextLabel(attributed: TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: codeBg.map(platform)), lineHeight: spec.lineHeight)
        case let .listItem(marker, task):
            HStack(alignment: .top, spacing: 0) {
                Group {
                    if let task {
                        Image(systemName: task ? "checkmark.square" : "square").font(.system(size: spec.size * 0.9)).foregroundStyle(task ? ink : muted)
                    } else {
                        Text(marker).font(.system(size: spec.size)).foregroundStyle(muted)
                    }
                }
                .frame(width: styles.listIndent, height: spec.lineHeight, alignment: .leading)
                TextLabel(attributed: TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: codeBg.map(platform)), lineHeight: spec.lineHeight)
            }
        case .quote:
            HStack(alignment: .top, spacing: 0) {
                Rectangle().fill(quoteBorder).frame(width: styles.quoteBorder)
                Color.clear.frame(width: styles.quotePad)
                TextLabel(attributed: TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(muted), link: platform(link), codeBackground: nil), lineHeight: spec.lineHeight)
            }
        case .codeBlock:
            TextLabel(attributed: TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: nil), lineHeight: spec.lineHeight, wraps: false)
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
