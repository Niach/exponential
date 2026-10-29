import Foundation
import ExponentialUICore
import ExponentialUIPrimitives

/// A control's box around its content (the core's `ControlBox`): every side
/// of the padding (logical keys already resolved by the direction), the
/// border, the gap and the recipe's fixed / minimum sizes.
struct ControlBox {
    var paddingHorizontal: CGFloat
    var paddingVertical: CGFloat
    var paddingTop: CGFloat
    var paddingRight: CGFloat
    var paddingBottom: CGFloat
    var paddingLeft: CGFloat
    var borderWidth: CGFloat
    var gap: CGFloat
    var minWidth: CGFloat?
    var minHeight: CGFloat?
    var width: CGFloat?
    var height: CGFloat?

    init(_ c: FfiControlBox) {
        paddingHorizontal = CGFloat(c.paddingHorizontal)
        paddingVertical = CGFloat(c.paddingVertical)
        paddingTop = CGFloat(c.paddingTop)
        paddingRight = CGFloat(c.paddingRight)
        paddingBottom = CGFloat(c.paddingBottom)
        paddingLeft = CGFloat(c.paddingLeft)
        borderWidth = CGFloat(c.borderWidth)
        gap = CGFloat(c.gap)
        minWidth = c.minWidth.map { CGFloat($0) }
        minHeight = c.minHeight.map { CGFloat($0) }
        width = c.width.map { CGFloat($0) }
        height = c.height.map { CGFloat($0) }
        // An older core sends only the averages: spread them over the sides.
        if paddingTop + paddingRight + paddingBottom + paddingLeft == 0, paddingHorizontal + paddingVertical > 0 {
            (paddingTop, paddingBottom) = (paddingVertical, paddingVertical)
            (paddingLeft, paddingRight) = (paddingHorizontal, paddingHorizontal)
        }
    }

    /// Symmetric padding (each side = its axis' value).
    init(paddingHorizontal: CGFloat = 0, paddingVertical: CGFloat = 0, borderWidth: CGFloat = 0, gap: CGFloat = 0, minWidth: CGFloat? = nil, minHeight: CGFloat? = nil, width: CGFloat? = nil, height: CGFloat? = nil) {
        self.paddingHorizontal = paddingHorizontal
        self.paddingVertical = paddingVertical
        paddingTop = paddingVertical
        paddingBottom = paddingVertical
        paddingLeft = paddingHorizontal
        paddingRight = paddingHorizontal
        self.borderWidth = borderWidth
        self.gap = gap
        self.minWidth = minWidth
        self.minHeight = minHeight
        self.width = width
        self.height = height
    }

    /// The horizontal and vertical insets around the content: the two
    /// sides of each axis plus the border on both (gpui `measure::insets`).
    var insets: (CGFloat, CGFloat) {
        (paddingLeft + paddingRight + 2 * borderWidth, paddingTop + paddingBottom + 2 * borderWidth)
    }

    /// The top inset (padding + border): where a text leaf's first line starts.
    var topInset: CGFloat { paddingTop + borderWidth }

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
    /// The native or macro owning a part leaf (`Tabs`, `Stepper`); nil for a plain node.
    public let ownerComponent: String?

    init(_ l: FfiLeaf, defaultFamily: String? = nil) {
        index = Int(l.index)
        id = l.id
        component = l.component
        part = l.part
        props = JSONValue.parse(l.propsJson).object ?? [:]
        text = l.text
        textStyle = TextStyle(l.textStyle, defaultFamily: defaultFamily)
        control = ControlBox(l.control)
        lines = l.lines.map { Int($0) }
        ownerComponent = l.ownerComponent
    }

    init(index: Int = 0, id: String = "leaf", component: String, part: String? = nil, props: Props = [:], textStyle: TextStyle = .body, control: ControlBox = ControlBox(), lines: Int? = nil) {
        self.index = index
        self.id = id
        self.component = component
        self.part = part
        self.props = props
        text = props.str("text")
        self.textStyle = textStyle
        self.control = control
        self.lines = lines
        ownerComponent = nil
    }
}

/// The identity the core keys its memo on (a font change = a new id).
private let measureIdentity: UInt64 = 0x5377_6966_7455_4900

/// Content size plus the first baseline from the CONTENT top (gpui `Content`).
struct MeasuredContent {
    var width: CGFloat
    var height: CGFloat
    var baseline: CGFloat?

    init(_ w: CGFloat, _ h: CGFloat, _ baseline: CGFloat? = nil) {
        width = w
        height = h
        self.baseline = baseline
    }

    init(_ s: CGSize) {
        self.init(s.width, s.height)
    }
}

/// The painter's `Measurer`: answers the core's BATCHED questions with the
/// TextKit shaper, the recipe boxes and the extension painters. Every
/// answer is the BORDER box of the control (the leaf's `ControlBox` padding
/// and border added around the content; fixed/minimum sizes win), and text
/// leaves also answer their FIRST BASELINE (`alignItems: baseline`). The
/// rules are gpui's `measure.rs`, rule for rule, so the frames match the
/// desktop and the web.
final class SurfaceMeasurer: Measurer, @unchecked Sendable {
    let theme: ThemeHandle?
    let mode: Mode
    let extensions: ExtensionRegistry
    /// Extension node id → its kind (`FfiLeaf` carries no kind).
    let kinds: [String: String]
    let generation: UInt64
    private(set) var calls = 0
    /// Whether a markdown image's src passes the host's media policy (the
    /// painter asks the same question).
    let mediaAllowed: (String) -> Bool

    /// The model's answers from earlier passes (nil: measure everything).
    let memo: MeasureMemo?

    init(theme: ThemeHandle?, mode: Mode, extensions: ExtensionRegistry, kinds: [String: String], generation: UInt64, memo: MeasureMemo? = nil, mediaAllowed: @escaping (String) -> Bool = { _ in true }) {
        self.mediaAllowed = mediaAllowed
        self.memo = memo
        memo?.stamp(theme: theme, mode: mode, generation: generation)
        self.theme = theme
        self.mode = mode
        self.extensions = extensions
        self.kinds = kinds
        self.generation = generation
    }

    /// A new font set (`TextFonts.install`) or a settings change re-keys
    /// the core's measure memo.
    func measureId() -> UInt64 { measureIdentity &+ generation &+ (TextFonts.epoch << 32) }

    func measureIntrinsics(leaves: [FfiLeaf]) -> [FfiIntrinsics] {
        leaves.map { raw in
            let key = MeasureMemo.key(raw)
            if let key, let hit = memo?.intrinsics[key] { return hit }
            let leaf = LeafRequest(raw, defaultFamily: theme?.sansFamily)
            let maxC = answer(leaf, wrap: nil)
            let minC = answer(leaf, wrap: 0)
            let out = FfiIntrinsics(minContentWidth: Float(min(minC.width, maxC.width)), maxContentWidth: Float(maxC.width), heightAtMaxContent: Float(maxC.height), baseline: maxC.baseline.map { Float($0) })
            if let key { memo?.intrinsics[key] = out }
            return out
        }
    }

    func measureHeights(leaves: [FfiLeaf], requests: [FfiHeightRequest]) -> [Float] {
        var raws: [UInt32: FfiLeaf] = [:]
        for l in leaves { raws[l.index] = l }
        // A leaf's request is built once, on the first miss.
        var requestsByIndex: [UInt32: LeafRequest] = [:]
        return requests.map { r in
            guard let raw = raws[r.index] else { return 0 }
            let key = MeasureMemo.key(raw).map { MeasureMemo.HeightKey(leaf: $0, width: r.width) }
            if let key, let hit = memo?.heights[key] { return hit }
            let leaf = requestsByIndex[r.index] ?? LeafRequest(raw, defaultFamily: theme?.sansFamily)
            requestsByIndex[r.index] = leaf
            let h = Float(answer(leaf, wrap: CGFloat(r.width)).height)
            if let key { memo?.heights[key] = h }
            return h
        }
    }

    // MARK: - helpers

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
        TextShaper.maxContent(ts.shown(text), ts)
    }

    /// Where the first baseline sits in a line box (`TextShaper.baseline`:
    /// half-leading + ascent, what `TextLabel` paints).
    static func baseline(_ ts: TextStyle) -> CGFloat { TextShaper.baseline(ts) }

    /// A one-line text with fixed chrome beside it (`lead`, `trail`).
    private func row(_ text: String, _ ts: TextStyle, lead: CGFloat, trail: CGFloat, wrap: CGFloat?) -> MeasuredContent {
        let w = lead + line(text, ts) + trail
        let used: CGFloat
        switch wrap {
        case .none: used = w
        case .some(let x) where x <= 0: used = lead + trail
        case .some(let x): used = min(w, x)
        }
        return MeasuredContent(used, ts.lineHeight, Self.baseline(ts))
    }

    /// Plain text at an inner wrap width (with the `lines` clamp).
    private func para(_ text: String, _ ts: TextStyle, wrap: CGFloat?, lines: Int?) -> MeasuredContent {
        let s = TextShaper.measure(text, ts, wrap: wrap, lines: lines)
        return MeasuredContent(s.width, s.height, Self.baseline(ts))
    }

    func answer(_ leaf: LeafRequest, wrap: CGFloat?) -> MeasuredContent {
        calls += 1
        if leaf.component == "Extension" {
            // The pass runs on the main actor (the core calls back
            // synchronously from `layout`); the painters live there too.
            let size = MainActor.assumeIsolated { () -> CGSize in
                guard let kind = kinds[leaf.id], let painter = extensions.painter(for: kind) else { return CGSize.zero }
                return painter.measure(ExtensionLeaf(request: leaf, theme: theme, mode: mode), wrap: wrap) ?? .zero
            }
            return MeasuredContent(size)
        }
        return measureLeaf(leaf, wrap: wrap)
    }

    /// One leaf at one border-box wrap width: its border box and baseline.
    func measureLeaf(_ leaf: LeafRequest, wrap: CGFloat?) -> MeasuredContent {
        let c = leaf.control
        let inner = c.innerWrap(wrap)
        let props = leaf.props
        let ts = leaf.textStyle
        let content: MeasuredContent
        switch (leaf.component, leaf.part) {
        case _ where Self.isInlineField(leaf.component, leaf.part):
            content = inlineField(leaf)
        case ("Select", "trigger"), ("DatePicker", "trigger"), ("DateRangePicker", "trigger"), ("TimePicker", "trigger"):
            content = trigger(leaf)
        case ("Text", let part):
            let count = props["count"].map(\.displayText) ?? ""
            let countW = count.isEmpty ? 0 : line(count, ts)
            // The core names the owner (`FfiLeaf.ownerComponent`, round 2); the
            // part-name guess stays for an owner this table does not know.
            let owner = leaf.ownerComponent.flatMap { Self.chromeOwners.contains($0) ? $0 : nil } ?? Self.textOwner(part, props)
            let chrome = Self.textChrome(owner, part, props, gap: c.gap, countWidth: countW, xs: spacing("xs"))
            // A bound number / boolean reads as its display string (the
            // core's `FfiLeaf.text`, contract round 2 §3).
            let raw = leaf.text.isEmpty ? props.text("text") : leaf.text
            if chrome != (0, 0) {
                content = row(raw, ts, lead: chrome.0, trail: chrome.1, wrap: inner)
            } else if part == "cell", props.str("cellType") == "boolean" {
                content = MeasuredContent(16, ts.lineHeight)
            } else if part == "cell", props.str("cellType") == "badge" {
                let badge = Self.badgeStyle(ts)
                let m = para(raw, badge, wrap: nil, lines: 1)
                content = MeasuredContent(m.width + 16, max(m.height, ts.lineHeight), m.baseline)
            } else if part == "code", props["tokens"] != nil {
                // A blank code line still holds one line box (the web's).
                let code = Self.codeStyle(ts)
                let m = para(raw, code, wrap: inner, lines: leaf.lines)
                content = MeasuredContent(m.width, max(m.height, code.lineHeight), m.baseline)
            } else {
                content = para(raw, ts, wrap: inner, lines: leaf.lines)
            }
        case ("Markdown", _):
            content = markdown(leaf, wrap: inner)
        case ("Button", _), ("Toggle", _):
            content = button(leaf)
        case ("Link", _):
            let label = props.str("label").isEmpty ? props.str("href") : props.str("label")
            content = MeasuredContent(line(label, ts), ts.lineHeight, Self.baseline(ts))
        case ("Icon", _): content = MeasuredContent(16, 16)
        case ("Avatar", _): content = MeasuredContent(32, 32)
        case ("Image", _): content = MeasuredContent(media(props, wrap: inner, defaultSize: Self.mediaDefault))
        case ("Video", _):
            // Without an aspect ratio or a height the web's `<video>` keeps
            // its default 150 px box height at any width.
            // Round 2 §7: `aspectRatio` (default 16:9), never a browser default.
            content = MeasuredContent(media(props, wrap: inner, defaultSize: Self.mediaDefault))
        case ("AudioPlayer", _):
            let track: CGFloat = props.str("title").isEmpty ? 0 : ts.lineHeight + spacing("xs")
            let w: CGFloat
            switch inner {
            case .none: w = 300
            case .some(let x) where x <= 0: w = 160
            case .some(let x): w = x
            }
            // Round 2 §7: the title line + xs + a controls row of `$control.row`.
            content = MeasuredContent(w, track + control("row", Self.audioControlsHeight))
        case ("Spinner", _): content = MeasuredContent(20, 20)
        case ("Ring", _): content = MeasuredContent(32, 32)
        // NEW-77: the root recipe's edge (sm 32 · md 56 · lg 160 · xl 200) wins.
        // VAPP-162: `variant: bars` = the meter box (style.json voiceWave.bars).
        case ("VoiceWave", _):
            content = props.str("variant") == "bars" ? MeasuredContent(Self.voiceWaveBarsBox(props.str("size"))) : MeasuredContent(56, 56)
        case ("QrCode", _): content = MeasuredContent(qrCode(leaf, wrap: inner))
        case ("QrScanner", _): content = MeasuredContent(qrScanner(leaf, wrap: inner))
        case ("Skeleton", _): content = MeasuredContent(skeleton(props, wrap: inner))
        case ("Chart", _): content = MeasuredContent(chart(leaf, wrap: inner))
        case ("Composer", _): content = MeasuredContent(composer(leaf, wrap: inner))
        case ("TreeGuides", _):
            // depth × `treeGuideColumn` wide; no height of its own (it
            // stretches to its row, round 2 §7).
            content = MeasuredContent(max(props.num("depth") ?? 0, 0) * Self.treeGuideColumn, 0)
        case ("Segmented", _): content = MeasuredContent(segmented(leaf))
        case ("Unknown", _):
            let label = part("Unknown", "label", props)
            let lts = TextStyle(fontSize: label.px("fontSize") ?? 12, fontWeight: 400, lineHeight: label.px("lineHeight") ?? 16, fontFamily: label.fontFamily)
            content = para(SurfaceMeasurer.unknownLabel(props, leaf.component), lts, wrap: inner, lines: nil)
        case ("Box", "indicator"):
            let n = max(props.num("count") ?? 0, 0)
            // The dots row: the recipe sizes each DOT, the row is one dot tall.
            let recipe = part("Carousel", "indicator", props)
            let dot = recipe.width ?? 8
            let gap = spacing("xs")
            content = MeasuredContent(n * dot + max(n - 1, 0) * gap, recipe.height ?? dot)
        case ("Input", "field"):
            content = MeasuredContent(160, ts.lineHeight, Self.baseline(ts))
        case ("Textarea", "field"):
            let s = textarea(leaf, inner: inner)
            content = MeasuredContent(s.width, s.height, Self.baseline(ts))
        case ("Checkbox", "box"), ("Checkbox", "checkbox"), ("Radio", "dot"): content = MeasuredContent(16, 16)
        case ("Switch", "track"): content = MeasuredContent(32, 20)
        case ("Slider", "track"):
            content = MeasuredContent(160, part("Slider", "thumb", props).height ?? control("slider", 16))
        // Geometry mode (no theme): every native is ONE measured leaf.
        case ("Input", nil), ("Select", nil), ("DatePicker", nil), ("NumberField", nil), ("TimePicker", nil), ("DateRangePicker", nil), ("ChipInput", nil):
            content = MeasuredContent(160, 36)
        case ("Textarea", nil): content = MeasuredContent(160, 72)
        case ("Switch", nil): content = MeasuredContent(44, 24)
        case ("Checkbox", nil), ("Radio", nil): content = MeasuredContent(16, 16)
        case ("Slider", nil): content = MeasuredContent(160, 16)
        case ("Table", nil):
            let rows = CGFloat(props.list("rows").count)
            let w: CGFloat = inner.flatMap { $0 > 0 ? $0 : nil } ?? 320
            content = MeasuredContent(w, (rows + 1) * 36)
        case ("CodeBlock", nil):
            var mono = ts
            mono.fontFamily = "ui-monospace"
            content = para(props.str("code"), mono, wrap: nil, lines: nil)
        case ("FileUpload", nil): content = MeasuredContent(240, 96)
        case ("ChoiceTiles", nil):
            // EXP-1280: an unexpanded ChoiceTiles (geometry mode) = its grid
            // of tiles (the core's FixedMeasure rule): per row a picture
            // (when any option has one, 1.6 : 1) over two lines, 8 padding,
            // 8 gaps.
            let options = props.list("options")
            let cols = min(max((props.num("columns") ?? 2).rounded(), 1), 4)
            let rows = (CGFloat(options.count) / cols).rounded(.up)
            let pictures = options.contains { !($0["image"]?.string ?? "").isEmpty }
            let w: CGFloat = inner.flatMap { $0 > 0 ? $0 : nil } ?? 320
            let tileW = (w - 8 * (cols - 1)) / cols - 18
            let tileH = (pictures ? tileW / 1.6 + 4 : 0) + 2 * ts.lineHeight + 4 + 18
            content = MeasuredContent(w, max(rows * tileH + max(rows - 1, 0) * 8, 0))
        default: content = MeasuredContent(0, 0)
        }
        let box = c.borderBox(CGSize(width: content.width, height: content.height))
        let baseline = content.baseline.map { b -> CGFloat in
            let fixed = c.height.map { abs($0 - (content.height + c.insets.1)) > 0.5 } ?? false
            let centred = fixed || ["trigger", "field", "input", "search"].contains(leaf.part ?? "") || leaf.component == "Button" || leaf.component == "Toggle"
            // A control centres its line in its box.
            return centred ? max(0, (box.height - content.height) / 2) + b : c.topInset + b
        }
        return MeasuredContent(box.width, box.height, baseline)
    }

    // MARK: - rules

    /// Is a leaf a host-owned one-line field (a searchable Select's
    /// `search`, NumberField / ChipInput `input`)?
    static func isInlineField(_ component: String, _ part: String?) -> Bool {
        switch (component, part) {
        case ("Select", "search"), ("SearchList", "search"), ("NumberField", "input"), ("ChipInput", "input"): true
        default: false
        }
    }

    /// The owners whose text parts carry chrome (`textChrome`).
    static let chromeOwners: Set<String> = ["Tabs", "Accordion", "Select", "TimePicker", "Menu", "Table"]

    /// The owner of a `Text` part when the core names none: the part names
    /// are unique per owner; a `Select` option always carries `disabled`
    /// (a TimePicker time does not).
    static func textOwner(_ part: String?, _ props: Props) -> String {
        switch part {
        case "tab": "Tabs"
        case "trigger": "Accordion"
        case "item": props["disabled"] != nil ? "Select" : "TimePicker"
        case "itemLabel": "Menu"
        case "headerCell", "cell": "Table"
        default: "Text"
        }
    }

    /// A prop that is set (not null, not "").
    static func has(_ props: Props, _ key: String) -> Bool {
        guard let v = props[key] else { return false }
        if v.isNull { return false }
        if case .string(let s) = v, s.isEmpty { return false }
        return true
    }

    /// The leading / trailing chrome a one-line text part carries beside its
    /// text (an icon, a count, a chevron, a check, a sort arrow), in px.
    static func textChrome(_ owner: String, _ part: String?, _ props: Props, gap: CGFloat, countWidth: CGFloat, xs: CGFloat = 4) -> (CGFloat, CGFloat) {
        let icon: (String) -> CGFloat = { has(props, $0) ? 16 + max(gap, 4) : 0 }
        switch (owner, part) {
        // The web's tab body: [icon] label [count], `$spacing.xs` apart.
        case ("Tabs", "tab"): return (has(props, "icon") ? 16 + xs : 0, has(props, "count") ? xs + countWidth : 0)
        // Round 2 §7: the count is its own muted part after the title.
        case ("Accordion", "trigger"): return (0, (has(props, "count") ? 8 + countWidth : 0) + 16 + max(gap, 8))
        case ("Select", "item"): return (icon("icon"), 16 + max(gap, 8))
        case ("Menu", "itemLabel"): return (icon("icon"), 0)
        case ("Table", "headerCell"): return (0, has(props, "sortIcon") || props.flag("sortable") ? 16 + 4 : 0)
        default: return (0, 0)
        }
    }

    /// A CodeBlock line's text style: the theme's mono family when the
    /// platform has it, else the system monospace (gpui maps a mono family it
    /// cannot load to the platform's mono, never to the sans).
    static func codeStyle(_ ts: TextStyle) -> TextStyle {
        var c = ts
        // The installed font set first (`NSFontManager`'s family list is
        // read once per process: a family registered later never shows).
        if let f = ts.fontFamily, TextFonts.resolve(f).family == f || ExponentialUIFonts.isAvailable(f) { return c }
        c.fontFamily = "ui-monospace"
        return c
    }

    /// A badge cell's text: two px smaller, at least 10.
    static func badgeStyle(_ ts: TextStyle) -> TextStyle {
        var b = ts
        b.fontSize = max(ts.fontSize - 2, 10)
        return b
    }

    static func unknownLabel(_ props: Props, _ component: String) -> String {
        let name = props.str("component").isEmpty ? component : props.str("component")
        return "Unknown component \(name)"
    }

    /// The `lines` clamp of a Markdown leaf (nil = every line).
    static func markdownClamp(_ props: Props) -> Int? {
        guard let n = props.num("lines"), n > 0 else { return nil }
        return Int(n)
    }

    private func markdown(_ leaf: LeafRequest, wrap: CGFloat?) -> MeasuredContent {
        let blocks = Markdown.resolveImages(Markdown.parse(leaf.props.str("text")), allowed: mediaAllowed)
        let ts = leaf.textStyle
        let styles = MarkdownPainter.styles(theme: theme, mode: mode, body: ts, props: leaf.props)
        let shaper = MarkdownShaper(mono: theme?.monoFamily)
        let width: CGFloat
        switch wrap {
        case .none: width = Markdown.maxContentWidth(blocks, styles, text: shaper)
        case .some(let w) where w <= 0: width = Markdown.minContentWidth(blocks, styles, text: shaper)
        case .some(let w): width = w
        }
        var height = Markdown.layout(blocks, styles, width: width, text: shaper).height
        // `lines`: the first n body lines (the web's -webkit-line-clamp).
        if let n = Self.markdownClamp(leaf.props) { height = min(height, CGFloat(n) * ts.lineHeight) }
        return MeasuredContent(width, max(height, blocks.isEmpty ? 0 : ts.lineHeight), Self.baseline(ts))
    }

    private func button(_ leaf: LeafRequest) -> MeasuredContent {
        let props = leaf.props
        let ts = leaf.textStyle
        let label = props.str("label")
        let loading = props.flag("loading")
        let hasIcon = Self.has(props, "icon") || loading
        let iconOnly = props.str("size") == "icon"
        let icon: CGFloat = (hasIcon || iconOnly) ? (part(leaf.component, "icon", props).width ?? control("iconSm", 16)) : 0
        let labelW = (iconOnly || label.isEmpty) ? 0 : line(label, ts)
        let gap = (hasIcon && labelW > 0) ? leaf.control.gap : 0
        var w = ((hasIcon || iconOnly) ? icon : 0) + gap + labelW
        // EXP-1259: the pending face (the spinner + `pendingLabel`) never
        // narrows the button below its idle width, nor clips the word.
        let pending = props.str("pendingLabel")
        if loading, !iconOnly, !pending.isEmpty { w = max(w, icon + leaf.control.gap + line(pending, ts)) }
        let h = max(ts.lineHeight, hasIcon ? icon : 0)
        return MeasuredContent(w, h, labelW > 0 ? (h - ts.lineHeight) / 2 + Self.baseline(ts) : nil)
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

    /// A chart: the box's width (320 max-content, a sparkline 120), the
    /// plot `height` (a sparkline 32) plus the title and legend rows.
    private func chart(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        let props = leaf.props
        let spark = props.str("kind") == "sparkline"
        let w: CGFloat
        switch wrap {
        case .none: w = spark ? 120 : 320
        case .some(let x) where x <= 0: w = 0
        case .some(let x): w = x
        }
        var h = CGFloat(props.num("height") ?? (spark ? 32 : 200))
        let gap = spacing("xs")
        if !props.str("title").isEmpty { h += leaf.textStyle.lineHeight + gap }
        if !ChartModel.legend(props).isEmpty {
            h += (part("Chart", "legend", props).px("lineHeight") ?? 16) + gap
        }
        return CGSize(width: w, height: h)
    }

    /// The composer: its text wrapped at the field width, clamped to the
    /// field's min height and 200 px, plus the send bar.
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
        // An empty field (no placeholder either) is one line tall.
        if text.isEmpty { text = " " }
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

    /// A Textarea field: `rows` lines, or (`autosize`) its text's wrapped
    /// lines clamped to `rows ... maxRows`.
    private func textarea(_ leaf: LeafRequest, inner: CGFloat?) -> CGSize {
        let props = leaf.props
        let ts = leaf.textStyle
        let rows = CGFloat(max(props.num("rows") ?? 3, 1).rounded(.down))
        let w: CGFloat = 160
        guard props.flag("autosize") else { return CGSize(width: w, height: rows * ts.lineHeight) }
        let maxRows = props.num("maxRows").map { CGFloat($0) } ?? .infinity
        let at = inner.flatMap { $0 > 0 ? $0 : nil } ?? w
        let value = props.str("value")
        let lines = value.isEmpty ? 1 : (TextShaper.wrappedHeight(TextShaper.attributed(value, ts), lineHeight: ts.lineHeight, wrap: at) / ts.lineHeight).rounded()
        return CGSize(width: w, height: min(max(lines, rows), max(maxRows, rows)) * ts.lineHeight)
    }

    /// A host-owned one-line field: its text (or placeholder) and a caret
    /// (the search field's glyph at the start).
    private func inlineField(_ leaf: LeafRequest) -> MeasuredContent {
        let props = leaf.props
        let typed = props.str("text")
        let value = typed.isEmpty ? (props["value"]?.displayText ?? "") : typed
        let ph = props.str("placeholder")
        let ts = leaf.textStyle
        let w = max(line(value, ts), line(ph, ts)) + 2
        let icon: CGFloat = leaf.part == "search" && Self.has(props, "icon") ? 16 + 8 : 0
        return MeasuredContent(max(w, 24) + icon, ts.lineHeight, Self.baseline(ts))
    }

    /// The `bar` Segmented's caption (round 3, gpui `bar_caption`): the
    /// `Text` caption recipe's size and line height over the base style,
    /// no tracking or case.
    static func barCaption(_ caption: PartStyle, base: TextStyle) -> TextStyle {
        TextStyle(fontSize: caption.px("fontSize") ?? 12, fontWeight: base.fontWeight, lineHeight: caption.px("lineHeight") ?? 16, fontFamily: base.fontFamily, italic: base.italic)
    }

    /// A `bar` Segmented (round 3, the old TabBar): `$control.tabBar` tall,
    /// each item a COLUMN (icon over a caption label) sharing the width; its
    /// min-content width = the widest of icon and label per item, plus `xs`
    /// on both sides (gpui `segmented_bar`).
    private func segmentedBar(_ leaf: LeafRequest) -> CGSize {
        let ts = Self.barCaption(part("Text", "root", ["variant": .string("caption")]), base: leaf.textStyle)
        let icon = control("iconMd", 20)
        let pad = spacing("xs")
        var w: CGFloat = 0
        for it in leaf.props.list("items") {
            let label = it["label"]?.displayText ?? ""
            let lw = label.isEmpty ? 0 : line(label, ts)
            let iw: CGFloat = it["icon"]?.string != nil ? icon : 0
            w += max(lw, iw) + 2 * pad
        }
        return CGSize(width: w, height: control("tabBar", 56))
    }

    /// VAPP-162 (round-5 contract §3): THE navigation rail (a vertical
    /// `bar`): the root recipe's width (72) × one `$control.tabBar` item
    /// per option (the core sets the height too).
    private func segmentedRail(_ leaf: LeafRequest) -> CGSize {
        let width = part("Segmented", "root", leaf.props).width ?? 72
        return CGSize(width: width, height: CGFloat(leaf.props.list("items").count) * control("tabBar", 56))
    }

    private func segmented(_ leaf: LeafRequest) -> CGSize {
        let props = leaf.props
        let vertical = props.str("orientation") == "vertical"
        if props.str("variant") == "bar" { return vertical ? segmentedRail(leaf) : segmentedBar(leaf) }
        let items = props.list("items")
        let item = part("Segmented", "item", props)
        let box = item.toggleItemBox
        let pad = box.paddingHorizontal
        let border = item.px("borderWidth") ?? 0
        let h = box.height
        let ts = TextStyle(fontSize: item.px("fontSize") ?? leaf.textStyle.fontSize, fontWeight: Int(item.props.num("fontWeight") ?? 500), lineHeight: leaf.textStyle.lineHeight, fontFamily: item.fontFamily, letterSpacing: leaf.textStyle.letterSpacing, textTransform: leaf.textStyle.textTransform)
        var w: CGFloat = 0
        for (i, it) in items.enumerated() {
            let label = it["label"]?.displayText ?? ""
            let hasIcon = it["icon"]?.string != nil
            let lw = label.isEmpty ? 0 : line(label, ts)
            let iw: CGFloat = hasIcon ? 16 : 0
            // Icon ↔ label: the `Segmented/item` recipe's gap (none = 0, as the web and gpui).
            let innerGap: CGFloat = (hasIcon && lw > 0) ? (item.px("gap") ?? 0) : 0
            let itemW = 2 * (pad + border) + iw + innerGap + lw
            if vertical {
                // VAPP-162: a column of segments, as wide as the widest.
                w = max(w, itemW)
            } else {
                w += itemW
                if i > 0 { w += leaf.control.gap }
            }
        }
        if vertical {
            let n = CGFloat(items.count)
            return CGSize(width: w, height: n * h + max(n - 1, 0) * leaf.control.gap)
        }
        return CGSize(width: w, height: h)
    }

    /// VAPP-162 (round-5 contract §5): the `bars` meter box per size:
    /// 20 bars × barWidth + 19 gaps wide, `height` tall.
    static func voiceWaveBarsBox(_ size: String) -> CGSize {
        switch size {
        case "sm": CGSize(width: 78, height: 16)
        case "lg": CGSize(width: 156, height: 40)
        case "xl": CGSize(width: 195, height: 56)
        default: CGSize(width: 117, height: 24)
        }
    }

    /// The QrCode `label` part's text style (a muted caption).
    static func qrLabelStyle(_ p: PartStyle, base: TextStyle) -> TextStyle {
        return TextStyle(fontSize: p.px("fontSize") ?? 12, fontWeight: Int(p.props.num("fontWeight") ?? 400), lineHeight: p.px("lineHeight") ?? 16, fontFamily: p.fontFamily ?? base.fontFamily)
    }

    /// The QrScanner `hint` part's text style.
    static func qrHintStyle(_ p: PartStyle, base: TextStyle) -> TextStyle {
        return TextStyle(fontSize: p.px("fontSize") ?? base.fontSize, fontWeight: Int(p.props.num("fontWeight") ?? 400), lineHeight: p.px("lineHeight") ?? base.lineHeight, fontFamily: p.fontFamily ?? base.fontFamily)
    }

    /// VAPP-162 (round-5 contract §6): the `size` px plate, then (with a
    /// `label`) the root gap (`labelGap` 8) and the caption, centred; as
    /// wide as the wider of the two (the label wraps at the wrap width).
    private func qrCode(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        let size = max(0, leaf.props.num("size") ?? 160)
        let label = leaf.props.str("label")
        guard !label.isEmpty else { return CGSize(width: size, height: size) }
        let ts = Self.qrLabelStyle(part("QrCode", "label", leaf.props), base: leaf.textStyle)
        let m = TextShaper.measure(label, ts, wrap: wrap.map { max($0, size) }, lines: nil)
        return CGSize(width: max(size, m.width), height: size + leaf.control.gap + m.height)
    }

    /// VAPP-162 (round-5 contract §7): as wide as it is given (the
    /// viewfinder a square up to `maxWidth` 280 inside), then (with a
    /// `hint`) the root gap (12) and the hint.
    private func qrScanner(_ leaf: LeafRequest, wrap: CGFloat?) -> CGSize {
        let width: CGFloat = wrap.flatMap { $0 > 0 ? $0 : nil } ?? QrScannerGeometry.maxWidth
        let finder = QrScannerGeometry.viewfinder(width: width)
        let hint = leaf.props.str("hint")
        guard !hint.isEmpty else { return CGSize(width: width, height: finder) }
        let m = TextShaper.measure(hint, Self.qrHintStyle(part("QrScanner", "hint", leaf.props), base: leaf.textStyle), wrap: width, lines: nil)
        return CGSize(width: width, height: finder + leaf.control.gap + m.height)
    }

    /// A picker trigger (Select / DatePicker / DateRangePicker / TimePicker
    /// `trigger`): the core's `text` + its glyph, `gap: sm` between (the web's
    /// `.xui-<C>-trigger`; the recipe's padding and border come from the
    /// control box).
    private func trigger(_ leaf: LeafRequest) -> MeasuredContent {
        let ts = leaf.textStyle
        let text = leaf.props.str("text").isEmpty ? leaf.props.str("placeholder") : leaf.props.str("text")
        let gap = max(leaf.control.gap, spacing("sm"))
        return MeasuredContent(line(text, ts) + gap + 16, ts.lineHeight, Self.baseline(ts))
    }

    /// The web's `<audio controls>` bar height (Chromium).
    /// The AudioPlayer controls row when the theme has no `$control.row`.
    static let audioControlsHeight: CGFloat = 32
    /// `catalog/layout.json` (round 2 §7): `mediaIntrinsicWidth` 320 at
    /// `mediaAspectRatio` 16:9; round 3: `treeGuideColumn` 14, `treeGuideRadius`
    /// 3 (the elbow corner), `treeGuideBridge` 1 (verticals overshoot the
    /// row's top, paint only).
    static let mediaDefault = CGSize(width: 320, height: 320 / 1.7777778)
    static let treeGuideColumn: CGFloat = 14
    static let treeGuideRadius: CGFloat = 3
    static let treeGuideBridge: CGFloat = 1
}

/// VAPP-142: the measurer's answers kept across passes, per model. A
/// leaf's answer is a pure function of its record (minus the slot and the
/// id), the theme, the mode, the measure generation and the installed
/// fonts: a re-windowed list row, a rebound template item with a text
/// seen before, a Table's repeated badge pay a dictionary lookup instead
/// of TextKit. Extensions (their painter decides) and Markdown (its
/// images pass the host's media policy) are never memoised. Cleared when
/// any input of the answer changes, and when it outgrows `limit`.
final class MeasureMemo: @unchecked Sendable {
    struct HeightKey: Hashable {
        var leaf: FfiLeaf
        var width: Float
    }

    var intrinsics: [FfiLeaf: FfiIntrinsics] = [:]
    var heights: [HeightKey: Float] = [:]
    private var inputs: (theme: ObjectIdentifier?, mode: Mode, generation: UInt64, fonts: UInt64)?
    /// Entries per table before it starts over (a 100k list keeps ~2 per row it showed).
    static let limit = 32_768

    /// The record as the memo keys it: slot and id dropped (nil = never memoised).
    static func key(_ leaf: FfiLeaf) -> FfiLeaf? {
        if leaf.component == "Extension" || leaf.component == "Markdown" { return nil }
        var k = leaf
        k.index = 0
        k.id = ""
        return k
    }

    /// The pass's inputs: a change empties the memo.
    func stamp(theme: ThemeHandle?, mode: Mode, generation: UInt64) {
        let now = (theme: theme.map(ObjectIdentifier.init), mode: mode, generation: generation, fonts: TextFonts.epoch)
        if let i = inputs, i.theme == now.theme, i.mode == now.mode, i.generation == now.generation, i.fonts == now.fonts {
            if intrinsics.count > Self.limit { intrinsics.removeAll(keepingCapacity: true) }
            if heights.count > Self.limit { heights.removeAll(keepingCapacity: true) }
            return
        }
        inputs = now
        intrinsics.removeAll()
        heights.removeAll()
    }
}
