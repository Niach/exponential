import SwiftUI
import ExponentialUIPrimitives

/// A layer's tree: the layer root at (0, 0) with its subtree nested, the
/// surface environment (direction, reduced motion) set on it like on the
/// main tree's root.
struct LayerView: View {
    let layer: LayerInfo
    let model: SurfaceModel

    var body: some View {
        NodeView(index: layer.root)
            .layoutValue(key: NodeIndexKey.self, value: layer.root)
            .frame(width: layer.frame.width, height: layer.frame.height, alignment: .topLeading)
            .environment(\.xuiRTL, model.paintsRTL)
            .environment(\.xuiReducedMotion, model.paintReducedMotion)
    }
}

/// Overlays are CORE LAYERS (contract §5): every layer the core reports is
/// painted inside the surface at its frames (`PaintedLayers`), whatever the
/// `OverlayPresentation`, so a Dialog, a Select popup or a toast sits
/// exactly where gpui and React put it. This modifier presents nothing; it
/// stays for source compatibility (`SurfaceView` applies it).
struct NativeOverlays: ViewModifier {
    let model: SurfaceModel

    func body(content: Content) -> some View { content }
}

/// The open layers painted above the base tree at the core's frames
/// (contract §5): base < overlay < toast, tree order inside a class; a
/// modal layer gets a scrim (the recipe's `overlay` part) that blocks the
/// tree beneath and dismisses on press only when the layer is
/// `dismissible`; a press outside the non-modal overlays (menus, popups,
/// popovers; never a tooltip or a toast) dismisses them, a press inside
/// any layer does not (a submenu keeps its parent open). Drawers and
/// sheets drag toward their edge to dismiss. A layer enters with a short
/// fade + rise on the theme's `fast` motion (none under reduced motion).
struct PaintedLayers: View {
    let model: SurfaceModel
    @Environment(\.accessibilityReduceMotion) private var platformReduced

    var body: some View {
        let ordered = PaintedLayers.stacked(model.layers)
        let lastModal = ordered.lastIndex { $0.paintModal } ?? -1
        let outside = ordered.indices.filter { $0 > lastModal && PaintedLayers.dismissesOnOutsidePress(ordered[$0]) }
        let reduced = platformReduced || model.paintReducedMotion
        let enter = reduced ? nil : Animation.timingCurve(0, 0, 0.2, 1, duration: (model.theme?.motion("fast") ?? 120) / 1000)
        let cover = CGSize(width: max(model.surfaceSize.width, model.width), height: max(model.surfaceSize.height, model.viewportHeight))
        ZStack(alignment: .topLeading) {
            ForEach(Array(ordered.enumerated()), id: \.element.id) { i, layer in
                if layer.paintModal {
                    Scrim(layer: layer, model: model, size: cover)
                        .transition(enter.map { AnyTransition.opacity.animation($0) } ?? .identity)
                }
                if i == outside.first {
                    // One catcher under every non-modal overlay above the
                    // last modal: a press that reaches it is outside them all.
                    let owners = outside.map { ordered[$0] }
                    Color.clear
                        .contentShape(Rectangle())
                        .frame(width: cover.width, height: cover.height)
                        .onTapGesture { for l in owners.reversed() { model.paintDismiss(l) } }
                        .accessibilityHidden(true)
                }
                PaintedLayer(layer: layer, model: model)
                    .transition(enter.map { .asymmetric(insertion: AnyTransition.opacity.combined(with: .offset(y: layer.position == "top" ? -8 : 8)).animation($0), removal: .identity) } ?? .identity)
            }
        }
    }

    /// Layers in paint order: the overlay class, then toasts (contract §5:
    /// no zIndex; the core's order inside a class).
    static func stacked(_ layers: [LayerInfo]) -> [LayerInfo] {
        layers.filter { $0.paintClass != "toast" } + layers.filter { $0.paintClass == "toast" }
    }

    /// Does a press outside layer `l` dismiss it (gpui `outside_press`)?
    static func dismissesOnOutsidePress(_ l: LayerInfo) -> Bool {
        !l.paintModal && l.paintClass == "overlay" && l.kind != "Tooltip"
    }
}

/// A modal layer's scrim: the recipe's `overlay` part colour (gpui:
/// `<kind>/overlay` in state `open`), covering the surface and blocking the
/// tree beneath; a press dismisses only a dismissible layer.
private struct Scrim: View {
    let layer: LayerInfo
    let model: SurfaceModel
    let size: CGSize

    var body: some View {
        let owner = model.index(of: layer.owner).flatMap { model.node($0) }
        let overlay = model.part(layer.kind, "overlay", props: owner?.props ?? [:], states: ["open"]).style.background
            ?? model.part("Dialog", "overlay", props: [:], states: ["open"]).style.background
            ?? Color.black.opacity(0.5)
        Rectangle()
            .fill(overlay)
            .frame(width: size.width, height: size.height)
            .contentShape(Rectangle())
            .onTapGesture { model.paintDismiss(layer) }
            .accessibilityHidden(true)
    }
}

/// One layer at its frame: the modal container (VoiceOver stays inside,
/// `.isModal`), Escape on macOS, the drawer drag.
private struct PaintedLayer: View {
    let layer: LayerInfo
    let model: SurfaceModel
    @State private var drag: CGSize = .zero

    var body: some View {
        let edge = PaintedLayer.sheetEdge(layer)
        LayerView(layer: layer, model: model)
            .offset(x: layer.frame.minX + drag.width, y: layer.frame.minY + drag.height)
            .modifier(LayerAccessibility(modal: layer.paintModal))
            .modifier(EscapeDismiss(layer: layer, model: model))
            .gesture(sheetDrag(edge ?? ""), including: edge != nil && layer.paintDismissible(model) ? .all : .subviews)
    }

    /// The viewport edge a Drawer / Sheet hangs from (nil: not a sheet).
    static func sheetEdge(_ l: LayerInfo) -> String? {
        guard l.kind == "Drawer" || l.kind == "Sheet" else { return nil }
        switch l.position {
        case "top", "right", "bottom", "left": return l.position
        default: return nil
        }
    }

    private func sheetDrag(_ edge: String) -> some Gesture {
        DragGesture(minimumDistance: 12)
            .onChanged { v in
                let o = sheetOffset(edge, v.translation.width, v.translation.height)
                drag = CGSize(width: o.x, height: o.y)
            }
            .onEnded { v in
                let o = sheetOffset(edge, v.translation.width, v.translation.height)
                if max(abs(o.x), abs(o.y)) >= sheetDismissDistance {
                    model.paintDismiss(layer)
                    drag = .zero
                } else {
                    withAnimation(.timingCurve(0, 0, 0.2, 1, duration: 0.2)) { drag = .zero }
                }
            }
    }
}

/// How far a Drawer is dragged toward its edge before it dismisses (gpui
/// `SHEET_DISMISS_PX`).
let sheetDismissDistance: CGFloat = 64

/// A sheet drag's offset: the pointer's travel TOWARD the sheet's edge
/// only (gpui `sheet_offset`).
func sheetOffset(_ side: String, _ x: CGFloat, _ y: CGFloat) -> CGPoint {
    switch side {
    case "bottom": CGPoint(x: 0, y: max(0, y))
    case "top": CGPoint(x: 0, y: min(0, y))
    case "left": CGPoint(x: min(0, x), y: 0)
    case "right": CGPoint(x: max(0, x), y: 0)
    default: .zero
    }
}

private struct LayerAccessibility: ViewModifier {
    let modal: Bool

    func body(content: Content) -> some View {
        if modal {
            // The focus trap for VoiceOver: a modal container hides its
            // siblings (the tree beneath) from the reader.
            content.accessibilityElement(children: .contain).accessibilityAddTraits(.isModal)
        } else {
            content
        }
    }
}

private struct EscapeDismiss: ViewModifier {
    let layer: LayerInfo
    let model: SurfaceModel

    func body(content: Content) -> some View {
        #if os(macOS)
        content.onExitCommand { model.paintDismiss(layer) }
        #else
        content
        #endif
    }
}

extension SurfaceModel {
    /// Dismiss layer `layer` through the core's path (`event(root,
    /// "dismiss")`: the core closes it only when dismissible and fires the
    /// author's handlers), remembering it so the trigger's own press does
    /// not reopen it.
    func paintDismiss(_ layer: LayerInfo) {
        guard layer.paintDismissible(self), layers.contains(where: { $0.owner == layer.owner }) else { return }
        justDismissed = layer.owner
        fire(layer.root, "dismiss")
        let owner = layer.owner
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(300))
            if self?.justDismissed == owner { self?.justDismissed = nil }
        }
    }

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

    /// Every core layer paints in the surface (contract §5).
    func paintsInSurface(_ layer: LayerInfo) -> Bool { true }
}

/// The DatePicker popup: a graphical native `DatePicker` in a popover
/// anchored at the field (a sheet on compact iPhones adapts itself). Only
/// for the legacy `popup` path; a DatePicker whose calendar the core opens
/// as a layer paints in `PaintedLayers`.
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

/// On a layer trigger: Tooltip hover (pointer) / long press (touch). Every
/// other trigger is a plain pressable: its press goes to the core, which
/// opens the layer (menus included: their items are core nodes).
struct TriggerModifier: ViewModifier {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel

    func body(content: Content) -> some View {
        if let target = node.triggerFor, let owner = model.index(of: target).flatMap({ model.node($0) }), owner.component == "Tooltip" {
            content
                .onLongPressGesture(minimumDuration: 0.4) { model.setOpen(target, !model.layers.contains { $0.owner == target }) }
                .onHover { model.tooltipHover(target, $0) }
        } else {
            content
        }
    }
}
