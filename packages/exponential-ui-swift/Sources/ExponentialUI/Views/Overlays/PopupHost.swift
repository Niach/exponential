import SwiftUI
import ExponentialUIPrimitives

/// VAPP-281 (round-7 contract): one anchored Popover / Menu layer the HOST
/// shows outside the surface (a macOS panel window, a compositor popup):
/// `anchor` = the anchor's frame in SURFACE coordinates, `side` + `align`
/// the placement, `size` the popup's unclipped size; `content` paints the
/// layer at (0, 0) wired to the model (presses, keys and dismissals route
/// back to the surface); `dismiss` closes it (the host's window system
/// dismissed it).
public struct HostedPopup: Identifiable {
    public let layer: LayerInfo
    public let content: AnyView
    public let dismiss: @MainActor () -> Void

    public var id: String { layer.owner }
    public var owner: String { layer.owner }
    public var kind: String { layer.kind }
    public var anchor: CGRect { layer.anchorFrame ?? layer.frame }
    public var side: String { layer.placementSide ?? layer.position }
    public var align: String { layer.align ?? "center" }
    public var size: CGSize { layer.frame.size }
}

/// VAPP-281: hears the surface's hosted popups whenever they change (empty
/// = all closed). Set it with `exponentialPopupHost(_:)` around
/// `ExponentialSurface`, with `SurfaceSettings.hostedPopups` on.
public struct PopupHost: Sendable {
    public let show: @MainActor @Sendable ([HostedPopup]) -> Void

    public init(_ show: @escaping @MainActor @Sendable ([HostedPopup]) -> Void) {
        self.show = show
    }
}

private struct PopupHostKey: EnvironmentKey {
    static let defaultValue: PopupHost? = nil
}

extension EnvironmentValues {
    /// VAPP-281: the host of `LayerInfo.hosted` layers (nil = painted in
    /// the surface).
    public var exponentialPopupHost: PopupHost? {
        get { self[PopupHostKey.self] }
        set { self[PopupHostKey.self] = newValue }
    }
}

extension View {
    /// VAPP-281: hosts the surface's anchored popups outside it.
    public func exponentialPopupHost(_ host: PopupHost?) -> some View {
        environment(\.exponentialPopupHost, host)
    }
}

extension HostedPopup {
    /// The hosted layers of `model` as popups (their content carries the
    /// model and the surface's environment, so it paints in any window).
    @MainActor
    static func of(_ model: SurfaceModel) -> [HostedPopup] {
        model.layers.filter(\.hosted).map { layer in
            HostedPopup(
                layer: layer,
                content: AnyView(
                    LayerView(layer: layer, model: model)
                        .environment(model)
                        .environment(\.layoutDirection, .leftToRight)
                        .environment(\.xuiRTL, model.isRTL)
                        .primitiveTokens(model.primitiveTokens)
                ),
                dismiss: { model.dismissLayer(layer.owner) }
            )
        }
    }
}

/// Tells the popup host about the hosted layers when they change.
struct HostedPopupsReporter: ViewModifier {
    let model: SurfaceModel
    @Environment(\.exponentialPopupHost) private var popupHost

    func body(content: Content) -> some View {
        content.onChange(of: model.layers.filter(\.hosted), initial: true) { _, _ in
            popupHost?.show(HostedPopup.of(model))
        }
    }
}
