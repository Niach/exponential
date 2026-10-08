import SwiftUI
import ExponentialUIPrimitives

/// A layer's tree: the layer root at (0, 0) with its subtree nested.
struct LayerView: View {
    let layer: LayerInfo
    let model: SurfaceModel

    var body: some View {
        NodeView(index: layer.root)
            .layoutValue(key: NodeIndexKey.self, value: layer.root)
            .frame(width: layer.frame.width, height: layer.frame.height)
    }
}

/// Native presentations of the open layers: Dialog / Drawer as sheets,
/// Popover as a popover (anchored at the core's anchor frame), Tooltip
/// and DropdownMenu content painted in place (the menu is a native `Menu`
/// on its trigger). In `.painted` mode nothing presents here.
struct NativeOverlays: ViewModifier {
    let model: SurfaceModel

    private var modal: Binding<LayerInfo?> {
        Binding(
            get: { model.options.overlays == .native ? model.modalLayers.first : nil },
            set: { if $0 == nil, let owner = model.modalLayers.first?.owner { model.dismissLayer(owner) } }
        )
    }

    private var popover: Binding<LayerInfo?> {
        Binding(
            get: { model.options.overlays == .native ? model.layers.first { $0.kind == "Popover" || ($0.kind == "DropdownMenu" && model.menuPainted($0.owner)) } : nil },
            set: { if $0 == nil, let l = model.layers.first(where: { $0.kind == "Popover" || $0.kind == "DropdownMenu" }) { model.dismissLayer(l.owner) } }
        )
    }

    func body(content: Content) -> some View {
        content
            .sheet(item: modal) { layer in
                ModalLayer(layer: layer, model: model)
            }
            .popover(item: popover, attachmentAnchor: .rect(.rect(popover.wrappedValue?.anchorFrame ?? .zero)), arrowEdge: arrowEdge(popover.wrappedValue)) { layer in
                LayerView(layer: layer, model: model)
                    .environment(model)
                    .environment(\.layoutDirection, .leftToRight)
                    .primitiveTokens(model.primitiveTokens)
                    .padding(0)
                    .presentationCompactAdaptation(.popover)
                    .presentationBackground(model.color("popover") ?? model.color("background") ?? .clear)
            }
    }

    private func arrowEdge(_ layer: LayerInfo?) -> Edge {
        switch layer?.placementSide ?? layer?.position {
        case "top": .bottom
        case "left": .trailing
        case "right": .leading
        default: .top
        }
    }
}

/// A Dialog / Drawer in a sheet: the layer tree centred, the sheet as tall
/// as the layer (a detent), swipe-to-dismiss unless `dismissible: false`.
private struct ModalLayer: View {
    let layer: LayerInfo
    let model: SurfaceModel

    var body: some View {
        let owner = model.index(of: layer.owner).flatMap { model.node($0) }
        let dismissible = owner?.props["dismissible"]?.bool ?? true
        ZStack {
            LayerView(layer: layer, model: model)
            // A second modal opened from this one stacks on it.
            Color.clear.frame(width: 0, height: 0).modifier(NestedModal(model: model, below: layer.owner))
        }
        .environment(model)
        .environment(\.layoutDirection, .leftToRight)
        .primitiveTokens(model.primitiveTokens)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.top, layer.kind == "Drawer" ? 0 : 12)
        .presentationBackground(model.color("background") ?? .clear)
        .modifier(SheetDetents(height: layer.frame.height + 24, drawer: layer.kind == "Drawer"))
        .interactiveDismissDisabled(!dismissible)
        #if os(macOS)
        .onExitCommand { if dismissible { model.dismissLayer(layer.owner) } }
        #endif
    }
}

private struct SheetDetents: ViewModifier {
    let height: CGFloat
    let drawer: Bool

    func body(content: Content) -> some View {
        #if os(iOS)
        content
            .presentationDetents([.height(max(120, height)), .large])
            .presentationDragIndicator(drawer ? .visible : .hidden)
        #else
        content.frame(minWidth: 320, minHeight: max(120, height))
        #endif
    }
}

private struct NestedModal: ViewModifier {
    let model: SurfaceModel
    let below: String

    private var next: Binding<LayerInfo?> {
        Binding(
            get: {
                let modals = model.modalLayers
                guard let i = modals.firstIndex(where: { $0.owner == below }), i + 1 < modals.count else { return nil }
                return modals[i + 1]
            },
            set: { if $0 == nil, let owner = next.wrappedValue?.owner { model.dismissLayer(owner) } }
        )
    }

    func body(content: Content) -> some View {
        content.sheet(item: next) { layer in ModalLayer(layer: layer, model: model) }
    }
}

/// Layers painted INSIDE the surface at the core's frames: tooltips always;
/// every kind in `.painted` mode (a scrim for Dialog / Drawer, outside tap
/// dismissal for the rest).
struct PaintedLayers: View {
    let model: SurfaceModel

    var body: some View {
        ForEach(model.layers) { layer in
            if model.paintsInSurface(layer) {
                if layer.isModal {
                    Rectangle()
                        .fill(model.part("Dialog", "overlay", props: [:]).style.background ?? Color.black.opacity(0.5))
                        .frame(width: model.surfaceSize.width, height: max(model.surfaceSize.height, model.viewportHeight))
                        .onTapGesture {
                            if model.index(of: layer.owner).flatMap({ model.node($0) })?.props["dismissible"]?.bool ?? true { model.dismissLayer(layer.owner) }
                        }
                        .accessibilityHidden(true)
                } else if layer.kind != "Tooltip" {
                    Color.clear
                        .contentShape(Rectangle())
                        .frame(width: model.surfaceSize.width, height: max(model.surfaceSize.height, model.viewportHeight))
                        .onTapGesture { model.dismissLayer(layer.owner) }
                        .accessibilityHidden(true)
                }
                LayerView(layer: layer, model: model)
                    .offset(x: layer.frame.minX, y: layer.frame.minY)
                    .accessibilityAddTraits(layer.isModal ? .isModal : [])
            }
        }
    }
}

extension SurfaceModel {
    /// Does the DropdownMenu `owner` paint its items (recipe `native: false`)
    /// instead of opening a native `Menu`?
    func menuPainted(_ owner: String) -> Bool {
        guard let i = index(of: owner), let n = node(i) else { return false }
        return part("DropdownMenu", "content", props: n.props).native == false
    }

    /// Does a Select paint its popup (recipe `native: false`)? The native
    /// `Menu` is the default.
    func selectPainted(_ ownerProps: Props) -> Bool {
        part("Select", "content", props: ownerProps).native == false
    }

    func paintsInSurface(_ layer: LayerInfo) -> Bool {
        if options.overlays == .painted { return true }
        switch layer.kind {
        case "Tooltip": return true
        case "DropdownMenu": return !menuPainted(layer.owner) ? false : false
        default: return false
        }
    }
}

/// The DatePicker popup: a graphical native `DatePicker` in a popover
/// anchored at the field (a sheet on compact iPhones adapts itself).
struct DatePopup: ViewModifier {
    let model: SurfaceModel

    private var presented: Binding<Bool> {
        Binding(get: { model.popup != nil }, set: { if !$0 { model.popup = nil } })
    }

    func body(content: Content) -> some View {
        let fieldIndex = model.popup.flatMap { model.index(of: $0) }
        content.popover(isPresented: presented, attachmentAnchor: .rect(.rect(fieldIndex.map { model.frame($0) } ?? .zero)), arrowEdge: .top) {
            if let fieldIndex {
                DatePopupContent(fieldIndex: fieldIndex, model: model)
                    .presentationCompactAdaptation(.popover)
            }
        }
    }
}

private struct DatePopupContent: View {
    let fieldIndex: Int
    let model: SurfaceModel
    @State private var date = Date()

    var body: some View {
        let props = model.ownerProps(fieldIndex)
        let minDate = DateModel.date(fromISO: props.str("min"))
        let maxDate = DateModel.date(fromISO: props.str("max"))
        VStack(spacing: 8) {
            Group {
                if let minDate, let maxDate {
                    DatePicker("", selection: $date, in: minDate...maxDate, displayedComponents: .date)
                } else if let minDate {
                    DatePicker("", selection: $date, in: minDate..., displayedComponents: .date)
                } else if let maxDate {
                    DatePicker("", selection: $date, in: ...maxDate, displayedComponents: .date)
                } else {
                    DatePicker("", selection: $date, displayedComponents: .date)
                }
            }
            .datePickerStyle(.graphical)
            .labelsHidden()
            .environment(\.timeZone, TimeZone(identifier: "UTC")!)
            Button("Done") { model.pickDate(fieldIndex, iso: DateModel.iso(from: date)) }
                .buttonStyle(.borderedProminent)
        }
        .padding(12)
        .frame(width: 320)
        .onAppear { date = DateModel.date(fromISO: model.dateValue(fieldIndex)) ?? Date() }
    }
}

/// On a layer trigger: Tooltip hover (macOS) / long press (touch), the
/// native DropdownMenu, the painted-menu tap.
struct TriggerModifier: ViewModifier {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel

    func body(content: Content) -> some View {
        if let target = node.triggerFor, let owner = model.index(of: target).flatMap({ model.node($0) }) {
            switch owner.component {
            case "Tooltip":
                content
                    .onLongPressGesture(minimumDuration: 0.4) { model.setOpen(target, !model.layers.contains { $0.owner == target }) }
                    #if os(macOS)
                    .onHover { model.tooltipHover(target, $0) }
                    #endif
            case "DropdownMenu" where !model.menuPainted(target) && model.options.overlays == .native:
                Menu {
                    MenuItems(owner: owner, model: model)
                } label: {
                    content
                }
                .menuStyle(.button)
                .buttonStyle(.plain)
            default:
                content
            }
        } else {
            content
        }
    }
}

/// The items of a native DropdownMenu, from the owner's `items` prop.
private struct MenuItems: View {
    let owner: NodeInfo
    let model: SurfaceModel

    var body: some View {
        if !owner.props.str("label").isEmpty {
            Text(owner.props.str("label"))
        }
        ForEach(Array(owner.props.list("items").enumerated()), id: \.offset) { _, item in
            if item["separator"]?.bool == true || item["type"]?.string == "separator" {
                Divider()
            } else {
                let value = item["value"] ?? .string(item["label"]?.displayText ?? "")
                Button(role: item["destructive"]?.bool == true ? .destructive : nil) {
                    model.fire(owner.index, "select", payload: .object(["value": value]))
                } label: {
                    if let icon = item["icon"]?.string, let view = model.host.icon(icon, size: 16) {
                        Label { Text(item["label"]?.displayText ?? "") } icon: { view }
                    } else {
                        Text(item["label"]?.displayText ?? "")
                    }
                }
                .disabled(item["disabled"]?.bool == true)
            }
        }
    }
}
