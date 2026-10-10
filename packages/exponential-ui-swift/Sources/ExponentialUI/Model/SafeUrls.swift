import Foundation

// VAPP-103: every href and every src a painter emits passes the host
// contract's policy (`catalog/host.json` urls / media): Link, markdown links
// and the openUrl function through the URL policy; Image, Avatar, Video
// poster, Audio and markdown images through the media request. A denied
// href paints as text; a denied src loads nothing (the fallback paints).
// A painter that cannot paint reports `onPaintError` (`paint`).
extension SurfaceModel {
    /// The absolute href a link may navigate to, or nil (paint it as text).
    public func linkHref(_ href: String) -> String? {
        host.effectiveHref(href)
    }

    /// Open `url` when the URL policy allows it (the host gets the absolute
    /// href). True when it was handed over.
    @discardableResult
    public func openLink(_ url: String) -> Bool {
        guard let href = linkHref(url) else { return false }
        host.openUrl(href)
        return true
    }

    /// The request a src loads with: the host's `mediaRequest` (resolveUrl
    /// rewrite + the media policy), re-checked against the media schemes /
    /// hosts so a host hook cannot widen them. nil = nothing loads.
    public func mediaRequest(_ src: String) -> URLRequest? {
        guard !src.isEmpty, let request = host.mediaRequest(src), mediaAllowed(request, options: host.mediaOptions) else { return nil }
        return request
    }

    /// The media policy a redirect of a src's request passes (the host's).
    public var mediaPolicy: MediaOptions? { host.mediaOptions }

    /// A painter failed for `componentId` painting `props` (their JSON):
    /// the node paints an empty box and is NOT painted again until its props
    /// change (`paintFailed`), and the host hears it ONCE per component
    /// (whatever the message), after the current view update.
    public func paintError(componentId: String, message: String, props: String = "") {
        if paintFailures[componentId]?.props == props { return }
        paintFailures[componentId] = PaintFailure(message: message, props: props)
        let error = SurfacePaintError(surfaceId: id, componentId: componentId, message: message)
        Task { @MainActor [host] in host.onPaintError(error) }
    }

    /// Whether `componentId` failed with these very `props` (skip its
    /// painter); props that changed clear ITS failure only, so it paints
    /// (and may report) again.
    public func paintFailed(componentId: String, props: String = "") -> Bool {
        guard let failure = paintFailures[componentId] else { return false }
        if failure.props == props { return true }
        paintFailures[componentId] = nil
        return false
    }

    /// Drop the failures of components that left the surface.
    func prunePaintFailures() {
        guard !paintFailures.isEmpty else { return }
        paintFailures = paintFailures.filter { index(of: $0.key) != nil }
    }
}

/// A component whose painter failed: the message and the props it failed on.
struct PaintFailure: Equatable {
    let message: String
    let props: String
}
