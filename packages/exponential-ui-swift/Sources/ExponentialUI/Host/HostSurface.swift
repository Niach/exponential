import SwiftUI

/// A surface the host's transport feeds (VAPP-91): the host's
/// `SurfaceModel` for `surfaceId` painted by `ExponentialSurface`, or
/// `fallback` while the host has none. As wide as its container, as tall as
/// its content: wrap it in YOUR scroller.
public struct HostSurface<Fallback: View>: View {
    let host: ExponentialHost
    let surfaceId: String
    let fallback: Fallback

    public init(host: ExponentialHost, surfaceId: String, @ViewBuilder fallback: () -> Fallback) {
        self.host = host
        self.surfaceId = surfaceId
        self.fallback = fallback()
    }

    public var body: some View {
        if let model = host.surfaces[surfaceId] {
            ExponentialSurface(model: model)
                .id(ObjectIdentifier(model))
        } else {
            fallback
        }
    }
}

public extension HostSurface where Fallback == EmptyView {
    init(host: ExponentialHost, surfaceId: String) {
        self.init(host: host, surfaceId: surfaceId) { EmptyView() }
    }
}
