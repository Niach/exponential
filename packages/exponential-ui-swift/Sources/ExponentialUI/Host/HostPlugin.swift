import SwiftUI
import ExponentialUICore

/// A user action: a node's `on.<event>` with an `event` action, resolved.
public struct SurfaceActionEvent: Sendable, Equatable {
    public let surfaceId: String
    /// The catalog event (`press`, `change`, `select`, `submit`…).
    public let event: String
    /// The action's `event.name`.
    public let name: String
    public let componentId: String
    public let context: JSONValue
    /// What the component adds (a Select's `value`, a Tabs' `value`…).
    public let payload: JSONValue?
}

/// A host-owned input edit. `change` fires debounced (150 ms) while typing
/// with a monotonically increasing `revision`; `commit` on blur / Enter.
public struct SurfaceInputEvent: Sendable, Equatable {
    public enum Kind: String, Sendable { case change, commit }
    public let surfaceId: String
    public let componentId: String
    /// The input's `name` prop.
    public let name: String
    /// The bound data model pointer, when `value` is a binding.
    public let path: String?
    public let value: JSONValue
    public let revision: Int
    public let kind: Kind
}

/// An `on.<event> = {functionCall: {call, args}}` to a host function (a
/// name outside the catalog's built-ins; `openUrl` goes to `openUrl`).
public struct SurfaceFunctionCall: Sendable, Equatable {
    public let surfaceId: String
    public let componentId: String
    /// The function name (`call`).
    public let name: String
    /// The resolved `args` (bindings read from the data model).
    public let args: Props

    public init(surfaceId: String, componentId: String, name: String, args: Props) {
        self.surfaceId = surfaceId
        self.componentId = componentId
        self.name = name
        self.args = args
    }
}

/// One file picked for (or dropped on) a FileUpload: its name, size and
/// MIME type (what the surface's `upload` event carries) plus the bytes and
/// the URL the host uploads from.
public struct SurfaceUploadFile: Sendable, Equatable {
    public let name: String
    public let size: Int
    /// The MIME type (`image/png`; `application/octet-stream` when unknown).
    public let type: String
    public let url: URL?
    public let data: Data?

    public init(name: String, size: Int, type: String, url: URL? = nil, data: Data? = nil) {
        self.name = name
        self.size = size
        self.type = type
        self.url = url
        self.data = data
    }
}

/// Files for a FileUpload (gpui `UploadEvent`): the host uploads them; the
/// surface already got `upload {files: [{name, size, type}]}`.
public struct SurfaceUploadEvent: Sendable, Equatable {
    public let surfaceId: String
    /// The FileUpload's id.
    public let componentId: String
    /// Its `name` prop (the form field).
    public let name: String
    public let files: [SurfaceUploadFile]
}

/// A component whose painter failed (`catalog/host.json` `paint`): it
/// painted an empty box; the host forwards this as an A2UI `RENDER_FAILED`
/// error. Reported once per surface + component + message.
public struct SurfacePaintError: Sendable, Equatable, Hashable {
    public let surfaceId: String
    public let componentId: String
    public let message: String

    public init(surfaceId: String, componentId: String, message: String) {
        self.surfaceId = surfaceId
        self.componentId = componentId
        self.message = message
    }
}

/// What an EMBEDDING APP provides. The SDK knows no transport: actions and
/// input edits are plain values the host forwards wherever it likes. Every
/// requirement has a default (the `NoHost` behaviour), so a host implements
/// what it needs. Called on the main actor.
@MainActor
public protocol HostPlugin: AnyObject {
    /// The icon registry: a catalog icon NAME (a registry concept such as
    /// `nav-inbox`) → a view drawn at `size` in the current foreground
    /// colour. nil = the placeholder circle.
    func icon(_ name: String, size: CGFloat) -> AnyView?
    /// Every A2UI action.
    func onAction(_ event: SurfaceActionEvent)
    /// Host-owned text edits (debounced `change`, `commit` on blur / Enter).
    func onInput(_ event: SurfaceInputEvent)
    /// `openUrl`, `Link` and markdown links: only hrefs `urlPolicy`
    /// allowed reach it, absolute. Default: the system opener.
    func openUrl(_ url: String)
    /// The URL policy every href passes (Link, markdown links, openUrl).
    /// nil = the contract defaults (https, http, mailto, tel; relative urls
    /// resolve against `mediaOptions.baseUrl`, else are denied).
    var urlPolicy: UrlPolicy? { get }
    /// The media policy every src passes (Image, Avatar, Video + poster,
    /// Audio, markdown images). nil = the contract defaults.
    var mediaOptions: MediaOptions? { get }
    /// Rewrites a media src BEFORE the media policy (relative attachment
    /// paths, signed URLs); its result never bypasses the policy.
    func resolveUrl(_ src: String) -> String
    /// A host function call (VAPP-91). `ExponentialHost` gates and runs it;
    /// a bare plugin ignores it.
    func onFunctionCall(_ call: SurfaceFunctionCall)
    /// The request an image / avatar / video poster loads with (auth
    /// headers). Default: `resolveUrl(src)` through the media policy
    /// (`mediaOptions`). The surface re-checks the result's url against the
    /// media schemes / hosts.
    func mediaRequest(_ src: String) -> URLRequest?
    /// A component's painter failed (it painted an empty box). Called once
    /// per surface + component + message.
    func onPaintError(_ error: SurfacePaintError)
    /// Called once per structure version for each `Unknown` placeholder.
    func onUnknown(component: String, catalogId: String?, id: String)
    /// A font family NAME a theme asks for → the platform font family to
    /// use (nil = the family as named, falling back to the system font).
    func fontFamily(_ name: String) -> String?
    /// A richer markdown renderer than the built-in one (nil = built-in).
    func markdown(_ text: String, width: CGFloat) -> AnyView?
    /// Files picked for / dropped on a FileUpload (read, with their bytes).
    func onUpload(_ event: SurfaceUploadEvent)
    /// A FileUpload asked for files. `true` = the host presents its own
    /// picker (and later calls `SurfaceModel.filesPicked`); default `false`
    /// = the surface presents `.fileImporter`.
    func pickFiles(_ request: FilePickRequest) -> Bool
    /// Something was announced (the model already posted it to VoiceOver):
    /// a Form's `invalidFields`, CodeBlock's `copied`, an `announce` command.
    func announce(text: String, live: String)
    /// Text for the clipboard (a CodeBlock copy). `true` = the host wrote
    /// it; default `false` = the general pasteboard.
    func copy(_ text: String) -> Bool
}

public extension HostPlugin {
    func icon(_ name: String, size: CGFloat) -> AnyView? { nil }
    func onAction(_ event: SurfaceActionEvent) {}
    func onInput(_ event: SurfaceInputEvent) {}
    func openUrl(_ url: String) {
        guard let href = effectiveHref(url), let u = URL(string: href) else { return }
        systemOpen(u)
    }
    var urlPolicy: UrlPolicy? { nil }
    var mediaOptions: MediaOptions? { nil }
    func resolveUrl(_ src: String) -> String { src }
    func onFunctionCall(_ call: SurfaceFunctionCall) {}
    func mediaRequest(_ src: String) -> URLRequest? {
        policyMediaRequest(resolveUrl(src), options: mediaOptions)
    }
    func onPaintError(_ error: SurfacePaintError) {}

    /// The URL policy with its `baseUrl` defaulting to the media one.
    var effectiveUrlPolicy: UrlPolicy {
        var policy = urlPolicy ?? UrlPolicy()
        if policy.baseUrl == nil { policy.baseUrl = mediaOptions?.baseUrl }
        return policy
    }

    /// `url` as the absolute href the URL policy allows, or nil.
    func effectiveHref(_ url: String) -> String? {
        safeHref(url, policy: effectiveUrlPolicy)
    }
    func onUnknown(component: String, catalogId: String?, id: String) {}
    func fontFamily(_ name: String) -> String? { nil }
    func markdown(_ text: String, width: CGFloat) -> AnyView? { nil }
    func onUpload(_ event: SurfaceUploadEvent) {}
    func pickFiles(_ request: FilePickRequest) -> Bool { false }
    func announce(text: String, live: String) {}
    func copy(_ text: String) -> Bool { false }
}

/// The platform opener (an href that already passed the URL policy).
@MainActor
func systemOpen(_ u: URL) {
    #if canImport(UIKit)
    UIApplication.shared.open(u)
    #elseif canImport(AppKit)
    NSWorkspace.shared.open(u)
    #endif
}

/// The host that does nothing (previews, tests).
@MainActor
public final class NoHost: HostPlugin {
    public init() {}
}

/// A host built from closures (the kitchen sink, quick embeddings).
@MainActor
public final class ClosureHost: HostPlugin {
    public var icons: (String, CGFloat) -> AnyView?
    public var actions: (SurfaceActionEvent) -> Void
    public var inputs: (SurfaceInputEvent) -> Void
    public var urls: ((String) -> Void)?
    public var unknowns: (String, String?, String) -> Void
    /// Picked / dropped files (with their bytes).
    public var uploads: (SurfaceUploadEvent) -> Void
    public var announcements: (String, String) -> Void
    public var paintErrors: (SurfacePaintError) -> Void
    public var urlPolicy: UrlPolicy?
    public var mediaOptions: MediaOptions?

    public init(
        icons: @escaping (String, CGFloat) -> AnyView? = { _, _ in nil },
        actions: @escaping (SurfaceActionEvent) -> Void = { _ in },
        inputs: @escaping (SurfaceInputEvent) -> Void = { _ in },
        urls: ((String) -> Void)? = nil,
        unknowns: @escaping (String, String?, String) -> Void = { _, _, _ in },
        uploads: @escaping (SurfaceUploadEvent) -> Void = { _ in },
        announcements: @escaping (String, String) -> Void = { _, _ in },
        paintErrors: @escaping (SurfacePaintError) -> Void = { _ in },
        urlPolicy: UrlPolicy? = nil,
        mediaOptions: MediaOptions? = nil
    ) {
        self.paintErrors = paintErrors
        self.urlPolicy = urlPolicy
        self.mediaOptions = mediaOptions
        self.icons = icons
        self.actions = actions
        self.inputs = inputs
        self.urls = urls
        self.unknowns = unknowns
        self.uploads = uploads
        self.announcements = announcements
    }

    public func onUpload(_ event: SurfaceUploadEvent) { uploads(event) }
    public func announce(text: String, live: String) { announcements(text, live) }

    public func icon(_ name: String, size: CGFloat) -> AnyView? { icons(name, size) }
    public func onAction(_ event: SurfaceActionEvent) { actions(event) }
    public func onInput(_ event: SurfaceInputEvent) { inputs(event) }
    public func openUrl(_ url: String) {
        guard let href = effectiveHref(url) else { return }
        if let urls { urls(href) } else if let u = URL(string: href) { systemOpen(u) }
    }
    public func onPaintError(_ error: SurfacePaintError) { paintErrors(error) }
    public func onUnknown(component: String, catalogId: String?, id: String) { unknowns(component, catalogId, id) }
}
