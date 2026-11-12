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
    static func blocks(_ text: String, model: SurfaceModel, options: MarkdownOptions? = nil) -> [MarkdownBlock] {
        // #VAPP-258 step 5: the host's options (hard breaks, a streamed prefix).
        let o = options ?? model.host.markdownOptions
        return Markdown.resolveImages(Markdown.parse(text, hardBreaks: o.hardBreaks, streaming: o.streaming)) { model.mediaRequest($0) != nil }
    }

    /// #VAPP-258 (`markdown.options.imageMaxHeight`): `styles` with the
    /// host's image cap and the viewport (pt, 0 = unknown) an uncapped
    /// image stops at: what the measurer and the painter both lay out with.
    static func imaged(_ styles: MarkdownStyles, options: MarkdownOptions, viewport: CGFloat) -> MarkdownStyles {
        var s = styles
        s.imageHeight = options.imageCap
        s.viewportHeight = viewport > 0 ? viewport : nil
        return s
    }

    /// #VAPP-258 step 5: the props a Markdown's recipes resolve with:
    /// `density: chat` added under the host's chat rhythm (a
    /// painter-supplied recipe prop, `catalog/recipes.json`).
    static func props(_ props: Props, chat: Bool) -> Props {
        // Round 9 §9.1: the inline profile answers to no host density.
        guard chat, inlineVariant(props) == nil else { return props }
        var p = props
        p["density"] = .string("chat")
        return p
    }

    /// Round 9 §9.1: an inline-profile Markdown's `variant` (`body` or
    /// `caption`); nil = the document profile.
    static func inlineVariant(_ props: Props) -> String? {
        guard props.str("profile") == "inline" else { return nil }
        return props.str("variant") == "caption" ? "caption" : "body"
    }

    /// One recipe an inline-profile part paints with.
    struct InlineRecipe: Equatable {
        let component: String
        let part: String
        let props: Props
    }

    /// `markdownInlineRecipes` (fixtures/markdown-inline.json), first to
    /// last, later over earlier: a paragraph IS a Text of its `variant`, a
    /// heading a Heading of its level (h5, h6 paint h4).
    static func inlineRecipes(paragraph variant: String) -> [InlineRecipe] {
        [InlineRecipe(component: "Text", part: "root", props: ["variant": .string(variant)])]
    }

    static func inlineRecipes(heading level: Int) -> [InlineRecipe] {
        [
            InlineRecipe(component: "Text", part: "root", props: ["variant": .string("title")]),
            InlineRecipe(component: "Heading", part: "root", props: ["level": .string("h\(min(max(level, 1), 4))")]),
        ]
    }

    /// The ink of an inline-profile paragraph or heading: its recipes'
    /// colour, nil = the node's.
    @MainActor
    static func inlineInk(_ kind: MarkdownBlockKind, props: Props, model: SurfaceModel) -> Color? {
        guard let variant = inlineVariant(props) else { return nil }
        let recipes: [InlineRecipe]
        switch kind {
        case .paragraph: recipes = inlineRecipes(paragraph: variant)
        case let .heading(level): recipes = inlineRecipes(heading: level)
        default: return nil
        }
        return recipes.reversed().lazy.compactMap { model.part($0.component, $0.part, props: $0.props).color }.first
    }

    /// `blocks` after the host's Markdown hooks (#VAPP-258, the measurer
    /// and the painter alike): one-link paragraphs the host's `attachment`
    /// classifies become attachment blocks, block images take the host's
    /// `imageSize`, every link passes the URL policy, and the text a host's
    /// `textLinks` links is a link (a valid chip on an allowed href = an
    /// inline chip, whose width the measurer counts).
    @MainActor
    static func painted(_ text: String, model: SurfaceModel, options: MarkdownOptions? = nil) -> [MarkdownBlock] {
        let host = model.host
        let attached = Markdown.resolveAttachments(blocks(text, model: model, options: options), href: model.linkHref) { raw in
            host.attachment(raw).map { (kind: $0.kind, name: $0.name) }
        }
        // #VAPP-258 (markdown-host.json `loaded`): an image the host cannot
        // size, at the size it decoded at once it loaded (the media cache).
        let sized = Markdown.sizeImages(attached) { src in
            Markdown.imageSize(host: host.imageSize(src), loaded: model.mediaRequest(src).flatMap { MediaLoader.shared.cached($0) }?.size)
        }
        return Markdown.linkText(Markdown.mapLinks(sized, model.linkHref), chips: { piece in
            model.textLinkSegments(piece)?.map { s in
                (text: s.text, href: s.href, chip: s.chip.map { MarkdownChip(kind: $0.kind, label: $0.label, icon: $0.icon, ident: $0.ident) })
            }
        })
    }

    /// From the theme's `Markdown/*` recipes (the root's own text style is
    /// the body); plain metrics in geometry mode. #VAPP-258: `fontScale` =
    /// the surface's type scale (Dynamic Type): `body` arrives scaled (the
    /// core's text style), every part the recipes size here (the chat
    /// body, headings, code, the chip label) scales by it too, so the
    /// measurer and the painter agree at every text size.
    static func styles(theme: ThemeHandle?, mode: Mode, body: TextStyle, props: Props, fontScale: CGFloat = 1) -> MarkdownStyles {
        let k = fontScale.isFinite && fontScale > 0 ? fontScale : 1
        let bodySpec = MarkdownTextSpec(size: body.fontSize, lineHeight: body.lineHeight, weight: body.fontWeight, family: body.fontFamily)
        var s = MarkdownStyles(body: bodySpec)
        guard let theme else { return s }
        func part(_ name: String, _ extra: Props = [:]) -> PartStyle { theme.part("Markdown", name, props: props.merging(extra) { _, b in b }, mode: mode) }
        func spec(_ p: PartStyle, _ base: MarkdownTextSpec) -> MarkdownTextSpec {
            MarkdownTextSpec(size: p.px("fontSize").map { $0 * k } ?? base.size, lineHeight: p.px("lineHeight").map { $0 * k } ?? base.lineHeight, weight: Int(p.props.num("fontWeight") ?? Double(base.weight)), family: p.fontFamily ?? base.family)
        }
        // #VAPP-258 step 5: the chat rhythm re-resolves the root (the node's
        // text style knows no density): its type is the body, its gap the
        // block gap. The document keeps the node's style and the `sm` gap.
        let chat = props["density"]?.string == "chat"
        let root = chat ? part("root") : nil
        if let root { s.body = spec(root, s.body) }
        var headingBase = s.body
        headingBase.weight = 600
        s.heading = spec(part("heading"), headingBase)
        // #VAPP-258: each heading level's type and margins (the `level`
        // the painter supplies; margins collapse like CSS block margins).
        let headingPart = part("heading")
        s.headingTop = headingPart.px("marginTop") ?? theme.spacing("md")
        s.headingBottom = headingPart.px("marginBottom") ?? theme.spacing("xs")
        for level in 1...6 {
            let p = part("heading", ["level": .number(Double(level))])
            s.headingLevels[level] = spec(p, headingBase)
            s.headingTops[level] = p.px("marginTop") ?? s.headingTop
            s.headingBottoms[level] = p.px("marginBottom") ?? s.headingBottom
        }
        let item = part("listItem")
        s.itemTop = item.px("marginTop") ?? 0
        s.itemBottom = item.px("marginBottom") ?? 0
        let code = part("codeBlock")
        var codeBase = s.code
        codeBase.family = theme.monoFamily
        s.code = spec(code, codeBase)
        s.codePad = code.px("padding") ?? code.px("paddingHorizontal") ?? s.codePad
        s.codeBorder = code.style.borderWidth
        let quote = part("quote")
        s.quoteBorder = quote.px("borderWidth") ?? s.quoteBorder
        s.quotePad = quote.px("paddingHorizontal") ?? s.quotePad
        s.imageRadius = part("image").style.radius
        s.blockGap = root?.px("gap") ?? theme.spacing("sm")
        // Round 9 §9.1: the inline profile's paragraph and heading type are
        // the Text / Heading recipes, no `Markdown/*` type (the document
        // gaps stay).
        if let variant = inlineVariant(props) {
            func type(_ recipes: [InlineRecipe]) -> MarkdownTextSpec {
                recipes.reduce(s.body) { spec(theme.part($1.component, $1.part, props: $1.props, mode: mode), $0) }
            }
            s.paragraph = type(inlineRecipes(paragraph: variant))
            for level in 1...6 { s.headingLevels[level] = type(inlineRecipes(heading: level)) }
        }
        // #VAPP-258: every cell's paddings (`Markdown/cell`).
        let cell = part("cell")
        s.cellPadH = cell.px("paddingHorizontal") ?? cell.px("padding") ?? theme.spacing("sm")
        s.cellPadV = cell.px("paddingVertical") ?? cell.px("padding") ?? theme.spacing("xs")
        // #VAPP-258: the inline chip (Chip's geometry, the Text caption's
        // type) and the attachment blocks (the media natives' boxes).
        let caption = theme.part("Text", "root", props: ["variant": .string("caption")], mode: mode)
        let captionSpec = MarkdownTextSpec(size: (caption.px("fontSize") ?? 12) * k, lineHeight: (caption.px("lineHeight") ?? 16) * k, weight: Int(caption.props.num("fontWeight") ?? Double(body.fontWeight)), family: caption.fontFamily ?? body.fontFamily)
        s.chip = MarkdownChipStyle(label: captionSpec, paddingH: theme.spacing("sm"), gap: theme.spacing("xs"), icon: theme.control("iconSm", 16) * k, height: theme.control("badge", 20) * k)
        // #VAPP-258: a theme's `Markdown/chip*` rules per chip kind (their
        // metrics; MarkdownChipView resolves the colours).
        for kind in MarkdownChip.kinds {
            let kp: Props = ["chip": .string(kind)]
            let box = part("chip", kp), label = part("chipLabel", kp), ident = part("chipIdent", kp)
            func over(_ p: PartStyle, _ base: MarkdownTextSpec) -> MarkdownTextSpec? {
                p.px("fontSize") == nil && p.px("lineHeight") == nil && p.props.num("fontWeight") == nil && p.fontFamily == nil ? nil : spec(p, base)
            }
            let look = MarkdownChipKindStyle(
                label: over(label, captionSpec),
                ident: over(ident, MarkdownTextSpec(size: captionSpec.size, lineHeight: captionSpec.lineHeight, weight: captionSpec.weight, family: theme.monoFamily)),
                paddingH: box.px("paddingHorizontal") ?? box.px("padding"),
                radius: box.px("borderRadius")
            )
            if look != MarkdownChipKindStyle() { s.chip.kinds[kind] = look }
        }
        // Round 9: inline code = the `Markdown/code` recipe, its type scaled with the rest.
        let inlineCode = part("code")
        s.chip.code = MarkdownCodeStyle(
            size: inlineCode.px("fontSize").map { $0 * k },
            paddingH: inlineCode.px("paddingHorizontal") ?? inlineCode.px("padding"),
            paddingV: inlineCode.px("paddingVertical") ?? inlineCode.px("padding") ?? 0,
            radius: inlineCode.px("borderRadius") ?? 4
        )
        s.audioTitleGap = theme.spacing("xs")
        s.audioControls = theme.control("row", SurfaceMeasurer.audioControlsHeight)
        s.fileIcon = theme.control("iconSm", 16)
        s.fileGap = theme.spacing("sm")
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

    /// `runs` split at its newlines (each line's attributes kept).
    static func lines(_ runs: NSAttributedString) -> [NSAttributedString] {
        let ns = runs.string as NSString
        var out: [NSAttributedString] = []
        var start = 0
        while true {
            let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
            let end = nl.location == NSNotFound ? ns.length : nl.location
            out.append(runs.attributedSubstring(from: NSRange(location: start, length: end - start)))
            if nl.location == NSNotFound { break }
            start = end + 1
        }
        return out
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
    /// The leaf's node index (a block image that decodes at a size the host
    /// did not know measures it again).
    var node: Int? = nil
    /// #VAPP-258: the options it renders with (nil = the host's).
    var options: MarkdownOptions? = nil
    /// #VAPP-258: blocks and their layout at `width` already computed (a
    /// `MarkdownText`'s, which measured them); nil = computed here.
    var prepared: (blocks: [MarkdownBlock], layout: MarkdownLayout)? = nil
    /// #VAPP-258: an unsized block image decoded (outside a surface: the
    /// host view lays out again); nil = `node` remeasures.
    var onImageLoaded: (() -> Void)? = nil
    /// The typeset text blocks this document keeps (a `MarkdownText`'s);
    /// nil = shaped on every pass.
    var paragraphs: MarkdownTextDocument? = nil

    /// #VAPP-258: `selectable` (iOS): text blocks select natively.
    @Environment(\.xuiTextPaint) private var paint

    private var selects: Bool {
        #if canImport(UIKit)
        paint.userSelect == "text"
        #else
        false
        #endif
    }

    /// The blocks as painted: images the media policy denies become their
    /// alt text (as the measurer saw them), links the URL policy denies
    /// plain text, and the text a host's `textLinks` links is a link too.
    private var blocks: [MarkdownBlock] { MarkdownPainter.painted(text, model: model, options: options) }

    var body: some View {
        let blocks = prepared?.blocks ?? self.blocks
        let layout = prepared?.layout ?? Markdown.layout(blocks, styles, width: width, text: MarkdownShaper(mono: model.theme?.monoFamily, chip: styles.chip))
        let link = model.part("Markdown", "link", props: props).color ?? model.color("primary") ?? ink
        let codePart = model.part("Markdown", "code", props: props)
        let codeBlockPart = model.part("Markdown", "codeBlock", props: props)
        let quotePart = model.part("Markdown", "quote", props: props)
        let muted = model.color("mutedForeground") ?? ink.opacity(0.6)
        let border = model.color("border") ?? ink.opacity(0.2)
        ZStack(alignment: .topLeading) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { i, block in
                let box = layout.blocks[i]
                blockView(block, index: i, box: box, link: link, codeBg: codePart.style.background, codeBlockBg: codeBlockPart.style.background ?? model.color("muted"), quoteBorder: quotePart.style.borderColor ?? border, muted: muted, border: border)
                    .frame(width: width, height: box.height, alignment: .topLeading)
                    .offset(y: box.y)
            }
        }
        .frame(width: width, height: layout.height, alignment: .topLeading)
    }

    /// A text block whose allowed links open on a tap (through the URL
    /// policy again in `openLink`); hit-tested with the measurer's
    /// typesetter at the block's wrap width. #VAPP-258: its inline chips
    /// paint over their reserved boxes (each opens its href).
    @ViewBuilder
    private func linked(_ lines: TextLines, spec: MarkdownTextSpec) -> some View {
        let runs = lines.text, width = lines.wrap
        let label = paragraph(lines, spec: spec)
        Group {
            if selects {
                label
            } else if lines.hasLinks {
                label.contentShape(Rectangle()).onTapGesture(coordinateSpace: .local) { point in
                    if let href = TextShaper.link(in: runs, lineHeight: spec.lineHeight, wrap: width, at: point) { model.openLink(href) }
                }
            } else {
                label
            }
        }
        .background(alignment: .topLeading) { codeFills(lines, spec: spec) }
        .overlay(alignment: .topLeading) { chips(lines, spec: spec) }
    }

    /// Round 9: inline code's `Markdown/code` boxes, under the text.
    @ViewBuilder
    private func codeFills(_ lines: TextLines, spec: MarkdownTextSpec) -> some View {
        if lines.hasChips {
            let rtl = model.isRTL
            let placed = lines.memoized("code|\(spec.size)|\(spec.weight)|\(spec.lineHeight)|\(spec.family ?? "")|\(rtl)") {
                let baseline = TextShaper.baseline(TextStyle(fontSize: spec.size, fontWeight: spec.weight, lineHeight: spec.lineHeight, fontFamily: spec.family))
                return TextShaper.codeRects(in: lines, lineHeight: spec.lineHeight, baseline: baseline, rtl: rtl)
            }
            if !placed.isEmpty {
                ZStack(alignment: .topLeading) {
                    ForEach(Array(placed.enumerated()), id: \.offset) { _, p in
                        RoundedRectangle(cornerRadius: p.radius)
                            .fill(Color(p.fill))
                            .frame(width: p.rect.width, height: p.rect.height)
                            .offset(x: p.rect.minX, y: p.rect.minY)
                    }
                }
                .frame(width: lines.wrap, alignment: .topLeading)
            }
        }
    }

    /// A text block's label. #VAPP-258: one carrying inline chips paints
    /// line by line at the typesetter's breaks (the measurer's, the chips'),
    /// so TextKit never wraps it differently.
    @ViewBuilder
    private func paragraph(_ lines: TextLines, spec: MarkdownTextSpec) -> some View {
        #if canImport(UIKit)
        if selects {
            // One selectable view per block; a chip text on the measurer's
            // lines (forced separators, no re-wrap).
            let runs = lines.text, width = lines.wrap
            let chipped = lines.hasChips
            SelectableTextLabel(
                attributed: chipped ? lines.memoized("separated") { TextShaper.withLineSeparators(lines) } : runs,
                lineHeight: spec.lineHeight, wraps: !chipped,
                onTap: lines.hasLinks ? { point in
                    if let href = TextShaper.link(in: runs, lineHeight: spec.lineHeight, wrap: width, at: point) { model.openLink(href) }
                } : nil
            )
            .frame(width: width, alignment: .topLeading)
        } else {
            plainParagraph(lines, spec: spec)
        }
        #else
        plainParagraph(lines, spec: spec)
        #endif
    }

    @ViewBuilder
    private func plainParagraph(_ lines: TextLines, spec: MarkdownTextSpec) -> some View {
        if lines.hasChips {
            let runs = lines.text, width = lines.wrap
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(lines.ranges.enumerated()), id: \.offset) { _, range in
                    TextLabel(attributed: runs.attributedSubstring(from: range), lineHeight: spec.lineHeight, wraps: false)
                        .frame(width: width, height: spec.lineHeight)
                }
            }
            .frame(width: width, alignment: .topLeading)
        } else {
            TextLabel(attributed: lines.text, lineHeight: spec.lineHeight)
        }
    }

    private func codeLines(_ code: NSAttributedString, spec: MarkdownTextSpec) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(MarkdownCode.lines(code).enumerated()), id: \.offset) { _, line in
                TextLabel(attributed: line, lineHeight: spec.lineHeight, wraps: false)
                    .frame(height: spec.lineHeight)
            }
        }
    }

    /// The inline chips of a block at the boxes the typesetter laid out.
    @ViewBuilder
    private func chips(_ lines: TextLines, spec: MarkdownTextSpec) -> some View {
        if lines.hasChips {
            let placed = TextShaper.chipRects(in: lines, lineHeight: spec.lineHeight, rtl: model.isRTL)
            if !placed.isEmpty {
                ZStack(alignment: .topLeading) {
                    ForEach(Array(placed.enumerated()), id: \.offset) { _, p in
                        MarkdownChipView(run: p.run, style: styles.chip, model: model, ink: ink, props: props)
                            .offset(x: p.rect.minX, y: p.rect.minY)
                    }
                }
                .frame(width: lines.wrap, alignment: .topLeading)
            }
        }
    }

    /// The runs of a text block (the chips sized for `width`), typeset once:
    /// a `MarkdownText`'s document keeps them per block (`slot`), so a row
    /// a lazy list realizes again shapes nothing.
    private func lines(_ slot: MarkdownParagraphSlot, _ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, ink: Color, link: Color, codeBackground: Color?, width: CGFloat) -> TextLines {
        let key = MarkdownParagraphKey(slot: slot, width: width, ink: ink, link: link, codeBackground: codeBackground, rtl: model.isRTL, epoch: TextFonts.epoch)
        if let hit = paragraphs?.paragraph(key) { return hit }
        let out = TextLines(runs(inlines, spec, ink: ink, link: link, codeBackground: codeBackground, width: width), wrap: width)
        paragraphs?.store(out, for: key)
        return out
    }

    /// The runs of a text block (the chips sized for `width`).
    private func runs(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, ink: Color, link: Color, codeBackground: Color?, width: CGFloat) -> NSAttributedString {
        TextShaper.runs(inlines, spec, mono: model.theme?.monoFamily, ink: platform(ink), link: platform(link), codeBackground: codeBackground.map(platform), chip: styles.chip, wrap: width)
    }

    private func platform(_ c: Color) -> PlatformColor {
        #if canImport(UIKit)
        UIColor(c)
        #else
        NSColor(c)
        #endif
    }

    /// Web's `list-style-position: outside` markers in the text colour: a
    /// disc 0.36em wide whose centre sits 0.46em into the indent, on the
    /// line's middle (Chromium's `disc`, `fixtures/markdown-inline.json`
    /// `documentPaint`); an ordered `3.` ends one space before the item text.
    @ViewBuilder
    private func listMarker(_ marker: String, spec: MarkdownTextSpec) -> some View {
        let font = ExponentialUIFonts.font(family: spec.family, weight: spec.weight, size: spec.size)
        if marker == "•" {
            let d = spec.size * MarkdownBullet.width
            Circle().fill(ink)
                .frame(width: d, height: d)
                .padding(.leading, spec.size * MarkdownBullet.centre - d / 2)
                .padding(.top, (spec.lineHeight - d) / 2)
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
    private func blockView(_ block: MarkdownBlock, index: Int, box: MarkdownBlockBox, link: Color, codeBg: Color?, codeBlockBg: Color?, quoteBorder: Color, muted: Color, border: Color) -> some View {
        let spec = styles.spec(of: block.kind)
        let mono = model.theme?.monoFamily
        let codeBlockPart = model.part("Markdown", "codeBlock", props: props).style
        let codeRadius = codeBlockPart.radius > 0 ? codeBlockPart.radius : (model.theme?.radius("md") ?? 6)
        let codeBlockBorder = codeBlockPart.borderColor
        switch block.kind {
        case .paragraph, .heading:
            linked(lines(MarkdownParagraphSlot(block: index), block.inlines, spec, ink: MarkdownPainter.inlineInk(block.kind, props: props, model: model) ?? ink, link: link, codeBackground: codeBg, width: width), spec: spec)
        case let .image(src, alt):
            let boxWidth = box.width ?? width
            let unsized = block.imageSize == nil
            let image = MediaImage(request: model.mediaRequest(src), options: model.mediaPolicy, appFile: model.appFile) { image in
                image.resizable().scaledToFit().frame(maxWidth: boxWidth, maxHeight: box.height, alignment: .topLeading)
                    // #VAPP-258: the picture decoded while its box was the
                    // placeholder: the leaf measures again at its aspect.
                    .onAppear {
                        guard unsized else { return }
                        if let onImageLoaded { onImageLoaded() } else if let node { model.remeasure(node) }
                    }
            } placeholder: {
                Text(alt).font(.system(size: spec.size)).foregroundStyle(muted).lineLimit(2)
            }
            .frame(width: boxWidth, height: box.height, alignment: .topLeading)
            // #VAPP-258: the `Markdown/image` recipe's corners and border
            // (drawn inside: the box never changes).
            .clipShape(RoundedRectangle(cornerRadius: styles.imageRadius))
            .overlay {
                let frame = model.part("Markdown", "image", props: props).style
                if frame.borderWidth > 0, let color = frame.borderColor {
                    RoundedRectangle(cornerRadius: styles.imageRadius).strokeBorder(color, lineWidth: frame.borderWidth)
                }
            }
            .accessibilityLabel(alt)
            // #VAPP-258: a press calls the host's `onImagePress` (raw src).
            if let press = model.host.onImagePress {
                image.contentShape(Rectangle()).onTapGesture { press(src) }
                    .accessibilityAddTraits(.isButton)
                    .frame(width: width, alignment: .topLeading)
            } else {
                image.frame(width: width, alignment: .topLeading)
            }
        case let .attachment(type, name, href):
            MarkdownAttachmentView(type: type, name: name, href: href, box: CGSize(width: box.width ?? width, height: box.height), styles: styles, model: model, ink: ink)
                .frame(width: width, alignment: .topLeading)
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
                let inner = max(width - styles.listInset(block), 1)
                linked(lines(MarkdownParagraphSlot(block: index), block.inlines, spec, ink: ink, link: link, codeBackground: codeBg, width: inner), spec: spec)
            }
            .padding(.leading, styles.listIndent * CGFloat(block.depth))
        case .quote:
            HStack(alignment: .top, spacing: 0) {
                Rectangle().fill(quoteBorder).frame(width: styles.quoteBorder)
                Color.clear.frame(width: styles.quotePad)
                let inner = max(width - styles.quoteBorder - styles.quotePad, 1)
                linked(lines(MarkdownParagraphSlot(block: index), block.inlines, spec, ink: muted, link: link, codeBackground: codeBg, width: inner), spec: spec)
            }
        case .codeBlock:
            // One non-wrapping label per line (a single-line label would
            // paint only the first): the measurer's height, clipped wide.
            let code = MarkdownCode.highlighted(TextShaper.runs(block.inlines, spec, mono: mono, ink: platform(ink), link: platform(link), codeBackground: nil), lang: block.lang) { kind in model.codeTokenLook(kind).color.map(platform) }
            Group {
                #if canImport(UIKit)
                if selects {
                    SelectableTextLabel(attributed: code, lineHeight: spec.lineHeight, wraps: false)
                } else {
                    codeLines(code, spec: spec)
                }
                #else
                codeLines(code, spec: spec)
                #endif
            }
                .frame(width: max(width - 2 * (styles.codePad + styles.codeBorder), 1), alignment: .topLeading)
                .clipped()
                .padding(styles.codePad + styles.codeBorder)
                .frame(width: width, height: box.height, alignment: .topLeading)
                .background(codeBlockBg ?? ink.opacity(0.06), in: RoundedRectangle(cornerRadius: codeRadius))
                // #VAPP-258: the `Markdown/codeBlock` border, inside the box.
                .overlay {
                    if styles.codeBorder > 0 {
                        RoundedRectangle(cornerRadius: codeRadius).strokeBorder(codeBlockBorder ?? border, lineWidth: styles.codeBorder)
                    }
                }
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
                            let cellText = max(cellW - 2 * styles.cellPadH, 1)
                            let cellLines = lines(MarkdownParagraphSlot(block: index, row: r, column: c), cell, cellSpec, ink: ink, link: link, codeBackground: codeBg, width: cellText)
                            paragraph(cellLines, spec: spec)
                                .overlay(alignment: .topLeading) { chips(cellLines, spec: spec) }
                                .padding(.horizontal, styles.cellPadH)
                                .padding(.vertical, styles.cellPadV)
                                .frame(width: cellW, height: box.rows[safe: r].map { $0 - 1 } ?? spec.lineHeight, alignment: .topLeading)
                        }
                    }
                    // #VAPP-258: the header row's `Markdown/headerCell` fill.
                    .background(r == 0 ? (model.part("Markdown", "headerCell", props: props).style.background ?? Color.clear) : Color.clear)
                    Rectangle().fill(border).frame(height: 1)
                }
            }
            .overlay(alignment: .top) { Rectangle().fill(border).frame(height: 1) }
        }
    }
}

/// A text block's place in its document (a table cell's row and column).
struct MarkdownParagraphSlot: Hashable {
    var block: Int
    var row = -1
    var column = -1
}

/// One typeset text block: its slot at a wrap width in its colours (the
/// document fixes the rest of what its runs depend on).
struct MarkdownParagraphKey: Hashable {
    let slot: MarkdownParagraphSlot
    let width: CGFloat
    let ink: Color
    let link: Color
    let codeBackground: Color?
    let rtl: Bool
    let epoch: UInt64
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

/// #VAPP-258: an inline chip (`catalog/host.json` markdown): the catalog
/// Chip's `root` colours (background, border drawn inside) on a capsule,
/// the icon at `$control.iconSm`, the label in the Text caption (the Chip
/// `label` colour, else the ink), one line; a tap opens its href.
struct MarkdownChipView: View {
    let run: MarkdownChipRun
    let style: MarkdownChipStyle
    let model: SurfaceModel
    let ink: Color
    var props: Props = [:]

    var body: some View {
        // #VAPP-258: the `Markdown/chip*` rules for this kind over Chip's.
        let kind: Props = ["chip": .string(run.chip.kind), "density": .string(props["density"]?.string ?? "document")]
        let own = model.part("Markdown", "chip", props: kind), ownLabel = model.part("Markdown", "chipLabel", props: kind), ownIdent = model.part("Markdown", "chipIdent", props: kind)
        let root = model.part("Chip", "root", props: [:])
        let labelPart = model.part("Chip", "label", props: [:])
        let style = self.style.resolved(run.chip.kind)
        let color = ownLabel.color ?? labelPart.color ?? ink
        let fill = own.style.background ?? root.style.background
        let stroke = own.style.borderColor ?? root.style.borderColor
        let strokeWidth = own.style.borderWidth > 0 ? own.style.borderWidth : root.style.borderWidth
        let border = strokeWidth > 0 ? strokeWidth : (stroke == nil ? 0 : 1)
        let shape = RoundedRectangle(cornerRadius: style.radius ?? run.height / 2)
        HStack(spacing: style.gap) {
            if let icon = run.chip.icon {
                ConceptIcon(name: icon, size: style.icon, color: color, model: model)
            }
            if let ident = run.chip.ident {
                // On the title's baseline (the two fonts centre differently).
                MeasuredLabel(text: ident, ts: TextShaper.chipIdentStyle(style), color: ownIdent.color ?? color).fixedSize()
                    .offset(y: TextShaper.baselineShift(TextShaper.chipLabelStyle(style), TextShaper.chipIdentStyle(style)))
            }
            MeasuredLabel(text: run.chip.label, ts: TextShaper.chipLabelStyle(style), color: color)
        }
        .padding(.horizontal, style.paddingH)
        .frame(width: run.width, height: run.height, alignment: .leading)
        .background(shape.fill(fill ?? Color.clear))
        .overlay(shape.strokeBorder(stroke ?? Color.clear, lineWidth: border))
        .clipShape(shape)
        .contentShape(shape)
        .onTapGesture { model.openLink(run.href) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(run.chip.label)
        .accessibilityAddTraits(.isLink)
    }
}

/// #VAPP-258: an attachment block: `video` = the Video native's face (no
/// poster), `audio` = the AudioPlayer's with the name as its title, `file`
/// = the FileUpload `file` row (fileIcon + fileName) that opens the href.
struct MarkdownAttachmentView: View {
    let type: MarkdownAttachmentKind
    let name: String
    let href: String
    let box: CGSize
    let styles: MarkdownStyles
    let model: SurfaceModel
    let ink: Color

    var body: some View {
        let ts = TextStyle(fontSize: styles.body.size, fontWeight: styles.body.weight, lineHeight: styles.body.lineHeight, fontFamily: styles.body.family)
        switch type {
        case .video:
            VideoFace(model: model, src: href, size: box)
                .accessibilityLabel(name)
        case .audio:
            AudioFace(model: model, title: name, src: href, ink: ink, font: Font(TextFonts.font(family: ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize) as CTFont), titleColor: model.part("AudioPlayer", "track", props: [:]).color, size: box)
        case .file:
            let row = model.part("FileUpload", "file", props: [:])
            let icon = model.part("FileUpload", "fileIcon", props: [:])
            let fileName = model.part("FileUpload", "fileName", props: [:])
            HStack(spacing: styles.fileGap) {
                ConceptIcon(name: BuiltinIcons.name("FileUpload.file"), size: styles.fileIcon, color: icon.color ?? ink.opacity(0.7), model: model)
                MeasuredLabel(text: name, ts: ts, color: fileName.color ?? row.color ?? ink)
                Spacer(minLength: 0)
            }
            .frame(width: box.width, height: box.height, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture { model.openLink(href) }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(name)
            .accessibilityAddTraits(.isLink)
        }
    }
}
