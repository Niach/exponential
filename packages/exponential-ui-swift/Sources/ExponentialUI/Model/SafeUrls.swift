import Foundation
import ExponentialUIPrimitives

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

    /// #EXP-1674: a `hostLinks` Text's segments when the host links part of
    /// it, else nil (the plain text): they must concatenate back to `text`; an
    /// href the URL policy denies stays text.
    public func textLinkSegments(_ text: String) -> [TextLinkSegment]? {
        guard let segments = host.textLinks(text), segments.map(\.text).joined() == text else { return nil }
        // #VAPP-258: a chip rides only on an allowed href.
        let policed = segments.map { s -> TextLinkSegment in
            let href = s.href.flatMap(linkHref)
            return TextLinkSegment(text: s.text, href: href, chip: href == nil ? nil : s.chip)
        }
        return policed.contains { $0.href != nil } ? policed : nil
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
    /// VAPP-271: an `app-file:<fileId>` src is resolved BEFORE the media
    /// policy: an `app-file:` request the loader answers from the host's
    /// `appFile`, nil (the fallback) when the host has no app files.
    public func mediaRequest(_ src: String) -> URLRequest? {
        if let id = appFileId(src) { return host.appFile == nil ? nil : MediaLoader.appFileRequest(id) }
        guard !src.isEmpty, let request = host.mediaRequest(src), mediaAllowed(request, options: host.mediaOptions) else { return nil }
        return request
    }

    /// #EXP-1926: the host's source rule for a content picture
    /// (`HostPlugin.imageSource`); a link's href passes the URL policy
    /// (denied = its text).
    public func imageSource(_ src: String, alt: String) -> ImageSource {
        switch host.imageSource(src, alt: alt) {
        case nil, .load?: return .load
        case let .link(href, text)?: return linkHref(href) != nil ? .link(href: href, text: text) : .text(text)
        case let .text(text)?: return .text(text)
        }
    }

    /// #EXP-1926: the host image menu's target for a rendered picture.
    func imageTarget(_ src: String, alt: String) -> ImageTarget {
        ImageTarget(src: src, alt: alt) { [weak self] in
            guard let self else { throw MediaLoader.Failure.denied }
            return try await self.originalImageBytes(src)
        }
    }

    /// #EXP-1926: the ORIGINAL bytes of `src` through the media policy (an
    /// `app-file:` through the host's files): the thumbnail width `w` is
    /// dropped from the query, any other parameter stays. Throws when denied
    /// or failed.
    public func originalImageBytes(_ src: String) async throws -> Data {
        guard let request = mediaRequest(originalImageSrc(src)) else { throw MediaLoader.Failure.denied }
        let limits = MediaLoader.shared.limits
        if let id = MediaLoader.appFileOf(request) {
            guard let files = appFile else { throw MediaLoader.Failure.denied }
            guard let data = await files(id) else { throw MediaLoader.Failure.http(404) }
            return data
        }
        return try await MediaLoader.fetchData(request, session: MediaLoader.shared.session, limits: limits, options: mediaPolicy).get().0
    }

    /// #EXP-1926: a long press began on a rendered picture under the host's
    /// image menu (an enclosing context Menu stands down).
    func claimImagePress() { imagePressClaimedAt = Date() }

    /// #EXP-1926 (`pressMenu`): did a picture take the current long press?
    var imagePressClaimed: Bool {
        imagePressClaimedAt.map { Date().timeIntervalSince($0) < 1 } ?? false
    }

    /// VAPP-271: the host's app files (`HostPlugin.appFile`), for the loader.
    public var appFile: AppFileLoader? { host.appFile }

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

/// #EXP-1926: `src` without the thumbnail width `w` in its query
/// (`copyFormat.downscale: false`); a `data:` src as it is.
public func originalImageSrc(_ src: String) -> String {
    if src.lowercased().hasPrefix("data:") { return src }
    guard let q = src.firstIndex(of: "?") else { return src }
    let hash = src[q...].firstIndex(of: "#") ?? src.endIndex
    let kept = src[src.index(after: q)..<hash].split(separator: "&").filter { $0 != "w" && !$0.hasPrefix("w=") }
    return String(src[..<q]) + (kept.isEmpty ? "" : "?" + kept.joined(separator: "&")) + String(src[hash...])
}
