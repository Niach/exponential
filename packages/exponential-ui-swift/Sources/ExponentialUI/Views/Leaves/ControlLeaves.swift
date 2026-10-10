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
            inputType: multiline ? "" : owner.str("type")
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
        let attachments = cx.props.flag("attachments")
        VStack(alignment: .leading, spacing: gap) {
            OwnedTextField(
                index: cx.index,
                model: cx.model,
                multiline: true,
                placeholder: cx.props.str("placeholder"),
                font: font,
                color: platformColor(field.color ?? cx.ink),
                placeholderColor: platformColor(placeholder.color ?? cx.themeColor("mutedForeground") ?? cx.ink.opacity(0.5)),
                lineHeight: lh,
                disabled: cx.model.isDisabled(cx.index),
                submitsOnReturn: true,
                accessibilityLabel: cx.props.str("placeholder").isEmpty ? cx.model.builtinString("send") : cx.props.str("placeholder"),
                inputType: ""
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            HStack(spacing: cx.model.spacing("xs")) {
                Spacer(minLength: 0)
                // `attachments: true` = the attach control (catalog: "Shows
                // the attach control"; gpui `paint.rs` composer bar), firing
                // `attach`; the files themselves are the host's business.
                if attachments {
                    let att = cx.part("Composer", "attachment")
                    let attH = min(sendSize, att.height ?? 28)
                    Button {
                        cx.model.fire(cx.index, "attach")
                    } label: {
                        ConceptIcon(name: BuiltinIcons.name("Composer.attachment"), size: 16, color: att.color ?? cx.ink, model: cx.model)
                            .padding(.horizontal, 8)
                            .frame(minWidth: att.width ?? 28, minHeight: attH, maxHeight: attH)
                            .background(att.style.background ?? .clear, in: att.style.shape(CGSize(width: att.width ?? 28, height: attH)))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(cx.model.builtinString("browse"))
                }
                let sendW = send.width ?? sendSize
                Button {
                    cx.model.composerSubmit(cx.index)
                } label: {
                    ConceptIcon(name: BuiltinIcons.name(busy ? "Composer.stop" : "Composer.send"), size: 16, color: send.color ?? cx.themeColor("primaryForeground") ?? .white, model: cx.model, weight: .semibold)
                        .frame(width: sendW, height: sendSize)
                        .background(send.style.background ?? cx.themeColor("primary") ?? cx.ink, in: send.style.radius > 0 ? AnyShape(send.style.shape(CGSize(width: sendW, height: sendSize))) : AnyShape(Circle()))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(busy ? cx.model.builtinString("stop") : (cx.props.str("submitLabel").isEmpty ? cx.model.builtinString("send") : cx.props.str("submitLabel")))
            }
            .frame(height: sendSize)
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }
}

/// A Checkbox `box` (or a Table's selection `checkbox` part, whose state
/// is its own): the check glyph when checked.
struct CheckBoxLeaf: View {
    let cx: LeafContext

    var body: some View {
        let own = cx.node.part == "checkbox" || cx.node.recipeComponent == "Table"
        let checked = own ? (cx.node.checked || cx.props.flag("checked")) : cx.model.checked(cx.index)
        if checked {
            let check = cx.part("Checkbox", "check")
            ConceptIcon(name: BuiltinIcons.name("Checkbox.check"), size: check.width ?? 12, color: check.color ?? cx.style.color ?? cx.ink, model: cx.model, weight: .bold)
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
            // The minimum sits at the right edge in rtl (gpui `slider_geometry`).
            let (fillX, fillW, thumbX) = cx.rtl ? (w * (1 - f), w * f, (w - t) * (1 - f)) : (0, w * f, (w - t) * f)
            let originX = cx.model.frame(cx.index).minX
            ZStack(alignment: .leading) {
                Capsule().fill(track.style.background ?? cx.themeColor("muted") ?? cx.ink.opacity(0.15)).frame(width: w, height: barH)
                Capsule().fill(range.style.background ?? cx.themeColor("primary") ?? cx.ink).frame(width: fillW, height: barH).offset(x: fillX)
                Circle()
                    .fill(thumb.style.background ?? .white)
                    .overlay(Circle().strokeBorder(thumb.style.borderColor ?? .clear, lineWidth: thumb.style.borderWidth))
                    .frame(width: t, height: t)
                    .offset(x: thumbX)
            }
            .frame(width: w, height: cx.size.height)
            .contentShape(Rectangle().inset(by: -8))
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { g in
                        cx.model.sliderDrag(cx.index, value: cx.model.sliderValue(at: originX + g.location.x, trackIndex: cx.index))
                    }
                    .onEnded { _ in cx.model.sliderRelease(cx.index) }
            )
            .disabled(disabled)
        } else {
            let geometry = SurfaceModel.sliderRange(min: min, max: max, step: step)
            let binding = Binding(get: { value }, set: { cx.model.sliderDrag(cx.index, value: SurfaceModel.snap($0, min: min, max: max, step: step)) })
            Group {
                if let s = geometry.step {
                    Slider(value: binding, in: geometry.range, step: s) { _ in cx.model.sliderRelease(cx.index) }
                } else {
                    Slider(value: binding, in: geometry.range) { _ in cx.model.sliderRelease(cx.index) }
                }
            }
            .tint(cx.themeColor("primary"))
            .disabled(disabled)
            .frame(width: cx.size.width)
            .frame(height: cx.size.height)
        }
    }
}

/// A Segmented (round 3, the old ToggleGroup): a row of items, the chosen
/// ones selected (single or multiple); `segmented` draws the row on a track,
/// `bar` (the old TabBar) lays each item out as a COLUMN (icon over a
/// caption label) sharing the full width. Keyboard focus rings the ROVING
/// item (`segmentedFocusIndex`, gpui `paint_segmented`).
struct SegmentedLeaf: View {
    let cx: LeafContext

    var body: some View {
        let props = cx.props
        let items = props.list("items")
        let values = cx.model.segmentedValues(cx.index)
        let bar = props.str("variant") == "bar"
        let item = cx.part("Segmented", "item")
        let box = item.toggleItemBox
        let pad = box.paddingHorizontal
        let h = bar ? cx.inner.height : box.height
        // The measurer's text style (`SurfaceMeasurer.segmented`): the
        // label is drawn by the shaper at its measured width, so the item
        // painted is the item measured (a SwiftUI `Text` sized itself in
        // its own font and truncated "Board" to "Boa…").
        let ts = TextStyle(fontSize: item.px("fontSize") ?? cx.textStyle.fontSize, fontWeight: Int(item.props.num("fontWeight") ?? 500), lineHeight: cx.textStyle.lineHeight, fontFamily: item.fontFamily, letterSpacing: cx.textStyle.letterSpacing, textTransform: cx.textStyle.textTransform)
        let border = item.px("borderWidth") ?? 0
        let fill = props.flag("fill") || bar
        let disabledAll = cx.model.isDisabled(cx.index)
        let roving = cx.model.focusVisible(cx.node.id) ? cx.model.segmentedFocusIndex(cx.index) : nil
        let row = HStack(spacing: cx.style.gap) {
            ForEach(Array(items.enumerated()), id: \.offset) { k, it in
                let value = it["value"] ?? .string(it["label"]?.displayText ?? "")
                let selected = values.contains(value.displayText)
                let disabled = disabledAll || it["disabled"]?.bool == true
                let ring = roving == k
                let states = (selected ? ["selected"] : []) + (disabled ? ["disabled"] : []) + (ring ? ["focus-visible"] : [])
                let st = cx.part("Segmented", "item", states: states)
                Button {
                    cx.model.segmentedSelect(cx.index, value: value)
                } label: {
                    Group {
                        if bar {
                            barItem(it, states: states, color: st.color ?? cx.ink)
                        } else {
                            HStack(spacing: item.px("gap") ?? 0) {
                                if let icon = it["icon"]?.string {
                                    ConceptIcon(name: icon, size: 16, color: st.color ?? cx.ink, model: cx.model)
                                }
                                if let label = it["label"]?.displayText, !label.isEmpty {
                                    TextLabel(label, ts, color: st.color ?? cx.ink, lines: 1)
                                        .frame(width: TextShaper.width(label, ts), height: ts.lineHeight)
                                }
                            }
                            .padding(.horizontal, pad + border)
                        }
                    }
                    .frame(maxWidth: fill ? .infinity : nil)
                    .frame(height: h)
                    .paintedBox(st.style, size: CGSize(width: 0, height: h))
                    .overlay {
                        if ring, st.style.shadows.isEmpty {
                            RoundedRectangle(cornerRadius: st.style.radius).strokeBorder(cx.themeColor("ring") ?? cx.ink.opacity(0.5), lineWidth: 2)
                        }
                    }
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
        // A `bar` = navigation (the bottom destinations): VoiceOver's tab
        // bar container; the selected item carries `.isSelected` (SwiftUI
        // has no `aria-current`).
        if bar {
            row.accessibilityElement(children: .contain).accessibilityAddTraits(.isTabBar)
        } else {
            row
        }
    }

    /// One `bar` item: the `icon` part (`$control.iconMd`) over the caption
    /// `label` part, `xxs` apart, `xs` vertical padding (gpui
    /// `segmented_bar_item`).
    @ViewBuilder
    private func barItem(_ it: JSONValue, states: [String], color: Color) -> some View {
        let iconInk = cx.part("Segmented", "icon", states: states).color ?? color
        let labelPart = cx.part("Segmented", "label", states: states)
        let caption = SurfaceMeasurer.barCaption(cx.part("Text", "root", props: ["variant": .string("caption")]), base: cx.textStyle)
        let ts = TextStyle(fontSize: caption.fontSize, fontWeight: Int(labelPart.props.num("fontWeight") ?? Double(cx.textStyle.fontWeight)), lineHeight: caption.lineHeight, fontFamily: caption.fontFamily, italic: caption.italic)
        VStack(spacing: cx.spacing("xxs")) {
            if let icon = it["icon"]?.string {
                ConceptIcon(name: icon, size: cx.control("iconMd", 20), color: iconInk, model: cx.model)
            }
            if let label = it["label"]?.displayText, !label.isEmpty {
                TextLabel(label, ts, color: labelPart.color ?? color, lines: 1)
                    .frame(maxWidth: .infinity)
                    .frame(height: ts.lineHeight)
            }
        }
        .padding(.vertical, cx.spacing("xs"))
        .frame(minWidth: 0, maxWidth: .infinity)
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
    /// A Segmented `item` as `SegmentedLeaf` draws it.
    var toggleItemBox: DrawnBox {
        DrawnBox(width: nil, height: height ?? 36, paddingHorizontal: px("paddingHorizontal") ?? px("padding") ?? 12, gap: style.gap, borderWidth: style.borderWidth, borderRadius: style.radius)
    }

    /// The drawn Slider `thumb` (a circle `width` across).
    var sliderThumbBox: DrawnBox {
        let t = width ?? 16
        return DrawnBox(width: t, height: height ?? t, borderWidth: style.borderWidth, borderRadius: style.radius)
    }
}
