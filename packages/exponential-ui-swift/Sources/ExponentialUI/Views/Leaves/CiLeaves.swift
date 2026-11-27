import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives
#if canImport(AppKit)
import AppKit
#endif

// #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md §1,
// §4, §5): the LogLines and JobGraph leaves. The core builds each as ONE
// leaf (its state resolved: `open`, `query`, `follow`, `timestamps`,
// `selection`; JobGraph `compact`) and owns every write; the leaf paints
// from the theme's recipes with the primitives' pure rules (`CiLog`,
// `JobGraph`) and sends `toggle`, `reveal`, `search`, `follow`,
// `timestamps`, `select`.

/// THE status mark (§1): the glyph of `style.json ciStatus` in the
/// recipe's colour; `running` turns once per `spinMs`, still under reduced
/// motion.
struct CiStatusGlyph: View {
    let status: String
    let color: Color
    let model: SurfaceModel
    var size: CGFloat = 16

    var body: some View {
        let entry = CiLog.entry(status)
        let glyph = ConceptIcon(name: entry.icon, size: size, color: color, model: model)
        if entry.spin, !model.reducedMotion {
            TimelineView(.animation) { t in
                let turn = t.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: CiLog.spinMs / 1000) / (CiLog.spinMs / 1000)
                glyph.rotationEffect(.degrees(turn * 360))
            }
            .frame(width: size, height: size)
        } else {
            glyph.frame(width: size, height: size)
        }
    }
}

/// Shared look helpers of the CI leaves.
@MainActor
enum CiLook {
    /// A part's text style over `base` (size, weight, line height, family).
    static func text(_ p: PartStyle, _ base: TextStyle) -> TextStyle {
        var ts = TextStyle(fontSize: p.px("fontSize") ?? base.fontSize, fontWeight: Int(p.props.num("fontWeight") ?? Double(base.fontWeight)), lineHeight: p.px("lineHeight") ?? base.lineHeight, fontFamily: p.fontFamily ?? base.fontFamily)
        ts.tabular = base.tabular
        return ts
    }

    /// Is Shift held (macOS; a touch never extends).
    static var shift: Bool {
        #if canImport(AppKit)
        NSEvent.modifierFlags.contains(.shift)
        #else
        false
        #endif
    }
}

/// A part's box: fill, hairline border, radius.
private struct PartBox: ViewModifier {
    let part: PartStyle

    func body(content: Content) -> some View {
        let s = part.style
        let shape = BoxShape(radius: s.radius)
        content
            .background { if let bg = s.background { shape.fill(bg) } }
            .overlay { if let c = s.borderColor, s.borderWidth > 0 { shape.strokeBorder(c, lineWidth: s.borderWidth) } }
    }
}

private extension View {
    func partBox(_ part: PartStyle) -> some View { modifier(PartBox(part: part)) }
}

// MARK: - LogLines

/// The model, rows, search and ANSI tokens of one LogLines, cached by
/// their inputs (a long log is parsed once, a line tokenized when it shows).
@MainActor
final class LogLinesCache {
    private var linesKey: JSONValue?
    private var sectionsKey: JSONValue?
    private(set) var model = CiLog.Model()
    private var searchKey: String?
    private(set) var matches: [CiLog.Match] = []
    private(set) var matchesByLine: [Int: [Int]] = [:]
    private var tokens: [Int: [JSONValue]] = [:]

    func update(lines: JSONValue?, sections: JSONValue?, query: String) {
        if lines != linesKey || sections != sectionsKey {
            linesKey = lines
            sectionsKey = sections
            model = CiLog.model(lines?.any, sections: sections?.any)
            tokens = [:]
            searchKey = nil
        }
        if query != searchKey {
            searchKey = query
            matches = CiLog.search(model, query: query)
            matchesByLine = [:]
            for (i, m) in matches.enumerated() { matchesByLine[m.line, default: []].append(i) }
        }
    }

    /// The `ansi` tokens of content line `n` (the catalog's ONE tokenizer).
    func tokens(_ n: Int) -> [JSONValue] {
        if let t = tokens[n] { return t }
        guard let line = model.line(n) else { return [] }
        let t = JSONValue.parse(tokenizeCodeJson(code: line.text, language: "ansi")).array?.first?.array ?? []
        tokens[n] = t
        return t
    }
}

struct LogLinesLeaf: View {
    let cx: LeafContext
    @State private var cache = LogLinesCache()
    @State private var current = 0
    @State private var text = ""
    @State private var handledSelection: String?
    @State private var ownSelection: String?
    @State private var hovered: CiLog.Row?
    @FocusState private var searchFocused: Bool

    var body: some View {
        let p = cx.props
        let query = p.str("query")
        let _ = cache.update(lines: p["lines"], sections: p["sections"], query: query)
        let model = cache.model
        let open = p.list("open").compactMap { $0.number.map { Int($0) } }
        let rows = CiLog.rows(model, open: open)
        let root = cx.part("LogLines", "root", props: [:])
        let toolbar = cx.part("LogLines", "toolbar", props: [:])
        let gap = root.style.gap > 0 ? root.style.gap : cx.spacing("sm")
        let barHeight = toolbar.height ?? cx.control("input", 36)
        VStack(alignment: .leading, spacing: gap) {
            toolbarView(height: barHeight, query: query, total: cache.matches.count, follow: p["follow"]?.bool ?? true, timestamps: p.flag("timestamps"))
            bodyView(rows: rows, open: open, height: max(0, cx.size.height - barHeight - gap))
        }
        .frame(width: cx.size.width, height: cx.size.height, alignment: .topLeading)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(cx.model.builtinString("logView"))
        .onAppear { text = query }
        .onChange(of: query) { _, q in
            current = 0
            if q != text { text = q }
        }
    }

    // MARK: toolbar

    @ViewBuilder
    private func toolbarView(height: CGFloat, query: String, total: Int, follow: Bool, timestamps: Bool) -> some View {
        let toolbar = cx.part("LogLines", "toolbar", props: [:])
        let search = cx.part("LogLines", "search", props: [:], states: searchFocused ? ["focus"] : [])
        let countPart = cx.part("LogLines", "count", props: [:])
        let sts = CiLook.text(search, cx.textStyle)
        var cts = CiLook.text(countPart, cx.textStyle)
        let _ = cts.tabular = true
        let shown = total == 0 ? 0 : min(current, total - 1) + 1
        HStack(spacing: toolbar.style.gap > 0 ? toolbar.style.gap : cx.spacing("sm")) {
            TextField(cx.model.builtinString("searchLog"), text: $text)
                .textFieldStyle(.plain)
                .font(Font(ExponentialUIFonts.font(family: sts.fontFamily, weight: sts.fontWeight, size: sts.fontSize, italic: false, tabular: false) as CTFont))
                .foregroundStyle(search.color ?? cx.ink)
                .focused($searchFocused)
                .autocorrectionDisabled()
                .onSubmit { step(CiLook.shift ? -1 : 1) }
                .padding(.horizontal, search.style.paddingHorizontal)
                .frame(minWidth: 48, maxWidth: CiLog.searchWidth, minHeight: search.height ?? height, maxHeight: search.height ?? height)
                .partBox(search)
                .layoutPriority(-1)
                .onChange(of: text) { _, t in
                    guard t != query else { return }
                    fire("search", ["query": .string(t)])
                }
                .accessibilityLabel(cx.model.builtinString("searchLog"))
            if !query.isEmpty {
                MeasuredLabel(text: cx.model.builtinString("matchCount", ["current": .number(Double(shown)), "total": .number(Double(total))]), ts: cts, color: countPart.color ?? cx.ink)
                    .fixedSize()
            }
            stepButton("prev", icon: "ui-chevron-up", label: "previousMatch", enabled: total > 0) { step(-1) }
            stepButton("next", icon: "ui-chevron-down", label: "nextMatch", enabled: total > 0) { step(1) }
            Spacer(minLength: 0)
            toggleButton("follow", on: follow) {
                fire("follow", ["follow": .bool(!follow)])
            }
            toggleButton("timestamps", on: timestamps) {
                fire("timestamps", ["timestamps": .bool(!timestamps)])
            }
        }
        .frame(width: cx.size.width, height: height)
    }

    private func stepButton(_ part: String, icon: String, label: String, enabled: Bool, action: @escaping () -> Void) -> some View {
        let look = cx.part("LogLines", part, props: [:], states: enabled ? [] : ["disabled"])
        let side = look.width ?? cx.control("buttonSm", 32)
        return Button(action: action) {
            ConceptIcon(name: icon, size: cx.control("iconSm", 16), color: look.color ?? cx.ink, model: cx.model)
                .frame(width: side, height: look.height ?? side)
                .partBox(look)
                .opacity(look.style.opacity ?? 1)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(cx.model.builtinString(label))
    }

    private func toggleButton(_ part: String, on: Bool, action: @escaping () -> Void) -> some View {
        let look = cx.part("LogLines", part, props: [:], states: on ? ["selected"] : [])
        let ts = CiLook.text(look, cx.textStyle)
        return Button(action: action) {
            MeasuredLabel(text: cx.model.builtinString(part), ts: ts, color: look.color ?? cx.ink)
                .fixedSize()
                .padding(.horizontal, look.style.paddingHorizontal)
                .frame(height: look.height ?? cx.control("buttonSm", 32))
                .partBox(look)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(cx.model.builtinString(part))
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    // MARK: body

    private func rowHeight(_ row: CiLog.Row) -> CGFloat {
        switch row {
        case .section: cx.part("LogLines", "section", props: [:]).height ?? cx.control("row", 40)
        case .line: lineHeight
        }
    }

    private var lineHeight: CGFloat { cx.part("LogLines", "line", props: [:]).px("lineHeight") ?? 22.75 }

    @ViewBuilder
    private func bodyView(rows: [CiLog.Row], open: [Int], height: CGFloat) -> some View {
        let look = cx.part("LogLines", "body", props: [:])
        let code = SurfaceMeasurer.codeStyle(CiLook.text(look, cx.textStyle))
        let digits = String(cache.model.lines.count).count
        let ch = TextShaper.maxContent("0", code)
        let gutterPart = cx.part("LogLines", "gutter", props: [:])
        let gutterWidth = CGFloat(digits + 1) * ch + 2 * gutterPart.style.paddingHorizontal
        let selection = CiLog.parseAnchor(cx.props["selection"]?.string)
        let viewport = max(0, cx.size.width - 2 * look.style.borderWidth)
        ScrollViewReader { proxy in
            ScrollView([.vertical, .horizontal], showsIndicators: true) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(rows, id: \.self) { row in
                        switch row {
                        case let .section(i):
                            sectionRow(i, open: open.contains(i), width: viewport).id(row)
                        case let .line(n):
                            lineRow(n, code: code, gutter: gutterWidth, selected: selection?.contains(n) == true, minWidth: viewport).id(row)
                        }
                    }
                }
                .padding(.vertical, look.style.paddingVertical)
            }
            .simultaneousGesture(DragGesture(minimumDistance: 8).onChanged { e in
                // A person scrolling UP leaves the end: Follow turns off.
                if e.translation.height > 8, cx.props["follow"]?.bool ?? true { fire("follow", ["follow": .bool(false)]) }
            })
            .onAppear {
                revealSelection(proxy)
                if handledSelection == nil, cx.props["follow"]?.bool ?? true, let last = rows.last { proxy.scrollTo(last, anchor: .bottom) }
            }
            .onChange(of: cache.model.lines.count) { _, _ in
                if cx.props["follow"]?.bool ?? true, let last = rows.last { proxy.scrollTo(last, anchor: .bottom) }
            }
            .onChange(of: cx.props["follow"]?.bool ?? true) { _, on in
                if on, let last = rows.last { proxy.scrollTo(last, anchor: .bottom) }
            }
            .onChange(of: cx.props["selection"]?.string) { _, _ in revealSelection(proxy) }
            .onChange(of: current) { _, _ in scrollToMatch(proxy) }
            .onChange(of: stepRequest) { _, _ in scrollToMatch(proxy) }
        }
        .frame(width: cx.size.width, height: height, alignment: .topLeading)
        .clipShape(BoxShape(radius: look.style.radius))
        .partBox(look)
        .accessibilityElement(children: .contain)
    }

    @State private var stepRequest = 0

    private func sectionRow(_ i: Int, open: Bool, width: CGFloat) -> some View {
        let s = cache.model.sections[i]
        let props: Props = ["status": .string(s.status), "open": .bool(open)]
        let row = CiLog.Row.section(i)
        let look = cx.part("LogLines", "section", props: props, states: hovered == row ? ["hover"] : [])
        let chevron = cx.part("LogLines", "chevron", props: props)
        let icon = cx.part("LogLines", "sectionIcon", props: props)
        let title = cx.part("LogLines", "sectionTitle", props: props)
        let meta = cx.part("LogLines", "sectionMeta", props: props)
        var mts = CiLook.text(meta, cx.textStyle)
        mts.tabular = true
        let iconSize = icon.width ?? cx.control("iconSm", 16)
        return HStack(spacing: look.style.gap > 0 ? look.style.gap : cx.spacing("sm")) {
            ConceptIcon(name: "ui-chevron-right", size: chevron.width ?? iconSize, color: chevron.color ?? cx.ink, model: cx.model)
                .rotationEffect(.degrees(open ? 90 : 0))
            CiStatusGlyph(status: s.status, color: icon.color ?? cx.ink, model: cx.model, size: iconSize)
            TextLabel(s.title, CiLook.text(title, cx.textStyle), color: title.color ?? cx.ink, lines: 1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: CiLook.text(title, cx.textStyle).lineHeight)
            if let d = s.duration {
                MeasuredLabel(text: d, ts: mts, color: meta.color ?? cx.ink).fixedSize()
            }
        }
        .padding(.horizontal, look.style.paddingHorizontal)
        .frame(width: width, height: look.height ?? cx.control("row", 40))
        .background(look.style.background ?? .clear)
        .contentShape(Rectangle())
        .onTapGesture { fire("toggle", ["section": .number(Double(i))]) }
        .onHover { hovered = $0 ? row : (hovered == row ? nil : hovered) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(s.title), \(cx.model.builtinString(CiLog.entry(s.status).label))")
        .accessibilityValue(open ? "expanded" : "collapsed")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { fire("toggle", ["section": .number(Double(i))]) }
    }

    private func lineRow(_ n: Int, code: TextStyle, gutter: CGFloat, selected: Bool, minWidth: CGFloat) -> some View {
        let line = cache.model.line(n)
        let look = cx.part("LogLines", "line", props: [:], states: selected ? ["selected"] : [])
        let gutterLook = cx.part("LogLines", "gutter", props: [:], states: hovered == .line(n) ? ["hover"] : [])
        let timeLook = cx.part("LogLines", "time", props: [:])
        let lh = look.px("lineHeight") ?? 22.75
        var ts = code
        ts.lineHeight = lh
        let indent = (line?.section ?? -1) >= 0 ? CiLog.indent : 0
        return HStack(spacing: 0) {
            Color.clear.frame(width: indent, height: 1)
            MeasuredLabel(text: String(n), ts: ts, color: gutterLook.color ?? cx.ink)
                .opacity(gutterLook.style.opacity ?? 1)
                .padding(.horizontal, gutterLook.style.paddingHorizontal)
                .frame(width: gutter, height: lh, alignment: .trailing)
                .contentShape(Rectangle())
                .onTapGesture { select(n) }
                .onHover { hovered = $0 ? .line(n) : (hovered == .line(n) ? nil : hovered) }
                .accessibilityLabel(cx.model.builtinString("lineAnchor", ["line": .number(Double(n))]))
                .accessibilityAddTraits(.isLink)
            if cx.props.flag("timestamps") {
                MeasuredLabel(text: line?.time ?? "        ", ts: ts, color: timeLook.color ?? cx.ink)
                    .fixedSize()
                    .padding(.horizontal, timeLook.style.paddingHorizontal)
            }
            TextLabel(attributed: attributed(n, kind: line?.kind, ts: ts), lineHeight: lh, lines: 1)
                .fixedSize()
                .frame(height: lh)
            Spacer(minLength: 0)
        }
        .padding(.trailing, look.style.paddingHorizontal)
        .padding(.leading, max(0, look.style.paddingHorizontal - gutterLook.style.paddingHorizontal))
        .frame(minWidth: minWidth, minHeight: lh, maxHeight: lh, alignment: .leading)
        .background(look.style.background ?? .clear)
    }

    /// Line `n`'s ansi tokens in the `token {kind}` colours (a prefixed
    /// line paints every token with the line's kind), the search marks
    /// behind them.
    private func attributed(_ n: Int, kind: String?, ts: TextStyle) -> NSAttributedString {
        let out = NSMutableAttributedString()
        var looks: [String: PartStyle] = [:]
        for token in cache.tokens(n) {
            let t = token["text"]?.string ?? ""
            if t.isEmpty { continue }
            let k = kind ?? token["kind"]?.string ?? "plain"
            let look = looks[k] ?? cx.part("LogLines", "token", props: ["kind": .string(k)])
            looks[k] = look
            var attrs = TextShaper.attributes(ts, italic: false, lineBreak: .byClipping)
            attrs[.foregroundColor] = HighlightPainter.platform(look.color ?? cx.ink)
            out.append(NSAttributedString(string: t, attributes: attrs))
        }
        guard let hits = cache.matchesByLine[n], !hits.isEmpty else { return out }
        let match = cx.part("LogLines", "match", props: [:])
        let currentLook = cx.part("LogLines", "match", props: [:], states: ["selected"])
        let scalars = Array(out.string.unicodeScalars)
        func utf16(_ scalar: Int) -> Int { scalars.prefix(scalar).reduce(0) { $0 + ($1.value > 0xFFFF ? 2 : 1) } }
        for i in hits {
            let m = cache.matches[i]
            let lo = utf16(m.start), hi = utf16(m.end)
            guard hi <= out.length, hi > lo else { continue }
            let fill = (i == current ? currentLook : match).style.background ?? cx.ink.opacity(0.2)
            out.addAttribute(.backgroundColor, value: HighlightPainter.platform(fill), range: NSRange(location: lo, length: hi - lo))
        }
        return out
    }

    // MARK: actions

    private func fire(_ event: String, _ payload: [String: JSONValue]) {
        cx.model.fire(cx.index, event, payload: .object(payload))
    }

    private func select(_ n: Int) {
        let anchor = CiLog.extendAnchor(cx.props["selection"]?.string, line: n, shift: CiLook.shift)
        ownSelection = anchor
        fire("select", ["line": .number(Double(n)), "shift": .bool(CiLook.shift)])
    }

    /// Enter = next, Shift+Enter = previous (both wrap); the match's step
    /// opens and shows its line.
    private func step(_ by: Int) {
        let total = cache.matches.count
        guard total > 0 else { return }
        current = ((current + by) % total + total) % total
        fire("reveal", ["line": .number(Double(cache.matches[current].line))])
        stepRequest += 1
    }

    private func scrollToMatch(_ proxy: ScrollViewProxy) {
        guard cache.matches.indices.contains(current) else { return }
        let line = cache.matches[current].line
        DispatchQueue.main.async { proxy.scrollTo(CiLog.Row.line(line), anchor: .center) }
    }

    /// A new `selection` (initial, or from outside): its start shows in the
    /// top third, its step opens, Follow turns off.
    private func revealSelection(_ proxy: ScrollViewProxy) {
        guard let sel = cx.props["selection"]?.string, sel != handledSelection else { return }
        handledSelection = sel
        if sel == ownSelection { return }
        guard let r = CiLog.parseAnchor(sel) else { return }
        fire("reveal", ["line": .number(Double(r.start))])
        if cx.props["follow"]?.bool ?? true { fire("follow", ["follow": .bool(false)]) }
        DispatchQueue.main.async { proxy.scrollTo(CiLog.Row.line(r.start), anchor: UnitPoint(x: 0, y: 1.0 / 3.0)) }
    }
}

// MARK: - JobGraph

struct JobGraphLeaf: View {
    let cx: LeafContext
    @State private var hovered: String?
    @FocusState private var focused: String?

    var body: some View {
        let p = cx.props
        let compact = p.flag("compact")
        let geometry = JobGraph.geometry
        let layout = JobGraph.layout(p["jobs"]?.any, compact: compact, geometry: geometry)
        Group {
            if compact {
                list(layout)
            } else {
                ScrollView(.horizontal, showsIndicators: true) { graph(layout) }
            }
        }
        .frame(width: cx.size.width, height: cx.size.height, alignment: .topLeading)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(cx.model.builtinString("jobGraph"))
    }

    private var active: String? { hovered ?? focused }

    private func label(_ n: JobGraph.Node) -> String {
        var parts = [n.name, cx.model.builtinString(CiLog.entry(n.status).label)]
        if let c = n.count { parts.append("\(c.passed)/\(c.total)") }
        return parts.joined(separator: ", ")
    }

    private func select(_ id: String) { cx.model.fire(cx.index, "select", payload: .object(["id": .string(id)])) }

    @ViewBuilder
    private func graph(_ layout: JobGraph.Layout) -> some View {
        let upstream = active.map { JobGraph.upstream(layout, $0) } ?? []
        let stage = cx.part("JobGraph", "stage", props: [:])
        let sts = CiLook.text(stage, cx.textStyle)
        let edge = cx.part("JobGraph", "edge", props: [:])
        let lit = Set(upstream)
        ZStack(alignment: .topLeading) {
            ForEach(Array(layout.stages.enumerated()), id: \.offset) { _, s in
                TextLabel(s.title, sts, color: stage.color ?? cx.ink, lines: 1)
                    .frame(width: JobGraph.geometry.nodeWidth, height: JobGraph.geometry.stageHeight, alignment: .leading)
                    .offset(x: s.x, y: 0)
            }
            // Connectors only for the hovered / focused node's upstream set.
            Canvas { ctx, _ in
                for e in layout.edges where lit.contains(e.from) && lit.contains(e.to) {
                    var path = Path()
                    path.move(to: e.start)
                    path.addCurve(to: e.end, control1: e.c1, control2: e.c2)
                    ctx.stroke(path, with: .color(edge.color ?? cx.ink), lineWidth: JobGraph.geometry.edgeWidth)
                }
            }
            .frame(width: layout.width, height: layout.height)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
            ForEach(layout.nodes, id: \.id) { n in
                node(n, highlight: lit.contains(n.id))
                    .offset(x: n.x, y: n.y)
            }
        }
        .frame(width: layout.width, height: layout.height, alignment: .topLeading)
    }

    private func states(_ n: JobGraph.Node, highlight: Bool) -> [String] {
        var s: [String] = []
        if hovered == n.id { s.append("hover") }
        if focused == n.id { s.append("focus") }
        if highlight { s.append("highlight") }
        if cx.props.str("value") == n.id { s.append("selected") }
        return s
    }

    private func node(_ n: JobGraph.Node, highlight: Bool) -> some View {
        let props: Props = ["status": .string(n.status)]
        let look = cx.part("JobGraph", "node", props: props, states: states(n, highlight: highlight))
        return content(n, props: props, look: look, chevron: false)
            .padding(.horizontal, look.style.paddingHorizontal)
            .frame(width: n.width, height: n.height)
            .partBox(look)
            .contentShape(Rectangle())
            .onTapGesture { select(n.id) }
            .onLongPressGesture(minimumDuration: 0.4) { hovered = hovered == n.id ? nil : n.id }
            .onHover { hovered = $0 ? n.id : (hovered == n.id ? nil : hovered) }
            .focusable()
            .focused($focused, equals: n.id)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label(n))
            .accessibilityAddTraits(cx.props.str("value") == n.id ? [.isButton, .isSelected] : .isButton)
            .accessibilityAction { select(n.id) }
    }

    @ViewBuilder
    private func content(_ n: JobGraph.Node, props: Props, look: PartStyle, chevron: Bool) -> some View {
        let icon = cx.part("JobGraph", "icon", props: props)
        let name = cx.part("JobGraph", "name", props: props)
        let meta = cx.part("JobGraph", "meta", props: props)
        let nts = CiLook.text(name, cx.textStyle)
        var mts = CiLook.text(meta, cx.textStyle)
        let _ = mts.tabular = true
        let size = icon.width ?? cx.control("iconSm", 16)
        HStack(spacing: look.style.gap > 0 ? look.style.gap : cx.spacing("sm")) {
            CiStatusGlyph(status: n.status, color: icon.color ?? cx.ink, model: cx.model, size: size)
            TextLabel(n.name, nts, color: name.color ?? cx.ink, lines: 1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: nts.lineHeight)
            if let m = n.meta {
                MeasuredLabel(text: m, ts: mts, color: meta.color ?? cx.ink).fixedSize()
            }
            if chevron {
                let c = cx.part("JobGraph", "chevron", props: props)
                ConceptIcon(name: "ui-chevron-right", size: c.width ?? size, color: c.color ?? cx.ink, model: cx.model)
            }
        }
    }

    private func list(_ layout: JobGraph.Layout) -> some View {
        VStack(spacing: 0) {
            ForEach(layout.nodes, id: \.id) { n in
                let props: Props = ["status": .string(n.status)]
                var s: [String] = []
                let _ = hovered == n.id ? s.append("hover") : ()
                let _ = cx.props.str("value") == n.id ? s.append("selected") : ()
                let look = cx.part("JobGraph", "row", props: props, states: s)
                content(n, props: props, look: look, chevron: true)
                    .padding(.horizontal, look.style.paddingHorizontal)
                    .frame(width: cx.size.width, height: n.height)
                    .background(look.style.background ?? .clear)
                    .overlay(alignment: .bottom) {
                        if let c = look.style.borderColor {
                            Rectangle().fill(c).frame(height: max(look.style.borderWidths[2], look.style.borderWidth, 1 / 3))
                        }
                    }
                    .contentShape(Rectangle())
                    .onTapGesture { select(n.id) }
                    .onHover { hovered = $0 ? n.id : (hovered == n.id ? nil : hovered) }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(label(n))
                    .accessibilityAddTraits(cx.props.str("value") == n.id ? [.isButton, .isSelected] : .isButton)
                    .accessibilityAction { select(n.id) }
            }
        }
    }
}
