import SwiftUI
import ExponentialUIPrimitives

/// A layer's tree: the layer root at (0, 0) with its subtree nested.
struct LayerView: View {
    let layer: LayerInfo
    let model: SurfaceModel

    var body: some View {
        NodeView(index: layer.root)
            .layoutValue(key: NodeIndexKey.self, value: layer.root)
            .frame(width: layer.frame.width, height: layer.frame.height, alignment: .topLeading)
    }
}

/// Overlays are CORE LAYERS (contract §5): every layer the core reports is
/// painted inside the surface at its frames, so a Dialog, a Select popup
/// or a toast sits exactly where gpui and React put it, above the base tree
/// at the core's frames: base < overlay < toast, tree order inside a class; a
/// modal layer gets a scrim (the recipe's `overlay` part) that blocks the
/// tree beneath and dismisses on press only when the layer is
/// `dismissible`; a press outside the non-modal overlays (menus, popups,
/// popovers; never a tooltip or a toast) dismisses them, a press inside
/// any layer does not (a submenu keeps its parent open). Drawers and
/// sheets drag toward their edge to dismiss. A layer enters with a short
/// fade + rise on the theme's `fast` motion (none under reduced motion).
struct PaintedLayers: View {
    let model: SurfaceModel

    var body: some View {
        let ordered = PaintedLayers.stacked(model.layers)
        let lastModal = ordered.lastIndex(where: \.modal) ?? -1
        let outside = ordered.indices.filter { $0 > lastModal && PaintedLayers.dismissesOnOutsidePress(ordered[$0]) }
        let enter = model.reducedMotion ? nil : Animation.timingCurve(0, 0, 0.2, 1, duration: (model.theme?.motion("fast") ?? 120) / 1000)
        let cover = CGSize(width: max(model.surfaceSize.width, model.width), height: max(model.surfaceSize.height, model.viewportHeight))
        ZStack(alignment: .topLeading) {
            ForEach(Array(ordered.enumerated()), id: \.element.id) { i, layer in
                if layer.modal {
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
                        .onTapGesture { for l in owners.reversed() { model.dismissLayer(l.owner) } }
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
        layers.filter { !$0.isToast } + layers.filter(\.isToast)
    }

    /// Does a press outside layer `l` dismiss it (gpui `outside_press`)?
    static func dismissesOnOutsidePress(_ l: LayerInfo) -> Bool {
        !l.modal && l.layerClass == "overlay" && l.kind != "Tooltip"
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
            .onTapGesture { model.dismissLayer(layer.owner) }
            .accessibilityHidden(true)
    }
}

/// One layer at its frame: the modal container (VoiceOver stays inside,
/// `.isModal`), the hover that holds a hover-opened card open, the pointer
/// over a toast (its timer pauses), the drawer drag. Escape is the
/// keyboard's (`SurfaceModel.escape`).
private struct PaintedLayer: View {
    let layer: LayerInfo
    let model: SurfaceModel
    @State private var drag: CGSize = .zero

    var body: some View {
        let edge = PaintedLayer.sheetEdge(layer)
        LayerView(layer: layer, model: model)
            .offset(x: layer.frame.minX + drag.width, y: layer.frame.minY + drag.height)
            .modifier(LayerAccessibility(modal: layer.modal))
            .modifier(LayerHover(layer: layer, model: model))
            .gesture(sheetDrag(edge ?? ""), including: edge != nil && layer.dismissible ? .all : .subviews)
    }

    /// The viewport edge a Drawer hangs from (nil: not a sheet).
    static func sheetEdge(_ l: LayerInfo) -> String? {
        guard l.kind == "Drawer" else { return nil }
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
                    model.dismissLayer(layer.owner)
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

/// The pointer over a layer: a hover-opened card (Tooltip, hover
/// Popover) stays open while hovered (gpui `hover_card`); a toast pauses.
private struct LayerHover: ViewModifier {
    let layer: LayerInfo
    let model: SurfaceModel

    func body(content: Content) -> some View {
        if layer.isToast {
            content.onHover { model.toastHover(id: layer.owner, $0) }
        } else if model.opensOnHover(layer.owner) {
            content.onHover { model.layerHover(owner: layer.owner, $0) }
        } else {
            content
        }
    }
}

/// A Tooltip trigger on touch: a long press toggles the tooltip (the
/// pointer path is the trigger's hover, `SurfaceModel.setHover`).
struct TooltipLongPress: ViewModifier {
    let node: NodeInfo
    let model: SurfaceModel

    func body(content: Content) -> some View {
        if let target = node.triggerFor, model.node(id: target)?.component == "Tooltip" {
            content.onLongPressGesture(minimumDuration: 0.4) { model.setOpen(target, !model.layers.contains { $0.owner == target }) }
        } else {
            content
        }
    }
}
