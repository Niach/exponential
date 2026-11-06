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
        // #VAPP-258 step 5: the host's options (hard breaks, a streamed prefix).
        let o = model.host.markdownOptions
        return Markdown.resolveImages(Markdown.parse(text, hardBreaks: o.hardBreaks, streaming: o.streaming)) { model.mediaRequest($0) != nil }
    }

    /// #VAPP-258 step 5: the props a Markdown's recipes resolve with:
    /// `density: chat` added under the host's chat rhythm (a
    /// painter-supplied recipe prop, `catalog/recipes.json`).
    static func props(_ props: Props, chat: Bool) -> Props {
        guard chat else { return props }
        var p = props
        p["density"] = .string("chat")
        return p
    }

    /// `blocks` after the host's Markdown hooks (#VAPP-258, the measurer
    /// and the painter alike): one-link paragraphs the host's `attachment`
    /// classifies become attachment blocks, block images take the host's
    /// `imageSize`, every link passes the URL policy, and the text a host's
    /// `textLinks` links is a link (a valid chip on an allowed href = an
    /// inline chip, whose width the measurer counts).
    @MainActor
    static func painted(_ text: String, model: SurfaceModel) -> [MarkdownBlock] {
        let host = model.host
        let attached = Markdown.resolveAttachments(blocks(text, model: model), href: model.linkHref) { raw in
            host.attachment(raw).map { (kind: $0.kind, name: $0.name) }
        }
        let sized = Markdown.sizeImages(attached) { host.imageSize($0) }
        return Markdown.linkText(Markdown.mapLinks(sized, model.linkHref), chips: { piece in
            model.textLinkSegments(piece)?.map { s in
                (text: s.text, href: s.href, chip: s.chip.map { MarkdownChip(kind: $0.kind, label: $0.label, icon: $0.icon) })
            }
        })
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
        // #VAPP-258 step 5: the chat rhythm re-resolves the root (the node's
        // text style knows no density): its type is the body, its gap the
        // block gap. The document keeps the node's style and the `sm` gap.
        let chat = props["density"]?.string == "chat"
        let root = chat ? part("root") : nil
        if let root { s.body = spec(root, s.body) }
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
        s.blockGap = root?.px("gap") ?? theme.spacing("sm")
        s.headingTop = theme.spacing("md")
        s.headingBottom = theme.spacing("xs")
        s.cellPadH = theme.spacing("sm")
        s.cellPadV = theme.spacing("xs")
        // #VAPP-258: the inline chip (Chip's geometry, the Text caption's
        // type) and the attachment blocks (the media natives' boxes).
        let caption = theme.part("Text", "root", props: ["variant": .string("caption")], mode: mode)
        let captionSpec = MarkdownTextSpec(size: caption.px("fontSize") ?? 12, lineHeight: caption.px("lineHeight") ?? 16, weight: Int(caption.props.num("fontWeight") ?? Double(body.fontWeight)), family: caption.fontFamily ?? body.fontFamily)
        s.chip = MarkdownChipStyle(label: captionSpec, paddingH: theme.spacing("sm"), gap: theme.spacing("xs"), icon: theme.control("iconSm", 16), height: theme.control("badge", 20))
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
    /// typesetter at the block's wrap width. #VAPP-258: its inline chips
    /// paint over their reserved boxes (each opens its href).
    @ViewBuilder
    private func linked(_ runs: NSAttributedString, spec: MarkdownTextSpec, width: CGFloat) -> some View {
        let label = TextLabel(attributed: runs, lineHeight: spec.lineHeight)
        Group {
            if TextShaper.hasLinks(runs) {
                label.contentShape(Rectangle()).onTapGesture(coordinateSpace: .local) { point in
                    if let href = TextShaper.link(in: runs, lineHeight: spec.lineHeight, wrap: width, at: point) { model.openLink(href) }
                }
            } else {
                label
            }
        }
        .overlay(alignment: .topLeading) { chips(runs, spec: spec, width: width) }
    }

    /// The inline chips of `runs` at the boxes the typesetter laid out.
    @ViewBuilder
    private func chips(_ runs: NSAttributedString, spec: MarkdownTextSpec, width: CGFloat) -> some View {
        let placed = TextShaper.chipRects(in: runs, lineHeight: spec.lineHeight, wrap: width, rtl: model.isRTL)
        if !placed.isEmpty {
            ZStack(alignment: .topLeading) {
                ForEach(Array(placed.enumerated()), id: \.offset) { _, p in
                    MarkdownChipView(run: p.run, style: styles.chip, model: model, ink: ink)
                        .offset(x: p.rect.minX, y: p.rect.minY)
                }
            }
            .frame(width: width, alignment: .topLeading)
        }
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
            linked(runs(block.inlines, spec, ink: ink, link: link, codeBackground: codeBg, width: width), spec: spec, width: width)
        case let .image(src, alt):
            let boxWidth = box.width ?? width
            let image = MediaImage(request: model.mediaRequest(src), options: model.mediaPolicy) { image in
                image.resizable().scaledToFit().frame(maxWidth: boxWidth, maxHeight: box.height, alignment: .topLeading)
            } placeholder: {
                Text(alt).font(.system(size: spec.size)).foregroundStyle(muted).lineLimit(2)
            }
            .frame(width: boxWidth, height: box.height, alignment: .topLeading)
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
                linked(runs(block.inlines, spec, ink: ink, link: link, codeBackground: codeBg, width: inner), spec: spec, width: inner)
            }
            .padding(.leading, styles.listIndent * CGFloat(block.depth))
        case .quote:
            HStack(alignment: .top, spacing: 0) {
                Rectangle().fill(quoteBorder).frame(width: styles.quoteBorder)
                Color.clear.frame(width: styles.quotePad)
                let inner = max(width - styles.quoteBorder - styles.quotePad, 1)
                linked(runs(block.inlines, spec, ink: muted, link: link, codeBackground: nil, width: inner), spec: spec, width: inner)
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
                            let cellText = max(cellW - 2 * styles.cellPadH, 1)
                            let cellRuns = runs(cell, cellSpec, ink: ink, link: link, codeBackground: codeBg, width: cellText)
                            TextLabel(attributed: cellRuns, lineHeight: spec.lineHeight)
                                .overlay(alignment: .topLeading) { chips(cellRuns, spec: spec, width: cellText) }
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

/// #VAPP-258: an inline chip (`catalog/host.json` markdown): the catalog
/// Chip's `root` colours (background, border drawn inside) on a capsule,
/// the icon at `$control.iconSm`, the label in the Text caption (the Chip
/// `label` colour, else the ink), one line; a tap opens its href.
struct MarkdownChipView: View {
    let run: MarkdownChipRun
    let style: MarkdownChipStyle
    let model: SurfaceModel
    let ink: Color

    var body: some View {
        let root = model.part("Chip", "root", props: [:])
        let labelPart = model.part("Chip", "label", props: [:])
        let color = labelPart.color ?? ink
        let border = root.style.borderWidth > 0 ? root.style.borderWidth : (root.style.borderColor == nil ? 0 : 1)
        HStack(spacing: style.gap) {
            if let icon = run.chip.icon {
                ConceptIcon(name: icon, size: style.icon, color: color, model: model)
            }
            MeasuredLabel(text: run.chip.label, ts: TextShaper.chipLabelStyle(style), color: color)
        }
        .padding(.horizontal, style.paddingH)
        .frame(width: run.width, height: run.height, alignment: .leading)
        .background(Capsule().fill(root.style.background ?? Color.clear))
        .overlay(Capsule().strokeBorder(root.style.borderColor ?? Color.clear, lineWidth: border))
        .clipShape(Capsule())
        .contentShape(Capsule())
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
