import SwiftUI
import ExponentialUIPrimitives

/// #VAPP-258: genUI's catalog `Markdown` outside a surface: the SAME
/// blocks, measurer, layout and painter a `Markdown` node paints with, for
/// a host's own screens (an app's issue comments, its chat). `context` is a
/// surface model whose theme, mode and `HostPlugin` (the `catalog/host.json`
/// markdown hooks: `textLinks` chips, `imageSize`, `onImagePress`,
/// `attachment`, `mediaRequest`, the URL policy, `openUrl`, `icon`) it paints
/// with; it needs no nodes, and one model serves every Markdown of a screen.
/// `options` = this text's render options (hard breaks, the chat density,
/// streaming, the image cap, selection), the model's host's are not read.
/// The type follows Dynamic Type (the theme's sizes × the font scale).
/// `hugsWidth`: as wide as its widest line (a chat bubble), else the
/// proposed width.
public struct MarkdownText: View {
    let text: String
    let context: SurfaceModel
    let options: MarkdownOptions
    let hugsWidth: Bool
    /// What the hooks' answers depend on beyond the text (resolved members,
    /// a synced row): a change parses again.
    let revision: String

    @Environment(\.dynamicTypeSize) private var dynamicType
    /// Unsized pictures that decoded since the last parse (each re-sizes its box).
    @State private var loaded = 0

    public init(_ text: String, context: SurfaceModel, options: MarkdownOptions = MarkdownOptions(), hugsWidth: Bool = false, revision: String = "") {
        self.text = text
        self.context = context
        self.options = options
        self.hugsWidth = hugsWidth
        self.revision = revision
    }

    public var body: some View {
        let look = MarkdownTextLook.resolve(context, options: options, fontScale: CGFloat(PlatformTraits.fontScale(dynamicType)))
        let doc = MarkdownTextCache.document(text, context: context, options: options, styles: look.styles, revision: "\(revision)|\(loaded)")
        if !doc.blocks.isEmpty {
            MarkdownTextFrame(doc: doc, hugs: hugsWidth) {
                GeometryReader { g in
                    MarkdownView(
                        text: text, width: g.size.width, styles: look.styles, ink: look.ink, model: context, props: look.props,
                        options: options, prepared: (doc.blocks, doc.layout(g.size.width)),
                        onImageLoaded: { loaded += 1 }, paragraphs: doc
                    )
                }
            }
            .markdownSelection(options.selectable, text: text, model: context)
        }
    }
}

/// The resolved look of one option set at one type scale.
struct MarkdownTextLook {
    let styles: MarkdownStyles
    let ink: Color
    let props: Props

    @MainActor
    static func resolve(_ model: SurfaceModel, options: MarkdownOptions, fontScale k: CGFloat) -> MarkdownTextLook {
        let props = MarkdownPainter.props([:], chat: options.chat)
        // A node's text style is the core's (the inherited type + the
        // `Markdown/root` recipe, scaled); here the recipe's own, scaled.
        let root = model.part("Markdown", "root", props: [:])
        let body = TextStyle(
            fontSize: (root.px("fontSize") ?? TextStyle.body.fontSize) * k,
            fontWeight: Int(root.props.num("fontWeight") ?? 400),
            lineHeight: (root.px("lineHeight") ?? TextStyle.body.lineHeight) * k,
            fontFamily: root.fontFamily ?? model.theme?.fontFamily("sans")
        )
        let styles = MarkdownPainter.imaged(
            MarkdownPainter.styles(theme: model.theme, mode: model.mode, body: body, props: props, fontScale: k),
            options: options, viewport: model.viewportHeight > 0 ? model.viewportHeight : screenHeight
        )
        return MarkdownTextLook(styles: styles, ink: root.color ?? model.color("foreground") ?? .primary, props: props)
    }
}

/// An uncapped picture stops at the screen's height (no surface viewport here).
@MainActor
private var screenHeight: CGFloat {
    #if canImport(UIKit)
    UIScreen.main.bounds.height
    #else
    0
    #endif
}

/// One parsed text: its painted blocks (the host's hooks applied) and its
/// layouts by width.
@MainActor
final class MarkdownTextDocument {
    let blocks: [MarkdownBlock]
    let styles: MarkdownStyles
    private let shaper: MarkdownShaper
    private var layouts: [CGFloat: MarkdownLayout] = [:]
    private var widest: CGFloat?

    init(blocks: [MarkdownBlock], styles: MarkdownStyles, mono: String?) {
        self.blocks = blocks
        self.styles = styles
        shaper = MarkdownShaper(mono: mono, chip: styles.chip)
    }

    /// Its text blocks as the painter typeset them (`MarkdownView.lines`).
    private var typeset: [MarkdownParagraphKey: TextLines] = [:]

    func paragraph(_ key: MarkdownParagraphKey) -> TextLines? { typeset[key] }

    func store(_ lines: TextLines, for key: MarkdownParagraphKey) {
        if typeset.count >= 4 * max(blocks.count, 16) { typeset.removeAll(keepingCapacity: true) }
        typeset[key] = lines
    }

    func layout(_ width: CGFloat) -> MarkdownLayout {
        let w = max(width, 1)
        if let hit = layouts[w] { return hit }
        let out = Markdown.layout(blocks, styles, width: w, text: shaper)
        if layouts.count > 8 { layouts.removeAll(keepingCapacity: true) }
        layouts[w] = out
        return out
    }

    /// Max-content: the widest block on one line.
    var maxContentWidth: CGFloat {
        if let widest { return widest }
        let w = Markdown.maxContentWidth(blocks, styles, text: shaper)
        widest = w
        return w
    }
}

/// The parsed documents shared by every `MarkdownText` (a lazy list drops a
/// row's state the moment it scrolls off; a re-realized row parses nothing).
@MainActor
enum MarkdownTextCache {
    private final class Entry {
        let doc: MarkdownTextDocument
        init(_ doc: MarkdownTextDocument) { self.doc = doc }
    }

    private static let cache: NSCache<NSString, Entry> = {
        let c = NSCache<NSString, Entry>()
        c.countLimit = 600
        return c
    }()

    static func document(_ text: String, context: SurfaceModel, options: MarkdownOptions, styles: MarkdownStyles, revision: String) -> MarkdownTextDocument {
        // A streamed prefix grows every frame: never cached (the parser
        // memoizes its stable blocks itself).
        if options.streaming {
            return MarkdownTextDocument(blocks: MarkdownPainter.painted(text, model: context, options: options), styles: styles, mono: context.theme?.monoFamily)
        }
        let key = "\(ObjectIdentifier(context).hashValue)|\(options.hardBreaks)|\(options.chat)|\(options.imageMaxHeight.map { "\($0)" } ?? "-")|\(styles.body.size)|\(revision)|\(text)" as NSString
        if let hit = cache.object(forKey: key), hit.doc.styles == styles { return hit.doc }
        let doc = MarkdownTextDocument(blocks: MarkdownPainter.painted(text, model: context, options: options), styles: styles, mono: context.theme?.monoFamily)
        cache.setObject(Entry(doc), forKey: key)
        return doc
    }
}

/// Sizes its one child to the document: the proposed width (or, hugging,
/// the max-content width within it) by the layout's height there.
struct MarkdownTextFrame: Layout {
    let doc: MarkdownTextDocument
    let hugs: Bool

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        MainActor.assumeIsolated {
            let proposed = proposal.width.flatMap { $0.isFinite ? $0 : nil }
            let width: CGFloat
            switch (proposed, hugs) {
            case let (.some(w), false): width = w
            case let (.some(w), true): width = min(w, doc.maxContentWidth)
            case (.none, _): width = doc.maxContentWidth
            }
            return CGSize(width: width, height: doc.layout(width).height)
        }
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for s in subviews { s.place(at: bounds.origin, proposal: ProposedViewSize(bounds.size)) }
    }
}
