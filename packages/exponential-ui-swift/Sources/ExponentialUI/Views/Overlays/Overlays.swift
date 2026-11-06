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
/// sheets drag toward their edge to dismiss. An overlay enters scaling from
/// the theme's `scale.popoverFrom` (anchored at its trigger's side) while
/// it fades in on `motion.standard`, and leaves with a faster fade; a toast
/// fades and rises in (none of it under reduced motion).
///
/// #VAPP-252: under `SurfaceOptions.liftsLayers` the first modal and what opens
/// above it are not the surface's: they paint on the `stage` of
/// `ExponentialSurfaceLayers`.
struct PaintedLayers: View {
    let model: SurfaceModel
    var stage: LayerStage? = nil

    /// The layers this painter owns, in paint order.
    private var shown: [LayerInfo] {
        let all = PaintedLayers.stacked(model.layers)
        guard model.options.liftsLayers, let first = all.firstIndex(where: \.modal) else { return stage == nil ? all : [] }
        return stage == nil ? Array(all[..<first]) : Array(all[first...])
    }

    var body: some View {
        let ordered = shown
        let lastModal = ordered.lastIndex(where: \.modal) ?? -1
        let outside = ordered.indices.filter { $0 > lastModal && PaintedLayers.dismissesOnOutsidePress(ordered[$0]) }
        // EXP-1259 (the EXP-1243 rules): enters `decelerate` at `standard`,
        // scaling from `scale.popoverFrom` toward the trigger's side while
        // they fade in; exits are opacity only, faster (`accelerate` at
        // `fast`); reduced motion = none.
        let enter = model.reducedMotion ? nil : Animation.timingCurve(0, 0, 0.2, 1, duration: (model.theme?.motion("standard") ?? 180) / 1000)
        let exit = model.reducedMotion ? nil : Animation.timingCurve(0.3, 0, 1, 1, duration: (model.theme?.motion("fast") ?? 120) / 1000)
        let from = CGFloat(model.theme?.scale("popoverFrom") ?? 0.96)
        let cover = stage?.cover ?? CGSize(width: max(model.surfaceSize.width, model.width), height: max(model.surfaceSize.height, model.viewportHeight))
        let coverAt = stage?.coverOrigin ?? .zero
        ZStack(alignment: .topLeading) {
            ForEach(Array(ordered.enumerated()), id: \.element.id) { i, layer in
                if layer.modal {
                    Scrim(layer: layer, model: model, size: cover)
                        .offset(x: coverAt.x, y: coverAt.y)
                        .transition(enter.map { AnyTransition.opacity.animation($0) } ?? .identity)
                }
                if i == outside.first {
                    // One catcher under every non-modal overlay above the
                    // last modal: a press that reaches it is outside them all.
                    let owners = outside.map { ordered[$0] }
                    Color.clear
                        .contentShape(Rectangle())
                        .frame(width: cover.width, height: cover.height)
                        .offset(x: coverAt.x, y: coverAt.y)
                        .onTapGesture { for l in owners.reversed() { model.dismissLayer(l.owner) } }
                        .accessibilityHidden(true)
                }
                PaintedLayer(layer: layer, model: model, shift: stage?.shift(of: layer, in: ordered, surfaceWidth: model.width, holds: model.dialogHolds) ?? .zero)
                    .transition(enter.map { .asymmetric(insertion: (layer.layerClass == "toast" ? AnyTransition.opacity.combined(with: .offset(y: 8)) : AnyTransition.opacity.combined(with: .scale(scale: from, anchor: layer.position == "top" ? .bottom : .top))).animation($0), removal: exit.map { AnyTransition.opacity.animation($0) } ?? .identity) } ?? .identity)
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

/// #VAPP-252: the part of the window a lifted layer lives in: `size` = its
/// safe area above the keyboard, `insets` = what lies outside it. The scrim
/// covers all of it; the core laid the layers out at `size.height`, a top or
/// bottom sheet at `size.width` too (`overlayViewport`, #VAPP-253: it spans the
/// safe area, which is the whole width of a phone held upright), so a modal lands by the place
/// the core gave it (`position`) and a popup opened inside it moves with it.
struct LayerStage: Equatable {
    var size: CGSize
    var insets: EdgeInsets
    /// The home indicator's inset: a bottom sheet rests on the screen's edge
    /// unless the keyboard stands there.
    var homeInset: CGFloat = 0

    var cover: CGSize {
        CGSize(width: size.width + insets.leading + insets.trailing, height: size.height + insets.top + insets.bottom)
    }

    var coverOrigin: CGPoint {
        CGPoint(x: -insets.leading, y: -insets.top)
    }

    /// Where a modal of `layerSize` lands: a Dialog centred (`OVERLAY_PADDING`
    /// clear of the top when taller), a sheet on its edge; a top or bottom
    /// sheet is `size.width` wide, so centring it lands it at the stage's left.
    /// EXP-1676: `held` = the Dialog's first height: it HOLDS its bottom edge
    /// at `centre + held/2` while its content grows or shrinks (the core's rule).
    func origin(position: String, layerSize: CGSize, held: CGFloat? = nil) -> CGPoint {
        let centreX = (size.width - layerSize.width) / 2
        switch position {
        case "left": return .zero
        case "right": return CGPoint(x: size.width - layerSize.width, y: 0)
        case "top": return CGPoint(x: centreX, y: 0)
        case "bottom":
            let edge = insets.bottom <= homeInset ? insets.bottom : 0
            return CGPoint(x: centreX, y: size.height - layerSize.height + edge)
        default:
            if let held {
                let h0 = min(held, max(size.height - 16, 0))
                return CGPoint(x: centreX, y: max(size.height / 2 + h0 / 2 - layerSize.height, 8))
            }
            return CGPoint(x: centreX, y: max((size.height - layerSize.height) / 2, 8))
        }
    }

    /// How far a layer moves from its core frame: a toast's bottom row
    /// centres under the window's width; the rest follow the modal at or
    /// below them.
    func shift(of layer: LayerInfo, in ordered: [LayerInfo], surfaceWidth: CGFloat, holds: [String: CGFloat] = [:]) -> CGPoint {
        if layer.isToast { return CGPoint(x: (size.width - surfaceWidth) / 2, y: 0) }
        let below = ordered.prefix { $0.id != layer.id }.last(where: \.modal)
        guard let modal = layer.modal ? layer : below else { return .zero }
        let at = origin(position: modal.position, layerSize: modal.frame.size, held: holds[modal.owner])
        return CGPoint(x: at.x - modal.frame.minX, y: at.y - modal.frame.minY)
    }
}

/// #VAPP-252: the lifted layers of a surface with `SurfaceOptions.liftsLayers`
/// (`SurfaceModel.liftedLayersOpen`): put this where the scrim should reach,
/// over the whole window (a clear `fullScreenCover`), and the surface paints
/// them no more. It holds the keyboard routing and the file picker while up;
/// the surface's own stay out of the way.
public struct ExponentialSurfaceLayers: View {
    let model: SurfaceModel
    @Environment(\.xuiReducedMotion) private var enclosingReducedMotion
    @FocusState private var rootFocused: Bool
    @State private var appeared = false
    @State private var homeInset: CGFloat = 0

    public init(model: SurfaceModel) {
        self.model = model
    }

    public var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .topLeading) {
                // Inserted after the first frame, so the layers enter.
                if appeared {
                    PaintedLayers(model: model, stage: LayerStage(size: proxy.size, insets: proxy.safeAreaInsets, homeInset: homeInset))
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height, alignment: .topLeading)
            .onChange(of: proxy.size, initial: true) { _, size in model.overlayViewport(size) }
        }
        .background {
            Color.clear
                .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.bottom } action: { homeInset = $0 }
                .ignoresSafeArea(.keyboard)
        }
        .environment(model)
        .environment(\.layoutDirection, .leftToRight)
        .primitiveTokens(model.primitiveTokens)
        .environment(\.xuiRTL, model.isRTL)
        .environment(\.xuiReducedMotion, model.reducedMotion || enclosingReducedMotion)
        .modifier(KeyboardRouting(model: model, rootFocused: $rootFocused, onStage: true))
        .modifier(FilePicking(model: model, onStage: true))
        .onAppear { appeared = true }
        .onDisappear { model.overlayViewport(nil) }
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
    var shift: CGPoint = .zero
    @State private var drag: CGSize = .zero

    var body: some View {
        let edge = PaintedLayer.sheetEdge(layer)
        LayerView(layer: layer, model: model)
            .offset(x: layer.frame.minX + shift.x + drag.width, y: layer.frame.minY + shift.y + drag.height)
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
