import SwiftUI
import ExponentialUIPrimitives

// The round-1 natives' leaf PARTS (contract §3). The core expands each
// native into recipe parts (`layout_tree/fields.rs`, `table.rs`, `misc.rs`,
// `overlays.rs`) and keeps every value (chips, files, sort, selection, the
// calendar month, the range anchor); these views paint the parts that carry
// content and route the platform input (inline text, drops, context menus)
// back through the core. Mirrors gpui `view/paint.rs` `paint_leaf` /
// `paint_text` and `paint/natives.rs`.

private func platformColor(_ c: Color) -> PlatformColor {
    #if canImport(UIKit)
    UIColor(c)
    #else
    NSColor(c)
    #endif
}

/// A logical `align` prop (`start | center | end`) as a frame alignment and
/// the `TextLabel` alignment name (SwiftUI mirrors leading / trailing).
private func alignment(_ align: String?) -> (Alignment, String?) {
    switch align {
    case "center": (.center, "center")
    case "end", "right": (.trailing, "end")
    default: (.leading, nil)
    }
}

extension LeafContext {
    /// The muted colour of placeholders and secondary glyphs.
    var muted: Color { themeColor("mutedForeground") ?? ink.opacity(0.6) }

    /// The leaf's `align` prop, else its style's `textAlign`.
    var alignName: String? { props["align"]?.string ?? style.textAlign }
}

// MARK: - inline fields

/// A host-owned one-line field inside a native: a NumberField's `input`
/// (locale number text, Up / Down step, adjustable for VoiceOver), a
/// ChipInput's `input` (Enter / comma add a chip, Backspace in an empty
/// field removes the last one), a searchable Select's `search` (its glyph
/// first; `search {query}` after the 150 ms debounce). Same field path as
/// an Input (`Model/Fields.swift`).
struct InlineFieldLeaf: View {
    let cx: LeafContext

    var body: some View {
        let model = cx.model
        let kind = FieldKind(cx.node)
        let owner = cx.ownerProps
        let ph = cx.part(cx.node.recipeComponent, "placeholder")
        let font = ExponentialUIFonts.font(family: cx.textStyle.fontFamily, weight: cx.textStyle.fontWeight, size: cx.textStyle.fontSize)
        let icon: String? = kind == .search ? (cx.props["icon"]?.string ?? BuiltinIcons.name("Select.search")) : nil
        let label = owner.str("label").isEmpty ? cx.props.str("placeholder") : owner.str("label")
        HStack(spacing: icon == nil ? 0 : 8) {
            if let icon {
                ConceptIcon(name: icon, size: 16, color: cx.ink, model: model).opacity(0.6)
            }
            OwnedTextField(
                index: cx.index,
                model: model,
                multiline: false,
                placeholder: cx.props.str("placeholder"),
                font: font,
                color: platformColor(cx.ink),
                placeholderColor: platformColor(ph.color ?? cx.muted),
                lineHeight: cx.textStyle.lineHeight,
                disabled: model.isDisabled(cx.index) || cx.props.flag("disabled"),
                submitsOnReturn: false,
                accessibilityLabel: label,
                secure: false
            )
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

// MARK: - pickers

/// A picker `trigger` (Select, DatePicker, DateRangePicker, TimePicker):
/// the core's text (dates re-formatted in the surface locale from the ISO
/// value), muted while it is the placeholder, and the trigger glyph the
/// core named. The press opens the core's popup LAYER (`triggerFor`).
struct PickerTriggerLeaf: View {
    let cx: LeafContext

    var body: some View {
        let isPlaceholder = cx.props["placeholder"]?.bool ?? cx.props.text("text").isEmpty
        let ph = cx.part(cx.node.recipeComponent, "placeholder")
        let icon = cx.props.str("icon").isEmpty ? "ui-chevron-down" : cx.props.str("icon")
        HStack(spacing: cx.spacing("sm")) {
            TextLabel(text(isPlaceholder), cx.textStyle, color: isPlaceholder ? (ph.color ?? cx.muted) : cx.ink, align: cx.style.textAlign, lines: 1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: cx.textStyle.lineHeight)
            ConceptIcon(name: icon, size: 16, color: cx.ink, model: cx.model).opacity(0.6)
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    private func text(_ isPlaceholder: Bool) -> String {
        let text = cx.props.text("text")
        if isPlaceholder { return text }
        switch cx.node.recipeComponent {
        case "DatePicker":
            return cx.model.formatDate(cx.props.str("value")) ?? text
        case "DateRangePicker":
            guard let start = cx.model.formatDate(cx.props.str("start")) else { return text }
            return "\(start) – \(cx.model.formatDate(cx.props.str("end")) ?? "…")"
        default:
            return text
        }
    }
}

/// A Select option (`item`): [icon] label [check], the check slot always
/// reserved (gpui `text_chrome`).
struct SelectItemLeaf: View {
    let cx: LeafContext

    var body: some View {
        let selected = cx.node.selected || cx.props.flag("selected")
        let check = cx.props.str("check").isEmpty ? BuiltinIcons.name("Select.check") : cx.props.str("check")
        HStack(spacing: max(cx.style.gap, 4)) {
            if let icon = cx.props["icon"]?.string, !icon.isEmpty {
                ConceptIcon(name: icon, size: 16, color: cx.ink, model: cx.model)
            }
            TextLabel(cx.props.text("text"), cx.textStyle, color: cx.ink, lines: 1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: cx.textStyle.lineHeight)
            ZStack {
                if selected { ConceptIcon(name: check, size: 16, color: cx.ink, model: cx.model) }
            }
            .frame(width: 16, height: 16)
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// A DropdownMenu / ContextMenu entry's `itemLabel`: [icon] label (the row,
/// its check, shortcut and submenu indicator are the core's own parts).
struct MenuItemLabelLeaf: View {
    let cx: LeafContext

    var body: some View {
        HStack(spacing: max(cx.style.gap, 4)) {
            if let icon = cx.props["icon"]?.string, !icon.isEmpty {
                ConceptIcon(name: icon, size: 16, color: cx.ink, model: cx.model)
            }
            TextLabel(cx.props.text("text"), cx.textStyle, color: cx.ink, lines: 1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: cx.textStyle.lineHeight)
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

// MARK: - Table

/// A Table `cell` by column type: number / date in the SURFACE locale from
/// the raw `value` (the core's text is the `en` fallback), boolean a tick
/// (else a muted dash), badge the value in a pill, text as is.
struct TableCellLeaf: View {
    let cx: LeafContext

    var body: some View {
        let (frameAlign, textAlign) = alignment(cx.alignName)
        let value = cx.props["value"] ?? .null
        Group {
            switch cx.props.str("cellType") {
            case "boolean":
                if value.displayText == "true" {
                    ConceptIcon(name: "ui-check", size: 16, color: cx.ink, model: cx.model)
                } else {
                    Text("–").font(cx.font).foregroundStyle(cx.muted)
                }
            case "badge" where !cx.props.text("text").isEmpty:
                let badge = SurfaceMeasurer.badgeStyle(cx.textStyle)
                TextLabel(cx.props.text("text"), badge, color: cx.themeColor("secondaryForeground") ?? cx.ink, lines: 1)
                    .fixedSize(horizontal: true, vertical: false)
                    .frame(height: badge.lineHeight)
                    .padding(.horizontal, 8)
                    .background(cx.themeColor("secondary") ?? cx.themeColor("muted") ?? cx.ink.opacity(0.1), in: Capsule())
            default:
                TextLabel(text(value), cx.textStyle, color: cx.ink, align: textAlign, lines: 1)
                    .frame(height: cx.textStyle.lineHeight)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: Alignment(horizontal: frameAlign.horizontal, vertical: .top))
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    private func text(_ value: JSONValue) -> String {
        let fallback = cx.props.text("text")
        switch cx.props.str("cellType") {
        case "number":
            guard let n = value.number ?? Double(value.displayText), !value.isNull, value.displayText != "" else { return fallback }
            return cx.model.formatNumber(n, precision: nil)
        case "date":
            return value.string.flatMap { cx.model.formatDate($0) } ?? fallback
        default:
            return fallback
        }
    }
}

/// A Table `headerCell`: the label and, for a sortable column, the slot of
/// the sort arrow the core named (`Table.sortIcon.asc|desc`).
struct TableHeaderCellLeaf: View {
    let cx: LeafContext

    var body: some View {
        let (frameAlign, textAlign) = alignment(cx.alignName)
        let sortIcon = cx.props.str("sortIcon")
        let slot = !sortIcon.isEmpty || cx.props.flag("sortable")
        HStack(spacing: 4) {
            TextLabel(cx.props.text("text"), cx.textStyle, color: cx.ink, align: textAlign, lines: 1)
                .frame(maxWidth: .infinity, alignment: frameAlign)
                .frame(height: cx.textStyle.lineHeight)
            if slot {
                ZStack {
                    if !sortIcon.isEmpty { ConceptIcon(name: sortIcon, size: 16, color: cx.ink, model: cx.model) }
                }
                .frame(width: 16, height: 16)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

// MARK: - CodeBlock

/// A CodeBlock `code` line: the core tokenizer's tokens (`tokenizeCode`,
/// contract §3), each coloured by the `CodeBlock/token {kind}` recipe
/// (colour, weight, italic) in the line's mono text style.
struct CodeLineLeaf: View {
    let cx: LeafContext

    var body: some View {
        let wraps = cx.ownerProps.flag("wrap")
        TextLabel(attributed: attributed(wraps: wraps), lineHeight: cx.textStyle.lineHeight, lines: wraps ? nil : 1, wraps: wraps)
            .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
            .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    private func attributed(wraps: Bool) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let language = cx.ownerProps.str("language")
        var looks: [String: PartStyle] = [:]
        for token in cx.props.list("tokens") {
            let text = token["text"]?.string ?? ""
            if text.isEmpty { continue }
            let kind = token["kind"]?.string ?? "plain"
            let look = looks[kind] ?? cx.model.part("CodeBlock", "token", props: ["kind": .string(kind), "language": .string(language)])
            looks[kind] = look
            var ts = SurfaceMeasurer.codeStyle(cx.textStyle)
            if let w = look.props.num("fontWeight") { ts.fontWeight = Int(w) }
            var attrs = TextShaper.attributes(ts, italic: look.props.str("fontStyle") == "italic", lineBreak: wraps ? .byWordWrapping : .byClipping)
            attrs[.foregroundColor] = platformColor(look.color ?? cx.ink)
            out.append(NSAttributedString(string: text, attributes: attrs))
        }
        return out
    }
}

// MARK: - platform hooks

/// The platform input a native takes beyond a press (gpui `interactive`):
/// files dropped on a FileUpload `dropzone` (`dragover` meanwhile), a
/// secondary click (macOS) or a long press (touch) on a ContextMenu, which
/// opens the core's menu LAYER at that point.
struct NativeHooks: ViewModifier {
    let node: NodeInfo
    let model: SurfaceModel

    @ViewBuilder
    func body(content: Content) -> some View {
        if node.recipeComponent == "FileUpload", node.part == "dropzone", !model.isDisabled(node.index) {
            content.dropDestination(for: URL.self) { urls, _ in
                model.filesDropped(on: node.id, urls: urls)
                return !urls.isEmpty
            } isTargeted: { on in
                model.setDragover(id: node.id, on)
            }
        } else if node.component == "ContextMenu", node.part == nil, node.layer == 0 {
            content.modifier(ContextMenuGesture(index: node.index, model: model))
        } else {
            content
        }
    }
}

/// Open a ContextMenu at the pointer: a secondary click on macOS, a long
/// press on touch (at the region, like Shift+F10).
private struct ContextMenuGesture: ViewModifier {
    let index: Int
    let model: SurfaceModel

    func body(content: Content) -> some View {
        #if canImport(AppKit) && !targetEnvironment(macCatalyst)
        content.overlay(SecondaryClick { point in
            let origin = model.frame(index).origin
            model.contextMenu(index, at: CGPoint(x: origin.x + point.x, y: origin.y + point.y))
        })
        #else
        content.simultaneousGesture(LongPressGesture(minimumDuration: 0.5).onEnded { _ in
            model.contextMenu(index)
        })
        #endif
    }
}

#if canImport(AppKit) && !targetEnvironment(macCatalyst)
import AppKit

/// A transparent AppKit view that takes ONLY secondary clicks (every other
/// event passes through to the SwiftUI content beneath) and reports the
/// point in its own coordinates (top-left origin).
private struct SecondaryClick: NSViewRepresentable {
    let action: (CGPoint) -> Void

    final class Catcher: NSView {
        var action: ((CGPoint) -> Void)?
        override var isFlipped: Bool { true }

        override func hitTest(_ point: NSPoint) -> NSView? {
            guard let e = NSApp.currentEvent, e.type == .rightMouseDown || (e.type == .leftMouseDown && e.modifierFlags.contains(.control)) else { return nil }
            return super.hitTest(point)
        }

        override func rightMouseDown(with event: NSEvent) { action?(convert(event.locationInWindow, from: nil)) }

        override func mouseDown(with event: NSEvent) { action?(convert(event.locationInWindow, from: nil)) }
    }

    func makeNSView(context: Context) -> Catcher { Catcher() }

    func updateNSView(_ view: Catcher, context: Context) { view.action = action }
}
#endif
