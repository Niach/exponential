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
/// first; `search {query}` after the 150 ms debounce).
struct InlineFieldLeaf: View {
    let cx: LeafContext

    var body: some View {
        let model = cx.model
        let field = model.inlineField(cx.index)
        let kind = field?.kind ?? .search
        let owner = cx.ownerProps
        let ph = cx.part(cx.node.recipeComponent, "placeholder")
        let font = ExponentialUIFonts.font(family: cx.textStyle.fontFamily, weight: cx.textStyle.fontWeight, size: cx.textStyle.fontSize)
        let icon: String? = kind == .search ? (cx.props["icon"]?.string ?? BuiltinIcons.name("Select.search")) : nil
        let label = owner.str("label").isEmpty ? cx.props.str("placeholder") : owner.str("label")
        let disabled = model.isDisabled(cx.index) || cx.props.flag("disabled")
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
                disabled: disabled,
                submitsOnReturn: false,
                accessibilityLabel: label,
                secure: false,
                inline: true,
                keyboard: kind == .number ? .number : (kind == .search ? .search : .text)
            )
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
        .modifier(NumberAdjustable(on: kind == .number && !disabled, ownerId: cx.node.owner ?? "", model: model))
    }
}

/// VoiceOver's increment / decrement on a NumberField (contract §9 SwiftUI 6).
private struct NumberAdjustable: ViewModifier {
    let on: Bool
    let ownerId: String
    let model: SurfaceModel

    func body(content: Content) -> some View {
        if on {
            content.accessibilityAdjustableAction { direction in
                switch direction {
                case .increment: model.numberFieldStep(ownerId: ownerId, up: true)
                case .decrement: model.numberFieldStep(ownerId: ownerId, up: false)
                @unknown default: break
                }
            }
        } else {
            content
        }
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
        let isPlaceholder = cx.props["placeholder"]?.bool ?? cx.props.str("text").isEmpty
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
        let text = cx.props.str("text")
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
            TextLabel(cx.props.str("text"), cx.textStyle, color: cx.ink, lines: 1)
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
            TextLabel(cx.props.str("text"), cx.textStyle, color: cx.ink, lines: 1)
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
            case "badge" where !cx.props.str("text").isEmpty:
                let badge = SurfaceMeasurer.badgeStyle(cx.textStyle)
                TextLabel(cx.props.str("text"), badge, color: cx.themeColor("secondaryForeground") ?? cx.ink, lines: 1)
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
        let fallback = cx.props.str("text")
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
            TextLabel(cx.props.str("text"), cx.textStyle, color: cx.ink, align: textAlign, lines: 1)
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

// MARK: - platform hooks on native leaves

/// The platform input a native's leaves take beyond a press (until the
/// container itself carries `NativeContainerHooks`): files dropped on a
/// FileUpload, a context menu inside a ContextMenu.
struct NativeLeafHooks: ViewModifier {
    let cx: LeafContext

    @ViewBuilder
    func body(content: Content) -> some View {
        let model = cx.model
        if cx.node.recipeComponent == "FileUpload", let owner = cx.node.owner, !model.isDisabled(cx.index) {
            content.modifier(FileDropTarget(ownerId: owner, model: model))
        } else if cx.node.layer == 0, let menu = model.contextMenuAncestor(of: cx.index) {
            content.modifier(ContextMenuTarget(menuIndex: menu, model: model))
        } else {
            content
        }
    }
}

/// The hooks for a native's CONTAINER node (the FileUpload `dropzone`, the
/// ContextMenu region): `NodeView` applies it to every container so the
/// whole box takes the drop / the secondary click.
public struct NativeContainerHooks: ViewModifier {
    let index: Int
    let model: SurfaceModel

    public init(index: Int, model: SurfaceModel) {
        self.index = index
        self.model = model
    }

    @ViewBuilder
    public func body(content: Content) -> some View {
        if let n = model.node(index), n.recipeComponent == "FileUpload", n.part == "dropzone", let owner = n.owner, !model.isDisabled(index) {
            content.modifier(FileDropTarget(ownerId: owner, model: model))
        } else if let n = model.node(index), n.component == "ContextMenu", n.part == nil, n.layer == 0 {
            content.modifier(ContextMenuTarget(menuIndex: index, model: model))
        } else {
            content
        }
    }
}

/// A FileUpload drop target: URLs dropped (Finder, Files, another app) go
/// to the core as `upload {files}`; the zone wears `dragover` meanwhile.
private struct FileDropTarget: ViewModifier {
    let ownerId: String
    let model: SurfaceModel

    func body(content: Content) -> some View {
        content.dropDestination(for: URL.self) { urls, _ in
            model.setDragOver(ownerId, false)
            model.fileUploadReceived(ownerId: ownerId, urls: urls)
            return !urls.isEmpty
        } isTargeted: { on in
            model.setDragOver(ownerId, on)
        }
    }
}

/// A ContextMenu region: with native overlays the platform `.contextMenu`
/// (right click / long press) lists the menu's items; painted, a long
/// press opens the core's menu layer.
private struct ContextMenuTarget: ViewModifier {
    let menuIndex: Int
    let model: SurfaceModel

    @ViewBuilder
    func body(content: Content) -> some View {
        if model.options.overlays == .native, let owner = model.node(menuIndex) {
            content.contextMenu {
                NativeMenuContent(items: owner.props.list("items"), ownerIndex: menuIndex, model: model)
            }
        } else {
            content.simultaneousGesture(LongPressGesture(minimumDuration: 0.5).onEnded { _ in
                model.openContextMenu(menuIndex, at: nil)
            })
        }
    }
}

extension SurfaceModel {
    /// The nearest ContextMenu at or above node `index` (main tree).
    func contextMenuAncestor(of index: Int) -> Int? {
        var cur = node(index)?.parent
        var hops = 0
        while let c = cur, hops < 64 {
            guard let n = node(c) else { return nil }
            if n.component == "ContextMenu", n.part == nil { return c }
            cur = n.parent
            hops += 1
        }
        return nil
    }

    /// The FileUpload `dropzone` of `ownerId` wears `dragover` while files
    /// hover it (a recipe state, resolved by the core).
    func setDragOver(_ ownerId: String, _ on: Bool) {
        let zone = "\(ownerId).dropzone"
        var states = self.states(of: zone)
        if on { states.append("dragover") }
        if surface.setStates(id: zone, states: states) { invalidate(structure: false) }
    }
}

/// A menu's entries as PLATFORM menu content (`.contextMenu`, `Menu`):
/// `menuItem.kind` item | checkbox (a toggle) | separator | label |
/// submenu (one level), shortcuts shown, destructive and disabled entries.
/// A choice fires `select {value}` (+ `{checked}`) on the owner.
public struct NativeMenuContent: View {
    let items: [JSONValue]
    let ownerIndex: Int
    let model: SurfaceModel

    public init(items: [JSONValue], ownerIndex: Int, model: SurfaceModel) {
        self.items = items
        self.ownerIndex = ownerIndex
        self.model = model
    }

    public var body: some View {
        ForEach(Array(items.enumerated()), id: \.offset) { _, item in
            entry(item)
        }
    }

    @ViewBuilder
    private func entry(_ item: JSONValue) -> some View {
        let kind = item["kind"]?.string ?? (item["separator"]?.bool == true || item["type"]?.string == "separator" ? "separator" : "item")
        let label = item["label"]?.displayText ?? ""
        let disabled = item["disabled"]?.bool == true
        switch kind {
        case "separator":
            Divider()
        case "label":
            Text(label)
        case "submenu":
            Menu {
                AnyView(NativeMenuContent(items: item["items"]?.array ?? [], ownerIndex: ownerIndex, model: model))
            } label: {
                entryLabel(item, label)
            }
            .disabled(disabled)
        case "checkbox":
            Toggle(isOn: Binding(get: { item["checked"]?.bool == true }, set: { _ in model.nativeMenuSelect(ownerIndex: ownerIndex, item: item) })) {
                entryLabel(item, label)
            }
            .disabled(disabled)
            .modifier(MenuShortcut(text: item["shortcut"]?.displayText))
        default:
            Button(role: item["destructive"]?.bool == true ? .destructive : nil) {
                model.nativeMenuSelect(ownerIndex: ownerIndex, item: item)
            } label: {
                entryLabel(item, label)
            }
            .disabled(disabled)
            .modifier(MenuShortcut(text: item["shortcut"]?.displayText))
        }
    }

    @ViewBuilder
    private func entryLabel(_ item: JSONValue, _ label: String) -> some View {
        if let icon = item["icon"]?.string, let view = model.host.icon(icon, size: 16) {
            Label { Text(label) } icon: { view }
        } else if let icon = item["icon"]?.string, let symbol = BuiltinIcons.symbol(icon) {
            Label(label, systemImage: symbol)
        } else {
            Text(label)
        }
    }
}

/// A menu entry's `shortcut` (`⌘K`, `Cmd+Shift+P`, `Ctrl+S`) as the
/// platform shortcut the menu displays; unparsable text shows nothing.
struct MenuShortcut: ViewModifier {
    let text: String?

    func body(content: Content) -> some View {
        if let s = text.flatMap(Self.parse) {
            content.keyboardShortcut(s)
        } else {
            content
        }
    }

    static func parse(_ text: String) -> KeyboardShortcut? {
        var mods: EventModifiers = []
        var rest = text.trimmingCharacters(in: .whitespaces)
        let symbols: [(Character, EventModifiers)] = [("⌘", .command), ("⇧", .shift), ("⌥", .option), ("⌃", .control)]
        while let first = rest.first, let m = symbols.first(where: { $0.0 == first }) {
            mods.insert(m.1)
            rest.removeFirst()
        }
        let parts = rest.split(separator: "+").map { $0.trimmingCharacters(in: .whitespaces) }
        guard let last = parts.last, !last.isEmpty else { return nil }
        for p in parts.dropLast() {
            switch p.lowercased() {
            case "cmd", "command", "meta", "mod": mods.insert(.command)
            case "shift": mods.insert(.shift)
            case "alt", "option", "opt": mods.insert(.option)
            case "ctrl", "control": mods.insert(.control)
            default: return nil
            }
        }
        let key: KeyEquivalent
        switch last.lowercased() {
        case "enter", "return": key = .return
        case "esc", "escape": key = .escape
        case "delete", "backspace": key = .delete
        case "tab": key = .tab
        case "space": key = .space
        default:
            guard last.count == 1, let c = last.lowercased().first else { return nil }
            key = KeyEquivalent(c)
        }
        return mods.isEmpty ? nil : KeyboardShortcut(key, modifiers: mods)
    }
}
