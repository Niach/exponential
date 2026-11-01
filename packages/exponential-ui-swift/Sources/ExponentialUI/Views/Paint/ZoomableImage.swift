import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives

public extension ZoomMaths {
    /// EXP-1586: THE zoom maths of the eUI core (the FFI's `zoomJson`,
    /// `fixtures/zoom.json` locks it): hand it to `ZoomableImageView`.
    static let core = ZoomMaths { zoomJson(requestJson: $0) }
}

/// EXP-1586: `ZoomableImage`: the host-loaded picture in the
/// `ZoomableImageView` primitive (every gesture through the core's zoom
/// maths), clipped by the `image` part's radius, THE pill painted from the
/// `pill` / `zoomOut` / `percent` / `zoomIn` / `divider` / `fit` recipe
/// parts (`fit` selected at rest) and THE Menu from the `Menu` recipe; the
/// placeholder while there is no picture. Fires `zoom {scale, atFit}` after
/// each settled zoom change and `tap`; follows a changed `scale` prop; the
/// keys arrive through the surface's focus routing (`zoomCommands`).
struct ZoomableImageLeaf: View {
    let cx: LeafContext
    @Environment(\.xuiReducedMotion) private var surfaceReduced
    @State private var loaded: PlatformImage?
    @State private var loadedKey: String?

    var body: some View {
        let src = cx.props.str("src")
        let alt = cx.props.str("alt")
        let request = src.isEmpty ? nil : cx.model.mediaRequest(src)
        let key = request.map(MediaLoader.key)
        let image = (key != nil && loadedKey == key) ? loaded : request.flatMap { MediaLoader.shared.cached($0) }
        let natural = Self.naturalSize(cx.props, image: image)
        let muted = cx.themeColor("mutedForeground") ?? cx.ink
        let radius = cx.part("ZoomableImage", "image", props: cx.props).style.radius
        Group {
            if let natural {
                ZoomableImageView(
                    naturalSize: natural,
                    maths: .core,
                    initial: ZoomInitial(prop: cx.props.str("initial")),
                    maxScale: cx.props.num("maxScale"),
                    controls: ZoomControls(prop: cx.props.str("controls")),
                    scale: cx.props.num("scale").map { s in Binding(get: { s }, set: { _ in }) },
                    resetID: src,
                    reducedMotion: surfaceReduced || cx.model.reducedMotion,
                    motion: Self.motion(cx.model),
                    label: alt,
                    command: cx.model.zoomCommands[cx.index],
                    cornerRadius: radius,
                    metrics: .standard,
                    fitLabel: cx.model.builtinString("zoomFit"),
                    onZoom: { e in cx.model.fire(cx.index, "zoom", payload: .object(["scale": .number(e.scale), "atFit": .bool(e.atFit)])) },
                    onTap: { cx.model.fire(cx.index, "tap") }
                ) {
                    if let image {
                        MediaImage<Image, EmptyView>.swiftUI(image).resizable().interpolation(.high)
                    } else {
                        ZoomPlaceholder(ink: muted, label: alt, model: cx.model)
                    }
                } pill: { c in
                    ZoomPill(controller: c, style: Self.pillStyle(cx), strings: Self.strings(cx.model)) { g, size, color in
                        AnyView(ConceptIcon(name: g.rawValue, size: size, color: color, model: cx.model))
                    }
                }
            } else {
                ZoomPlaceholder(ink: muted, label: alt, model: cx.model)
                    .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
        .task(id: key) {
            guard let request else {
                loaded = nil
                loadedKey = nil
                return
            }
            let got = await MediaLoader.shared.load(request, options: cx.model.mediaPolicy)
            loaded = got
            loadedKey = key
        }
    }

    /// `naturalWidth` × `naturalHeight`, else the loaded picture's pixels;
    /// nil = nothing to zoom yet.
    static func naturalSize(_ props: Props, image: PlatformImage?) -> CGSize? {
        if let w = props.num("naturalWidth"), let h = props.num("naturalHeight"), w > 0, h > 0 {
            return CGSize(width: w, height: h)
        }
        guard let image else { return nil }
        #if canImport(UIKit)
        let px = CGSize(width: image.size.width * image.scale, height: image.size.height * image.scale)
        #else
        let rep = image.representations.first
        let px = rep.map { CGSize(width: $0.pixelsWide, height: $0.pixelsHigh) } ?? image.size
        #endif
        return px.width > 0 && px.height > 0 ? px : nil
    }

    /// The theme's `motion.standard` on its `ease.standard` (nil = the default).
    static func motion(_ model: SurfaceModel) -> Animation? {
        guard let ms = model.theme?.motion("standard"), ms > 0 else { return nil }
        let e = model.theme?.ease("standard") ?? []
        return e.count == 4 ? .timingCurve(e[0], e[1], e[2], e[3], duration: ms / 1000) : .easeOut(duration: ms / 1000)
    }

    static func strings(_ model: SurfaceModel) -> ZoomStrings {
        ZoomStrings(zoomOut: model.builtinString("zoomOut"), zoomIn: model.builtinString("zoomIn"), zoomToFit: model.builtinString("zoomToFit"), zoomLevel: model.builtinString("zoomLevel"), zoomFit: model.builtinString("zoomFit"))
    }

    /// One pill button's states from its recipe part.
    static func look(_ cx: LeafContext, _ part: String, fallback: Color) -> ZoomButtonLook {
        let base = cx.part("ZoomableImage", part, props: cx.props)
        let hover = cx.part("ZoomableImage", part, props: cx.props, states: ["hover"])
        let selected = cx.part("ZoomableImage", part, props: cx.props, states: ["selected"])
        let disabled = cx.part("ZoomableImage", part, props: cx.props, states: ["disabled"])
        let height = base.height ?? 28
        return ZoomButtonLook(
            width: base.width,
            height: height,
            radius: min(base.style.radius, height / 2),
            paddingHorizontal: base.px("paddingHorizontal") ?? base.style.paddingHorizontal,
            foreground: base.color ?? fallback,
            background: base.style.background ?? .clear,
            hoverForeground: hover.color ?? base.color ?? fallback,
            hoverBackground: hover.style.background ?? base.style.background ?? .clear,
            selectedForeground: selected.color ?? base.color ?? fallback,
            selectedBackground: selected.style.background ?? base.style.background ?? .clear,
            disabledOpacity: disabled.style.opacity ?? 0.5,
            glyphSize: 16
        )
    }

    /// THE pill and THE Menu from the theme's recipes.
    static func pillStyle(_ cx: LeafContext) -> ZoomPillStyle {
        let fg = cx.themeColor("foreground") ?? cx.ink
        let pill = cx.part("ZoomableImage", "pill", props: cx.props)
        let percentPart = cx.part("ZoomableImage", "percent", props: cx.props)
        let divider = cx.part("ZoomableImage", "divider", props: cx.props)
        let shadow = pill.style.shadows.last
        var metrics = ZoomPillMetrics.standard
        if let p = pill.px("padding") { metrics.padding = p }
        if let g = pill.px("gap") { metrics.gap = g }
        if let h = divider.height { metrics.dividerHeight = h }
        let ps = percentPart.style
        let percentFont = Font(ExponentialUIFonts.font(family: ps.fontFamily ?? percentPart.fontFamily, weight: ps.fontWeight ?? 500, size: ps.fontSize ?? 14, tabular: true) as CTFont)
        let content = cx.part("Menu", "content", props: [:])
        let item = cx.part("Menu", "item", props: [:])
        let itemHover = cx.part("Menu", "item", props: [:], states: ["hover"])
        let check = cx.part("Menu", "check", props: [:])
        let menuShadow = content.style.shadows.last
        let itemFont = Font(ExponentialUIFonts.font(family: item.style.fontFamily ?? item.fontFamily, weight: item.style.fontWeight ?? 400, size: item.style.fontSize ?? 14, tabular: true) as CTFont)
        let popoverFg = cx.themeColor("popoverForeground") ?? fg
        let menu = ZoomMenuLook(
            background: content.style.background ?? cx.themeColor("popover") ?? .clear,
            border: content.style.borderColor ?? cx.themeColor("border") ?? .clear,
            borderWidth: content.style.borderWidth,
            radius: content.style.radius,
            padding: content.px("padding") ?? cx.spacing("xs"),
            shadow: menuShadow?.color ?? .clear,
            shadowRadius: (menuShadow?.blur ?? 0) / 2,
            shadowY: menuShadow?.y ?? 0,
            minWidth: 128,
            itemHeight: item.height ?? 32,
            itemPaddingHorizontal: item.px("paddingHorizontal") ?? cx.spacing("sm"),
            itemRadius: item.style.radius,
            itemGap: item.px("gap") ?? cx.spacing("sm"),
            itemForeground: item.color ?? popoverFg,
            itemHoverForeground: itemHover.color ?? item.color ?? popoverFg,
            itemHoverBackground: itemHover.style.background ?? .clear,
            checkSize: check.width ?? 16,
            checkColor: check.color ?? fg,
            font: itemFont
        )
        return ZoomPillStyle(
            metrics: metrics,
            background: pill.style.background ?? cx.themeColor("popover") ?? .clear,
            border: pill.style.borderColor ?? .clear,
            borderWidth: pill.style.borderWidth,
            glass: pill.style.backdropBlur != nil,
            shadow: shadow?.color ?? .clear,
            shadowRadius: (shadow?.blur ?? 0) / 2,
            shadowY: shadow?.y ?? 0,
            button: look(cx, "zoomOut", fallback: fg),
            fit: look(cx, "fit", fallback: fg),
            percent: look(cx, "percent", fallback: fg),
            percentFont: percentFont,
            divider: divider.style.background ?? cx.themeColor("border") ?? fg.opacity(0.2),
            dividerWidth: divider.width ?? 1,
            menu: menu
        )
    }
}

/// The picture area before (or without) a picture: the image glyph and the
/// alt text over a light tint — Compose's `ZoomableImage` overlay and web's
/// box. Its own view: EXP-1555 made the `Image` leaf's placeholder glyph-only.
private struct ZoomPlaceholder: View {
    let ink: Color
    let label: String
    let model: SurfaceModel

    var body: some View {
        VStack(spacing: 4) {
            ConceptIcon(name: BuiltinIcons.name("Image.fallback"), size: 20, color: ink.opacity(0.6), model: model)
            if !label.isEmpty {
                Text(label).font(.system(size: 12)).foregroundStyle(ink.opacity(0.6)).lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(ink.opacity(0.08))
    }
}
