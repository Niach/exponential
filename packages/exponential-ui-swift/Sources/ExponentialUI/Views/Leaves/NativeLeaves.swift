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
        let disabled = model.isDisabled(cx.index) || cx.props.flag("disabled")
        // #EXP-1629: the glyph-to-text gap is the field's recipe `gap` (else
        // 8); the glyph takes the placeholder's ink (`mutedForeground`) as
        // the IDE's search rows draw it, else the text's at 60%.
        let iconGap = cx.style.gap > 0 ? cx.style.gap : 8
        let glyph = cx.themeColor("mutedForeground").map { ($0, 1.0) } ?? (cx.ink, 0.6)
        // #EXP-1765: a Select search's glyph is its `searchIcon` part's size;
        // a `pill`'s ends in the clear button while it holds text.
        let glyphSize = cx.node.ownerComponent == "Select" ? cx.part("Select", "searchIcon").width ?? 16 : 16
        let clearable = model.selectSearchClearable(cx.index)
        HStack(spacing: icon == nil && !clearable ? 0 : iconGap) {
            if let icon {
                ConceptIcon(name: icon, size: glyphSize, color: glyph.0, model: model).opacity(glyph.1)
            }
            OwnedTextField(
                index: cx.index,
                model: model,
                multiline: false,
                placeholder: cx.props.str("placeholder"),
                font: font,
                // No recipe dims an inline field's box (its owner's `field`
                // has no disabled state); a disabled one reads muted, as
                // gpui's disabled input draws it.
                color: platformColor(disabled ? cx.muted : cx.ink),
                placeholderColor: platformColor(ph.color ?? cx.muted),
                lineHeight: cx.textStyle.lineHeight,
                disabled: disabled,
                submitsOnReturn: false,
                accessibilityLabel: label,
                inputType: ""
            )
            if clearable {
                SelectSearchClearButton(cx: cx, glyphSize: glyphSize)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// #EXP-1765: a `pill` Select search's clear button (while it holds text):
/// the `Select.clear` glyph at 0.75 × the `searchIcon` size in
/// `mutedForeground`, centred in a square box 1.25 × that size; "Clear
/// search", a button. Pressing it empties the field (the core hears "")
/// and keeps the focus in the field.
struct SelectSearchClearButton: View {
    let cx: LeafContext
    let glyphSize: CGFloat

    var body: some View {
        let box = glyphSize * SelectTrigger.clearBox
        let index = cx.index
        let model = cx.model
        Button {
            model.selectSearchClear(index)
        } label: {
            ConceptIcon(name: BuiltinIcons.name("Select.clear"), size: glyphSize * SelectTrigger.clearGlyph, color: cx.muted, model: model)
                .frame(width: box, height: box)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(width: box, height: box)
        .accessibilityLabel(model.builtinString("clearSearch"))
        .accessibilityAddTraits(.isButton)
    }
}

// MARK: - pickers

/// A picker `trigger` (Select, DatePicker, DateRangePicker, TimePicker):
/// the core's text (dates re-formatted in the surface locale from the ISO
/// value), muted while it is the placeholder, and the trigger glyph the
/// core named. The press opens the core's popup LAYER (`triggerFor`).
/// #EXP-1629: a Select's text is its `value` part and its glyph its
/// `chevron` part (a `row` trigger is the core's container of parts,
/// painted as nodes, never this leaf).
/// #EXP-1765: a `pill` is the same parts content-sized (the core's), its
/// glyph the `Select.dropdown` slot at the `chevron` part's size and colour
/// (opacity 1 unless the recipe fades it), the recipe `gap` apart; the
/// trigger's `disabled` reaches the value / placeholder parts. Its `label`
/// is the trigger's accessible name only (the core's).
struct PickerTriggerLeaf: View {
    let cx: LeafContext

    var body: some View {
        let isPlaceholder = cx.props["placeholder"]?.bool ?? cx.props.text("text").isEmpty
        let select = cx.node.recipeComponent == "Select"
        // #EXP-1778: a `button` trigger paints as a pill does (its own recipe).
        let pill = select && SelectTrigger.isPillLike(cx.props, owner: cx.ownerProps)
        let states = pill && cx.model.isDisabled(cx.index) ? ["disabled"] : []
        let ph = cx.part(cx.node.recipeComponent, "placeholder", states: states)
        let value = select ? cx.part("Select", "value", states: states) : .empty
        let chevron = select ? cx.part("Select", "chevron") : .empty
        let icon = cx.props.str("icon").isEmpty ? "ui-chevron-down" : cx.props.str("icon")
        let glyph = SelectTrigger.glyphSize(chevron)
        let glyphView = ConceptIcon(name: icon, size: glyph, color: chevron.color ?? cx.ink, model: cx.model)
            .opacity(chevron.style.opacity ?? (pill ? 1 : 0.6))
            .frame(width: glyph, height: glyph)
        let ink = isPlaceholder ? (ph.color ?? cx.muted) : (value.color ?? cx.ink)
        let inkOpacity = isPlaceholder ? (pill ? ph.style.opacity ?? 1 : 1) : (value.style.opacity ?? 1)
        HStack(spacing: pill ? SelectTrigger.pillGap(cx.style.gap, sm: cx.spacing("sm")) : cx.spacing("sm")) {
            TextLabel(text(isPlaceholder), cx.textStyle, color: ink, align: cx.style.textAlign, lines: 1)
                .opacity(inkOpacity)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: cx.textStyle.lineHeight)
            glyphView
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
/// reserved (gpui `text_chrome`; #EXP-1778: a `button`'s rows on the chosen
/// row only). #EXP-1765: a `mono` option (the core
/// sets its type) carries its `trailing` as its own muted run in the
/// theme's sans type, right after the label (`SelectTrigger.trailingGap`
/// apart); the label keeps its line and truncates first.
struct SelectItemLeaf: View {
    let cx: LeafContext

    var body: some View {
        let selected = cx.node.selected || cx.props.flag("selected")
        let check = cx.props.str("check").isEmpty ? BuiltinIcons.name("Select.check") : cx.props.str("check")
        // #EXP-1629: the check is the `check` part (its size and ink).
        let look = cx.part("Select", "check", states: selected ? ["selected"] : [])
        let size = SelectTrigger.glyphSize(look)
        let trailing = cx.props["trailing"].map(\.displayText) ?? ""
        HStack(spacing: max(cx.style.gap, 4)) {
            if let icon = cx.props["icon"]?.string, !icon.isEmpty {
                ConceptIcon(name: icon, size: 16, color: cx.ink, model: cx.model)
            }
            if trailing.isEmpty {
                TextLabel(cx.props.text("text"), cx.textStyle, color: cx.ink, lines: 1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(height: cx.textStyle.lineHeight)
            } else {
                labelAndTrailing(trailing)
            }
            // #EXP-1778: a `button` menu row holds the check on the chosen row only.
            if selected || !SelectTrigger.checkOnChosenOnly(cx.props) {
                ZStack {
                    if selected { ConceptIcon(name: check, size: size, color: look.color ?? cx.ink, model: cx.model).opacity(look.style.opacity ?? 1) }
                }
                .frame(width: size, height: size)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

extension SelectItemLeaf {
    /// The label (its line, at most what the trailing leaves) then the
    /// trailing, `trailingGap` apart, filling the text slot.
    @ViewBuilder
    func labelAndTrailing(_ trailing: String) -> some View {
        let gap = SelectTrigger.trailingGap(cx.style.gap)
        let sans = SelectTrigger.trailingStyle(cx.textStyle, sans: cx.model.theme?.sansFamily)
        GeometryReader { g in
            let t = TextShaper.maxContent(sans.shown(trailing), sans)
            let l = min(TextShaper.maxContent(cx.textStyle.shown(cx.props.text("text")), cx.textStyle), max(0, g.size.width - t - gap))
            HStack(spacing: gap) {
                TextLabel(cx.props.text("text"), cx.textStyle, color: cx.ink, lines: 1)
                    .frame(width: l, height: cx.textStyle.lineHeight)
                TextLabel(trailing, sans, color: cx.muted, lines: 1)
                    .frame(width: min(t, max(0, g.size.width - l - gap)), height: sans.lineHeight)
            }
            .frame(width: g.size.width, height: g.size.height, alignment: .leading)
        }
        .frame(maxWidth: .infinity)
        .frame(height: cx.textStyle.lineHeight)
    }
}

/// A Menu entry's `itemLabel`: [icon] label (the row,
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
                // EXP-1548: the lines as measured (wrapped; one with an
                // ellipsis in a `truncate` column), centred in the row like
                // gpui and the web's `align-items: center`.
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(TextLeaf.shown(text(value), cx.textStyle, width: cx.inner.width, clamp: cx.node.lines).enumerated()), id: \.offset) { _, line in
                        TextLabel(line, cx.textStyle, color: cx.ink, align: textAlign, lines: 1)
                            .frame(width: cx.inner.width, height: cx.textStyle.lineHeight)
                    }
                }
                .frame(height: cx.inner.height)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: Alignment(horizontal: frameAlign.horizontal, vertical: .center))
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    private func text(_ value: JSONValue) -> String {
        let fallback = cx.props.text("text")
        switch cx.props.str("cellType") {
        case "number":
            guard let n = value.number ?? Double(value.displayText), !value.isNull, value.displayText != "" else { return fallback }
            return cx.model.formatNumber(n, precision: nil)
        case "date":
            // FEED-202: a column `format` (dd.MM.yy) is the core's text.
            if !cx.props.str("format").isEmpty { return fallback }
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
        // EXP-1555: the block's `variant` reaches the token recipe, so a
        // destructive block (a failed call's output) paints EVERY token in
        // its own colour, like web's `text-current` tokens.
        var base: Props = ["language": .string(language)]
        if let variant = cx.ownerProps["variant"] { base["variant"] = variant }
        if let size = cx.ownerProps["size"] { base["size"] = size }
        var looks: [String: PartStyle] = [:]
        for token in cx.props.list("tokens") {
            let text = token["text"]?.string ?? ""
            if text.isEmpty { continue }
            let kind = token["kind"]?.string ?? "plain"
            let look = looks[kind] ?? cx.model.codeTokenLook(kind, base: base)
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

extension SurfaceModel {
    /// A CodeBlock token's look by its `kind`: the `CodeBlock/token {kind}`
    /// recipe under `base`. CodeBlock lines paint its colour, weight and
    /// italic; a Markdown code block (#VAPP-258) its colour only.
    func codeTokenLook(_ kind: String, base: Props = [:]) -> PartStyle {
        part("CodeBlock", "token", props: base.merging(["kind": .string(kind)]) { $1 })
    }
}

// MARK: - platform hooks

/// The platform input a native takes beyond a press (gpui `interactive`):
/// files dropped on a FileUpload `dropzone` (`dragover` meanwhile), a
/// secondary click (macOS) or a long press (touch) on a context Menu (`openOn: contextmenu`), which
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
        } else if node.component == "Menu", node.part == nil, node.layer == 0, node.props.str("openOn") == "contextmenu" {
            content.modifier(MenuContextGesture(index: node.index, model: model))
        } else {
            content
        }
    }
}

/// Open a context Menu at the pointer: a secondary click on macOS, a long
/// press on touch (at the region, like Shift+F10).
private struct MenuContextGesture: ViewModifier {
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
