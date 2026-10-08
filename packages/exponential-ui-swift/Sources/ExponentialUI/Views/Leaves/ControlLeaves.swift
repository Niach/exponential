import SwiftUI
import ExponentialUIPrimitives

private func platformColor(_ c: Color) -> PlatformColor {
    #if canImport(UIKit)
    UIColor(c)
    #else
    NSColor(c)
    #endif
}

/// An Input / Textarea `.field`: the owned text view inside the field box
/// (the node's own visual paints the chrome).
struct TextFieldLeaf: View {
    let cx: LeafContext
    let multiline: Bool

    var body: some View {
        let owner = cx.ownerProps
        let placeholder = cx.part(cx.node.recipeComponent, "placeholder")
        let font = ExponentialUIFonts.font(family: cx.textStyle.fontFamily, weight: cx.textStyle.fontWeight, size: cx.textStyle.fontSize)
        OwnedTextField(
            index: cx.index,
            model: cx.model,
            multiline: multiline,
            placeholder: owner.str("placeholder"),
            font: font,
            color: platformColor(cx.ink),
            placeholderColor: platformColor(placeholder.color ?? cx.themeColor("mutedForeground") ?? cx.ink.opacity(0.5)),
            lineHeight: cx.textStyle.lineHeight,
            disabled: cx.model.isDisabled(cx.index),
            submitsOnReturn: false,
            accessibilityLabel: owner.str("label").isEmpty ? owner.str("placeholder") : owner.str("label"),
            secure: owner.str("type") == "password"
        )
        .frame(width: cx.inner.width, height: cx.inner.height)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// The Composer: a growing text view over a send row (attachments, send).
struct ComposerLeaf: View {
    let cx: LeafContext

    var body: some View {
        let field = cx.part("Composer", "field")
        let send = cx.part("Composer", "send")
        let placeholder = cx.part("Composer", "placeholder")
        let sendSize = send.height ?? cx.control("buttonIcon", 36)
        let gap = cx.style.gap
        let busy = cx.props.flag("busy")
        let fs = field.px("fontSize") ?? cx.textStyle.fontSize
        let lh = field.px("lineHeight") ?? cx.textStyle.lineHeight
        let font = ExponentialUIFonts.font(family: field.fontFamily ?? cx.textStyle.fontFamily, weight: cx.textStyle.fontWeight, size: fs)
        let attachments = cx.props.list("attachments")
        VStack(alignment: .leading, spacing: gap) {
            OwnedTextField(
                index: cx.index,
                model: cx.model,
                multiline: true,
                placeholder: cx.props.str("placeholder").isEmpty ? "Message" : cx.props.str("placeholder"),
                font: font,
                color: platformColor(field.color ?? cx.ink),
                placeholderColor: platformColor(placeholder.color ?? cx.themeColor("mutedForeground") ?? cx.ink.opacity(0.5)),
                lineHeight: lh,
                disabled: cx.model.isDisabled(cx.index),
                submitsOnReturn: true,
                accessibilityLabel: cx.props.str("placeholder").isEmpty ? "Message" : cx.props.str("placeholder"),
                secure: false
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            HStack(spacing: 8) {
                ForEach(Array(attachments.enumerated()), id: \.offset) { _, a in
                    let chip = cx.part("Composer", "attachment")
                    HStack(spacing: 4) {
                        GlyphView(glyph: .attach, size: 12, color: chip.color ?? cx.ink)
                        Text(a["name"]?.displayText ?? a.displayText).font(.system(size: 12)).foregroundStyle(chip.color ?? cx.ink).lineLimit(1)
                    }
                    .padding(.horizontal, 8)
                    .frame(height: chip.height ?? 24)
                    .background(chip.style.background ?? cx.ink.opacity(0.08), in: Capsule())
                }
                Spacer(minLength: 0)
                Button {
                    cx.model.composerSubmit(cx.index)
                } label: {
                    GlyphView(glyph: busy ? .stop : .send, size: 18, color: send.color ?? cx.themeColor("primaryForeground") ?? .white, weight: .semibold)
                        .frame(width: send.width ?? sendSize, height: sendSize)
                        .background(send.style.background ?? cx.themeColor("primary") ?? cx.ink, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(busy ? "Stop" : (cx.props.str("submitLabel").isEmpty ? "Send" : cx.props.str("submitLabel")))
            }
            .frame(height: sendSize)
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// A Select / DatePicker trigger's content: the value (or placeholder) and
/// the chevron / calendar glyph.
struct TriggerContent: View {
    let cx: LeafContext
    let label: String
    let isPlaceholder: Bool
    let glyph: Glyph

    var body: some View {
        let trigger = cx.part(cx.node.recipeComponent, "trigger")
        let ph = cx.part(cx.node.recipeComponent, "placeholder")
        let muted = ph.color ?? cx.themeColor("mutedForeground") ?? cx.ink.opacity(0.6)
        let ts = TextStyle(fontSize: trigger.px("fontSize") ?? cx.textStyle.fontSize, fontWeight: 400, lineHeight: trigger.px("lineHeight") ?? cx.textStyle.lineHeight, fontFamily: trigger.fontFamily ?? cx.textStyle.fontFamily)
        let pad = trigger.px("paddingHorizontal") ?? trigger.px("padding") ?? 12
        let border = trigger.px("borderWidth") ?? 0
        HStack(spacing: cx.spacing("sm")) {
            TextLabel(label, ts, color: isPlaceholder ? muted : cx.ink, lines: 1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: ts.lineHeight)
            GlyphView(glyph: glyph, size: 16, color: cx.ink.opacity(0.6))
        }
        .padding(.horizontal, pad + border)
        .frame(width: cx.size.width, height: cx.size.height)
    }
}

/// A Select `.field`: the trigger chrome; a native `Menu` opens the
/// options (the painted popup when the `content` recipe is `native: false`).
struct SelectFieldLeaf: View {
    let cx: LeafContext

    var body: some View {
        let owner = cx.ownerProps
        let props: Props = { var p = owner; p["value"] = cx.model.selectValue(cx.index); return p }()
        let label = SurfaceMeasurer.selectLabel(props)
        let isPlaceholder = (props["value"]?.isNull ?? true) || (props["value"]?.array?.isEmpty ?? false)
        let values = cx.model.toggleValuesOf(cx.model.selectValue(cx.index))
        let disabled = cx.model.isDisabled(cx.index)
        Menu {
            ForEach(Array(owner.list("options").enumerated()), id: \.offset) { _, option in
                let v = option["value"] ?? .null
                Button {
                    cx.model.selectPick(cx.index, value: v)
                } label: {
                    if values.contains(v.displayText) {
                        Label(option["label"]?.displayText ?? v.displayText, systemImage: "checkmark")
                    } else {
                        Text(option["label"]?.displayText ?? v.displayText)
                    }
                }
                .disabled(option["disabled"]?.bool == true)
            }
        } label: {
            TriggerContent(cx: cx, label: label, isPlaceholder: isPlaceholder, glyph: .chevronsUpDown)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .disabled(disabled)
        .accessibilityLabel(owner.str("label"))
        .accessibilityValue(label)
    }
}

extension SurfaceModel {
    func toggleValuesOf(_ v: JSONValue) -> [String] {
        switch v {
        case let .array(a): a.map(\.displayText)
        case .null: []
        default: [v.displayText]
        }
    }
}

/// A DatePicker `.field`: the trigger chrome; a tap opens the native
/// graphical date picker (`DatePopup` on the surface root).
struct DateFieldLeaf: View {
    let cx: LeafContext

    var body: some View {
        let owner = cx.ownerProps
        let value = cx.model.dateValue(cx.index)
        let label = SurfaceMeasurer.dateLabel(value) ?? (owner.str("placeholder").isEmpty ? "Pick a date" : owner.str("placeholder"))
        Button {
            cx.model.press(cx.index)
        } label: {
            TriggerContent(cx: cx, label: label, isPlaceholder: SurfaceMeasurer.dateLabel(value) == nil, glyph: .calendar)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(cx.model.isDisabled(cx.index))
        .accessibilityLabel(owner.str("label"))
        .accessibilityValue(label)
    }
}

/// A Checkbox `box`: the check glyph when checked.
struct CheckBoxLeaf: View {
    let cx: LeafContext

    var body: some View {
        if cx.model.checked(cx.index) {
            let check = cx.part("Checkbox", "check")
            GlyphView(glyph: .check, size: check.width ?? 12, color: check.color ?? cx.ink, weight: .bold)
                .frame(width: cx.size.width, height: cx.size.height)
        } else {
            Color.clear
        }
    }
}

/// A Radio `dot`: the inner dot when chosen.
struct RadioDotLeaf: View {
    let cx: LeafContext

    var body: some View {
        if cx.model.radioChecked(cx.index) {
            let dot = cx.part("Radio", "dot", states: ["checked"])
            let d = dot.width ?? min(cx.size.width, cx.size.height) / 2
            Circle().fill(dot.style.background ?? cx.ink).frame(width: d, height: d)
                .frame(width: cx.size.width, height: cx.size.height)
        } else {
            Color.clear
        }
    }
}

/// A Switch `track`: the thumb travelled right when checked (the drawn
/// style), or the platform switch scaled into the track when the recipe
/// leaves `native` unset.
struct SwitchTrackLeaf: View {
    let cx: LeafContext

    var body: some View {
        let checked = cx.model.checked(cx.index)
        let track = cx.part("Switch", "track")
        let props: Props = { var p = cx.ownerProps; p["checked"] = .bool(checked); return p }()
        let thumb = cx.part("Switch", "thumb", props: props, states: checked ? ["checked"] : [])
        let size = thumb.width ?? max(4, cx.size.height - 4)
        let inset = max(0, (cx.size.height - size) / 2)
        if track.native == false || cx.model.theme == nil {
            ZStack(alignment: .leading) {
                Circle()
                    .fill(thumb.style.background ?? cx.themeColor("background") ?? .white)
                    .frame(width: size, height: size)
                    .shadow(color: .black.opacity(0.15), radius: 1, y: 1)
                    .offset(x: checked ? cx.size.width - size - inset : inset, y: inset)
            }
            .frame(width: cx.size.width, height: cx.size.height, alignment: .topLeading)
            .animation(.easeInOut(duration: 0.15), value: checked)
        } else {
            #if os(iOS)
            let scale = cx.size.height / 31
            Toggle("", isOn: Binding(get: { checked }, set: { _ in cx.model.press(cx.index) }))
                .labelsHidden()
                .toggleStyle(.switch)
                .tint(cx.themeColor("primary"))
                .scaleEffect(scale)
                .frame(width: cx.size.width, height: cx.size.height)
            #else
            Toggle("", isOn: Binding(get: { checked }, set: { _ in cx.model.press(cx.index) }))
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.small)
                .frame(width: cx.size.width, height: cx.size.height)
            #endif
        }
    }
}

/// A Slider `track`: range + thumb at the value (drawn, with a drag), or
/// the platform slider when the recipe leaves `native` unset.
struct SliderTrackLeaf: View {
    let cx: LeafContext

    var body: some View {
        let min = cx.props.num("min") ?? 0
        let max = cx.props.num("max") ?? 100
        let step = cx.props.num("step") ?? 1
        let value = cx.model.sliderValue(cx.index)
        let track = cx.part("Slider", "track")
        let disabled = cx.model.isDisabled(cx.index)
        if track.native == false || cx.model.theme == nil {
            let range = cx.part("Slider", "range")
            let thumb = cx.part("Slider", "thumb", states: cx.states.filter { $0 == "focus" || $0 == "hover" })
            let barH = Swift.min(track.height ?? 6, Swift.max(cx.size.height, 1))
            let t = thumb.sliderThumbBox.width ?? 16
            let f = max > min ? CGFloat((value - min) / (max - min)) : 0
            let w = cx.size.width
            ZStack(alignment: .leading) {
                Capsule().fill(track.style.background ?? cx.themeColor("muted") ?? cx.ink.opacity(0.15)).frame(width: w, height: barH)
                Capsule().fill(range.style.background ?? cx.themeColor("primary") ?? cx.ink).frame(width: w * f, height: barH)
                Circle()
                    .fill(thumb.style.background ?? .white)
                    .overlay(Circle().strokeBorder(thumb.style.borderColor ?? .clear, lineWidth: thumb.style.borderWidth))
                    .frame(width: t, height: t)
                    .offset(x: (w - t) * f)
            }
            .frame(width: w, height: cx.size.height)
            .contentShape(Rectangle().inset(by: -8))
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { g in
                        let frac = w > 0 ? Swift.min(1, Swift.max(0, g.location.x / w)) : 0
                        cx.model.sliderDrag(cx.index, value: SurfaceModel.snap(min + Double(frac) * (max - min), min: min, max: max, step: step))
                    }
                    .onEnded { _ in cx.model.sliderRelease(cx.index) }
            )
            .disabled(disabled)
        } else {
            Slider(value: Binding(get: { value }, set: { cx.model.sliderDrag(cx.index, value: SurfaceModel.snap($0, min: min, max: max, step: step)) }), in: min...Swift.max(max, min + 1), step: step > 0 ? step : 1) { _ in
                cx.model.sliderRelease(cx.index)
            }
            .tint(cx.themeColor("primary"))
            .disabled(disabled)
            .frame(width: cx.size.width)
            .frame(height: cx.size.height)
        }
    }
}

/// A ToggleGroup: a row of items, the chosen ones selected (single or
/// multiple); `segmented` draws the row on a track.
struct ToggleGroupLeaf: View {
    let cx: LeafContext

    var body: some View {
        let props = cx.props
        let items = props.list("items")
        let values = cx.model.toggleGroupValues(cx.index)
        let item = cx.part("ToggleGroup", "item")
        let box = item.toggleItemBox
        let pad = box.paddingHorizontal
        let h = box.height
        let fs = item.px("fontSize") ?? cx.textStyle.fontSize
        let weight = ExponentialUIFonts.swiftUIWeight(Int(item.props.num("fontWeight") ?? 500))
        let fill = props.flag("fill")
        let disabledAll = cx.model.isDisabled(cx.index)
        HStack(spacing: cx.style.gap) {
            ForEach(Array(items.enumerated()), id: \.offset) { _, it in
                let value = it["value"] ?? .string(it["label"]?.displayText ?? "")
                let selected = values.contains(value.displayText)
                let disabled = disabledAll || it["disabled"]?.bool == true
                let st = cx.part("ToggleGroup", "item", states: (selected ? ["selected"] : []) + (disabled ? ["disabled"] : []))
                Button {
                    cx.model.toggleGroupSelect(cx.index, value: value)
                } label: {
                    HStack(spacing: 6) {
                        if let icon = it["icon"]?.string {
                            IconView(name: icon, size: 16, color: st.color ?? cx.ink, model: cx.model)
                        }
                        if let label = it["label"]?.displayText, !label.isEmpty {
                            Text(label).font(.system(size: fs, weight: weight)).foregroundStyle(st.color ?? cx.ink).lineLimit(1)
                        }
                    }
                    .padding(.horizontal, pad)
                    .frame(maxWidth: fill ? .infinity : nil)
                    .frame(height: h)
                    .paintedBox(st.style, size: CGSize(width: 0, height: h))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(disabled)
                .accessibilityLabel(it["label"]?.displayText ?? value.displayText)
                .accessibilityAddTraits(selected ? [.isButton, .isSelected] : [.isButton])
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .leading)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// The border box of a sub-part the painter DRAWS inside a leaf (no node of
/// its own): what the control-geometry conformance suite probes (VAPP-91).
struct DrawnBox: Equatable {
    var width: CGFloat?
    var height: CGFloat
    var paddingHorizontal: CGFloat = 0
    var paddingVertical: CGFloat = 0
    var gap: CGFloat = 0
    var borderWidth: CGFloat = 0
    var borderRadius: CGFloat = 0
}

extension PartStyle {
    /// A ToggleGroup `item` as `ToggleGroupLeaf` draws it.
    var toggleItemBox: DrawnBox {
        DrawnBox(width: nil, height: height ?? 36, paddingHorizontal: px("paddingHorizontal") ?? px("padding") ?? 12, gap: style.gap, borderWidth: style.borderWidth, borderRadius: style.radius)
    }

    /// The drawn Slider `thumb` (a circle `width` across).
    var sliderThumbBox: DrawnBox {
        let t = width ?? 16
        return DrawnBox(width: t, height: height ?? t, borderWidth: style.borderWidth, borderRadius: style.radius)
    }
}
