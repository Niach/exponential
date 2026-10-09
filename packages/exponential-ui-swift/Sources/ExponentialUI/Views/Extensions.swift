import SwiftUI
import ExponentialUICore

/// A leaf an extension painter measures.
public struct ExtensionLeaf {
    public let request: LeafRequest
    public let theme: ThemeHandle?
    public let mode: Mode
    public var props: Props { request.props }
    public var textStyle: TextStyle { request.textStyle }
}

/// What an extension painter receives to paint one node: the node, its
/// resolved props, the box visual, the frame size, the theme and an
/// emitter for the node's `on` handlers.
@MainActor
public struct ExtensionContext {
    public let node: NodeInfo
    public let props: Props
    public let style: PaintStyle
    public let textStyle: TextStyle
    public let ink: Color
    public let size: CGSize
    public let theme: ThemeHandle?
    public let mode: Mode
    public let model: SurfaceModel
    /// The node's children, already rendered (container extensions).
    public let children: AnyView?

    /// Fire one of the node's `on` handlers.
    public func emit(_ event: String, payload: JSONValue? = nil) {
        model.fire(node.index, event, payload: payload)
    }
}

/// A painter for one extension native (`extension_kind`).
@MainActor
public protocol ExtensionPainter: AnyObject {
    /// The border box of the leaf at a wrap width (nil wrap = max-content,
    /// 0 = min-content). nil = 0×0.
    func measure(_ leaf: ExtensionLeaf, wrap: CGFloat?) -> CGSize?
    /// The view drawn INSIDE the frame.
    func paint(_ context: ExtensionContext) -> AnyView
}

/// The extension registry: catalog definitions (registered on every new
/// surface) and painters per kind.
@MainActor
public final class ExtensionRegistry {
    public static let shared = ExtensionRegistry()
    public private(set) var definitions: [String] = []
    private var painters: [String: ExtensionPainter] = [:]

    public init() {}

    public func register(definition json: String) {
        definitions.append(json)
    }

    public func register(kind: String, painter: ExtensionPainter) {
        painters[kind] = painter
    }

    public func painter(for kind: String) -> ExtensionPainter? { painters[kind] }

    public func reset() {
        definitions.removeAll()
        painters.removeAll()
    }
}

/// The SDK's entry points.
public enum ExponentialUI {
    /// Register an extension catalog (its JSON definition) and the painters
    /// of its natives. Every `SurfaceModel` created afterwards knows it;
    /// `SurfaceModel.register` adds one to a live surface.
    @MainActor
    public static func register(extension json: String, painters: [String: ExtensionPainter]) throws {
        _ = try extensionErrors(extensionJson: json)
        ExtensionRegistry.shared.register(definition: json)
        for (kind, painter) in painters { ExtensionRegistry.shared.register(kind: kind, painter: painter) }
    }

    /// Register a font file a theme names (`.ttf`/`.otf`) with the process.
    @discardableResult
    public static func registerFont(at url: URL) -> Bool {
        ExponentialUIFonts.register(url: url)
    }

    /// The built-in theme ids.
    public static var builtinThemes: [String] { builtinThemeIds() }
    public static var defaultTheme: String { defaultThemeId() }
    public static var coreCatalog: String { coreCatalogId() }
    public static var basicCatalog: String { basicCatalogId() }
    /// The core's version.
    public static var coreVersion: String { ExponentialUICore.version() }
}
