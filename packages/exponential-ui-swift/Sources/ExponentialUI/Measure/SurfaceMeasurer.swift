import Foundation
import ExponentialUICore
import ExponentialUIPrimitives

/// A control's box around its content (the core's `ControlBox`).
struct ControlBox {
    var paddingHorizontal: CGFloat
    var paddingVertical: CGFloat
    var borderWidth: CGFloat
    var gap: CGFloat
    var minWidth: CGFloat?
    var minHeight: CGFloat?
    var width: CGFloat?
    var height: CGFloat?

    init(_ c: FfiControlBox) {
        paddingHorizontal = CGFloat(c.paddingHorizontal)
        paddingVertical = CGFloat(c.paddingVertical)
        borderWidth = CGFloat(c.borderWidth)
        gap = CGFloat(c.gap)
        minWidth = c.minWidth.map { CGFloat($0) }
        minHeight = c.minHeight.map { CGFloat($0) }
        width = c.width.map { CGFloat($0) }
        height = c.height.map { CGFloat($0) }
    }

    init(paddingHorizontal: CGFloat = 0, paddingVertical: CGFloat = 0, borderWidth: CGFloat = 0, gap: CGFloat = 0, minWidth: CGFloat? = nil, minHeight: CGFloat? = nil, width: CGFloat? = nil, height: CGFloat? = nil) {
        self.paddingHorizontal = paddingHorizontal
        self.paddingVertical = paddingVertical
        self.borderWidth = borderWidth
        self.gap = gap
        self.minWidth = minWidth
        self.minHeight = minHeight
        self.width = width
        self.height = height
    }

    /// The horizontal and vertical insets around the content.
    var insets: (CGFloat, CGFloat) { (2 * (paddingHorizontal + borderWidth), 2 * (paddingVertical + borderWidth)) }

    /// The content wrap width inside a border-box wrap (`0` stays min-content).
    func innerWrap(_ wrap: CGFloat?) -> CGFloat? {
        guard let w = wrap else { return nil }
        return w <= 0 ? 0 : max(0, w - insets.0)
    }

    /// Content size → the border box: insets added, fixed sizes win, minimums clamp.
    func borderBox(_ content: CGSize) -> CGSize {
        let (ih, iv) = insets
        let w = max(width ?? (content.width + ih), minWidth ?? 0)
        let h = max(height ?? (content.height + iv), minHeight ?? 0)
        return CGSize(width: w, height: h)
    }
}

/// One leaf the core asks about (the facade's `FfiLeaf`, props parsed).
public struct LeafRequest {
    public let index: Int
    public let id: String
    public let component: String
    public let part: String?
    public let props: Props
    public let text: String
    public let textStyle: TextStyle
    let control: ControlBox
    public let lines: Int?

    init(_ l: FfiLeaf) {
        index = Int(l.index)
        id = l.id
        component = l.component
        part = l.part
        props = JSONValue.parse(l.propsJson).object ?? [:]
        text = l.text
        textStyle = TextStyle(l.textStyle)
        control = ControlBox(l.control)
        lines = l.lines.map { Int($0) }
    }
}

/// The identity the core keys its memo on (a font change = a new id).
private let measureIdentity: UInt64 = 0x5377_6966_7455_4900

/// The painter's `Measurer`: answers the core's BATCHED questions with the
/// TextKit shaper, the recipe boxes and the extension painters. Every
/// answer is the BORDER box of the control (the leaf's `ControlBox` padding
/// and border added around the content; fixed/minimum sizes win), the gpui
/// painter's rules.
final class SurfaceMeasurer: Measurer, @unchecked Sendable {
    let theme: ThemeHandle?
    let mode: Mode
    let extensions: ExtensionRegistry
    /// Extension node id → its kind (`FfiLeaf` carries no kind).
    let kinds: [String: String]
    let generation: UInt64
    private(set) var calls = 0

    init(theme: ThemeHandle?, mode: Mode, extensions: ExtensionRegistry, kinds: [String: String], generation: UInt64) {
        self.theme = theme
        self.mode = mode
        self.extensions = extensions
        self.kinds = kinds
        self.generation = generation
    }

    func measureId() -> UInt64 { measureIdentity &+ generation }

    func measureIntrinsics(leaves: [FfiLeaf]) -> [FfiIntrinsics] {
        leaves.map { raw in
            let leaf = LeafRequest(raw)
            let maxC = answer(leaf, wrap: nil)
            let minC = answer(leaf, wrap: 0)
            return FfiIntrinsics(minContentWidth: Float(min(minC.width, maxC.width)), maxContentWidth: Float(maxC.width), heightAtMaxContent: Float(maxC.height))
        }
    }

    func measureHeights(leaves: [FfiLeaf], requests: [FfiHeightRequest]) -> [Float] {
        var byIndex: [UInt32: LeafRequest] = [:]
        for l in leaves { byIndex[l.index] = LeafRequest(l) }
        return requests.map { r in
            guard let leaf = byIndex[r.index] else { return 0 }
            return Float(answer(leaf, wrap: CGFloat(r.width)).height)
        }
    }

    // MARK: - rules

    private func part(_ component: String, _ part: String, _ props: Props, _ states: [String] = []) -> PartStyle {
        theme?.part(component, part, props: props, states: states, mode: mode) ?? .empty
    }

    private func spacing(_ name: String) -> CGFloat {
        theme.map { $0.spacing(name) } ?? GeometrySpacing.value(name)
    }

    private func control(_ name: String, _ fallback: CGFloat) -> CGFloat {
        theme?.control(name, fallback) ?? fallback
    }

    private func line(_ text: String, _ ts: TextStyle) -> CGFloat {
        TextShaper.maxContent(text, ts)
    }

    /// Text plus fixed-width chrome beside it, one line.
    private func textWithChrome(_ text: String, _ ts: TextStyle, chrome: CGFloat, wrap: CGFloat?) -> CGSize {
        let w = line(text, ts) + chrome
        let used: CGFloat
        switch wrap {
        case .none: used = w
        case .some(let x) where x <= 0: used = chrome
        case .some(let x): used = min(w, x)
        }
        return CGSize(width: used, height: ts.lineHeight)
    }

    func answer(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        calls += 1
        if leaf.component == "Extension" {
            // The pass runs on the main actor (the core calls back
            // synchronously from `layout`); the painters live there too.
            return MainActor.assumeIsolated {
                guard let kind = kinds[leaf.id], let painter = extensions.painter(for: kind) else { return CGSize.zero }
                return painter.measure(ExtensionLeaf(request: leaf, theme: theme, mode: mode), wrap: wrap) ?? .zero
            }
        }
        return measureLeaf(leaf, wrap: wrap)
    }

    func measureLeaf(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        let c = leaf.control
        let inner = c.innerWrap(wrap)
        let props = leaf.props
        let ts = leaf.textStyle
        let content: CGSize
        switch (leaf.component, leaf.part) {
        case ("Text", "tab"):
            let icon: CGFloat = props["icon"]?.string != nil ? 16 + 4 : 0
            let count = props["count"]?.displayText ?? ""
            let countW: CGFloat = count.isEmpty ? 0 : 4 + line(count, ts) + 12
            content = textWithChrome(props.str("text"), ts, chrome: icon + countW, wrap: inner)
        case ("Text", "trigger"):
            var text = props.str("text")
            if let count = props["count"] { text += " · \(count.displayText)" }
            content = textWithChrome(text, ts, chrome: 16 + max(c.gap, 8), wrap: inner)
        case ("Text", "item"):
            let icon: CGFloat = props["icon"]?.string != nil ? 16 + max(c.gap, 8) : 0
            content = textWithChrome(props.str("text"), ts, chrome: icon, wrap: inner)
        case ("Text", _):
            content = TextShaper.measure(props.str("text"), ts, wrap: inner, lines: leaf.lines)
        case ("Markdown", _):
            content = markdown(leaf, wrap: inner)
        case ("Button", _), ("Toggle", _):
            content = button(leaf)
        case ("Link", _):
            let label = props.str("label").isEmpty ? props.str("href") : props.str("label")
            content = CGSize(width: line(label, ts), height: ts.lineHeight)
        case ("Icon", _): content = CGSize(width: 16, height: 16)
        case ("Avatar", _): content = CGSize(width: 32, height: 32)
        case ("Image", _), ("Video", _): content = media(props, wrap: inner, defaultSize: CGSize(width: 320, height: 180))
        case ("AudioPlayer", _):
            let track: CGFloat = props.str("title").isEmpty ? 0 : ts.lineHeight + spacing("xs")
            let w: CGFloat
            switch inner {
            case .none: w = 300
            case .some(let x) where x <= 0: w = 160
            case .some(let x): w = x
            }
            content = CGSize(width: w, height: track + 40)
        case ("Spinner", _): content = CGSize(width: 20, height: 20)
        case ("Ring", _): content = CGSize(width: 32, height: 32)
        case ("Skeleton", _): content = skeleton(props, wrap: inner)
        case ("Chart", _): content = chart(leaf, wrap: inner)
        case ("Composer", _): content = composer(leaf, wrap: inner)
        case ("TreeGuides", _):
            content = CGSize(width: max(props.num("depth") ?? 0, 0) * 16, height: ts.lineHeight)
        case ("ToggleGroup", _): content = toggleGroup(leaf)
        case ("Unknown", _):
            let label = part("Unknown", "label", props)
            let lts = TextStyle(fontSize: label.px("fontSize") ?? 12, fontWeight: 400, lineHeight: label.px("lineHeight") ?? 16, fontFamily: label.fontFamily)
            content = TextShaper.measure(SurfaceMeasurer.unknownLabel(props, leaf.component), lts, wrap: inner, lines: nil)
        case ("Box", "indicator"):
            let n = max(props.num("count") ?? 0, 0)
            let dot = part("Carousel", "indicator", props).width ?? 8
            let gap = spacing("xs")
            content = CGSize(width: n * dot + max(n - 1, 0) * gap, height: dot + spacing("sm"))
        case ("Input", "field"): content = CGSize(width: 160, height: ts.lineHeight)
        case ("Textarea", "field"):
            content = CGSize(width: 160, height: max(props.num("rows") ?? 3, 1) * ts.lineHeight)
        case ("Select", "field"), ("DatePicker", "field"):
            return trigger(leaf, leaf.component)
        case ("Checkbox", "box"), ("Radio", "dot"): content = CGSize(width: 16, height: 16)
        case ("Switch", "track"): content = CGSize(width: 32, height: 20)
        case ("Slider", "track"):
            content = CGSize(width: 160, height: part("Slider", "thumb", props).height ?? control("slider", 16))
        // Geometry mode (no theme): every native is ONE measured leaf.
        case ("Input", nil), ("Select", nil), ("DatePicker", nil): content = CGSize(width: 160, height: 36)
        case ("Textarea", nil): content = CGSize(width: 160, height: 72)
        case ("Switch", nil): content = CGSize(width: 44, height: 24)
        case ("Checkbox", nil), ("Radio", nil): content = CGSize(width: 16, height: 16)
        case ("Slider", nil): content = CGSize(width: 160, height: 16)
        default: content = .zero
        }
        return c.borderBox(content)
    }

    static func unknownLabel(_ props: Props, _ component: String) -> String {
        let name = props.str("component").isEmpty ? component : props.str("component")
        return "Unknown component \(name)"
    }

    private func markdown(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        let blocks = Markdown.parse(leaf.props.str("text"))
        let ts = leaf.textStyle
        let styles = MarkdownPainter.styles(theme: theme, mode: mode, body: ts, props: leaf.props)
        let shaper = MarkdownShaper(mono: theme?.monoFamily)
        let out: CGSize
        switch wrap {
        case .none:
            let w = Markdown.maxContentWidth(blocks, styles, text: shaper)
            out = CGSize(width: w, height: Markdown.layout(blocks, styles, width: w, text: shaper).height)
        case .some(let w) where w <= 0:
            let mw = Markdown.minContentWidth(blocks, styles, text: shaper)
            out = CGSize(width: mw, height: Markdown.layout(blocks, styles, width: mw, text: shaper).height)
        case .some(let w):
            out = CGSize(width: w, height: Markdown.layout(blocks, styles, width: w, text: shaper).height)
        }
        return CGSize(width: out.width, height: max(out.height, blocks.isEmpty ? 0 : ts.lineHeight))
    }

    private func button(_ leaf: LeafRequest) -> CGSize {
        let props = leaf.props
        let ts = leaf.textStyle
        let label = props.str("label")
        let hasIcon = !props.str("icon").isEmpty || props.flag("loading")
        let iconOnly = props.str("size") == "icon"
        let icon: CGFloat = (hasIcon || iconOnly) ? (part(leaf.component, "icon", props).width ?? control("iconSm", 16)) : 0
        let labelW = (iconOnly || label.isEmpty) ? 0 : line(label, ts)
        let gap = (hasIcon && labelW > 0) ? leaf.control.gap : 0
        let w = ((hasIcon || iconOnly) ? icon : 0) + gap + labelW
        return CGSize(width: w, height: max(ts.lineHeight, hasIcon ? icon : 0))
    }

    private func media(_ props: Props, wrap: CGFloat?, defaultSize: CGSize) -> CGSize {
        let ratio = (props.num("aspectRatio").map { CGFloat($0) }).flatMap { $0 > 0 ? $0 : nil } ?? (defaultSize.width / defaultSize.height)
        let fixedW = props.px("width")
        let fixedH = props.px("height")
        let w: CGFloat
        switch (fixedW, wrap) {
        case (.some(let fw), _): w = fw
        case (nil, .none): w = defaultSize.width
        case (nil, .some(let x)) where x <= 0: w = 0
        case (nil, .some(let x)): w = min(x, max(defaultSize.width, x))
        }
        let h = fixedH ?? (w > 0 ? w / ratio : defaultSize.width / ratio)
        return CGSize(width: w, height: h)
    }

    private func skeleton(_ props: Props, wrap: CGFloat?) -> CGSize {
        let w: CGFloat
        if let px = props.px("width") {
            w = px
        } else if let p = props.percent("width") {
            if let x = wrap, x > 0 { w = x * p } else if wrap != nil { w = 0 } else { w = 240 }
        } else {
            switch wrap {
            case .none: w = 240
            case .some(let x) where x > 0: w = x
            default: w = 0
            }
        }
        return CGSize(width: w, height: props.px("height") ?? 16)
    }

    private func chart(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        let props = leaf.props
        let w: CGFloat
        switch wrap {
        case .none: w = 320
        case .some(let x) where x <= 0: w = 0
        case .some(let x): w = x
        }
        var h = CGFloat(props.num("height") ?? 200)
        let gap = spacing("xs")
        if !props.str("title").isEmpty { h += leaf.textStyle.lineHeight + gap }
        if !ChartModel.legend(props).isEmpty {
            h += (part("Chart", "legend", props).px("lineHeight") ?? 16) + gap
        }
        return CGSize(width: w, height: h)
    }

    private func composer(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        let props = leaf.props
        let ts = leaf.textStyle
        let field = part("Composer", "field", props)
        let fs = field.px("fontSize") ?? ts.fontSize
        let lh = field.px("lineHeight") ?? ts.lineHeight
        let minH = field.px("minHeight") ?? lh
        let send = part("Composer", "send", props).height ?? control("buttonIcon", 36)
        let gap = leaf.control.gap
        var text = props.str("value").isEmpty ? props.str("placeholder") : props.str("value")
        if text.isEmpty { text = "Message" }
        let fieldTs = TextStyle(fontSize: fs, fontWeight: ts.fontWeight, lineHeight: lh, fontFamily: ts.fontFamily)
        let w: CGFloat
        switch wrap {
        case .none: w = 320
        case .some(let x) where x <= 0: w = 120
        case .some(let x): w = x
        }
        let th = TextShaper.measure(text, fieldTs, wrap: max(w, 1), lines: nil).height
        return CGSize(width: w, height: min(max(th, minH), 200) + gap + send)
    }

    private func toggleGroup(_ leaf: LeafRequest) -> CGSize {
        let props = leaf.props
        let items = props.list("items")
        let item = part("ToggleGroup", "item", props)
        let pad = item.px("paddingHorizontal") ?? item.px("padding") ?? 12
        let border = item.px("borderWidth") ?? 0
        let h = item.height ?? 36
        let ts = TextStyle(fontSize: item.px("fontSize") ?? leaf.textStyle.fontSize, fontWeight: Int(item.props.num("fontWeight") ?? 500), lineHeight: leaf.textStyle.lineHeight, fontFamily: item.fontFamily)
        var w: CGFloat = 0
        for (i, it) in items.enumerated() {
            let label = it["label"]?.displayText ?? ""
            let hasIcon = it["icon"]?.string != nil
            let lw = label.isEmpty ? 0 : line(label, ts)
            let iw: CGFloat = hasIcon ? 16 : 0
            let innerGap: CGFloat = (hasIcon && lw > 0) ? 6 : 0
            w += 2 * (pad + border) + iw + innerGap + lw
            if i > 0 { w += leaf.control.gap }
        }
        return CGSize(width: w, height: h)
    }

    /// Select / DatePicker `.field`: the TRIGGER recipe (the field has none).
    private func trigger(_ leaf: LeafRequest, _ component: String) -> CGSize {
        let props = leaf.props
        let t = part(component, "trigger", props)
        let fs = t.px("fontSize") ?? leaf.textStyle.fontSize
        let lh = t.px("lineHeight") ?? leaf.textStyle.lineHeight
        let pad = t.px("paddingHorizontal") ?? t.px("padding") ?? 12
        let border = t.px("borderWidth") ?? 0
        let h = t.height ?? (lh + 16)
        let ts = TextStyle(fontSize: fs, fontWeight: 400, lineHeight: lh, fontFamily: t.fontFamily)
        let label = component == "Select" ? SurfaceMeasurer.selectLabel(props) : (SurfaceMeasurer.dateLabel(props.str("value")) ?? (props.str("placeholder").isEmpty ? "Pick a date" : props.str("placeholder")))
        let w = max(line(label, ts) + 2 * (pad + border) + 16 + spacing("sm"), 160)
        return CGSize(width: w, height: h)
    }

    /// A Select's trigger text: the chosen option labels or the placeholder.
    static func selectLabel(_ props: Props) -> String {
        let options = props.list("options")
        let chosen: [String]
        switch props["value"] {
        case .some(.array(let vals)): chosen = vals.map(\.displayText)
        case .some(let v) where !v.isNull: chosen = [v.displayText]
        default: chosen = []
        }
        let labels = options.filter { chosen.contains($0["value"]?.displayText ?? "") }.map { $0["label"]?.displayText ?? "" }
        if labels.isEmpty { return props.str("placeholder").isEmpty ? "Choose" : props.str("placeholder") }
        return labels.joined(separator: ", ")
    }

    /// `"2026-10-14"` → `"Oct 14, 2026"`.
    static func dateLabel(_ value: String) -> String? {
        guard let (y, m, d) = DateModel.parseISO(value) else { return nil }
        let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        return "\(months[m - 1]) \(d), \(y)"
    }
}
