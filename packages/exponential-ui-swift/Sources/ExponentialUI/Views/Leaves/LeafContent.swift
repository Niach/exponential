import SwiftUI
import ExponentialUIPrimitives

/// The content of a measured leaf, dispatched by component and part. The
/// round-1 natives are the core's PARTS (a picker's `trigger`, a calendar's
/// `day`, a NumberField's `input`, a Table's `headerCell` / `cell`, a
/// CodeBlock's `code` line, a menu's `itemLabel`, a toast's `icon`): the
/// core builds and lays them out; each is painted here.
struct LeafContent: View {
    let context: LeafContext

    @ViewBuilder
    var body: some View {
        let cx = context
        let n = cx.node
        let owner = n.recipeComponent
        switch (n.component, n.part) {
        case ("Extension", _):
            ExtensionLeafView(context: cx)
        case _ where SurfaceMeasurer.isInlineField(owner, n.part):
            InlineFieldLeaf(cx: cx)
        case ("Select", "trigger"), ("DatePicker", "trigger"), ("DateRangePicker", "trigger"), ("TimePicker", "trigger"):
            PickerTriggerLeaf(cx: cx)
        case ("Text", "tab"):
            TabLeaf(cx: cx)
        case ("Text", "trigger"):
            AccordionTriggerLeaf(cx: cx)
        case ("Text", "cell") where owner == "Table":
            TableCellLeaf(cx: cx)
        case ("Text", "headerCell"):
            TableHeaderCellLeaf(cx: cx)
        case ("Text", "code") where owner == "CodeBlock" && n.props["tokens"] != nil:
            CodeLineLeaf(cx: cx)
        case ("Text", "item") where owner == "Select":
            SelectItemLeaf(cx: cx)
        case ("Text", "itemLabel"):
            MenuItemLabelLeaf(cx: cx)
        case ("Text", _):
            TextLeaf(cx: cx)
        case ("Markdown", _):
            MarkdownLeaf(cx: cx)
        case ("Button", _), ("Toggle", _):
            ButtonLeaf(cx: cx)
        case ("Link", _):
            LinkLeaf(cx: cx)
        case ("Icon", _):
            IconLeaf(cx: cx)
        case ("Avatar", _):
            AvatarLeaf(cx: cx)
        case ("Image", _):
            ImageLeaf(cx: cx)
        case ("Video", _):
            VideoLeaf(cx: cx)
        case ("AudioPlayer", _):
            AudioLeaf(cx: cx)
        case ("Spinner", _):
            SpinnerView(size: min(cx.inner.width, cx.inner.height), color: cx.ink).frame(width: cx.size.width, height: cx.size.height)
        case ("Ring", _):
            RingLeaf(cx: cx)
        case ("Skeleton", _):
            Color.clear
        case ("Chart", _):
            ChartLeaf(cx: cx)
        case ("TreeGuides", _):
            TreeGuidesLeaf(cx: cx)
        case ("Unknown", _):
            UnknownLeaf(cx: cx)
        case ("Box", "indicator"):
            CarouselIndicatorLeaf(cx: cx)
        case ("Input", "field"), ("Textarea", "field"):
            TextFieldLeaf(cx: cx, multiline: n.component == "Textarea")
        case ("Composer", _):
            ComposerLeaf(cx: cx)
        case ("Checkbox", "box"), ("Checkbox", "checkbox"):
            CheckBoxLeaf(cx: cx)
        case ("Radio", "dot"):
            RadioDotLeaf(cx: cx)
        case ("Switch", "track"):
            SwitchTrackLeaf(cx: cx)
        case ("Slider", "track"):
            SliderTrackLeaf(cx: cx)
        case ("Segmented", _):
            SegmentedLeaf(cx: cx)
        // Geometry mode (no theme): a field native is one leaf showing its
        // value; a Table / CodeBlock / FileUpload its text. (Themed, the
        // natives are containers of parts; a Table stays a leaf that carries
        // its parts, which paint over it.)
        case (let c, nil) where ["Input", "Select", "DatePicker", "Textarea", "NumberField", "TimePicker", "DateRangePicker", "ChipInput"].contains(c) && (cx.model.children[safe: n.index] ?? []).isEmpty:
            let v = n.props["value"]?.displayText ?? ""
            TextLabel(v.isEmpty ? n.props.str("placeholder") : v, cx.textStyle, color: cx.ink, lines: 1)
                .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
                .offset(x: cx.inner.minX, y: cx.inner.minY)
        case (let c, nil) where ["Table", "CodeBlock", "FileUpload"].contains(c) && (cx.model.children[safe: n.index] ?? []).isEmpty:
            TextLabel(n.component == "CodeBlock" ? n.props.str("code") : n.component, cx.textStyle, color: cx.ink)
                .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
                .offset(x: cx.inner.minX, y: cx.inner.minY)
        default:
            Color.clear
        }
    }
}

private struct ExtensionLeafView: View {
    let context: LeafContext

    var body: some View {
        let kind = context.node.extensionKind ?? ""
        if let painter = context.model.extensions.painter(for: kind) {
            painter.paint(ExtensionContext(node: context.node, props: context.props, style: context.style, textStyle: context.textStyle, ink: context.ink, size: context.size, theme: context.model.theme, mode: context.model.mode, model: context.model, children: nil))
        } else {
            Text(kind).font(.system(size: 12)).foregroundStyle(context.ink.opacity(0.6)).frame(width: context.size.width, height: context.size.height, alignment: .topLeading)
        }
    }
}

// MARK: - Text

/// A `Text` leaf: its lines broken EXACTLY as the measurer broke them
/// (`TextShaper.lines` at the content width), each drawn unwrapped one line
/// box tall (gpui `paint_text` + `text_lines`), so painted wraps equal
/// measured wraps. A `lines` clamp keeps the first lines, the last one
/// carrying the rest truncated.
struct TextLeaf: View {
    let cx: LeafContext

    var body: some View {
        let align = cx.style.textAlign ?? cx.props["align"]?.string
        let lh = cx.textStyle.lineHeight
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(Self.shown(cx.props.text("text"), cx.textStyle, width: cx.inner.width, clamp: cx.node.lines).enumerated()), id: \.offset) { _, line in
                TextLabel(line, cx.textStyle, color: cx.ink, align: align, lines: 1)
                    .frame(width: cx.inner.width, height: lh)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
        .clipped()
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    /// The lines a text leaf draws (gpui `text_lines`).
    static func shown(_ text: String, _ ts: TextStyle, width: CGFloat, clamp: Int?) -> [String] {
        let clamp = clamp.flatMap { $0 > 0 ? $0 : nil }
        if clamp == 1 { return [text.replacingOccurrences(of: "\n", with: " ")] }
        let lines = TextShaper.lines(text, ts, wrap: max(width, 1))
        guard let n = clamp, lines.count > n else { return lines }
        return Array(lines[..<(n - 1)]) + [lines[(n - 1)...].joined(separator: " ")]
    }
}

/// A Tabs `tab`: icon + label + count, centred; the indicator under it.
struct TabLeaf: View {
    let cx: LeafContext

    var body: some View {
        let selected = cx.node.selected
        let ind = cx.part("Tabs", "indicator", states: selected ? ["selected"] : [])
        let indH = ind.height ?? 0
        ZStack(alignment: .bottom) {
            // [icon] label [count], `$spacing.xs` apart (the measure's chrome);
            // the count in the label's style, muted.
            HStack(spacing: cx.spacing("xs")) {
                if let icon = cx.props["icon"]?.string {
                    ConceptIcon(name: icon, size: 16, color: cx.ink, model: cx.model)
                }
                Text(cx.textStyle.shown(cx.props.text("text"))).font(cx.font).foregroundStyle(cx.ink).lineLimit(1)
                if let count = cx.props["count"] {
                    Text(count.displayText).font(cx.font).foregroundStyle(cx.themeColor("mutedForeground") ?? cx.ink).lineLimit(1)
                }
            }
            .frame(width: cx.inner.width, height: cx.inner.height)
            .offset(y: 0)
            if selected, indH > 0, let bg = ind.style.background {
                RoundedRectangle(cornerRadius: ind.style.radius).fill(bg).frame(width: cx.size.width, height: indH)
            }
        }
        .frame(width: cx.size.width, height: cx.size.height, alignment: .bottom)
    }
}

/// An Accordion `trigger`: title (+ count) and a chevron that turns open.
struct AccordionTriggerLeaf: View {
    let cx: LeafContext

    var body: some View {
        let title = cx.textStyle.shown(cx.props.text("text"))
        // Round 2 §7: the count is its own muted part after the title.
        return HStack(spacing: max(cx.style.gap, 8)) {
            HStack(spacing: 8) {
                Text(title).font(cx.font).foregroundStyle(cx.ink).lineLimit(1)
                if let count = cx.props["count"] {
                    Text(count.displayText).font(cx.font).foregroundStyle(cx.themeColor("mutedForeground") ?? cx.ink).lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            ConceptIcon(name: BuiltinIcons.name("Accordion.trigger"), size: 16, color: cx.ink, model: cx.model).rotationEffect(.degrees(cx.node.open ? 180 : 0))
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

struct MarkdownLeaf: View {
    let cx: LeafContext

    var body: some View {
        let text = cx.props.text("text")
        if let view = cx.model.host.markdown(text, width: cx.inner.width) {
            view.frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading).offset(x: cx.inner.minX, y: cx.inner.minY)
        } else {
            let styles = MarkdownPainter.styles(theme: cx.model.theme, mode: cx.model.mode, body: cx.textStyle, props: cx.props)
            MarkdownView(text: text, width: cx.inner.width, styles: styles, ink: cx.ink, model: cx.model, props: cx.props)
                .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
                .clipped()
                .offset(x: cx.inner.minX, y: cx.inner.minY)
        }
    }
}

/// Button / Toggle content: spinner or icon, then the label, centred (a
/// CodeBlock `copy` part's `copied` resets through the model's timer).
struct ButtonLeaf: View {
    let cx: LeafContext

    var body: some View {
        let label = cx.props.str("label")
        let icon = cx.props.str("icon")
        let loading = cx.props.flag("loading")
        let iconOnly = cx.props.str("size") == "icon"
        let iconSize = cx.part(cx.node.component, "icon").width ?? cx.control("iconSm", 16)
        HStack(spacing: cx.style.gap) {
            if loading {
                SpinnerView(size: iconSize, color: cx.ink)
            } else if !icon.isEmpty {
                ConceptIcon(name: icon, size: iconSize, color: cx.ink, model: cx.model)
            } else if iconOnly {
                ConceptIcon(name: "", size: iconSize, color: cx.ink, model: cx.model)
            }
            if !iconOnly, !label.isEmpty {
                Text(cx.textStyle.shown(label)).font(cx.font).foregroundStyle(cx.ink).lineLimit(1)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// A Link: an underlined label, centred when the leaf is wider.
struct LinkLeaf: View {
    let cx: LeafContext

    var body: some View {
        let label = cx.props.str("label").isEmpty ? cx.props.str("href") : cx.props.str("label")
        Text(label).font(cx.font).underline().foregroundStyle(cx.ink).lineLimit(1)
            .frame(width: cx.inner.width, height: cx.inner.height)
            .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// The `Unknown` placeholder note.
struct UnknownLeaf: View {
    let cx: LeafContext

    var body: some View {
        let label = cx.part("Unknown", "label")
        let color = label.color ?? cx.themeColor("destructive") ?? cx.ink
        let ts = TextStyle(fontSize: label.px("fontSize") ?? 12, fontWeight: 400, lineHeight: label.px("lineHeight") ?? 16, fontFamily: label.fontFamily)
        TextLabel(SurfaceMeasurer.unknownLabel(cx.props, cx.node.component), ts, color: color)
            .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
            .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}
