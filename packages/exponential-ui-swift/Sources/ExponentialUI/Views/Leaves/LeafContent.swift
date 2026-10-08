import SwiftUI
import ExponentialUIPrimitives

/// The content of a measured leaf, dispatched by component and part.
struct LeafContent: View {
    let context: LeafContext

    var body: some View {
        let cx = context
        let n = cx.node
        switch (n.component, n.part) {
        case ("Extension", _):
            ExtensionLeafView(context: cx)
        case ("Text", "tab"):
            TabLeaf(cx: cx)
        case ("Text", "trigger"):
            AccordionTriggerLeaf(cx: cx)
        case ("Text", "item"):
            MenuItemLeaf(cx: cx)
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
        case ("Select", "field"):
            SelectFieldLeaf(cx: cx)
        case ("DatePicker", "field"):
            DateFieldLeaf(cx: cx)
        case ("Checkbox", "box"):
            CheckBoxLeaf(cx: cx)
        case ("Radio", "dot"):
            RadioDotLeaf(cx: cx)
        case ("Switch", "track"):
            SwitchTrackLeaf(cx: cx)
        case ("Slider", "track"):
            SliderTrackLeaf(cx: cx)
        case ("ToggleGroup", _):
            ToggleGroupLeaf(cx: cx)
        // Geometry mode (no theme): natives are bare boxes.
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

struct TextLeaf: View {
    let cx: LeafContext

    var body: some View {
        let align = cx.style.textAlign ?? cx.props["align"]?.string
        TextLabel(cx.props.str("text"), cx.textStyle, color: cx.ink, align: align, lines: cx.node.lines)
            .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
            .offset(x: cx.inner.minX, y: cx.inner.minY)
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
            HStack(spacing: 4) {
                if let icon = cx.props["icon"]?.string {
                    IconView(name: icon, size: 16, color: cx.ink, model: cx.model)
                }
                Text(cx.props.str("text")).font(cx.font).foregroundStyle(cx.ink).lineLimit(1)
                if let count = cx.props["count"] {
                    Text(count.displayText).font(.system(size: 12)).foregroundStyle(cx.themeColor("mutedForeground") ?? cx.ink).padding(.horizontal, 6)
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
        var title = cx.props.str("text")
        if let count = cx.props["count"] { title += " · \(count.displayText)" }
        return HStack(spacing: max(cx.style.gap, 8)) {
            Text(title).font(cx.font).foregroundStyle(cx.ink).lineLimit(1)
            Spacer(minLength: 0)
            GlyphView(glyph: .chevronDown, size: 16, color: cx.ink).rotationEffect(.degrees(cx.node.open ? 180 : 0))
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// A DropdownMenu `item`: icon + label (destructive tinted).
struct MenuItemLeaf: View {
    let cx: LeafContext

    var body: some View {
        let ink = cx.props.flag("destructive") ? (cx.themeColor("destructive") ?? cx.ink) : cx.ink
        HStack(spacing: max(cx.style.gap, 8)) {
            if let icon = cx.props["icon"]?.string {
                IconView(name: icon, size: 16, color: ink, model: cx.model)
            }
            Text(cx.props.str("text")).font(cx.font).foregroundStyle(ink).lineLimit(1)
            Spacer(minLength: 0)
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

struct MarkdownLeaf: View {
    let cx: LeafContext

    var body: some View {
        let text = cx.props.str("text")
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

/// Button / Toggle content: spinner or icon, then the label, centred.
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
                IconView(name: icon, size: iconSize, color: cx.ink, model: cx.model)
            } else if iconOnly {
                IconView(name: "", size: iconSize, color: cx.ink, model: cx.model)
            }
            if !iconOnly, !label.isEmpty {
                Text(label).font(cx.font).foregroundStyle(cx.ink).lineLimit(1)
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
